"""No-network tests for the standard-library Pi MCP setup CLI."""

from __future__ import annotations

import importlib.util
import io
import json
import os
from pathlib import Path
import re
import stat
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "setup-pi.py"
SPEC = importlib.util.spec_from_file_location("setup_pi", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
setup_pi = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(setup_pi)


class SetupPiTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.root = self.base / "my project"
        self.root.mkdir()
        self.binary = self.base / "bin" / "tinymem"
        self.database = self.base / "private" / "memory.db"
        self.config_path = self.root / ".pi" / "mcp.json"

    def run_setup(self, *extra: str) -> tuple[int, str, str]:
        stdout, stderr = io.StringIO(), io.StringIO()
        argv = [
            "--root", str(self.root),
            "--binary", str(self.binary),
            "--db", str(self.database),
            "--user", "alice",
            "--project", "sample-project",
            *extra,
        ]
        status = setup_pi.main(argv, stdout=stdout, stderr=stderr)
        return status, stdout.getvalue(), stderr.getvalue()

    def read_config(self) -> dict:
        return json.loads(self.config_path.read_text(encoding="utf-8"))

    def test_creates_native_project_config_with_absolute_paths_and_exact_scope(self) -> None:
        status, output, error = self.run_setup()
        self.assertEqual(status, 0, error)
        config = self.read_config()
        self.assertEqual(set(config["mcpServers"]), {"tinymem"})
        server = config["mcpServers"]["tinymem"]
        self.assertEqual(server["command"], str(self.binary.resolve()))
        self.assertTrue(Path(server["command"]).is_absolute())
        self.assertEqual(server["args"], [
            "--db", str(self.database.resolve()),
            "mcp", str(self.root.resolve()),
            "--user", "alice", "--project", "sample-project",
        ])
        self.assertTrue(Path(server["args"][1]).is_absolute())
        self.assertTrue(Path(server["args"][3]).is_absolute())
        self.assertEqual(server["exposure"], "deferred")
        self.assertIn("database contents were not opened or changed", output)
        self.assertFalse(self.database.exists())
        if os.name == "posix":
            self.assertEqual(self.database.parent.stat().st_mode & 0o777, 0o700)
            self.assertEqual(self.config_path.stat().st_mode & 0o777, 0o600)

    def test_release_binary_is_default_and_default_database_is_per_project(self) -> None:
        self.assertEqual(setup_pi.default_binary_path(self.base, platform="linux"),
                         self.base / "target" / "release" / "tinymem")
        self.assertEqual(setup_pi.default_binary_path(self.base, platform="win32"),
                         self.base / "target" / "release" / "tinymem.exe")
        root_a = self.base / "alpha"
        root_b = self.base / "beta"
        root_a.mkdir()
        root_b.mkdir()
        home = self.base / "home"
        database_a = setup_pi.default_database_path(root_a, home=home, platform="linux")
        database_a_again = setup_pi.default_database_path(root_a, home=home, platform="linux")
        database_b = setup_pi.default_database_path(root_b, home=home, platform="linux")
        self.assertEqual(database_a, database_a_again)
        self.assertNotEqual(database_a, database_b)
        self.assertEqual(setup_pi.default_project_scope(root_a), "alpha")
        odd_root = self.root
        default_scope = setup_pi.default_project_scope(odd_root)
        self.assertRegex(default_scope, r"^my-project-[0-9a-f]{12}$")
        moved_root = self.base / "moved" / "my project"
        moved_root.mkdir(parents=True)
        self.assertNotEqual(default_scope, setup_pi.default_project_scope(moved_root))
        self.assertEqual(database_a.parent.parent, home.resolve() / ".local" / "share" / "tinymem")
        self.assertEqual(
            setup_pi.data_root(
                home=home, environ={"XDG_DATA_HOME": str(self.base / "xdg")}, platform="linux"
            ),
            self.base / "xdg" / "tinymem",
        )
        self.assertEqual(setup_pi.data_root(home=home, platform="darwin"),
                         home / "Library" / "Application Support" / "tinymem")

    def test_default_release_binary_and_username_project_defaults(self) -> None:
        stdout, stderr = io.StringIO(), io.StringIO()
        with patch.object(setup_pi, "REPOSITORY_ROOT", self.base), patch.object(
            setup_pi.getpass, "getuser", return_value="os-user"
        ), patch.dict(os.environ, {"HOME": str(self.base / "home")}):
            status = setup_pi.main(["--root", str(self.root), "--dry-run"],
                                   stdout=stdout, stderr=stderr)
        self.assertEqual(status, 0, stderr.getvalue())
        expected_binary = str((self.base / "target" / "release" / "tinymem").resolve())
        self.assertIn(expected_binary, stdout.getvalue())
        self.assertIn("user='os-user'", stdout.getvalue())
        project_match = re.search(r"project='([^']+)'", stdout.getvalue())
        self.assertIsNotNone(project_match)
        assert project_match is not None
        self.assertRegex(project_match.group(1), r"^my-project-[0-9a-f]{12}$")
        self.assertFalse(self.config_path.exists())

    def test_scope_tokens_match_sdk_contract_and_invalid_os_username_is_actionable(self) -> None:
        self.assertEqual(setup_pi._scope_token("alice-1", "user scope", "--user"), "alice-1")
        for invalid in ("", "with spaces", "éclair", "_leading", "x" * 129):
            with self.subTest(invalid=invalid), self.assertRaises(setup_pi.SetupError):
                setup_pi._scope_token(invalid, "user scope", "--user")

        stdout, stderr = io.StringIO(), io.StringIO()
        with patch.object(setup_pi.getpass, "getuser", return_value="domain\\user"):
            status = setup_pi.main(
                ["--root", str(self.root), "--binary", str(self.binary), "--dry-run"],
                stdout=stdout, stderr=stderr,
            )
        self.assertEqual(status, 1)
        self.assertIn("--user", stderr.getvalue())
        self.assertFalse(self.config_path.exists())

    def test_rejects_non_regular_and_oversized_config_files(self) -> None:
        self.config_path.parent.mkdir()
        self.config_path.mkdir()
        status, _, error = self.run_setup()
        self.assertEqual(status, 1)
        self.assertIn("regular file", error)
        self.config_path.rmdir()

        oversized = b" " * (setup_pi.MAX_CONFIG_BYTES + 1)
        self.config_path.write_bytes(oversized)
        status, _, error = self.run_setup()
        self.assertEqual(status, 1)
        self.assertIn("exceeds 65536 bytes", error)
        self.assertEqual(self.config_path.stat().st_size, len(oversized))

    def test_merges_unrelated_servers_and_preserves_existing_config_mode(self) -> None:
        self.config_path.parent.mkdir()
        original = {
            "mcpServers": {"docs": {"url": "https://example.invalid/mcp", "headers": {
                "Authorization": "Bearer private-value",
            }}},
            "otherPiSetting": ["preserved", {"n": 1}],
        }
        self.config_path.write_text(json.dumps(original, indent=4), encoding="utf-8")
        self.config_path.chmod(0o640)
        expected_mode = stat.S_IMODE(self.config_path.stat().st_mode)
        status, _, error = self.run_setup()
        self.assertEqual(status, 0, error)
        updated = self.read_config()
        self.assertEqual(updated["mcpServers"]["docs"], original["mcpServers"]["docs"])
        self.assertEqual(updated["otherPiSetting"], original["otherPiSetting"])
        self.assertEqual(stat.S_IMODE(self.config_path.stat().st_mode), expected_mode)

    def test_existing_database_is_never_changed(self) -> None:
        self.database.parent.mkdir(mode=0o700)
        self.database.write_bytes(b"existing database bytes must stay intact")
        before = self.database.read_bytes()
        status, _, error = self.run_setup()
        self.assertEqual(status, 0, error)
        self.assertEqual(self.database.read_bytes(), before)

    def test_binary_override_must_remain_extension_compatible(self) -> None:
        status, _, error = self.run_setup("--binary", str(self.base / "renamed-executable"))
        self.assertEqual(status, 1)
        self.assertIn("must resolve to the native executable named tinymem", error)
        self.assertFalse(self.config_path.exists())

    def test_dry_run_changes_nothing(self) -> None:
        status, output, error = self.run_setup("--dry-run")
        self.assertEqual(status, 0, error)
        self.assertIn(
            "dry run: no configuration, database, or directory was created or changed", output
        )
        self.assertFalse(self.config_path.exists())
        self.assertFalse(self.database.parent.exists())

    def test_conflicting_named_server_refuses_without_force_and_merges_with_force(self) -> None:
        self.config_path.parent.mkdir()
        original = {
            "mcpServers": {
                "tinymem": {"command": "/old/tinymem", "args": ["old-config"]},
                "other": {"command": "other-mcp", "args": [], "headers": {
                    "Authorization": "Bearer unrelated-secret",
                }},
            },
            "unrelated": True,
        }
        self.config_path.write_text(json.dumps(original), encoding="utf-8")
        before = self.config_path.read_bytes()
        status, _, error = self.run_setup()
        self.assertEqual(status, 1)
        self.assertIn("not overwriting it", error)
        self.assertEqual(self.config_path.read_bytes(), before)

        status, _, error = self.run_setup("--force")
        self.assertEqual(status, 0, error)
        updated = self.read_config()
        self.assertEqual(updated["mcpServers"]["other"], original["mcpServers"]["other"])
        self.assertEqual(updated["mcpServers"]["other"]["headers"]["Authorization"],
                         "Bearer unrelated-secret")
        self.assertEqual(updated["unrelated"], True)
        self.assertEqual(updated["mcpServers"]["tinymem"]["command"], str(self.binary.resolve()))
        self.assertFalse(self.database.exists())

    def test_force_refuses_to_discard_credentials_from_target_entry(self) -> None:
        self.config_path.parent.mkdir()
        original = {
            "mcpServers": {
                "tinymem": {"command": "/old/tinymem", "args": ["old-config"],
                            "env": {"API_TOKEN": "keep-this"}},
            },
        }
        self.config_path.write_text(json.dumps(original), encoding="utf-8")
        before = self.config_path.read_bytes()
        status, _, error = self.run_setup("--force")
        self.assertEqual(status, 1)
        self.assertIn("refusing to remove or transfer", error)
        self.assertEqual(self.config_path.read_bytes(), before)

    def test_existing_native_tinymem_test_prevents_duplicate_server(self) -> None:
        self.config_path.parent.mkdir()
        existing_database = self.base / "legacy.db"
        existing_database.write_bytes(b"keep")
        existing = {
            "command": "/old/path/tinymem",
            "args": ["--db", str(existing_database), "mcp", str(self.root), "--user", "alice"],
            "exposure": "direct",
        }
        test_config = {"mcpServers": {"tinymem-test": existing}}
        self.config_path.write_text(json.dumps(test_config), encoding="utf-8")
        before = self.config_path.read_bytes()
        status, _, error = self.run_setup()
        self.assertEqual(status, 1)
        self.assertIn("would start duplicate memory servers", error)
        self.assertIn("--server-name", error)
        self.assertEqual(self.config_path.read_bytes(), before)

        status, _, error = self.run_setup("--server-name", "tinymem-test", "--force")
        self.assertEqual(status, 0, error)
        self.assertEqual(self.read_config()["mcpServers"]["tinymem-test"]["command"],
                         str(self.binary.resolve()))
        self.assertEqual(existing_database.read_bytes(), b"keep")
        self.assertNotIn("tinymem", self.read_config()["mcpServers"])

    def test_pi_hyphen_underscore_name_collision_is_refused(self) -> None:
        self.config_path.parent.mkdir()
        self.config_path.write_text(json.dumps({"mcpServers": {"tiny_mem": {"command": "x"}}}),
                                    encoding="utf-8")
        before = self.config_path.read_bytes()
        status, _, error = self.run_setup("--server-name", "tiny-mem")
        self.assertEqual(status, 1)
        self.assertIn("treats server names", error)
        self.assertEqual(self.config_path.read_bytes(), before)

    def test_malformed_config_and_symlinks_are_not_replaced(self) -> None:
        self.config_path.parent.mkdir()
        self.config_path.write_text("not json", encoding="utf-8")
        before = self.config_path.read_bytes()
        status, _, error = self.run_setup()
        self.assertEqual(status, 1)
        self.assertIn("not valid JSON", error)
        self.assertEqual(self.config_path.read_bytes(), before)

        self.config_path.unlink()
        destination = self.base / "linked.json"
        destination.write_text("{}", encoding="utf-8")
        try:
            self.config_path.symlink_to(destination)
        except (OSError, NotImplementedError):
            self.skipTest("symlinks are unavailable on this platform")
        status, _, error = self.run_setup()
        self.assertEqual(status, 1)
        self.assertIn("symlinked Pi config", error)
        self.assertEqual(destination.read_text(encoding="utf-8"), "{}")

        self.config_path.unlink()
        self.config_path.parent.rmdir()
        external_pi = self.base / "external-pi"
        external_pi.mkdir()
        try:
            self.config_path.parent.symlink_to(external_pi, target_is_directory=True)
        except (OSError, NotImplementedError):
            self.skipTest("directory symlinks are unavailable on this platform")
        status, _, error = self.run_setup()
        self.assertEqual(status, 1)
        self.assertIn("symlinked project config directory", error)
        self.assertFalse((external_pi / "mcp.json").exists())

    def test_identical_server_config_is_an_idempotent_no_op(self) -> None:
        status, _, error = self.run_setup()
        self.assertEqual(status, 0, error)
        before = self.config_path.read_bytes()
        database_parent_mtime = self.database.parent.stat().st_mtime_ns
        status, output, error = self.run_setup()
        self.assertEqual(status, 0, error)
        self.assertIn("Already configured", output)
        self.assertEqual(self.config_path.read_bytes(), before)
        self.assertEqual(self.database.parent.stat().st_mtime_ns, database_parent_mtime)
        self.assertFalse(self.database.exists())

    def test_optional_agent_and_direct_exposure_are_configured(self) -> None:
        status, _, error = self.run_setup("--agent", "pi", "--exposure", "direct")
        self.assertEqual(status, 0, error)
        server = self.read_config()["mcpServers"]["tinymem"]
        self.assertEqual(server["args"][-6:], [
            "--user", "alice", "--agent", "pi", "--project", "sample-project",
        ])
        self.assertEqual(server["exposure"], "direct")


if __name__ == "__main__":
    unittest.main()
