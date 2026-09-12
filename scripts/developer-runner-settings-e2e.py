#!/usr/bin/env python3
"""Disposable native process proof of developer AI settings and pinned provider calls."""
import argparse
from contextlib import closing
import http.server
import json
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
from pathlib import Path

from developer_planning_fixture import enqueue_with_plan
from developer_review_fixture import reviewer_arguments


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    args = parser.parse_args()
    entered = threading.Event()
    release = threading.Event()
    exhausted_mode = False
    exhausted_calls = []
    reviewer_executable = None
    hidden_reviewer = None

    class LocalModel(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            self.rfile.read(int(self.headers['Content-Length']))
            entered.set()
            if not release.wait(20):
                self.send_error(503)
                return
            value = 1
            if exhausted_mode:
                exhausted_calls.append(True)
                value = 0 if len(exhausted_calls) <= 3 else 1
                if len(exhausted_calls) == 4:
                    reviewer_executable.rename(hidden_reviewer)
            content = json.dumps({'files': [{'path': 'app.py', 'content': f'VALUE = {value}\n'}]})
            body = json.dumps({'choices': [{'message': {'content': content}, 'finish_reason': 'stop'}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *_args):
            pass

    model = http.server.ThreadingHTTPServer(('127.0.0.1', 0), LocalModel)
    threading.Thread(target=model.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix='aw-settings-e2e-') as temp:
            root = Path(temp)
            data = root / 'state'
            data.mkdir()
            projects = root / 'projects'
            reviewer = reviewer_arguments(root)
            reviewer_executable = Path(reviewer[1])
            hidden_reviewer = reviewer_executable.with_name('temporarily-unavailable-reviewer')
            process = None
            port = 0
            token = ''
            output = (root / 'runner.log').open('wb')

            def api(path='status', body=None, credential=None):
                request = urllib.request.Request(f'http://127.0.0.1:{port}/' + path,
                    data=None if body is None else json.dumps(body).encode(),
                    headers={'Authorization': 'Bearer ' + (token if credential is None else credential),
                             'Content-Type': 'application/json'})
                with urllib.request.urlopen(request, timeout=10) as response:
                    return json.load(response)

            def wait(predicate, timeout=40):
                deadline = time.monotonic() + timeout
                last = None
                while time.monotonic() < deadline:
                    try:
                        last = api()
                        if predicate(last):
                            return last
                    except (OSError, urllib.error.URLError):
                        pass
                    time.sleep(.05)
                raise AssertionError(f'Timed out: {last}\n' + (root / 'runner.log').read_text(errors='replace'))

            def start():
                nonlocal process, port, token
                with socket.socket() as reservation:
                    reservation.bind(('127.0.0.1', 0))
                    port = reservation.getsockname()[1]
                process = subprocess.Popen([str(Path(args.binary).resolve()),
                    '--data-dir', str(data), '--workspace-root', str(projects),
                    '--bind', f'127.0.0.1:{port}', '--model-url', f'http://127.0.0.1:{model.server_port}/v1',
                    *reviewer], stdin=subprocess.DEVNULL, stdout=output, stderr=output)
                deadline = time.monotonic() + 45
                while not (data / 'developer-token').exists() and time.monotonic() < deadline:
                    if process.poll() is not None:
                        raise AssertionError((root / 'runner.log').read_text(errors='replace'))
                    time.sleep(.05)
                assert (data / 'developer-token').exists(), (
                    'Runner did not become ready within 45 seconds: '
                    + (root / 'runner.log').read_text(errors='replace'))
                token = (data / 'developer-token').read_text().strip()
                return wait(lambda _s: True)

            def stop():
                nonlocal process
                if process and process.poll() is None:
                    try:
                        api('control', {'action': 'shutdown'})
                        process.wait(timeout=15)
                    except (OSError, urllib.error.URLError, subprocess.TimeoutExpired):
                        process.terminate()
                        try:
                            process.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait(timeout=10)
                process = None

            def rejected(body, path='settings', credential=None, expected=(400, 409, 422)):
                try:
                    api(path, body, credential)
                    raise AssertionError('Unexpectedly accepted rejected settings request')
                except urllib.error.HTTPError as error:
                    assert error.code in expected, error.code

            def selection(model_id, effort):
                return {'model': model_id, 'reasoning_effort': effort}

            def update(current, orchestrator, reviewer_selection):
                return {'expected_revision': current['ai_settings']['revision'],
                        'orchestrator': orchestrator, 'reviewer': reviewer_selection}

            try:
                initial = start()
                assert api('settings')['ai_settings'] == initial['ai_settings']
                assert initial['ai_settings']['orchestrator'] == selection('gpt-5.6-sol', 'high')
                assert initial['ai_settings']['reviewer'] == selection('gpt-5.6-sol', 'high')
                ids = {m['id'] for m in initial['ai_models']}
                assert len(ids) == 7 and {'gpt-6-astra', 'gpt-5.3-codex-spark'} <= ids
                assert initial['ai_catalog_source'] == 'bundled'
                planner_choice = selection('gpt-6-astra', 'ultra')
                reviewer_choice = selection('gpt-5.6-terra', 'max')
                request = update(initial, planner_choice, reviewer_choice)
                rejected(request, credential='wrong-owner', expected=(401,))
                saved = api('settings', request)
                assert saved['ai_settings']['revision'] == initial['ai_settings']['revision'] + 1
                rejected(request)  # A stale revision cannot overwrite a newer choice.
                for invalid in [selection('not-a-model', 'high'), selection('gpt-5.5', 'ultra'),
                                selection('gpt-6-astra', 'arbitrary'), {}, {'model': 'gpt-6-astra'}]:
                    rejected(update(saved, invalid, reviewer_choice))
                    assert api()['ai_settings'] == saved['ai_settings']
                unknown = update(saved, planner_choice, reviewer_choice)
                unknown['unexpected'] = True
                rejected(unknown)

                feature_id = str(uuid.uuid4())
                validation = f'"{sys.executable}" -B -c "import app; assert app.VALUE == 1"'
                feature = {'id': feature_id, 'project': 'settings-proof',
                           'instruction': 'Create app.py with VALUE = 1.', 'validation': validation}
                queued = enqueue_with_plan(f'http://127.0.0.1:{port}', token, feature)
                planned = api('planning?id=' + feature_id)
                assert planned['model'] == planner_choice['model']
                assert planned['reasoning_effort'] == planner_choice['reasoning_effort']
                pinned = next(f for f in queued['queue'] if f['id'] == feature_id)
                assert pinned['review_model'] == reviewer_choice['model']
                assert pinned['review_reasoning_effort'] == reviewer_choice['reasoning_effort']
                later = api('settings', update(queued, selection('gpt-5.6-sol', 'low'),
                                              selection('gpt-5.6-luna', 'medium')))
                assert api('planning?id=' + feature_id) == planned
                assert next(f for f in api()['queue'] if f['id'] == feature_id) == pinned
                api('control', {'action': 'start', 'expected_feature_id': feature_id,
                    'expected_model_target': 'mac', 'expected_status': pinned['status'],
                    'expected_checkpoint': pinned['checkpoint']})
                assert entered.wait(15)
                rejected(update(later, planner_choice, reviewer_choice))
                release.set()
                completed = wait(lambda s: not s['running'] and any(
                    f['id'] == feature_id and f['status'] in ('succeeded', 'failed') for f in s['queue']))
                result = next(f for f in completed['queue'] if f['id'] == feature_id)
                assert result['status'] == 'succeeded' and result['review_status'] == 'approved', result
                assert result['review_model'] == reviewer_choice['model']
                assert result['review_reasoning_effort'] == reviewer_choice['reasoning_effort']
                evidence = [json.loads(line) for line in (root / 'review-fixture/review-input-evidence.jsonl').read_text().splitlines()]
                for kind, choice in [('planning', planner_choice), ('review', reviewer_choice)]:
                    records = [r for r in evidence if r['kind'] == kind]
                    assert records and all(r['model_id'] == choice['model'] and
                        r['reasoning_effort'] == choice['reasoning_effort'] for r in records), records
                    assert any(r['kind'] == 'argv' and r['model'] == choice['model'] and
                        r['reasoning_effort'] == choice['reasoning_effort'] for r in evidence), evidence

                stop()
                restarted = start()
                assert restarted['ai_settings'] == later['ai_settings']
                assert next(f for f in restarted['queue'] if f['id'] == feature_id) == result
                assert api('planning?id=' + feature_id) == planned
                busy_id = str(uuid.uuid4())
                busy = api('planning', {'action': 'start', 'feature_id': busy_id,
                    'request_id': str(uuid.uuid4()), 'expected_revision': 0, 'project': 'settings-busy',
                    'instruction': 'Wait for cancellation [planning:wait]', 'validation': validation,
                    'model_target': 'mac'})
                assert busy['running']
                assert busy['model'] == later['ai_settings']['orchestrator']['model']
                assert busy['reasoning_effort'] == later['ai_settings']['orchestrator']['reasoning_effort']
                rejected(update(restarted, planner_choice, reviewer_choice))
                api('control', {'action': 'stop'})
                idle = wait(lambda s: not s['planning_running'])
                assert idle['ai_settings'] == later['ai_settings']

                retired_id = str(uuid.uuid4())
                queued_before_retirement = enqueue_with_plan(f'http://127.0.0.1:{port}', token,
                    {'id': retired_id, 'project': 'retired-reviewer',
                     'instruction': 'Create app.py with VALUE = 1.', 'validation': validation})
                retired = next(f for f in queued_before_retirement['queue'] if f['id'] == retired_id)
                stop()
                narrowed = [m for m in initial['ai_models'] if m['id'] in ('gpt-5.5', 'gpt-5.3-codex-spark')]
                cache = {'models': [{'slug': m['id'], 'display_name': m['name'], 'visibility': 'list',
                    'default_reasoning_level': m['default_reasoning_effort'],
                    'supported_reasoning_levels': [{'effort': e} for e in m['reasoning_efforts']]}
                    for m in narrowed]}
                (root / 'review-fixture/auth/models_cache.json').write_text(json.dumps(cache), encoding='utf-8')
                retired_state = start()
                assert retired_state['ai_settings'] == later['ai_settings']
                assert retired_state['ai_models'] == narrowed
                assert next(f for f in retired_state['queue'] if f['id'] == retired_id) == retired
                assert next(f for f in retired_state['queue'] if f['id'] == feature_id) == result
                rejected({'action': 'start', 'feature_id': str(uuid.uuid4()),
                    'request_id': str(uuid.uuid4()), 'expected_revision': 0, 'project': 'unavailable-planner',
                    'instruction': 'Create app.py with VALUE = 1.', 'validation': validation,
                    'model_target': 'mac'}, path='planning')
                recovered = api('settings', update(retired_state, selection('gpt-5.5', 'high'),
                                                  selection('gpt-5.3-codex-spark', 'high')))
                assert recovered['ai_settings']['orchestrator'] == selection('gpt-5.5', 'high')
                rejected({'action': 'start', 'expected_feature_id': retired_id,
                    'expected_model_target': 'mac', 'expected_status': retired['status'],
                    'expected_checkpoint': retired['checkpoint']}, path='control')
                assert next(f for f in api()['queue'] if f['id'] == retired_id) == retired
                def change_request(snapshot, target, choice):
                    return {'id': target['id'], 'expected_revision': snapshot['revision'],
                        'expected_checkpoint': target['checkpoint'], 'expected_model': target['review_model'],
                        'expected_reasoning_effort': target['review_reasoning_effort'], 'reviewer': choice}

                spark = selection('gpt-5.3-codex-spark', 'high')
                before_switch = api()
                switch = change_request(before_switch, retired, spark)
                assert before_switch['feature_reviewer_selection'] and retired['can_change_reviewer']
                for bad in [dict(switch, unexpected=True), dict(switch, expected_checkpoint='stale'),
                            dict(switch, expected_model='gpt-5.5'),
                            dict(switch, expected_reasoning_effort='low'),
                            dict(switch, reviewer=selection('gpt-5.3-codex-spark', 'ultra')),
                            dict(switch, reviewer=selection('gpt-missing', 'high'))]:
                    rejected(bad, path='feature-reviewer')
                    assert api() == before_switch
                rejected(switch, path='feature-reviewer', credential='wrong-owner', expected=(401,))
                switched = api('feature-reviewer', switch)
                rejected(switch, path='feature-reviewer')
                assert switched['ai_settings'] == before_switch['ai_settings']
                rebound = next(f for f in switched['queue'] if f['id'] == retired_id)
                assert rebound['review_model'] == spark['model'] and rebound['review_reasoning_effort'] == 'high'
                assert not switched['running'] and rebound['status'] == 'queued'
                stop()
                restarted_switch = start()
                assert next(f for f in restarted_switch['queue'] if f['id'] == retired_id) == rebound
                api('control', {'action': 'start', 'expected_feature_id': retired_id,
                    'expected_model_target': 'mac', 'expected_status': rebound['status'],
                    'expected_checkpoint': rebound['checkpoint']})
                retired_done = wait(lambda s: not s['running'] and any(
                    f['id'] == retired_id and f['status'] == 'succeeded' for f in s['queue']))
                retired_result = next(f for f in retired_done['queue'] if f['id'] == retired_id)
                assert retired_result['review_status'] == 'approved'
                assert not retired_result['can_change_reviewer']
                rejected(change_request(retired_done, retired_result, selection('gpt-5.5', 'high')),
                         path='feature-reviewer')

                # Restore the complete catalog before exercising Sol usage exhaustion.
                stop()
                (root / 'review-fixture/auth/models_cache.json').unlink()
                full = start()
                defaults = api('settings', update(full, selection('gpt-5.5', 'high'),
                                                  selection('gpt-5.6-sol', 'high')))['ai_settings']
                exhausted_id = str(uuid.uuid4())
                outage = enqueue_with_plan(f'http://127.0.0.1:{port}', token,
                    {'id': exhausted_id, 'project': 'exhausted-reviewer',
                     'instruction': 'Create app.py with VALUE = 1.', 'validation': validation})
                exhausted = next(f for f in outage['queue'] if f['id'] == exhausted_id)
                # Hold the model response so busy rejection is exercised deterministically.
                exhausted_mode = True
                entered.clear()
                release.clear()
                api('control', {'action': 'start', 'expected_feature_id': exhausted_id,
                    'expected_model_target': 'mac', 'expected_status': exhausted['status'],
                    'expected_checkpoint': exhausted['checkpoint']})
                assert entered.wait(15)
                busy_switch = api()
                busy_feature = next(f for f in busy_switch['queue'] if f['id'] == exhausted_id)
                assert not busy_feature['can_change_reviewer']
                rejected(change_request(busy_switch, busy_feature, spark), path='feature-reviewer')
                release.set()
                failed = wait(lambda s: not s['running'] and any(
                    f['id'] == exhausted_id and f['status'] == 'failed' for f in s['queue']), timeout=80)
                api('control', {'action': 'repair', 'id': exhausted_id, 'expected_attempts': 0})
                failed = wait(lambda s: not s['running'] and any(
                    f['id'] == exhausted_id and f['status'] == 'failed'
                    and f['repair_attempts'] == 3 for f in s['queue']), timeout=80)
                exhausted = next(f for f in failed['queue'] if f['id'] == exhausted_id)
                assert exhausted['repair_attempts'] == 3 and len(exhausted_calls) == 4, exhausted
                assert exhausted['review_status'] == 'unavailable', exhausted
                assert exhausted['review_model'] == 'gpt-5.6-sol'
                hidden_reviewer.rename(reviewer_executable)

                def durable_feature():
                    with closing(sqlite3.connect(data / 'developer.sqlite3')) as db:
                        durable = json.loads(db.execute('SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                    return next(f for f in durable['queue_v10'] if f['id'] == exhausted_id)

                prior_evidence = durable_feature()
                project_bytes = (projects / 'exhausted-reviewer/app.py').read_bytes()
                changed = api('feature-reviewer', change_request(failed, exhausted, spark))
                current = next(f for f in changed['queue'] if f['id'] == exhausted_id)
                assert changed['ai_settings'] == defaults and not changed['running']
                assert current['checkpoint'] == exhausted['checkpoint'] and current['repair_attempts'] == 3
                changed_evidence = durable_feature()
                for key in ['repair_history', 'review_history', 'edits', 'planning', 'escalation_history']:
                    assert changed_evidence[key] == prior_evidence[key], key
                assert changed_evidence['reviewer_selection_history'][-1]['selected_model'] == spark['model']
                stop()
                persisted = start()
                assert next(f for f in persisted['queue'] if f['id'] == exhausted_id) == current
                assert durable_feature()['reviewer_selection_history'] == changed_evidence['reviewer_selection_history']
                api('control', {'action': 'resume', 'expected_feature_id': exhausted_id,
                    'expected_model_target': 'mac', 'expected_status': current['status'],
                    'expected_checkpoint': current['checkpoint']})
                final = wait(lambda s: not s['running'] and any(
                    f['id'] == exhausted_id and f['status'] in ('succeeded', 'failed') for f in s['queue']))
                completed_switch = next(f for f in final['queue'] if f['id'] == exhausted_id)
                assert completed_switch['status'] == 'succeeded', completed_switch
                assert completed_switch['review_status'] == 'approved' and completed_switch['repair_attempts'] == 3
                assert completed_switch['review_attempts'] == exhausted['review_attempts'] + 1
                assert len(exhausted_calls) == 4
                assert (projects / 'exhausted-reviewer/app.py').read_bytes() == project_bytes
                final_evidence = durable_feature()
                assert final_evidence['review_history'][:-1] == prior_evidence['review_history']
                assert final_evidence['review_history'][-1]['packet_sha256'] != prior_evidence['review_history'][-1]['packet_sha256']
                inputs = [json.loads(line) for line in (root / 'review-fixture/review-input-evidence.jsonl').read_text().splitlines()]
                assert [r for r in inputs if r['kind'] == 'review'][-1]['model_id'] == spark['model']
                last_argv = [r for r in inputs if r['kind'] == 'argv'][-1]
                assert last_argv['model'] == spark['model'] and last_argv['reasoning_effort'] == 'high'
                print('Developer AI settings native E2E: PASS (catalog, auth, strict/stale/busy rejection, role pinning, argv, review, restart, retired-model recovery, existing-feature reviewer switch after three repairs)')
            finally:
                release.set()
                stop()
                output.close()
    finally:
        model.shutdown()
        model.server_close()


if __name__ == '__main__':
    main()
