#!/usr/bin/env python3
"""Tiny controllable MCP process boundary, not a native engine emulator."""
import json
import os
import signal
import sys
import time
from pathlib import Path

args = sys.argv[1:]
mode = args[args.index("--user") + 1]
if "--db" in args:
    Path(args[args.index("--db") + 1]).write_text(str(os.getpid()))
if mode == "ignore-term":
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
if mode == "init-hang":
    time.sleep(60)


def emit(value):
    frame = json.dumps(value, ensure_ascii=False).encode() + b"\n"
    if mode == "fragmented":
        for start in range(0, len(frame), 7):
            sys.stdout.buffer.write(frame[start:start + 7])
            sys.stdout.flush()
            time.sleep(0.001)
    else:
        sys.stdout.buffer.write(frame)
        sys.stdout.flush()


messages = []
for line in sys.stdin.buffer:
    request = json.loads(line)
    messages.append(request)
    if request["method"] == "initialize":
        assert request["id"] == 1
        assert request["params"]["protocolVersion"] == "2025-06-18"
        assert request["params"]["capabilities"] == {}
        if mode == "stderr-flood":
            sys.stderr.buffer.write(b"e" * 8192)
            sys.stderr.flush()
        if mode == "startup-exit":
            sys.stderr.write("intentional startup failure\n")
            sys.exit(7)
        result = {"protocolVersion": "wrong" if mode == "wrong-version" else "2025-06-18", "capabilities": {"tools": {}},
                  "serverInfo": {"name": "fixture", "version": "1"}}
        if mode == "bad-server-info":
            result["serverInfo"] = {"name": 7}
    elif request["method"] == "notifications/initialized":
        assert "id" not in request
        if mode == "no-read":
            time.sleep(60)
        continue
    else:
        assert request["id"] == 2 and request["method"] == "tools/call"
        assert messages[-2]["method"] == "notifications/initialized"
        if mode == "tool-hang":
            time.sleep(60)
        if mode in ("invalid-json", "invalid-utf8", "frame-flood", "partial-frame"):
            frames = {"invalid-json": b"not-json\n", "invalid-utf8": b"\xff\n",
                      "frame-flood": b"x" * 8192 + b"\n", "partial-frame": b"{\"jsonrpc\":"}
            sys.stdout.buffer.write(frames[mode])
            sys.stdout.flush()
            continue
        if mode in ("rpc-error", "bad-rpc-error"):
            emit({"jsonrpc": "2.0", "id": 2, "error": {
                "code": "invalid" if mode == "bad-rpc-error" else -32602,
                "message": "invalid arguments", "data": {"detail": 1}}})
            continue
        if mode == "notification-flood":
            for _ in range(100):
                emit({"jsonrpc": "2.0", "method": "notifications/progress", "params": {"v": "x" * 100}})
        if mode in ("notifications", "malformed-notification"):
            notification = {"jsonrpc": "2.0", "method": "notifications/message", "params": {"message": "hello"}}
            if mode == "malformed-notification":
                notification["result"] = {}
            emit(notification)
        structured = {"arguments": request["params"]["arguments"], "argv": args, "pid": os.getpid()}
        if mode == "wrappers":
            payload = request["params"]["arguments"]
            record = {"id": "fixture-id", "scope": {"user_id": mode, "agent_id": None, "project_id": None},
                      "content": json.dumps(payload), "kind": "fact", "source": request["params"]["name"],
                      "expires_at_ms": None, "version": 1, "created_at_ms": 1, "updated_at_ms": 1}
            report = {"repository": "/fixture", "indexed": 0, "unchanged": 0, "removed": 0, "skipped": 0, "indexed_at_ms": 1}
            event = {"event": "added", "version": 1, "at_ms": 1, "memory": record}
            structured = {"memory": record, "memories": [record], "history": [event], "deleted": True,
                          "stable": [record], "dynamic": [], **report, "refresh": report, "hits": []}
            if request["params"]["name"] == "assemble_context":
                prompt = json.dumps(payload)
                structured = {"prompt": prompt, "entries": [], "token_budget": payload["token_budget"],
                              "tokens_used": len(prompt.encode()), "token_count_method": "utf8_bytes_upper_bound"}
        if mode == "null-memory":
            structured = {"memory": None}
        if mode == "large-result":
            structured = {"payload": "x" * (1024 * 1024 + 64)}
        content_text = (structured["prompt"] if request["params"]["name"] == "assemble_context"
                        and "prompt" in structured else json.dumps(structured, ensure_ascii=False))
        result = {"content": [{"type": "text", "text": content_text}],
                  "structuredContent": structured, "isError": False}
        if mode == "business-error":
            result = {"content": [{"type": "text", "text": "business failure"}], "isError": True}
        if mode == "malformed-result":
            result["structuredContent"] = []
        if mode == "wrong-id":
            request["id"] = 9
    response = {"jsonrpc": "2.0", "id": request["id"], "result": result}
    emit(response)
    if mode == "duplicate-id" and request["id"] == 2:
        emit(response)
    if mode == "unsolicited-zero" and request["id"] == 2:
        emit({**response, "id": 0})
if mode == "ignore-term":
    time.sleep(60)
if mode == "exit-nonzero":
    sys.stderr.write("intentional exit after response\n")
    sys.exit(9)
if mode not in ("wrong-version", "bad-server-info", "startup-exit"):
    assert [message["method"] for message in messages] == ["initialize", "notifications/initialized", "tools/call"]
