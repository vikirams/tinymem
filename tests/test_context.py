import json
import os
import shlex
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tinymem.cli import main
from tinymem.connect import connect
from tinymem.context import assemble, emit, index_line
from tinymem.store import INDEX_CAP, Store


class RecentIndexTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.store = Store(Path(self.directory.name) / "memory.db")
        self.addCleanup(self.store.close)

    def test_recent_is_recency_capped_and_namespaced(self) -> None:
        old = self.store.add("Oldest fact in alpha", namespace="alpha")
        self.store.add("Noise in beta", namespace="beta")
        newer = self.store.add("Newer fact in alpha", namespace="alpha")
        rows = self.store.recent("alpha")
        self.assertEqual([row["id"] for row in rows], [newer["id"], old["id"]])
        self.store.delete(newer["id"], namespace="alpha")
        self.assertEqual([row["id"] for row in self.store.recent("alpha")], [old["id"]])
        expired = self.store.add("Gone", namespace="alpha", valid_until="2000-01-02")
        ids = [row["id"] for row in self.store.recent("alpha")]
        self.assertNotIn(expired["id"], ids)

    def test_index_line_is_one_line(self) -> None:
        self.assertEqual(index_line("first\nsecond"), "first")
        self.assertTrue(index_line("x" * 200).endswith("…"))
        self.assertLessEqual(len(index_line("x" * 200)), 120)

    def test_assemble_context_includes_profile_and_index(self) -> None:
        self.store.set_profile("alpha", "Python stdlib. store.py is the Store.")
        record = self.store.add("Cookie auth uses SameSite", "decision", namespace="alpha")
        text = assemble(self.store, "alpha")
        self.assertIn("Python stdlib", text)
        self.assertIn(record["id"], text)
        self.assertIn("decision:", text)
        self.assertIn("search_memories", text)
        self.assertNotIn("\n\n\n", text)

    def test_recent_cap_is_five(self) -> None:
        for i in range(INDEX_CAP + 3):
            self.store.add(f"Fact number {i} about alpha", namespace="alpha")
        self.assertEqual(len(self.store.recent("alpha")), INDEX_CAP)


class ContextCliTests(unittest.TestCase):
    def test_context_fail_open(self) -> None:
        previous = os.environ.get("TINYMEM_DB")
        os.environ["TINYMEM_DB"] = "/dev/null/tinymem.db"
        try:
            self.assertEqual(main(["context"]), 0)
        finally:
            if previous is None:
                os.environ.pop("TINYMEM_DB", None)
            else:
                os.environ["TINYMEM_DB"] = previous

    def test_emit_claude_is_json(self) -> None:
        payload = json.loads(emit("missing-ns-xyz", "claude"))
        self.assertIn("hookSpecificOutput", payload)


