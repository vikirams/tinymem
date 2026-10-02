"""Stdio MCP for coding agents. Newline-delimited JSON-RPC, no per-agent dialect."""

from __future__ import annotations

import json
import time
from collections import deque
from typing import TextIO

from tinymem import __version__
from tinymem.store import MAX_PROFILE, KINDS, Store, StoreError, default_namespace

PROTOCOL_VERSIONS = ("2024-11-05", "2025-03-26", "2025-06-18")
LATEST = PROTOCOL_VERSIONS[-1]
MAX_LINE = 1_048_576
MAX_RESULT = 16_000
INSTRUCTIONS = (
    "Local memory shared by coding agents in one repo. Retrieval is FTS5 lexical "
    "with porter stemming, prefix match, and BM25 plus recency; model-free and offline. "
    "Write discipline: add_memory only for durable facts, preferences, decisions, plans "
    "in 1-2 sentences (aim under 500 chars); never re-add a correction, use update_memory; "
    "set valid_until (YYYY-MM-DD) for transient facts like exams, branches, WIP. "
    "Namespace isolates projects and defaults to TINYMEM_NAMESPACE for this repo; pass it "
    "explicitly only to read another project. "
    "Read discipline: prefer get_profile for stack/conventions, then search_memories with "
    "1-3 distinctive terms and limit 3-5; shorten the query on zero hits. "
    "delete_memory hides a note from search; it does not erase the database file."
)


class Session:
    def __init__(self, store: Store) -> None:
        self.store = store
        self.version: str | None = None
        self._calls: deque[float] = deque()

    def handle(self, message: object) -> dict | None:
        if not isinstance(message, dict) or message.get("jsonrpc") != "2.0" or not isinstance(message.get("method"), str):
            return _error(_id_of(message), -32600, "Invalid Request")
        method = message["method"]
        has_id = "id" in message and message["id"] is not None
        if has_id and not _valid_id(message["id"]):
            return _error(None, -32600, "Invalid Request")
        if not has_id:
            return None
        request_id = message["id"]
        if method == "initialize":
            return self._initialize(request_id, message.get("params"))
        if method == "ping":
            return _result(request_id, {})
        if self.version is None:
            return _error(request_id, -32600, "Server not initialized")
        if method == "tools/list":
            return self._list(request_id, message.get("params"))
        if method == "tools/call":
            return self._call(request_id, message.get("params"))
        return _error(request_id, -32601, "Method not found")

    def _initialize(self, request_id: object, params: object) -> dict:
        requested = params.get("protocolVersion") if isinstance(params, dict) else None
        self.version = requested if requested in PROTOCOL_VERSIONS else LATEST
        return _result(
            request_id,
            {
                "protocolVersion": self.version,
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {"name": "tinymem", "version": __version__},
                "instructions": INSTRUCTIONS,
            },
        )

    def _list(self, request_id: object, params: object) -> dict:
        if isinstance(params, dict) and params.get("cursor"):
            return _error(request_id, -32602, "Invalid cursor")
        return _result(request_id, {"tools": [_tool(name, description, schema, read_only, destructive) for name, description, schema, read_only, destructive in TOOLS]})

    def _call(self, request_id: object, params: object) -> dict:
        if not isinstance(params, dict) or not isinstance(params.get("name"), str):
            return _error(request_id, -32602, "Invalid params")
        arguments = params.get("arguments", {})
        if arguments is None:
            arguments = {}
        if not isinstance(arguments, dict):
            return _error(request_id, -32602, "Invalid params")
        if not self._allow():
            return _result(request_id, _tool_error("Tool call rate limit exceeded; retry later"))
        try:
            data = _dispatch(self.store, params["name"], arguments)
        except StoreError as exc:
            return _result(request_id, _tool_error(str(exc)))
        except KeyError:
            return _error(request_id, -32602, "Unknown tool")
        except Exception:
            return _error(request_id, -32603, "Internal error")
        return _result(request_id, _tool_ok(data, self.version or LATEST))

    def _allow(self) -> bool:
        now = time.monotonic()
        while self._calls and now - self._calls[0] > 10:
            self._calls.popleft()
        if len(self._calls) >= 60:
            return False
        self._calls.append(now)
        return True


def serve(stdin: TextIO, stdout: TextIO, store: Store) -> None:
    session = Session(store)
    while True:
        line = stdin.buffer.readline(MAX_LINE + 1)
        if not line:
            return
        if len(line) > MAX_LINE and not line.endswith(b"\n"):
            _drain(stdin)
            _write(stdout, _error(None, -32700, "Parse error"))
            continue
        try:
            message = json.loads(line.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError):
            _write(stdout, _error(None, -32700, "Parse error"))
            continue
        response = session.handle(message)
        if response is not None:
            _write(stdout, response)


def _dispatch(store: Store, name: str, arguments: dict) -> dict:
    if name == "add_memory":
        namespace = arguments.get("namespace")
        if namespace is None:
            namespace = default_namespace()
        return store.add(
            _required(arguments, "content"),
            arguments.get("kind", "fact"),
            namespace,
            arguments.get("valid_until"),
        )
    if name == "search_memories":
        namespace = arguments.get("namespace")
        if namespace is None:
            namespace = default_namespace()
        return store.search(
            _required(arguments, "query"),
            arguments.get("limit", 5),
            namespace,
            arguments.get("kind"),
        )
    if name == "update_memory":
        if "content" not in arguments and "kind" not in arguments and "valid_until" not in arguments:
            raise StoreError("nothing to update: provide content, kind, or valid_until")
        kwargs: dict = {}
        if "content" in arguments:
            kwargs["content"] = arguments["content"]
        if "kind" in arguments:
            kwargs["kind"] = arguments["kind"]
        if "valid_until" in arguments:
            kwargs["valid_until"] = arguments["valid_until"]
        return store.update(_required(arguments, "id"), namespace=default_namespace(), **kwargs)
    if name == "delete_memory":
        return store.delete(_required(arguments, "id"), namespace=default_namespace())
    if name == "get_profile":
        namespace = arguments.get("namespace")
        if namespace is None:
            namespace = default_namespace()
        return store.get_profile(namespace)
    if name == "set_profile":
        namespace = arguments.get("namespace")
        if namespace is None:
            namespace = default_namespace()
        return store.set_profile(namespace, _required(arguments, "profile"))
    if name == "memory_stats":
        return store.stats()
    raise KeyError(name)


