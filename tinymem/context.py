"""Assemble SessionStart Context: Profile plus a recency Index. Fail-open."""

from __future__ import annotations

import json
import sqlite3

from tinymem.store import INDEX_CAP, INDEX_LINE, Store, StoreError, default_db, default_namespace

FORMATS = ("text", "claude", "cursor")


def index_line(content: str) -> str:
    line = content.strip().splitlines()[0] if content.strip() else ""
    if len(line) > INDEX_LINE:
        return line[: INDEX_LINE - 1] + "…"
    return line


def assemble(store: Store, namespace: str) -> str:
    space = namespace or default_namespace()
    profile = store.get_profile(space)["profile"].strip()
    rows = store.recent(space, INDEX_CAP)
    parts: list[str] = ["# TinyMem"]
    if profile:
        parts.append("## Profile")
        parts.append(profile)
    parts.append("## Index")
    if not rows:
        parts.append("(empty)")
    else:
        for row in rows:
            parts.append(f"- `{row['id']}` {row['kind']}: {index_line(row['content'])}")
    parts.append("Full Memory text: search_memories. Do not persist from this Hook.")
    return "\n".join(parts) + "\n"


def emit(namespace: str | None = None, fmt: str = "text") -> str:
    if fmt not in FORMATS:
        fmt = "text"
    text = ""
    try:
        store = Store(default_db())
        try:
            text = assemble(store, namespace or default_namespace())
        finally:
            store.close()
    except (OSError, sqlite3.Error, StoreError, TypeError, ValueError):
        text = ""
    if fmt == "claude":
        return json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": text,
                }
            },
            separators=(",", ":"),
        )
    if fmt == "cursor":
        return json.dumps({"additionalContext": text}, separators=(",", ":"))
    return text
