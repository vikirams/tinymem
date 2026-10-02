"""Model-free, disposable native MCP latency workload (not coding-quality evidence).

Measures persistent MCP (including repository refresh and dependency validation)
and one-operation-per-process MCP, retaining raw millisecond samples. All evidence
is synthetic; never reads a personal database or calls an embedding/model provider.
"""

import argparse
import hashlib
import json
import math
import platform
import queue
import subprocess
import tempfile
import threading
import time
from pathlib import Path

MAX_FRAME = 3 * 1024 * 1024


class Session:
    def __init__(self, binary, db, repository):
        self.process = subprocess.Popen(
            [str(binary), "--db", str(db), "mcp", str(repository),
             "--user", "benchmark", "--project", "fixture"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        )
        self.frames = queue.Queue(maxsize=4)
        self.next_id = 0
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()
        try:
            self.request("initialize", {
                "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "tinymem-local-benchmark", "version": "0.1.0"},
            })
            self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        except BaseException:
            self.close()
            raise

    def _read(self):
        try:
            while True:
                frame = self.process.stdout.readline(MAX_FRAME + 1)
                if not frame:
                    self.frames.put(RuntimeError("MCP ended before its response"))
                    return
                if len(frame) > MAX_FRAME or not frame.endswith(b"\n"):
                    raise RuntimeError("MCP frame exceeds the bounded output limit")
                self.frames.put(json.loads(frame))
        except Exception as error:
            self.frames.put(error)

    def send(self, frame):
        encoded = (json.dumps(frame) + "\n").encode("utf-8")
        if len(encoded) > 1024 * 1024:
            raise ValueError("request exceeds MCP input limit")
        self.process.stdin.write(encoded)
        self.process.stdin.flush()

    def request(self, method, params):
        self.next_id += 1
        self.send({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        try:
            frame = self.frames.get(timeout=10)
        except queue.Empty as error:
            raise TimeoutError("MCP benchmark operation exceeded 10 seconds") from error
        if isinstance(frame, Exception):
            raise frame
        if frame.get("id") != self.next_id or "error" in frame:
            raise RuntimeError("unexpected or failed MCP response")
        return frame["result"]

    def tool(self, name, arguments):
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        if result.get("isError"):
            raise RuntimeError("native benchmark tool failed")
        return result["structuredContent"]

    def close(self):
        if self.process.stdin:
            self.process.stdin.close()
        try:
            self.process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:
                self.process.wait(timeout=1)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=1)
        if self.process.stdout:
            self.process.stdout.close()
        self.reader.join(timeout=1)


def summary(samples):
    ordered = sorted(samples)
    def percentile(fraction):
        return ordered[max(0, math.ceil(len(ordered) * fraction) - 1)]
    return {"samples_ms": samples, "p50_ms": percentile(.50), "p95_ms": percentile(.95), "p99_ms": percentile(.99)}


def elapsed(start):
    return (time.perf_counter_ns() - start) / 1_000_000


def verify(context, budget):
    if context["token_count_method"] != "utf8_bytes_upper_bound":
        raise AssertionError("unexpected token counter")
    if len(context["prompt"].encode("utf-8")) != context["tokens_used"] or context["tokens_used"] > budget:
        raise AssertionError("context budget violation")
    if not any("fixture_clock_utc" in entry["content"] for entry in context["entries"]):
        raise AssertionError("expected current memory not selected")


def run(binary, samples, memories, files, budget):
    with tempfile.TemporaryDirectory(prefix="tinymem-local-benchmark-") as temporary:
        work = Path(temporary)
        repository = work / "repository"
        repository.mkdir()
        (repository / ".pi").mkdir()
        (repository / ".pi/dependency.txt").write_text("UTC\n", encoding="utf-8")
        for index in range(files):
            (repository / f"fixture_{index}.py").write_text(
                f"def fixture_{index}():\n    return {index}\n", encoding="utf-8",
            )
        db = work / "native.db"
        start = time.perf_counter_ns()
        session = Session(binary, db, repository)
        initialize_ms = elapsed(start)
        try:
            for index in range(memories):
                # Deliberately pace OUTSIDE measurement to avoid the MCP 128/s limiter.
                time.sleep(.01)
                session.tool("add_memory", {
                    "content": "fixture_clock_utc: persist clock timestamps in UTC, never local time." if index == 0 else f"fixture_rule_{index}: use explicit evidence for independent convention {index}.",
                    "kind": "decision",
                    "source_refs": [{"kind": "file", "role": "dependency", "path": ".pi/dependency.txt"}] if index == 0 else [],
                })
            time.sleep(.01)
            start = time.perf_counter_ns()
            first = session.tool("assemble_context", {"query": "clock timestamps UTC", "token_budget": budget})
            first_refresh_ms = elapsed(start)
            verify(first, budget)
            persistent = []
            for _ in range(samples):
                time.sleep(.01)
                start = time.perf_counter_ns()
                context = session.tool("assemble_context", {"query": "clock timestamps UTC", "token_budget": budget})
                persistent.append(elapsed(start))
                verify(context, budget)
        finally:
            session.close()
        one_shot = []
        for _ in range(samples):
            start = time.perf_counter_ns()
            short = Session(binary, db, repository)
            try:
                context = short.tool("assemble_context", {"query": "clock timestamps UTC", "token_budget": budget})
            finally:
                short.close()
            one_shot.append(elapsed(start))
            verify(context, budget)
        return {
            "format": "tinymem-local-latency-v1",
            "binary_version": subprocess.check_output([str(binary), "--version"], text=True).strip(),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "platform": platform.platform(), "machine": platform.machine(),
            "workload": {"memories": memories, "python_files": files, "file_dependencies": 1, "token_budget": budget, "query": "clock timestamps UTC", "samples": samples},
            "notes": [
                "Synthetic sparse lexical recall; no network/provider; no task-quality comparison.",
                "Persistent timing includes MCP framing, engine assembly, full repository refresh and dependency checks.",
                "One-shot timing also includes process start, initialize, EOF, child exit and reader cleanup (SDK-shaped, not the TypeScript SDK itself).",
                "Warm OS filesystem cache, seeded DB, one reader, no concurrent writers; not cold-host latency.",
                "10ms pacing between persistent calls is excluded; throughput is not measured.",
                "p99 on small sample counts is effectively the maximum; do not infer a release guarantee.",
            ],
            "start_and_initialize_ms": initialize_ms,
            "first_repository_refresh_ms": first_refresh_ms,
            "context_units": first["tokens_used"], "context_bytes": len(first["prompt"].encode("utf-8")),
            "persistent_mcp": summary(persistent), "one_shot_mcp": summary(one_shot),
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/tinymem"))
    parser.add_argument("--samples", type=int, default=30)
    parser.add_argument("--memories", type=int, default=100)
    parser.add_argument("--files", type=int, default=25)
    parser.add_argument("--budget", type=int, default=4096)
    parser.add_argument("--output", type=Path, help="Write complete synthetic report instead of stdout (must not exist)")
    args = parser.parse_args()
    if not 1 <= args.samples <= 1000 or not 1 <= args.memories <= 10000 or not 0 <= args.files <= 10000 or not 2048 <= args.budget <= 262144:
        parser.error("samples 1..1000, memories 1..10000, files 0..10000, budget 2048..262144 required")
    if args.output and args.output.exists():
        parser.error("output exists; choose a new artifact path")
    report = run(args.binary.resolve(strict=True), args.samples, args.memories, args.files, args.budget)
    serialized = json.dumps(report, indent=2) + "\n"
    if args.output:
        with args.output.open("x", encoding="utf-8") as output:
            output.write(serialized)
        print(f"Wrote synthetic latency samples: {args.output}")
    else:
        print(serialized, end="")


if __name__ == "__main__":
    main()
