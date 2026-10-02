"""Bounded, per-operation stdio MCP adapter for the native tinymem binary."""
from __future__ import annotations

import json
import math
import os
import queue
import re
import subprocess
import threading
import time
from typing import Any, cast

from .types import FileDependencyCheck, FileSourceInput, FileSourceReference, Freshness, FreshnessReason, FreshnessStatus, SourceRole
from .types import AssembledContext, ContextEntry, ContextSource, EmbeddingIndexReport, MemoryEvent, MemoryKind, MemoryProvenance, MemoryRecord, Profile, RefreshReport, RepositoryHit, RepositoryProvenance, RepositorySearchResult, Scope, SearchMode, Symbol

__all__ = [
    "FileDependencyCheck", "FileSourceInput", "FileSourceReference", "Freshness", "FreshnessReason", "FreshnessStatus", "SourceRole",
    "Client", "NativeError", "TransportError", "ProtocolError", "OutputLimitError",
    "RPCTimeoutError", "RPCError", "ToolError", "MemoryEvent", "MemoryKind",
    "MemoryRecord", "Profile", "RefreshReport", "RepositoryHit", "RepositorySearchResult",
    "Scope", "Symbol", "SearchMode", "EmbeddingIndexReport", "AssembledContext", "ContextEntry", "ContextSource",
    "MemoryProvenance", "RepositoryProvenance", "PROTOCOL_VERSION", "DEFAULT_MAX_FRAME_BYTES",
]

PROTOCOL_VERSION = "2025-06-18"
DEFAULT_MAX_FRAME_BYTES = 3 * 1024 * 1024
_SCOPE_KEYS = {"scope", "user", "agent", "project", "user_id", "agent_id", "project_id",
               "userId", "agentId", "projectId", "repository", "db", "binary"}


class NativeError(Exception):
    """Base SDK error. Operation errors carry bounded stderr and returncode."""
    stderr: str = ""
    returncode: int | None = None


class TransportError(NativeError):
    pass


class ProtocolError(TransportError):
    pass


class OutputLimitError(TransportError):
    pass


class RPCTimeoutError(TransportError, TimeoutError):
    pass


class RPCError(NativeError):
    def __init__(self, error: dict[str, Any]):
        self.code = error["code"]
        self.data = error.get("data")
        super().__init__(error["message"])


class ToolError(NativeError):
    def __init__(self, result: dict[str, Any]):
        self.result = result
        super().__init__("\n".join(item["text"] for item in result["content"]
                                  if item.get("type") == "text") or "native tool failed")


def _token(value: str, name: str) -> str:
    if not isinstance(value, str) or re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.:@-]{0,127}", value) is None:
        raise ValueError(f"{name} must be a 1-128 character ASCII identifier token")
    return value


def _reject_constant(value: str) -> None:
    raise ValueError(f"invalid JSON constant {value}")


