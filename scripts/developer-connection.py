#!/usr/bin/env python3
"""Persistent, fail-closed SSH connection for Assemblywright Developer Mode."""

import argparse
import fcntl
import ipaddress
import json
import os
from pathlib import Path
import plistlib
import re
import signal
import socket
import stat
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

LABEL = "com.nobiletechnology.assemblywright.developer-connection"
SSH = "/usr/bin/ssh"
LOCAL_FORWARD = "127.0.0.1:17796:127.0.0.1:7796"
REMOTE_FORWARD = "127.0.0.1:18080:127.0.0.1:8080"
MAX_LOG_BYTES = 512 * 1024
BACKOFF = (1, 2, 4, 8, 16, 30)
RUNNER_RETRY_SECONDS = 5
HEALTH_INTERVAL_SECONDS = 2
STOP = False


def default_state():
    return Path.home() / "Library/Application Support/Assemblywright/Developer"


def control_socket():
    return Path.home() / ".ssh/assemblywright-developer.sock"


def atomic_write(path, data, mode=0o600):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    temporary = path.with_name(f".{path.name}.{uuid.uuid4().hex}.tmp")
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    descriptor = os.open(temporary, flags, mode)
    try:
        with os.fdopen(descriptor, "wb", closefd=False) as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
    finally:
        os.close(descriptor)
    os.chmod(temporary, mode)
    os.replace(temporary, path)
    directory = os.open(path.parent, os.O_RDONLY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def atomic_json(path, value):
    atomic_write(path, (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode())


def append_log(state, message):
    path = Path(state) / "connection.log"
    if path.exists() or path.is_symlink():
        metadata = path.lstat()
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
            raise ValueError("Connection log must be a direct single-link regular file")
    if path.exists() and path.stat().st_size > MAX_LOG_BYTES:
        replacement = path.with_suffix(".log.previous")
        replacement.unlink(missing_ok=True)
        path.replace(replacement)
    flags = os.O_WRONLY | os.O_CREAT | os.O_APPEND
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    descriptor = os.open(path, flags, 0o600)
    with os.fdopen(descriptor, "a", encoding="utf-8") as output:
        output.write(f"{int(time.time())} {message[:500]}\n")
    os.chmod(path, 0o600)


def write_status(state, phase, message, attempt):
    if phase not in {"connecting", "connected", "reconnecting", "needs_attention", "stopped"}:
        raise ValueError("Invalid connection phase")
    atomic_json(Path(state) / "connection-status.json", {
        "phase": phase,
        "message": message[:500],
        "updated_at": int(time.time()),
        "attempt": int(attempt),
    })


def direct_private_file(path, state, label):
    path, state = Path(path), Path(state)
    if not path.is_absolute():
        raise ValueError(f"{label} must be absolute")
    if path.parent != state:
        raise ValueError(f"{label} must be a direct child of the developer state directory")
    current = Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        if stat.S_ISLNK(current.lstat().st_mode):
            raise ValueError(f"{label} cannot use symlinks")
    state = state.resolve(strict=True)
    resolved = path.resolve(strict=True)
    if not resolved.is_relative_to(state):
        raise ValueError(f"{label} must be inside the developer state directory")
    metadata = resolved.stat()
    if (not stat.S_ISREG(metadata.st_mode) or stat.S_IMODE(metadata.st_mode) != 0o600
            or metadata.st_nlink != 1):
        raise ValueError(f"{label} must be a direct single-link 0600 regular file")
    return resolved


def direct_private_directory(path, label, create=False, require_private=True):
    path = Path(path)
    if not path.is_absolute():
        raise ValueError(f"{label} must be absolute")
    if create:
        path.mkdir(parents=True, exist_ok=True, mode=0o700)
    current = Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        metadata = current.lstat()
        if stat.S_ISLNK(metadata.st_mode):
            raise ValueError(f"{label} cannot use symlinks")
    metadata = path.stat()
    if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.getuid():
        raise ValueError(f"{label} must be an owner-controlled directory")
    if require_private and stat.S_IMODE(metadata.st_mode) & 0o077:
        raise ValueError(f"{label} must be an owner-only directory")
    return path.resolve(strict=True)


def direct_executable_file(path, label):
    path = Path(path)
    if not path.is_absolute():
        raise ValueError(f"{label} must be absolute")
    current = Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        metadata = current.lstat()
        if stat.S_ISLNK(metadata.st_mode):
            raise ValueError(f"{label} cannot use symlinks")
    metadata = path.stat()
    if (not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1
            or not os.access(path, os.X_OK)):
        raise ValueError(f"{label} must be a direct executable regular file")
    return path.resolve(strict=True)


def validate_config(state):
    state = direct_private_directory(state, "developer state directory")
    path = state / "connection.json"
    if path.is_symlink() or not path.is_file() or stat.S_IMODE(path.stat().st_mode) != 0o600:
        raise ValueError("connection.json must be a direct 0600 file")
    if path.stat().st_nlink != 1 or path.stat().st_size > 16 * 1024:
        raise ValueError("connection.json must be a bounded single-link file")
    value = json.loads(path.read_text(encoding="utf-8"))
    return validate_config_payload(state, value)


def validate_config_payload(state, value):
    state = direct_private_directory(state, "developer state directory")
    if not isinstance(value, dict) or set(value) != {"schema_version", "host", "identity_file", "known_hosts_file",
                         "remote_root", "model_controller"} or value["schema_version"] != 1:
        raise ValueError("connection.json has an unsupported schema")
    if not isinstance(value["host"], str) or not re.fullmatch(
            r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,63}@[A-Za-z0-9][A-Za-z0-9.-]{0,252}",
            value["host"]):
        raise ValueError("Connection host must be a bounded user@host value")
    if not isinstance(value["remote_root"], str) or not re.fullmatch(
            r"[A-Za-z]:[/\\][A-Za-z0-9_/\\-]+", value["remote_root"]):
        raise ValueError("Remote root must be a simple absolute Windows path without spaces")
    if not all(isinstance(value[field], str) for field in
               ("identity_file", "known_hosts_file", "model_controller")):
        raise ValueError("Connection paths must be strings")
    controller = direct_executable_file(value["model_controller"], "model controller")
    value = dict(value)
    value["model_controller"] = str(controller)
    value["identity_file"] = str(direct_private_file(value["identity_file"], state, "identity file"))
    value["known_hosts_file"] = str(direct_private_file(value["known_hosts_file"], state, "known-hosts file"))
    if Path(value["identity_file"]).name != "connection_ed25519":
        raise ValueError("Identity file must use the fixed app-owned filename")
    if Path(value["known_hosts_file"]).name != "known_hosts":
        raise ValueError("Known-hosts file must use the fixed app-owned filename")
    try:
        known_lines = Path(value["known_hosts_file"]).read_text(encoding="utf-8").splitlines()
    except UnicodeError as error:
        raise ValueError("Known-hosts file must be UTF-8") from error
    hostname = value["host"].rsplit("@", 1)[1]
    if (len(known_lines) != 1 or not re.fullmatch(
            re.escape(hostname) + r" ssh-ed25519 [A-Za-z0-9+/=]{32,512}", known_lines[0])):
        raise ValueError("Known-hosts file must contain one exact Ed25519 record for the configured host")
    return value


def ssh_option_path(name, path):
    value = str(path)
    if any(character in value for character in ('"', "\r", "\n")):
        raise ValueError(f"{name} path contains unsupported characters")
    return f'{name}="{value}"'


def ownership_record():
    return {
        "schema_version": 1,
        "label": LABEL,
        "control_socket": str(control_socket()),
        "local_forward": LOCAL_FORWARD,
        "remote_forward": REMOTE_FORWARD,
    }


def has_connection_ownership(state):
    path = Path(state) / "connection-ownership.json"
    if not path.exists():
        return False
    if (path.is_symlink() or not path.is_file()
            or stat.S_IMODE(path.stat().st_mode) != 0o600 or path.stat().st_nlink != 1
            or path.stat().st_size > 4096):
        raise ValueError("Connection ownership record is not a direct bounded 0600 file")
    if json.loads(path.read_text(encoding="utf-8")) != ownership_record():
        raise ValueError("Connection ownership record is malformed")
    return True


def record_connection_ownership(state):
    direct_private_directory(state, "developer state directory")
    atomic_json(Path(state) / "connection-ownership.json", ownership_record())


def ssh_base(config):
    return [SSH, "-F", "/dev/null", "-o", "BatchMode=yes", "-o", "IdentitiesOnly=yes",
            "-o", "IdentityAgent=none", "-o", "KbdInteractiveAuthentication=no",
            "-o", "PasswordAuthentication=no", "-o", "StrictHostKeyChecking=yes",
            "-o", ssh_option_path("UserKnownHostsFile", config["known_hosts_file"]),
            "-o", "GlobalKnownHostsFile=/dev/null", "-i", config["identity_file"],
            "-o", "ConnectTimeout=10", config["host"]]


def master_command(config, socket_path):
    return ssh_base(config)[:-1] + ["-M", "-S", str(socket_path), "-N",
        "-o", "ControlMaster=yes", "-o", "ControlPersist=no",
        "-o", "ServerAliveInterval=10", "-o", "ServerAliveCountMax=3",
        "-o", "ExitOnForwardFailure=yes", "-L", LOCAL_FORWARD, "-R", REMOTE_FORWARD,
        config["host"]]


def channel_command(config, socket_path, remote_command=None):
    # If the control socket disappears, /usr/bin/false prevents ssh from silently
    # opening a fresh unmanaged TCP connection for a runner or token command.
    command = ssh_base(config)[:-1] + ["-S", str(socket_path),
        "-o", "ProxyCommand=/usr/bin/false", config["host"]]
    if remote_command is not None:
        command.append(remote_command)
    return command


def control_command(config, socket_path, operation, *arguments):
    return ssh_base(config)[:-1] + ["-S", str(socket_path), "-O", operation,
        *arguments, config["host"]]


def safe_runtime_settings(state):
    path = Path(state) / "runtime.json"
    if not path.exists():
        return {}
    if (path.is_symlink() or not path.is_file() or stat.S_IMODE(path.stat().st_mode) != 0o600
            or path.stat().st_nlink != 1 or path.stat().st_size > 16 * 1024):
        raise ValueError("runtime.json must be a direct 0600 file")
    value = json.loads(path.read_text(encoding="utf-8"))
    allowed = {"endpoint", "token", "windows_model_url", "windows_model",
               "windows_model_start_script", "review_codex_executable", "review_codex_home",
               "opencode_executable"}
    if not isinstance(value, dict) or not set(value).issubset(allowed):
        raise ValueError("runtime.json contains unknown settings")
    if value.get("endpoint") not in (None, "http://127.0.0.1:17796"):
        raise ValueError("runtime.json endpoint is not the fixed loopback endpoint")
    if "token" in value and not valid_token(value["token"]):
        raise ValueError("runtime.json token is malformed")
    url, model = value.get("windows_model_url"), value.get("windows_model")
    if bool(url) != bool(model):
        raise ValueError("runtime.json contains incomplete Windows model settings")
    if url:
        if not isinstance(url, str) or not isinstance(model, str):
            raise ValueError("runtime.json contains unsafe Windows model settings")
        parsed = urllib.parse.urlsplit(url)
        try:
            local = ipaddress.ip_address(parsed.hostname or "").is_loopback
            port = parsed.port
        except ValueError:
            local, port = False, None
        if (parsed.scheme != "http" or not local or not port or parsed.username is not None
                or parsed.password is not None or parsed.query or parsed.fragment
                or not re.fullmatch(r"http://[0-9a-fA-F:.\[\]]+/[A-Za-z0-9/_-]*", url)
                or not isinstance(model, str)
                or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}", model)):
            raise ValueError("runtime.json contains unsafe Windows model settings")
    script = value.get("windows_model_start_script")
    if script and (not url or not isinstance(script, str)
            or not re.fullmatch(r"[A-Za-z]:[/\\][A-Za-z0-9_/\\.-]+\.ps1", script)):
        raise ValueError("runtime.json contains an unsafe Windows model start script")
    executable, home = value.get("review_codex_executable"), value.get("review_codex_home")
    if bool(executable) != bool(home):
        raise ValueError("runtime.json contains incomplete reviewer settings")
    for reviewer_path in (executable, home):
        if reviewer_path and (not isinstance(reviewer_path, str)
                or not re.fullmatch(r"[A-Za-z]:[/\\][A-Za-z0-9_/@.\\-]+", reviewer_path)):
            raise ValueError("runtime.json contains an unsafe reviewer path")
    if executable and executable.replace("\\", "/").rsplit("/", 1)[-1].lower() != "codex.exe":
        raise ValueError("runtime.json reviewer executable must be codex.exe")
    tool_executable = value.get("opencode_executable")
    if "opencode_executable" in value and (
            not isinstance(tool_executable, str)
            or not re.fullmatch(r"[A-Za-z]:[/\\][A-Za-z0-9_/\\.-]+", tool_executable)
            or tool_executable.replace("\\", "/").rsplit("/", 1)[-1].lower() != "opencode.exe"
            or any(part in (".", "..") for part in tool_executable.replace("\\", "/").split("/"))):
        raise ValueError("runtime.json contains an unsafe OpenCode executable path")
    return value


def runner_command(config, runtime):
    remote = config["remote_root"].replace("/", "\\").rstrip("\\")
    command = (f"{remote}\\bin\\assemblywright-developer.exe --data-dir {remote}\\state "
               f"--workspace-root {remote}\\projects --model-url http://127.0.0.1:18080/v1")
    if runtime.get("windows_model_url"):
        command += f" --windows-model-url {runtime['windows_model_url']} --windows-model {runtime['windows_model']}"
    if runtime.get("review_codex_executable"):
        command += (f" --review-codex-executable {runtime['review_codex_executable']}"
                    f" --review-codex-home {runtime['review_codex_home']}")
    if runtime.get("opencode_executable"):
        command += f" --opencode-executable {runtime['opencode_executable']}"
    if not re.fullmatch(r"[A-Za-z0-9_@.:/\\\[\] -]+", command):
        raise ValueError("Runner command contains unsafe characters")
    return command


def expected_workspace_root(config):
    remote = config["remote_root"].replace("/", "\\").rstrip("\\")
    return normalize_windows_drive_path(remote + "\\projects")


def normalize_windows_drive_path(value):
    if not isinstance(value, str):
        return None
    candidate = value.replace("/", "\\").rstrip("\\")
    if candidate.startswith("\\\\?\\"):
        candidate = candidate[4:]
    if candidate.startswith("\\\\") or not re.fullmatch(
            r"[A-Za-z]:\\[A-Za-z0-9_@.-]+(?:\\[A-Za-z0-9_@.-]+)*", candidate):
        return None
    if any(segment in {".", ".."} for segment in candidate[3:].split("\\")):
        return None
    return candidate.lower()


def valid_token(value):
    uuid_pattern = r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"
    return isinstance(value, str) and re.fullmatch(uuid_pattern + uuid_pattern, value) is not None


def valid_status_model_bindings(value):
    """Observe Windows-owned selections; never choose or change a model here."""
    settings = value.get("ai_settings")
    if settings is None:
        return (value.get("review_model") == "gpt-5.6-sol"
                and value.get("planning_model") == "gpt-5.6-sol")
    if not isinstance(settings, dict):
        return False
    for role, prefix in (("reviewer", "review"), ("orchestrator", "planning")):
        selection = settings.get(role)
        if not isinstance(selection, dict):
            return False
        model, effort = selection.get("model"), selection.get("reasoning_effort")
        if (not isinstance(model, str) or not re.fullmatch(r"gpt-[a-z0-9.-]{1,124}", model)
                or effort not in ("none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra")
                or value.get(prefix + "_model") != model
                or value.get(prefix + "_reasoning_effort") != effort):
            return False
    return True


def authenticated_status(runtime, config):
    endpoint, token = runtime.get("endpoint"), runtime.get("token")
    if endpoint != "http://127.0.0.1:17796" or not valid_token(token):
        return None
    request = urllib.request.Request(endpoint + "/status", method="GET",
                                     headers={"Authorization": "Bearer " + token})
    try:
        with urllib.request.urlopen(request, timeout=3) as response:
            value = json.load(response)
    except (OSError, ValueError, urllib.error.HTTPError):
        return None
    if (value.get("mode") != "supervised_developer" or value.get("review_required") is not True
            or value.get("review_provider") != "openai.codex"
            or value.get("planning_required") is not True or value.get("planning_provider") != "openai.codex"
            or not valid_status_model_bindings(value)
            or normalize_windows_drive_path(value.get("workspace_root"))
               != expected_workspace_root(config)):
        return None
    return value


def read_remote_token(config, socket_path):
    remote = config["remote_root"].replace("/", "\\").rstrip("\\")
    result = subprocess.run(channel_command(config, socket_path,
        f"cmd.exe /d /c type {remote}\\state\\developer-token"),
        stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=10)
    token = result.stdout.strip()
    if result.returncode or not valid_token(token):
        return None
    return token


def refresh_runtime(state, config, socket_path):
    token = read_remote_token(config, socket_path)
    if token is None:
        return None
    runtime = safe_runtime_settings(state)
    runtime.update(endpoint="http://127.0.0.1:17796", token=token)
    atomic_json(Path(state) / "runtime.json", runtime)
    return runtime


def runner_process_absent(config, socket_path):
    remote = config["remote_root"].replace("/", "\\").rstrip("\\")
    expected = remote + "\\bin\\assemblywright-developer.exe"
    script = (
        "$ErrorActionPreference='Stop';"
        "function Normalize-DrivePath([string]$path){"
        "if([string]::IsNullOrEmpty($path)){return $null};"
        "if($path.StartsWith('\\\\?\\')){$path=$path.Substring(4)};"
        "if($path.StartsWith('\\\\') -or $path -notmatch '^[A-Za-z]:\\\\'){return $null};"
        "$segments=$path.Substring(3).Split('\\');"
        "if($segments.Count -eq 0 -or $segments.Where({$_ -eq '.' -or $_ -eq '..'}).Count -gt 0){return $null};"
        "return $path.TrimEnd('\\').ToLowerInvariant()};"
        f"$expected='{expected}';"
        "$normalizedExpected=Normalize-DrivePath $expected;"
        "if($null -eq $normalizedExpected){Write-Output 'UNKNOWN';exit 3};"
        "$items=@(Get-CimInstance Win32_Process | Where-Object {"
        "$_.Name -eq 'assemblywright-developer.exe'});"
        "if($items.Count -eq 0){Write-Output 'ABSENT';exit 0};"
        "foreach($item in $items){"
        "$observed=Normalize-DrivePath $item.ExecutablePath;"
        "if($null -eq $observed){Write-Output 'UNKNOWN';exit 3};"
        "if($observed -eq $normalizedExpected){Write-Output 'PRESENT';exit 0}};"
        "Write-Output 'ABSENT';exit 0")
    command = f'powershell.exe -NoProfile -NonInteractive -Command "{script}"'
    try:
        result = subprocess.run(channel_command(config, socket_path, command),
            stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=8)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if result.returncode or len(result.stdout) > 128:
        return None
    outcome = result.stdout.strip()
    if outcome == "ABSENT":
        return True
    if outcome == "PRESENT":
        return False
    return None


def socket_is_live(config, socket_path):
    if not socket_path.exists() or not stat.S_ISSOCK(socket_path.lstat().st_mode):
        return False
    try:
        return subprocess.run(control_command(config, socket_path, "check"),
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, timeout=5).returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        return False


def clear_stale_socket(config, socket_path):
    if not os.path.lexists(socket_path):
        return
    if socket_is_live(config, socket_path):
        return
    mode = socket_path.lstat().st_mode
    if not stat.S_ISSOCK(mode):
        raise ValueError("Dedicated SSH control path is occupied by a non-socket")
    socket_path.unlink()


def terminate(process):
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def acquire_connection_window(state):
    path = Path(state) / "connection-maintenance.lock"
    descriptor = path.open("a+b")
    os.chmod(path, 0o600)
    try:
        fcntl.flock(descriptor, fcntl.LOCK_SH | fcntl.LOCK_NB)
    except BlockingIOError:
        descriptor.close()
        return None
    return descriptor


def release_connection_window(descriptor):
    if descriptor is not None:
        fcntl.flock(descriptor, fcntl.LOCK_UN)
        descriptor.close()


def respawn_runner_if_allowed(state, config, socket_path, runtime):
    window = acquire_connection_window(state)
    if window is None:
        return None
    try:
        return subprocess.Popen(
            channel_command(config, socket_path, runner_command(config, runtime)),
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL)
    finally:
        release_connection_window(window)


def supervise(state):
    global STOP
    STOP = False
    state = direct_private_directory(state, "developer state directory", create=True)
    lock_path = state / "connection.lock"
    lock = lock_path.open("a+b")
    os.chmod(lock_path, 0o600)
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        # Only the lock owner publishes shared connection status.
        return 2
    socket_path = control_socket()
    attempt = 0
    config = None
    master = runner = None
    reused_master = False
    while not STOP:
        connection_window = acquire_connection_window(state)
        if connection_window is None:
            write_status(state, "reconnecting",
                         "Connection maintenance is in progress.", attempt)
            time.sleep(.2)
            continue
        write_status(state, "connecting" if attempt == 0 else "reconnecting",
                     "Connecting to the Windows developer host.", attempt)
        try:
            config = validate_config(state)
            direct_private_directory(socket_path.parent,
                                     "dedicated SSH socket directory", create=True)
            clear_stale_socket(config, socket_path)
            if socket_is_live(config, socket_path):
                reused_master = True
            else:
                reused_master = False
                master = subprocess.Popen(master_command(config, socket_path),
                    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                deadline = time.monotonic() + 12
                while time.monotonic() < deadline and master.poll() is None and not STOP:
                    if socket_is_live(config, socket_path):
                        break
                    time.sleep(.1)
                if STOP or not socket_is_live(config, socket_path):
                    raise RuntimeError("SSH connection or required forwards failed")
            runtime = safe_runtime_settings(state)
            runtime = refresh_runtime(state, config, socket_path) or runtime
            status = authenticated_status(runtime, config)
            if status is None and runner_process_absent(config, socket_path) is True:
                runner = subprocess.Popen(channel_command(config, socket_path, runner_command(config, runtime)),
                    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline and not STOP:
                    runtime = refresh_runtime(state, config, socket_path) or runtime
                    status = authenticated_status(runtime, config)
                    if status is not None:
                        break
                    if runner.poll() is not None and runner.returncode not in (0, None):
                        time.sleep(.2)
                    time.sleep(.2)
            release_connection_window(connection_window)
            connection_window = None
            status_failures = 0
            last_runner_attempt = time.monotonic()
            while status is None and not STOP:
                if not socket_is_live(config, socket_path):
                    raise RuntimeError("SSH connection was lost before runner authentication")
                status_failures += 1
                phase = "needs_attention" if status_failures >= 8 else "reconnecting"
                write_status(state, phase,
                    "Authenticated runner status needs attention."
                    if phase == "needs_attention"
                    else "SSH is connected; waiting for authenticated runner status.",
                    status_failures)
                time.sleep(HEALTH_INTERVAL_SECONDS)
                runtime = refresh_runtime(state, config, socket_path) or runtime
                status = authenticated_status(runtime, config)
                if (status is None and (runner is None or runner.poll() is not None)
                        and runner_process_absent(config, socket_path) is True
                        and time.monotonic() - last_runner_attempt >= RUNNER_RETRY_SECONDS):
                    replacement = respawn_runner_if_allowed(
                        state, config, socket_path, runtime)
                    if replacement is not None:
                        runner = replacement
                        last_runner_attempt = time.monotonic()
            if STOP:
                break
            runner_owned = runner is not None and runner.poll() is None
            attempt = 0
            write_status(state, "connected", "Connected to the authenticated Windows developer runner.", attempt)
            status_failures = 0
            last_runner_attempt = time.monotonic()
            while not STOP:
                time.sleep(HEALTH_INTERVAL_SECONDS)
                if not socket_is_live(config, socket_path):
                    raise RuntimeError("SSH connection was lost")
                runtime = safe_runtime_settings(state)
                if authenticated_status(runtime, config) is not None:
                    status_failures = 0
                    write_status(state, "connected",
                                 "Connected to the authenticated Windows developer runner.", 0)
                    continue
                if (runner_owned and runner.poll() is not None
                        and time.monotonic() - last_runner_attempt >= RUNNER_RETRY_SECONDS):
                    replacement = respawn_runner_if_allowed(
                        state, config, socket_path, runtime)
                    if replacement is None:
                        write_status(state, "reconnecting",
                                     "Connection maintenance is in progress.", status_failures)
                        continue
                    runner = replacement
                    last_runner_attempt = time.monotonic()
                refreshed = refresh_runtime(state, config, socket_path)
                if refreshed is not None and authenticated_status(refreshed, config) is not None:
                    status_failures = 0
                    write_status(state, "connected",
                                 "Connected to the authenticated Windows developer runner.", 0)
                    continue
                # A slow or unavailable HTTP response is not permission to terminate
                # a potentially active runner. Only SSH loss leaves this loop.
                status_failures += 1
                if (not runner_owned and status_failures >= 3
                        and runner_process_absent(config, socket_path) is True
                        and time.monotonic() - last_runner_attempt >= RUNNER_RETRY_SECONDS):
                    replacement = respawn_runner_if_allowed(
                        state, config, socket_path, runtime)
                    if replacement is not None:
                        runner = replacement
                        runner_owned = True
                        last_runner_attempt = time.monotonic()
                phase = "needs_attention" if status_failures >= 8 else "reconnecting"
                message = ("Authenticated runner status needs attention."
                           if phase == "needs_attention"
                           else "SSH is connected; waiting for authenticated runner status.")
                write_status(state, phase, message, status_failures)
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
            release_connection_window(connection_window)
            connection_window = None
            append_log(state, type(error).__name__)
            terminate(runner); runner = None
            terminate(master); master = None
            attempt += 1
            if attempt >= 8:
                write_status(state, "needs_attention", "Connection needs attention; check the configured identity and host record.", attempt)
            else:
                write_status(state, "reconnecting", "Connection interrupted; retrying with bounded backoff.", attempt)
            deadline = time.monotonic() + BACKOFF[min(attempt - 1, len(BACKOFF) - 1)]
            while time.monotonic() < deadline and not STOP:
                time.sleep(.1)
    terminate(runner)
    terminate(master)
    if config is not None and reused_master and socket_is_live(config, socket_path):
        try:
            subprocess.run(control_command(config, socket_path, "exit"),
                           stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL, timeout=5)
        except (OSError, subprocess.TimeoutExpired):
            append_log(state, "ControlExitFailed")
    write_status(state, "stopped", "Connection supervisor stopped.", attempt)
    return 0


def launchd_domain():
    return f"gui/{os.getuid()}"


def service_is_loaded():
    try:
        return subprocess.run(["/bin/launchctl", "print", f"{launchd_domain()}/{LABEL}"],
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, timeout=5).returncode == 0
    except subprocess.TimeoutExpired as error:
        raise RuntimeError("launchd service state is unavailable") from error


def activate_service(plist_path, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if not service_is_loaded():
            subprocess.run(["/bin/launchctl", "bootstrap", launchd_domain(),
                            str(plist_path)], stdin=subprocess.DEVNULL,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                           timeout=5)
        if service_is_loaded():
            kicked = subprocess.run(
                ["/bin/launchctl", "kickstart", f"{launchd_domain()}/{LABEL}"],
                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL, timeout=5)
            if kicked.returncode == 0:
                time.sleep(.1)
                if service_is_loaded():
                    return
        time.sleep(.2)
    raise RuntimeError("Persistent connection service could not be activated")


def install(state, source):
    state = Path(state)
    validate_config(state)
    destination = state / "developer-connection.py"
    atomic_write(destination, Path(source).read_bytes(), 0o700)
    plist_path = Path.home() / "Library/LaunchAgents" / f"{LABEL}.plist"
    direct_private_directory(plist_path.parent, "LaunchAgents directory",
                             create=True, require_private=False)
    payload = {"Label": LABEL, "ProgramArguments": [sys.executable, str(destination), "supervise", "--state-dir", str(state)],
               "RunAtLoad": True, "KeepAlive": True, "ThrottleInterval": 5,
               "ProcessType": "Background"}
    atomic_write(plist_path, plistlib.dumps(payload), 0o600)
    activate_service(plist_path)
    record_connection_ownership(state)


def stop_signal(unused_signum, unused_frame):
    global STOP
    STOP = True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="action", required=True)
    for name in ("supervise", "validate-config"):
        child = subparsers.add_parser(name)
        child.add_argument("--state-dir", type=Path, default=default_state())
    child = subparsers.add_parser("install")
    child.add_argument("--state-dir", type=Path, default=default_state())
    child.add_argument("--source", type=Path, required=True)
    args = parser.parse_args()
    if args.action == "validate-config":
        validate_config(args.state_dir)
        return
    if args.action == "install":
        install(args.state_dir, args.source)
        return
    signal.signal(signal.SIGTERM, stop_signal)
    signal.signal(signal.SIGINT, stop_signal)
    raise SystemExit(supervise(args.state_dir))


if __name__ == "__main__":
    main()
