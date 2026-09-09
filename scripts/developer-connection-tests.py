#!/usr/bin/env python3
"""Focused trust-boundary tests for the persistent developer connection."""

import importlib.util
import json
import os
from pathlib import Path
from types import SimpleNamespace
import stat
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "developer_connection", Path(__file__).with_name("developer-connection.py"))
connection = importlib.util.module_from_spec(spec)
spec.loader.exec_module(connection)

TOKEN = "00000000-0000-4000-8000-000000000000" * 2


class ConnectionFixture(unittest.TestCase):
    def make_state(self, root):
        state = Path(root) / "state"
        state.mkdir(mode=0o700)
        controller = Path(root) / "local-ai"
        controller.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        controller.chmod(0o700)
        identity = state / "connection_ed25519"
        identity.write_text("fixture-key", encoding="utf-8")
        identity.chmod(0o600)
        known = state / "known_hosts"
        known.write_text("100.64.23.14 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFixtureFixtureFixture12345678\n", encoding="utf-8")
        known.chmod(0o600)
        config = {
            "schema_version": 1,
            "host": "mike@100.64.23.14",
            "identity_file": str(identity),
            "known_hosts_file": str(known),
            "remote_root": "C:/a/aw-developer",
            "model_controller": str(controller),
        }
        path = state / "connection.json"
        path.write_text(json.dumps(config), encoding="utf-8")
        path.chmod(0o600)
        return state, config


