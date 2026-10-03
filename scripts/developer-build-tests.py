#!/usr/bin/env python3
"""Focused launcher configuration checks; never starts SSH or a model."""
import importlib.util
from pathlib import Path
import sys
from types import SimpleNamespace
import threading
import unittest
from unittest.mock import patch, MagicMock
import tempfile
import json
from io import BytesIO
import http.server
import os
import socket
import time
from contextlib import ExitStack

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('developer_build', Path(__file__).with_name('developer-build.py'))
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


class WindowsModelSettingsTests(unittest.TestCase):
    def args(self, **values):
        return SimpleNamespace(**dict.fromkeys(
            ['windows_model_url', 'windows_model', 'windows_model_start_script']) | values)

    def test_legacy_mac_configuration_stays_unchanged(self):
        self.assertEqual(launcher.windows_model_settings(self.args(), {}), {})

    def test_windows_configuration_survives_later_launches(self):
        values = {'windows_model_url': 'http://127.0.0.1:18081/v1',
                  'windows_model': 'windows-coder'}
        self.assertEqual(launcher.windows_model_settings(self.args(**values), {}), values)
        self.assertEqual(launcher.windows_model_settings(self.args(), values), values)
        changed = launcher.windows_model_settings(self.args(windows_model='coder-v2'), values)
        self.assertEqual(changed['windows_model'], 'coder-v2')
        self.assertEqual(values['windows_model'], 'windows-coder')

    def test_legacy_start_script_is_not_advertised_or_executed(self):
        saved = {'windows_model_url': 'http://127.0.0.1:18081/v1',
                 'windows_model': 'windows-coder',
                 'windows_model_start_script': 'C:/a/aw-local-model/start.ps1'}
        settings = launcher.windows_model_settings(self.args(), saved)
        self.assertNotIn('windows_model_start_script', settings)
        self.assertEqual(settings['windows_model'], 'windows-coder')
        with self.assertRaisesRegex(ValueError, 'managed independently'):
            launcher.windows_model_settings(self.args(windows_model_start_script=saved['windows_model_start_script']), saved)

    def test_partial_or_unsafe_configuration_is_rejected(self):
        for url in ['https://127.0.0.1:18081/v1', 'http://example.com:80/v1',
                    'http://10.0.0.1:18081/v1', 'http://user:secret@127.0.0.1:18081/v1',
                    'http://127.0.0.1:18081/v1?key=secret', 'http://127.0.0.1:18081/v1#x',
                    'http://127.0.0.1:18081/v1&whoami', 'http://127.0.0.1:99999/v1',
                    'http://127.0.0.1:18081/%PATH%']:
            with self.subTest(url=url), self.assertRaises(ValueError):
                launcher.windows_model_settings(self.args(windows_model_url=url, windows_model='coder'), {})
        for model in ['bad&command', '%PATH%', 'bad name', '-flag', 'x'*129]:
            with self.subTest(model=model), self.assertRaises(ValueError):
                launcher.windows_model_settings(self.args(windows_model_url='http://127.0.0.1:18081/v1', windows_model=model), {})
        with self.assertRaises(ValueError):
            launcher.windows_model_settings(self.args(windows_model='coder'), {})
        with self.assertRaises(ValueError):
            launcher.windows_model_settings(self.args(windows_model_start_script='C:/start.ps1'), {})
        with self.assertRaises(ValueError):
            launcher.windows_model_settings(self.args(windows_model_url='http://127.0.0.1:18081/v1', windows_model='coder', windows_model_start_script='C:/start.ps1&whoami'), {})


