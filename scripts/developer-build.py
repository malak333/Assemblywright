#!/usr/bin/env python3
"""Build and open the supervised developer app with its persistent connection."""

import argparse
import base64
from contextlib import contextmanager
import fcntl
import importlib.util
import ipaddress
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import socket
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
STATE = Path.home() / "Library/Application Support/Assemblywright/Developer"
APP = ROOT / "target/developer/Assemblywright Developer.app"
CONNECTION_SOURCE = Path(__file__).with_name("developer-connection.py")
MIGRATION_FILE = "connection-migration.json"


def _connection_module():
    spec = importlib.util.spec_from_file_location("assemblywright_developer_connection",
                                                  CONNECTION_SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def windows_model_settings(args, saved):
    if getattr(args, "windows_model_start_script", None) is not None:
        raise ValueError("Windows model startup is managed independently. Start its service on Windows, then set --windows-model-url and --windows-model.")
    names = ["windows_model_url", "windows_model"]
    values = {name: getattr(args, name) if getattr(args, name) is not None else saved.get(name)
              for name in names}
    url, model = values["windows_model_url"], values["windows_model"]
    if bool(url) != bool(model):
        raise ValueError("Set both --windows-model-url and --windows-model.")
    if url:
        parsed = urllib.parse.urlsplit(url)
        try:
            local = ipaddress.ip_address(parsed.hostname or "").is_loopback
            port = parsed.port
        except ValueError:
            local, port = False, None
        if (parsed.scheme != "http" or not local or not port or parsed.username is not None
                or parsed.password is not None or parsed.query or parsed.fragment
                or not re.fullmatch(r"http://[0-9a-fA-F:.\[\]]+/[A-Za-z0-9/_-]*", url)):
            raise ValueError("Windows model URL must be credential-free loopback HTTP, such as http://127.0.0.1:18081/v1.")
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}", model):
            raise ValueError("Windows model name contains unsupported characters.")
        values["windows_model_url"] = url.rstrip("/")
    return {key: value for key, value in values.items() if value}


def review_settings(args, saved):
    names = ["review_codex_executable", "review_codex_home"]
    values = {name: getattr(args, name, None) if getattr(args, name, None) is not None
              else saved.get(name) for name in names}
    if bool(values["review_codex_executable"]) != bool(values["review_codex_home"]):
        raise ValueError("Set both --review-codex-executable and --review-codex-home.")
    for value in values.values():
        if value and not re.fullmatch(r"[A-Za-z]:[/\\][A-Za-z0-9_/@.\\-]+", value):
            raise ValueError("Reviewer paths must be simple absolute Windows paths without spaces or shell characters.")
    executable = values["review_codex_executable"]
    if executable and executable.replace("\\", "/").rsplit("/", 1)[-1].lower() != "codex.exe":
        raise ValueError("Reviewer executable must be the native codex.exe binary.")
    return {key: value for key, value in values.items() if value}


def load_saved_runtime(connection):
    try:
        return connection.safe_runtime_settings(STATE)
    except ValueError as error:
        raise SystemExit(str(error)) from error


def authenticated_status(connection, runtime, config):
    return connection.authenticated_status(runtime, config) if config is not None else None


