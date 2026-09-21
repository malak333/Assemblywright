#!/usr/bin/env python3
"""Native runner/HTTP proof: selected local project chat is isolated from execution."""
from developer_review_fixture import reviewer_arguments
from developer_planning_fixture import enqueue_with_plan

import argparse
import base64
import binascii
import hashlib
import http.server
import json
from pathlib import Path
import socket
import stat
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


def port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def png(width, height, fill=b'\x7f', compression=6):
    """Construct a fully decodable 8-bit grayscale PNG without third-party modules."""
    row = (fill * ((width + len(fill) - 1) // len(fill)))[:width]
    raw = b''.join(b'\0' + row for _ in range(height))

    def chunk(kind, content):
        return (struct.pack('>I', len(content)) + kind + content
            + struct.pack('>I', binascii.crc32(kind + content) & 0xffffffff))

    return (b'\x89PNG\r\n\x1a\n'
        + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 0, 0, 0, 0))
        + chunk(b'IDAT', zlib.compress(raw, compression)) + chunk(b'IEND', b''))


def attachment(name, media_type, content):
    return {'name': name, 'media_type': media_type,
        'data_base64': base64.b64encode(content).decode()}


def is_reparse_entry(path):
    """True when a directory entry itself is a symlink or Windows reparse point."""
    try:
        info = path.lstat()
    except FileNotFoundError:
        return False
    if sys.platform == 'win32':
        return bool(info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT)
    return stat.S_ISLNK(info.st_mode)


def assert_ordinary_tree(root, stage):
    """Fail loudly unless every entry under `root` is ordinary, never a link."""
    assert root.is_dir() and not is_reparse_entry(root), (stage, str(root))
    for path in sorted(root.rglob('*')):
        assert not is_reparse_entry(path), (stage, 'unexpected reparse entry', str(path))