class ReviewerSettingsTests(unittest.TestCase):
    def args(self, **values):
        return SimpleNamespace(**values)

    def test_saved_reviewer_is_reused_without_model_override(self):
        saved = {'review_codex_executable': 'C:/tools/@openai/codex.exe',
                 'review_codex_home': 'C:/Users/mike/.codex'}
        self.assertEqual(launcher.review_settings(self.args(), saved), saved)
        self.assertEqual(launcher.review_settings(self.args(**saved), {}), saved)
        self.assertEqual(launcher.review_settings(self.args(), {}), {})

    def test_missing_pair_and_shell_injection_are_rejected(self):
        for executable, home in [('C:/tools/codex.exe', None),
                                 (None, 'C:/Users/mike/.codex'),
                                 ('C:/tools/codex.cmd', 'C:/Users/mike/.codex'),
                                 ('C:/tools/codex.exe&whoami', 'C:/Users/mike/.codex'),
                                 ('C:/tools/codex.exe', 'C:/%USERNAME%/.codex'),
                                 ('C:/tools/codex.exe', 'C:/x$(whoami)'),
                                 ('codex.exe', 'C:/Users/mike/.codex')]:
            with self.subTest(executable=executable, home=home), self.assertRaises(ValueError):
                launcher.review_settings(self.args(review_codex_executable=executable,
                                                    review_codex_home=home), {})


class BuildProfileTests(unittest.TestCase):
    def test_developer_profile_retains_debug_identity_and_paths(self):
        profile = launcher.DEVELOPER_BUILD_PROFILE
        self.assertEqual(profile.app, launcher.ROOT / 'target/developer/Assemblywright Developer.app')
        self.assertEqual(profile.app_name, 'Assemblywright Developer')
        self.assertEqual(profile.bundle_id, 'com.nobiletechnology.assemblywright.developer')
        self.assertEqual(profile.swift_configuration, 'debug')
        self.assertEqual(profile.windows_configuration, 'debug')
        self.assertNotIn('-c', launcher.swift_build_command(profile))
        self.assertEqual(
            launcher.swift_executable_path(profile),
            launcher.ROOT / 'apps/mac/.build/debug/AssemblywrightMacApp')
        self.assertEqual(
            launcher.windows_executable_path(profile, r'C:\a\assemblywright'),
            r'C:\a\assemblywright\target\debug\assemblywright-developer.exe')

    def test_production_profile_uses_release_commands_and_fixed_identity(self):
        profile = launcher.PRODUCTION_BUILD_PROFILE
        self.assertEqual(profile.app, launcher.ROOT / 'target/production/Assemblywright.app')
        self.assertEqual(profile.app_name, 'Assemblywright')
        self.assertEqual(profile.bundle_id, 'com.nobiletechnology.assemblywright')
        self.assertEqual(profile.swift_configuration, 'release')
        self.assertEqual(profile.windows_configuration, 'release')
        self.assertEqual(launcher.swift_build_command(profile)[3:5], ['-c', 'release'])
        command = launcher.windows_build_command(profile, r'C:\a\assemblywright')
        self.assertIn('cargo build --release -p assemblywright-master', command)
        self.assertIn('--bin assemblywright-developer', command)
        self.assertEqual(
            launcher.swift_executable_path(profile),
            launcher.ROOT / 'apps/mac/.build/release/AssemblywrightMacApp')
        self.assertEqual(
            launcher.windows_executable_path(profile, r'C:\a\assemblywright'),
            r'C:\a\assemblywright\target\release\assemblywright-developer.exe')

    def test_both_profiles_use_canonical_version_and_developer_runtime(self):
        completed = SimpleNamespace(stdout='1.2.3\n')
        with patch.object(launcher.subprocess, 'run', return_value=completed) as run:
            version = launcher.release_version()
        self.assertEqual(version, '1.2.3')
        run.assert_called_once_with(
            [str(launcher.ROOT / 'scripts/release-version.sh')],
            check=True, capture_output=True, text=True)

        developer = launcher.app_info(launcher.DEVELOPER_BUILD_PROFILE, version)
        production = launcher.app_info(launcher.PRODUCTION_BUILD_PROFILE, version)
        for info in (developer, production):
            self.assertEqual(info['CFBundleVersion'], version)
            self.assertEqual(info['CFBundleShortVersionString'], version)
            self.assertEqual(info['AssemblywrightRuntime'], 'developer')
        self.assertTrue(developer['AssemblywrightDeveloperBuild'])
        self.assertNotIn('AssemblywrightDeveloperBuild', production)

    def test_malformed_canonical_version_is_rejected(self):
        completed = SimpleNamespace(stdout='1.2; touch /tmp/unsafe\n')
        with patch.object(launcher.subprocess, 'run', return_value=completed):
            with self.assertRaisesRegex(SystemExit, 'malformed'):
                launcher.release_version()