def request_shutdown(connection, runtime, config):
    status = authenticated_status(connection, runtime, config)
    if status is None:
        raise SystemExit("The running connection state is unavailable. No process was stopped; reconnect before rebuilding or stopping.")
    if status.get("running") or status.get("chat_running") or status.get("planning_running"):
        raise SystemExit("Use Stop, Stop reply, or Cancel brainstorming in the app and wait for work to finish before changing settings, rebuilding, or closing the runner.")
    request = urllib.request.Request(
        runtime["endpoint"] + "/control", method="POST",
        data=json.dumps({"action": "shutdown"}).encode(),
        headers={"Authorization": "Bearer " + runtime["token"],
                 "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=5) as response:
            json.load(response)
    except (OSError, ValueError, urllib.error.HTTPError) as error:
        raise SystemExit("The idle runner did not accept shutdown. No connection process was stopped.") from error
    return status


def stop_supervisor(connection):
    if not connection.service_is_loaded():
        return
    result = subprocess.run(["/bin/launchctl", "bootout",
                             f"{connection.launchd_domain()}/{connection.LABEL}"],
                            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL, timeout=10)
    if result.returncode:
        raise SystemExit("The idle connection supervisor could not be stopped safely.")
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if (not connection.service_is_loaded()
                and not os.path.lexists(connection.control_socket())):
            return
        time.sleep(.1)
    raise SystemExit("The connection supervisor is still unloading; no replacement was started.")


@contextmanager
def maintenance_lock():
    path = STATE / "connection-maintenance.lock"
    descriptor = path.open("a+b")
    os.chmod(path, 0o600)
    deadline = time.monotonic() + 20
    while True:
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
            break
        except BlockingIOError:
            if time.monotonic() >= deadline:
                descriptor.close()
                raise SystemExit("The connection is still changing state. No maintenance action was started.")
            time.sleep(.1)
    try:
        yield
    finally:
        fcntl.flock(descriptor, fcntl.LOCK_UN)
        descriptor.close()


def old_ssh_base(args):
    if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,63}@[A-Za-z0-9][A-Za-z0-9.-]{0,252}",
                        args.host):
        raise SystemExit("Host must be a bounded user@host value.")
    socket_path = Path(args.socket)
    if not socket_path.is_absolute() or any(character in str(socket_path)
            for character in ('"', "\r", "\n")):
        raise SystemExit("Bootstrap SSH socket must be an absolute path without control characters.")
    return ["/usr/bin/ssh", "-F", "/dev/null", "-S", str(socket_path),
            "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", args.host]


def old_socket_live(args):
    try:
        return subprocess.run(old_ssh_base(args)[:-1] + ["-O", "check", args.host],
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, timeout=5).returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        return False


def migration_template(args, revision):
    return {
        "schema_version": 1,
        "host": args.host,
        "socket": str(Path(args.socket)),
        "local_forward": "17796:127.0.0.1:7796",
        "remote_forward": "18080:127.0.0.1:8080",
        "observed_revision": int(revision),
        "shutdown_confirmed": False,
        "local_detached": False,
        "remote_detached": False,
    }


def load_migration(connection, args):
    path = STATE / MIGRATION_FILE
    if not path.exists():
        return None
    metadata = path.lstat()
    if (path.is_symlink() or not path.is_file() or metadata.st_nlink != 1
            or metadata.st_mode & 0o077 or metadata.st_size > 4096):
        raise ValueError("Connection migration journal is not a direct bounded 0600 file")
    value = json.loads(path.read_text(encoding="utf-8"))
    expected = migration_template(args, value.get("observed_revision", -1)
                                  if isinstance(value, dict) else -1)
    if not isinstance(value, dict) or set(value) != set(expected):
        raise ValueError("Connection migration journal is malformed")
    for key in ("host", "socket", "local_forward", "remote_forward"):
        if value[key] != expected[key]:
            raise ValueError("Connection migration journal is bound to another SSH session")
    if (not isinstance(value["observed_revision"], int)
            or not all(isinstance(value[key], bool) for key in
                       ("shutdown_confirmed", "local_detached", "remote_detached"))):
        raise ValueError("Connection migration journal has invalid state")
    return value


def save_migration(connection, value):
    connection.atomic_json(STATE / MIGRATION_FILE, value)


def old_runner_absent(args):
    command = old_ssh_base(args)[:-1] + ["-o", "ProxyCommand=/usr/bin/false",
        args.host, 'tasklist.exe /FI "IMAGENAME eq assemblywright-developer.exe" /NH']
    try:
        result = subprocess.run(command, stdin=subprocess.DEVNULL,
                                capture_output=True, text=True, timeout=8)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if result.returncode:
        return None
    return "assemblywright-developer.exe" not in result.stdout.lower()


