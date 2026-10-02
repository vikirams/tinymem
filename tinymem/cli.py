"""Launch the shared MCP server, or register it with coding agents."""

from __future__ import annotations

import argparse
import json
import sqlite3
import sys

from tinymem.connect import TARGETS, ConnectError, connect
from tinymem.protocol import serve
from tinymem.store import Store, StoreError, default_db, default_namespace


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="tinymem")
    sub = parser.add_subparsers(dest="command")
    sub.add_parser("mcp", help="serve the shared stdio MCP server")
    register = sub.add_parser("connect", help="register that same server with coding agents")
    register.add_argument("--project", default=".")
    register.add_argument("--agent", action="append", choices=sorted(TARGETS), help="repeat to select agents; default is all")
    show = sub.add_parser("stats", help="show memory counts and database size")
    show.add_argument("--json", action="store_true", help="print machine-readable JSON")
    clean = sub.add_parser("purge", help="hard-delete soft-deleted and long-expired rows, then vacuum")
    clean.add_argument("--grace-days", type=int, default=30, help="age in days before hard delete (1-365)")
    profile = sub.add_parser("profile", help="get or set the static profile for a namespace")
    profile_sub = profile.add_subparsers(dest="profile_command", required=True)
    profile_get = profile_sub.add_parser("get", help="print the profile for a namespace")
    profile_get.add_argument("--namespace", default=None)
    profile_set = profile_sub.add_parser("set", help="set the profile for a namespace")
    profile_set.add_argument("--namespace", default=None)
    profile_set.add_argument("content", nargs="?", default=None, help="profile text; omit with --clear")
    profile_set.add_argument("--clear", action="store_true", help="clear the profile")
    args = parser.parse_args(argv)
    if args.command == "connect":
        return _connect(args.project, args.agent)
    if args.command == "stats":
        return _stats(args.json)
    if args.command == "purge":
        return _purge(args.grace_days)
    if args.command == "profile":
        if args.profile_command == "get":
            return _profile_get(args.namespace)
        return _profile_set(args.namespace, args.content, args.clear)
    return _serve()


def _serve() -> int:
    try:
        store = Store(default_db())
    except (OSError, sqlite3.Error) as exc:
        print(f"tinymem: cannot open memory database: {exc}", file=sys.stderr)
        return 1
    try:
        serve(sys.stdin, sys.stdout, store)
    finally:
        store.close()
    return 0


def _open_store() -> Store:
    return Store(default_db())


def _stats(as_json: bool) -> int:
    try:
        store = _open_store()
    except (OSError, sqlite3.Error) as exc:
        print(f"tinymem: cannot open memory database: {exc}", file=sys.stderr)
        return 1
    try:
        data = store.stats()
    finally:
        store.close()
    if as_json:
        print(json.dumps(data, indent=2))
    else:
        print(f"db: {data['db_path']} ({data['db_bytes']} bytes)")
        print(f"active: {data['active']} deleted: {data['deleted']} expired-hidden: {data['expired']}")
        for entry in data["by_namespace"]:
            print(f"ns {entry['namespace']}: {entry['active']}")
        for entry in data["by_kind"]:
            print(f"kind {entry['kind']}: {entry['active']}")
    return 0


def _purge(grace_days: int) -> int:
    try:
        store = _open_store()
    except (OSError, sqlite3.Error) as exc:
        print(f"tinymem: cannot open memory database: {exc}", file=sys.stderr)
        return 1
    try:
        try:
            result = store.purge(grace_days)
        except StoreError as exc:
            print(f"tinymem: {exc}", file=sys.stderr)
            return 1
    finally:
        store.close()
    print(json.dumps(result))
    return 0


def _profile_get(namespace: str | None) -> int:
    space = namespace or default_namespace()
    try:
        store = _open_store()
    except (OSError, sqlite3.Error) as exc:
        print(f"tinymem: cannot open memory database: {exc}", file=sys.stderr)
        return 1
    try:
        try:
            result = store.get_profile(space)
        except StoreError as exc:
            print(f"tinymem: {exc}", file=sys.stderr)
            return 1
    finally:
        store.close()
    print(json.dumps(result, indent=2))
    return 0


def _profile_set(namespace: str | None, content: str | None, clear: bool) -> int:
    space = namespace or default_namespace()
    if clear:
        text = ""
    elif content is None:
        print("tinymem: provide profile text or --clear", file=sys.stderr)
        return 1
    else:
        text = content
    try:
        store = _open_store()
    except (OSError, sqlite3.Error) as exc:
        print(f"tinymem: cannot open memory database: {exc}", file=sys.stderr)
        return 1
    try:
        try:
            result = store.set_profile(space, text)
        except StoreError as exc:
            print(f"tinymem: {exc}", file=sys.stderr)
            return 1
    finally:
        store.close()
    print(json.dumps(result, indent=2))
    return 0


def _connect(project: str, agents: list[str] | None) -> int:
    try:
        from pathlib import Path

        written = connect(Path(project), agents)
    except ConnectError as exc:
        print(f"tinymem: {exc}", file=sys.stderr)
        return 1
    for path in written:
        print(path)
    return 0