def cmd_windows_builtin(argv, check=False):
    """Run one cmd.exe builtin with Python-owned quoting, never /c re-parsing.

    The command is built from an argv-like list with subprocess.list2cmdline,
    so every TEMP or project path is quoted exactly once by Python's Windows
    quoting facility instead of ad-hoc string handling of uncontrolled path
    text. The quoted command is passed as one single command string after
    `cmd.exe /d /s /c` with shell=False: /d skips AutoRun, /s strips exactly
    the fixed outer quote pair, and /c preserves the mklink /J and rmdir
    junction semantics, so paths containing spaces stay single Windows
    arguments. check=True fails loudly with the captured stderr.
    """
    assert sys.platform == 'win32', 'cmd.exe builtin requires Windows: ' + repr(argv)
    command = subprocess.list2cmdline([str(part) for part in argv])
    completed = subprocess.run('cmd.exe /d /s /c "' + command + '"',
        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if check:
        assert completed.returncode == 0, (command, completed.returncode,
            completed.stdout.decode(errors='replace'), completed.stderr.decode(errors='replace'))
    return completed


def remove_reparse_entry(path):
    """Exactly remove one symlink or Windows junction itself, never its target."""
    if sys.platform == 'win32':
        assert is_reparse_entry(path), 'expected a Windows reparse point: ' + str(path)
        cleanup = cmd_windows_builtin(['rmdir', path])
        assert cleanup.returncode == 0, ('rmdir', str(path), cleanup.returncode,
            cleanup.stderr.decode(errors='replace'))
    else:
        assert is_reparse_entry(path), 'expected a symlink: ' + str(path)
        path.unlink()
    assert not is_reparse_entry(path) and not path.exists(), \
        'reparse fixture survived removal: ' + str(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    binary = str(Path(parser.parse_args().binary).resolve())
    fixture = {'mode': 'normal', 'calls': [], 'mac_calls': [], 'mac_chat_calls': []}
    entered, release = threading.Event(), threading.Event()

    class Model(http.server.BaseHTTPRequestHandler):
        def reply(self, body, status=200):
            raw = json.dumps(body).encode()
            try:
                self.send_response(status)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)
            except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                pass

        def do_GET(self):
            if self.path == '/props':
                return self.reply({'total_slots': 2 if fixture['mode'] == 'split' else 1,
                    'modalities': {'vision': fixture['mode'] != 'no_vision'},
                    'n_ctx': 262144, 'default_generation_settings': {
                        'n_ctx': 32768 if fixture['mode'] == 'small' else 262144}})
            self.reply({'error': 'unexpected route'}, 404)

        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            fixture['calls'].append((self.path, request))
            if self.path == '/apply-template' and fixture['mode'] == 'oversized':
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Connection', 'close')
                self.end_headers()
                fixture['streamed_bytes'] = 0
                try:
                    self.wfile.write(b'{"prompt":"')
                    for _ in range(512):
                        self.wfile.write(b'x' * 65536)
                        self.wfile.flush()
                        fixture['streamed_bytes'] += 65536
                        time.sleep(.002)
                    self.wfile.write(b'"}')
                except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                    pass
                self.close_connection = True
                return
            if self.path == '/apply-template':
                return self.reply({'prompt': json.dumps(request['messages'])})
            if self.path == '/tokenize':
                return self.reply({'count': 300000 if fixture['mode'] == 'overflow' else
                    max(1, len(request['content']) // 4)})
            if self.path != '/v1/chat/completions':
                return self.reply({'error': 'unexpected route'}, 404)
            mode = fixture['mode']
            is_feature = 'feature-block' in json.dumps(request['messages'])
            if mode == 'block' or is_feature:
                entered.set()
                release.wait(15)
            if is_feature:
                content = json.dumps({'files': [{'path': 'generated.py', 'content': 'VALUE = 1\n'}]})
            else:
                content = 'On Windows, open project alpha and run python temperature_gui.py. ALPHA_ONLY'
            message = {'content': content, 'tool_calls': None}
            if mode == 'tools':
                message['tool_calls'] = [{'id': 'call', 'type': 'function',
                    'function': {'name': 'write_file', 'arguments': '{}'}}]
            self.reply({'choices': [{'message': message,
                'finish_reason': 'length' if mode == 'length' else 'stop'}]})

        def log_message(self, *unused):
            pass

    class Mac(Model):
        def do_GET(self):
            if self.path == '/props':
                return self.reply({'total_slots': 1, 'modalities': {'vision': False},
                    'n_ctx': 262144, 'default_generation_settings': {'n_ctx': 262144}})
            self.reply({'error': 'unexpected route'}, 404)

        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            if self.path in ('/apply-template', '/tokenize'):
                fixture['mac_chat_calls'].append((self.path, request))
                if self.path == '/apply-template':
                    return self.reply({'prompt': json.dumps(request['messages'])})
                return self.reply({'count': max(1, len(request['content']) // 4)})
            if self.path != '/v1/chat/completions':
                return self.reply({'error': 'unexpected route'}, 404)
            if request.get('response_format') is None:
                fixture['mac_chat_calls'].append((self.path, request))
                if fixture['mode'] == 'block':
                    entered.set()
                    release.wait(15)
                return self.reply({'choices': [{'message': {
                    'content': 'Mac AI diagnosis for this project.', 'tool_calls': None},
                    'finish_reason': 'stop'}]})
            fixture['mac_calls'].append(request)
            if 'feature-block' in json.dumps(request.get('messages', [])):
                entered.set()
                release.wait(15)
            self.reply({'choices': [{'message': {'content': json.dumps({'files': [
                {'path': 'generated.py', 'content': 'VALUE = 1\n'}]})}, 'finish_reason': 'stop'}]})

    windows = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    mac = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Mac)
    for server in (windows, mac):
        server.daemon_threads = True
        threading.Thread(target=server.serve_forever, daemon=True).start()

    with tempfile.TemporaryDirectory(prefix='assemblywright-chat-e2e-') as temp:
        root = Path(temp)
        projects = root / 'projects'
        for name in ('alpha', 'beta'):
            (projects / name).mkdir(parents=True)
        (projects / 'alpha' / 'README.md').write_text('ALPHA_ONLY: launch on Windows with python temperature_gui.py')
        (projects / 'alpha' / 'temperature_gui.py').write_text('print("GUI fixture")\n')
        (projects / 'alpha' / 'oversized.txt').write_text('x' * 40000)
        (projects / 'beta' / 'README.md').write_text('BETA_PRIVATE_SENTINEL')
        outside = root / 'outside'
        outside.mkdir()
        (outside / 'secret.txt').write_text('OUTSIDE_PRIVATE_SENTINEL')
        link_created = False
        nested_link_created = False
        reparse_project = reparse_link = None
        try:
            (projects / 'escape').symlink_to(outside, target_is_directory=True)
            link_created = True
        except OSError:
            if sys.platform == 'win32':
                cmd_windows_builtin(['mklink', '/J', projects / 'escape', outside], check=True)
                link_created = True
        try:
            (projects / 'alpha' / 'escape').symlink_to(outside, target_is_directory=True)
            nested_link_created = True
        except OSError:
            if sys.platform == 'win32':
                cmd_windows_builtin(['mklink', '/J', projects / 'alpha' / 'escape', outside],
                    check=True)
                nested_link_created = True
        if sys.platform == 'win32':
            assert link_created and nested_link_created, 'Windows junction coverage requires both escape fixtures'
        state = root / 'state'
        output = (root / 'runner.log').open('wb')
        process, listen, token = None, port(), ''

        def api(path='status', body=None, authenticated=True):
            request = urllib.request.Request(f'http://127.0.0.1:{listen}/{path}',
                data=None if body is None else json.dumps(body).encode(),
                headers={'Content-Type': 'application/json',
                    'Authorization': 'Bearer ' + token if authenticated else 'Bearer invalid'})
            with urllib.request.urlopen(request, timeout=5) as response:
                return json.load(response)

        def rejected(path, body=None, code=409, authenticated=True):
            try:
                api(path, body, authenticated)
            except urllib.error.HTTPError as error:
                assert error.code == code, (path, error.code, error.read())
                return
            raise AssertionError('Unexpected acceptance: ' + path)

        def wait(path, predicate, timeout=15):
            deadline = time.monotonic() + timeout
            last = None
            while time.monotonic() < deadline:
                try:
                    last = api(path)
                    if predicate(last):
                        return last
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(.03)
            raise AssertionError(f'Timed out: {path}: {last}\n' + (root / 'runner.log').read_text(errors='replace'))

        def launch(windows_url=True):
            nonlocal process, listen, token
            listen = port()
            command = [binary, '--data-dir', str(state), '--workspace-root', str(projects),
                '--bind', f'127.0.0.1:{listen}', '--model-url', f'http://127.0.0.1:{mac.server_port}/v1',
                '--model', 'mac-fixture']
            if windows_url:
                command += ['--windows-model-url', f'http://127.0.0.1:{windows.server_port if windows_url is True else windows_url}/v1',
                    '--windows-model', 'windows-fixture']
            command += reviewer_arguments(root)
            process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=output, stderr=output)
            deadline = time.monotonic() + 15
            while not (state / 'developer-token').exists() and time.monotonic() < deadline:
                time.sleep(.03)
            token = (state / 'developer-token').read_text().strip()
            wait('status', lambda value: not value['running'])

        def terminate():
            if process and process.poll() is None:
                process.terminate()
                process.wait(timeout=5)

        def ask(message='How do I open the GUI?', project='alpha', request_id=None,
                attachments=None, model_target='windows'):
            request = {'project': project, 'message': message,
                'id': request_id or str(uuid.uuid4()), 'attachments': attachments or [],
                'model_target': model_target}
            return request, api('chat', request)

        def complete(project='alpha'):
            return wait('chat?project=' + project, lambda value: not value['running'])

        def generation_started(stage, timeout=15):
            """Deterministically wait for the fixture model server to observe the
            expected generation request before serialization probes run.

            The runner admits work synchronously but issues the generation
            request from another thread across a process boundary, so no fixed
            sleep can prove arrival; the fixture event is the real handshake.
            The bound matches the harness's other bounded waits, early-exits if
            the runner process is gone, and on expiry reports thread state,
            captured fixture requests, the runner snapshot, and the runner log.
            """
            deadline = time.monotonic() + timeout
            sampled, status, exit_code = 0., None, None
            while True:
                if entered.wait(.05):
                    return True
                if exit_code is not None or deadline - time.monotonic() <= 0:
                    break
                if time.monotonic() >= sampled:
                    sampled = time.monotonic() + .25
                    exit_code = None if process is None else process.poll()
                    try:
                        status = api()
                    except (OSError, urllib.error.URLError) as error:
                        status = type(error).__name__ + ': ' + str(error)
            windows = [path for path, _ in fixture['calls']]
            mac = [path for path, _ in fixture['mac_chat_calls']]
            raise AssertionError(
                f'No generation request reached the fixture model server within {timeout}s: {stage}; '
                + f'mode={fixture["mode"]!r} release={release.is_set()} runner_exit={exit_code} '
                + f'runner_status={status} windows_requests={len(windows)} windows_tail={windows[-6:]} '
                + f'mac_requests={len(mac)} mac_tail={mac[-6:]} '
                + f'threads={[(thread.name, thread.is_alive()) for thread in threading.enumerate()]}\n'
                + (root / 'runner.log').read_text(errors='replace'))

        def files():
            return {str(path.relative_to(projects)): hashlib.sha256(path.read_bytes()).hexdigest()
                for directory in ('alpha', 'beta') for path in (projects / directory).rglob('*') if path.is_file()}

        def control(action, **values):
            if action == 'enqueue':
                return enqueue_with_plan(f'http://127.0.0.1:{listen}', token, values)
            return api('control', dict(action=action, **values))

        try:
            launch()
            rejected('chat/projects', code=401, authenticated=False)
            rejected('chat?project=alpha', code=401, authenticated=False)
            rejected('chat', {'project': 'alpha', 'message': 'question', 'id': str(uuid.uuid4())}, code=401, authenticated=False)
            assert api('chat/projects')['projects'] == ['alpha', 'beta']
            for invalid in ('../outside', 'missing') + (('escape',) if link_created else ()):
                rejected('chat?project=' + urllib.parse.quote(invalid, safe=''))

            # The Windows reparse-point case stays a real fail-closed product
            # proof, isolated exactly like the planning E2E: it runs in a
            # dedicated throwaway project, must fail before any model call, and
            # its reparse fixture is removed exactly before positive generation.
            if sys.platform == 'win32':
                reparse_id = str(uuid.uuid4())
                reparse_project = projects / 'reparse-negative'
                reparse_project.mkdir()
                reparse_link = reparse_project / 'escape'
                cmd_windows_builtin(['mklink', '/J', reparse_link, outside], check=True)
                assert is_reparse_entry(reparse_link), reparse_link
                prior_auto_run = api()['auto_run']
                control('auto_run', enabled=False)
                control('enqueue', id=reparse_id, project='reparse-negative',
                    instruction='Reject the unsafe project alias', model_target='windows',
                    validation='"' + sys.executable + '" -B -c "raise SystemExit(0)"')
                control('start', expected_feature_id=reparse_id, expected_model_target='windows',
                    expected_status='queued', expected_checkpoint='not_started')
                reparse_failed = wait('status', lambda value: not value['running'] and any(
                    item['id'] == reparse_id and item['status'] == 'failed'
                    for item in value['queue']))
                reparse_state = next(item for item in reparse_failed['queue']
                    if item['id'] == reparse_id)
                assert reparse_state['message'] == \
                    'Automatic repair project context refuses a Windows reparse point', reparse_state
                endpoint_counts = {'windows': len(fixture['calls']),
                    'mac_feature': len(fixture['mac_calls']),
                    'mac_chat': len(fixture['mac_chat_calls'])}
                assert not any(endpoint_counts.values()), \
                    ('Windows reparse project must fail closed before any model call',
                    endpoint_counts)
                control('remove', id=reparse_id)
                wait('status', lambda value: all(
                    item['id'] != reparse_id for item in value['queue']))
                control('auto_run', enabled=prior_auto_run)
                assert api()['auto_run'] == prior_auto_run
                remove_reparse_entry(reparse_link)
                reparse_project.rmdir()
                assert not reparse_project.exists()
                assert (outside / 'secret.txt').read_text() == 'OUTSIDE_PRIVATE_SENTINEL'

            # Every reparse fixture is now gone from positive surfaces: remove
            # the remaining negative links exactly, then prove the shared alpha
            # project and the whole workspace hold only ordinary entries before
            # any later positive chat or feature generation begins.
            if link_created:
                remove_reparse_entry(projects / 'escape')
            if nested_link_created:
                remove_reparse_entry(projects / 'alpha' / 'escape')
            assert_ordinary_tree(projects / 'alpha',
                'positive alpha project before later generation')
            assert_ordinary_tree(projects, 'positive generation workspace isolation')

            before_files, before_queue = files(), api()['queue']
            request, admitted = ask()
            assert admitted['running'] and admitted['request_id'] == request['id']
            answer = complete()
            assert not answer['error'], answer
            assert answer['messages'][-1]['role'] == 'assistant'
            assert 'python temperature_gui.py' in answer['messages'][-1]['content']
            assert answer['messages'][-1]['request_id'] == request['id']
            assert answer['messages'][-1]['model_target'] == 'windows'
            assert answer['messages'][-1]['model'] == 'windows-fixture'
            assert len(answer['messages'][-1]['content_sha256']) == 64
            assert answer['model_target'] == 'windows'
            assert answer['context_limit'] == 262144 and answer['context_tokens'] > 0
            assert 'temperature_gui.py' in answer['context_files']
            assert answer['omitted_files'] >= 1
            generation = [body for path, body in fixture['calls'] if path == '/v1/chat/completions'][-1]
            prompt = json.dumps(generation['messages'])
            assert 'ALPHA_ONLY' in prompt and 'BETA_PRIVATE_SENTINEL' not in prompt and 'OUTSIDE_PRIVATE_SENTINEL' not in prompt
            assert generation['model'] == 'windows-fixture'
            assert generation['chat_template_kwargs']['enable_thinking'] is False
            count = len(fixture['calls'])
            assert api('chat', request)['messages'] == answer['messages']
            assert len(fixture['calls']) == count
            rejected('chat', dict(request, message='changed'))
            assert api('chat?project=beta')['messages'] == []
            assert files() == before_files and api()['queue'] == before_queue and not fixture['mac_calls']

            # Switching local backends preserves the same project conversation and
            # attributes the reply without falling back to the Windows endpoint.
            windows_completion_count = len([1 for path, unused in fixture['calls']
                if path == '/v1/chat/completions'])
            mac_request, admitted = ask('Can the Mac AI verify that?', model_target='mac')
            assert admitted['running'] and admitted['model_target'] == 'mac'
            mac_answer = complete()
            assert not mac_answer['error'], mac_answer
            assert mac_answer['messages'][-1]['request_id'] == mac_request['id']
            assert mac_answer['messages'][-1]['model_target'] == 'mac'
            assert mac_answer['messages'][-1]['model'] == 'mac-fixture'
            assert mac_answer['messages'][-1]['content'] == 'Mac AI diagnosis for this project.'
            mac_generation = [body for path, body in fixture['mac_chat_calls']
                if path == '/v1/chat/completions'][-1]
            mac_prompt = json.dumps(mac_generation['messages'])
            assert 'python temperature_gui.py' in mac_prompt
            assert len([1 for path, unused in fixture['calls']
                if path == '/v1/chat/completions']) == windows_completion_count

            screen = attachment('screen.png', 'image/png', png(2, 2))
            notes = attachment('notes.txt', 'text/plain', b'ATTACHMENT_REFERENCE_ONLY')
            attachment_request, admitted = ask('', attachments=[screen, notes])
            assert admitted['running']
            attached = complete()
            assert not attached['error'], attached
            attached_user = attached['messages'][-2]
            assert attached_user['role'] == 'user' and attached_user['content'] == ''
            assert attached_user['attachments'] == [screen, notes]
            assert attached_user['model_target'] == 'windows'
            assert attached['messages'][-1]['attachments'] == []
            generation = [body for path, body in fixture['calls']
                if path == '/v1/chat/completions'][-1]
            user_parts = next(message['content'] for message in reversed(generation['messages'])
                if isinstance(message.get('content'), list))
            assert user_parts[0]['type'] == 'text'
            assert 'Describe these attachments' in user_parts[0]['text']
            assert 'UNTRUSTED TEXT ATTACHMENT' in user_parts[0]['text']
            assert 'ATTACHMENT_REFERENCE_ONLY' in user_parts[0]['text']
            expected_url = 'data:image/png;base64,' + screen['data_base64']
            assert user_parts[1] == {'type': 'image_url', 'image_url': {'url': expected_url}}
            assert attached['context_tokens'] >= 4096
            call_count = len(fixture['calls'])
            assert api('chat', attachment_request)['messages'] == attached['messages']
            assert len(fixture['calls']) == call_count
            changed_attachment = json.loads(json.dumps(attachment_request))
            changed_attachment['attachments'][0]['name'] = 'changed.png'
            rejected('chat', changed_attachment)

            # A following turn keeps the exact image part in local-model context.
            ask('What did the image show?')
            followed = complete()
            assert not followed['error'], followed
            generation = [body for path, body in fixture['calls']
                if path == '/v1/chat/completions'][-1]
            assert any(part.get('image_url', {}).get('url') == expected_url
                for message in generation['messages'] if isinstance(message.get('content'), list)
                for part in message['content'])

            # Switching to a backend without vision fails explicitly while the
            # saved image remains in context; it cannot silently omit or route it.
            windows_completion_count = len([1 for path, unused in fixture['calls']
                if path == '/v1/chat/completions'])
            mac_completion_count = len([1 for path, unused in fixture['mac_chat_calls']
                if path == '/v1/chat/completions'])
            ask('Can Mac inspect the saved screenshot?', model_target='mac')
            mac_no_vision = complete()
            assert 'vision support' in mac_no_vision['error'], mac_no_vision
            assert len([1 for path, unused in fixture['calls']
                if path == '/v1/chat/completions']) == windows_completion_count
            assert len([1 for path, unused in fixture['mac_chat_calls']
                if path == '/v1/chat/completions']) == mac_completion_count

            terminate(); launch()
            durable = api('chat?project=alpha')
            assert any(message['attachments'] == [screen, notes]
                for message in durable['messages']), durable
            assert api('chat?project=beta')['messages'] == []

            malformed = [
                {'name': 'remote.png', 'media_type': 'image/png',
                    'data_base64': 'https://example.invalid/image.png'},
                attachment('../escape.png', 'image/png', png(1, 1)),
                attachment('mismatch.jpg', 'image/jpeg', png(1, 1)),
                attachment('archive.zip', 'application/zip', b'PK\x03\x04'),
                attachment('nul.txt', 'text/plain', b'a\0b'),
                dict(attachment('unknown.png', 'image/png', png(1, 1)),
                    url='https://example.invalid/image.png'),
            ]
            for item in malformed:
                rejected('chat', {'project': 'alpha', 'message': 'invalid',
                    'id': str(uuid.uuid4()), 'attachments': [item]})
            rejected('chat', {'project': 'alpha', 'message': 'too many',
                'id': str(uuid.uuid4()), 'attachments': [notes] * 5})
            rejected('chat', {'project': 'alpha', 'message': 'too much text',
                'id': str(uuid.uuid4()), 'attachments': [
                    attachment('large.txt', 'text/plain', b'x' * (128 * 1024 + 1))]})
            rejected('chat', {'project': 'alpha', 'message': 'too wide',
                'id': str(uuid.uuid4()), 'attachments': [
                    attachment('wide.png', 'image/png', png(1601, 1))]})
            rejected('chat', {'project': 'alpha', 'message': 'too large',
                'id': str(uuid.uuid4()), 'attachments': [
                    attachment('large.png', 'image/png', png(1600, 1400, compression=0))]})
            total_images = [attachment(f'total-{index}.png', 'image/png',
                png(1600, 1000, bytes([31 + index]), compression=0)) for index in range(4)]
            rejected('chat', {'project': 'alpha', 'message': 'too much total',
                'id': str(uuid.uuid4()), 'attachments': total_images})

            fixture['mode'] = 'no_vision'
            ask('Read this image', attachments=[screen])
            unavailable = complete()
            assert 'vision support' in unavailable['error'], unavailable
            fixture['mode'] = 'normal'
            assert files() == before_files and api()['queue'] == before_queue

            for mode in ('small', 'split', 'overflow', 'tools', 'length', 'oversized'):
                fixture['mode'] = mode
                ask('Test ' + mode)
                failed = complete()
                assert failed['error'], (mode, failed)
                assert failed['messages'][-1]['role'] == 'user', (mode, failed)
                if mode == 'oversized':
                    assert fixture['streamed_bytes'] < 32 * 1024 * 1024
            fixture['mode'] = 'normal'

            feature_id = str(uuid.uuid4())
            control('enqueue', id=feature_id, project='alpha', instruction='feature-block',
                model_target='windows', validation='"' + sys.executable + '" -B -c "raise SystemExit(0)"')
            fixture['mode'] = 'block'
            entered.clear(); release.clear()
            request, _ = ask('Wait for cancellation')
            assert generation_started('chat generation before cancel and start serialization probes')
            assert api()['chat_running']
            rejected('chat/cancel', {'id': str(uuid.uuid4())})
            rejected('control', {'action': 'shutdown'})
            rejected('chat', {'id': str(uuid.uuid4()), 'project': 'beta', 'message': 'second'})
            binding = {'expected_feature_id': feature_id, 'expected_model_target': 'windows',
                'expected_status': 'queued', 'expected_checkpoint': 'not_started'}
            rejected('control', dict(action='start', **binding))
            api('chat/cancel', {'id': request['id']})
            assert complete()['error']
            release.set(); fixture['mode'] = 'normal'
            control('remove', id=feature_id)

            fixture['mode'] = 'block'; entered.clear(); release.clear()
            ask('Emergency cancellation')
            assert generation_started('chat generation before Emergency Pause cancellation')
            control('emergency')
            assert complete()['error']
            assert api()['emergency_paused']
            rejected('chat', {'id': str(uuid.uuid4()), 'project': 'alpha', 'message': 'blocked by emergency'})
            control('clear_emergency'); release.set(); fixture['mode'] = 'normal'

            feature_id = str(uuid.uuid4())
            control('auto_run', enabled=False)
            control('enqueue', id=feature_id, project='alpha', instruction='feature-block',
                model_target='windows', validation='"' + sys.executable + '" -B -c "raise SystemExit(0)"')
            entered.clear(); release.clear()
            control('start', expected_feature_id=feature_id, expected_model_target='windows',
                expected_status='queued', expected_checkpoint='not_started')
            assert generation_started('Windows feature generation before busy-chat rejection')
            rejected('chat', {'id': str(uuid.uuid4()), 'project': 'alpha', 'message': 'busy feature'})
            control('stop'); release.set()
            wait('status', lambda value: not value['running'])
            control('remove', id=feature_id)

            mac_id = str(uuid.uuid4())
            control('auto_run', enabled=False)
            control('enqueue', id=mac_id, project='mac-side', instruction='Mac side feature',
                model_target='mac', validation='"' + sys.executable + '" -B -c "raise SystemExit(0)"')
            fixture['mode'] = 'block'; entered.clear(); release.clear()
            mac_chat_request, _ = ask('Mac chat blocks Mac generation', project='beta',
                model_target='mac')
            assert generation_started('Mac chat generation before start rejection')
            rejected('control', dict(action='start', expected_feature_id=mac_id, expected_model_target='mac',
                expected_status='queued', expected_checkpoint='not_started'))
            api('chat/cancel', {'id': mac_chat_request['id']})
            assert complete('beta')['error']
            release.set(); fixture['mode'] = 'normal'
            control('start', expected_feature_id=mac_id, expected_model_target='mac',
                expected_status='queued', expected_checkpoint='not_started')
            wait('status', lambda value: not value['running'] and any(
                item['id'] == mac_id and item['status'] == 'succeeded' for item in value['queue']))

            # The inverse direction is also serialized: active Mac feature
            # generation prevents admission of a Mac chat reply.
            blocking_mac_id = str(uuid.uuid4())
            control('enqueue', id=blocking_mac_id, project='mac-block', instruction='feature-block',
                model_target='mac', validation='"' + sys.executable + '" -B -c "raise SystemExit(0)"')
            entered.clear(); release.clear()
            control('start', expected_feature_id=blocking_mac_id, expected_model_target='mac',
                expected_status='queued', expected_checkpoint='not_started')
            assert generation_started('Mac feature generation before Mac chat rejection')
            rejected('chat', {'id': str(uuid.uuid4()), 'project': 'beta',
                'message': 'busy Mac feature', 'attachments': [], 'model_target': 'mac'})
            control('stop'); release.set()
            wait('status', lambda value: not value['running'])
            control('remove', id=blocking_mac_id)

            fixture['mode'] = 'block'; entered.clear(); release.clear()
            ask('Restart during answer')
            assert generation_started('chat generation before restart termination')
            terminate(); release.set(); fixture['mode'] = 'normal'
            launch()
            recovered = complete()
            assert recovered['error'] and not recovered['running']
            assert recovered['messages'][0]['content'] == 'How do I open the GUI?'
            beta_history = api('chat?project=beta')['messages']
            assert beta_history and beta_history[-1]['content'] == 'Mac chat blocks Mac generation'
            terminate(); launch(windows_url=False)
            rejected('chat', {'id': str(uuid.uuid4()), 'project': 'alpha', 'message': 'no model'})
            terminate(); launch(windows_url=port())
            prior_mac_calls = len(fixture['mac_calls'])
            ask('offline Windows')
            assert complete()['error'] and len(fixture['mac_calls']) == prior_mac_calls
            assert files() == before_files
            print('PASS: authenticated project chat, explicit Mac/Windows selection, grounding, read-only files/queue, context admission, history, serialization, cancellation, emergency, restart, and isolated reparse-point fail-closed cleanup')
        finally:
            release.set()
            terminate()
            output.close()
            for server in (windows, mac):
                server.shutdown(); server.server_close()
            for created, path in ((link_created, projects / 'escape'),
                    (nested_link_created, projects / 'alpha' / 'escape')):
                if created and is_reparse_entry(path):
                    remove_reparse_entry(path)
            if reparse_link is not None and is_reparse_entry(reparse_link):
                remove_reparse_entry(reparse_link)
            if reparse_project is not None and reparse_project.is_dir():
                if not any(reparse_project.iterdir()):
                    reparse_project.rmdir()
                    assert not reparse_project.exists()


if __name__ == '__main__':
    main()
