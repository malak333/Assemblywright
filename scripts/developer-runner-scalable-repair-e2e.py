#!/usr/bin/env python3
"""Disposable native runner proof: large context, cumulative batches, real images."""
import argparse
import base64
from contextlib import closing
import hashlib
import http.server
import json
import os
from pathlib import Path
import py_compile
import re
import signal
import shlex
import shutil
import socket
import sqlite3
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
import zlib

from developer_planning_fixture import enqueue_with_plan
from developer_opencode_runtime import provision_pinned_runtime
from developer_review_fixture import reviewer_arguments


class FailurePreservingTemporaryDirectory:
    def __init__(self, prefix):
        self.path = Path(tempfile.mkdtemp(prefix=prefix))

    def __enter__(self):
        return str(self.path)

    def __exit__(self, kind, unused_value, unused_traceback):
        if kind is None:
            cleanup_path = str(self.path)
            if os.name == 'nt':
                cleanup_path = '\\\\?\\' + str(Path(self.path).resolve())

            def missing_descendant(function, path, error):
                if isinstance(error[1], FileNotFoundError) and path != cleanup_path:
                    return
                raise error[1]

            shutil.rmtree(cleanup_path, onerror=missing_descendant)
        else:
            print(f'preserved failed native fixture: {self.path}', file=sys.stderr)


def png():
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 128, 128, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress((b'\0' + b'\x20\x60\x90' * 128) * 128)) + chunk(b'IEND', b''))


