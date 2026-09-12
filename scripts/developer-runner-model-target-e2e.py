#!/usr/bin/env python3
"""Native process/HTTP coverage for immutable per-feature local-model targets."""
from developer_review_fixture import reviewer_arguments
from developer_planning_fixture import enqueue_with_plan

import argparse
from contextlib import closing
import http.server
import json
from pathlib import Path
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
import uuid


def reserve_port():
    with socket.socket() as reservation:
        reservation.bind(('127.0.0.1', 0))
        return reservation.getsockname()[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    args = parser.parse_args()
    calls = {'mac': [], 'windows': []}

    def handler(label):
        class Model(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                calls[label].append(request)
                prompt = request['messages'][1]['content']
                if 'repair-windows' in prompt:
                    value = '1' if 'Repair attempt:' in prompt else '0'
                    content = f'VALUE = {value}\n'
                else:
                    content = f'ORIGIN = {label!r}\n'
                body = json.dumps({'choices': [{'message': {'content': json.dumps({
                    'files': [{'path': 'app.py', 'content': content}],
                })}, 'finish_reason': 'stop'}]}).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *unused):
                pass

        return Model

    mac = http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler('mac'))
    windows = http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler('windows'))
    threading.Thread(target=mac.serve_forever, daemon=True).start()
    threading.Thread(target=windows.serve_forever, daemon=True).start()

    with tempfile.TemporaryDirectory(prefix='assemblywright-model-target-e2e-') as temp:
        root = Path(temp)
        projects = root / 'projects'
        data = root / 'state'
        data.mkdir()
        legacy_v3 = json.dumps({
            'revision': 7,
            'auto_run': True,
            'emergency_paused': False,
            'queue_v3': [],
        }, separators=(',', ':'))
        with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
            database.execute('CREATE TABLE developer_state(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL)')
            database.execute('INSERT INTO developer_state(id,state) VALUES(1,?)', (legacy_v3,))
            database.commit()

        port = reserve_port()

        def command(port_value, state=data, workspace=projects, windows_url=None):
            result = [
                str(Path(args.binary).resolve()), '--data-dir', str(state),
                '--workspace-root', str(workspace), '--bind', f'127.0.0.1:{port_value}',
                '--model-url', f'http://127.0.0.1:{mac.server_port}/v1',
                '--model', 'mac-fixture',
            ]
            if windows_url is not None:
                result += ['--windows-model-url', windows_url, '--windows-model', 'windows-fixture']
            return result + reviewer_arguments(root)

        output = (root / 'runner.log').open('wb')
        process = subprocess.Popen(
            command(port, windows_url=f'http://127.0.0.1:{windows.server_port}/v1'),
            stdin=subprocess.DEVNULL, stdout=output, stderr=output,
        )
        token = ''

        def call(action=None, **values):
            if action == 'enqueue':
                return enqueue_with_plan(f'http://127.0.0.1:{port}', token, values)
            body = json.dumps(dict(action=action, **values)).encode() if action else None
            request = urllib.request.Request(
                f'http://127.0.0.1:{port}/' + ('control' if action else 'status'),
                data=body,
                headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'},
            )
            return json.load(urllib.request.urlopen(request, timeout=5))

        def rejected(action, **values):
            try:
                call(action, **values)
                raise AssertionError(f'{action} unexpectedly succeeded')
            except urllib.error.HTTPError as error:
                assert error.code == 409, error.code

        def wait(predicate, timeout=20):
            deadline = time.monotonic() + timeout
            snapshot = None
            while time.monotonic() < deadline:
                try:
                    snapshot = call()
                    if predicate(snapshot):
                        return snapshot
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(.05)
            raise AssertionError('Timed out: ' + json.dumps(snapshot) + '\n' + (root / 'runner.log').read_text(errors='replace'))

        def await_token(state=data):
            deadline = time.monotonic() + 15
            while not (state / 'developer-token').exists() and time.monotonic() < deadline:
                time.sleep(.05)
            return (state / 'developer-token').read_text().strip()

        def restart(current_command):
            nonlocal process, port
            process.terminate()
            process.wait(timeout=5)
            port = reserve_port()
            process = subprocess.Popen(
                current_command(port), stdin=subprocess.DEVNULL, stdout=output, stderr=output,
            )
            return wait(lambda snapshot: not snapshot['running'])

        try:
            token = await_token()
            initial = wait(lambda snapshot: not snapshot['running'])
            assert initial['model_targets'] == [
                {'id': 'mac', 'name': 'Mac', 'model': 'mac-fixture'},
                {'id': 'windows', 'name': 'Windows', 'model': 'windows-fixture'},
            ]
            python = '"' + sys.executable + '"'

            def binding(feature_id, target, status='queued', checkpoint='not_started'):
                return {
                    'expected_feature_id': feature_id,
                    'expected_model_target': target,
                    'expected_status': status,
                    'expected_checkpoint': checkpoint,
                }

            stale_mac_id = str(uuid.uuid4())
            stale_windows_id = str(uuid.uuid4())
            call('enqueue', id=stale_mac_id, project='stale-mac', instruction='stale-mac fixture',
                 validation=python + ' -B -c "raise SystemExit(0)"')
            call('enqueue', id=stale_windows_id, project='stale-windows',
                 instruction='stale-windows fixture',
                 validation=python + ' -B -c "raise SystemExit(0)"', model_target='windows')
            call('remove', id=stale_mac_id)
            rejected('start', **binding(stale_mac_id, 'mac'))
            stale = call()
            assert len(stale['queue']) == 1
            assert stale['queue'][0]['id'] == stale_windows_id
            assert stale['queue'][0]['status'] == 'queued'
            assert not calls['mac'] and not calls['windows']
            rejected('start')
            call('remove', id=stale_windows_id)

            mac_id = str(uuid.uuid4())
            mac_feature = dict(
                id=mac_id, project='choice-mac', instruction='choice-mac fixture',
                validation=python + ' -B -c "import app; assert app.ORIGIN == \'mac\'"',
            )
            mac_enqueued = call('enqueue', **mac_feature)
            assert mac_enqueued['queue'][-1]['model_target'] == 'mac'
            assert len(call('enqueue', **mac_feature)['queue']) == 1
            rejected('enqueue', **dict(mac_feature, model_target='windows'))
            rejected('enqueue', id=str(uuid.uuid4()), project='invalid', instruction='invalid target',
                     validation=python + ' -B -c "raise SystemExit(0)"', model_target='other')

            windows_id = str(uuid.uuid4())
            call('enqueue', id=windows_id, project='choice-windows', instruction='choice-windows fixture',
                 validation=python + ' -B -c "import app; assert app.ORIGIN == \'windows\'"',
                 model_target='windows')
            repair_id = str(uuid.uuid4())
            call('enqueue', id=repair_id, project='repair-windows', instruction='repair-windows fixture',
                 validation=python + ' -B -c "import app; assert app.VALUE == 1"',
                 model_target='windows')
            rejected('start', expected_feature_id=mac_id)
            rejected('start')
            call('start', **binding(mac_id, 'mac'))
            failed = wait(lambda snapshot: not snapshot['running'] and snapshot['queue'][-1]['status'] == 'failed')
            assert [feature['status'] for feature in failed['queue']] == ['succeeded', 'succeeded', 'failed']
            assert len(calls['mac']) == 1 and len(calls['windows']) == 2
            assert calls['mac'][0]['model'] == 'mac-fixture'
            assert all(request['model'] == 'windows-fixture' for request in calls['windows'])
            call('repair', id=repair_id, expected_attempts=0)
            repaired = wait(lambda snapshot: not snapshot['running'] and snapshot['queue'][-1]['status'] == 'succeeded')
            assert repaired['queue'][-1]['model_target'] == 'windows'
            assert repaired['queue'][-1]['repair_attempts'] == 1
            assert len(calls['mac']) == 1 and len(calls['windows']) == 3

            call('auto_run', enabled=False)
            persisted_id = str(uuid.uuid4())
            call('enqueue', id=persisted_id, project='restart-windows', instruction='restart-windows fixture',
                 validation=python + ' -B -c "import time, app; time.sleep(2); assert app.ORIGIN == \'windows\'"',
                 model_target='windows')
            restarted = restart(lambda new_port: command(
                new_port, windows_url=f'http://127.0.0.1:{windows.server_port}/v1'))
            persisted = next(feature for feature in restarted['queue'] if feature['id'] == persisted_id)
            assert persisted['model_target'] == 'windows' and persisted['status'] == 'queued'
            call('start', **binding(persisted_id, 'windows'))
            wait(lambda snapshot: snapshot['running'] and snapshot['queue'][-1]['checkpoint'] == 'applied')
            call('stop')
            paused = wait(lambda snapshot: not snapshot['running'] and snapshot['queue'][-1]['status'] == 'paused')
            assert paused['queue'][-1]['checkpoint'] == 'applied'
            rejected('resume', **binding(persisted_id, 'windows', 'running', 'applied'))
            assert call()['queue'][-1]['status'] == 'paused'
            call('resume', **binding(persisted_id, 'windows', 'paused', 'applied'))
            wait(lambda snapshot: not snapshot['running'] and snapshot['queue'][-1]['status'] == 'succeeded')
            assert len(calls['mac']) == 1 and len(calls['windows']) == 4
            with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
                durable = json.loads(database.execute('SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                backup_v3 = database.execute('SELECT state FROM developer_state_v3_backup WHERE id=1').fetchone()[0]
            assert backup_v3 == legacy_v3
            assert 'queue_v10' in durable and 'queue_v4' not in durable and 'queue_v3' not in durable
            assert next(feature for feature in durable['queue_v10'] if feature['id'] == persisted_id)['model_target'] == 'windows'

            process.terminate()
            process.wait(timeout=5)

            offline_data = root / 'offline-state'
            offline_projects = root / 'offline-projects'
            offline_data.mkdir()
            offline_port = reserve_port()
            unreachable_port = reserve_port()
            port = offline_port
            process = subprocess.Popen(
                command(port, offline_data, offline_projects,
                        f'http://127.0.0.1:{unreachable_port}/v1'),
                stdin=subprocess.DEVNULL, stdout=output, stderr=output,
            )
            token = await_token(offline_data)
            wait(lambda snapshot: not snapshot['running'])
            before = {key: len(value) for key, value in calls.items()}
            offline_id = str(uuid.uuid4())
            call('enqueue', id=offline_id, project='offline-windows', instruction='offline-windows fixture',
                 validation=python + ' -B -c "raise SystemExit(0)"', model_target='windows')
            call('start', **binding(offline_id, 'windows'))
            offline = wait(lambda snapshot: not snapshot['running'] and snapshot['queue'][0]['status'] == 'failed')
            assert 'Windows model target request failed' in offline['queue'][0]['message']
            assert {key: len(value) for key, value in calls.items()} == before
            process.terminate()
            process.wait(timeout=5)

            absent_data = root / 'absent-state'
            absent_projects = root / 'absent-projects'
            absent_data.mkdir()
            absent_id = str(uuid.uuid4())
            absent_state = json.dumps({
                'revision': 1, 'auto_run': True, 'emergency_paused': False,
                'queue_v4': [{
                    'id': absent_id, 'project': 'absent-windows',
                    'instruction': 'absent-windows fixture',
                    'validation': python + ' -B -c "raise SystemExit(0)"',
                    'status': 'queued', 'checkpoint': 'not_started', 'message': 'Ready to start',
                    'edits': None, 'repair_attempts': 0, 'repair_pending': False,
                    'repair_history': [], 'model_target': 'windows',
                }],
            }, separators=(',', ':'))
            with closing(sqlite3.connect(absent_data / 'developer.sqlite3')) as database:
                database.execute('CREATE TABLE developer_state(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL)')
                database.execute('INSERT INTO developer_state(id,state) VALUES(1,?)', (absent_state,))
                database.commit()
            port = reserve_port()
            process = subprocess.Popen(
                command(port, absent_data, absent_projects),
                stdin=subprocess.DEVNULL, stdout=output, stderr=output,
            )
            token = await_token(absent_data)
            absent = wait(lambda snapshot: not snapshot['running'])
            assert absent['model_targets'] == [{'id': 'mac', 'name': 'Mac', 'model': 'mac-fixture'}]
            assert absent['queue'][0]['model_target'] == 'windows'
            rejected('enqueue', id=str(uuid.uuid4()), project='unconfigured-windows',
                     instruction='unconfigured-windows fixture',
                     validation=python + ' -B -c "raise SystemExit(0)"', model_target='windows')
            before_mac = len(calls['mac'])
            call('start', **binding(absent_id, 'windows'))
            absent = wait(lambda snapshot: not snapshot['running'] and snapshot['queue'][0]['status'] == 'failed')
            assert 'Windows model target is unavailable because this runner was not started with its model configuration' in absent['queue'][0]['message']
            assert len(calls['mac']) == before_mac
            rejected('repair', id=absent_id, expected_attempts=0)
            absent = call()
            assert absent['queue'][0]['repair_attempts'] == 0
            assert len(calls['mac']) == before_mac

            print(json.dumps({
                'native_platform': sys.platform,
                'mac_and_windows_endpoints_selected_per_feature': True,
                'repair_retains_windows_target': True,
                'offline_target_has_no_fallback': True,
                'unconfigured_persisted_target_has_no_fallback': True,
                'unconfigured_repair_rejected_before_attempt': True,
                'invalid_target_and_id_reuse_rejected': True,
                'stale_and_unbound_start_rejected': True,
                'stale_pre_stop_resume_rejected': True,
                'queue_v3_migrated_with_exact_backup': True,
                'queue_v10_restart_preserves_target': True,
            }))
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
            output.close()
    mac.shutdown()
    windows.shutdown()


if __name__ == '__main__':
    main()