def _required(arguments: dict, name: str) -> object:
    if name not in arguments:
        raise StoreError(f"{name} is required")
    return arguments[name]


def _tool(name: str, description: str, schema: dict, read_only: bool, destructive: bool) -> dict:
    return {
        "name": name,
        "description": description,
        "inputSchema": schema,
        "annotations": {
            "readOnlyHint": read_only,
            "destructiveHint": destructive,
            "idempotentHint": name
            in ("search_memories", "update_memory", "get_profile", "set_profile", "memory_stats"),
            "openWorldHint": False,
        },
    }


def _object(properties: dict, required: list[str]) -> dict:
    return {"type": "object", "properties": properties, "required": required, "additionalProperties": False}


TOOLS = (
    (
        "add_memory",
        "Store a durable fact, preference, decision, or plan in 1-2 sentences. Never re-add a correction; use update_memory. Set valid_until for transient facts.",
        _object(
            {
                "content": {"type": "string", "minLength": 1, "maxLength": 8000},
                "kind": {"type": "string", "enum": list(KINDS), "default": "fact"},
                "namespace": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 64,
                    "pattern": "^[A-Za-z0-9_:-]{1,64}$",
                },
                "valid_until": {"type": ["string", "null"]},
            },
            ["content"],
        ),
        False,
        False,
    ),
    (
        "search_memories",
        "Search active memories. Use 1-3 distinctive terms with limit 3-5; shorten the query on zero hits. Prefer get_profile for stack/conventions.",
        _object(
            {
                "query": {"type": "string", "minLength": 1, "maxLength": 200},
                "limit": {"type": "integer", "minimum": 1, "maximum": 20, "default": 5},
                "namespace": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 64,
                    "pattern": "^[A-Za-z0-9_:-]{1,64}$",
                },
                "kind": {"type": "string", "enum": list(KINDS)},
            },
            ["query"],
        ),
        True,
        False,
    ),
    (
        "update_memory",
        "Correct a memory instead of re-adding. Bumps updated_at so fresher facts rank higher. Pass valid_until null to clear expiry.",
        _object(
            {
                "id": {"type": "string", "minLength": 32, "maxLength": 32},
                "content": {"type": "string", "minLength": 1, "maxLength": 8000},
                "kind": {"type": "string", "enum": list(KINDS)},
                "valid_until": {"type": ["string", "null"]},
            },
            ["id"],
        ),
        False,
        False,
    ),
    (
        "get_profile",
        "Get the small static profile for a namespace (stack, conventions, commands). Cheaper than search; read this first.",
        _object(
            {
                "namespace": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 64,
                    "pattern": "^[A-Za-z0-9_:-]{1,64}$",
                },
            },
            [],
        ),
        True,
        False,
    ),
    (
        "set_profile",
        "Set the static profile for a namespace (aim under 4000 chars). Pass an empty profile to clear it.",
        _object(
            {
                "namespace": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 64,
                    "pattern": "^[A-Za-z0-9_:-]{1,64}$",
                },
                "profile": {"type": "string", "maxLength": MAX_PROFILE},
            },
            ["profile"],
        ),
        False,
        False,
    ),
    (
        "memory_stats",
        "Show memory counts by namespace/kind plus database size. Use to check quota before bulk adds.",
        _object({}, []),
        True,
        False,
    ),
    (
        "delete_memory",
        "Hide a memory from search. This is not secure erasure of the database file.",
        _object({"id": {"type": "string", "minLength": 32, "maxLength": 32}}, ["id"]),
        False,
        True,
    ),
)


def _tool_ok(data: dict, version: str) -> dict:
    result = _content_result(data, False)
    if version >= "2025-06-18":
        result["structuredContent"] = data
    return result


def _tool_error(message: str) -> dict:
    return _content_result({"error": message}, True)


def _content_result(data: dict, is_error: bool) -> dict:
    text = json.dumps(data, ensure_ascii=False, separators=(",", ":"))
    if len(text.encode("utf-8")) > MAX_RESULT:
        text = json.dumps({"error": "Result exceeds the output byte limit"}, separators=(",", ":"))
        is_error = True
    return {"content": [{"type": "text", "text": text}], "isError": is_error}


def _result(request_id: object, result: dict) -> dict:
    return {"jsonrpc": "2.0", "id": request_id, "result": result}


def _error(request_id: object, code: int, message: str) -> dict:
    return {"jsonrpc": "2.0", "id": request_id, "error": {"code": code, "message": message}}


def _id_of(message: object) -> object:
    if isinstance(message, dict) and _valid_id(message.get("id")):
        return message["id"]
    return None


def _valid_id(value: object) -> bool:
    return isinstance(value, str) or (isinstance(value, int) and not isinstance(value, bool))


def _write(stdout: TextIO, message: dict) -> None:
    stdout.buffer.write(json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode("utf-8") + b"\n")
    stdout.buffer.flush()


def _drain(stdin: TextIO) -> None:
    while True:
        more = stdin.buffer.readline(65536)
        if not more or more.endswith(b"\n"):
            return
