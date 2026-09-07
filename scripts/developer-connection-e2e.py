#!/usr/bin/env python3
"""Native process/socket recovery proof for the developer connection supervisor."""

import fcntl
import json
import os
from pathlib import Path
import signal
import stat
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
HELPER = ROOT / "scripts/developer-connection.py"
TOKEN = "00000000-0000-4000-8000-000000000000" * 2


def wait_until(predicate, message, timeout=12):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            value = predicate()
            if value:
                return value
        except (OSError, ValueError, json.JSONDecodeError):
            pass
        time.sleep(.05)
    raise AssertionError(message)


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def write_fixture(root):
    root = Path(root)
    state = root / "state"
    state.mkdir(mode=0o700)
    socket_path = root / "ssh" / "assemblywright-developer.sock"
    socket_path.parent.mkdir(mode=0o700)
    controller = root / "local-ai"
    controller.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    controller.chmod(0o700)
    identity = state / "connection_ed25519"
    identity.write_text("fixture-key", encoding="utf-8")
    identity.chmod(0o600)
    known = state / "known_hosts"
    known.write_text("host.test ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFixtureFixtureFixture12345678\n", encoding="utf-8")
    known.chmod(0o600)
    config = {
        "schema_version": 1,
        "host": "fixture@host.test",
        "identity_file": str(identity),
        "known_hosts_file": str(known),
        "remote_root": "C:/a/aw-developer",
        "model_controller": str(controller),
    }
    config_path = state / "connection.json"
    config_path.write_text(json.dumps(config), encoding="utf-8")
    config_path.chmod(0o600)
    runtime = state / "runtime.json"
    runtime.write_text(json.dumps({
        "endpoint": "http://127.0.0.1:17796",
        "token": TOKEN,
        "review_codex_executable": "C:/tools/codex.exe",
        "review_codex_home": "C:/Users/mike/.codex",
    }), encoding="utf-8")
    runtime.chmod(0o600)

    fake = root / "fake-ssh.py"
    fake.write_text(r'''#!/usr/bin/env python3
import json, os, pathlib, signal, socket, sys, time
args = sys.argv[1:]
root = pathlib.Path(os.environ["AW_FAKE_ROOT"])
with (root / "ssh-arguments.jsonl").open("a", encoding="utf-8") as output:
    output.write(json.dumps(args) + "\n")
def value_after(flag):
    return args[args.index(flag) + 1]
if "-O" in args:
    operation = value_after("-O")
    socket_path = pathlib.Path(value_after("-S"))
    pid_path = root / "master.pid"
    if operation == "check":
        try:
            pid = int(pid_path.read_text())
            os.kill(pid, 0)
            raise SystemExit(0 if socket_path.exists() else 1)
        except (OSError, ValueError):
            raise SystemExit(1)
    if operation == "exit":
        try: os.kill(int(pid_path.read_text()), signal.SIGTERM)
        except (OSError, ValueError): pass
        raise SystemExit(0)
    raise SystemExit(2)
if "-M" in args:
    socket_path = pathlib.Path(value_after("-S"))
    socket_path.unlink(missing_ok=True)
    server = socket.socket(socket.AF_UNIX)
    server.bind(str(socket_path))
    server.listen(1)
    (root / "master.pid").write_text(str(os.getpid()))
    def stop(*unused):
        server.close()
        socket_path.unlink(missing_ok=True)
        raise SystemExit(0)
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    while True: time.sleep(.1)
remote = args[-1] if args else ""
if "Get-CimInstance Win32_Process" in remote:
    if (root / "remote-runner-present").exists():
        print("PRESENT")
    else:
        print("ABSENT")
    raise SystemExit(0)
if "developer-token" in remote:
    print("00000000-0000-4000-8000-000000000000" * 2)
    raise SystemExit(0)
if "assemblywright-developer.exe" in remote:
    count_path = root / "runner-count"
    try: count = int(count_path.read_text()) + 1
    except (OSError, ValueError): count = 1
    count_path.write_text(str(count))
    (root / "runner.pid").write_text(str(os.getpid()))
    (root / "remote-runner-present").touch()
    if (root / "crash-next-runner").exists():
        (root / "crash-next-runner").unlink()
        raise SystemExit(7)
    def stop(*unused):
        (root / "remote-runner-present").unlink(missing_ok=True)
        raise SystemExit(0)
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    while True: time.sleep(.1)
raise SystemExit(2)
''', encoding="utf-8")
    fake.chmod(0o700)
    return state, socket_path, fake


