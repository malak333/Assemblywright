#!/usr/bin/env python3
"""Disposable native HTTP/process coverage; the model response is a labeled fixture."""
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    args = parser.parse_args()
    calls = []

    class Model(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            calls.append(request)
            content = json.dumps({'files': [{'path': 'result.txt', 'content': 'real file from fixture model\n'}]})
            if len(calls) == 2:
                content = '```json\n' + content + '\n```'
            if 'Fixture malformed-response boundary' in request['messages'][1]['content']:
                content = 'invalid JSON fixture'
            body = json.dumps({'choices': [{'message': {'content': content}}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *unused):
            pass

    model = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    threading.Thread(target=model.serve_forever, daemon=True).start()
    with socket.socket() as reservation:
        reservation.bind(('127.0.0.1', 0))
        port = reservation.getsockname()[1]
    with tempfile.TemporaryDirectory(prefix='assemblywright-developer-e2e-') as temp:
        root = Path(temp)
        data = root / 'state'
        projects = root / 'projects'
        data.mkdir()
        legacy_state = json.dumps({'revision': 0, 'auto_run': True, 'emergency_paused': False, 'queue': []}, separators=(',', ':'))
        with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
            database.execute('CREATE TABLE developer_state(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL)')
            database.execute('INSERT INTO developer_state(id,state) VALUES(1,?)', (legacy_state,))
            database.commit()
        command = [str(Path(args.binary).resolve()), '--data-dir', str(data), '--workspace-root', str(projects), '--bind', f'127.0.0.1:{port}', '--model-url', f'http://127.0.0.1:{model.server_port}/v1']
        command += reviewer_arguments(root)
        output = (root / 'runner.log').open('wb')
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=output, stderr=output)
        token = ''

        def call(action=None, **values):
            if action == 'enqueue':
                return enqueue_with_plan(f'http://127.0.0.1:{port}', token, values)
            if action in ('start', 'resume') and 'expected_feature_id' not in values:
                current = call()
                feature = next(f for f in current['queue'] if f['status'] not in ('succeeded', 'removed'))
                values.update(expected_feature_id=feature['id'], expected_model_target=feature['model_target'],
                              expected_status=feature['status'], expected_checkpoint=feature['checkpoint'])
            body = json.dumps(dict(action=action, **values)).encode() if action else None
            req = urllib.request.Request(f'http://127.0.0.1:{port}/' + ('control' if action else 'status'), data=body, headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'})
            return json.load(urllib.request.urlopen(req, timeout=5))

        def wait(predicate, timeout=20):
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                try:
                    snapshot = call()
                    if predicate(snapshot):
                        return snapshot
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(.05)
            raise AssertionError('Timed out: ' + json.dumps(locals().get('snapshot')) + '\n' + (root / 'runner.log').read_text(errors='replace'))

        try:
            deadline = time.monotonic() + 15
            while not (data / 'developer-token').exists() and time.monotonic() < deadline:
                time.sleep(.05)
            token = (data / 'developer-token').read_text().strip()
            wait(lambda s: not s['running'])
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{port}/status', timeout=3)
                raise AssertionError('Unauthenticated status accepted')
            except urllib.error.HTTPError as error:
                assert error.code == 401
            python = '"' + sys.executable + '"'
            validation = python + ' -c "import time; from pathlib import Path; time.sleep(2); assert Path(\'result.txt\').is_file()"'
            first_id = str(uuid.uuid4())
            feature = dict(id=first_id, project='first', instruction='Create a fixture result file', validation=validation)
            call('enqueue', **feature)
            assert len(call('enqueue', **feature)['queue']) == 1
            try:
                call('enqueue', **dict(feature, instruction='different request'))
                raise AssertionError('Changed duplicate request accepted')
            except urllib.error.HTTPError as error:
                assert error.code == 409
            for project in ['second', 'third']:
                feature_id = str(uuid.uuid4())
                call('enqueue', id=feature_id, project=project, instruction='Create a fixture result file', validation=python + ' -c "from pathlib import Path; assert Path(\'result.txt\').is_file()"')
                if project == 'second':
                    second_id = feature_id
            call('auto_run', enabled=False)
            call('start')
            wait(lambda s: s['queue'][0]['checkpoint'] == 'applied' and s['running'])
            modified = (projects / 'first/result.txt').stat().st_mtime_ns
            for feature_id in [first_id, second_id]:
                try:
                    call('remove', id=feature_id)
                    raise AssertionError('Removal accepted while runner was active')
                except urllib.error.HTTPError as error:
                    assert error.code == 409
            started = time.monotonic()
            call('stop')
            paused = wait(lambda s: not s['running'], 5)
            assert paused['queue'][0]['status'] == 'paused'
            assert paused['queue'][0]['checkpoint'] == 'applied'
            stop_seconds = time.monotonic() - started
            call('resume')
            wait(lambda s: s['running'])
            call('emergency')
            paused = wait(lambda s: not s['running'], 5)
            assert paused['emergency_paused']
            try:
                call('resume')
                raise AssertionError('Resume ignored Emergency Pause')
            except urllib.error.HTTPError as error:
                assert error.code == 409
            call('clear_emergency')
            call('resume')
            completed = wait(lambda s: not s['running'] and s['queue'][0]['status'] == 'succeeded')
            assert completed['queue'][1]['status'] == 'queued', completed
            assert len(calls) == 1, 'Resume repeated model generation'
            assert (projects / 'first/result.txt').stat().st_mtime_ns == modified, 'Resume rewrote applied files'
            call('auto_run', enabled=True)
            call('start')
            completed = wait(lambda s: not s['running'] and all(f['status'] == 'succeeded' for f in s['queue']))
            assert len(calls) == 3
            try:
                call('remove', id=first_id)
                raise AssertionError('Completed feature removal accepted')
            except urllib.error.HTTPError as error:
                assert error.code == 409
            for invalid_id in ['not-a-uuid', str(uuid.uuid4())]:
                try:
                    call('remove', id=invalid_id)
                    raise AssertionError('Invalid removal target accepted')
                except urllib.error.HTTPError as error:
                    assert error.code == 409
            malformed_id = str(uuid.uuid4())
            blocked_id = str(uuid.uuid4())
            call('enqueue', id=malformed_id, project='malformed', instruction='Fixture malformed-response boundary', validation=python + ' -c "raise SystemExit(0)"')
            call('enqueue', id=blocked_id, project='blocked-by-malformed', instruction='Create a fixture result file', validation=python + ' -c "from pathlib import Path; assert Path(\'result.txt\').is_file()"')
            call('start')
            malformed = wait(lambda s: not s['running'] and s['queue'][3]['status'] == 'failed')
            assert malformed['queue'][4]['status'] == 'queued'
            assert not (projects / 'malformed/result.txt').exists()
            assert len(calls) == 4
            call('remove', id=malformed_id)
            call('start')
            wait(lambda s: not s['running'] and s['queue'][-1]['id'] == blocked_id and s['queue'][-1]['status'] == 'succeeded')
            assert len(calls) == 5
            failed_id = str(uuid.uuid4())
            waiting_id = str(uuid.uuid4())
            call('enqueue', id=failed_id, project='failed-applied', instruction='Create a fixture file before validation fails', validation=python + ' -c "raise SystemExit(7)"')
            call('enqueue', id=waiting_id, project='must-wait', instruction='Create a fixture result file', validation=python + ' -c "from pathlib import Path; assert Path(\'result.txt\').is_file()"')
            call('start')
            failed = wait(lambda s: not s['running'] and s['queue'][4]['status'] == 'failed')
            assert failed['queue'][5]['status'] == 'queued'
            assert len(calls) == 6
            failed_file = projects / 'failed-applied/result.txt'
            assert failed_file.read_text() == 'real file from fixture model\n'
            try:
                request = urllib.request.Request(f'http://127.0.0.1:{port}/control', data=json.dumps({'action': 'remove', 'id': failed_id}).encode(), headers={'Content-Type': 'application/json'})
                urllib.request.urlopen(request, timeout=5)
                raise AssertionError('Unauthenticated removal accepted')
            except urllib.error.HTTPError as error:
                assert error.code == 401
            removed = call('remove', id=failed_id)
            assert failed_id not in [feature['id'] for feature in removed['queue']]
            assert waiting_id in [feature['id'] for feature in removed['queue']]
            call('remove', id=failed_id)
            assert failed_file.read_text() == 'real file from fixture model\n'
            process.terminate()
            process.wait(timeout=5)
            process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=output, stderr=output)
            restored = wait(lambda s: len(s['queue']) == 5)
            assert [f['status'] for f in restored['queue']] == ['succeeded'] * 4 + ['queued']
            call('remove', id=failed_id)
            assert not restored['running']
            with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
                durable = json.loads(database.execute('SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                legacy_backup = database.execute('SELECT state FROM developer_state_v1_backup WHERE id=1').fetchone()[0]
            assert legacy_backup == legacy_state
            assert 'queue' not in durable
            assert 'queue_v2' not in durable
            assert 'queue_v3' not in durable
            tombstone = next(feature for feature in durable['queue_v10'] if feature['id'] == failed_id)
            assert all(feature['model_target'] == 'mac' for feature in durable['queue_v10'])
            assert tombstone['status'] == 'removed'
            assert tombstone['checkpoint'] == 'applied'
            assert tombstone['edits'][0]['path'] == 'result.txt'
            assert 'Validation failed' in tombstone['message']
            call('start')
            advanced = wait(lambda s: not s['running'] and s['queue'][-1]['id'] == waiting_id and s['queue'][-1]['status'] == 'succeeded')
            assert failed_id not in [feature['id'] for feature in advanced['queue']]
            assert failed_file.read_text() == 'real file from fixture model\n'
            assert len(calls) == 7
            print(json.dumps({'native_platform': sys.platform, 'stop_seconds': round(stop_seconds, 3), 'checkpoint_resume_no_rewrite_or_replanning': True, 'emergency_during_validation': True, 'auto_run_off_waits': True, 'auto_run_on_advances': True, 'restart_preserves_results': True, 'malformed_output_blocks_advancement': True, 'remove_failed_advances_queue': True, 'remove_preserves_files_and_evidence': True, 'remove_persists_across_restart': True, 'remove_rejected_while_running': True, 'model_calls': len(calls)}))
        finally:
            try:
                call('emergency')
                wait(lambda s: not s['running'], 5)
            except Exception:
                pass
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
            output.close()
    model.shutdown()


if __name__ == '__main__':
    main()