class ConfigurationTests(ConnectionFixture):
    def test_valid_config_builds_fixed_strict_commands(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as root:
            state, _ = self.make_state(root)
            config = connection.validate_config(state)
            master = connection.master_command(config, Path(root) / "control.sock")
            channel = connection.channel_command(config, Path(root) / "control.sock", "safe")
        self.assertEqual(master[0:3], ["/usr/bin/ssh", "-F", "/dev/null"])
        self.assertIn("StrictHostKeyChecking=yes", master)
        self.assertIn('UserKnownHostsFile="' + config["known_hosts_file"] + '"', master)
        self.assertIn("127.0.0.1:17796:127.0.0.1:7796", master)
        self.assertIn("127.0.0.1:18080:127.0.0.1:8080", master)
        self.assertIn("ProxyCommand=/usr/bin/false", channel)

    def test_symlink_and_hardlink_credentials_fail_closed(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as root:
            state, config = self.make_state(root)
            target = state / "real-key"
            target.write_text("fixture", encoding="utf-8")
            target.chmod(0o600)
            (state / "connection_ed25519").unlink()
            (state / "connection_ed25519").symlink_to(target)
            with self.assertRaisesRegex(ValueError, "symlinks"):
                connection.validate_config(state)
            (state / "connection_ed25519").unlink()
            os.link(target, state / "connection_ed25519")
            with self.assertRaisesRegex(ValueError, "single-link"):
                connection.validate_config(state)

    def test_schema_host_paths_and_empty_host_record_are_rejected(self):
        mutations = [
            ("host", "mike@host;touch /tmp/x"),
            ("remote_root", "C:/a/root & whoami"),
            ("identity_file", "/tmp/key"),
        ]
        for field, value in mutations:
            with self.subTest(field=field), tempfile.TemporaryDirectory(dir=Path.home()) as root:
                state, config = self.make_state(root)
                config[field] = value
                path = state / "connection.json"
                path.write_text(json.dumps(config), encoding="utf-8")
                path.chmod(0o600)
                with self.assertRaises(ValueError):
                    connection.validate_config(state)
        with tempfile.TemporaryDirectory(dir=Path.home()) as root:
            state, _ = self.make_state(root)
            (state / "known_hosts").write_text("", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "one exact Ed25519 record"):
                connection.validate_config(state)

    def test_runtime_settings_reject_unknown_or_injectable_values(self):
        bad = [
            {"extra": "value"},
            {"endpoint": "http://example.com", "token": TOKEN},
            {"token": "secret"},
            {"windows_model_url": "http://127.0.0.1:18081/v1&whoami",
             "windows_model": "coder"},
            {"review_codex_executable": "C:/tools/codex.exe&whoami",
             "review_codex_home": "C:/Users/mike/.codex"},
        ]
        for value in bad:
            with self.subTest(value=value), tempfile.TemporaryDirectory(dir=Path.home()) as root:
                state, _ = self.make_state(root)
                runtime = state / "runtime.json"
                runtime.write_text(json.dumps(value), encoding="utf-8")
                runtime.chmod(0o600)
                with self.assertRaises(ValueError):
                    connection.safe_runtime_settings(state)

    def test_json_hardlinks_and_malformed_field_types_fail_closed(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as root:
            state, _ = self.make_state(root)
            os.link(state / "connection.json", Path(root) / "outside-config.json")
            with self.assertRaisesRegex(ValueError, "single-link"):
                connection.validate_config(state)
        with tempfile.TemporaryDirectory(dir=Path.home()) as root:
            state, config = self.make_state(root)
            config["host"] = ["mike", "host"]
            path = state / "connection.json"
            path.write_text(json.dumps(config), encoding="utf-8")
            path.chmod(0o600)
            with self.assertRaises(ValueError):
                connection.validate_config(state)
        with tempfile.TemporaryDirectory(dir=Path.home()) as root:
            state, _ = self.make_state(root)
            runtime = state / "runtime.json"
            runtime.write_text(json.dumps({"windows_model_url": 7,
                                           "windows_model": "coder"}), encoding="utf-8")
            runtime.chmod(0o600)
            os.link(runtime, Path(root) / "outside-runtime.json")
            with self.assertRaisesRegex(ValueError, "direct 0600"):
                connection.safe_runtime_settings(state)


class AuthenticationTests(ConnectionFixture):
    def test_authenticated_status_is_get_only_and_binds_workspace_and_models(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as root:
            state, _ = self.make_state(root)
            config = connection.validate_config(state)
            value = {
                "mode": "supervised_developer",
                "workspace_root": r"\\?\C:\a\aw-developer\projects",
                "review_required": True,
                "review_provider": "openai.codex",
                "review_model": "gpt-5.6-sol",
                "planning_required": True,
                "planning_provider": "openai.codex",
                "planning_model": "gpt-5.6-sol",
            }

            class Response:
                def __enter__(self):
                    return self
                def __exit__(self, *unused):
                    return False
                def read(self):
                    return json.dumps(value).encode()

            observed = []
            def urlopen(request, timeout):
                observed.append((request, timeout))
                return Response()

            with patch.object(connection.urllib.request, "urlopen", side_effect=urlopen):
                self.assertEqual(connection.authenticated_status(
                    {"endpoint": connection.ENDPOINT if hasattr(connection, "ENDPOINT")
                     else "http://127.0.0.1:17796", "token": TOKEN}, config), value)
                value["workspace_root"] = "C:\\other\\projects"
                self.assertIsNone(connection.authenticated_status(
                    {"endpoint": "http://127.0.0.1:17796", "token": TOKEN}, config))
            self.assertTrue(all(request.method == "GET" for request, _ in observed))
            self.assertTrue(all(request.full_url.endswith("/status") for request, _ in observed))

    def test_windows_path_normalization_rejects_aliases_and_wrong_roots(self):
        self.assertEqual(connection.normalize_windows_drive_path(
            r"\\?\C:\a\aw-developer\projects"),
            r"c:\a\aw-developer\projects")
        self.assertEqual(connection.normalize_windows_drive_path(
            r"C:/a/aw-developer/projects"),
            r"c:\a\aw-developer\projects")
        for value in [r"\\server\share\projects", r"\\.\C:\a\projects",
                      r"\\?\UNC\server\share\projects", r"C:\a\..\projects",
                      r"C:\a\.\projects", r"\Device\HarddiskVolume1\projects"]:
            with self.subTest(value=value):
                self.assertIsNone(connection.normalize_windows_drive_path(value))

    def test_runtime_refresh_preserves_valid_model_and_reviewer_settings(self):
        with tempfile.TemporaryDirectory(dir=Path.home()) as root:
            state, _ = self.make_state(root)
            runtime = state / "runtime.json"
            settings = {
                "windows_model_url": "http://127.0.0.1:18081/v1",
                "windows_model": "coder",
                "review_codex_executable": "C:/tools/codex.exe",
                "review_codex_home": "C:/Users/mike/.codex",
            }
            runtime.write_text(json.dumps(settings), encoding="utf-8")
            runtime.chmod(0o600)
            config = connection.validate_config(state)
            with patch.object(connection, "read_remote_token", return_value=TOKEN):
                refreshed = connection.refresh_runtime(state, config, Path(root) / "socket")
            self.assertEqual(refreshed["token"], TOKEN)
            self.assertEqual(refreshed["windows_model"], "coder")
            self.assertEqual(refreshed["review_codex_home"], "C:/Users/mike/.codex")
            self.assertEqual(stat.S_IMODE(runtime.stat().st_mode), 0o600)

    def test_unowned_runner_recovery_requires_exact_positive_absence(self):
        config = {
            "remote_root": "C:/a/aw-developer",
            "host": "mike@host.test",
            "identity_file": "/tmp/key",
            "known_hosts_file": "/tmp/known_hosts",
        }
        results = [
            (SimpleNamespace(returncode=0, stdout="PRESENT\n"), False),
            (SimpleNamespace(returncode=0, stdout="ABSENT\n"), True),
            (SimpleNamespace(returncode=3, stdout="UNKNOWN\n"), None),
            (SimpleNamespace(returncode=0, stdout="unexpected\n"), None),
        ]
        for result, expected in results:
            with self.subTest(output=result.stdout), \
                 patch.object(connection.subprocess, "run", return_value=result) as run:
                self.assertIs(connection.runner_process_absent(config, Path("/tmp/socket")),
                              expected)
                command = run.call_args.args[0][-1]
                self.assertIn("Get-CimInstance Win32_Process", command)
                self.assertIn("Normalize-DrivePath", command)
                self.assertIn("C:\\a\\aw-developer\\bin\\assemblywright-developer.exe",
                              command)

    def test_launchd_activation_retries_unloading_job_without_forced_restart(self):
        bootstrap_count = 0
        commands = []
        def run(command, **unused):
            nonlocal bootstrap_count
            commands.append(command)
            if 'bootstrap' in command:
                bootstrap_count += 1
                return SimpleNamespace(returncode=1 if bootstrap_count == 1 else 0)
            return SimpleNamespace(returncode=0)
        def loaded():
            return bootstrap_count >= 2
        with patch.object(connection, "service_is_loaded", side_effect=loaded), \
             patch.object(connection.subprocess, "run", side_effect=run), \
             patch.object(connection.time, "sleep"):
            connection.activate_service(Path("/tmp/fixture.plist"), timeout=2)
        self.assertEqual(sum('bootstrap' in command for command in commands), 2)
        kicks = [command for command in commands if 'kickstart' in command]
        self.assertEqual(len(kicks), 1)
        self.assertNotIn('-k', kicks[0])


if __name__ == "__main__":
    unittest.main()
