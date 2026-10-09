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
    parser.add_argument("--candidate-rejection-route-only", action="store_true",
        help="prove source-review feedback survives a rejected protected candidate and routes directly to staged repair")
    args = parser.parse_args()
    binary = str(Path(args.binary).resolve())
    opencode = args.opencode_executable or provision_pinned_runtime(
        Path(__file__).resolve().parents[1])
    opencode = str(Path(opencode).resolve())
    if not Path(opencode).is_file():
        parser.error(f"pinned OpenCode executable does not exist: {opencode}")

    phases = []
    staged_feedback_observed = []

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
            candidate_route = "candidate-rejection-route" in prompt
            if candidate_route and "repair attempt 1 of 3" in prompt.lower():
                phase = "ordinary_protected_candidate"
                code = ("from pathlib import Path; "
                    "Path('app.py').write_text('VALUE = 1\\n'); "
                    "Path('tests/test_site.py').write_text('assert False\\n')")
            elif candidate_route and "Staged build environment:" in prompt:
                phase = "candidate_rejection_staged_automatic"
                staged_feedback_observed.append(
                    "Correct the publisher output and add the regression assertion." in prompt)
                code = ("from pathlib import Path; "
                    "Path('app.py').write_text('VALUE = 1\\n'); "
                    "Path('stage.txt').write_text('candidate rejection staged repair\\n')")
            elif candidate_route:
                phase = "candidate_route_initial"
                code = ("from pathlib import Path; Path('tests').mkdir(exist_ok=True); "
                    "Path('app.py').write_text('VALUE = 0\\n'); "
                    "Path('tests/test_site.py').write_text('assert True\\n')")
            elif "repair attempt 1 of 3" in prompt.lower():
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
            project_name = ("candidate-rejection-route"
                if args.candidate_rejection_route_only else "lineage")
            instruction = ((
                "Build the candidate-rejection-route fixture "
                "[fixture:source-regression]. The source correction must preserve the "
                "existing protected regression input."
            ) if args.candidate_rejection_route_only else (
                "Build the lineage fixture [fixture:reject-zero]. "
                "The first protected blocker requires staged repair; any later source "
                "blocker must use ordinary repair."
            ))
            state = enqueue_with_plan(f"http://127.0.0.1:{port}", token, {
                "id": feature_id, "project": project_name, "model_target": "windows",
                "instruction": instruction,
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
            expected_review_attempts = 2 if args.candidate_rejection_route_only else 3
            assert public["review_attempts"] == expected_review_attempts, public
            if args.candidate_rejection_route_only:
                assert phases == ["candidate_route_initial", "ordinary_protected_candidate",
                    "candidate_rejection_staged_automatic"], phases
            else:
                assert phases == [
                    "initial", "staged_automatic", "ordinary_successor"], phases
            assert (projects / project_name / "app.py").read_text() == "VALUE = 1\n"
            assert (projects / project_name / "stage.txt").is_file()
            if args.candidate_rejection_route_only:
                assert staged_feedback_observed == [True], staged_feedback_observed
                assert (projects / project_name / "tests/test_site.py").read_text() == \
                    "assert True\n"

            with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
                durable = json.loads(database.execute(
                    "SELECT state FROM developer_state WHERE id=1").fetchone()[0])
            queue = next(value for key, value in durable.items() if key.startswith("queue_v"))
            feature = next(item for item in queue if item["id"] == feature_id)
            proposal = feature["escalation_proposal"]
            expected_proposal_status = ("succeeded"
                if args.candidate_rejection_route_only else "failed")
            assert proposal["status"] == expected_proposal_status and proposal["staged_binding"]
            assert proposal["application_state_sha256"]
            receipts = [item for item in feature["escalation_history"]
                if item["proposal_id"] == proposal["proposal_id"]]
            assert [item["outcome"] for item in receipts] == [
                "ready", "policy_authorized", expected_proposal_status], receipts
            repair = feature["repair_history"][0]
            expected_prior_review = ("review_1_rejected"
                if args.candidate_rejection_route_only else "review_2_rejected")
            assert repair["prior_checkpoint"] == expected_prior_review, repair
            if args.candidate_rejection_route_only:
                assert "Correct the publisher output and add the regression assertion." in \
                    repair["prior_message"]
                finding = feature["review_history"][0]["blocking_findings"][0]
                assert finding == {"finding_id": "publisher-regression", "path": "app.py",
                    "message": "Correct the publisher output and add the regression assertion."}
                assert [item["outcome"] for item in feature["review_history"]] == [
                    "rejected", "approved"]
                assert proposal["feature_checkpoint"] == "staged_tool_candidate_rejected"
                assert len(feature["repair_history"]) == 1
            else:
                feedback = json.loads(repair["prior_message"])
                assert feedback["feedback"]["blocking_findings"][0]["path"] == "app.py"
                assert [item["outcome"] for item in feature["review_history"]] == [
                    "rejected", "rejected", "approved"]

            malformed_linkage_held = False
            if args.candidate_rejection_route_only:
                calls_before_malformed = list(phases)
                reviews_before_malformed = len(feature["review_history"])
                process.terminate()
                process.wait(timeout=15)
                with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
                    persisted = json.loads(database.execute(
                        "SELECT state FROM developer_state WHERE id=1").fetchone()[0])
                    persisted_queue = next(value for key, value in persisted.items()
                        if key.startswith("queue_v"))
                    malformed = next(item for item in persisted_queue
                        if item["id"] == feature_id)
                    malformed["status"] = "failed"
                    malformed["checkpoint"] = "staged_tool_candidate_rejected"
                    malformed["message"] = "Rejected staged candidate retained for malformed-linkage proof"
                    malformed["last_failure_kind"] = "candidate_rejection"
                    malformed["last_code_failure_summary"] = malformed["message"]
                    malformed["repair_attempts"] = 1
                    malformed["repair_history"] = []
                    malformed["repair_pending"] = False
                    malformed["escalation_pending"] = False
                    malformed["review_attempts"] = 1
                    malformed["review_history"] = malformed["review_history"][:1]
                    malformed["review_pending"] = None
                    malformed["review_status"] = "rejected"
                    malformed["review_summary"] = malformed["review_history"][0]["summary"]
                    malformed["edits"] = None
                    malformed["auto_repair_lifecycle"] = "held"
                    malformed["auto_repair_reason"] = \
                        "Malformed-linkage proof awaits explicit Resume"
                    persisted["revision"] += 1
                    database.execute("UPDATE developer_state SET state=? WHERE id=1",
                        (json.dumps(persisted, separators=(",", ":")),))
                    database.commit()
                process = subprocess.Popen(command, stdin=subprocess.DEVNULL,
                    stdout=log, stderr=log)
                restarted = wait(lambda state: not state["running"])
                resumable = next(item for item in restarted["queue"]
                    if item["id"] == feature_id)
                api("control", {"action": "resume", "expected_feature_id": feature_id,
                    "expected_model_target": resumable["model_target"],
                    "expected_status": resumable["status"],
                    "expected_checkpoint": resumable["checkpoint"]})
                held = wait(lambda state: not state["running"] and
                    next(item for item in state["queue"] if item["id"] == feature_id)
                    ["auto_repair_lifecycle"] == "held")
                held_feature = next(item for item in held["queue"]
                    if item["id"] == feature_id)
                assert held_feature["checkpoint"] == "staged_tool_candidate_rejected"
                assert held_feature["repair_attempts"] == 1
                assert held_feature["escalation_count"] == 1
                assert "malformed, stale, or incomplete rejected-review linkage" in \
                    held_feature["auto_repair_reason"]
                assert phases == calls_before_malformed
                durable_held = None
                with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
                    persisted = json.loads(database.execute(
                        "SELECT state FROM developer_state WHERE id=1").fetchone()[0])
                    persisted_queue = next(value for key, value in persisted.items()
                        if key.startswith("queue_v"))
                    durable_held = next(item for item in persisted_queue
                        if item["id"] == feature_id)
                assert len(durable_held["review_history"]) == 1
                assert len(durable_held["review_history"]) == reviews_before_malformed - 1
                assert durable_held["repair_history"] == []
                malformed_linkage_held = True
            print(json.dumps({"platform": sys.platform,
                "protected_review_routes_to_staged_automatic": True,
                "staged_application_state_bound": True,
                "source_only_review_routes_to_ordinary_successor": True,
                "ordinary_successor_revalidated_and_freshly_reviewed": True,
                "candidate_rejection_routes_before_ordinary_attempt_two":
                    args.candidate_rejection_route_only,
                "candidate_rejection_preserves_exact_review_feedback":
                    args.candidate_rejection_route_only,
                "malformed_candidate_review_linkage_holds": malformed_linkage_held,
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
