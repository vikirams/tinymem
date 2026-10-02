#!/usr/bin/env python3
"""Configure TinyMem's native stdio MCP server in a project's .pi/mcp.json.

This uses only Python's standard library. It never launches the server or opens,
creates, truncates, or deletes the database; it only creates a private parent
folder when needed. Existing MCP configuration is merged safely.
"""

from __future__ import annotations

import argparse
import getpass
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
from typing import Any, TextIO


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
SERVER_NAME_RE = re.compile(r"^[A-Za-z0-9_-]+$")
SCOPE_TOKEN_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.:@-]{0,127}")
MAX_CONFIG_BYTES = 64 * 1024


class SetupError(Exception):
    """A safe, user-actionable setup failure."""


def _scope_token(value: str, label: str, option: str) -> str:
    if not SCOPE_TOKEN_RE.fullmatch(value):
        raise SetupError(
            f"{label} must be a 1-128 character ASCII identifier token "
            "matching [A-Za-z0-9][A-Za-z0-9_.:@-]{0,127}; "
            f"choose a valid value with {option}"
        )
    return value


def default_project_scope(root: Path) -> str:
    """Use a readable valid basename, or a path-stable sanitized token when needed."""
    canonical_root = root.resolve()
    basename = canonical_root.name
    if SCOPE_TOKEN_RE.fullmatch(basename):
        return basename
    slug = re.sub(r"[^A-Za-z0-9_.:@-]+", "-", basename).strip("._:@-") or "project"
    fingerprint = hashlib.sha256(os.fsencode(canonical_root)).hexdigest()[:12]
    slug = slug[:115].rstrip("._:@-") or "project"
    return f"{slug}-{fingerprint}"


def _server_name(value: str) -> str:
    if not SERVER_NAME_RE.fullmatch(value):
        raise SetupError("server name may contain only letters, digits, '_' and '-'")
    return value


def _name_key(value: str) -> str:
    # Pi considers hyphen and underscore equivalent for MCP server names.
    return value.replace("-", "_")


def data_root(*, home: Path | None = None, environ: dict[str, str] | None = None,
              platform: str | None = None) -> Path:
    """Return the conventional per-user data directory without creating it."""
    env = os.environ if environ is None else environ
    home_path = Path.home() if home is None else home
    system = sys.platform if platform is None else platform
    if system == "darwin":
        return home_path / "Library" / "Application Support" / "tinymem"
    if system == "win32":
        local_default = str(home_path / "AppData" / "Local")
        local_app_data = Path(env.get("LOCALAPPDATA", local_default)).expanduser()
        if not local_app_data.is_absolute():
            local_app_data = home_path / "AppData" / "Local"
        return local_app_data / "TinyMem"
    xdg = env.get("XDG_DATA_HOME")
    if xdg:
        xdg_path = Path(xdg).expanduser()
        if xdg_path.is_absolute():
            return xdg_path / "tinymem"
    return home_path / ".local" / "share" / "tinymem"


def default_binary_path(
    repository_root: Path | None = None, *, platform: str | None = None
) -> Path:
    system = sys.platform if platform is None else platform
    executable = "tinymem.exe" if system == "win32" else "tinymem"
    checkout = REPOSITORY_ROOT if repository_root is None else repository_root
    return checkout / "target" / "release" / executable


def default_database_path(root: Path, *, home: Path | None = None,
                          environ: dict[str, str] | None = None,
                          platform: str | None = None) -> Path:
    """Choose one private database location per canonical project root."""
    canonical_root = root.resolve()
    project_label = re.sub(r"[^A-Za-z0-9._-]+", "-", canonical_root.name).strip("-._")
    if not project_label:
        project_label = "repository"
    project_label = project_label[:40]
    fingerprint = hashlib.sha256(os.fsencode(canonical_root)).hexdigest()[:12]
    directory = data_root(home=home, environ=environ, platform=platform).resolve()
    return directory / f"{project_label}-{fingerprint}" / "memory.db"