def _encode(message: dict[str, Any], limit: int) -> bytes:
    frame = json.dumps(message, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8")
    if len(frame) > limit:
        raise ValueError("request frame exceeds max_frame_bytes")
    return frame + b"\n"


def _result(message: dict[str, Any]) -> dict[str, Any]:
    if "error" in message:
        error = message["error"]
        if not isinstance(error, dict) or type(error.get("code")) is not int or not isinstance(error.get("message"), str):
            raise ProtocolError("malformed JSON-RPC error")
        raise RPCError(error)
    if not isinstance(message.get("result"), dict):
        raise ProtocolError("MCP result must be an object")
    return cast(dict[str, Any], message["result"])


class _Operation:
    def __init__(self, client: Client):
        self.client = client
        self.deadline = time.monotonic() + client._timeout
        self.condition = threading.Condition()
        self.failure: NativeError | None = None
        self.responses: dict[int, dict[str, Any]] = {}
        self.expected_id = 1
        self.eof = False
        self.stderr = bytearray()
        self.stop = threading.Event()
        self.outgoing: queue.Queue[bytes | None] = queue.Queue(maxsize=4)
        self.threads: list[threading.Thread] = []
        try:
            self.process = subprocess.Popen(client._args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                            stderr=subprocess.PIPE, bufsize=0, shell=False)
        except (OSError, ValueError) as error:
            raise TransportError(f"cannot start tinymem: {error}") from error

    def remaining(self) -> float:
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise RPCTimeoutError(f"MCP operation exceeded {self.client._timeout}s deadline")
        return remaining

    def fail(self, error: NativeError) -> None:
        with self.condition:
            self.failure = self.failure or error
            self.condition.notify_all()

    def read_stdout(self) -> None:
        assert self.process.stdout is not None
        buffer = bytearray()
        total = 0
        try:
            while chunk := os.read(self.process.stdout.fileno(), 65536):
                total += len(chunk)
                if total > self.client._max_output_bytes:
                    raise OutputLimitError("stdout exceeds max_output_bytes")
                for index, part in enumerate(chunk.split(b"\n")):
                    if index:
                        self.receive(bytes(buffer))
                        buffer.clear()
                    if len(buffer) + len(part) > self.client._max_frame_bytes:
                        raise OutputLimitError("stdout frame exceeds max_frame_bytes")
                    buffer.extend(part)
            if buffer:
                raise ProtocolError("stdout ended with an incomplete JSON frame")
        except (OSError, ValueError, RecursionError, NativeError) as error:
            self.fail(error if isinstance(error, NativeError) else ProtocolError(f"invalid stdout: {error}"))
        finally:
            with self.condition:
                self.eof = True
                self.condition.notify_all()

    def receive(self, frame: bytes) -> None:
        message = json.loads(frame.decode("utf-8"), parse_constant=_reject_constant)
        if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
            raise ProtocolError("expected JSON-RPC 2.0 object")
        if "id" not in message and isinstance(message.get("method"), str) and message["method"].startswith("notifications/"):
            if "result" in message or "error" in message or ("params" in message and not isinstance(message["params"], dict)):
                raise ProtocolError("malformed server notification")
            return
        with self.condition:
            if (self.expected_id not in (1, 2) or type(message.get("id")) is not int
                    or message["id"] != self.expected_id or "method" in message):
                raise ProtocolError("unexpected or duplicate response id")
            if ("result" in message) == ("error" in message):
                raise ProtocolError("response must contain exactly one result/error")
            self.responses[message["id"]] = message
            self.expected_id = 0
            self.condition.notify_all()

    def read_stderr(self) -> None:
        assert self.process.stderr is not None
        total = 0
        try:
            while chunk := os.read(self.process.stderr.fileno(), 65536):
                total += len(chunk)
                with self.condition:
                    room = self.client._max_stderr_bytes - len(self.stderr)
                    self.stderr.extend(chunk[:max(0, room)])
                if total > self.client._max_stderr_bytes:
                    self.fail(OutputLimitError("stderr exceeds max_stderr_bytes"))
                    return
        except (OSError, ValueError) as error:
            if not self.stop.is_set():
                self.fail(TransportError(f"cannot read stderr: {error}"))

    def write_stdin(self) -> None:
        assert self.process.stdin is not None
        try:
            while not self.stop.is_set():
                try:
                    frame = self.outgoing.get(timeout=0.05)
                except queue.Empty:
                    continue
                if frame is None:
                    self.process.stdin.close()
                    return
                view = memoryview(frame)
                while view and not self.stop.is_set():
                    written = os.write(self.process.stdin.fileno(), view)
                    if not written:
                        raise OSError("stdin write made no progress")
                    view = view[written:]
        except (OSError, ValueError) as error:
            if not self.stop.is_set():
                self.fail(TransportError(f"cannot write stdin: {error}"))

    def wait_response(self, id: int) -> dict[str, Any]:
        with self.condition:
            while id not in self.responses and self.failure is None and not self.eof:
                self.condition.wait(self.remaining())
            if self.failure:
                raise self.failure
            if id not in self.responses:
                raise TransportError(f"stdout ended before response id {id}")
            return _result(self.responses.pop(id))

    def run(self, initialize: bytes, notification: bytes, tool: bytes) -> dict[str, Any]:
        try:
            for name, target in (("stdout", self.read_stdout), ("stderr", self.read_stderr), ("stdin", self.write_stdin)):
                thread = threading.Thread(target=target, daemon=True, name=f"tinymem-{name}")
                self.threads.append(thread)
                thread.start()
            self.outgoing.put_nowait(initialize)
            result = self.wait_response(1)
            if result.get("protocolVersion") != PROTOCOL_VERSION:
                raise ProtocolError("server negotiated unsupported MCP version")
            info = result.get("serverInfo")
            if (not isinstance(result.get("capabilities"), dict) or not isinstance(info, dict)
                    or not isinstance(info.get("name"), str) or not isinstance(info.get("version"), str)):
                raise ProtocolError("malformed initialize result")
            with self.condition:
                self.expected_id = 2
            for frame in (notification, tool, None):
                self.outgoing.put_nowait(frame)
            result = self.wait_response(2)
            try:
                code = self.process.wait(timeout=self.remaining())
            except subprocess.TimeoutExpired as error:
                raise RPCTimeoutError("server did not exit after stdin EOF before deadline") from error
            for thread in self.threads:
                thread.join(timeout=self.remaining())
                if thread.is_alive():
                    raise RPCTimeoutError("stream cleanup exceeded operation deadline")
            if self.failure:
                raise self.failure
            if code != 0:
                raise TransportError(f"tinymem exited with code {code}")
            return result
        finally:
            self.cleanup()

    def cleanup(self) -> None:
        self.stop.set()
        if self.process.stdin is not None:
            self.process.stdin.close()
        try:
            self.process.wait(timeout=self.client._close_timeout)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:
                self.process.wait(timeout=self.client._close_timeout)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()  # Always reap the owned child after SIGKILL.
        for thread in self.threads:
            if thread.is_alive():
                thread.join(timeout=self.client._close_timeout)
        for stream in (self.process.stdout, self.process.stderr):
            if stream is not None:
                stream.close()


class Client:
    """Immutable startup configuration; each synchronous call owns/reaps a child."""
    def __init__(self, *, user_id: str, binary: str | os.PathLike[str] = "tinymem",
                 repository: str | os.PathLike[str] = ".", db: str | os.PathLike[str] | None = None,
                 agent_id: str | None = None, project_id: str | None = None,
                 embedding_command: str | os.PathLike[str] | None = None,
                 embedding_model: str | None = None, embedding_dimensions: int | None = None,
                 embedding_timeout_ms: int = 30000,
                 timeout: float = 10.0, close_timeout: float = 0.5,
                 max_frame_bytes: int = DEFAULT_MAX_FRAME_BYTES,
                 max_output_bytes: int = 4 * 1024 * 1024, max_stderr_bytes: int = 64 * 1024):
        _token(user_id, "user_id")
        for name, value in (("timeout", timeout), ("close_timeout", close_timeout)):
            if not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value) or value <= 0:
                raise ValueError(f"{name} must be finite and positive")
        for name, value, minimum in (("max_frame_bytes", max_frame_bytes, 1), ("max_output_bytes", max_output_bytes, 1),
                                     ("max_stderr_bytes", max_stderr_bytes, 0)):
            if type(value) is not int or value < minimum:
                raise ValueError(f"{name} must be an integer >= {minimum}")
        args = [os.fspath(binary)]
        if db is not None:
            args += ["--db", os.fspath(db)]
        args += ["mcp", os.fspath(repository), "--user", user_id]
        for flag, identifier in (("--agent", agent_id), ("--project", project_id)):
            if identifier is not None:
                args += [flag, _token(identifier, flag)]
        if any(value is not None for value in (embedding_command, embedding_model, embedding_dimensions)):
            if embedding_command is None or not isinstance(embedding_model, str) or not embedding_model.strip() or "\0" in embedding_model or len(embedding_model.encode("utf-8")) > 4096:
                raise ValueError("embedding_command and a nonblank embedding_model are required together")
            if type(embedding_dimensions) is not int or not 1 <= embedding_dimensions <= 4096:
                raise ValueError("embedding_dimensions must be an integer between 1 and 4096")
            if type(embedding_timeout_ms) is not int or not 1 <= embedding_timeout_ms <= 120000:
                raise ValueError("embedding_timeout_ms must be between 1 and 120000")
            args += ["--embedding-command", os.fspath(embedding_command), "--embedding-model", embedding_model,
                     "--embedding-dimensions", str(embedding_dimensions), "--embedding-timeout-ms", str(embedding_timeout_ms)]
        self._args = tuple(args)
        self._timeout, self._close_timeout = timeout, close_timeout
        self._max_frame_bytes, self._max_output_bytes, self._max_stderr_bytes = max_frame_bytes, max_output_bytes, max_stderr_bytes

    def call_tool(self, name: str, arguments: dict[str, Any] | None = None) -> dict[str, Any]:
        if not isinstance(name, str) or not name:
            raise ValueError("tool name must be nonempty")
        args = {} if arguments is None else arguments
        if not isinstance(args, dict):
            raise ValueError("arguments must be an object")
        if _SCOPE_KEYS.intersection(args):
            raise ValueError("scope and repository are fixed at process startup")
        initialize = _encode({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": PROTOCOL_VERSION, "capabilities": {},
            "clientInfo": {"name": "tinymem-native-python", "version": "0.1.0"}}}, self._max_frame_bytes)
        notification = _encode({"jsonrpc": "2.0", "method": "notifications/initialized"}, self._max_frame_bytes)
        tool = _encode({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": name, "arguments": args}}, self._max_frame_bytes)
        operation = _Operation(self)
        try:
            result = operation.run(initialize, notification, tool)
            content = result.get("content")
            if type(result.get("isError")) is not bool or not isinstance(content, list):
                raise ProtocolError("malformed MCP tool result")
            if any(not isinstance(item, dict) or not isinstance(item.get("type"), str)
                   or (item["type"] == "text" and not isinstance(item.get("text"), str)) for item in content):
                raise ProtocolError("malformed MCP content")
            if result["isError"]:
                raise ToolError(result)
            return cast(dict[str, Any], self._field(result, "structuredContent", dict))
        except NativeError as error:
            error.stderr = operation.stderr.decode("utf-8", errors="replace")
            error.returncode = operation.process.returncode
            if error.stderr:
                error.args = (f"{error}\nstderr: {error.stderr}",)
            raise

    @staticmethod
    def _field(result: dict[str, Any], key: str, expected: type, nullable: bool = False) -> Any:
        if key not in result or not ((nullable and result[key] is None) or type(result[key]) is expected):
            raise ProtocolError(f"malformed tool result: expected {key}")
        return result[key]

    @staticmethod
    def _budget(limit: int | None, max_bytes: int | None) -> dict[str, Any]:
        return {key: value for key, value in (("limit", limit), ("max_bytes", max_bytes)) if value is not None}

    def add_memory(self, content: str, *, kind: MemoryKind | None = None, source: str | None = None,
                   expires_at_ms: int | None = None, source_refs: list[FileSourceInput] | None = None) -> MemoryRecord | None:
        args: dict[str, Any] = {"content": content}
        args.update({key: value for key, value in (("kind", kind), ("source", source), ("expires_at_ms", expires_at_ms), ("source_refs", source_refs)) if value is not None})
        return cast(MemoryRecord | None, self._field(self.call_tool("add_memory", args), "memory", dict, nullable=True))

    def get_memory(self, id: str) -> MemoryRecord | None:
        return cast(MemoryRecord | None, self._field(self.call_tool("get_memory", {"id": _token(id, "id")}), "memory", dict, nullable=True))

    def search_memories(self, query: str, *, limit: int | None = None, max_bytes: int | None = None,
                        mode: SearchMode | None = None, min_similarity: float | None = None) -> list[MemoryRecord]:
        args: dict[str, Any] = {"query": query, **self._budget(limit, max_bytes)}
        args.update({key: value for key, value in (("mode", mode), ("min_similarity", min_similarity)) if value is not None})
        return cast(list[MemoryRecord], self._field(self.call_tool("search_memories", args), "memories", list))

    def index_memories(self, *, limit: int | None = None) -> EmbeddingIndexReport:
        result = self.call_tool("index_memories", {} if limit is None else {"limit": limit})
        for key in ("indexed", "stale", "remaining", "dimensions"):
            self._field(result, key, int)
        self._field(result, "model", str)
        return cast(EmbeddingIndexReport, result)

    def update_memory(self, id: str, content: str, *, expected_version: int | None = None) -> MemoryRecord | None:
        args: dict[str, Any] = {"id": _token(id, "id"), "content": content}
        if expected_version is not None:
            args["expected_version"] = expected_version
        return cast(MemoryRecord | None, self._field(self.call_tool("update_memory", args), "memory", dict, nullable=True))

    def delete_memory(self, id: str, *, expected_version: int | None = None) -> bool:
        args: dict[str, Any] = {"id": _token(id, "id")}
        if expected_version is not None:
            args["expected_version"] = expected_version
        return cast(bool, self._field(self.call_tool("delete_memory", args), "deleted", bool))

    def memory_history(self, id: str, *, limit: int | None = None, max_bytes: int | None = None) -> list[MemoryEvent]:
        return cast(list[MemoryEvent], self._field(self.call_tool("memory_history", {"id": _token(id, "id"), **self._budget(limit, max_bytes)}), "history", list))

    def get_profile(self, *, limit: int | None = None, max_bytes: int | None = None) -> Profile:
        result = self.call_tool("get_profile", self._budget(limit, max_bytes))
        self._field(result, "stable", list)
        self._field(result, "dynamic", list)
        return cast(Profile, result)

    def assemble_context(self, query: str, token_budget: int, *,
                         sources: list[ContextSource] | None = None,
                         exclude_paths: list[str] | None = None) -> AssembledContext:
        """Pack relevant evidence in the fixed scope; insert only result['prompt'].

        Budget uses a conservative UTF-8-byte token bound, including provenance.
        Repository cache refreshes; personal memory is never changed.
        """
        if type(token_budget) is not int or token_budget < 0:
            raise ValueError("token_budget must be an integer >= 0")
        args: dict[str, Any] = {"query": query, "token_budget": token_budget}
        if sources is not None:
            if (not isinstance(sources, list) or len(sources) > 2
                    or any(source not in ("memory", "repository") for source in sources)
                    or len(set(sources)) != len(sources)):
                raise ValueError("sources must be unique memory/repository values")
            args["sources"] = list(sources)
        if exclude_paths is not None:
            if (not isinstance(exclude_paths, list) or len(exclude_paths) > 64
                    or any(not isinstance(pattern, str) or not pattern
                           or len(pattern.encode("utf-8")) > 1024
                           or re.search(r"[\x00-\x1f\x7f-\x9f\\:]", pattern)
                           or any(part in ("", ".", "..") for part in pattern.split("/"))
                           for pattern in exclude_paths)
                    or sum(len(pattern.encode("utf-8")) for pattern in exclude_paths) > 16384):
                raise ValueError("exclude_paths must contain at most 64 normalized relative globs (1024 bytes each, 16 KiB total)")
            args["exclude_paths"] = list(exclude_paths)
        result = self.call_tool("assemble_context", args)
        for key, expected in (("prompt", str), ("entries", list), ("token_budget", int),
                              ("tokens_used", int), ("token_count_method", str)):
            self._field(result, key, expected)
        return cast(AssembledContext, result)

    def refresh_repository(self) -> RefreshReport:
        return cast(RefreshReport, self.call_tool("refresh_repository"))

    def search_repository(self, query: str, *, limit: int | None = None, max_bytes: int | None = None) -> RepositorySearchResult:
        result = self.call_tool("search_repository", {"query": query, **self._budget(limit, max_bytes)})
        self._field(result, "refresh", dict)
        self._field(result, "hits", list)
        return cast(RepositorySearchResult, result)