def validation_cache_snapshot(project):
    snapshot = {}
    for path in project.rglob('*'):
        relative = path.relative_to(project)
        cache_path = (path.suffix in ('.pyc', '.pyo')
            or any(part == '__pycache__'
                or part.startswith('.assemblywright-validation-pycache-')
                for part in relative.parts))
        if not cache_path:
            continue
        if path.is_dir():
            snapshot[relative.as_posix()] = ('directory', path.stat().st_mtime_ns)
        elif path.is_file():
            snapshot[relative.as_posix()] = ('file',
                hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mtime_ns)
    return snapshot


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    parser.add_argument('--provider-retry-success', action='store_true',
        help='Prove successful frozen review after one real provider exit')
    parser.add_argument('--opencode-executable',
        default=os.environ.get('ASSEMBLYWRIGHT_DEVELOPER_OPENCODE_EXECUTABLE'),
        help='exact pinned OpenCode 1.18.23 executable used by the staged repair proof')
    arguments = parser.parse_args()
    binary = str(Path(arguments.binary).resolve())
    configured_opencode = arguments.opencode_executable
    if configured_opencode is None:
        configured_opencode = provision_pinned_runtime(Path(__file__).resolve().parents[1])
    opencode = str(Path(configured_opencode).resolve())
    if not Path(opencode).is_file():
        parser.error(f'pinned OpenCode executable does not exist: {opencode}')
    calls = []
    opencode_calls = []
    long_stage_opencode_calls = []
    attributable_chat_opencode_calls = []
    long_stage_probe_count = 25
    review_batch_entry_limit = 40
    review_candidate_entry_limit = 320
    staged_generated_file_count = 300
    staged_candidate_entry_count = staged_generated_file_count + 6
    staged_recovery_entry_count = staged_candidate_entry_count + 1
    staged_review_batch_count = (
        staged_recovery_entry_count + review_batch_entry_limit - 1
    ) // review_batch_entry_limit
    assert staged_recovery_entry_count <= review_candidate_entry_limit
    assert staged_review_batch_count <= \
        review_candidate_entry_limit // review_batch_entry_limit
    stage_entered = threading.Event()
    stage_release = threading.Event()
    staged_image = png()

    def model_reply(handler, request, message, finish_reason):
        if request.get('stream'):
            chunks = [
                {'id': 'fixture-completion', 'object': 'chat.completion.chunk', 'created': 1,
                 'model': request.get('model', 'fixture'),
                 'choices': [{'index': 0, 'delta': {'role': 'assistant', **message},
                              'finish_reason': None}]},
                {'id': 'fixture-completion', 'object': 'chat.completion.chunk', 'created': 1,
                 'model': request.get('model', 'fixture'),
                 'choices': [{'index': 0, 'delta': {}, 'finish_reason': finish_reason}]},
            ]
            body = b''.join(b'data: ' + json.dumps(chunk).encode() + b'\n\n' for chunk in chunks)
            body += b'data: [DONE]\n\n'
            content_type = 'text/event-stream'
        else:
            body = json.dumps({'id': 'fixture-completion', 'object': 'chat.completion',
                'created': 1, 'model': request.get('model', 'fixture'),
                'choices': [{'index': 0, 'message': {'role': 'assistant', **message},
                             'finish_reason': finish_reason}],
                'usage': {'prompt_tokens': 1, 'completion_tokens': 1, 'total_tokens': 2}}).encode()
            content_type = 'application/json'
        handler.send_response(200)
        handler.send_header('Content-Type', content_type)
        handler.send_header('Content-Length', str(len(body)))
        handler.end_headers()
        handler.wfile.write(body)

    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self, *unused):
            pass

        def do_GET(self):
            if self.path != '/props':
                self.send_error(404)
                return
            body = json.dumps({
                'total_slots': 1,
                'modalities': {'vision': False},
                'n_ctx': 262144,
                'default_generation_settings': {'n_ctx': 262144},
            }).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            if request.get('tools'):
                prompt_text = json.dumps(request.get('messages', []))
                attributable_chat = 'fixture:attributable-chat-mutation' in prompt_text
                long_stage = 'staged-automatic:' in prompt_text
                if attributable_chat:
                    opencode_calls.append(request)
                    attributable_chat_opencode_calls.append(request)
                    tool_results = [message for message in request.get('messages', [])
                        if message.get('role') == 'tool']
                    if tool_results:
                        return model_reply(self, request,
                            {'content': 'Applied the attributable project-chat mutation.'},
                            'stop')
                    tools = {item['function']['name']: item['function']
                        for item in request['tools'] if item.get('type') == 'function'}
                    assert 'bash' in tools, sorted(tools)
                    mutation_code = (
                        "from pathlib import Path; "
                        "Path('chat-attributable.txt').write_bytes("
                        "b'attributable project-chat mutation\\n')")
                    mutation_command = (subprocess.list2cmdline(
                        [sys.executable, '-B', '-c', mutation_code])
                        if os.name == 'nt' else shlex.join(
                            [sys.executable, '-B', '-c', mutation_code]))
                    mutation_arguments = {
                        'command': mutation_command,
                        'description': 'write attributable project-chat mutation',
                    }
                    for required in tools['bash'].get('parameters', {}).get('required', []):
                        mutation_arguments.setdefault(required,
                            'attributable project-chat mutation')
                    return model_reply(self, request, {'content': None, 'tool_calls': [{
                        'index': 0, 'id': 'attributable-chat-write', 'type': 'function',
                        'function': {'name': 'bash',
                                     'arguments': json.dumps(mutation_arguments)}}]}, 'tool_calls')
                assert 'Staged build environment:' in prompt_text
                assert 'Never install packages into global Python' in prompt_text
                if os.name == 'nt':
                    assert 'Windows PowerShell' in prompt_text
                opencode_calls.append(request)
                if long_stage:
                    long_stage_opencode_calls.append(request)
                stage_entered.set()
                if not stage_release.wait(90):
                    raise AssertionError('staged repair fixture was not released')
                tool_results = [message for message in request.get('messages', [])
                    if message.get('role') == 'tool']
                tools = {item['function']['name']: item['function']
                    for item in request['tools'] if item.get('type') == 'function'}
                assert 'bash' in tools, sorted(tools)
                schema = tools['bash'].get('parameters', {})
                if long_stage and len(tool_results) < long_stage_probe_count:
                    probe_index = len(tool_results)
                    marker = f'aw-long-stage-probe-{probe_index:02}'
                    probe_code = (
                        "from pathlib import Path; "
                        "assert Path('app.py').is_file(); "
                        f"print('{marker}')")
                    probe_command = (subprocess.list2cmdline(
                        [sys.executable, '-B', '-c', probe_code])
                        if os.name == 'nt' else shlex.join(
                            [sys.executable, '-B', '-c', probe_code]))
                    probe_arguments = {
                        'command': probe_command,
                        'description': f'bounded staged probe {probe_index:02}',
                    }
                    for required in schema.get('required', []):
                        probe_arguments.setdefault(required, 'bounded staged probe')
                    return model_reply(self, request, {'content': None, 'tool_calls': [{
                        'index': 0, 'id': marker, 'type': 'function',
                        'function': {'name': 'bash',
                                     'arguments': json.dumps(probe_arguments)}}]}, 'tool_calls')
                if tool_results and (not long_stage
                        or len(tool_results) > long_stage_probe_count):
                    return model_reply(self, request,
                        {'content': 'Implemented the staged correction, generated the PNG, and ran validation.'},
                        'stop')
                image_base64 = base64.b64encode(staged_image).decode()
                corrected_test_base64 = base64.b64encode((
                    'from pathlib import Path\nimport sys\n'
                    f"with Path({str(root / 'staged-validation-events')!r}).open('a') as log: "
                    "log.write('validated\\n')\n"
                    'sys.path.insert(0,str(Path(__file__).parents[1]))\n'
                    'import app\nassert app.VALUE == 1\n').encode()).decode()
                protected_rejection = 'staged-protected' in json.dumps(request)
                code = ''.join([
                    "from pathlib import Path; import base64; ",
                    "Path('app.py').write_bytes(b'VALUE = 1\\n'); ",
                    "Path('tests').mkdir(exist_ok=True); ",
                    ("Path('tests/test_existing.py').write_bytes(b'assert False\\n'); "
                     if protected_rejection else
                     f"Path('tests/test_existing.py').write_bytes(base64.b64decode('{corrected_test_base64}')); "
                     "Path('tests/test_staged_regression.py').write_bytes("
                     "b'from pathlib import Path\\nimport sys\\nsys.path.insert(0,str(Path(__file__).parents[1]))"
                     "\\nimport app\\nassert app.VALUE == 1\\n'); "
                     "Path('dist').mkdir(exist_ok=True); "
                     "Path('dist/index.html').write_bytes(b'<main>rebuilt staged output</main>\\n'); "
                     "Path('dist/obsolete-generated.html').unlink(); "
                     "[Path(f'dist/staged-{index:03}.html').write_bytes("
                     f"f'<p>staged generated {{index}}</p>\\n'.encode()) for index in "
                     f"range({staged_generated_file_count})]; "),
                    f"Path('dist/route.png').write_bytes(base64.b64decode('{image_base64}')); ",
                    "assert Path('dist/route.png').read_bytes().startswith(b'\\x89PNG\\r\\n\\x1a\\n')",
                ])
                command = (subprocess.list2cmdline([sys.executable, '-c', code])
                    if os.name == 'nt' else shlex.join([sys.executable, '-c', code]))
                arguments = {'command': command, 'description': 'write staged repair and PNG'}
                for required in schema.get('required', []):
                    arguments.setdefault(required, 'staged repair fixture')
                return model_reply(self, request, {'content': None, 'tool_calls': [{
                    'index': 0, 'id': 'staged-write', 'type': 'function',
                    'function': {'name': 'bash', 'arguments': json.dumps(arguments)}}]}, 'tool_calls')
            prompt = request['messages'][1]['content']
            calls.append(prompt)
            repair = 'Repair attempt:' in prompt
            staged = 'staged-automatic' in prompt or 'staged-drift' in prompt
            automatic = 'Automatic escalation:' in prompt
            if staged:
                repair_match = re.search(r'Repair attempt: (\d+) of 3', prompt)
                attempt = int(repair_match.group(1)) if repair_match else 0
                value = attempt + 1 if repair else 0
                files = ([{'path': 'app.py', 'content': f'VALUE = {value}\n'},
                          {'path': 'tests/test_existing.py', 'content':
                           'from pathlib import Path\nimport sys\n'
                           f"with Path({str(root / 'staged-validation-events')!r}).open('a') as log: log.write('validated\\n')\n"
                           'sys.path.insert(0,str(Path(__file__).parents[1]))\n'
                           'import app\nassert app.VALUE == 999\n'},
                          {'path': 'dist/index.html', 'content':
                           '<main>stale generated output</main>\n'},
                          {'path': 'dist/obsolete-generated.html', 'content':
                           '<main>obsolete generated output</main>\n'}]
                         if not repair else [{'path': 'app.py', 'content': f'VALUE = {value}\n'}])
            else:
                files = [{'path': 'app.py', 'content': 'VALUE = 1\n' if repair else 'VALUE = 0\n'}]
                indices = range(38, 48) if repair else range(38)
                files += [{'path': f'source/module_{i:03}.py',
                           'content': f'# module {i}\n' + '# bounded source evidence\n' * 240} for i in indices]
            content = '{' if staged and automatic else json.dumps({'files': files})
            body = json.dumps({'choices': [{'message': {'content': content},
                                             'finish_reason': 'stop'}]}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    model = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    model.daemon_threads = True
    threading.Thread(target=model.serve_forever, daemon=True).start()
    windows_model = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    windows_model.daemon_threads = True
    threading.Thread(target=windows_model.serve_forever, daemon=True).start()
    with FailurePreservingTemporaryDirectory(prefix='aw-scalable-repair-e2e-') as temporary:
        root = Path(temporary)
        data, projects = root / 'state', root / 'projects'
        data.mkdir()
        projects.mkdir()
        project = projects / 'large-repair'
        (project / 'site').mkdir(parents=True)
        for index in range(90):
            (project / 'site' / f'generated-{index}.html').write_text('generated-output ' * 260)
        with socket.socket() as reservation:
            reservation.bind(('127.0.0.1', 0))
            port = reservation.getsockname()[1]
        log = (root / 'runner.log').open('wb')
        command = [binary, '--data-dir', str(data), '--workspace-root', str(projects),
                   '--bind', f'127.0.0.1:{port}', '--model-url', f'http://127.0.0.1:{model.server_port}/v1']
        command += ['--windows-model-url', f'http://127.0.0.1:{windows_model.server_port}/v1',
                    '--windows-model', 'staged-fixture']
        command += reviewer_arguments(root)
        opencode_enabled = False
        process = None
        token = ''

        def api(path='status', body=None):
            request = urllib.request.Request(f'http://127.0.0.1:{port}/{path}',
                data=None if body is None else json.dumps(body).encode(),
                headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'})
            return json.load(urllib.request.urlopen(request, timeout=10))

        def set_global_permissions(mode):
            current = api('permissions')
            if current['mode'] == mode:
                return current
            return api('permissions', {'mode': mode,
                'expected_revision': current['revision']})

        def wait(predicate, timeout=90):
            deadline = time.monotonic() + timeout
            state = None
            while time.monotonic() < deadline:
                try:
                    state = api()
                    if predicate(state):
                        return state
                except OSError:
                    pass
                time.sleep(.05)
            raise AssertionError((state, (root / 'runner.log').read_text(errors='replace')))

        def launch(wait_until_idle=True):
            nonlocal process, token
            launch_command = command + (['--opencode-executable', opencode]
                                        if opencode_enabled else [])
            process = subprocess.Popen(launch_command, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
            deadline = time.monotonic() + 15
            while not (data / 'developer-token').exists() and time.monotonic() < deadline:
                time.sleep(.05)
            # Same owner-only token file used by the other native runner harnesses.
            token_path = data / 'developer-token'
            token = token_path.read_text().strip()
            if wait_until_idle:
                wait(lambda s: not s['running'])

        def stop():
            nonlocal process
            if process is not None and process.poll() is None:
                process.terminate()
                process.wait(timeout=15)

        def suspend_runner():
            if os.name != 'nt':
                os.kill(process.pid, signal.SIGSTOP)
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    stopped, status = os.waitpid(process.pid, os.WUNTRACED | os.WNOHANG)
                    if stopped == process.pid and os.WIFSTOPPED(status):
                        return None
                    time.sleep(.001)
                raise AssertionError('runner did not enter the suspended state')
            import ctypes
            access = 0x0800 | 0x1000  # PROCESS_SUSPEND_RESUME | PROCESS_QUERY_LIMITED_INFORMATION
            handle = ctypes.windll.kernel32.OpenProcess(access, False, process.pid)
            if not handle:
                raise OSError(ctypes.get_last_error(), 'OpenProcess failed for runner suspension')
            result = ctypes.windll.ntdll.NtSuspendProcess(handle)
            if result != 0:
                ctypes.windll.kernel32.CloseHandle(handle)
                raise OSError(result, 'NtSuspendProcess failed')
            return handle

        def resume_runner(handle):
            if os.name != 'nt':
                os.kill(process.pid, signal.SIGCONT)
                return
            import ctypes
            result = ctypes.windll.ntdll.NtResumeProcess(handle)
            ctypes.windll.kernel32.CloseHandle(handle)
            if result != 0:
                raise OSError(result, 'NtResumeProcess failed')

        def terminate_fixture_process(pid):
            if os.name != 'nt':
                os.kill(pid, signal.SIGKILL)
                return
            import ctypes
            process_terminate = 0x0001
            handle = ctypes.windll.kernel32.OpenProcess(process_terminate, False, pid)
            if not handle:
                raise OSError(ctypes.get_last_error(),
                    'OpenProcess failed for reviewer termination')
            try:
                if not ctypes.windll.kernel32.TerminateProcess(handle, 86):
                    raise OSError(ctypes.get_last_error(), 'TerminateProcess failed for reviewer')
            finally:
                ctypes.windll.kernel32.CloseHandle(handle)

        def resume(feature_id):
            feature = next(item for item in api()['queue'] if item['id'] == feature_id)
            api('control', {'action': 'resume', 'expected_feature_id': feature_id,
                'expected_model_target': feature['model_target'], 'expected_status': feature['status'],
                'expected_checkpoint': feature['checkpoint']})

        try:
            launch()
            initial = api()
            api('auto-ai-repair', {'enabled': True, 'max_escalations': 3,
                'expected_revision': initial['revision']})
            feature_id = str(uuid.uuid4())
            state = enqueue_with_plan(f'http://127.0.0.1:{port}', token, {
                'id': feature_id, 'project': 'large-repair',
                'instruction': 'Create the bounded source modules and provide a meaningful route map asset.',
                'validation': f'"{sys.executable}" -B -c "import app; assert app.VALUE == 1"',
                'model_target': 'mac'})
            resume(feature_id)
            state = wait(lambda s: not s['running'] and s['queue'][0]['status'] == 'succeeded')
            assert state['queue'][0]['status'] == 'succeeded', state['queue'][0]
            assert len(state['queue'][0]['changed_files']) == 49, (state['queue'][0]['changed_files'], len(calls))
            assert len(calls) == 2, len(calls)
            assert 'manifest_sha256' in calls[-1] and 'selected_portions' in calls[-1]
            assert len(calls[-1]) < 220_000, len(calls[-1])

            # Seed a known, exact already-applied binary candidate in this disposable
            # database. This tests migration/resume, never edits a user's database.
            stop()
            image = png()
            (project / 'route.png').write_bytes(image)
            with closing(sqlite3.connect(data / 'developer.sqlite3')) as database, database:
                durable = json.loads(database.execute('SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                feature = next(item for item in durable['queue_v13'] if item['id'] == feature_id)
                feature.update(status='failed', checkpoint='applied', review_status='interrupted',
                               review_pending=None, last_failure_kind='operational')
                feature['edits'].append({'path': 'route.png', 'content': '', 'before': None,
                    'asset': {'media_type': 'image/png', 'data_base64': base64.b64encode(image).decode(),
                              'width': 128, 'height': 128}})
                durable['revision'] += 1
                database.execute('UPDATE developer_state SET state=? WHERE id=1', (json.dumps(durable),))
            previous_calls = len(calls)
            launch()
            resume(feature_id)
            state = wait(lambda s: not s['running'] and s['queue'][0]['status'] == 'succeeded')
            assert state['queue'][0]['status'] == 'succeeded', state['queue'][0]
            assert len(calls) == previous_calls, 'Resume replayed implementation'
            evidence = [json.loads(line) for line in (root / 'review-fixture/review-input-evidence.jsonl').read_text().splitlines()]
            batches = [item for item in evidence if item.get('kind') == 'review_batch']
            aggregates = [item for item in evidence if item.get('kind') == 'review_aggregate']
            assert len(batches) >= 4 and len(aggregates) >= 2, evidence
            assert any(hashlib.sha256(image).hexdigest() in item['image_sha256s'] for item in batches)
            assert any(item['batch_count'] >= 2 for item in batches)
            # Exercise the public migration path from a legacy path-only binary
            # quarantine. Current/new source and generated assets must be adopted
            # together, and a digest observed before any change must be rejected.
            stop()
            cache_response = project / 'pip/cache/http-v2/0/e/1/a/1/response.body'
            cache_response.parent.mkdir(parents=True)
            cache_response.write_bytes(b'\xffhistorical pip response cache')
            cache_selfcheck = project / 'pip/cache/selfcheck/state.json'
            cache_selfcheck.parent.mkdir(parents=True)
            cache_selfcheck.write_text('{"checked":true}\n')
            bytecode = project / 'tests/__pycache__/test_site.cpython-312.pyc'
            bytecode.parent.mkdir(parents=True, exist_ok=True)
            bytecode.write_bytes(b'historical bytecode')
            retained_generated = project / 'site/retained-generated.html'
            retained_generated.write_text('<main>retained generated output</main>\n')
            with closing(sqlite3.connect(data / 'developer.sqlite3')) as database, database:
                durable = json.loads(database.execute('SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                feature = next(item for item in durable['queue_v13'] if item['id'] == feature_id)
                # Match the live feature's exhausted ordinary-repair budget.
                # Copy exact retained prior evidence into two additional fixture
                # attempts; no actual provider calls are made while seeding.
                for attempt in (2, 3):
                    prior = json.loads(json.dumps(feature['repair_history'][0]))
                    prior['attempt'] = attempt
                    prior['prior_checkpoint'] = f'repair_{attempt - 1}_applied'
                    feature['repair_history'].append(prior)
                feature['repair_attempts'] = 3
                retained_generated_bytes = retained_generated.read_bytes()
                feature['edits'].append({
                    'path': 'site/retained-generated.html',
                    'content': retained_generated_bytes.decode(),
                    'before': hashlib.sha256(retained_generated_bytes).hexdigest(),
                    'asset': None})
                feature.update(status='failed', checkpoint='tool_effects_quarantined',
                    review_status='interrupted', review_pending=None, last_failure_kind='operational',
                    auto_repair_lifecycle='held', tool_workspace_revision=7,
                    review_summary='Tool session changed files that cannot enter bounded review: route.png')
                durable['revision'] += 1
                database.execute('UPDATE developer_state SET state=? WHERE id=1', (json.dumps(durable),))
                mutations = [
                    {'revision': 1, 'request_id': str(uuid.uuid4()), 'feature_id': feature_id,
                     'edits': [
                         {'path': 'pip/cache/selfcheck/state.json', 'before_sha256': None,
                          'after': '{"checked":false}\n'},
                         {'path': 'tests/test_obsolete.py', 'before_sha256': None,
                          'after': 'assert False\n'}],
                     'unreviewable_paths': [
                         'route.png', 'pip/cache/http-v2/0/e/1/a/1/response.body',
                         'tests/__pycache__/test_site.cpython-312.pyc']},
                    {'revision': 6, 'request_id': str(uuid.uuid4()), 'feature_id': None,
                     'edits': [{'path': 'source/new-owner-source.py', 'before_sha256': None,
                                'after': '# stale side-chat bytes\n'}],
                     'unreviewable_paths': []},
                    {'revision': 7, 'request_id': str(uuid.uuid4()), 'feature_id': None,
                     'edits': [], 'unreviewable_paths': ['site/route.png']},
                ]
                database.execute('INSERT OR REPLACE INTO developer_tool_workspace VALUES (?,?)',
                                 ('large-repair', 7))
                for mutation in mutations:
                    database.execute('INSERT OR REPLACE INTO developer_tool_mutation VALUES (?,?,?,?,?)',
                        ('large-repair', mutation['revision'], mutation['request_id'],
                         mutation['feature_id'], json.dumps(mutation)))
            launch()
            held = api()['queue'][0]
            old_digest = held['asset_recovery_sha256']
            (project / 'source' / 'new-owner-source.py').write_text('# Explicitly adopted current source\n')
            (project / 'site' / 'route.png').write_bytes(image)
            binding = {'action': 'resume', 'expected_feature_id': feature_id,
                       'expected_model_target': held['model_target'], 'expected_status': held['status'],
                       'expected_checkpoint': held['checkpoint'], 'expected_asset_recovery_sha256': old_digest}
            try:
                api('control', binding)
                raise AssertionError('Stale owner adoption digest was accepted')
            except urllib.error.HTTPError as error:
                assert error.code == 409, error.code
            fresh = api()['queue'][0]
            assert fresh['checkpoint'] == 'tool_effects_quarantined'
            assert fresh['asset_recovery_sha256'] != old_digest
            binding['expected_asset_recovery_sha256'] = fresh['asset_recovery_sha256']
            api('control', binding)
            state = wait(lambda s: not s['running'] and s['queue'][0]['status'] == 'succeeded')
            assert len(calls) == previous_calls, 'Owner adoption replayed implementation'
            assert {'source/new-owner-source.py', 'site/route.png',
                    'site/retained-generated.html'} <= set(state['queue'][0]['changed_files'])
            assert not ({'tests/test_obsolete.py', 'pip/cache/selfcheck/state.json',
                         'pip/cache/http-v2/0/e/1/a/1/response.body'}
                        & set(state['queue'][0]['changed_files']))
            assert state['queue'][0]['repair_attempts'] == 3
            assert state['queue'][0]['escalation_count'] == 0
            final_evidence = [json.loads(line) for line in (root / 'review-fixture/review-input-evidence.jsonl').read_text().splitlines()]
            reviewed = {(entry['path'], entry['content_sha256'])
                        for item in final_evidence if item.get('kind') == 'review_batch'
                        for entry in item['entries']}
            for path in ('source/new-owner-source.py', 'site/route.png',
                         'site/retained-generated.html'):
                assert (path, hashlib.sha256((project / path).read_bytes()).hexdigest()) in reviewed, path

            # Reproduce the exact live checkpoint left after all three ordinary
            # repairs and an unavailable manual proposal. Resume may adopt only
            # a freshly displayed current-snapshot digest; it must not replay an
            # implementation or erase the retained counters and proposal.
            stop()
            checkpoint_validation_gate = root / 'checkpoint-validation-gate'
            # Historical chat tests can retain pytest's hidden output among the
            # cumulative edits. It must survive on disk without becoming an
            # editable source path in exact-snapshot adoption or review.
            pytest_cache_path = '.pytest_cache/v/cache/nodeids'
            pytest_cache_bytes = b'["tests/test_site.py::test_build"]\n'
            pytest_cache_file = project / pytest_cache_path
            pytest_cache_file.parent.mkdir(parents=True, exist_ok=True)
            pytest_cache_file.write_bytes(pytest_cache_bytes)
            sensitive_cache_file = project / '.pytest_cache/credentials.toml'
            sensitive_cache_file.write_bytes(b'fixture-cache-value-a\n')
            checkpoint_validation_started = root / 'checkpoint-validation-started'
            checkpoint_validation_gate.write_text('hold exact snapshot validation')
            checkpoint_validation_code = '; '.join([
                'from pathlib import Path', 'import signal,sys,time',
                f"started=Path({str(checkpoint_validation_started)!r})",
                f"gate=Path({str(checkpoint_validation_gate)!r})",
                "signal.signal(signal.SIGTERM, lambda *_: sys.exit(130))",
                "started.write_bytes(b'started')",
                "deadline=time.monotonic()+30",
                "exec('while gate.exists() and time.monotonic() < deadline:\\n time.sleep(0.05)')",
                "assert not gate.exists()",
                'import app', 'assert app.VALUE == 1',
            ])
            checkpoint_validation = (subprocess.list2cmdline(
                [sys.executable, '-B', '-c', checkpoint_validation_code])
                if os.name == 'nt' else shlex.join(
                    [sys.executable, '-B', '-c', checkpoint_validation_code]))
            with closing(sqlite3.connect(data / 'developer.sqlite3')) as database, database:
                durable = json.loads(database.execute(
                    'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                feature = next(item for item in durable['queue_v13'] if item['id'] == feature_id)
                durable['auto_ai_repair_max_escalations'] = 100
                feature['edits'].append({'path': pytest_cache_path,
                    'content': pytest_cache_bytes.decode(), 'before': None})
                diagnosis = 'No further correction is needed'
                diagnosis_sha256 = hashlib.sha256(diagnosis.encode()).hexdigest()
                proposal_id = str(uuid.uuid4())
                chat_request_id = str(uuid.uuid4())
                candidate_sha256 = hashlib.sha256(b'{"files":[]}').hexdigest()
                feature.update(status='failed',
                    checkpoint='tool_workspace_changed_requires_proposal',
                    validation=checkpoint_validation,
                    repair_pending=False, escalation_count=34, escalation_pending=False,
                    review_status='unavailable', review_pending=None,
                    last_failure_kind='operational', auto_ai_repair_limit=100,
                    auto_repair_lifecycle='quarantined', auto_repair_epoch=19,
                    auto_repair_step_started_at_ms=None,
                    auto_repair_reason='Automatic repair stopped after project files were applied')
                feature['escalation_proposal'] = {
                    'proposal_id': proposal_id, 'attempt': 34,
                    'feature_id': feature_id,
                    'feature_checkpoint': 'tool_workspace_changed_requires_proposal',
                    'binding_revision': durable['revision'] + 1,
                    'model_target': feature['model_target'], 'model': 'fixture',
                    'chat_request_id': chat_request_id, 'chat_model_target': 'windows',
                    'chat_model': 'fixture', 'diagnosis': diagnosis,
                    'diagnosis_sha256': diagnosis_sha256,
                    'status': 'unavailable',
                    'summary': 'The selected AI did not provide a bounded file proposal',
                    'error': 'Expected 1 to 40 proposed files', 'files': [],
                    'source': 'manual_chat', 'limit_snapshot': 100,
                    'review_slot_terminal': False,
                }
                terminal_metadata = {
                    'proposal_id': proposal_id, 'attempt': 34,
                    'model_target': feature['model_target'], 'model': 'fixture',
                    'chat_request_id': chat_request_id,
                    'diagnosis_sha256': diagnosis_sha256, 'proposal_sha256': None,
                    'candidate_sha256': candidate_sha256,
                    'summary': 'Proposal preparation did not produce an authorized application',
                    'source': 'manual_chat', 'automatic_epoch': None,
                    'policy_revision': None, 'limit_snapshot': 100,
                    'project_state_sha256': None,
                }
                durable['revision'] += 1
                database.execute('UPDATE developer_state SET state=? WHERE id=1',
                    (json.dumps(durable),))
            launch()
            incomplete = api()['queue'][0]
            assert incomplete.get('asset_recovery_sha256') is None, incomplete
            stop()
            with closing(sqlite3.connect(data / 'developer.sqlite3')) as database, database:
                durable = json.loads(database.execute(
                    'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                feature = next(item for item in durable['queue_v13'] if item['id'] == feature_id)
                feature['escalation_proposal']['review_slot_terminal'] = True
                feature['escalation_history'].extend([
                    dict(terminal_metadata, outcome='unavailable'),
                    dict(terminal_metadata, outcome='authorization_not_run'),
                    dict(terminal_metadata, outcome='application_not_run'),
                ])
                feature['review_history'].append({
                    'attempt': 34, 'packet_sha256': candidate_sha256,
                    'validation_evidence_sha256': diagnosis_sha256,
                    'outcome': 'not_run', 'decision_sha256': None,
                    'binding_version': 0, 'batch_packet_sha256s': [],
                    'batch_receipt_sha256s': [], 'blocking_findings': [],
                    'summary': 'Independent review was not run because proposal preparation failed',
                })
                durable['revision'] += 1
                database.execute('UPDATE developer_state SET state=? WHERE id=1',
                    (json.dumps(durable),))
            launch()
            held = api()['queue'][0]
            checkpoint_digest = held.get('asset_recovery_sha256')
            assert checkpoint_digest and len(checkpoint_digest) == 64, held
            checkpoint_binding = {'action': 'resume', 'expected_feature_id': feature_id,
                'expected_model_target': held['model_target'], 'expected_status': held['status'],
                'expected_checkpoint': held['checkpoint']}
            try:
                api('control', checkpoint_binding)
                raise AssertionError('Checkpoint recovery accepted a missing snapshot digest')
            except urllib.error.HTTPError as error:
                assert error.code in (400, 409), error.code
            sensitive_cache_file.write_bytes(b'fixture-cache-value-b\n')
            try:
                api('control', dict(checkpoint_binding,
                    expected_asset_recovery_sha256=checkpoint_digest))
                raise AssertionError('Checkpoint recovery accepted redacted cache drift')
            except urllib.error.HTTPError as error:
                assert error.code == 409, error.code
            cache_fresh = api()['queue'][0]
            cache_digest = cache_fresh.get('asset_recovery_sha256')
            assert cache_digest and cache_digest != checkpoint_digest, cache_fresh
            checkpoint_digest = cache_digest
            (project / 'source' / 'new-owner-source.py').write_bytes(
                b'# Exact checkpoint owner snapshot\n')
            try:
                api('control', dict(checkpoint_binding,
                    expected_asset_recovery_sha256=checkpoint_digest))
                raise AssertionError('Checkpoint recovery accepted a stale snapshot digest')
            except urllib.error.HTTPError as error:
                assert error.code == 409, error.code
            fresh = api()['queue'][0]
            fresh_digest = fresh.get('asset_recovery_sha256')
            assert fresh_digest and fresh_digest != checkpoint_digest, fresh
            project_files_before_adoption = {str(path.relative_to(project)):
                hashlib.sha256(path.read_bytes()).hexdigest()
                for path in project.rglob('*') if path.is_file()}
            model_calls_before_checkpoint_adoption = (len(calls), len(opencode_calls))
            api('control', dict(checkpoint_binding,
                expected_asset_recovery_sha256=fresh_digest))
            wait(lambda unused: checkpoint_validation_started.exists())
            api('control', {'action': 'stop'})
            checkpoint_validation_gate.unlink()
            stopped_state = wait(lambda s: not s['running'])
            stopped_checkpoint = stopped_state['queue'][0]
            assert stopped_checkpoint['checkpoint'] != 'auto_repair_effects_quarantined', \
                stopped_checkpoint
            assert stopped_checkpoint['repair_attempts'] == 3
            assert stopped_checkpoint['escalation_count'] == 34
            if stopped_checkpoint['status'] == 'paused':
                assert stopped_checkpoint['auto_repair_lifecycle'] == 'inactive', \
                    stopped_checkpoint
                resume(feature_id)
            else:
                # A platform can conservatively report unconfirmed process-tree
                # cleanup. Preserve that stronger safety boundary, then restore
                # the same terminal fixture to prove the ordinary success path.
                assert stopped_checkpoint['status'] == 'failed', stopped_checkpoint
                assert stopped_checkpoint['checkpoint'] == 'validation_cleanup_unconfirmed', \
                    stopped_checkpoint
                assert stopped_checkpoint['auto_repair_lifecycle'] == 'quarantined', \
                    stopped_checkpoint
                assert stopped_state['emergency_paused'], stopped_state
                api('control', {'action': 'clear_emergency'})
                stop()
                with closing(sqlite3.connect(data / 'developer.sqlite3')) as database, database:
                    durable = json.loads(database.execute(
                        'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                    feature = next(item for item in durable['queue_v13']
                        if item['id'] == feature_id)
                    feature.update(status='failed',
                        checkpoint='tool_workspace_changed_requires_proposal',
                        repair_pending=False, escalation_pending=False,
                        review_status='unavailable', review_pending=None,
                        last_failure_kind='operational', auto_repair_lifecycle='quarantined',
                        auto_repair_step_started_at_ms=None,
                        auto_repair_reason='Automatic repair stopped after project files were applied')
                    durable['revision'] += 1
                    database.execute('UPDATE developer_state SET state=? WHERE id=1',
                        (json.dumps(durable),))
                launch()
                restored = api()['queue'][0]
                restored_digest = restored.get('asset_recovery_sha256')
                assert restored_digest and len(restored_digest) == 64, restored
                api('control', dict(checkpoint_binding,
                    expected_asset_recovery_sha256=restored_digest))
            state = wait(lambda s: not s['running'] and s['queue'][0]['status'] == 'succeeded')
            recovered = state['queue'][0]
            assert recovered['repair_attempts'] == 3
            assert recovered['escalation_count'] == 34
            assert model_calls_before_checkpoint_adoption == (len(calls), len(opencode_calls)), \
                'Exact checkpoint adoption replayed implementation'
            assert project_files_before_adoption == {str(path.relative_to(project)):
                hashlib.sha256(path.read_bytes()).hexdigest()
                for path in project.rglob('*') if path.is_file()}, \
                'Exact checkpoint adoption replayed file writes'
            checkpoint_evidence = [json.loads(line) for line in
                (root / 'review-fixture/review-input-evidence.jsonl').read_text().splitlines()]
            assert pytest_cache_file.read_bytes() == pytest_cache_bytes
            assert not any(entry['path'] == pytest_cache_path
                for item in checkpoint_evidence if item.get('kind') == 'review_batch'
                for entry in item['entries']), 'Pytest cache entered source review'
            assert any(item.get('kind') == 'review_batch'
                and any(entry['path'] == 'source/new-owner-source.py'
                    and entry['content_sha256'] == hashlib.sha256(
                        (project / 'source/new-owner-source.py').read_bytes()).hexdigest()
                    for entry in item['entries'])
                for item in checkpoint_evidence), 'Current checkpoint snapshot was not reviewed'

            # Emergency Pause at the same validation-only boundary must preserve
            # resumability and never manufacture staged automatic-write evidence.
            stop()
            if checkpoint_validation_started.exists():
                checkpoint_validation_started.unlink()
            checkpoint_validation_gate.write_text('hold emergency validation')
            with closing(sqlite3.connect(data / 'developer.sqlite3')) as database, database:
                durable = json.loads(database.execute(
                    'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                feature = next(item for item in durable['queue_v13'] if item['id'] == feature_id)
                feature.update(status='failed',
                    checkpoint='tool_workspace_changed_requires_proposal',
                    repair_pending=False, escalation_pending=False,
                    review_status='unavailable', review_pending=None,
                    last_failure_kind='operational', auto_repair_lifecycle='quarantined',
                    auto_repair_step_started_at_ms=None,
                    auto_repair_reason='Automatic repair stopped after project files were applied')
                durable['revision'] += 1
                database.execute('UPDATE developer_state SET state=? WHERE id=1',
                    (json.dumps(durable),))
            launch()
            emergency_held = api()['queue'][0]
            emergency_digest = emergency_held.get('asset_recovery_sha256')
            assert emergency_digest and len(emergency_digest) == 64, emergency_held
            api('control', {'action': 'resume', 'expected_feature_id': feature_id,
                'expected_model_target': emergency_held['model_target'],
                'expected_status': emergency_held['status'],
                'expected_checkpoint': emergency_held['checkpoint'],
                'expected_asset_recovery_sha256': emergency_digest})
            wait(lambda unused: checkpoint_validation_started.exists())
            api('control', {'action': 'emergency'})
            checkpoint_validation_gate.unlink()
            emergency_state = wait(lambda s: not s['running'] and s['emergency_paused'])
            emergency_feature = emergency_state['queue'][0]
            assert emergency_feature['checkpoint'] != 'auto_repair_effects_quarantined', \
                emergency_feature
            if emergency_feature['status'] == 'paused':
                assert emergency_feature['auto_repair_lifecycle'] == 'inactive', \
                    emergency_feature
            else:
                # Emergency Pause deliberately escalates uncertain process-tree
                # cleanup. That fail-closed quarantine is valid so long as it
                # cannot be mistaken for staged automatic file application.
                assert emergency_feature['status'] == 'failed', emergency_feature
                assert emergency_feature['checkpoint'] == 'validation_cleanup_unconfirmed', \
                    emergency_feature
                assert emergency_feature['auto_repair_lifecycle'] == 'quarantined', \
                    emergency_feature
            assert emergency_feature['repair_attempts'] == 3
            assert emergency_feature['escalation_count'] == 34
            with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
                emergency_durable = json.loads(database.execute(
                    'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
            emergency_durable_feature = next(item for item in emergency_durable['queue_v13']
                if item['id'] == feature_id)
            emergency_proposal = emergency_durable_feature['escalation_proposal']
            assert emergency_proposal['source'] == 'manual_chat'
            assert emergency_proposal['status'] == 'unavailable'
            assert emergency_proposal.get('automatic_epoch') is None
            assert emergency_proposal.get('application_state_sha256') is None
            assert not emergency_proposal.get('staged_candidate')
            assert not emergency_proposal.get('applied_paths')
            api('control', {'action': 'clear_emergency'})
            if emergency_feature['status'] == 'paused':
                resume(feature_id)
            else:
                stop()
                with closing(sqlite3.connect(data / 'developer.sqlite3')) as database, database:
                    durable = json.loads(database.execute(
                        'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                    feature = next(item for item in durable['queue_v13']
                        if item['id'] == feature_id)
                    feature.update(status='failed',
                        checkpoint='tool_workspace_changed_requires_proposal',
                        repair_pending=False, escalation_pending=False,
                        review_status='unavailable', review_pending=None,
                        last_failure_kind='operational', auto_repair_lifecycle='quarantined',
                        auto_repair_step_started_at_ms=None,
                        auto_repair_reason='Automatic repair stopped after project files were applied')
                    durable['revision'] += 1
                    database.execute('UPDATE developer_state SET state=? WHERE id=1',
                        (json.dumps(durable),))
                launch()
                restored = api()['queue'][0]
                restored_digest = restored.get('asset_recovery_sha256')
                assert restored_digest and len(restored_digest) == 64, restored
                api('control', dict(checkpoint_binding,
                    expected_asset_recovery_sha256=restored_digest))
            wait(lambda s: not s['running'] and s['queue'][0]['status'] == 'succeeded')
            assert model_calls_before_checkpoint_adoption == (len(calls), len(opencode_calls))
            assert project_files_before_adoption == {str(path.relative_to(project)):
                hashlib.sha256(path.read_bytes()).hexdigest()
                for path in project.rglob('*') if path.is_file()}


            # Exercise the approved staged automatic-repair boundary with the
            # real pinned OpenCode process and a deterministic loopback model.
            # The live project must remain byte-for-byte untouched until the
            # exact typed text+asset candidate has been policy-authorized.
            api('control', {'action': 'auto_run', 'enabled': False})
            staged_project = projects / 'staged-automatic'
            staged_project.mkdir()
            staged_feature_id = str(uuid.uuid4())
            staged_validation = f'"{sys.executable}" tests/test_existing.py'
            staged_instruction = ('staged-automatic: correct VALUE, add a focused regression, '
                'and rebuild dist output with a real route PNG.'
                + (' [fixture:wait]' if os.name == 'nt' else ''))
            staged_state = enqueue_with_plan(f'http://127.0.0.1:{port}', token, {
                'id': staged_feature_id, 'project': 'staged-automatic',
                'instruction': staged_instruction,
                'validation': staged_validation, 'model_target': 'windows'})
            staged_feature = next(item for item in staged_state['queue']
                if item['id'] == staged_feature_id)
            resume(staged_feature_id)
            exhausted = wait(lambda s: not s['running'] and next(item for item in s['queue']
                if item['id'] == staged_feature_id)['auto_repair_lifecycle'] == 'held', 120)
            exhausted_feature = next(item for item in exhausted['queue']
                if item['id'] == staged_feature_id)
            assert exhausted_feature['status'] == 'failed'
            assert exhausted_feature['repair_attempts'] == 3
            assert exhausted_feature['escalation_count'] == 1
            stale_source = (staged_project / 'app.py').read_bytes()
            assert stale_source != b'VALUE = 1\n'
            py_compile.compile(str(staged_project / 'app.py'), doraise=True,
                invalidation_mode=py_compile.PycInvalidationMode.UNCHECKED_HASH)
            staged_cache_before = validation_cache_snapshot(staged_project)
            assert any(path.endswith('.pyc') for path in staged_cache_before), staged_cache_before
            live_app_before_authorization = (staged_project / 'app.py').read_bytes()
            protected_test = staged_project / 'tests/test_existing.py'
            protected_test_before = protected_test.read_bytes()
            protected_test_after = protected_test_before.replace(
                b'assert app.VALUE == 999\n', b'assert app.VALUE == 1\n')
            existing_generated = staged_project / 'dist/index.html'
            existing_generated_before = existing_generated.read_bytes()
            existing_generated_before_sha256 = hashlib.sha256(existing_generated_before).hexdigest()
            obsolete_generated = staged_project / 'dist/obsolete-generated.html'
            obsolete_generated_before = obsolete_generated.read_bytes()
            obsolete_generated_before_sha256 = hashlib.sha256(
                obsolete_generated_before).hexdigest()
            (staged_project / '.env.local').write_text('PRIVATE=live-only\n')
            (staged_project / '.aws').mkdir()
            (staged_project / '.aws/credentials').write_text('live-only credentials\n')
            (staged_project / 'ordinary-notes.txt').write_text('Bearer fixture-private-value\n')
            (staged_project / 'safe-notes.txt').write_text('safe staged context\n')

            stop()
            opencode_enabled = True
            launch()
            assert set_global_permissions('full')['mode'] == 'full'
            stage_entered.clear()
            stage_release.clear()
            resume_errors = []
            def resume_staged():
                try:
                    resume(staged_feature_id)
                except Exception as error:
                    resume_errors.append(error)
            resume_thread = threading.Thread(target=resume_staged, daemon=True)
            resume_thread.start()
            assert stage_entered.wait(90), (api(), (root / 'runner.log').read_text(errors='replace'))
            with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
                live_workspace_revision = database.execute(
                    'SELECT revision FROM developer_tool_workspace WHERE project=?',
                    ('staged-automatic',)).fetchone()
                stage_rows_during_preparation = database.execute(
                    'SELECT COUNT(*) FROM developer_tool_stage WHERE project=?',
                    ('staged-automatic',)).fetchone()[0]
                stage_project_name = database.execute(
                    'SELECT stage_project FROM developer_tool_stage WHERE project=?',
                    ('staged-automatic',)).fetchone()[0]
            assert live_workspace_revision is None or live_workspace_revision[0] == 0
            assert stage_rows_during_preparation == 1
            stage_project_path = projects / stage_project_name
            assert (stage_project_path / 'safe-notes.txt').read_text() == 'safe staged context\n'
            assert not (stage_project_path / '.env.local').exists()
            assert not (stage_project_path / '.aws').exists()
            assert not (stage_project_path / 'ordinary-notes.txt').exists()
            assert (staged_project / '.env.local').read_text() == 'PRIVATE=live-only\n'
            assert live_app_before_authorization != b'VALUE = 1\n'
            assert protected_test.read_bytes() == protected_test_before
            assert existing_generated.read_bytes() == existing_generated_before
            assert obsolete_generated.read_bytes() == obsolete_generated_before
            assert not (staged_project / 'dist/route.png').exists()
            assert not (staged_project / 'tests/test_staged_regression.py').exists()
            assert validation_cache_snapshot(staged_project) == staged_cache_before
            preparing = api('repair/escalation?id=' + staged_feature_id)
            assert preparing['status'] == 'preparing'
            assert 'stage' not in preparing and 'staged_binding' not in preparing
            restart_captured = threading.Event()
            restart_capture = {}
            restart_capture_errors = []
            negative_state = root / 'restart-negative-state'
            negative_projects = root / 'restart-negative-projects'
            partial_application_timeout = 300

            def capture_partial_application():
                # The pinned OpenCode session performs 25 audited probes before
                # the staged write. Hosted Windows can spend more than 90 seconds
                # in that real tool loop under CI load, before application begins.
                deadline = time.monotonic() + partial_application_timeout
                last_observed = 0
                last_observed_state = None
                last_observation_error = None
                while time.monotonic() < deadline:
                    try:
                        with closing(sqlite3.connect(
                                f'file:{data / "developer.sqlite3"}?mode=ro', uri=True,
                                timeout=.05)) as database:
                            durable = json.loads(database.execute(
                                'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                        current = next(item for item in durable['queue_v13']
                            if item['id'] == staged_feature_id)
                        proposal = current.get('escalation_proposal') or {}
                        applied_paths = proposal.get('applied_paths') or []
                        staged_candidate = proposal.get('staged_candidate') or []
                        escalation_history = current.get('escalation_history') or []
                        last_observed_state = {
                            'proposal_status': proposal.get('status'),
                            'proposal_summary': (proposal.get('summary') or '')[:1_000],
                            'proposal_error': (proposal.get('error') or '')[:1_000],
                            'applied_count': len(applied_paths),
                            'candidate_count': len(staged_candidate),
                            'feature_checkpoint': current.get('checkpoint'),
                            'feature_message': (current.get('message') or '')[:1_000],
                            'auto_repair_lifecycle': current.get('auto_repair_lifecycle'),
                            'last_escalation_history': [{
                                'proposal_id': item.get('proposal_id'),
                                'attempt': item.get('attempt'),
                                'outcome': item.get('outcome'),
                                'summary': (item.get('summary') or '')[:1_000],
                            } for item in escalation_history[-3:]],
                            'runner_exit_code': process.poll(),
                        }
                        terminal_proposal = proposal.get('status') in {
                            'unavailable', 'cancelled', 'no_op', 'duplicate', 'interrupted'}
                        terminal_lifecycle = current.get('auto_repair_lifecycle') in {
                            'held', 'quarantined', 'limit_reached'}
                        if not applied_paths and (terminal_proposal or terminal_lifecycle):
                            raise AssertionError({
                                'error': 'staged preparation terminated before partial application',
                                'last_observed_state': last_observed_state,
                            })
                        if (proposal.get('status') != 'applying' or not applied_paths
                                or len(applied_paths) >= len(staged_candidate)):
                            time.sleep(.001)
                            continue
                        if len(applied_paths) == last_observed:
                            time.sleep(.001)
                            continue
                        last_observed = len(applied_paths)
                        suspension = suspend_runner()
                        try:
                            with closing(sqlite3.connect(
                                    f'file:{data / "developer.sqlite3"}?mode=ro', uri=True,
                                    timeout=1)) as database:
                                frozen = json.loads(database.execute(
                                    'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                                stage_archive = database.execute(
                                    'SELECT status,mutation_sha256,mutation_count FROM '
                                    'developer_tool_stage WHERE project=?',
                                    ('staged-automatic',)).fetchone()
                                raw_stage_rows = database.execute(
                                    'SELECT COUNT(*) FROM developer_tool_stage_mutation').fetchone()[0]
                            frozen_feature = next(item for item in frozen['queue_v13']
                                if item['id'] == staged_feature_id)
                            frozen_proposal = frozen_feature['escalation_proposal']
                            candidate = frozen_proposal['staged_candidate']
                            frozen_applied = set(frozen_proposal['applied_paths'])
                            exact_applied = []
                            exact_pending = []
                            ambiguous = []
                            for entry in candidate:
                                path = staged_project / entry['path']
                                current_bytes = path.read_bytes() if path.is_file() else None
                                if entry.get('delete'):
                                    after = None
                                elif entry.get('asset') is not None:
                                    after = base64.b64decode(entry['asset']['data_base64'])
                                else:
                                    after = entry['content'].encode()
                                before = entry.get('before')
                                matches_after = current_bytes == after
                                matches_before = ((current_bytes is None and before is None)
                                    or (current_bytes is not None and before is not None
                                        and hashlib.sha256(current_bytes).hexdigest() == before))
                                if entry['path'] in frozen_applied and matches_after:
                                    exact_applied.append(entry['path'])
                                elif entry['path'] not in frozen_applied and matches_before:
                                    exact_pending.append(entry['path'])
                                else:
                                    ambiguous.append(entry['path'])
                            if ambiguous or not exact_applied or not exact_pending:
                                last_observed_state.update({
                                    'frozen_applied_count': len(frozen_applied),
                                    'exact_applied_count': len(exact_applied),
                                    'exact_pending_count': len(exact_pending),
                                    'ambiguous_count': len(ambiguous),
                                })
                                resume_runner(suspension)
                                continue
                            ready = [item for item in frozen_feature['escalation_history']
                                if item['proposal_id'] == frozen_proposal['proposal_id']
                                and item['outcome'] == 'ready']
                            assert len(ready) == 1
                            assert stage_archive[0] == 'compacted'
                            assert len(stage_archive[1]) == 64 and stage_archive[2] > 0
                            assert raw_stage_rows == 0
                            restart_capture.update({
                                'apply_request_id': frozen_proposal['apply_request_id'],
                                'proposal_id': frozen_proposal['proposal_id'],
                                'candidate_sha256': ready[0]['candidate_sha256'],
                                'stage_mutation_sha256': stage_archive[1],
                                'candidate_count': len(candidate),
                                'applied_paths': exact_applied,
                                'pending_paths': exact_pending,
                                'applied_mtimes': {path: (staged_project / path).stat().st_mtime_ns
                                    for path in exact_applied
                                    if (staged_project / path).is_file()},
                                'opencode_calls': len(opencode_calls),
                            })
                            negative_state.mkdir()
                            with closing(sqlite3.connect(negative_state / 'developer.sqlite3')) \
                                    as negative_database, closing(sqlite3.connect(
                                        f'file:{data / "developer.sqlite3"}?mode=ro',
                                        uri=True)) as source_database:
                                source_database.backup(negative_database)
                            shutil.copytree(projects, negative_projects)
                            process.kill()
                            process.wait(timeout=15)
                            if os.name == 'nt':
                                import ctypes
                                ctypes.windll.kernel32.CloseHandle(suspension)
                            restart_captured.set()
                            return
                        except BaseException:
                            if process.poll() is None:
                                resume_runner(suspension)
                            raise
                    except (OSError, sqlite3.Error) as error:
                        last_observation_error = f'{type(error).__name__}: {error}'
                        time.sleep(.001)
                raise AssertionError({
                    'error': 'runner did not expose a durable partial staged application',
                    'timeout_seconds': partial_application_timeout,
                    'last_observed_state': last_observed_state,
                    'last_observation_error': last_observation_error,
                    'runner_exit_code': process.poll(),
                    'runner_log_tail': (root / 'runner.log').read_text(
                        errors='replace')[-8_000:],
                })

            def observe_restart():
                try:
                    capture_partial_application()
                except BaseException as error:
                    restart_capture_errors.append(error)
                    restart_captured.set()

            restart_observer = threading.Thread(target=observe_restart, daemon=True)
            restart_observer.start()
            stage_release.set()
            resume_thread.join(30)
            assert not resume_thread.is_alive() and not resume_errors, resume_errors
            assert restart_captured.wait(partial_application_timeout + 5), \
                'partial staged application was not captured'
            restart_observer.join(5)
            assert not restart_observer.is_alive() and not restart_capture_errors, \
                restart_capture_errors
            assert 0 < len(restart_capture['applied_paths']) < \
                restart_capture['candidate_count']
            assert len(restart_capture['pending_paths']) + len(
                restart_capture['applied_paths']) == restart_capture['candidate_count']
            apply_started = time.monotonic()
            review_gate = root / 'review-fixture/staged-review.gate'
            review_gate.write_text('Hold the first completed-candidate review for a real Stop')
            native_review_started = root / 'review-fixture/started.pid'
            if native_review_started.exists():
                native_review_started.unlink()
            review_gate_started = review_gate.with_suffix('.started')
            if review_gate_started.exists():
                review_gate_started.unlink()
            provider_unavailable_observed = False
            adopted_after_stop = False
            launch(wait_until_idle=False)
            apply_timeline = []
            last_progress = None
            deadline = apply_started + 600
            while True:
                try:
                    staged_complete = api()
                except OSError:
                    if time.monotonic() >= deadline:
                        raise AssertionError((apply_timeline,
                            (root / 'runner.log').read_text(errors='replace')))
                    time.sleep(.05)
                    continue
                observed = next(item for item in staged_complete['queue']
                    if item['id'] == staged_feature_id)
                progress = (observed['status'], observed['checkpoint'], observed['message'],
                            observed['review_status'])
                if progress != last_progress:
                    apply_timeline.append((round(time.monotonic() - apply_started, 3), progress))
                    last_progress = progress
                review_started = review_gate_started.exists()
                if (not adopted_after_stop and observed['review_status'] == 'reviewing'
                        and review_started):
                    if not provider_unavailable_observed:
                        # Terminate the actual first provider process while its
                        # batch is held. The durable unavailable decision must
                        # preserve the exact applied candidate so Resume can
                        # revalidate it without model inference or file replay.
                        provider_pid = int(review_gate_started.read_text())
                        unavailable_files = {str(path.relative_to(staged_project)):
                            (hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mtime_ns)
                            for path in staged_project.rglob('*') if path.is_file()}
                        unavailable_cache = validation_cache_snapshot(staged_project)
                        unavailable_model_counts = (len(calls), len(opencode_calls))
                        unavailable_validation_count = len(
                            (root / 'staged-validation-events').read_text().splitlines())
                        with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
                            unavailable_durable = json.loads(database.execute(
                                'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                        unavailable_feature = next(item for item in unavailable_durable['queue_v13']
                            if item['id'] == staged_feature_id)
                        unavailable_proposal = unavailable_feature['escalation_proposal']
                        unavailable_identity = (unavailable_proposal['proposal_id'],
                            unavailable_proposal['apply_request_id'],
                            unavailable_proposal['application_state_sha256'])
                        unavailable_receipts = [item for item in
                            unavailable_feature['escalation_history']
                            if item['proposal_id'] == unavailable_proposal['proposal_id']]
                        assert unavailable_receipts
                        assert all(item['candidate_sha256'] == restart_capture['candidate_sha256']
                            for item in unavailable_receipts)
                        candidate_entry_keys = ('path', 'kind', 'content_sha256',
                            'before_sha256', 'media_type', 'width', 'height')
                        unavailable_terminal = api('repair/escalation?id=' + staged_feature_id)
                        assert unavailable_terminal['candidate_sha256'] == \
                            restart_capture['candidate_sha256']
                        unavailable_candidate_entries = [{key: entry.get(key)
                            for key in candidate_entry_keys}
                            for entry in unavailable_terminal['candidate_entries']]
                        terminate_fixture_process(provider_pid)
                        # Runners with bounded provider-outage recovery may
                        # retry the same frozen review once. Exercise exhaustion
                        # by terminating that distinct real provider too; older
                        # runners can expose unavailable after the first exit.
                        def unavailable_or_provider_retry(state):
                            current = next(item for item in state['queue']
                                if item['id'] == staged_feature_id)
                            if not state['running']:
                                return True
                            try:
                                return int(review_gate_started.read_text()) != provider_pid
                            except (OSError, ValueError):
                                return False
                        outage_state = wait(unavailable_or_provider_retry)
                        if arguments.provider_retry_success:
                            assert outage_state['running'], 'bounded provider retry did not start'
                            retry_pid = int(review_gate_started.read_text())
                            assert retry_pid != provider_pid
                            review_gate.unlink()
                            retry_success = wait(lambda s: not s['running'])
                            retry_feature = next(item for item in retry_success['queue']
                                if item['id'] == staged_feature_id)
                            assert retry_feature['status'] == 'succeeded', retry_feature
                            assert retry_feature['review_status'] == 'approved', retry_feature
                            assert unavailable_files == {str(path.relative_to(staged_project)):
                                (hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mtime_ns)
                                for path in staged_project.rglob('*') if path.is_file()}
                            assert validation_cache_snapshot(staged_project) == unavailable_cache
                            assert unavailable_model_counts == (len(calls), len(opencode_calls))
                            assert unavailable_validation_count == len(
                                (root / 'staged-validation-events').read_text().splitlines())
                            with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
                                retry_durable = json.loads(database.execute(
                                    'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                            retry_record = next(item for item in retry_durable['queue_v13']
                                if item['id'] == staged_feature_id)
                            retry_proposal = retry_record['escalation_proposal']
                            assert unavailable_identity == (retry_proposal['proposal_id'],
                                retry_proposal['apply_request_id'],
                                retry_proposal['application_state_sha256'])
                            retry_receipts = [item for item in
                                retry_record['escalation_history']
                                if item['proposal_id'] == retry_proposal['proposal_id']]
                            assert retry_receipts[:-1] == unavailable_receipts
                            assert retry_receipts[-1]['outcome'] == 'succeeded'
                            assert all(item['candidate_sha256'] == restart_capture['candidate_sha256']
                                for item in retry_receipts)
                            retry_terminal = api('repair/escalation?id=' + staged_feature_id)
                            assert retry_terminal['candidate_sha256'] == \
                                restart_capture['candidate_sha256']
                            assert [{key: entry.get(key) for key in candidate_entry_keys}
                                for entry in retry_terminal['candidate_entries']] == \
                                unavailable_candidate_entries
                            assert retry_terminal['candidate_payload_state'] == 'hash_only'
                            assert all('after' not in entry
                                for entry in retry_terminal['candidate_entries'])
                            assert retry_record['review_history'][-1]['outcome'] == 'approved'
                            print(json.dumps({'platform': sys.platform,
                                'staged_review_success_after_one_provider_exit': True,
                                'candidate_identity_and_bytes_preserved': True,
                                'no_model_write_or_validation_replay': True}))
                            return
                        if outage_state['running']:
                            retry_pid = int(review_gate_started.read_text())
                            assert retry_pid != provider_pid
                            terminate_fixture_process(retry_pid)
                        unavailable_state = wait(lambda s: not s['running'] and next(item
                            for item in s['queue'] if item['id'] == staged_feature_id
                            )['checkpoint'] == 'review_1_unavailable')
                        unavailable_public = next(item for item in unavailable_state['queue']
                            if item['id'] == staged_feature_id)
                        assert unavailable_public['status'] == 'failed'
                        assert unavailable_public['review_status'] == 'unavailable'
                        assert unavailable_public['auto_repair_lifecycle'] == 'held'
                        assert unavailable_public['last_failure_kind'] == 'operational'
                        with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
                            unavailable_durable = json.loads(database.execute(
                                'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                        unavailable_feature = next(item for item in unavailable_durable['queue_v13']
                            if item['id'] == staged_feature_id)
                        unavailable_proposal = unavailable_feature['escalation_proposal']
                        assert unavailable_proposal['status'] == 'applied'
                        assert unavailable_feature['escalation_pending']
                        assert unavailable_identity == (unavailable_proposal['proposal_id'],
                            unavailable_proposal['apply_request_id'],
                            unavailable_proposal['application_state_sha256'])
                        assert unavailable_feature['review_history'][-1]['outcome'] == 'unavailable'
                        assert unavailable_files == {str(path.relative_to(staged_project)):
                            (hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mtime_ns)
                            for path in staged_project.rglob('*') if path.is_file()}
                        assert validation_cache_snapshot(staged_project) == unavailable_cache
                        assert unavailable_model_counts == (len(calls), len(opencode_calls))
                        review_gate.unlink()
                        if review_gate_started.exists():
                            review_gate_started.unlink()
                        if native_review_started.exists():
                            native_review_started.unlink()
                        review_gate.write_text(
                            'Hold the resumed exact-candidate review for a real Stop')
                        resume(staged_feature_id)
                        provider_unavailable_observed = True
                        continue
                    # The provider has actually begun reviewing all applied
                    # bytes. Stop must hold them; owner adoption may only start
                    # fresh validation/review, never replay model or file work.
                    assert len((root / 'staged-validation-events').read_text().splitlines()) \
                        == unavailable_validation_count + 1
                    assert unavailable_files == {str(path.relative_to(staged_project)):
                        (hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mtime_ns)
                        for path in staged_project.rglob('*') if path.is_file()}
                    assert validation_cache_snapshot(staged_project) == unavailable_cache
                    assert unavailable_model_counts == (len(calls), len(opencode_calls))
                    api('control', {'action': 'stop'})
                    stopped = wait(lambda s: not any(s.get(key) for key in
                        ('running', 'escalation_running', 'tools_running')))
                    held = next(item for item in stopped['queue'] if item['id'] == staged_feature_id)
                    assert held['checkpoint'] == 'auto_repair_effects_quarantined', held
                    assert held['auto_repair_lifecycle'] == 'quarantined'
                    pre_chat_digest = held.get('asset_recovery_sha256')
                    assert pre_chat_digest and len(pre_chat_digest) == 64, held
                    held_terminal = api('repair/escalation?id=' + staged_feature_id)
                    assert held_terminal['status'] == 'interrupted', held_terminal
                    assert held_terminal['files'] == []
                    assert held_terminal['candidate_payload_state'] == 'hash_only'
                    assert len(held_terminal['candidate_entries']) == \
                        restart_capture['candidate_count']
                    assert all('after' not in entry
                        for entry in held_terminal['candidate_entries'])
                    assert 'data_base64' not in json.dumps(held_terminal)
                    with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
                        held_durable = json.loads(database.execute(
                            'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                    held_durable_feature = next(item for item in held_durable['queue_v13']
                        if item['id'] == staged_feature_id)
                    held_proposal = held_durable_feature['escalation_proposal']
                    assert held_proposal['status'] == 'interrupted'
                    assert 'staged_candidate' not in held_proposal
                    assert len(held_proposal['staged_candidate_manifest']) == \
                        restart_capture['candidate_count']
                    held_receipts = [item for item in held_durable_feature['escalation_history']
                        if item['proposal_id'] == held_proposal['proposal_id']]
                    assert [item['outcome'] for item in held_receipts] == [
                        'ready', 'policy_authorized', 'interrupted'], held_receipts
                    assert len({item['candidate_sha256'] for item in held_receipts}) == 1
                    assert held_receipts[0]['candidate_sha256'] == \
                        held_terminal['candidate_sha256']
                    assert held_receipts[0]['candidate_sha256']
                    assert len({item['proposal_sha256'] for item in held_receipts}) == 1
                    assert held_receipts[0]['proposal_sha256']
                    assert held_receipts[0].get('apply_request_id') is None
                    assert held_receipts[1]['apply_request_id'] == \
                        held_proposal['apply_request_id']
                    assert held_receipts[2]['apply_request_id'] == \
                        held_proposal['apply_request_id']
                    assert held_receipts[1]['authorization_revision'] is not None
                    assert held_receipts[0].get('authorization_revision') is None
                    assert held_receipts[2].get('authorization_revision') is None
                    receipt_binding_keys = ('attempt', 'model_target', 'model',
                        'diagnosis_sha256', 'source', 'automatic_epoch', 'policy_revision',
                        'limit_snapshot', 'project_state_sha256')
                    assert all(item[key] == held_receipts[0][key]
                        for item in held_receipts[1:]
                        for key in receipt_binding_keys)
                    held_escalation_history = held_durable_feature['escalation_history']
                    held_escalation_count = held_durable_feature['escalation_count']
                    chat_created = api('chat/conversations', {
                        'id': str(uuid.uuid4()), 'project': 'staged-automatic'})
                    chat_id = chat_created['chat_id']
                    chat_request_id = str(uuid.uuid4())
                    chat_admitted = api('chat', {
                        'project': 'staged-automatic', 'chat_id': chat_id,
                        'message': 'fixture:attributable-chat-mutation',
                        'id': chat_request_id, 'attachments': [],
                        'model_target': 'windows'})
                    assert chat_admitted['running']
                    chat_path = 'chat?' + urllib.parse.urlencode({
                        'project': 'staged-automatic', 'chat_id': chat_id})
                    chat_deadline = time.monotonic() + 90
                    while True:
                        chat_answer = api(chat_path)
                        if not chat_answer['running']:
                            break
                        if time.monotonic() >= chat_deadline:
                            raise AssertionError((chat_answer,
                                (root / 'runner.log').read_text(errors='replace')))
                        time.sleep(.05)
                    assert not chat_answer['error'], chat_answer
                    assert chat_answer['messages'][-1]['request_id'] == chat_request_id
                    assert chat_answer['messages'][-1]['content'] == \
                        'Applied the attributable project-chat mutation.'
                    assert len(attributable_chat_opencode_calls) == 2, \
                        len(attributable_chat_opencode_calls)
                    attributable_path = staged_project / 'chat-attributable.txt'
                    assert attributable_path.read_text() == \
                        'attributable project-chat mutation\n'
                    mutated_state = wait(lambda s: not s['tools_running'] and next(
                        item for item in s['queue'] if item['id'] == staged_feature_id
                        )['checkpoint'] == 'tool_workspace_changed_requires_proposal')
                    held = next(item for item in mutated_state['queue']
                        if item['id'] == staged_feature_id)
                    assert held['auto_repair_lifecycle'] == 'quarantined'
                    digest = held.get('asset_recovery_sha256')
                    assert digest and len(digest) == 64 and digest != pre_chat_digest, held
                    restart_capture['attributable_chat_workspace_revision'] = \
                        held['tool_workspace_revision']
                    restart_capture['opencode_calls_after_attributable_chat'] = len(opencode_calls)
                    assert validation_cache_snapshot(staged_project) == staged_cache_before
                    saved_files = {str(path.relative_to(staged_project)):
                        (hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mtime_ns)
                        for path in staged_project.rglob('*') if path.is_file()}
                    validation_count = len((root / 'staged-validation-events').read_text().splitlines())
                    review_attempts_before_adoption = held['review_attempts']
                    model_counts_before_adoption = (len(calls), len(opencode_calls))
                    runner_pid_before_adoption = process.pid
                    binding = {'action': 'resume', 'expected_feature_id': staged_feature_id,
                        'expected_model_target': held['model_target'], 'expected_status': held['status'],
                        'expected_checkpoint': held['checkpoint']}
                    for bad in (binding,
                            dict(binding, expected_asset_recovery_sha256='0' * 64),
                            dict(binding, expected_asset_recovery_sha256=pre_chat_digest)):
                        try:
                            api('control', bad)
                            raise AssertionError('missing/stale owner adoption was accepted')
                        except urllib.error.HTTPError as error:
                            assert error.code in (400, 409)
                        rejected = api()
                        rejected_feature = next(item for item in rejected['queue']
                            if item['id'] == staged_feature_id)
                        assert not rejected['running']
                        assert rejected_feature['checkpoint'] == held['checkpoint']
                        assert rejected_feature['review_attempts'] == review_attempts_before_adoption
                        assert rejected_feature.get('asset_recovery_sha256') == digest
                        assert process.pid == runner_pid_before_adoption and process.poll() is None
                        assert model_counts_before_adoption == (len(calls), len(opencode_calls))
                        assert validation_count == len((root / 'staged-validation-events').read_text().splitlines())
                        assert saved_files == {str(path.relative_to(staged_project)):
                            (hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mtime_ns)
                            for path in staged_project.rglob('*') if path.is_file()}
                        assert validation_cache_snapshot(staged_project) == staged_cache_before
                    review_gate.unlink()
                    api('control', dict(binding, expected_asset_recovery_sha256=digest))
                    adopted_after_stop = True
                    continue
                if not staged_complete['running']:
                    if observed['status'] == 'succeeded':
                        break
                    raise AssertionError((apply_timeline, observed,
                        (root / 'runner.log').read_text(errors='replace')))
                if time.monotonic() >= deadline:
                    raise AssertionError((apply_timeline,
                        (root / 'runner.log').read_text(errors='replace')))
                time.sleep(.05)
            staged_feature = next(item for item in staged_complete['queue']
                if item['id'] == staged_feature_id)
            assert provider_unavailable_observed
            assert adopted_after_stop
            assert staged_feature['review_attempts'] == review_attempts_before_adoption + 1
            assert len((root / 'staged-validation-events').read_text().splitlines()) == validation_count + 1
            assert saved_files == {str(path.relative_to(staged_project)):
                (hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mtime_ns)
                for path in staged_project.rglob('*') if path.is_file()}
            assert model_counts_before_adoption == (len(calls), len(opencode_calls))
            assert validation_cache_snapshot(staged_project) == staged_cache_before
            assert staged_feature['validation'] == staged_validation
            assert staged_feature['repair_attempts'] == 3
            assert staged_feature['escalation_count'] == 2
            assert staged_feature['review_status'] == 'approved'
            assert (staged_project / 'app.py').read_bytes() == b'VALUE = 1\n'
            assert protected_test.read_bytes() == protected_test_after
            assert (staged_project / 'tests/test_staged_regression.py').is_file()
            assert existing_generated.read_text() == '<main>rebuilt staged output</main>\n'
            assert not obsolete_generated.exists()
            assert (staged_project / 'dist/route.png').read_bytes() == staged_image
            assert (staged_project / 'chat-attributable.txt').read_text() == \
                'attributable project-chat mutation\n'
            assert len(list((staged_project / 'dist').glob('staged-*.html'))) == \
                staged_generated_file_count
            assert all((staged_project / path).stat().st_mtime_ns == modified
                for path, modified in restart_capture['applied_mtimes'].items()), \
                'a durably recorded path was rewritten after restart'

            terminal = api('repair/escalation?id=' + staged_feature_id)
            # The original authorization stays interrupted; the separately
            # adopted exact bytes received a new validation/review decision.
            assert terminal['status'] == 'interrupted', terminal
            assert terminal['source'] == 'automatic_failure'
            assert terminal['files'] == []
            assert terminal['candidate_schema_version'] == 2
            assert terminal['candidate_payload_state'] == 'hash_only'
            assert len(terminal['candidate_sha256']) == 64
            candidate_entries = terminal['candidate_entries']
            assert len(candidate_entries) == staged_candidate_entry_count, \
                len(candidate_entries)
            assert [entry['path'] for entry in candidate_entries] == sorted(
                entry['path'] for entry in candidate_entries)
            text_entry = next(entry for entry in candidate_entries if entry['path'] == 'app.py')
            generated_entry = next(entry for entry in candidate_entries
                if entry['path'] == 'dist/index.html')
            asset_entry = next(entry for entry in candidate_entries
                if entry['path'] == 'dist/route.png')
            deletion_entry = next(entry for entry in candidate_entries
                if entry['path'] == 'dist/obsolete-generated.html')
            assert text_entry['kind'] == 'text' and 'after' not in text_entry
            assert generated_entry['kind'] == 'text'
            assert generated_entry['before_sha256'] == existing_generated_before_sha256
            assert generated_entry['content_sha256'] == hashlib.sha256(
                existing_generated.read_bytes()).hexdigest()
            assert asset_entry['kind'] == 'asset'
            assert asset_entry['media_type'] == 'image/png'
            assert asset_entry['width'] == 128 and asset_entry['height'] == 128
            assert deletion_entry['kind'] == 'delete'
            assert deletion_entry['before_sha256'] == obsolete_generated_before_sha256
            assert 'data_base64' not in json.dumps(terminal)
            assert not list(projects.glob('aw-repair-stage-*'))

            with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
                durable = json.loads(database.execute(
                    'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                durable_staged = next(item for item in durable['queue_v13']
                    if item['id'] == staged_feature_id)
                stage_archive = database.execute(
                    'SELECT scope_id,status,mutation_sha256,mutation_count,text_bytes,asset_bytes,serialized_bytes '
                    'FROM developer_tool_stage WHERE project=?', ('staged-automatic',)).fetchone()
                stage_payload_count = database.execute(
                    'SELECT COUNT(*) FROM developer_tool_stage_mutation WHERE scope_id=?',
                    (stage_archive[0],)).fetchone()[0]
                final_live_workspace_revision = database.execute(
                    'SELECT revision FROM developer_tool_workspace WHERE project=?',
                    ('staged-automatic',)).fetchone()
                attributable_chat_mutation = database.execute(
                    'SELECT revision,request_id,feature_id,evidence '
                    'FROM developer_tool_mutation WHERE project=? AND request_id=?',
                    ('staged-automatic', chat_request_id)).fetchone()
                long_stage_actions = database.execute(
                    'SELECT request_id,tool,summary,status,output '
                    'FROM developer_tool_action WHERE feature_id=? ORDER BY updated_unix,id',
                    (staged_feature_id,)).fetchall()
            assert stage_archive[1] == 'compacted'
            assert len(stage_archive[2]) == 64 and all(value > 0 for value in stage_archive[3:])
            assert stage_payload_count == 0
            assert final_live_workspace_revision == (
                restart_capture['attributable_chat_workspace_revision'],)
            assert attributable_chat_mutation[:3] == (
                restart_capture['attributable_chat_workspace_revision'],
                chat_request_id, None)
            attributable_chat_evidence = json.loads(attributable_chat_mutation[3])
            assert attributable_chat_evidence['feature_id'] is None
            assert attributable_chat_evidence['edits'] == [{
                'path': 'chat-attributable.txt', 'before_sha256': None,
                'after': 'attributable project-chat mutation\n'}]
            assert len(long_stage_opencode_calls) == long_stage_probe_count + 2, \
                len(long_stage_opencode_calls)
            assert len(long_stage_actions) == long_stage_probe_count + 1, long_stage_actions
            assert len({row[0] for row in long_stage_actions}) == 1
            assert all(row[1] == 'bash' and row[3] == 'completed'
                for row in long_stage_actions), long_stage_actions
            probe_actions = [row for row in long_stage_actions
                if 'aw-long-stage-probe-' in row[2] and
                'aw-long-stage-probe-' in (row[4] or '')]
            write_actions = [row for row in long_stage_actions
                if 'aw-long-stage-probe-' not in row[2]]
            assert len(probe_actions) == long_stage_probe_count, probe_actions
            assert len(write_actions) == 1, write_actions
            assert 'maximum steps' not in json.dumps(durable_staged).lower()
            assert 'maximum steps' not in (root / 'runner.log').read_text(
                errors='replace').lower()
            assert durable_staged['escalation_count'] == held_escalation_count
            assert durable_staged['escalation_history'] == held_escalation_history
            staged_outcomes = [item for item in durable_staged['escalation_history']
                if item['proposal_id'] == terminal['proposal_id']]
            assert [item['outcome'] for item in staged_outcomes] == [
                'ready', 'policy_authorized', 'interrupted'], staged_outcomes
            assert all(item['candidate_sha256'] == terminal['candidate_sha256']
                for item in staged_outcomes)
            assert staged_outcomes[1]['authorization_revision'] is not None
            assert staged_outcomes[1]['apply_request_id'] == restart_capture['apply_request_id']
            assert staged_outcomes[2]['apply_request_id'] == restart_capture['apply_request_id']
            assert durable_staged['escalation_proposal']['apply_request_id'] == \
                restart_capture['apply_request_id']
            assert terminal['proposal_id'] == restart_capture['proposal_id']
            assert terminal['candidate_sha256'] == restart_capture['candidate_sha256']
            assert stage_archive[2] == restart_capture['stage_mutation_sha256']
            assert len(opencode_calls) == \
                restart_capture['opencode_calls_after_attributable_chat']
            assert 'staged_candidate' not in durable_staged['escalation_proposal']
            assert len(durable_staged['escalation_proposal']['staged_candidate_manifest']) == \
                staged_candidate_entry_count

            staged_evidence = [json.loads(line) for line in
                (root / 'review-fixture/review-input-evidence.jsonl').read_text().splitlines()]
            staged_batches = [item for item in staged_evidence if item.get('kind') == 'review_batch'
                and any(entry['path'] == 'dist/route.png' for entry in item['entries'])]
            assert staged_batches and all(
                item['batch_count'] == staged_review_batch_count for item in staged_batches), \
                staged_batches
            staged_reviewed = {(entry['path'], entry['content_sha256'])
                for item in staged_evidence if item.get('kind') == 'review_batch'
                for entry in item['entries']}
            for path in ('app.py', 'dist/index.html', 'dist/route.png', 'tests/test_existing.py',
                         'tests/test_staged_regression.py',
                         f'dist/staged-{staged_generated_file_count - 1:03}.html',
                         'chat-attributable.txt'):
                assert (path, hashlib.sha256((staged_project / path).read_bytes()).hexdigest()) \
                    in staged_reviewed, path
            deletion_reviews = [entry for item in staged_evidence
                if item.get('kind') == 'review_batch' for entry in item['entries']
                if entry['path'] == 'dist/obsolete-generated.html']
            assert deletion_reviews and deletion_reviews[0]['delete'] is True
            assert deletion_reviews[0]['before_sha256'] == obsolete_generated_before_sha256
            assert deletion_reviews[0]['content_sha256'] == hashlib.sha256(b'').hexdigest()

            negative_project = negative_projects / 'staged-automatic'
            drift = negative_project / 'unrelated-restart-drift.txt'
            drift.write_text('unrelated bytes must block replay\n')
            negative_cache_before = validation_cache_snapshot(negative_project)
            negative_before = {path.relative_to(negative_project).as_posix():
                (path.read_bytes(), path.stat().st_mtime_ns)
                for path in negative_project.rglob('*') if path.is_file()}
            with socket.socket() as reservation:
                reservation.bind(('127.0.0.1', 0))
                negative_port = reservation.getsockname()[1]
            negative_root = root / 'restart-negative-runner'
            negative_root.mkdir()
            negative_log = (negative_root / 'runner.log').open('wb')
            negative_command = [binary, '--data-dir', str(negative_state),
                '--workspace-root', str(negative_projects), '--bind',
                f'127.0.0.1:{negative_port}', '--model-url',
                f'http://127.0.0.1:{model.server_port}/v1', '--windows-model-url',
                f'http://127.0.0.1:{windows_model.server_port}/v1', '--windows-model',
                'staged-fixture'] + reviewer_arguments(negative_root) + [
                '--opencode-executable', opencode]
            negative_process = subprocess.Popen(negative_command, stdin=subprocess.DEVNULL,
                stdout=negative_log, stderr=negative_log)
            try:
                negative_token_path = negative_state / 'developer-token'
                deadline = time.monotonic() + 30
                while not negative_token_path.exists() and time.monotonic() < deadline:
                    time.sleep(.01)
                negative_token = negative_token_path.read_text().strip()

                def negative_api():
                    request = urllib.request.Request(
                        f'http://127.0.0.1:{negative_port}/status', headers={
                            'Authorization': 'Bearer ' + negative_token})
                    return json.load(urllib.request.urlopen(request, timeout=2))

                negative_feature = None
                while time.monotonic() < deadline:
                    try:
                        state = negative_api()
                        negative_feature = next(item for item in state['queue']
                            if item['id'] == staged_feature_id)
                        if not state['running'] and negative_feature['status'] == 'failed':
                            break
                    except OSError:
                        pass
                    time.sleep(.01)
                assert negative_feature is not None
                assert negative_feature['checkpoint'] == 'escalation_2_apply_interrupted'
                assert negative_feature['auto_repair_lifecycle'] == 'quarantined'
                assert 'restart' in negative_feature['auto_repair_reason'].lower()
                negative_after = {path.relative_to(negative_project).as_posix():
                    (path.read_bytes(), path.stat().st_mtime_ns)
                    for path in negative_project.rglob('*') if path.is_file()}
                assert negative_after == negative_before, \
                    'restart drift rejection performed another live write'
                assert validation_cache_snapshot(negative_project) == negative_cache_before
                assert len(opencode_calls) == \
                    restart_capture['opencode_calls_after_attributable_chat']
                with closing(sqlite3.connect(negative_state / 'developer.sqlite3')) as database:
                    negative_durable = json.loads(database.execute(
                        'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
                    negative_stage = database.execute(
                        'SELECT status,mutation_sha256 FROM developer_tool_stage '
                        'WHERE project=?', ('staged-automatic',)).fetchone()
                negative_durable_feature = next(item for item in negative_durable['queue_v13']
                    if item['id'] == staged_feature_id)
                negative_proposal = negative_durable_feature['escalation_proposal']
                assert negative_proposal['apply_request_id'] == \
                    restart_capture['apply_request_id']
                assert negative_proposal['applied_paths'] == restart_capture['applied_paths']
                assert negative_stage == ('compacted',
                    restart_capture['stage_mutation_sha256'])
            finally:
                if negative_process.poll() is None:
                    negative_process.terminate()
                    negative_process.wait(timeout=15)
                negative_log.close()
            # Exercise the full queue-loop post-apply validation guard. The
            # validation command writes a new live file only after the staged
            # candidate has been applied, so the inner guard records one
            # quarantine. The outer loop must not finalize it a second time.
            drift_project = projects / 'staged-drift'
            drift_project.mkdir()
            (drift_project / 'drift_validation.py').write_text(
                "from pathlib import Path\nimport app\n"
                "if app.VALUE == 1:\n    Path('validation-drift.txt').write_text('outside candidate\\n')\n"
                "assert app.VALUE == 1\n")
            drift_feature_id = str(uuid.uuid4())
            drift_validation = f'"{sys.executable}" drift_validation.py'
            stop()
            opencode_enabled = False
            launch()
            enqueue_with_plan(f'http://127.0.0.1:{port}', token, {
                'id': drift_feature_id, 'project': 'staged-drift',
                'instruction': 'staged-drift: correct VALUE and rebuild the site.',
                'validation': drift_validation, 'model_target': 'windows'})
            resume(drift_feature_id)
            drift_exhausted = wait(lambda s: not s['running'] and next(item for item in s['queue']
                if item['id'] == drift_feature_id)['auto_repair_lifecycle'] == 'held', 120)
            drift_exhausted_feature = next(item for item in drift_exhausted['queue']
                if item['id'] == drift_feature_id)
            assert drift_exhausted_feature['repair_attempts'] == 3
            drift_escalations_before = drift_exhausted_feature['escalation_count']
            stop()
            opencode_enabled = True
            launch()
            assert set_global_permissions('full')['mode'] == 'full'
            resume(drift_feature_id)
            drift_state = wait(lambda s: not s['running'] and next(item for item in s['queue']
                if item['id'] == drift_feature_id)['checkpoint'] ==
                'auto_repair_effects_quarantined', 150)
            drift_public = next(item for item in drift_state['queue']
                if item['id'] == drift_feature_id)
            assert drift_public['status'] == 'failed'
            assert drift_public['auto_repair_lifecycle'] == 'quarantined'
            assert (drift_project / 'validation-drift.txt').is_file()
            with closing(sqlite3.connect(data / 'developer.sqlite3')) as database:
                drift_durable = json.loads(database.execute(
                    'SELECT state FROM developer_state WHERE id=1').fetchone()[0])
            drift_record = next(item for item in drift_durable['queue_v13']
                if item['id'] == drift_feature_id)
            drift_proposal = drift_record['escalation_proposal']
            drift_receipts = [item for item in drift_record['escalation_history']
                if item['proposal_id'] == drift_proposal['proposal_id']]
            assert [item['outcome'] for item in drift_receipts] == [
                'ready', 'policy_authorized', 'interrupted'], drift_receipts
            assert drift_proposal['status'] == 'interrupted'
            assert drift_record['escalation_count'] == drift_escalations_before + 1
            print(json.dumps({'platform': sys.platform, 'large_context_selected': True,
                'cumulative_49_file_review': True, 'actual_image_cli_transport': True,
                'aggregate_review_required': True, 'asset_resume_without_model_replay': True,
                'legacy_quarantine_exact_owner_adoption': True, 'stale_adoption_rejected': True,
                'staged_live_workspace_unchanged_until_authorized': True,
                'staged_typed_text_and_png_candidate': True,
                'staged_typed_generated_text_deletion': True,
                'staged_existing_test_corrected_and_new_regression_added': True,
                'staged_partial_restart_continued_exact_remaining_paths': True,
                'staged_review_stop_requires_exact_owner_adoption': True,
                'staged_owner_adoption_revalidates_without_model_or_write_replay': True,
                'staged_validation_ignores_and_preserves_project_bytecode_cache': True,
                'staged_review_provider_unavailable_resume_revalidated': True,
                'staged_restart_drift_quarantined_without_second_write': True,
                'staged_post_apply_validation_drift_single_quarantine_receipt': True,
                'staged_immutable_validation_and_batched_review': True,
                'staged_mutation_payloads_compacted': True,
                'staged_generated_files': staged_generated_file_count,
                'staged_candidate_entries': staged_candidate_entry_count,
                'staged_recovery_entries': staged_recovery_entry_count,
                'staged_review_batches': staged_review_batch_count,
                'staged_opencode_calls': len(long_stage_opencode_calls),
                'staged_tool_actions': len(long_stage_actions),
                'staged_later_attributable_chat_recovery': True,
                'staged_apply_progress_events': len(apply_timeline),
                'staged_apply_elapsed_seconds': round(time.monotonic() - apply_started, 3)}))
        finally:
            stop()
            log.close()
            model.shutdown()
            windows_model.shutdown()


if __name__ == '__main__':
    main()