def local_forward_absent(port=17796):
    probe = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        # SSH requests a listener. SO_REUSEADDR permits a new listener after an
        # authenticated app connection closes and leaves only TIME_WAIT state.
        probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        probe.bind(("127.0.0.1", port))
        probe.listen(1)
        return True
    except OSError:
        return False
    finally:
        probe.close()


def remote_forward_absent(args):
    remote = ('powershell.exe -NoProfile -NonInteractive -Command '
              '"if (Test-NetConnection 127.0.0.1 -Port 18080 '
              '-InformationLevel Quiet) { exit 1 } else { exit 0 }"')
    command = old_ssh_base(args)[:-1] + ["-o", "ProxyCommand=/usr/bin/false",
                                         args.host, remote]
    try:
        return subprocess.run(command, stdin=subprocess.DEVNULL,
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                              timeout=10).returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        return False


def cancel_known_old_forwards(connection, args, migration):
    checks = [
        ("local_detached", "-L", migration["local_forward"], local_forward_absent),
        ("remote_detached", "-R", migration["remote_forward"],
         lambda: remote_forward_absent(args)),
    ]
    for field, direction, specification, absent in checks:
        if migration[field]:
            continue
        try:
            result = subprocess.run(old_ssh_base(args)[:-1] +
                ["-O", "cancel", direction, specification, args.host],
                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL, timeout=5)
        except (OSError, subprocess.TimeoutExpired):
            result = None
        if (result is None or result.returncode != 0) and not absent():
            save_migration(connection, migration)
            raise SystemExit("A known legacy forward could not be released; migration remains resumable and no ownership completion was recorded.")
        migration[field] = True
        save_migration(connection, migration)
    if not local_forward_absent() or not remote_forward_absent(args):
        raise SystemExit("Legacy forward release could not be verified; migration remains resumable.")
    return migration


def confirm_migration_shutdown(connection, args, migration, saved, config):
    if migration["shutdown_confirmed"]:
        return migration
    current = authenticated_status(connection, saved, config)
    if current is not None:
        request_shutdown(connection, saved, config)
    elif old_runner_absent(args) is not True:
        raise SystemExit("Legacy shutdown is unconfirmed; migration remains resumable and no forward was changed.")
    migration["shutdown_confirmed"] = True
    save_migration(connection, migration)
    return migration


def legacy_migration_candidate(connection, loaded, args):
    if loaded or connection.has_connection_ownership(STATE):
        return False
    return old_socket_live(args)


def connection_payload(args):
    identity = STATE / "connection_ed25519"
    known_hosts = STATE / "known_hosts"
    return {
        "schema_version": 1,
        "host": args.host,
        "identity_file": str(identity),
        "known_hosts_file": str(known_hosts),
        "remote_root": args.remote_root,
        "model_controller": str(Path(args.model_controller)),
    }


def write_connection_config(connection, args):
    payload = connection_payload(args)
    connection.atomic_json(STATE / "connection.json", payload)
    return connection.validate_config(STATE)


def write_runtime(connection, saved, windows_settings, reviewer_settings):
    retained = {key: saved[key] for key in ("endpoint", "token") if key in saved}
    connection.atomic_json(STATE / "runtime.json",
                           {**retained, **windows_settings, **reviewer_settings})
    connection.safe_runtime_settings(STATE)


