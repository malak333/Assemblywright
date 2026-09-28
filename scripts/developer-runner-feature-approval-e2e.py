#!/usr/bin/env python3
"""Native runner/OpenCode proof for feature-scoped tool approvals."""

from developer_planning_fixture import enqueue_with_plan
from developer_review_fixture import reviewer_arguments

import argparse
import http.server
import json
from pathlib import Path
import shlex
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
import uuid


def free_port():
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        return reservation.getsockname()[1]


def shell_command(parts):
    if sys.platform == "win32":
        return subprocess.list2cmdline([str(part) for part in parts])
    return shlex.join([str(part) for part in parts])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--opencode-executable", required=True)
    args = parser.parse_args()
    binary = str(Path(args.binary).resolve())
    opencode = str(Path(args.opencode_executable).resolve())
    assert Path(opencode).is_file(), opencode

    fixture = {"marker": "", "calls": 0, "requests": []}

    class Model(http.server.BaseHTTPRequestHandler):
        def reply(self, value, status=200):
            encoded = json.dumps(value).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)

        def do_GET(self):
            if self.path == "/v1/models":
                return self.reply({"object": "list", "data": [
                    {"id": "approval-fixture", "object": "model"}
                ]})
            return self.reply({"error": "unexpected route"}, 404)

        def do_POST(self):
            if self.path != "/v1/chat/completions":
                return self.reply({"error": "unexpected route"}, 404)
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            fixture["calls"] += 1
            fixture["requests"].append({"stream": request.get("stream"),
                "roles": [message.get("role") for message in request.get("messages", [])]})
            continued = any(message.get("role") == "tool" for message in request["messages"])
            if continued:
                message = {"role": "assistant", "content": "The exact requested action finished."}
                finish = "stop"
            else:
                command = shell_command([
                    sys.executable, "-c",
                    "from pathlib import Path; Path(" + repr(fixture["marker"])
                    + ").write_text('once', encoding='utf-8')",
                ])
                message = {"role": "assistant", "content": None, "tool_calls": [{
                    "id": "approval-call-" + str(fixture["calls"]),
                    "type": "function",
                    "function": {"name": "bash", "arguments": json.dumps({
                        "command": command, "description": "Write exact approval marker"
                    })},
                }]}
                finish = "tool_calls"
            response = {"id": "fixture", "object": "chat.completion", "created": 1,
                "model": "approval-fixture", "choices": [{"index": 0, "message": message,
                "finish_reason": finish}], "usage": {"prompt_tokens": 1,
                "completion_tokens": 1, "total_tokens": 2}}
            if not request.get("stream"):
                return self.reply(response)
            delta = {"role": "assistant"}
            if message.get("content") is not None:
                delta["content"] = message["content"]
            if message.get("tool_calls") is not None:
                delta["tool_calls"] = [dict(call, index=index)
                    for index, call in enumerate(message["tool_calls"])]
            chunks = [
                {"id": "fixture", "object": "chat.completion.chunk", "created": 1,
                    "model": "approval-fixture", "choices": [{"index": 0,
                    "delta": delta, "finish_reason": None}]},
                {"id": "fixture", "object": "chat.completion.chunk", "created": 1,
                    "model": "approval-fixture", "choices": [{"index": 0,
                    "delta": {}, "finish_reason": finish}]},
            ]
            encoded = b"".join(b"data: " + json.dumps(chunk).encode() + b"\n\n"
                for chunk in chunks) + b"data: [DONE]\n\n"
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)

        def log_message(self, *_unused):
            pass

    model = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
    model.daemon_threads = True
    threading.Thread(target=model.serve_forever, daemon=True).start()

    with tempfile.TemporaryDirectory(prefix="assemblywright-feature-approval-e2e-") as temp:
        root = Path(temp)
        data = root / "state"
        projects = root / "projects"
        data.mkdir()
        projects.mkdir()
        output = (root / "runner.log").open("wb")
        listen = free_port()
        process = subprocess.Popen([
            binary, "--data-dir", str(data), "--workspace-root", str(projects),
            "--bind", f"127.0.0.1:{listen}", "--model-url",
            f"http://127.0.0.1:{model.server_port}/v1", "--model", "approval-fixture",
            "--opencode-executable", opencode, *reviewer_arguments(root),
        ], stdin=subprocess.DEVNULL, stdout=output, stderr=output)
        token = ""

        def api(path="status", body=None):
            request = urllib.request.Request(f"http://127.0.0.1:{listen}/{path}",
                data=None if body is None else json.dumps(body).encode(), headers={
                    "Authorization": "Bearer " + token, "Content-Type": "application/json"})
            with urllib.request.urlopen(request, timeout=20) as response:
                return json.load(response)

        def rejected(path, body, expected=(400, 409, 422)):
            try:
                api(path, body)
                raise AssertionError(("unexpected acceptance", path, body))
            except urllib.error.HTTPError as error:
                assert error.code in expected, (error.code, error.read().decode(errors="replace"))

        def wait(predicate, timeout=60):
            deadline = time.monotonic() + timeout
            last = None
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise AssertionError(("runner exited", (root / "runner.log").read_text(errors="replace")))
                try:
                    last = api()
                    if predicate(last):
                        return last
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(0.05)
            raise AssertionError(("timed out", last, fixture["requests"],
                (root / "runner.log").read_text(errors="replace")))

        def feature(snapshot, feature_id):
            return next(item for item in snapshot["queue"] if item["id"] == feature_id)

        def pending(feature_id):
            state = wait(lambda current: any(item["id"] == feature_id
                and item.get("pending_tool_approval") for item in current["queue"]))
            return state, feature(state, feature_id), feature(state, feature_id)["pending_tool_approval"]

        def approval_body(item, approval, decision):
            return {"feature_id": item["id"], "project": item["project"],
                "expected_checkpoint": item["checkpoint"], "request_id": approval["request_id"],
                "approval_id": approval["id"], "access_revision": approval["access_revision"],
                "decision": decision}

        def enqueue(name, marker):
            project = projects / name
            project.mkdir()
            fixture["marker"] = str(project / marker)
            feature_id = str(uuid.uuid4())
            state = enqueue_with_plan(f"http://127.0.0.1:{listen}", token, {
                "id": feature_id, "project": name,
                "instruction": "Request the provided bash tool exactly once.",
                "validation": shell_command([sys.executable, "-c",
                    "from pathlib import Path; assert Path(" + repr(fixture["marker"])
                    + ").read_text(encoding='utf-8') == 'once'"]),
            })
            item = feature(state, feature_id)
            api("control", {"action": "start", "expected_feature_id": feature_id,
                "expected_model_target": item.get("model_target", "mac"),
                "expected_status": item["status"], "expected_checkpoint": item["checkpoint"]})
            return feature_id, Path(fixture["marker"])

        try:
            deadline = time.monotonic() + 45
            while not (data / "developer-token").exists() and time.monotonic() < deadline:
                if process.poll() is not None:
                    raise AssertionError((root / "runner.log").read_text(errors="replace"))
                time.sleep(0.05)
            token = (data / "developer-token").read_text().strip()
            wait(lambda _state: True)

            approved_id, approved_marker = enqueue("approved", "approved.txt")
            _, approved_feature, approved = pending(approved_id)
            exact = approval_body(approved_feature, approved, "approve")
            assert approved_feature["project"] == "approved"
            assert approved["summary"].startswith("bash:")
            assert approved["tool"] == "bash"
            for changed in [
                dict(exact, feature_id=str(uuid.uuid4())),
                dict(exact, project="wrong-project"),
                dict(exact, expected_checkpoint="stale-checkpoint"),
                dict(exact, request_id=str(uuid.uuid4())),
                dict(exact, approval_id=str(uuid.uuid4())),
                dict(exact, access_revision=approved["access_revision"] + 1),
            ]:
                rejected("feature/tool-approval", changed)
                current = feature(api(), approved_id)["pending_tool_approval"]
                assert current["id"] == approved["id"]
                assert not approved_marker.exists()
            api("feature/tool-approval", exact)
            wait(lambda current: approved_marker.exists() and not current["running"]
                and feature(current, approved_id)["status"] in ("succeeded", "failed"))
            assert approved_marker.read_text() == "once"
            rejected("feature/tool-approval", exact)

            stopped_id, stopped_marker = enqueue("stopped", "stopped.txt")
            _, stopped_feature, stopped = pending(stopped_id)
            stopped_request = approval_body(stopped_feature, stopped, "approve")
            api("control", {"action": "stop"})
            rejected("feature/tool-approval", stopped_request)
            stopped_state = wait(lambda current: feature(current, stopped_id)["status"] == "paused")
            assert not stopped_marker.exists()
            assert feature(stopped_state, stopped_id).get("pending_tool_approval") is None
            api("control", {"action": "remove", "id": stopped_id})

            emergency_id, emergency_marker = enqueue("emergency", "emergency.txt")
            _, emergency_feature, emergency = pending(emergency_id)
            emergency_request = approval_body(emergency_feature, emergency, "approve")
            api("control", {"action": "emergency"})
            rejected("feature/tool-approval", emergency_request)
            emergency_state = wait(lambda current: current["emergency_paused"]
                and not current["running"]
                and feature(current, emergency_id)["status"] == "paused"
                and not feature(current, emergency_id).get("pending_tool_approval"))
            assert not emergency_marker.exists()
            assert feature(emergency_state, emergency_id).get("pending_tool_approval") is None
            api("control", {"action": "clear_emergency"})
            api("control", {"action": "remove", "id": emergency_id})

            denied_id, denied_marker = enqueue("denied", "denied.txt")
            _, denied_feature, denied = pending(denied_id)
            denied_request = approval_body(denied_feature, denied, "deny")
            api("feature/tool-approval", denied_request)
            wait(lambda current: not current["running"]
                and feature(current, denied_id)["status"] in ("succeeded", "failed"))
            assert not denied_marker.exists()
            rejected("feature/tool-approval", denied_request)

            print(json.dumps({"feature_tool_approval_native_e2e": "passed",
                "projection_and_exact_binding": True, "approve_executes_once": True,
                "deny_rejects_exact_action": True, "replay_rejected": True,
                "stop_wins": True, "emergency_pause_rejects": True}, sort_keys=True))
        finally:
            if process.poll() is None:
                try:
                    api("control", {"action": "shutdown"})
                    process.wait(timeout=15)
                except (OSError, urllib.error.URLError, subprocess.TimeoutExpired):
                    process.kill()
                    process.wait(timeout=10)
            output.close()
            model.shutdown()
            model.server_close()


if __name__ == "__main__":
    main()