class ConnectHookTests(unittest.TestCase):
    def test_generated_hooks_use_the_mcp_database_and_namespace(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            project = root / "release-project"
            project.mkdir()
            db = root / "data with 'quotes' $dollar ; spaces" / "memory.db"
            store = Store(db)
            try:
                store.set_profile("release-project", "RIGHT_PROJECT_PROFILE")
                store.add("RIGHT_PROJECT_INDEX", namespace="release-project")
                store.set_profile("default", "WRONG_DEFAULT_PROFILE")
                store.set_profile("other-project", "WRONG_OTHER_PROFILE")
            finally:
                store.close()
            with patch.dict(os.environ, {"TINYMEM_DB": str(db)}):
                connect(project, ["claude", "cursor"], hooks=True)
            other_db = root / "other.db"
            other = Store(other_db)
            try:
                other.set_profile("release-project", "WRONG_DATABASE_PROFILE")
            finally:
                other.close()
            # Exercise the real CLI without requiring a globally installed script.
            bin_dir = root / "bin"
            bin_dir.mkdir()
            script = bin_dir / "tinymem"
            script.write_text(
                f'#!/bin/sh\nexec {shlex.quote(sys.executable)} -m tinymem "$@"\n',
                encoding="utf-8",
            )
            script.chmod(0o755)
            env = dict(os.environ)
            env["PATH"] = str(bin_dir) + os.pathsep + env.get("PATH", "")
            env["PYTHONPATH"] = str(Path(__file__).resolve().parents[1])
            # Host hooks do not inherit the MCP subprocess's environment.
            for inherited in (
                {"HOME": str(root / "host-home")},
                {"TINYMEM_DB": str(db)},
                {"TINYMEM_DB": str(other_db), "TINYMEM_NAMESPACE": "other-project"},
            ):
                host_env = {
                    key: value for key, value in env.items()
                    if key not in ("TINYMEM_DB", "TINYMEM_NAMESPACE")
                }
                host_env.update(inherited)
                for agent in ("claude", "cursor"):
                    with self.subTest(agent=agent, inherited=inherited):
                        if agent == "claude":
                            config = json.loads((project / ".claude" / "settings.json").read_text(encoding="utf-8"))
                            command = config["hooks"]["SessionStart"][0]["hooks"][0]["command"]
                        else:
                            config = json.loads((project / ".cursor" / "hooks.json").read_text(encoding="utf-8"))
                            command = config["hooks"]["sessionStart"][0]["command"]
                        result = subprocess.run(
                            command, shell=True, cwd=project, env=host_env,
                            capture_output=True, text=True, timeout=10, check=True,
                        )
                        payload = json.loads(result.stdout)
                        text = (
                            payload["hookSpecificOutput"]["additionalContext"]
                            if agent == "claude" else payload["additionalContext"]
                        )
                        self.assertIn("RIGHT_PROJECT_PROFILE", text)
                        self.assertIn("RIGHT_PROJECT_INDEX", text)
                        self.assertNotIn("WRONG_", text)

    def test_reconnect_replaces_legacy_and_stale_hooks_without_touching_other_hooks(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            project = root / "release-project"
            (project / ".claude").mkdir(parents=True)
            (project / ".cursor").mkdir()
            keep = {"type": "command", "command": "echo keep", "timeout": 7}
            legacy = {"type": "command", "command": "tinymem context --format claude"}
            claude_path = project / ".claude" / "settings.json"
            claude_path.write_text(json.dumps({
                "keep": True,
                "hooks": {
                    "SessionStart": [{"matcher": "startup", "hooks": [keep, legacy, legacy]}],
                    "OtherEvent": [],
                },
            }), encoding="utf-8")
            cursor_path = project / ".cursor" / "hooks.json"
            cursor_path.write_text(json.dumps({
                "version": 1,
                "hooks": {
                    "sessionStart": [
                        {"command": "echo keep"},
                        {"command": "tinymem context --format cursor"},
                        {"command": "tinymem context --format cursor"},
                    ],
                    "otherEvent": [],
                },
            }), encoding="utf-8")
            for db in (root / "first.db", root / "second.db"):
                with self.subTest(db=db), patch.dict(os.environ, {"TINYMEM_DB": str(db)}):
                    connect(project, ["claude", "cursor"], hooks=True)
                    # A repeat must neither duplicate hooks nor retain stale bindings.
                    connect(project, ["claude", "cursor"], hooks=True)
                    claude = json.loads(claude_path.read_text(encoding="utf-8"))
                    self.assertTrue(claude["keep"])
                    self.assertEqual(claude["hooks"]["OtherEvent"], [])
                    inner = [hook for group in claude["hooks"]["SessionStart"] for hook in group["hooks"]]
                    self.assertIn(keep, inner)
                    cursor = json.loads(cursor_path.read_text(encoding="utf-8"))
                    self.assertEqual(cursor["hooks"]["otherEvent"], [])
                    self.assertIn({"command": "echo keep"}, cursor["hooks"]["sessionStart"])
                    for hooks in (inner, cursor["hooks"]["sessionStart"]):
                        commands = [item["command"] for item in hooks if item["command"] != "echo keep"]
                        self.assertEqual(len(commands), 1)
                        arguments = shlex.split(commands[0])
                        self.assertIn(f"TINYMEM_DB={db.resolve()}", arguments)
                        self.assertIn("TINYMEM_NAMESPACE=release-project", arguments)

    def test_hooks_and_discipline_are_opt_in(self) -> None:
        project = Path(self.id().replace(".", "_")).resolve()
        project.mkdir()
        self.addCleanup(lambda: _remove(project))
        written = connect(project, ["claude", "cursor"], hooks=True, discipline=True)
        rel = [path.relative_to(project).as_posix() for path in written]
        self.assertIn(".claude/settings.json", rel)
        self.assertIn(".cursor/hooks.json", rel)
        self.assertIn("skills/tinymem/SKILL.md", rel)
        self.assertIn("AGENTS.md", rel)
        claude = json.loads((project / ".claude" / "settings.json").read_text(encoding="utf-8"))
        self.assertIn("SessionStart", claude["hooks"])
        cursor = json.loads((project / ".cursor" / "hooks.json").read_text(encoding="utf-8"))
        self.assertIn("sessionStart", cursor["hooks"])
        skill = (project / "skills" / "tinymem" / "SKILL.md").read_text(encoding="utf-8")
        self.assertIn("add_memory", skill)
        connect(project, ["claude"], hooks=True)
        claude2 = json.loads((project / ".claude" / "settings.json").read_text(encoding="utf-8"))
        self.assertEqual(len(claude2["hooks"]["SessionStart"]), 1)


def _remove(path: Path) -> None:
    if path.is_file() or path.is_symlink():
        path.unlink()
        return
    for child in path.iterdir():
        _remove(child)
    path.rmdir()