def child_command(state, socket_path, fake, root):
    code = r'''
import importlib.util, json, pathlib, signal, sys
spec=importlib.util.spec_from_file_location("connection", sys.argv[1])
module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
state=pathlib.Path(sys.argv[2]); socket_path=pathlib.Path(sys.argv[3]); root=pathlib.Path(sys.argv[4])
module.SSH=sys.argv[5]
module.BACKOFF=(.1,.1,.1,.1,.1,.1)
module.RUNNER_RETRY_SECONDS=.2
module.HEALTH_INTERVAL_SECONDS=.1
module.control_socket=lambda: socket_path
def status(runtime, config):
    with (root / "http-evidence.jsonl").open("a", encoding="utf-8") as output:
        output.write(json.dumps({"method":"GET","path":"/status"}) + "\n")
    if not (root / "ready").exists(): return None
    return {"mode":"supervised_developer","host":"fixture","workspace_root":"C:\\a\\aw-developer\\projects",
      "running":False,"chat_running":False,"planning_running":False,
      "review_required":True,"review_provider":"openai.codex","review_model":"gpt-5.6-sol",
      "planning_required":True,"planning_provider":"openai.codex","planning_model":"gpt-5.6-sol"}
module.authenticated_status=status
signal.signal(signal.SIGTERM, module.stop_signal)
signal.signal(signal.SIGINT, module.stop_signal)
raise SystemExit(module.supervise(state))
'''
    return [sys.executable, "-c", code, str(HELPER), str(state), str(socket_path),
            str(root), str(fake)]


