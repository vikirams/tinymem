"""Register the same server command with each coding agent's MCP config."""

from __future__ import annotations

import json
import os
import re
from pathlib import Path

from tinymem.store import default_db

SERVER_NAME = "tinymem"
DESCRIPTION = "Local memory shared by every coding agent"


class ConnectError(ValueError):
    pass


def launch(namespace: str | None = None) -> dict[str, object]:
    return {
        "command": "tinymem",
        "args": ["mcp"],
        "env": {"TINYMEM_DB": str(default_db()), "TINYMEM_NAMESPACE": namespace or "default"},
    }


def project_namespace(root: Path) -> str:
    name = ""
    try:
        import subprocess

        completed = subprocess.run(
            ["git", "-C", str(root), "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            timeout=5,
            check=False,
        )
        if completed.returncode == 0 and completed.stdout.strip():
            name = Path(completed.stdout.strip()).name
        else:
            name = root.name
    except Exception:
        name = root.name
    sanitized = re.sub(r"[^A-Za-z0-9_:-]", "-", name.strip())
    sanitized = re.sub(r"-{2,}", "-", sanitized).strip("-")[:64].strip("-")
    if not sanitized:
        return "default"
    return sanitized


def connect(project: Path, agents: list[str] | None = None) -> list[Path]:
    root = project.expanduser().resolve()
    if not root.is_dir():
        raise ConnectError(f"project is not a directory: {root}")
    selected = agents or list(TARGETS)
    unknown = [name for name in selected if name not in TARGETS]
    if unknown:
        raise ConnectError(f"unknown agent: {', '.join(unknown)}")
    entry = launch(project_namespace(root))
    written: list[Path] = []
    for name in selected:
        path = TARGETS[name](root, entry)
        written.append(path)
    return written


def write_pi(root: Path, entry: dict) -> Path:
    return _write_json(root / ".pi" / "mcp.json", "mcpServers", {**entry, "description": DESCRIPTION, "exposure": "direct"})


def write_cursor(root: Path, entry: dict) -> Path:
    return _write_json(root / ".cursor" / "mcp.json", "mcpServers", entry)


def write_claude(root: Path, entry: dict) -> Path:
    return _write_json(root / ".mcp.json", "mcpServers", entry)


def write_vscode(root: Path, entry: dict) -> Path:
    return _write_json(root / ".vscode" / "mcp.json", "servers", {**entry, "type": "stdio"})


def write_codex(root: Path, entry: dict) -> Path:
    path = root / ".codex" / "config.toml"
    _refuse_symlink(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    existing = path.read_text(encoding="utf-8") if path.exists() else ""
    kept = _without_codex_section(existing)
    block = _codex_block(entry)
    text = kept.rstrip()
    path.write_text((text + "\n\n" + block if text else block), encoding="utf-8")
    return path


TARGETS = {
    "pi": write_pi,
    "cursor": write_cursor,
    "claude": write_claude,
    "vscode": write_vscode,
    "codex": write_codex,
}


def _write_json(path: Path, key: str, entry: dict) -> Path:
    _refuse_symlink(path)
    data: dict = {}
    if path.exists():
        try:
            loaded = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, json.JSONDecodeError) as exc:
            raise ConnectError(f"refusing to overwrite unreadable config: {path}") from exc
        if not isinstance(loaded, dict):
            raise ConnectError(f"refusing to overwrite non-object config: {path}")
        data = loaded
    servers = data.get(key, {})
    if not isinstance(servers, dict):
        raise ConnectError(f"refusing to overwrite non-object {key} in {path}")
    current = servers.get(SERVER_NAME, {})
    if not isinstance(current, dict):
        raise ConnectError(f"refusing to overwrite non-object {SERVER_NAME} entry in {path}")
    current.update(entry)
    servers[SERVER_NAME] = current
    data[key] = servers
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    os.replace(temporary, path)
    return path


def _refuse_symlink(path: Path) -> None:
    if path.is_symlink():
        raise ConnectError(f"refusing to follow symlink: {path}")


def _without_codex_section(text: str) -> str:
    kept: list[str] = []
    skipping = False
    for line in text.splitlines(keepends=True):
        header = re.match(r"\s*\[([^]]+)]\s*(?:#.*)?$", line)
        if header:
            name = header.group(1).strip().strip('"').strip("'")
            skipping = name == "mcp_servers.tinymem" or name.startswith("mcp_servers.tinymem.")
        if not skipping:
            kept.append(line)
    return "".join(kept)


def _codex_block(entry: dict) -> str:
    args = ", ".join(_toml(arg) for arg in entry["args"])
    env = entry["env"]
    db = env.get("TINYMEM_DB", "")
    namespace = env.get("TINYMEM_NAMESPACE", "default")
    return (
        "[mcp_servers.tinymem]\n"
        f"command = {_toml(entry['command'])}\n"
        f"args = [{args}]\n"
        "\n"
        "[mcp_servers.tinymem.env]\n"
        f"TINYMEM_DB = {_toml(db)}\n"
        f"TINYMEM_NAMESPACE = {_toml(namespace)}\n"
    )


def _toml(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'
