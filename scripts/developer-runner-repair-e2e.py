#!/usr/bin/env python3
"""Native HTTP/process coverage for bounded developer-runner repair attempts."""
from developer_review_fixture import reviewer_arguments
from developer_planning_fixture import enqueue_with_plan

import argparse
from contextlib import closing
import http.server
import json
from pathlib import Path
import re
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
    repair_calls = {}
    stop_request_started = threading.Event()
    owner_request_started = threading.Event()
    owner_response_released = threading.Event()

    class Model(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            calls.append(request)
            prompt = request['messages'][1]['content']
            attempt_match = re.search(r'Repair attempt: (\d+) of 3', prompt)
            attempt = int(attempt_match.group(1)) if attempt_match else 0
            marker = next(name for name in [
                'loop-success', 'exhaust-three', 'stop-model',
                'emergency-validation', 'restart-applied', 'owner-conflict',
                'malicious-test', 'malicious-directory'
            ] if name in prompt)
            if attempt:
                repair_calls[marker] = repair_calls.get(marker, 0) + 1
            value = 0
            if marker == 'loop-success' and attempt:
                if attempt == 1:
                    time.sleep(.5)
                value = attempt
            elif marker == 'exhaust-three' and attempt:
                value = attempt
            elif marker in ['stop-model', 'emergency-validation', 'restart-applied'] and attempt:
                value = 1
                if marker == 'stop-model' and repair_calls[marker] == 1:
                    stop_request_started.set()
                    time.sleep(3)
            elif marker == 'owner-conflict' and attempt:
                owner_request_started.set()
                owner_response_released.wait(5)
                value = 1
            elif marker == 'malicious-test' and attempt:
                value = 1
            elif marker == 'malicious-directory' and attempt:
                value = 1
            files = [{'path': 'app.py', 'content': f'VALUE = {value}\n'}]
            if marker == 'malicious-test':
                files.append({
                    'path': 'tests/test_guard.py',
                    'content': 'ASSERTION = False\n' if attempt else 'ASSERTION = True\n',
                })
            if marker == 'malicious-directory':
                files.append({
                    'path': 'checks/arbitrary.py',
                    'content': 'EXPECTED = 0\n' if attempt else 'EXPECTED = 1\n',
                })
            content = json.dumps({'files': files})
            body = json.dumps({'choices': [{'message': {'content': content}, 'finish_reason': 'stop'}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                pass

        def log_message(self, *unused):
            pass

    model = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    threading.Thread(target=model.serve_forever, daemon=True).start()
    with socket.socket() as reservation:
        reservation.bind(('127.0.0.1', 0))
        port = reservation.getsockname()[1]

    with tempfile.TemporaryDirectory(prefix='assemblywright-developer-repair-e2e-') as temp:
        root = Path(temp)
        data = root / 'state'
        projects = root / 'projects'
        data.mkdir()
        legacy_v2 = json.dumps({'revision': 4, 'auto_run': False, 'emergency_paused': False, 'queue_v2': []}, separators=(',', ':'))
        with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
            database.execute('CREATE TABLE developer_state(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL)')
            database.execute('INSERT INTO developer_state(id,state) VALUES(1,?)', (legacy_v2,))
            database.commit()
        def runner_command(port_value):
            return [str(Path(args.binary).resolve()), '--data-dir', str(data), '--workspace-root', str(projects), '--bind', f'127.0.0.1:{port_value}', '--model-url', f'http://127.0.0.1:{model.server_port}/v1'] + reviewer_arguments(root)

        command = runner_command(port)
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
            req = urllib.request.Request(
                f'http://127.0.0.1:{port}/' + ('control' if action else 'status'),
                data=body,
                headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'},
            )
            return json.load(urllib.request.urlopen(req, timeout=5))

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

        def enqueue_failed(marker, expected, slow=False):
            feature_id = str(uuid.uuid4())
            sleep = 'import time; time.sleep(5); ' if slow else ''
            validation = f'"{sys.executable}" -B -c "{sleep}import app; assert app.VALUE == {expected}"'
            call('enqueue', id=feature_id, project=marker, instruction=f'Repair fixture {marker}', validation=validation)
            call('start')
            failed = wait(lambda s: not s['running'] and s['queue'][-1]['id'] == feature_id and s['queue'][-1]['status'] == 'failed')
            assert failed['queue'][-1]['repair_attempts'] == 0
            assert failed['queue'][-1]['validation'] == validation
            return feature_id, validation

        def restart():
            nonlocal command, port, process
            process.terminate()
            process.wait(timeout=5)
            with socket.socket() as restart_reservation:
                restart_reservation.bind(('127.0.0.1', 0))
                port = restart_reservation.getsockname()[1]
            command = runner_command(port)
            process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=output, stderr=output)
            return wait(lambda s: not s['running'])

        try:
            deadline = time.monotonic() + 15
            while not (data / 'developer-token').exists() and time.monotonic() < deadline:
                time.sleep(.05)
            token = (data / 'developer-token').read_text().strip()
            initial = wait(lambda s: not s['running'])
            assert initial['repair_limit'] == 3
            assert not initial['repair_active']

            loop_id, loop_validation = enqueue_failed('loop-success', 2)
            unauthenticated = urllib.request.Request(
                f'http://127.0.0.1:{port}/control',
                data=json.dumps({'action': 'repair', 'id': loop_id, 'expected_attempts': 0}).encode(),
                headers={'Content-Type': 'application/json'},
            )
            try:
                urllib.request.urlopen(unauthenticated, timeout=5)
                raise AssertionError('Unauthenticated repair accepted')
            except urllib.error.HTTPError as error:
                assert error.code == 401
            admitted = call('repair', id=loop_id, expected_attempts=0)
            assert admitted['repair_active']
            rejected('repair', id=loop_id, expected_attempts=0)
            repaired = wait(lambda s: not s['running'] and s['queue'][-1]['id'] == loop_id and s['queue'][-1]['status'] == 'succeeded')
            assert repaired['queue'][-1]['repair_attempts'] == 2
            assert repaired['queue'][-1]['validation'] == loop_validation
            rejected('repair', id=loop_id, expected_attempts=0)

            exhausted_id, exhausted_validation = enqueue_failed('exhaust-three', 9)
            call('repair', id=exhausted_id, expected_attempts=0)
            exhausted = wait(lambda s: not s['running'] and s['queue'][-1]['id'] == exhausted_id and s['queue'][-1]['status'] == 'failed' and s['queue'][-1]['repair_attempts'] == 3)
            assert exhausted['queue'][-1]['validation'] == exhausted_validation
            assert exhausted['queue'][-1]['message'].startswith('Repair stopped after 3 attempts.')
            rejected('repair', id=exhausted_id, expected_attempts=2)
            rejected('repair', id=exhausted_id, expected_attempts=3)
            call('remove', id=exhausted_id)

            stop_id, _ = enqueue_failed('stop-model', 1)
            call('repair', id=stop_id, expected_attempts=0)
            wait(lambda s: s['repair_active'] and s['queue'][-1]['checkpoint'] == 'repair_1_reserved')
            assert stop_request_started.wait(5)
            call('stop')
            stopped = wait(lambda s: not s['running'] and s['queue'][-1]['status'] == 'paused')
            assert stopped['queue'][-1]['repair_attempts'] == 1
            call('resume')
            resumed = wait(lambda s: not s['running'] and s['queue'][-1]['status'] == 'succeeded')
            assert resumed['queue'][-1]['repair_attempts'] == 1
            assert repair_calls['stop-model'] == 2

            emergency_id, _ = enqueue_failed('emergency-validation', 1, slow=True)
            call('repair', id=emergency_id, expected_attempts=0)
            wait(lambda s: s['running'] and s['queue'][-1]['checkpoint'] == 'repair_1_applied')
            call('emergency')
            emergency = wait(lambda s: not s['running'] and s['emergency_paused'])
            assert emergency['queue'][-1]['status'] == 'paused'
            rejected('resume')
            call('clear_emergency')
            call('resume')
            wait(lambda s: not s['running'] and s['queue'][-1]['status'] == 'succeeded')
            assert repair_calls['emergency-validation'] == 1

            restart_id, _ = enqueue_failed('restart-applied', 1, slow=True)
            call('repair', id=restart_id, expected_attempts=0)
            wait(lambda s: s['running'] and s['queue'][-1]['checkpoint'] == 'repair_1_applied')
            restarted = restart()
            current = next(feature for feature in restarted['queue'] if feature['id'] == restart_id)
            assert current['status'] == 'paused'
            assert current['checkpoint'] == 'repair_1_applied'
            assert current['repair_attempts'] == 1
            call('resume')
            wait(lambda s: not s['running'] and s['queue'][-1]['status'] == 'succeeded')
            assert repair_calls['restart-applied'] == 1

            owner_id, _ = enqueue_failed('owner-conflict', 1)
            call('repair', id=owner_id, expected_attempts=0)
            assert owner_request_started.wait(5)
            owner_file = projects / 'owner-conflict/app.py'
            owner_file.write_text('VALUE = 77\n')
            owner_response_released.set()
            conflict = wait(lambda s: not s['running'] and s['queue'][-1]['status'] == 'failed')
            assert conflict['queue'][-1]['repair_attempts'] == 1
            assert owner_file.read_text() == 'VALUE = 77\n'
            assert repair_calls['owner-conflict'] == 1
            rejected('resume')
            call('remove', id=owner_id)

            malicious_id, _ = enqueue_failed('malicious-test', 1)
            original_test = projects / 'malicious-test/tests/test_guard.py'
            assert original_test.read_text() == 'ASSERTION = True\n'
            call('repair', id=malicious_id, expected_attempts=0)
            malicious = wait(lambda s: not s['running'] and s['queue'][-1]['status'] == 'failed')
            assert malicious['queue'][-1]['repair_attempts'] == 1
            assert 'cannot modify existing test or validation input' in malicious['queue'][-1]['message']
            assert (projects / 'malicious-test/app.py').read_text() == 'VALUE = 0\n'
            assert original_test.read_text() == 'ASSERTION = True\n'
            assert repair_calls['malicious-test'] == 1
            call('remove', id=malicious_id)

            directory_id = str(uuid.uuid4())
            directory_validation = f'"{sys.executable}" -B -c "import app; assert app.VALUE == 1" checks'
            call('enqueue', id=directory_id, project='malicious-directory', instruction='Repair fixture malicious-directory', validation=directory_validation)
            call('start')
            wait(lambda s: not s['running'] and s['queue'][-1]['status'] == 'failed')
            expected_file = projects / 'malicious-directory/checks/arbitrary.py'
            assert expected_file.read_text() == 'EXPECTED = 1\n'
            call('repair', id=directory_id, expected_attempts=0)
            directory_attack = wait(lambda s: not s['running'] and s['queue'][-1]['status'] == 'failed')
            assert directory_attack['queue'][-1]['repair_attempts'] == 1
            assert 'cannot modify existing test or validation input' in directory_attack['queue'][-1]['message']
            assert (projects / 'malicious-directory/app.py').read_text() == 'VALUE = 0\n'
            assert expected_file.read_text() == 'EXPECTED = 1\n'

            with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
                durable = json.loads(database.execute('SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                backup_v2 = database.execute('SELECT state FROM developer_state_v2_backup WHERE id=1').fetchone()[0]
            assert backup_v2 == legacy_v2
            assert 'queue_v3' not in durable and 'queue_v2' not in durable and 'queue' not in durable
            loop_feature = next(feature for feature in durable['queue_v10'] if feature['id'] == loop_id)
            assert loop_feature['model_target'] == 'mac'
            assert loop_feature['validation'] == loop_validation
            assert len(loop_feature['repair_history']) == 2
            for number, evidence in enumerate(loop_feature['repair_history'], 1):
                assert evidence['attempt'] == number
                assert 'Validation failed' in evidence['prior_message']
                assert evidence['prior_edits'][0]['path'] == 'app.py'
                assert len(evidence['prior_edits'][0]['content_hash']) == 64
            exhausted_feature = next(feature for feature in durable['queue_v10'] if feature['id'] == exhausted_id)
            assert exhausted_feature['status'] == 'removed'
            assert exhausted_feature['repair_attempts'] == 3
            assert len(exhausted_feature['repair_history']) == 3
            repair_prompts = [request['messages'][1]['content'] for request in calls if 'Repair attempt:' in request['messages'][1]['content']]
            assert all('Unchanged validation command:' in prompt for prompt in repair_prompts)
            assert all('Prior failure evidence:' in prompt for prompt in repair_prompts)
            print(json.dumps({
                'native_platform': sys.platform,
                'repair_loop_succeeds_on_second_attempt': True,
                'repair_exhausts_at_three': True,
                'stale_duplicate_and_unauthenticated_rejected': True,
                'stop_model_wait_resumes_same_attempt': True,
                'emergency_validation_resumes_checkpoint': True,
                'restart_preserves_applied_checkpoint': True,
                'owner_edit_conflict_preserved_without_auto_loop': True,
                'test_weakening_rejected_without_partial_write': True,
                'validation_directory_descendant_protected': True,
                'validation_unchanged_and_history_durable': True,
                'restart_used_fresh_fixture_port_after_forced_termination': True,
            }))
        finally:
            owner_response_released.set()
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