def make_server_config(*, binary: Path, root: Path, database: Path, user: str,
                       project: str, agent: str | None, exposure: str) -> dict[str, Any]:
    args = ["--db", str(database), "mcp", str(root), "--user", user]
    if agent is not None:
        args.extend(["--agent", agent])
    args.extend(["--project", project])
    return {
        "command": str(binary),
        "args": args,
        "exposure": exposure,
        "description": "Local, project-scoped memory and repository context",
    }


def _read_regular_config_file(path: Path, *, missing_ok: bool) -> tuple[bytes | None, int | None]:
    """Read a bounded regular file without blocking on FIFOs or following symlinks."""
    if path.is_symlink():
        raise SetupError(f"refusing to replace symlinked Pi config: {path}")
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        if missing_ok:
            return None, None
        raise SetupError(f"Pi config disappeared during setup: {path}")
    except OSError as error:
        raise SetupError(f"cannot inspect Pi config {path}: {error}") from error
    if not stat.S_ISREG(metadata.st_mode):
        raise SetupError(f"Pi config must be a regular file: {path}")
    if metadata.st_size > MAX_CONFIG_BYTES:
        raise SetupError(f"Pi config exceeds {MAX_CONFIG_BYTES} bytes: {path}")

    flags = os.O_RDONLY | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = -1
    try:
        descriptor = os.open(path, flags)
        with os.fdopen(descriptor, "rb") as stream:
            descriptor = -1
            opened_metadata = os.fstat(stream.fileno())
            if not stat.S_ISREG(opened_metadata.st_mode):
                raise SetupError(f"Pi config must be a regular file: {path}")
            if opened_metadata.st_size > MAX_CONFIG_BYTES:
                raise SetupError(f"Pi config exceeds {MAX_CONFIG_BYTES} bytes: {path}")
            original = stream.read(MAX_CONFIG_BYTES + 1)
        if len(original) > MAX_CONFIG_BYTES:
            raise SetupError(f"Pi config exceeds {MAX_CONFIG_BYTES} bytes: {path}")
        return original, stat.S_IMODE(opened_metadata.st_mode)
    except FileNotFoundError as error:
        if missing_ok:
            return None, None
        raise SetupError(f"Pi config disappeared during setup: {path}") from error
    except OSError as error:
        raise SetupError(f"cannot read Pi config {path}: {error}") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def _read_config(path: Path) -> tuple[dict[str, Any], bytes | None, int | None]:
    original, mode = _read_regular_config_file(path, missing_ok=True)
    if original is None:
        return {"mcpServers": {}}, None, None
    try:
        value = json.loads(original)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SetupError(f"Pi config is not valid JSON: {path}") from error
    if not isinstance(value, dict):
        raise SetupError("Pi config must contain a top-level JSON object")
    servers = value.setdefault("mcpServers", {})
    if not isinstance(servers, dict):
        raise SetupError("Pi config 'mcpServers' must be a JSON object")
    return value, original, mode


def _is_native_server(entry: Any) -> bool:
    if not isinstance(entry, dict):
        return False
    command = entry.get("command")
    args = entry.get("args")
    if not isinstance(command, str) or not isinstance(args, list):
        return False
    return Path(command).name.lower() in {"tinymem", "tinymem.exe"} and "mcp" in args


def _create_private_directory(path: Path) -> None:
    """Create a missing directory with private permissions; never chmod existing paths."""
    try:
        path.mkdir(mode=0o700, parents=True, exist_ok=True)
    except OSError as error:
        raise SetupError(f"cannot create database directory {path}: {error}") from error
    if not path.is_dir():
        raise SetupError(f"database parent is not a directory: {path}")
    # Existing directories may be shared intentionally; do not change their mode.