def stop_process(process):
    if process.poll() is None:
        process.send_signal(signal.SIGTERM)
        try:
            process.wait(timeout=8)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def main():
    with tempfile.TemporaryDirectory(dir=Path.home(),
                                     prefix="assemblywright-developer-connection-") as temp:
        root = Path(temp)
        state, socket_path, fake = write_fixture(root)
        env = {**os.environ, "AW_FAKE_ROOT": str(root)}
        command = child_command(state, socket_path, fake, root)

        # Initial HTTP uncertainty with positive process presence must not launch
        # a duplicate runner before authentication has ever succeeded.
        (root / "remote-runner-present").touch()
        initial_uncertain = subprocess.Popen(
            command, env=env, stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            wait_until(lambda: read_json(state / "connection-status.json")["phase"]
                       in {"reconnecting", "needs_attention"},
                       "Initial HTTP uncertainty was not reported")
            time.sleep(.8)
            assert not (root / "runner-count").exists(), \
                "Initial HTTP uncertainty duplicated a positively live runner"
        finally:
            stop_process(initial_uncertain)
        (root / "remote-runner-present").unlink()

        supervisor = subprocess.Popen(command, env=env, stdin=subprocess.DEVNULL,
                                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            wait_until(lambda: (root / "runner-count").exists(),
                       "Supervisor did not attach its runner channel")
            (root / "ready").touch()
            connected = wait_until(
                lambda: (read_json(state / "connection-status.json")
                         if read_json(state / "connection-status.json")["phase"] == "connected"
                         else None),
                "Supervisor did not publish authenticated connected status")
            first_update = connected["updated_at"]
            wait_until(lambda: read_json(state / "connection-status.json")["updated_at"] > first_update,
                       "Connected status did not receive a freshness heartbeat", timeout=5)

            loser = subprocess.run(command, env=env, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                   timeout=5)
            assert loser.returncode == 2, "Second supervisor did not lose the singleton lock"
            assert read_json(state / "connection-status.json")["phase"] == "connected"

            # An actual owned runner-channel exit is retried without an HTTP control replay.
            (root / "ready").unlink()
            old_count = int((root / "runner-count").read_text())
            os.kill(int((root / "runner.pid").read_text()), signal.SIGTERM)
            wait_until(lambda: int((root / "runner-count").read_text()) > old_count,
                       "Exited owned runner channel was not recreated")
            (root / "ready").touch()
            wait_until(lambda: read_json(state / "connection-status.json")["phase"] == "connected",
                       "Runner-channel recovery did not authenticate")

            # A launcher maintenance lease prevents reconnect/spawn until it is released.
            lock = (state / "connection-maintenance.lock").open("a+b")
            fcntl.flock(lock, fcntl.LOCK_EX)
            (root / "ready").unlink()
            prior_master = int((root / "master.pid").read_text())
            os.kill(prior_master, signal.SIGTERM)
            wait_until(lambda: read_json(state / "connection-status.json")["phase"] == "reconnecting",
                       "Connection loss was not reported")
            time.sleep(.5)
            assert int((root / "master.pid").read_text()) == prior_master, \
                "Supervisor reconnected during maintenance"
            fcntl.flock(lock, fcntl.LOCK_UN)
            lock.close()
            wait_until(lambda: int((root / "master.pid").read_text()) != prior_master,
                       "Supervisor did not reconnect after maintenance")
            wait_until(lambda: int((root / "runner-count").read_text()) > old_count + 1,
                       "Runner was not reattached after connection loss")
            (root / "ready").touch()
            wait_until(lambda: read_json(state / "connection-status.json")["phase"] == "connected",
                       "Connection-loss recovery did not authenticate")
        finally:
            stop_process(supervisor)

        final_status = read_json(state / "connection-status.json")
        assert final_status["phase"] == "stopped", (final_status, supervisor.returncode)

        # A supervisor may inherit a healthy runner without owning its original
        # channel. If that runner later disappears, bounded status failures attach
        # a replacement while the Windows exclusive-instance guard arbitrates it.
        inherited_count = int((root / "runner-count").read_text())
        (root / "remote-runner-present").touch()
        inherited = subprocess.Popen(command, env=env, stdin=subprocess.DEVNULL,
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            wait_until(lambda: read_json(state / "connection-status.json")["phase"] == "connected",
                       "Inherited runner did not authenticate")
            assert int((root / "runner-count").read_text()) == inherited_count
            (root / "ready").unlink()
            time.sleep(.8)
            assert int((root / "runner-count").read_text()) == inherited_count, \
                "HTTP uncertainty duplicated a positively live inherited runner"
            (root / "remote-runner-present").unlink()
            wait_until(lambda: int((root / "runner-count").read_text()) > inherited_count,
                       "Missing inherited runner was not recreated")
            (root / "ready").touch()
            wait_until(lambda: read_json(state / "connection-status.json")["phase"] == "connected",
                       "Inherited-runner recovery did not authenticate")
        finally:
            stop_process(inherited)

        evidence = [json.loads(line) for line in
                    (root / "http-evidence.jsonl").read_text(encoding="utf-8").splitlines()]
        assert evidence and all(row == {"method": "GET", "path": "/status"} for row in evidence), \
            "Supervisor emitted a mutating HTTP request"
        arguments = [json.loads(line) for line in
                     (root / "ssh-arguments.jsonl").read_text(encoding="utf-8").splitlines()]
        runner_arguments = [row for row in arguments
                            if row and "assemblywright-developer.exe" in row[-1]]
        assert runner_arguments and all("ProxyCommand=/usr/bin/false" in row
                                        for row in runner_arguments), \
            "Runner channels could fall back to unmanaged TCP"
        serialized = json.dumps(arguments)
        assert all(action not in serialized for action in
                   ["--action", "clear_emergency", "approve_and_enqueue",
                    "/planning", "/review", "/control"]), \
            "Supervisor replayed an owner action"

        # A non-socket collision is never removed or replaced during reconnect attempts.
        socket_path.write_text("owner-file", encoding="utf-8")
        collision = subprocess.Popen(command, env=env, stdin=subprocess.DEVNULL,
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            wait_until(lambda: read_json(state / "connection-status.json")["attempt"] >= 2,
                       "Socket collision did not enter bounded reconnect")
            assert socket_path.read_text(encoding="utf-8") == "owner-file"
            assert not stat.S_ISSOCK(socket_path.lstat().st_mode)
        finally:
            stop_process(collision)

    print("developer connection native process/socket E2E passed")


if __name__ == "__main__":
    main()
