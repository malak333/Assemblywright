#!/usr/bin/env python3
"""Opt-in check of the installed Codex tool catalog using only a loopback provider.

No account configuration, project data, real inference, or model output is used.
Run again after upgrading the configured Codex executable. This verifies that
runtime's request catalog; it is not an OS sandbox or network-containment test.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import threading


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--codex-executable', required=True)
    args = parser.parse_args()
    executable = Path(args.codex_executable).resolve(strict=True)
    source = (Path(__file__).resolve().parents[1] /
              'crates/assemblywright-master/src/developer_review.rs').read_text()
    model = re.search(r'MODEL_ID: &str = ("[^"]+")', source).group(1)

    def argument_profile(name):
        match = re.search(
            rf'const {name}: &\[&str\] = &\[(.*?)\];', source, re.DOTALL)
        assert match, f'{name} is not a fixed Rust string array'
        arguments = []
        for line in match.group(1).splitlines():
            line = line.strip().removesuffix(',')
            if line:
                arguments.append(json.loads(model if line == 'MODEL_ID' else line))
        return arguments

    # Match the production platform selection without dynamically probing or
    # accepting unknown configuration keys.
    arguments = argument_profile('CODEX_ARGUMENTS')
    non_windows = argument_profile('NON_WINDOWS_CODEX_ARGUMENTS')
    if os.name != 'nt':
        arguments.extend(non_windows)
    arguments.append('--output-schema')
    assert arguments[0] == 'exec' and arguments[-1] == '--output-schema'
    assert '--ignore-user-config' in arguments and '--strict-config' in arguments
    assert all((feature in arguments) == (os.name != 'nt') for feature in (
        'features.sleep_tool=false',
        'features.in_app_chat=false',
        'features.in_app_dictation=false',
        'features.in_app_local_automation=false',
    ))
    captured = []
    received = threading.Event()

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            length = int(self.headers.get('Content-Length', '0'))
            if not 0 < length <= 2 * 1024 * 1024:
                self.send_error(413)
                return
            packet = json.loads(self.rfile.read(length))
            captured.append(packet.get('tools', []))
            received.set()
            # Never supply model output, so no tool can be requested/executed.
            self.send_response(400)
            self.send_header('Content-Length', '0')
            self.end_headers()

        def log_message(self, *unused):
            pass

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix='aw-review-catalog-') as temporary:
            root = Path(temporary)
            schema = root / 'output.json'
            schema.write_text(json.dumps({'type': 'object', 'properties': {},
                                          'additionalProperties': False}))
            command = [str(executable), *arguments, str(schema), '--cd', str(root),
                       '-c', 'model_provider="catalog_fixture"',
                       '-c', 'model_providers.catalog_fixture.name="Catalog fixture"',
                       '-c', f'model_providers.catalog_fixture.base_url="http://127.0.0.1:{server.server_port}/v1"',
                       '-c', 'model_providers.catalog_fixture.wire_api="responses"',
                       '-c', 'model_providers.catalog_fixture.requires_openai_auth=false',
                       '-c', 'model_providers.catalog_fixture.request_max_retries=0', '-']
            environment = {'CODEX_HOME': str(root)}
            if os.name == 'nt':
                environment['SystemRoot'] = os.environ['SystemRoot']
                environment['PATH'] = str(Path(os.environ['SystemRoot']) / 'System32')
            result = subprocess.run(command, input='Reply with an empty JSON object.', text=True,
                           capture_output=True, env=environment, cwd=root, timeout=45)
            assert received.is_set(), (
                f'CLI did not reach the loopback provider; catalog unverified '
                f'(exit {result.returncode}): {result.stderr[-4000:]}')
            assert captured and all(tools == [] for tools in captured), 'Runtime exposed tools'
            print(json.dumps({'codex_sha256': hashlib.sha256(executable.read_bytes()).hexdigest(),
                              'requests_observed': len(captured), 'tool_count': 0,
                              'real_model_called': False}))
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


if __name__ == '__main__':
    main()
