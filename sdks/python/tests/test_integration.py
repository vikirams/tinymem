"""Actual native MCP/SQLite behavior through the SDK public interface only."""
import json
import os
from pathlib import Path
import tempfile
import time
import unittest

from tinymem_native import Client, MemoryEvent, NativeError, ToolError, TransportError

BINARY = os.environ.get("TINYMEM_BINARY", str(Path(__file__).resolve().parents[3] / "target/debug/tinymem"))


class NativeIntegrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Probe through the bounded adapter, not an unbounded --help capture.
        with tempfile.TemporaryDirectory(prefix="tinymem probe ") as directory:
            try:
                Client(binary=BINARY, user_id="probe", repository=directory,
                       db=Path(directory) / "probe.db", timeout=5).get_profile()
            except TransportError as error:
                unavailable = "cannot start tinymem" in str(error) or "unrecognized subcommand 'mcp'" in error.stderr
                if not unavailable or "TINYMEM_BINARY" in os.environ or os.environ.get("TINYMEM_REQUIRE_INTEGRATION") == "1":
                    raise
                raise unittest.SkipTest(f"PENDING: native mcp not available at {BINARY}: {error}") from error

    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="tinymem sdk real ")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name) / "repository"
        self.root.mkdir()
        self.options = {"binary": BINARY, "db": Path(directory.name) / "memory.db", "repository": self.root,
                        "user_id": "alice", "agent_id": "assistant", "project_id": "project"}
        self.client = Client(**self.options)

    def test_file_provenance_freshness_and_immutable_history(self):
        path = self.root / "clock.txt"
        path.write_text("UTC", encoding="utf-8")
        refs = [{"kind": "file", "role": "dependency", "path": "clock.txt"}]
        saved = self.client.add_memory("clock correction", source="user correction", source_refs=refs)
        self.assertEqual(saved["source_refs"][0]["path"], "clock.txt")
        self.assertEqual(saved["source_refs"][0]["role"], "dependency")
        self.assertTrue(saved["source_refs"][0]["content_hash"].startswith("blake3:"))
        self.assertEqual(self.client.get_memory(saved["id"]), saved)
        self.assertEqual(self.client.search_memories("correction"), [saved])
        self.assertEqual(self.client.memory_history(saved["id"])[0]["freshness"]["status"], "current")
        self.assertEqual(self.client.add_memory("clock correction", source="user correction", source_refs=refs)["id"], saved["id"])
        origin = self.client.add_memory("clock origin", source_refs=[{"kind": "file", "role": "origin", "path": "clock.txt"}])
        context = self.client.assemble_context("correction", 8000)
        self.assertEqual(context["entries"][0]["provenance"][0]["source_refs"], saved["source_refs"])
        other = Client(**(self.options | {"project_id": None}))
        self.assertIsNone(other.get_memory(saved["id"]))
        self.assertEqual(other.memory_history(saved["id"]), [])
        path.write_text("monotonic", encoding="utf-8")
        for reason in ["content_changed", "file_missing", "source_unavailable"]:
            if reason == "file_missing":
                path.unlink()
            elif reason == "source_unavailable":
                path.mkdir()
            self.assertIsNone(self.client.get_memory(saved["id"]))
            self.assertEqual(self.client.search_memories("correction"), [])
            self.assertEqual(self.client.get_profile()["stable"], [origin])
            self.assertEqual(self.client.assemble_context("correction", 8000)["entries"], [])
            event = self.client.memory_history(saved["id"])[0]
            self.assertEqual(event["memory"], saved)
            self.assertEqual(event["freshness"]["reason"], reason)
            self.assertEqual(event["freshness"]["checks"][0]["expected_hash"], saved["source_refs"][0]["content_hash"])
            self.assertEqual(event["freshness"]["status"], "unknown" if reason == "source_unavailable" else "stale")
        self.assertEqual(self.client.get_memory(origin["id"]), origin)
        self.assertTrue(self.client.delete_memory(saved["id"], expected_version=1))
        self.assertEqual([event["event"] for event in self.client.memory_history(saved["id"])], ["added", "deleted"])

    def test_crud_dedup_restart_history_conflicts_and_deletion(self):
        memory = self.client.add_memory("Saffron tea is preferred", kind="preference", source="sdk test")
        self.assertIsNotNone(memory)
        id = memory["id"]
        self.assertEqual(memory["scope"], {"user_id": "alice", "agent_id": "assistant", "project_id": "project"})
        self.assertEqual(memory["version"], 1)
        duplicate = self.client.add_memory("Saffron tea is preferred", kind="preference", source="sdk test")
        self.assertEqual(duplicate["id"], id)
        self.assertEqual(self.client.get_memory(id), memory)
        self.assertIn(id, {record["id"] for record in self.client.search_memories("Saffron")})
        updated = self.client.update_memory(id, "Saffron coffee is preferred", expected_version=1)
        self.assertEqual(updated["id"], id)
        self.assertEqual(updated["version"], 2)
        self.assertEqual(updated["kind"], "preference")
        self.assertEqual(updated["source"], "sdk test")
        with self.assertRaises(ToolError):
            self.client.update_memory(id, "stale write must fail", expected_version=1)
        with self.assertRaises(ToolError):
            self.client.delete_memory(id, expected_version=1)
        restarted = Client(**self.options)  # Every preceding call already exited/reaped its server.
        self.assertEqual(restarted.get_memory(id), updated)
        history: list[MemoryEvent] = restarted.memory_history(id, limit=100, max_bytes=64000)
        self.assertEqual([event["event"] for event in history], ["added", "updated"])
        self.assertEqual([event["version"] for event in history], [1, 2])
        self.assertEqual([event["memory"]["id"] for event in history], [id, id])
        self.assertEqual([event["memory"]["content"] for event in history],
                         ["Saffron tea is preferred", "Saffron coffee is preferred"])
        self.assertTrue(all(event["at_ms"] > 0 for event in history))
        self.assertTrue(restarted.delete_memory(id, expected_version=2))
        self.assertIsNone(restarted.get_memory(id))
        self.assertEqual(restarted.search_memories("Saffron"), [])
        self.assertFalse(restarted.delete_memory(id))
        deleted_history = restarted.memory_history(id, limit=100, max_bytes=64000)
        self.assertEqual([event["event"] for event in deleted_history], ["added", "updated", "deleted"])
        self.assertEqual([event["version"] for event in deleted_history], [1, 2, 3])
        self.assertEqual(deleted_history[:2], history)
        self.assertEqual(deleted_history[2]["memory"]["id"], id)
        self.assertEqual(deleted_history[2]["memory"]["content"], "Saffron coffee is preferred")
        with self.assertRaises(ToolError):
            restarted.update_memory(id, "must not resurrect a deleted record")

    def test_exact_scope_isolation_and_scope_local_duplicate_content(self):
        original = self.client.add_memory("Juniper common content", source="explicit")
        for change in [{"user_id": "bob"}, {"agent_id": "other"}, {"project_id": "other"},
                       {"agent_id": None}, {"project_id": None}, {"agent_id": None, "project_id": None}]:
            with self.subTest(change=change):
                other = Client(**(self.options | change))
                self.assertIsNone(other.get_memory(original["id"]))
                self.assertEqual(other.search_memories("Juniper"), [])
                self.assertEqual(other.memory_history(original["id"]), [])
                self.assertFalse(other.delete_memory(original["id"]))
                profile = other.get_profile()
                self.assertEqual(profile["stable"] + profile["dynamic"], [])
                with self.assertRaises(ToolError):
                    other.update_memory(original["id"], "foreign scope must not write")
                duplicate = other.add_memory("Juniper common content", source="explicit")
                self.assertNotEqual(duplicate["id"], original["id"])
                self.assertIsNone(self.client.get_memory(duplicate["id"]))
        self.assertEqual(self.client.get_memory(original["id"]), original)

    def test_profiles_expiration_and_reads_do_not_add_or_update_memory(self):
        stable = self.client.add_memory("Prefer tea", kind="preference")
        dynamic = self.client.add_memory("Plan a release", kind="plan")
        expired = self.client.add_memory("Expired saffron fact", expires_at_ms=int(time.time() * 1000) - 1000)
        history = self.client.memory_history(stable["id"], limit=100, max_bytes=64000)
        self.client.get_memory(stable["id"])
        self.client.search_memories("tea", limit=1, max_bytes=16000)
        profile = self.client.get_profile(limit=100, max_bytes=64000)
        self.assertIn(stable["id"], {record["id"] for record in profile["stable"]})
        self.assertIn(dynamic["id"], {record["id"] for record in profile["dynamic"]})
        self.assertNotIn(expired["id"], {record["id"] for record in profile["stable"] + profile["dynamic"]})
        self.assertEqual(self.client.search_memories("Expired saffron"), [])
        self.assertTrue(self.client.memory_history(expired["id"]))
        self.client.refresh_repository()
        self.client.search_repository("no_source_match")
        self.assertEqual(self.client.memory_history(stable["id"], limit=100, max_bytes=64000), history)
        self.assertEqual(self.client.get_memory(stable["id"]), stable)
        matches = self.client.search_memories("tea", limit=1, max_bytes=16000)
        self.assertLessEqual(len(matches), 1)
        self.assertLessEqual(len(json.dumps(matches, ensure_ascii=False, separators=(",", ":")).encode()), 16000)
        self.assertLessEqual(len(self.client.memory_history(stable["id"], limit=1, max_bytes=16000)), 1)

    def test_repository_refresh_tracks_edits_deletions_without_memory_writes(self):
        source = self.root / "login.py"
        source.write_text("def obsolete_login():\n    pass\n", encoding="utf-8")
        self.assertEqual(self.client.refresh_repository()["indexed"], 1)
        self.assertEqual(self.client.search_repository("obsolete_login")["hits"][0]["path"], "login.py")
        source.write_text("def current_login():\n    return True\n", encoding="utf-8")
        edited = self.client.search_repository("current_login")
        self.assertEqual(edited["refresh"]["indexed"], 1)
        self.assertIn("current_login", edited["hits"][0]["content"])
        self.assertEqual(self.client.search_repository("obsolete_login")["hits"], [])
        source.unlink()
        deleted = self.client.search_repository("current_login")
        self.assertEqual(deleted["hits"], [])
        self.assertEqual(deleted["refresh"]["removed"], 1)
        profile = self.client.get_profile()
        self.assertEqual(profile["stable"] + profile["dynamic"], [])

    def test_invalid_memory_input_and_native_startup_errors_propagate(self):
        with self.assertRaises(ToolError):
            self.client.add_memory("")
        with self.assertRaises(NativeError):
            Client(**(self.options | {"repository": self.root / "missing"})).get_profile()
        self.assertIsNotNone(self.client.add_memory("Still usable after failed operations"))

    def test_opt_in_semantic_backfill_hybrid_and_invalidation(self):
        fixture = Path(__file__).resolve().parents[3] / "scripts/fixtures/embed.py"
        fixture.chmod(0o755)
        original = self.client.add_memory("morning drink")
        options = self.options | {"embedding_command": fixture, "embedding_model": "fixture-v1",
                                  "embedding_dimensions": 3}
        semantic = Client(**options)
        self.assertEqual(semantic.index_memories(limit=1)["indexed"], 1)
        self.assertEqual(semantic.search_memories("hot beverage", mode="semantic"), [original])
        assembled = semantic.assemble_context("hot beverage", 4000)
        self.assertEqual(assembled["entries"][0]["content"], "morning drink")
        self.assertEqual(assembled["entries"][0]["provenance"][0]["type"], "memory")
        self.assertEqual(len(semantic.memory_history(original["id"])), 1)
        source = self.root / "drink.txt"
        source.write_text("espresso", encoding="utf-8")
        exact = semantic.add_memory("espresso", source_refs=[{"kind": "file", "role": "dependency", "path": "drink.txt"}])
        keyword = semantic.add_memory("compiler")
        hybrid = semantic.search_memories("espresso compiler", mode="hybrid", min_similarity=0.5)
        self.assertEqual(hybrid[0], exact)
        self.assertEqual({record["id"] for record in hybrid}, {original["id"], exact["id"], keyword["id"]})
        self.assertEqual(Client(**options).search_memories("hot beverage", mode="semantic"),
                         semantic.search_memories("hot beverage", mode="semantic"))
        other = Client(**(options | {"user_id": "bob"}))
        self.assertEqual(other.search_memories("hot beverage", mode="semantic"), [])
        self.client.update_memory(original["id"], "compiler updated", expected_version=1)
        self.assertNotIn(original["id"], {record["id"] for record in semantic.search_memories("hot beverage", mode="semantic")})
        self.assertEqual(semantic.index_memories()["indexed"], 1)
        self.assertEqual(len(semantic.memory_history(exact["id"])), 1)
        with self.assertRaises(ToolError):
            self.client.search_memories("espresso", mode="hybrid")
        source.write_text("different recommendation", encoding="utf-8")
        for mode in ["semantic", "hybrid"]:
            self.assertNotIn(exact["id"], {record["id"] for record in semantic.search_memories("espresso", mode=mode)})
        self.assertNotIn("espresso", semantic.assemble_context("hot beverage", 8000)["prompt"])
        self.assertEqual(semantic.memory_history(exact["id"])[0]["freshness"]["reason"], "content_changed")

    def test_context_assembly_budget_dedup_provenance_refresh_and_scope(self):
        source = self.root / "auth.py"
        source.write_text("auth = True\n", encoding="utf-8")
        memory = self.client.add_memory("Prefer auth tests", kind="preference", source="user")
        self.client.add_memory("auth uses cookies", kind="decision", source="ADR-1")
        self.client.add_memory("auth uses cookies", kind="decision", source="review")
        self.client.add_memory("auth expired", expires_at_ms=0)
        history = self.client.memory_history(memory["id"])
        result = self.client.assemble_context("Please update the auth flow", 8000)
        self.assertEqual(result["token_count_method"], "utf8_bytes_upper_bound")
        self.assertEqual(result["tokens_used"], len(result["prompt"].encode("utf-8")))
        self.assertLessEqual(result["tokens_used"], 8000)
        self.assertEqual([entry["category"] for entry in result["entries"]], ["preference", "decision", "repository"])
        self.assertEqual(json.loads(result["prompt"].split("\n", 1)[1]), result["entries"])
        self.assertEqual(result["entries"][0]["provenance"][0]["id"], memory["id"])
        self.assertEqual(len(result["entries"][1]["provenance"]), 2)
        self.assertEqual(result["entries"][2]["provenance"][0]["path"], "auth.py")
        self.assertNotIn("expired", result["prompt"])
        self.assertEqual(self.client.assemble_context("auth", result["tokens_used"])["prompt"], result["prompt"])
        self.assertEqual(self.client.assemble_context("auth", 0)["prompt"], "")
        memory_only = self.client.assemble_context(
            "auth", 8000, sources=["memory"], exclude_paths=["auth.py"]
        )
        self.assertTrue(memory_only["entries"])
        self.assertTrue(all(entry["category"] != "repository" for entry in memory_only["entries"]))
        repository_only = self.client.assemble_context(
            "auth", 8000, sources=["repository"], exclude_paths=["auth.py"]
        )
        self.assertEqual(repository_only["entries"], [])
        self.assertTrue(any(hit["path"] == "auth.py" for hit in self.client.search_repository("auth")["hits"]))
        source.unlink()
        self.assertTrue(all(entry["category"] != "repository" for entry in self.client.assemble_context("auth", 8000)["entries"]))
        self.assertEqual(Client(**(self.options | {"user_id": "bob"})).assemble_context("auth", 8000)["entries"], [])
        self.assertEqual(self.client.memory_history(memory["id"]), history)
        self.assertEqual(self.client.get_memory(memory["id"]), memory)