def _atomic_write_config(path: Path, rendered: bytes, original: bytes | None,
                         existing_mode: int | None) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.is_symlink():
        raise SetupError(f"refusing to replace symlinked Pi config: {path}")
    if original is None:
        if path.exists():
            raise SetupError(f"Pi config appeared during setup; refusing to overwrite: {path}")
    else:
        current, _ = _read_regular_config_file(path, missing_ok=False)
        if current != original:
            raise SetupError(f"Pi config changed during setup; refusing to overwrite: {path}")

    file_descriptor = -1
    temporary: str | None = None
    try:
        file_descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp",
                                                       dir=path.parent)
        mode = existing_mode if existing_mode is not None else 0o600
        os.chmod(temporary, mode)
        with os.fdopen(file_descriptor, "wb") as stream:
            file_descriptor = -1
            stream.write(rendered)
            stream.flush()
            os.fsync(stream.fileno())
        if path.is_symlink():
            raise SetupError(f"refusing to replace symlinked Pi config: {path}")
        if original is None:
            # Hard-link creation is atomic and will not overwrite a config that
            # appeared after the initial check.
            try:
                os.link(temporary, path)
            except FileExistsError as error:
                raise SetupError(
                    f"Pi config appeared during setup; refusing to overwrite: {path}"
                ) from error
            os.unlink(temporary)
            temporary = None
        else:
            current, _ = _read_regular_config_file(path, missing_ok=False)
            if current != original:
                raise SetupError(f"Pi config changed during setup; refusing to overwrite: {path}")
            os.replace(temporary, path)
            temporary = None
    except OSError as error:
        raise SetupError(f"cannot write Pi config {path}: {error}") from error
    finally:
        if file_descriptor >= 0:
            os.close(file_descriptor)
        if temporary is not None:
            try:
                os.unlink(temporary)
            except FileNotFoundError:
                pass


def _format_scope(user: str, project: str, agent: str | None) -> str:
    text = f"user={user!r}, project={project!r}"
    if agent is not None:
        text += f", agent={agent!r}"
    else:
        text += ", agent=<shared across agents in this user/project scope>"
    return text


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Safely add TinyMem's native MCP server to project .pi/mcp.json."
    )
    parser.add_argument("--root", type=Path, default=Path.cwd(),
                        help="project/repository root (default: current directory)")
    parser.add_argument(
        "--binary", type=Path, default=None,
        help="native tinymem executable (default: this checkout's target/release/tinymem[.exe])",
    )
    parser.add_argument("--db", type=Path, default=None,
                        help="database path (default: private per-project user data directory)")
    parser.add_argument("--user", default=None,
                        help="exact TinyMem user scope (default: current OS username)")
    parser.add_argument("--project", default=None,
                        help="exact TinyMem project scope (default: project directory name)")
    parser.add_argument("--agent", default=None,
                        help="optional exact agent scope; omitted means shared within user/project")
    parser.add_argument("--server-name", "--tinymem-server", dest="server_name", default="tinymem",
                        help="Pi MCP server key (default: tinymem)")
    parser.add_argument("--exposure", choices=("deferred", "direct", "codemode"),
                        default="deferred",
                        help="Pi tool exposure (default: deferred to reduce prompt overhead)")
    parser.add_argument("--force", action="store_true",
                        help="replace an existing entry with the selected server name")
    parser.add_argument("--dry-run", action="store_true",
                        help="preview the target and generated server entry without writing files")
    return parser


