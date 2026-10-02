import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tinymem.protocol import PROTOCOL_VERSIONS, Session
from tinymem.store import Store


class MemoryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.store = Store(Path(self.directory.name) / "memory.db")
        self.addCleanup(self.store.close)

    def test_search_is_literal_and_and_hides_deleted(self) -> None:
        first = self.store.add("Cookie auth uses SameSite", "decision")
        self.store.add("Unrelated deployment note")
        again = self.store.add("Cookie auth uses SameSite", "decision")
        self.assertFalse(again["created"])
        self.assertEqual(again["id"], first["id"])
        found = self.store.search("cookie samesite")
        self.assertEqual([item["id"] for item in found["memories"]], [first["id"]])
        self.assertEqual(self.store.search("cookie missing")["memories"], [])
        self.store.delete(first["id"])
        self.assertEqual(self.store.search("cookie")["memories"], [])

    def test_rejects_wide_queries_and_unknown_kinds(self) -> None:
        with self.assertRaises(ValueError):
            self.store.search("t1 t2 t3 t4 t5 t6 t7 t8 t9 t10 t11")
        # 4 terms are now allowed (limit is 10)
        self.store.add("one two three four note")
        self.assertEqual(len(self.store.search("one two three four")["memories"]), 1)
        with self.assertRaises(ValueError):
            self.store.add("note", "secret")

    def test_prefix_and_stemming_recall(self) -> None:
        record = self.store.add("User prefers TypeScript with strict mode")
        self.assertEqual(
            [item["id"] for item in self.store.search("type")["memories"]],
            [record["id"]],
        )
        self.assertEqual(
            [item["id"] for item in self.store.search("prefer typescript")["memories"]],
            [record["id"]],
        )

    def test_namespace_isolation(self) -> None:
        first = self.store.add("Deploy with blue green", namespace="proj-a")
        second = self.store.add("Deploy with blue green", namespace="proj-b")
        self.assertNotEqual(first["id"], second["id"])
        self.assertEqual(
            [item["id"] for item in self.store.search("deploy", namespace="proj-a")["memories"]],
            [first["id"]],
        )
        self.assertEqual(len(self.store.search("deploy")["memories"]), 2)
        with self.assertRaises(ValueError):
            self.store.add("note", namespace="bad ns!")

    def test_update_bumps_timestamp_and_search(self) -> None:
        record = self.store.add("Cookie auth uses SameSite", "decision")
        updated = self.store.update(record["id"], content="Cookie auth uses Strict SameSite")
        self.assertTrue(updated["updated"])
        self.assertEqual(updated["id"], record["id"])
        self.assertGreaterEqual(updated["updated_at"], record["created_at"])
        self.assertEqual(
            [item["id"] for item in self.store.search("strict samesite")["memories"]],
            [record["id"]],
        )
        noop = self.store.update(record["id"], content="Cookie auth uses Strict SameSite")
        self.assertFalse(noop["updated"])
        with self.assertRaises(ValueError):
            self.store.update("0" * 32, content="missing")

    def test_valid_until_expiry_hides_from_search(self) -> None:
        record = self.store.add("Exam tomorrow at noon", valid_until="2000-01-02")
        self.assertEqual(self.store.search("exam")["memories"], [])
        future = self.store.add("Launch next year", valid_until="2099-01-01")
        self.assertEqual(
            [item["id"] for item in self.store.search("launch")["memories"]],
            [future["id"]],
        )
        cleared = self.store.update(record["id"], valid_until=None)
        self.assertIsNone(cleared["valid_until"])
        self.assertEqual(
            [item["id"] for item in self.store.search("exam")["memories"]],
            [record["id"]],
        )
        with self.assertRaises(ValueError):
            self.store.add("note", valid_until="not-a-date")

    def test_fuzzy_dedup_collapses_case_and_punctuation(self) -> None:
        first = self.store.add("Cookie auth uses SameSite", "decision")
        again = self.store.add("cookie auth uses samesite!!", "decision")
        self.assertFalse(again["created"])
        self.assertEqual(again["id"], first["id"])
        # Different kind is a different memory.
        other = self.store.add("cookie auth uses samesite", "fact")
        self.assertTrue(other["created"])
        self.assertNotEqual(other["id"], first["id"])

    def test_update_refuses_to_create_duplicate(self) -> None:
        first = self.store.add("Alpha beta gamma")
        second = self.store.add("Delta epsilon zeta")
        with self.assertRaises(ValueError):
            self.store.update(second["id"], content="Alpha, beta, gamma!")
        # Updating expiry alone is fine.
        updated = self.store.update(second["id"], valid_until="2099-01-01")
        self.assertTrue(updated["updated"])

    def test_profiles_round_trip_and_clear(self) -> None:
        empty = self.store.get_profile("proj-a")
        self.assertEqual(empty["profile"], "")
        self.assertIsNone(empty["updated_at"])
        saved = self.store.set_profile("proj-a", "Stack: python + sqlite. Use FTS first.")
        self.assertFalse(saved["cleared"])
        fetched = self.store.get_profile("proj-a")
        self.assertEqual(fetched["profile"], "Stack: python + sqlite. Use FTS first.")
        cleared = self.store.set_profile("proj-a", "   ")
        self.assertTrue(cleared["cleared"])
        self.assertEqual(self.store.get_profile("proj-a")["profile"], "")
        with self.assertRaises(ValueError):
            self.store.set_profile("proj-a", "x" * 4001)

    def test_stats_and_purge(self) -> None:
        self.store.add("Keep me", namespace="proj-a")
        old = self.store.add("Gone long ago", namespace="proj-a")
        self.store.delete(old["id"])
        stats = self.store.stats()
        self.assertEqual(stats["active"], 1)
        self.assertEqual(stats["deleted"], 1)
        # Backdate the soft-delete past the grace window, then purge.
        self.store._conn.execute(
            "UPDATE memories SET deleted_at = ? WHERE id = ?",
            ("2000-01-01T00:00:00Z", old["id"]),
        )
        self.store._conn.commit()
        result = self.store.purge(grace_days=30)
        self.assertEqual(result["purged"], 1)
        stats = self.store.stats()
        self.assertEqual(stats["active"], 1)
        self.assertEqual(stats["deleted"], 0)
        with self.assertRaises(ValueError):
            self.store.purge(grace_days=0)


class ProtocolTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.store = Store(Path(self.directory.name) / "memory.db")
        self.session = Session(self.store)
        self.addCleanup(self.store.close)

    def test_tools_require_initialize_and_echo_supported_versions(self) -> None:
        denied = self.session.handle({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})
        self.assertEqual(denied["error"]["code"], -32600)
        for version in PROTOCOL_VERSIONS:
            response = self.session.handle({"jsonrpc": "2.0", "id": version, "method": "initialize", "params": {"protocolVersion": version}})
            self.assertEqual(response["result"]["protocolVersion"], version)
        negotiated = self.session.handle({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {"protocolVersion": "1999-01-01"}})
        self.assertEqual(negotiated["result"]["protocolVersion"], "2025-06-18")

    def test_call_round_trip_and_unknown_tool(self) -> None:
        self.session.handle({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}})
        self.session.handle({"jsonrpc": "2.0", "method": "notifications/initialized"})
        added = self.session.handle({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "add_memory", "arguments": {"content": "Use MCP for every agent"}}})
        self.assertFalse(added["result"]["isError"])
        self.assertIn("structuredContent", added["result"])
        memory_id = added["result"]["structuredContent"]["id"]
        found = self.session.handle({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "search_memories", "arguments": {"query": "MCP agent"}}})
        self.assertEqual(found["result"]["structuredContent"]["memories"][0]["id"], memory_id)
        unknown = self.session.handle({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "nope"}})
        self.assertEqual(unknown["error"]["code"], -32602)

    def test_update_round_trip(self) -> None:
        self.session.handle({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}})
        old = os.environ.get("TINYMEM_NAMESPACE")
        os.environ["TINYMEM_NAMESPACE"] = "proj-a"
        try:
            added = self.session.handle({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "add_memory", "arguments": {"content": "old note", "namespace": "proj-a"}}})
            memory_id = added["result"]["structuredContent"]["id"]
            updated = self.session.handle({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "update_memory", "arguments": {"id": memory_id, "content": "new note"}}})
            self.assertFalse(updated["result"]["isError"])
            self.assertEqual(updated["result"]["structuredContent"]["updated"], True)
            empty = self.session.handle({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "update_memory", "arguments": {"id": memory_id}}})
            self.assertTrue(empty["result"]["isError"])
        finally:
            if old is None:
                del os.environ["TINYMEM_NAMESPACE"]
            else:
                os.environ["TINYMEM_NAMESPACE"] = old

    def test_profile_and_stats_tools(self) -> None:
        self.session.handle({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}})
        saved = self.session.handle({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "set_profile", "arguments": {"namespace": "proj-a", "profile": "Stack: python."}}})
        self.assertFalse(saved["result"]["isError"])
        fetched = self.session.handle({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "get_profile", "arguments": {"namespace": "proj-a"}}})
        self.assertEqual(fetched["result"]["structuredContent"]["profile"], "Stack: python.")
        stats = self.session.handle({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "memory_stats", "arguments": {}}})
        self.assertFalse(stats["result"]["isError"])
        self.assertIn("active", stats["result"]["structuredContent"])

    def test_add_defaults_to_env_namespace(self) -> None:
        self.session.handle({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}})
        old = os.environ.get("TINYMEM_NAMESPACE")
        os.environ["TINYMEM_NAMESPACE"] = "env-proj"
        try:
            added = self.session.handle({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "add_memory", "arguments": {"content": "env scoped note"}}})
        finally:
            if old is None:
                del os.environ["TINYMEM_NAMESPACE"]
            else:
                os.environ["TINYMEM_NAMESPACE"] = old
        self.assertFalse(added["result"]["isError"])
        self.assertEqual(added["result"]["structuredContent"]["namespace"], "env-proj")

    def test_search_defaults_to_connected_namespace(self) -> None:
        first = self.store.add("Deploy with blue green", namespace="proj-a")
        other = self.store.add("Deploy with blue green", namespace="proj-b")
        self.session.handle({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}})
        old = os.environ.get("TINYMEM_NAMESPACE")
        os.environ["TINYMEM_NAMESPACE"] = "proj-a"
        try:
            found = self.session.handle({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "search_memories", "arguments": {"query": "deploy"}}})
            cross = self.session.handle({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "search_memories", "arguments": {"query": "deploy", "namespace": "proj-b"}}})
        finally:
            if old is None:
                del os.environ["TINYMEM_NAMESPACE"]
            else:
                os.environ["TINYMEM_NAMESPACE"] = old
        self.assertFalse(found["result"]["isError"])
        self.assertEqual(
            [item["id"] for item in found["result"]["structuredContent"]["memories"]],
            [first["id"]],
        )
        self.assertEqual(
            [item["id"] for item in cross["result"]["structuredContent"]["memories"]],
            [other["id"]],
        )
        self.assertEqual(found["result"]["structuredContent"]["namespace"], "proj-a")

    def test_cross_namespace_update_delete_refused(self) -> None:
        first = self.store.add("Secret deploy note", namespace="proj-a")
        self.store.add("Secret deploy note", namespace="proj-b")
        self.session.handle({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}})
        old = os.environ.get("TINYMEM_NAMESPACE")
        os.environ["TINYMEM_NAMESPACE"] = "proj-b"
        try:
            updated = self.session.handle({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "update_memory", "arguments": {"id": first["id"], "content": "hijacked"}}})
            deleted = self.session.handle({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "delete_memory", "arguments": {"id": first["id"]}}})
        finally:
            if old is None:
                del os.environ["TINYMEM_NAMESPACE"]
            else:
                os.environ["TINYMEM_NAMESPACE"] = old
        self.assertTrue(updated["result"]["isError"])
        self.assertTrue(deleted["result"]["isError"])
        # Original row is untouched and still searchable in its own namespace.
        self.assertEqual(
            [item["id"] for item in self.store.search("secret deploy", namespace="proj-a")["memories"]],
            [first["id"]],
        )

    def test_hide_removes_from_fts_atomically(self) -> None:
        record = self.store.add("Cookie auth uses SameSite", namespace="proj-a")
        self.assertEqual(
            [item["id"] for item in self.store.search("cookie", namespace="proj-a")["memories"]],
            [record["id"]],
        )
        self.store.delete(record["id"], namespace="proj-a")
        self.assertEqual(self.store.search("cookie", namespace="proj-a")["memories"], [])
        fts_count = self.store._conn.execute("SELECT COUNT(*) FROM memories_fts").fetchone()[0]
        active = self.store._conn.execute("SELECT COUNT(*) FROM memories WHERE deleted_at IS NULL").fetchone()[0]
        self.assertEqual(fts_count, active)

    def test_startup_rebuild_repairs_stale_fts(self) -> None:
        record = self.store.add("Rebuild me please", namespace="proj-a")
        # Simulate a crash that left the FTS row behind after hide.
        self.store._conn.execute("UPDATE memories SET deleted_at = ? WHERE id = ?", ("2026-01-01T00:00:00Z", record["id"]))
        self.store._conn.commit()
        self.store.close()
        reopened = Store(Path(self.directory.name) / "memory.db")
        self.addCleanup(reopened.close)
        self.assertEqual(reopened.search("rebuild", namespace="proj-a")["memories"], [])

    def test_concurrent_adds_do_not_lose_rows(self) -> None:
        db_path = str(Path(self.directory.name) / "memory.db")
        first_script = "from pathlib import Path; from tinymem.store import Store; s=Store(Path(%r)); s.add('Concurrent alpha note', namespace='proj-a'); s.close()" % db_path
        second_script = "from pathlib import Path; from tinymem.store import Store; s=Store(Path(%r)); s.add('Concurrent beta note', namespace='proj-a'); s.close()" % db_path
        first = subprocess.run([sys.executable, "-c", first_script], capture_output=True, text=True, timeout=10)
        second = subprocess.run([sys.executable, "-c", second_script], capture_output=True, text=True, timeout=10)
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual(second.returncode, 0, second.stderr)
        found = self.store.search("concurrent", namespace="proj-a")
        self.assertEqual(len(found["memories"]), 2)

    def test_simultaneous_cold_start_open(self) -> None:
        db_path = str(Path(self.directory.name) / "cold.db")
        script = (
            "from pathlib import Path; from tinymem.store import Store;"
            " s=Store(Path(%r)); s.add('Cold start note', namespace='proj-a'); s.close()"
        ) % db_path
        procs = [
            subprocess.Popen([sys.executable, "-c", script], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            for _ in range(6)
        ]
        for proc in procs:
            _, stderr = proc.communicate(timeout=15)
            self.assertEqual(proc.returncode, 0, stderr)

    def test_older_protocol_omits_structured_content(self) -> None:
        self.session.handle({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05"}})
        added = self.session.handle({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "add_memory", "arguments": {"content": "legacy client"}}})
        self.assertNotIn("structuredContent", added["result"])
        self.assertIn("legacy client", added["result"]["content"][0]["text"])


class StdioTests(unittest.TestCase):
    def test_agent_launch_speaks_newline_jsonrpc(self) -> None:
        root = Path(__file__).resolve().parents[1]
        script = root / "tinymem" / "__main__.py"
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        db = Path(directory.name) / "memory.db"
        messages = [
            {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26", "clientInfo": {"name": "cursor", "version": "0"}}},
            {"jsonrpc": "2.0", "method": "notifications/initialized"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
            {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "add_memory", "arguments": {"content": "shared across agents"}}},
        ]
        completed = subprocess.run(
            [sys.executable, str(script)],
            input="".join(json.dumps(message) + "\n" for message in messages),
            text=True,
            capture_output=True,
            cwd=root,
            env={**os.environ, "TINYMEM_DB": str(db)},
            timeout=10,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(completed.stderr, "")
        responses = [json.loads(line) for line in completed.stdout.splitlines()]
        self.assertEqual([item["id"] for item in responses], [1, 2, 3])
        self.assertEqual(responses[0]["result"]["serverInfo"]["name"], "tinymem")
        self.assertEqual([tool["name"] for tool in responses[1]["result"]["tools"]], ["add_memory", "search_memories", "update_memory", "get_profile", "set_profile", "memory_stats", "delete_memory"])
        self.assertFalse(responses[2]["result"]["isError"])