class RunnerMaintenanceAdmissionTests(unittest.TestCase):
    def setUp(self):
        self.runtime = {'endpoint': 'http://127.0.0.1:17796',
                        'token': 'a' * 64}
        self.config = {'remote_root': 'C:/a/aw-developer'}

    def test_active_planning_cannot_be_interrupted(self):
        status = {'running': False, 'chat_running': False, 'planning_running': True}
        connection = SimpleNamespace(authenticated_status=lambda runtime, config: status)
        with patch.object(launcher.urllib.request, 'urlopen') as request:
            with self.assertRaisesRegex(SystemExit, 'Cancel brainstorming'):
                launcher.request_shutdown(connection, self.runtime, self.config)
            request.assert_not_called()

    def test_unknown_state_cannot_stop_supervisor(self):
        connection = SimpleNamespace(authenticated_status=lambda runtime, config: None)
        with patch.object(launcher.urllib.request, 'urlopen') as request:
            with self.assertRaisesRegex(SystemExit, 'No process was stopped'):
                launcher.request_shutdown(connection, self.runtime, self.config)
            request.assert_not_called()

    def test_active_github_work_cannot_be_interrupted(self):
        for key in ('github_publication_running', 'github_setup_busy'):
            with self.subTest(key=key):
                connection = SimpleNamespace(authenticated_status=lambda runtime, config: {key: True})
                with patch.object(launcher.urllib.request, 'urlopen') as request:
                    with self.assertRaisesRegex(SystemExit, 'GitHub sign-in'):
                        launcher.request_shutdown(connection, self.runtime, self.config)
                    request.assert_not_called()

    def test_changed_destination_stops_existing_runner_before_configuration_write(self):
        for refusal in (None, 'shutdown', 'github_publication_unresolved', 'github_setup_unresolved'):
            refuse_shutdown = refusal == 'shutdown'
            unresolved = refusal is not None and refusal.startswith('github_')
            with self.subTest(refusal=refusal), tempfile.TemporaryDirectory() as temp, ExitStack() as stack:
                state = Path(temp)
                (state / 'connection.json').write_text('{}')
                old_config = {'host': 'mike@old.test'}
                desired = {'host': 'mike@new.test'}
                connection = SimpleNamespace(
                    direct_private_directory=lambda *args, **kwargs: None,
                    validate_config=lambda state: old_config,
                    validate_config_payload=lambda *args: desired,
                    service_is_loaded=lambda: True)
                stack.enter_context(patch.object(launcher, 'STATE', state))
                stack.enter_context(patch.object(launcher.sys, 'argv', ['developer-build.py', '--stop', '--host', 'mike@new.test']))
                for name, value in [('_connection_module', connection), ('load_saved_runtime', {}),
                    ('authenticated_status', {'revision': 1, **({refusal: True} if unresolved else {})}), ('load_migration', None),
                    ('legacy_migration_candidate', False), ('preflight_connection', None)]:
                    stack.enter_context(patch.object(launcher, name, return_value=value))
                shutdown = stack.enter_context(patch.object(launcher, 'request_shutdown',
                    side_effect=SystemExit('active work') if refuse_shutdown else None))
                stop = stack.enter_context(patch.object(launcher, 'stop_supervisor'))
                write = stack.enter_context(patch.object(launcher, 'write_connection_config'))
                if unresolved:
                    with self.assertRaisesRegex(SystemExit, 'Resolve pending GitHub work'):
                        launcher.main()
                    shutdown.assert_not_called()
                    stop.assert_not_called()
                elif refuse_shutdown:
                    with self.assertRaisesRegex(SystemExit, 'active work'):
                        launcher.main()
                    stop.assert_not_called()
                else:
                    launcher.main()
                    stop.assert_called_once_with(connection)
                if not unresolved:
                    shutdown.assert_called_once_with(connection, {}, old_config)
                write.assert_not_called()

    def test_tool_pin_survives_rebuild_and_rejects_unsafe_paths(self):
        saved = {'opencode_executable': 'C:/tools/opencode.exe'}
        self.assertEqual(launcher.tool_settings(SimpleNamespace(), saved), saved)
        for path in ('opencode.exe', 'C:/tools/../opencode.exe', 'C:/tools/wrong.exe', 'C:/tools/opencode.exe&whoami'):
            with self.subTest(path=path), self.assertRaises(ValueError):
                launcher.tool_settings(SimpleNamespace(opencode_executable=path), {})

    def test_idle_shutdown_is_the_only_control_action(self):
        status = {'running': False, 'chat_running': False, 'planning_running': False}
        connection = SimpleNamespace(authenticated_status=lambda runtime, config: status)
        response = BytesIO(b'{}')
        with patch.object(launcher.urllib.request, 'urlopen', return_value=response) as request:
            launcher.request_shutdown(connection, self.runtime, self.config)
        sent = request.call_args.args[0]
        self.assertEqual(sent.method, 'POST')
        self.assertEqual(json.loads(sent.data), {'action': 'shutdown'})

    def test_legacy_migration_cancels_only_known_forwards(self):
        args = SimpleNamespace(host='mike@100.64.23.14',
                               socket='/tmp/legacy-control.sock')
        completed = SimpleNamespace(returncode=0)
        connection = SimpleNamespace(atomic_json=lambda path, value: None)
        migration = launcher.migration_template(args, 7)
        with patch.object(launcher, 'local_forward_absent', return_value=True), \
             patch.object(launcher, 'remote_forward_absent', return_value=True), \
             patch.object(launcher.subprocess, 'run', return_value=completed) as run:
            launcher.cancel_known_old_forwards(connection, args, migration)
        commands = [call.args[0] for call in run.call_args_list if '-O' in call.args[0]]
        self.assertEqual(len(commands), 2)
        self.assertTrue(all('-O' in command and 'cancel' in command for command in commands))
        self.assertFalse(any('exit' in command for command in commands))
        self.assertEqual({command[-2] for command in commands},
                         {'17796:127.0.0.1:7796', '18080:127.0.0.1:8080'})

    def test_partial_forward_cancel_stays_resumable_without_completion(self):
        args = SimpleNamespace(host='mike@100.64.23.14',
                               socket='/tmp/legacy-control.sock')
        saved = []
        connection = SimpleNamespace(atomic_json=lambda path, value: saved.append(dict(value)),
                                     record_connection_ownership=lambda state: self.fail(
                                         'completion must not be recorded'))
        outcomes = [SimpleNamespace(returncode=0), SimpleNamespace(returncode=255)]
        migration = launcher.migration_template(args, 7)
        migration['shutdown_confirmed'] = True
        with patch.object(launcher, 'local_forward_absent', return_value=True), \
             patch.object(launcher, 'remote_forward_absent', return_value=False), \
             patch.object(launcher.subprocess, 'run', side_effect=outcomes):
            with self.assertRaisesRegex(SystemExit, 'remains resumable'):
                launcher.cancel_known_old_forwards(connection, args, migration)
        self.assertTrue(saved[-1]['local_detached'])
        self.assertFalse(saved[-1]['remote_detached'])
        self.assertTrue(saved[-1]['shutdown_confirmed'])

    def test_crash_after_remote_shutdown_resumes_from_process_absence(self):
        args = SimpleNamespace(host='mike@100.64.23.14',
                               socket='/tmp/legacy-control.sock')
        saved_states = []
        connection = SimpleNamespace(
            authenticated_status=lambda runtime, config: None,
            atomic_json=lambda path, value: saved_states.append(dict(value)))
        migration = launcher.migration_template(args, 9)
        with patch.object(launcher, 'old_runner_absent', return_value=True), \
             patch.object(launcher, 'request_shutdown') as shutdown:
            result = launcher.confirm_migration_shutdown(
                connection, args, migration, {'token': 'stale'}, {'remote_root': 'C:/a'})
        shutdown.assert_not_called()
        self.assertTrue(result['shutdown_confirmed'])
        self.assertTrue(saved_states[-1]['shutdown_confirmed'])

    def test_local_forward_probe_allows_time_wait_but_rejects_live_listener(self):
        listener = socket.socket()
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(('127.0.0.1', 0))
        port = listener.getsockname()[1]
        listener.listen(1)
        self.assertFalse(launcher.local_forward_absent(port))

        client = socket.create_connection(('127.0.0.1', port))
        accepted, _ = listener.accept()
        accepted.shutdown(socket.SHUT_WR)
        accepted.close()
        self.assertEqual(client.recv(1), b'')
        client.close()
        listener.close()
        self.assertTrue(launcher.local_forward_absent(port))

    def test_supervisor_stop_waits_for_launchd_unload_and_socket_exit(self):
        with tempfile.TemporaryDirectory() as temp:
            socket_path = Path(temp) / 'dedicated.sock'
            socket_path.write_text('fixture')
            loaded = iter([True, True, False, False])
            connection = SimpleNamespace(
                LABEL='com.nobiletechnology.assemblywright.developer-connection',
                launchd_domain=lambda: 'gui/501',
                service_is_loaded=lambda: next(loaded),
                control_socket=lambda: socket_path)
            completed = SimpleNamespace(returncode=0)
            def finish_unload(_):
                socket_path.unlink(missing_ok=True)
            with patch.object(launcher.subprocess, 'run', return_value=completed) as run, \
                 patch.object(launcher.time, 'sleep', side_effect=finish_unload) as sleep:
                launcher.stop_supervisor(connection)
            self.assertEqual(run.call_args.args[0][1], 'bootout')
            self.assertTrue(sleep.called)

    def test_maintenance_lock_is_single_owner(self):
        with tempfile.TemporaryDirectory() as temp, patch.object(launcher, 'STATE', Path(temp)):
            os.chmod(temp, 0o700)
            with launcher.maintenance_lock():
                descriptor = Path(temp, 'connection-maintenance.lock').open('a+b')
                try:
                    import fcntl
                    with self.assertRaises(BlockingIOError):
                        fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
                finally:
                    descriptor.close()

    def test_durable_ownership_ignores_unrelated_legacy_master_after_stop(self):
        args = SimpleNamespace(host='mike@100.64.23.14',
                               socket='/tmp/unrelated-user-master.sock')
        connection = SimpleNamespace(has_connection_ownership=lambda state: True)
        with patch.object(launcher, 'old_socket_live') as old_socket:
            self.assertFalse(launcher.legacy_migration_candidate(
                connection, False, args))
            old_socket.assert_not_called()

    def test_first_install_still_detects_legacy_master(self):
        args = SimpleNamespace(host='mike@100.64.23.14',
                               socket='/tmp/legacy-user-master.sock')
        connection = SimpleNamespace(has_connection_ownership=lambda state: False)
        with patch.object(launcher, 'old_socket_live', return_value=True):
            self.assertTrue(launcher.legacy_migration_candidate(
                connection, False, args))

    def test_changed_host_preflight_failure_has_no_shutdown_side_effect(self):
        connection = SimpleNamespace(ssh_base=lambda config: ['/usr/bin/ssh', config['host']])
        failed = SimpleNamespace(returncode=255)
        with patch.object(launcher.subprocess, 'run', return_value=failed), \
             patch.object(launcher, 'request_shutdown') as shutdown, \
             patch.object(launcher, 'stop_supervisor') as stop:
            with self.assertRaisesRegex(SystemExit, 'No runner was stopped'):
                launcher.preflight_connection(connection, {'host': 'mike@unreachable.test'})
            shutdown.assert_not_called()
            stop.assert_not_called()