def build_products(connection, config, args):
    remote = args.remote_root.replace("/", "\\").rstrip("\\")
    ssh = connection.ssh_base(config)
    print("Building the Mac app and Windows runner…", flush=True)
    subprocess.run(["swift", "build", "--disable-sandbox", "--package-path",
                    str(ROOT / "apps/mac"), "--product", "AssemblywrightMacApp"], check=True)
    with tempfile.TemporaryDirectory(prefix="assemblywright-developer-build-") as temp:
        archive = Path(temp) / "source.tar.gz"
        with tarfile.open(archive, "w:gz") as output:
            for name in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "crates"]:
                output.add(ROOT / name, arcname=name)
        subprocess.run(ssh + [f"if not exist {remote} mkdir {remote}"], check=True)
        scp = ["/usr/bin/scp", *ssh[1:-1], str(archive),
               f"{config['host']}:{args.remote_root}/source.tar.gz"]
        subprocess.run(scp, check=True)
    subprocess.run(ssh + [f"cd /d {remote} && tar -xf source.tar.gz"], check=True)
    touch_sources = "import os,pathlib; [os.utime(p,None) for p in pathlib.Path('crates').rglob('*.rs')]"
    encoded_touch = base64.b64encode(touch_sources.encode()).decode()
    subprocess.run(ssh + [f"cd /d {remote} && python -c \"import base64;exec(base64.b64decode('{encoded_touch}'))\""], check=True)
    subprocess.run(ssh + [f"cd /d {remote} && cargo build -p assemblywright-master --bin assemblywright-developer && if not exist bin mkdir bin"], check=True)
    subprocess.run(ssh + [f"copy /Y {remote}\\target\\debug\\assemblywright-developer.exe {remote}\\bin\\assemblywright-developer.exe >NUL"], check=True)
    (APP / "Contents/MacOS").mkdir(parents=True, exist_ok=True)
    replacement = APP / "Contents/MacOS/.AssemblywrightMacApp.new"
    shutil.copy2(ROOT / "apps/mac/.build/debug/AssemblywrightMacApp", replacement)
    replacement.replace(APP / "Contents/MacOS/AssemblywrightMacApp")
    (APP / "Contents/Info.plist").write_bytes(plistlib.dumps({
        "CFBundleIdentifier": "com.nobiletechnology.assemblywright.developer",
        "CFBundleExecutable": "AssemblywrightMacApp",
        "CFBundleName": "Assemblywright Developer",
        "CFBundleDisplayName": "Assemblywright Developer",
        "CFBundleVersion": "1",
        "CFBundleShortVersionString": "0.1.4",
        "CFBundlePackageType": "APPL",
        "LSMinimumSystemVersion": "14.0",
        "AssemblywrightDeveloperBuild": True,
    }))
    subprocess.run(["codesign", "--force", "--sign", "-", str(APP)], check=True)


def preflight_connection(connection, config):
    if config is None:
        raise SystemExit("Provision connection.json and the app-owned SSH identity before connecting.")
    try:
        probe = subprocess.run(connection.ssh_base(config) + ["cmd.exe /d /c exit 0"],
                               stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                               stderr=subprocess.DEVNULL, timeout=15)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise SystemExit("The app-owned SSH identity could not reach Windows. No runner was stopped.") from error
    if probe.returncode:
        raise SystemExit("The app-owned SSH identity could not reach Windows. No runner was stopped.")


def start_local_model(args):
    try:
        urllib.request.urlopen("http://127.0.0.1:8080/health", timeout=3).close()
        return
    except OSError:
        pass
    print("Starting the configured local model…", flush=True)
    try:
        result = subprocess.run([args.model_controller, "start"], timeout=60)
        started = result.returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        started = False
    if not started:
        print("Mac model did not start. Its features will report a model error; no other model is substituted.")


