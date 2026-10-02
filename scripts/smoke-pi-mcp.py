"""Discover native TinyMem tools using Pi, without inference or user configuration.

Requires a built native binary and Pi's built-in `mcp list --json` support
(verified with Pi 0.99.2). Uses only Python's standard library. This checks MCP
interoperability, not agent memory quality or Cloudflare routing.
"""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


EXPECTED_TOOLS = {
    "add_memory", "search_memories", "index_memories", "get_memory",
    "update_memory", "delete_memory", "memory_history", "get_profile",
    "refresh_repository", "search_repository", "assemble_context",
}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/tinymem"))
    parser.add_argument("--pi", default="pi", help="Pi executable (not a shell command)")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)

    with tempfile.TemporaryDirectory(prefix="tinymem-pi-smoke-") as temporary:
        work = Path(temporary)
        agent = work / "agent"
        repository = work / "repository"
        agent.mkdir()
        repository.mkdir()
        (repository / "auth.py").write_text(
            "def authenticate_user():\n    return True\n", encoding="utf-8",
        )
        configuration = {
            "mcpServers": {
                "tinymem": {
                    "command": str(binary),
                    "args": [
                        "--db", str(work / "native.db"), "mcp", str(repository),
                        "--user", "smoke", "--project", "fixture",
                    ],
                    "exposure": "direct",
                },
            },
        }
        (agent / "mcp.json").write_text(
            json.dumps(configuration), encoding="utf-8",
        )
        # No personal/project config is read; shell-level MCP commands do not
        # load extensions. Disable Pi's automatic catalog/update/telemetry IO.
        environment = dict(
            os.environ,
            PI_CODING_AGENT_DIR=str(agent),
            PI_OFFLINE="1",
            PI_SKIP_VERSION_CHECK="1",
            PI_TELEMETRY="0",
        )
        result = subprocess.run(
            [args.pi, "mcp", "list", "--json"],
            cwd=repository, env=environment, capture_output=True, text=True,
            encoding="utf-8", timeout=30, check=True,
        )
        report = json.loads(result.stdout)
        if report.get("errors"):
            raise RuntimeError("Pi reported MCP configuration errors")
        servers = report.get("servers", [])
        if len(servers) != 1 or servers[0].get("name") != "tinymem":
            raise RuntimeError("Pi did not discover exactly the configured TinyMem server")
        server = servers[0]
        if server.get("state") != "connected":
            raise RuntimeError("Pi could not connect to native TinyMem")
        if set(server.get("tools", [])) != EXPECTED_TOOLS:
            raise RuntimeError("Pi discovered an unexpected native tool set")
        print("Pi/native MCP discovery: PASS (11 tools; no model calls)")


if __name__ == "__main__":
    main()