def _make_server(handler_cls):
    """Build a ThreadingHTTPServer bound to port 0, start serve_forever in a thread, return (server, port, thread)."""
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler_cls)
    port = server.socket.getsockname()[1]
    started = threading.Event()

    def _serve():
        started.set()
        server.serve_forever()

    thread = threading.Thread(target=_serve)
    thread.start()
    started.wait()
    return server, port, thread


class ShutdownAcknowledgementTests(unittest.TestCase):
    """Bounded shutdown acknowledgement behaviour."""

    def _runtime(self, port=None):
        return {'endpoint': f'http://127.0.0.1:{port}', 'token': 'test-token'}

    def _connection(self, status=None):
        return SimpleNamespace(authenticated_status=lambda runtime, config: status)

    def test_valid_acknowledgement_with_delayed_object_success(self):
        """Produce a real HTTP 200 with a JSON-object acknowledgement arriving after the former 5s deadline."""
        captured = []
        delay = 5.1

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                content_length = int(self.headers.get('Content-Length', 0))
                body = self.rfile.read(content_length)
                captured.append({
                    'path': self.path,
                    'method': self.command,
                    'headers': dict(self.headers),
                    'body': body,
                })
                time.sleep(delay)
                payload = json.dumps({'status': 'ok'}).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(payload)))
                self.send_header('Connection', 'close')
                self.end_headers()
                self.wfile.write(payload)

            def log_message(self, format, *args):
                pass

        server, port, thread = _make_server(Handler)
        try:
            self.assertIs(launcher.SHUTDOWN_ACK_TIMEOUT_SECONDS.__class__, int)
            self.assertEqual(launcher.SHUTDOWN_ACK_TIMEOUT_SECONDS, 15)
            conn = self._connection({'revision': 1})
            runtime = self._runtime(port)
            result = launcher.request_shutdown(conn, runtime, {})
            self.assertEqual(result, {'revision': 1})
            self.assertEqual(len(captured), 1)
            req = captured[0]
            self.assertEqual(req['path'], '/control')
            self.assertEqual(req['method'], 'POST')
            self.assertEqual(req['headers'].get('Content-Type'), 'application/json')
            self.assertEqual(req['headers'].get('Authorization'), 'Bearer test-token')
            self.assertEqual(req['body'], json.dumps({'action': 'shutdown'}).encode())
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive())

    def test_delayed_beyond_bounded_timeout_fails_closed(self):
        """Client timeout (0.05 s) fires before the server delay (0.15 s); assert unknown-outcome wording and exactly one request."""
        request_count = [0]

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                content_length = int(self.headers.get('Content-Length', 0))
                self.rfile.read(content_length)
                request_count[0] += 1
                time.sleep(0.15)
                try:
                    payload = json.dumps({'status': 'ok'}).encode()
                    self.send_response(200)
                    self.send_header('Content-Type', 'application/json')
                    self.send_header('Content-Length', str(len(payload)))
                    self.send_header('Connection', 'close')
                    self.end_headers()
                    self.wfile.write(payload)
                except (BrokenPipeError, ConnectionResetError):
                    pass

            def log_message(self, format, *args):
                pass

        server, port, thread = _make_server(Handler)
        try:
            conn = self._connection({'revision': 1})
            runtime = self._runtime(port)
            with patch.object(launcher, 'SHUTDOWN_ACK_TIMEOUT_SECONDS', 0.05):
                try:
                    launcher.request_shutdown(conn, runtime, {})
                    self.fail("SystemExit was not raised")
                except SystemExit as exc:
                    self.assertIn('acknowledgement is unavailable', str(exc))
                    self.assertIn('outcome is unknown', str(exc))
                    self.assertIn('connection supervisor was retained', str(exc))
            self.assertEqual(request_count[0], 1)
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive())

    def test_unavailable_status_rejects_before_any_post(self):
        status_mock = MagicMock(return_value=None)
        conn = SimpleNamespace(authenticated_status=status_mock)
        runtime = self._runtime(17796)
        with patch.object(launcher.urllib.request, 'urlopen') as request:
            with self.assertRaises(SystemExit) as ctx:
                launcher.request_shutdown(conn, runtime, {})
            self.assertIn('running connection state is unavailable', str(ctx.exception))
            self.assertIn('No process was stopped', str(ctx.exception))
            request.assert_not_called()
        status_mock.assert_called_once_with(runtime, {})

    def test_active_work_rejects_before_any_post(self):
        for key in ('running', 'chat_running', 'planning_running',
                     'escalation_running', 'tools_running',
                     'github_publication_running', 'github_setup_busy'):
            with self.subTest(key=key):
                conn = self._connection({key: True})
                runtime = self._runtime(17796)
                with patch.object(launcher.urllib.request, 'urlopen') as request:
                    with self.assertRaisesRegex(SystemExit, 'work to finish'):
                        launcher.request_shutdown(conn, runtime, {})
                    request.assert_not_called()

    def test_http_error_fails_with_not_verified(self):
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                content_length = int(self.headers.get('Content-Length', 0))
                self.rfile.read(content_length)
                self.send_response(500)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', '0')
                self.send_header('Connection', 'close')
                self.end_headers()

            def log_message(self, format, *args):
                pass

        server, port, thread = _make_server(Handler)
        try:
            conn = self._connection({'revision': 1})
            runtime = self._runtime(port)
            try:
                launcher.request_shutdown(conn, runtime, {})
                self.fail("SystemExit was not raised")
            except SystemExit as exc:
                self.assertIsInstance(exc.__cause__, launcher.urllib.error.HTTPError)
                with exc.__cause__:
                    self.assertIn('acknowledgement was not verified', str(exc))
                    self.assertIn('outcome is unknown', str(exc))
                    self.assertIn('connection supervisor was retained', str(exc))
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive())

    def test_http_409_fails_with_not_verified(self):
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                content_length = int(self.headers.get('Content-Length', 0))
                self.rfile.read(content_length)
                self.send_response(409)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', '0')
                self.send_header('Connection', 'close')
                self.end_headers()

            def log_message(self, format, *args):
                pass

        server, port, thread = _make_server(Handler)
        try:
            conn = self._connection({'revision': 1})
            runtime = self._runtime(port)
            try:
                launcher.request_shutdown(conn, runtime, {})
                self.fail("SystemExit was not raised")
            except SystemExit as exc:
                self.assertIsInstance(exc.__cause__, launcher.urllib.error.HTTPError)
                with exc.__cause__:
                    self.assertIn('acknowledgement was not verified', str(exc))
                    self.assertIn('outcome is unknown', str(exc))
                    self.assertIn('connection supervisor was retained', str(exc))
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive())

    def test_malformed_json_fails_with_not_verified(self):
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                content_length = int(self.headers.get('Content-Length', 0))
                self.rfile.read(content_length)
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(b'{invalid json}')))
                self.send_header('Connection', 'close')
                self.end_headers()
                self.wfile.write(b'{invalid json}')

            def log_message(self, format, *args):
                pass

        server, port, thread = _make_server(Handler)
        try:
            conn = self._connection({'revision': 1})
            runtime = self._runtime(port)
            try:
                launcher.request_shutdown(conn, runtime, {})
                self.fail("SystemExit was not raised")
            except SystemExit as exc:
                self.assertIn('acknowledgement was not verified', str(exc))
                self.assertIn('outcome is unknown', str(exc))
                self.assertIn('connection supervisor was retained', str(exc))
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive())

    def test_json_null_fails_with_not_verified(self):
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                content_length = int(self.headers.get('Content-Length', 0))
                self.rfile.read(content_length)
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(b'null')))
                self.send_header('Connection', 'close')
                self.end_headers()
                self.wfile.write(b'null')

            def log_message(self, format, *args):
                pass

        server, port, thread = _make_server(Handler)
        try:
            conn = self._connection({'revision': 1})
            runtime = self._runtime(port)
            try:
                launcher.request_shutdown(conn, runtime, {})
                self.fail("SystemExit was not raised")
            except SystemExit as exc:
                self.assertIn('acknowledgement was not verified', str(exc))
                self.assertIn('outcome is unknown', str(exc))
                self.assertIn('connection supervisor was retained', str(exc))
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive())

    def test_json_array_fails_with_not_verified(self):
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                content_length = int(self.headers.get('Content-Length', 0))
                self.rfile.read(content_length)
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(b'[1, 2, 3]')))
                self.send_header('Connection', 'close')
                self.end_headers()
                self.wfile.write(b'[1, 2, 3]')

            def log_message(self, format, *args):
                pass

        server, port, thread = _make_server(Handler)
        try:
            conn = self._connection({'revision': 1})
            runtime = self._runtime(port)
            try:
                launcher.request_shutdown(conn, runtime, {})
                self.fail("SystemExit was not raised")
            except SystemExit as exc:
                self.assertIn('acknowledgement was not verified', str(exc))
                self.assertIn('outcome is unknown', str(exc))
                self.assertIn('connection supervisor was retained', str(exc))
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive())

    def test_json_string_fails_with_not_verified(self):
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                content_length = int(self.headers.get('Content-Length', 0))
                self.rfile.read(content_length)
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(b'"ok"')))
                self.send_header('Connection', 'close')
                self.end_headers()
                self.wfile.write(b'"ok"')

            def log_message(self, format, *args):
                pass

        server, port, thread = _make_server(Handler)
        try:
            conn = self._connection({'revision': 1})
            runtime = self._runtime(port)
            try:
                launcher.request_shutdown(conn, runtime, {})
                self.fail("SystemExit was not raised")
            except SystemExit as exc:
                self.assertIn('acknowledgement was not verified', str(exc))
                self.assertIn('outcome is unknown', str(exc))
                self.assertIn('connection supervisor was retained', str(exc))
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive())

    def test_json_number_fails_with_not_verified(self):
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                content_length = int(self.headers.get('Content-Length', 0))
                self.rfile.read(content_length)
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(b'42')))
                self.send_header('Connection', 'close')
                self.end_headers()
                self.wfile.write(b'42')

            def log_message(self, format, *args):
                pass

        server, port, thread = _make_server(Handler)
        try:
            conn = self._connection({'revision': 1})
            runtime = self._runtime(port)
            try:
                launcher.request_shutdown(conn, runtime, {})
                self.fail("SystemExit was not raised")
            except SystemExit as exc:
                self.assertIn('acknowledgement was not verified', str(exc))
                self.assertIn('outcome is unknown', str(exc))
                self.assertIn('connection supervisor was retained', str(exc))
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive())

    def test_stop_supervisor_retained_when_ack_unverified(self):
        """Exercise main's disruptive build path regression: argv-driven --build/--no-open, real maintenance_lock, acknowledgement failure, supervisor retained."""
        with tempfile.TemporaryDirectory() as temp:
            state = Path(temp)
            (state / 'connection.json').write_text('{}')
            os.chmod(str(state), 0o700)

            saved = {'endpoint': 'http://127.0.0.1:17796', 'token': 'a' * 64,
                     'review_codex_executable': 'C:/tools/@openai/codex.exe',
                     'review_codex_home': 'C:/Users/mike/.codex'}
            existing_config = {'host': 'fixture-host'}

            fake_connection = SimpleNamespace(
                direct_private_directory=lambda *a, **kw: None,
                validate_config=lambda s: existing_config,
                validate_config_payload=lambda *a: existing_config,
                service_is_loaded=lambda: True,
            )
            install_mock = MagicMock()
            fake_connection.install = install_mock

            with ExitStack() as stack:
                stack.enter_context(patch.object(launcher, 'STATE', state))
                stack.enter_context(patch.object(launcher.sys, 'argv', ['developer-build.py', '--build', '--no-open']))
                stack.enter_context(patch.object(launcher, '_connection_module', return_value=fake_connection))
                stack.enter_context(patch.object(launcher, 'load_saved_runtime', return_value=saved))
                stack.enter_context(patch.object(launcher, 'authenticated_status', return_value={'revision': 1}))
                stack.enter_context(patch.object(launcher, 'load_migration', return_value=None))
                stack.enter_context(patch.object(launcher, 'legacy_migration_candidate', return_value=False))
                stack.enter_context(patch.object(launcher, 'request_shutdown', side_effect=SystemExit('The shutdown acknowledgement was not verified, the outcome is unknown, and the connection supervisor was retained.')))
                stop_mock = stack.enter_context(patch.object(launcher, 'stop_supervisor'))
                write_config = stack.enter_context(patch.object(launcher, 'write_connection_config'))
                write_runtime = stack.enter_context(patch.object(launcher, 'write_runtime'))
                build = stack.enter_context(patch.object(launcher, 'build_products'))
                start_model = stack.enter_context(patch.object(launcher, 'start_local_model'))
                wait = stack.enter_context(patch.object(launcher, 'wait_for_connected'))
                stack.enter_context(patch.object(launcher, 'preflight_connection', return_value=None))

                with self.assertRaises(SystemExit) as exc:
                    launcher.main()
                self.assertEqual(exc.exception.args[0],
                    'The shutdown acknowledgement was not verified, the outcome is unknown, and the connection supervisor was retained.')

                launcher.request_shutdown.assert_called_once_with(fake_connection, saved, existing_config)
                stop_mock.assert_not_called()
                write_config.assert_not_called()
                write_runtime.assert_not_called()
                build.assert_not_called()
                start_model.assert_not_called()
                wait.assert_not_called()
                install_mock.assert_not_called()


if __name__ == '__main__':
    unittest.main()