def run_setup(args: argparse.Namespace, *, stdout: TextIO = sys.stdout,
              stderr: TextIO = sys.stderr) -> int:
    try:
        root = args.root.expanduser().resolve(strict=True)
        if not root.is_dir():
            raise SetupError(f"project root is not a directory: {root}")
        binary_input = args.binary if args.binary is not None else default_binary_path()
        binary = binary_input.expanduser().resolve()
        if binary.name.lower() not in {"tinymem", "tinymem.exe"}:
            raise SetupError("--binary must resolve to the native executable named tinymem or tinymem.exe")
        db_input = args.db.expanduser() if args.db is not None else default_database_path(root)
        if not db_input.is_absolute():
            db_input = root / db_input
        database = db_input.resolve()
        if database.exists() and database.is_dir():
            raise SetupError(f"database path is a directory, not a file: {database}")

        try:
            user_value = args.user if args.user is not None else getpass.getuser()
            project_value = (
                args.project if args.project is not None else default_project_scope(root)
            )
            user = _scope_token(user_value, "user scope", "--user")
            project = _scope_token(project_value, "project scope", "--project")
            agent = (
                _scope_token(args.agent, "agent scope", "--agent")
                if args.agent is not None else None
            )
            server_name = _server_name(args.server_name)
        except (OSError, SetupError) as error:
            if isinstance(error, SetupError):
                raise
            raise SetupError(f"cannot determine the current OS username: {error}") from error

        config_path = root / ".pi" / "mcp.json"
        if config_path.parent.is_symlink():
            raise SetupError(
                f"refusing to write through symlinked project config "
                f"directory: {config_path.parent}"
            )
        if config_path.parent.exists() and not config_path.parent.is_dir():
            raise SetupError(f"project config path is not a directory: {config_path.parent}")
        config, original, existing_mode = _read_config(config_path)
        servers = config["mcpServers"]
        name_matches = [name for name in servers if isinstance(name, str)
                        and _name_key(name) == _name_key(server_name)]
        aliases = [name for name in name_matches if name != server_name]
        if aliases:
            existing_name = aliases[0]
            raise SetupError(
                f"Pi treats server names {server_name!r} and {existing_name!r} as the same; "
                f"use --server-name {existing_name!r} to update that entry and "
                "remove any duplicate aliases"
            )

        other_native = [name for name, entry in servers.items()
                        if name != server_name and _is_native_server(entry)]
        if other_native:
            names = ", ".join(repr(name) for name in other_native)
            raise SetupError(
                f"native TinyMem server(s) already configured as {names}; adding {server_name!r} "
                "would start duplicate memory servers. Reuse the existing server name with "
                "--server-name, or remove the old entry explicitly before setup."
            )

        entry = make_server_config(binary=binary, root=root, database=database, user=user,
                                   project=project, agent=agent, exposure=args.exposure)
        has_entry = server_name in servers
        old_entry = servers.get(server_name)
        if has_entry and old_entry != entry and not args.force:
            raise SetupError(
                f"Pi MCP server {server_name!r} already exists with different settings; "
                "not overwriting it. Review the entry, then rerun with --force to replace only "
                "that server entry. Existing databases and credentials are never removed."
            )
        if has_entry and old_entry != entry and args.force and isinstance(old_entry, dict):
            credential_fields = {"env", "headers", "oauth"}.intersection(old_entry)
            if credential_fields:
                fields = ", ".join(sorted(credential_fields))
                raise SetupError(
                    f"Pi MCP server {server_name!r} contains credential configuration ({fields}); "
                    "refusing to remove or transfer it. Preserve those credentials manually before "
                    "replacing this entry."
                )
        changed = not has_entry or old_entry != entry
        if changed:
            servers[server_name] = entry
        rendered = (json.dumps(config, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
        if len(rendered) > MAX_CONFIG_BYTES:
            raise SetupError(f"merged Pi config exceeds {MAX_CONFIG_BYTES} bytes")

        action = ("Would write" if changed else "Already configured") if args.dry_run else (
            "Configured" if changed else "Already configured"
        )
        print(f"{action}: {config_path}", file=stdout)
        print(f"  server: {server_name} (exposure: {args.exposure})", file=stdout)
        print(f"  native binary: {binary}", file=stdout)
        if not binary.is_file():
            print(
                "  note: binary is not present yet; build it with "
                "`cargo build --release --locked -p tinymem-engine`",
                file=stdout,
            )
        print(f"  repository: {root}", file=stdout)
        print(f"  database: {database}", file=stdout)
        print(f"  exact scope: {_format_scope(user, project, agent)}", file=stdout)
        if args.dry_run:
            print(
                "  dry run: no configuration, database, or directory was created or changed",
                file=stdout,
            )
            return 0
        if not changed:
            return 0

        # The native process creates/opens the database later. Setup creates only
        # its parent, privately when new, and deliberately leaves an existing DB alone.
        _create_private_directory(database.parent)
        _atomic_write_config(config_path, rendered, original, existing_mode)
        print("  database contents were not opened or changed", file=stdout)
        return 0
    except (SetupError, OSError, RuntimeError) as error:
        print(f"setup-pi: {error}", file=stderr)
        return 1


def main(argv: list[str] | None = None, *, stdout: TextIO = sys.stdout,
         stderr: TextIO = sys.stderr) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return run_setup(args, stdout=stdout, stderr=stderr)


if __name__ == "__main__":
    raise SystemExit(main())
