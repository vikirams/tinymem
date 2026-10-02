import json
import os
from pathlib import Path
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor
import unittest

from tinymem_native import Client, OutputLimitError, ProtocolError, RPCError, RPCTimeoutError, ToolError, TransportError

FIXTURE = Path(__file__).with_name("fake_server.py").resolve()
FIXTURE.chmod(0o755)


class ClientTests(unittest.TestCase):
    def test_public_wrappers_send_only_native_versioned_tool_arguments(self):
        client = Client(binary=FIXTURE, user_id="wrappers")
        added = client.add_memory("tea", kind="preference", source="explicit", expires_at_ms=123)
        self.assertEqual(added["source"], "add_memory")
        self.assertEqual(json.loads(added["content"]),
                         {"content": "tea", "kind": "preference", "source": "explicit", "expires_at_ms": 123})
        refs = [{"kind": "file", "role": "dependency", "path": "src/clock.rs", "expected_hash": "blake3:fixture"}]
        referenced = client.add_memory("clock", source_refs=refs)
        self.assertEqual(json.loads(referenced["content"]), {"content": "clock", "source_refs": refs})
        updated = client.update_memory("fixture-id", "coffee", expected_version=1)
        self.assertEqual(json.loads(updated["content"]), {"id": "fixture-id", "content": "coffee", "expected_version": 1})
        self.assertEqual(client.get_memory("fixture-id")["source"], "get_memory")
        self.assertEqual(json.loads(client.search_memories("tea", limit=2, max_bytes=500)[0]["content"]),
                         {"query": "tea", "limit": 2, "max_bytes": 500})
        self.assertEqual(json.loads(client.search_memories("tea", mode="hybrid", min_similarity=0.4)[0]["content"]),
                         {"query": "tea", "mode": "hybrid", "min_similarity": 0.4})
        self.assertEqual(json.loads(client.memory_history("fixture-id", limit=3, max_bytes=700)[0]["memory"]["content"]),
                         {"id": "fixture-id", "limit": 3, "max_bytes": 700})
        self.assertTrue(client.delete_memory("fixture-id", expected_version=2))
        self.assertEqual(client.get_profile()["dynamic"], [])
        self.assertEqual(client.refresh_repository()["repository"], "/fixture")
        self.assertEqual(client.search_repository("hello")["hits"], [])
        context = client.assemble_context("auth", 500)
        self.assertEqual(json.loads(context["prompt"]), {"query": "auth", "token_budget": 500})
        self.assertEqual(context["entries"], [])
        self.assertEqual(context["token_count_method"], "utf8_bytes_upper_bound")
        filtered = client.assemble_context(
            "auth", 500, sources=["memory"], exclude_paths=["src/generated/**"]
        )
        self.assertEqual(json.loads(filtered["prompt"]), {
            "query": "auth", "token_budget": 500,
            "sources": ["memory"], "exclude_paths": ["src/generated/**"],
        })
        self.assertEqual(filtered["entries"], [])
        self.assertEqual(filtered["tokens_used"], len(filtered["prompt"].encode("utf-8")))
        empty = client.assemble_context("auth", 500, sources=[], exclude_paths=[])
        self.assertEqual(json.loads(empty["prompt"]), {
            "query": "auth", "token_budget": 500, "sources": [], "exclude_paths": [],
        })
        with self.assertRaises(ValueError):
            client.assemble_context("auth", 500, sources=["memory", "memory"])
        with self.assertRaises(ValueError):
            client.assemble_context("auth", 500, exclude_paths=["../secrets/**"])

    def assert_rejected_and_reaped(self, mode, error_type, *, arguments=None, **options):
        with tempfile.TemporaryDirectory() as directory:
            db = Path(directory) / "pid"
            timeout = options.pop("timeout", 2.0)
            client = Client(binary=FIXTURE, user_id=mode, db=db, timeout=timeout, close_timeout=0.05, **options)
            started = time.monotonic()
            with self.assertRaises(error_type) as caught:
                client.call_tool("echo", arguments)
            self.assertLess(time.monotonic() - started, timeout + 1)
            with self.assertRaises(ProcessLookupError):
                os.kill(int(db.read_text()), 0)
            return caught.exception

    def test_invalid_handshakes_frames_and_nonzero_exit_cannot_succeed(self):
        for mode in ["wrong-version", "bad-server-info", "wrong-id", "duplicate-id", "unsolicited-zero", "invalid-json",
                     "invalid-utf8", "partial-frame", "bad-rpc-error", "malformed-result", "malformed-notification"]:
            with self.subTest(mode=mode):
                self.assert_rejected_and_reaped(mode, ProtocolError)
        error = self.assert_rejected_and_reaped("exit-nonzero", TransportError)
        self.assertEqual(error.returncode, 9)
        self.assertIn("intentional exit after response", error.stderr)
        error = self.assert_rejected_and_reaped("startup-exit", TransportError)
        self.assertIn("intentional startup failure", error.stderr)

    def test_deadline_includes_handshake_blocked_write_tool_and_eof_exit(self):
        for mode in ["init-hang", "tool-hang", "no-read", "ignore-term"]:
            with self.subTest(mode=mode):
                arguments = {"content": "x" * (1024 * 1024)} if mode == "no-read" else {}
                self.assert_rejected_and_reaped(mode, RPCTimeoutError, arguments=arguments, timeout=0.5)

    def test_caps_output_frames_cumulative_notifications_and_stderr(self):
        self.assert_rejected_and_reaped("frame-flood", OutputLimitError, max_frame_bytes=512)
        self.assert_rejected_and_reaped("notification-flood", OutputLimitError, max_output_bytes=1024)
        error = self.assert_rejected_and_reaped("stderr-flood", OutputLimitError, max_stderr_bytes=128)
        self.assertLessEqual(len(error.stderr.encode()), 128)

    def test_business_and_jsonrpc_errors_preserve_their_distinct_types(self):
        self.assert_rejected_and_reaped("business-error", ToolError)
        error = self.assert_rejected_and_reaped("rpc-error", RPCError)
        self.assertEqual(error.code, -32602)
        self.assertEqual(error.data, {"detail": 1})
        with self.assertRaises(TransportError):
            Client(binary="/no/such/tinymem", user_id="alice").get_memory("memory")

    def test_notifications_fragmented_utf8_null_memory_and_large_dual_results(self):
        for mode in ["notifications", "fragmented"]:
            result = Client(binary=FIXTURE, user_id=mode).call_tool("echo", {"content": "漢字 café\nline"})
            self.assertEqual(result["arguments"], {"content": "漢字 café\nline"})
        self.assertIsNone(Client(binary=FIXTURE, user_id="null-memory").get_memory("missing"))
        result = Client(binary=FIXTURE, user_id="large-result").call_tool("echo")
        self.assertEqual(len(result["payload"]), 1024 * 1024 + 64)

    def test_local_validation_happens_before_any_child_starts(self):
        for scope in ["", "alice bob", "$(execute)", "--option", "漢字", "x" * 129]:
            with self.subTest(scope=scope), self.assertRaises(ValueError):
                Client(user_id=scope)
        for option in [{"timeout": float("inf")}, {"max_frame_bytes": 0}, {"max_stderr_bytes": -1},
                       {"embedding_command": FIXTURE}, {"embedding_model": "test"},
                       {"embedding_command": FIXTURE, "embedding_model": "test", "embedding_dimensions": 0},
                       {"embedding_command": FIXTURE, "embedding_model": "test", "embedding_dimensions": 3, "embedding_timeout_ms": 0}]:
            with self.subTest(option=option), self.assertRaises(ValueError):
                Client(user_id="alice", **option)
        with tempfile.TemporaryDirectory() as directory:
            db = Path(directory) / "pid"
            client = Client(binary=FIXTURE, user_id="alice", db=db, max_frame_bytes=512)
            for key in ["scope", "user_id", "agent_id", "project_id", "user", "repository"]:
                with self.assertRaises(ValueError):
                    client.call_tool("get_memory", {"id": "id", key: "other"})
            with self.assertRaises(ValueError):
                client.call_tool("echo", {"content": "漢" * 512})
            for budget in [-1, True, 1.5, None]:
                with self.assertRaises(ValueError):
                    client.assemble_context("auth", budget)
            for options in [
                {"sources": ["other"]},
                {"sources": ["memory", "memory"]},
                {"exclude_paths": ["../outside/**"]},
            ]:
                with self.subTest(options=options), self.assertRaises(ValueError):
                    client.assemble_context("auth", 500, **options)
            self.assertFalse(db.exists())

    def test_calls_on_shared_configuration_are_independent_and_thread_safe(self):
        client = Client(binary=FIXTURE, user_id="alice")
        with ThreadPoolExecutor(max_workers=4) as executor:
            results = list(executor.map(lambda i: client.call_tool("echo", {"n": i}), range(4)))
        self.assertEqual([result["arguments"]["n"] for result in results], [0, 1, 2, 3])
        self.assertEqual(len({result["pid"] for result in results}), 4)
        for result in results:
            with self.assertRaises(ProcessLookupError):
                os.kill(result["pid"], 0)

    def test_each_operation_handshakes_uses_literal_arguments_and_reaps_on_eof(self):
        with tempfile.TemporaryDirectory(prefix="sdk space ' ; ") as directory:
            db = Path(directory) / "pid"
            client = Client(binary=FIXTURE, db=db, repository=directory,
                            user_id="alice", agent_id="assistant", project_id="project")
            pids = []
            for content in ["hello", "café 漢字\n$(do not execute)"]:
                result = client.call_tool("echo", {"content": content})
                self.assertEqual(result["arguments"], {"content": content})
                self.assertEqual(result["argv"], ["--db", str(db), "mcp", directory,
                                                  "--user", "alice", "--agent", "assistant", "--project", "project"])
                pid = result["pid"]
                self.assertEqual(int(db.read_text()), pid)
                with self.assertRaises(ProcessLookupError):
                    os.kill(pid, 0)
                pids.append(pid)
            self.assertNotEqual(*pids)
