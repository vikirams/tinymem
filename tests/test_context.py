import json
import os
import tempfile
import unittest
from pathlib import Path

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
        command = claude["hooks"]["SessionStart"][0]["hooks"][0]["command"]
        self.assertEqual(command, "tinymem context --format claude")
        cursor = json.loads((project / ".cursor" / "hooks.json").read_text(encoding="utf-8"))
        self.assertEqual(cursor["hooks"]["sessionStart"][0]["command"], "tinymem context --format cursor")
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
