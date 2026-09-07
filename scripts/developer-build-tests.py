#!/usr/bin/env python3
"""Focused launcher configuration checks; never starts SSH or a model."""
import importlib.util
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import tempfile
import json
from io import BytesIO
import os
import socket

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


if __name__ == '__main__':
    unittest.main()
