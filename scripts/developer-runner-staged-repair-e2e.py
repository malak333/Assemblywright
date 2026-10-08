#!/usr/bin/env python3
"""Native staged-to-ordinary repair lineage proof with real process boundaries."""

from developer_opencode_runtime import provision_pinned_runtime
from developer_planning_fixture import enqueue_with_plan
from developer_review_fixture import reviewer_arguments

import argparse
from contextlib import closing
import http.server
import json
import os
from pathlib import Path
import shlex
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
import uuid


def free_port():
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        return reservation.getsockname()[1]


def model_reply(handler, request, message, finish_reason):
    if request.get("stream"):
        chunks = [
            {"id": "fixture", "object": "chat.completion.chunk", "created": 1,
             "model": request.get("model", "fixture"),
             "choices": [{"index": 0, "delta": {"role": "assistant", **message},
                          "finish_reason": None}]},
            {"id": "fixture", "object": "chat.completion.chunk", "created": 1,
             "model": request.get("model", "fixture"),
             "choices": [{"index": 0, "delta": {}, "finish_reason": finish_reason}]},
        ]
        body = b"".join(b"data: " + json.dumps(item).encode() + b"\n\n" for item in chunks)
        body += b"data: [DONE]\n\n"
        content_type = "text/event-stream"
    else:
        body = json.dumps({"id": "fixture", "object": "chat.completion", "created": 1,
            "model": request.get("model", "fixture"),
            "choices": [{"index": 0, "message": {"role": "assistant", **message},
                         "finish_reason": finish_reason}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1,
                      "total_tokens": 2}}).encode()
        content_type = "application/json"
    handler.send_response(200)
    handler.send_header("Content-Type", content_type)
    handler.send_header("Content-Length", str(len(body)))
    handler.end_headers()
    handler.wfile.write(body)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--opencode-executable",
        default=os.environ.get("ASSEMBLYWRIGHT_DEVELOPER_OPENCODE_EXECUTABLE"))
    args = parser.parse_args()
    binary = str(Path(args.binary).resolve())
    opencode = args.opencode_executable or provision_pinned_runtime(
        Path(__file__).resolve().parents[1])
    opencode = str(Path(opencode).resolve())
    if not Path(opencode).is_file():
        parser.error(f"pinned OpenCode executable does not exist: {opencode}")

    phases = []

    class Model(http.server.BaseHTTPRequestHandler):
        def log_message(self, *unused):
            pass

        def do_GET(self):
            if self.path == "/props":
                body = json.dumps({"total_slots": 1, "modalities": {"vision": False},
                    "default_generation_settings": {"n_ctx": 262144}}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return
            self.send_error(404)

        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            if self.path == "/apply-template":
                return self._json({"prompt": json.dumps(request["messages"])})
            if self.path == "/tokenize":
                return self._json({"count": max(1, len(request["content"]) // 4)})
            if self.path != "/v1/chat/completions":
                self.send_error(404)
                return
            prompt = json.dumps(request.get("messages", []))
            if not request.get("tools"):
                raise AssertionError("lineage fixture expected the real project-tool lane")
            tool_results = [message for message in request.get("messages", [])
                if message.get("role") == "tool"]
            if tool_results:
                return model_reply(self, request,
                    {"content": "Completed the bounded project change."}, "stop")
            if "repair attempt 1 of 3" in prompt.lower():
                phase = "ordinary_successor"
                code = "from pathlib import Path; Path('app.py').write_text('VALUE = 1\\n')"
            elif "Staged build environment:" in prompt:
                phase = "staged_automatic"
                code = ("from pathlib import Path; "
                    "Path('app.py').write_text('VALUE = 0\\n'); "
                    "Path('tests/test_site.py').write_text('assert True\\n'); "
                    "Path('stage.txt').write_text('staged automatic repair\\n')")
            else:
                phase = "initial"
                code = ("from pathlib import Path; Path('tests').mkdir(exist_ok=True); "
                    "Path('app.py').write_text('WRONG = 0\\n'); "
                    "Path('tests/test_site.py').write_text('# VALUE = 0\\nassert True\\n')")
            phases.append(phase)
            command = (subprocess.list2cmdline([sys.executable, "-B", "-c", code])
                if os.name == "nt" else shlex.join([sys.executable, "-B", "-c", code]))
            tools = {item["function"]["name"]: item["function"]
                for item in request["tools"] if item.get("type") == "function"}
            bash = tools["bash"]
            arguments = {"command": command, "description": phase}
            for required in bash.get("parameters", {}).get("required", []):
                arguments.setdefault(required, phase)
            model_reply(self, request, {"content": None, "tool_calls": [{
                "index": 0, "id": f"{phase}-write", "type": "function",
                "function": {"name": "bash", "arguments": json.dumps(arguments)}}]},
                "tool_calls")

        def _json(self, value):
            body = json.dumps(value).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    model = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
    windows_model = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
    model.daemon_threads = True
    windows_model.daemon_threads = True
    threading.Thread(target=model.serve_forever, daemon=True).start()
    threading.Thread(target=windows_model.serve_forever, daemon=True).start()

    with tempfile.TemporaryDirectory(prefix="aw-staged-repair-lineage-e2e-") as temporary:
        root = Path(temporary)
        data, projects = root / "state", root / "projects"
        data.mkdir()
        projects.mkdir()
        with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
            database.execute("CREATE TABLE developer_state(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL)")
            database.execute("INSERT INTO developer_state(id,state) VALUES(1,?)", (json.dumps({
                "revision": 4, "auto_run": False, "emergency_paused": False,
                "queue_v2": []}),))
            database.commit()

        port = free_port()
        log_path = root / "runner.log"
        log = log_path.open("ab")
        review_args = reviewer_arguments(root)
        command = [binary, "--data-dir", str(data), "--workspace-root", str(projects),
            "--bind", f"127.0.0.1:{port}", "--model-url",
            f"http://127.0.0.1:{model.server_port}/v1", "--model", "fixture",
            "--windows-model-url", f"http://127.0.0.1:{windows_model.server_port}/v1",
            "--windows-model", "fixture", "--opencode-executable", opencode] + review_args
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        token = ""

        def api(path="status", body=None):
            request = urllib.request.Request(f"http://127.0.0.1:{port}/{path}",
                data=None if body is None else json.dumps(body).encode(),
                headers={"Content-Type": "application/json",
                         "Authorization": "Bearer " + token})
            with urllib.request.urlopen(request, timeout=15) as response:
                return json.load(response)

        def wait(predicate, timeout=150):
            deadline = time.monotonic() + timeout
            state = None
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise AssertionError(log_path.read_text(errors="replace"))
                try:
                    state = api()
                    if predicate(state):
                        return state
                except OSError:
                    pass
                time.sleep(.05)
            raise AssertionError(f"Timed out: {state}\n{log_path.read_text(errors='replace')}")

        try:
            deadline = time.monotonic() + 20
            ready = False
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise AssertionError(log_path.read_text(errors="replace"))
                token_path = data / "developer-token"
                if token_path.exists():
                    token = token_path.read_text().strip()
                    try:
                        api()
                        ready = True
                        break
                    except OSError:
                        pass
                time.sleep(.05)
            if not ready:
                raise AssertionError(log_path.read_text(errors="replace"))
            state = api()
            api("auto-ai-repair", {"enabled": True, "max_escalations": 5,
                "expected_revision": state["revision"]})
            permissions = api("permissions")
            if permissions["mode"] != "full":
                api("permissions", {"mode": "full",
                    "expected_revision": permissions["revision"]})
            feature_id = str(uuid.uuid4())
            state = enqueue_with_plan(f"http://127.0.0.1:{port}", token, {
                "id": feature_id, "project": "lineage", "model_target": "windows",
                "instruction": ("Build the lineage fixture [fixture:reject-zero]. "
                    "The first protected blocker requires staged repair; any later source "
                    "blocker must use ordinary repair."),
                "validation": f'"{sys.executable}" -B tests/test_site.py'})
            feature = next(item for item in state["queue"] if item["id"] == feature_id)
            api("control", {"action": "resume", "expected_feature_id": feature_id,
                "expected_model_target": feature["model_target"],
                "expected_status": feature["status"],
                "expected_checkpoint": feature["checkpoint"]})
            completed = wait(lambda state: not state["running"] and
                next(item for item in state["queue"] if item["id"] == feature_id)["status"]
                == "succeeded")
            public = next(item for item in completed["queue"] if item["id"] == feature_id)
            assert public["repair_attempts"] == 1, public
            assert public["escalation_count"] == 1, public
            assert public["review_attempts"] == 3, public
            assert phases == ["initial", "staged_automatic", "ordinary_successor"], phases
            assert (projects / "lineage/app.py").read_text() == "VALUE = 1\n"
            assert (projects / "lineage/stage.txt").is_file()

            with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
                durable = json.loads(database.execute(
                    "SELECT state FROM developer_state WHERE id=1").fetchone()[0])
            queue = next(value for key, value in durable.items() if key.startswith("queue_v"))
            feature = next(item for item in queue if item["id"] == feature_id)
            proposal = feature["escalation_proposal"]
            assert proposal["status"] == "failed" and proposal["staged_binding"]
            assert proposal["application_state_sha256"]
            receipts = [item for item in feature["escalation_history"]
                if item["proposal_id"] == proposal["proposal_id"]]
            assert [item["outcome"] for item in receipts] == [
                "ready", "policy_authorized", "failed"], receipts
            repair = feature["repair_history"][0]
            assert repair["prior_checkpoint"] == "review_2_rejected", repair
            feedback = json.loads(repair["prior_message"])
            assert feedback["feedback"]["blocking_findings"][0]["path"] == "app.py"
            assert [item["outcome"] for item in feature["review_history"]] == [
                "rejected", "rejected", "approved"]
            print(json.dumps({"platform": sys.platform,
                "protected_review_routes_to_staged_automatic": True,
                "staged_application_state_bound": True,
                "source_only_review_routes_to_ordinary_successor": True,
                "ordinary_successor_revalidated_and_freshly_reviewed": True,
                "repair_attempts": public["repair_attempts"],
                "escalation_count": public["escalation_count"],
                "review_attempts": public["review_attempts"]}))
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=15)
            log.close()
            model.shutdown()
            windows_model.shutdown()


if __name__ == "__main__":
    main()
