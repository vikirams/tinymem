import json
import unittest
from pathlib import Path

from tinymem.connect import ConnectError, connect, launch, project_namespace


class ConnectTests(unittest.TestCase):
    def test_same_command_is_registered_for_every_agent(self) -> None:
        project = Path(self.id().replace(".", "_")).resolve()
        project.mkdir()
        self.addCleanup(lambda: _remove(project))
        (project / ".cursor").mkdir()
        (project / ".cursor" / "mcp.json").write_text(
            json.dumps({"mcpServers": {"other": {"command": "other"}}, "keep": True}) + "\n",
            encoding="utf-8",
        )
        (project / ".codex").mkdir()
        (project / ".codex" / "config.toml").write_text(
            'model = "keep"\n\n[mcp_servers.other]\ncommand = "other"\n\n[mcp_servers.tinymem]\ncommand = "old"\n',
            encoding="utf-8",
        )
        written = connect(project)
        self.assertEqual(
            [path.relative_to(project).as_posix() for path in written],
            [".pi/mcp.json", ".cursor/mcp.json", ".mcp.json", ".vscode/mcp.json", ".codex/config.toml"],
        )
        commands = []
        for relative, key in (
            (".pi/mcp.json", "mcpServers"),
            (".cursor/mcp.json", "mcpServers"),
            (".mcp.json", "mcpServers"),
            (".vscode/mcp.json", "servers"),
        ):
            data = json.loads((project / relative).read_text(encoding="utf-8"))
            commands.append((data[key]["tinymem"]["command"], data[key]["tinymem"]["args"]))
        self.assertEqual(commands, [commands[0]] * len(commands))
        cursor = json.loads((project / ".cursor" / "mcp.json").read_text(encoding="utf-8"))
        self.assertEqual(cursor["mcpServers"]["other"]["command"], "other")
        self.assertTrue(cursor["keep"])
        vscode = json.loads((project / ".vscode" / "mcp.json").read_text(encoding="utf-8"))
        self.assertEqual(vscode["servers"]["tinymem"]["type"], "stdio")
        pi = json.loads((project / ".pi" / "mcp.json").read_text(encoding="utf-8"))
        self.assertEqual(pi["mcpServers"]["tinymem"]["exposure"], "direct")
        codex = (project / ".codex" / "config.toml").read_text(encoding="utf-8")
        self.assertIn('model = "keep"', codex)
        self.assertIn("[mcp_servers.other]", codex)
        self.assertEqual(codex.count("[mcp_servers.tinymem]"), 1)
        self.assertIn(commands[0][0], codex)
        self.assertNotIn('command = "old"', codex)
        namespaces = []
        for relative, key in (
            (".pi/mcp.json", "mcpServers"),
            (".cursor/mcp.json", "mcpServers"),
            (".mcp.json", "mcpServers"),
            (".vscode/mcp.json", "servers"),
        ):
            data = json.loads((project / relative).read_text(encoding="utf-8"))
            namespaces.append(data[key]["tinymem"]["env"]["TINYMEM_NAMESPACE"])
        self.assertEqual(namespaces, [namespaces[0]] * len(namespaces))
        self.assertIn("TINYMEM_NAMESPACE", codex)

    def test_project_namespace_sanitizes(self) -> None:
        self.assertEqual(project_namespace(Path("/tmp/My Cool.Project!")), "My-Cool-Project")
        self.assertEqual(launch("proj-a")["env"]["TINYMEM_NAMESPACE"], "proj-a")

    def test_launch_is_stable_console_script(self) -> None:
        entry = launch("proj-a")
        self.assertEqual(entry["command"], "tinymem")
        self.assertEqual(entry["args"], ["mcp"])
        self.assertEqual(entry["env"]["TINYMEM_NAMESPACE"], "proj-a")
        self.assertIn("TINYMEM_DB", entry["env"])
        self.assertNotIn("Desktop", str(entry["command"]) + str(entry["args"]))

    def test_refuses_to_destroy_invalid_config(self) -> None:
        project = Path(self.id().replace(".", "_") + "_bad")
        project.mkdir()
        self.addCleanup(lambda: _remove(project))
        (project / ".mcp.json").write_text("{", encoding="utf-8")
        with self.assertRaises(ConnectError):
            connect(project, ["claude"])
        self.assertEqual((project / ".mcp.json").read_text(encoding="utf-8"), "{")


def _remove(path: Path) -> None:
    if path.is_file() or path.is_symlink():
        path.unlink()
        return
    for child in path.iterdir():
        _remove(child)
    path.rmdir()
