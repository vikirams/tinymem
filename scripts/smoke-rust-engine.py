"""Smoke-check the release CLI without modifying the supplied repository.

Only Python's standard library is needed for this optional development check.
The engine itself does not require Python.
"""

import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/tinymem"))
    parser.add_argument("--repository", type=Path, default=Path("."))
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    repository = args.repository.resolve(strict=True)

    with tempfile.TemporaryDirectory(prefix="tinymem-smoke-") as temporary:
        work = Path(temporary)
        database = work / "checkout.db"
        command = [str(binary), "--db", str(database)]
        indexed = subprocess.run(
            command + ["index", str(repository)],
            check=True, capture_output=True, text=True, timeout=120,
        )
        report = json.loads(indexed.stdout)
        require(report["indexed"] > 0, "checkout indexing returned no source")
        searched = subprocess.run(
            command + ["search", str(repository), "validate_ignores", "--limit", "1"],
            check=True, capture_output=True, text=True, timeout=120,
        )
        result = json.loads(searched.stdout)
        require(result["refresh"]["indexed"] == 0, "unchanged checkout was reindexed")
        require(bool(result["hits"]), "release CLI did not retrieve engine source")
        require(len(result["hits"][0]["content_hash"]) == 64, "missing source hash")
        print(f"Checkout indexing/search: PASS ({report['indexed']} files)")

        fixture = work / "fixture"
        fixture.mkdir()
        source = fixture / "auth.py"
        source.write_text("def authenticate(): pass\n", encoding="utf-8")
        if os.name != "posix":
            print("Watch SIGINT shutdown: SKIPPED (POSIX-only smoke check)")
            return
        watcher = subprocess.Popen(
            [str(binary), "watch", str(fixture), "--interval-ms", "100"],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        try:
            # First JSON report proves the handler is installed and the watcher is ready.
            require(watcher.stdout is not None, "watch stdout unavailable")
            initial = json.loads(watcher.stdout.readline())
            require(initial["indexed"] == 1, "watch initial refresh failed")
            watcher.send_signal(signal.SIGINT)
            _, errors = watcher.communicate(timeout=10)
            require(watcher.returncode == 0, f"watch shutdown failed: {errors}")
            require(not errors, f"watch unexpectedly wrote diagnostics: {errors}")
            print("Watch SIGINT shutdown: PASS (exit 0)")
        finally:
            if watcher.poll() is None:
                watcher.kill()
            watcher.communicate()


if __name__ == "__main__":
    main()