def wait_for_connected(connection, config):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        runtime = load_saved_runtime(connection)
        status = authenticated_status(connection, runtime, config)
        if status is not None:
            return status
        time.sleep(.25)
    raise SystemExit("The persistent connection did not reach authenticated ready status. Check connection-status.json.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="mike@100.64.23.14")
    parser.add_argument("--socket", default=str(Path.home() / ".ssh/assemblywright-codex-windows.sock"),
                        help="Existing bootstrap/build SSH control socket; never used by the runtime")
    parser.add_argument("--remote-root", default="C:/a/aw-developer-20260905")
    parser.add_argument("--model-controller", default=str(
        Path.home() / "Antigravity/local-ai-mac/scripts/local-ai"))
    parser.add_argument("--windows-model-url")
    parser.add_argument("--windows-model")
    parser.add_argument("--windows-model-start-script", help=argparse.SUPPRESS)
    parser.add_argument("--review-codex-executable")
    parser.add_argument("--review-codex-home")
    parser.add_argument("--build", action="store_true")
    parser.add_argument("--no-open", action="store_true")
    parser.add_argument("--stop", action="store_true")
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z]:[/\\][A-Za-z0-9_/\\-]+", args.remote_root):
        raise SystemExit("Remote root must be a simple absolute Windows path without spaces.")

    connection = _connection_module()
    connection.direct_private_directory(STATE, "developer state directory", create=True)
    with maintenance_lock():
        saved = load_saved_runtime(connection)
        try:
            windows_settings = windows_model_settings(args, saved)
            reviewer_settings = review_settings(args, saved)
        except ValueError as error:
            raise SystemExit(str(error)) from error
        if not reviewer_settings and not args.stop:
            raise SystemExit("Configure the required Codex reviewer before launching or rebuilding. The current runner was left unchanged.")
        setting_keys = ["windows_model_url", "windows_model", "windows_model_start_script",
                        "review_codex_executable", "review_codex_home"]
        desired = {**windows_settings, **reviewer_settings}
        settings_changed = any(saved.get(key) != desired.get(key) for key in setting_keys)
        existing_connection = None
        connection_path = STATE / "connection.json"
        if connection_path.exists():
            try:
                existing_connection = connection.validate_config(STATE)
            except ValueError as error:
                raise SystemExit(str(error)) from error
        try:
            validated_desired = connection.validate_config_payload(
                STATE, connection_payload(args))
        except ValueError as error:
            raise SystemExit(str(error)) from error
        comparable_existing = ({key: existing_connection[key] for key in validated_desired}
                               if existing_connection else None)
        connection_changed = comparable_existing != validated_desired
        loaded = connection.service_is_loaded()
        observed = authenticated_status(connection, saved, existing_connection)
        try:
            journal = load_migration(connection, args)
            legacy_socket = journal is not None or legacy_migration_candidate(
                connection, loaded, args)
        except ValueError as error:
            raise SystemExit(str(error)) from error
        if journal is None and legacy_socket and saved.get("token") and observed is None:
            raise SystemExit("The legacy connection is present but its authenticated runner state is unavailable. No forward or process was changed.")
        migration = journal is not None or (legacy_socket and observed is not None)
        disruptive = args.stop or args.build or settings_changed or connection_changed or migration
        if args.build or connection_changed:
            preflight_connection(connection, validated_desired)

        if migration:
            if journal is None:
                if any(observed.get(key) for key in
                       ("running", "chat_running", "planning_running")):
                    raise SystemExit("Stop active work before migrating the legacy connection.")
                revision = observed.get("revision")
                if not isinstance(revision, int) or isinstance(revision, bool):
                    raise SystemExit("Legacy status lacks a valid revision; no migration was started.")
                journal = migration_template(args, revision)
                save_migration(connection, journal)
            journal = confirm_migration_shutdown(
                connection, args, journal, saved, existing_connection)
            journal = cancel_known_old_forwards(connection, args, journal)
            connection.record_connection_ownership(STATE)
            (STATE / MIGRATION_FILE).unlink()
        elif disruptive:
            if loaded or observed is not None:
                request_shutdown(connection, saved, existing_connection)
                if loaded:
                    stop_supervisor(connection)
            elif args.stop:
                print("Developer connection is already stopped. Projects and queue are retained.")
                return

        if args.stop:
            print("Developer runner and connection stopped. Projects and queue are retained.")
            return
        config = write_connection_config(connection, args)
        write_runtime(connection, saved, windows_settings, reviewer_settings)
        if args.build:
            build_products(connection, config, args)
        start_local_model(args)
        connection.install(STATE, CONNECTION_SOURCE)
    status = wait_for_connected(connection, config)
    print("Connected to " + status["host"] + ". Projects: " + status["workspace_root"])
    if not args.no_open:
        if not APP.exists():
            raise SystemExit("Run this command again with --build to create the Mac app.")
        subprocess.run(["open", str(APP)], check=True)
    print("Developer app: " + str(APP))


if __name__ == "__main__":
    main()
