#!/usr/bin/env python3
"""Native staged repair routing proofs with real process boundaries."""

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
    parser.add_argument("--protected-staged-retry-only", action="store_true",
        help="prove a protected rejection after staged repair autonomously starts a second staged attempt")
    parser.add_argument("--empty-staged-retry-only", action="store_true",
        help="prove a completed clean empty stage consumes one attempt and autonomously starts a fresh staged attempt")
    args = parser.parse_args()
    selected_modes = sum([args.candidate_rejection_route_only,
        args.protected_staged_retry_only, args.empty_staged_retry_only])
    if selected_modes > 1:
        parser.error("select only one staged repair proof mode")
    binary = str(Path(args.binary).resolve())
    opencode = args.opencode_executable or provision_pinned_runtime(
        Path(__file__).resolve().parents[1])
    opencode = str(Path(opencode).resolve())
    if not Path(opencode).is_file():
        parser.error(f"pinned OpenCode executable does not exist: {opencode}")

    phases = []
    staged_feedback_observed = []
    model_calls = []

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
            model_calls.append(None)
            prompt = json.dumps(request.get("messages", []))
            if not request.get("tools"):
                raise AssertionError("lineage fixture expected the real project-tool lane")
            tool_results = [message for message in request.get("messages", [])
                if message.get("role") == "tool"]
            staged_prompt = "Staged build environment:" in prompt
            if tool_results:
                return model_reply(self, request,
                    {"content": "Completed the bounded project change."}, "stop")
            candidate_route = "candidate-rejection-route" in prompt
            protected_staged_retry = "protected-staged-retry" in prompt
            empty_staged_retry = "empty-staged-retry" in prompt
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
            elif protected_staged_retry and "Staged build environment:" in prompt:
                staged_attempt = 1 + sum(
                    phase.startswith("protected_staged_") for phase in phases)
                phase = f"protected_staged_{staged_attempt}"
                if staged_attempt == 1:
                    code = ("from pathlib import Path; "
                        "Path('app.py').write_text('VALUE = 1\\n'); "
                        "Path('tests/test_site.py').write_text('# VALUE = 0\\nassert True\\n'); "
                        "Path('stage.txt').write_text('protected staged repair 1\\n')")
                else:
                    code = ("from pathlib import Path; "
                        "Path('app.py').write_text('VALUE = 1\\n'); "
                        "Path('tests/test_site.py').write_text('assert True\\n'); "
                        "Path('stage.txt').write_text('protected staged repair 2\\n')")
            elif protected_staged_retry:
                phase = "protected_retry_initial"
                code = ("from pathlib import Path; Path('tests').mkdir(exist_ok=True); "
                    "Path('app.py').write_text('WRONG = 0\\n'); "
                    "Path('tests/test_site.py').write_text('# VALUE = 0\\nassert True\\n')")
            elif empty_staged_retry and staged_prompt:
                staged_attempt = 1 + sum(
                    phase.startswith("empty_staged_") for phase in phases)
                phase = f"empty_staged_{staged_attempt}"
                if staged_attempt == 1:
                    code = ("from pathlib import Path; "
                        "assert Path('tests/test_site.py').read_text(); "
                        "assert Path('app.py').read_text()")
                else:
                    code = ("from pathlib import Path; "
                        "Path('app.py').write_text('VALUE = 1\\n'); "
                        "Path('tests/test_site.py').write_text('assert True\\n'); "
                        "Path('stage.txt').write_text('fresh staged repair after empty\\n')")
            elif empty_staged_retry:
                phase = "empty_retry_initial"
                code = ("from pathlib import Path; Path('tests').mkdir(exist_ok=True); "
                    "Path('app.py').write_text('WRONG = 0\\n'); "
                    "Path('tests/test_site.py').write_text('# VALUE = 0\\nassert True\\n')")
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

        def timeout_diagnostic(state, timeout):
            queue = state.get("queue", []) if isinstance(state, dict) else []
            feature = queue[0] if len(queue) == 1 else None
            current = None if feature is None else {
                key: feature.get(key) for key in [
                    "status", "checkpoint", "auto_repair_lifecycle",
                    "auto_repair_step_elapsed_ms", "repair_attempts",
                    "escalation_count", "escalation_status", "review_attempts",
                    "review_status", "tool_workspace_revision",
                ]
            }
            global_state = None if not isinstance(state, dict) else {
                key: state.get(key) for key in [
                    "revision", "running", "emergency_paused", "repair_active",
                    "escalation_running", "tools_running", "tools_need_attention",
                ]
            }
            return {
                "timeout_seconds": timeout,
                "phase_count": len(phases),
                "phases": list(phases),
                "model_call_count": len(model_calls),
                "state": global_state,
                "feature": current,
                "runner_log_bytes": log_path.stat().st_size if log_path.exists() else 0,
            }

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
            raise AssertionError("Timed out: " + json.dumps(
                timeout_diagnostic(state, timeout), sort_keys=True))

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
                if args.candidate_rejection_route_only else
                "protected-staged-retry" if args.protected_staged_retry_only else
                "empty-staged-retry" if args.empty_staged_retry_only else "lineage")
            instruction = ((
                "Build the candidate-rejection-route fixture "
                "[fixture:source-regression]. The source correction must preserve the "
                "existing protected regression input."
            ) if args.candidate_rejection_route_only else ((
                "Build the protected-staged-retry fixture [fixture:reject-zero]. "
                "Every protected blocker requires a fresh staged repair."
            ) if args.protected_staged_retry_only else (
                "Build the empty-staged-retry fixture [fixture:reject-zero]. "
                "A clean completed empty attempt must consume its bound and continue."
            ) if args.empty_staged_retry_only else (
                "Build the lineage fixture [fixture:reject-zero]. "
                "The first protected blocker requires staged repair; any later source "
                "blocker must use ordinary repair."
            )))
            state = enqueue_with_plan(f"http://127.0.0.1:{port}", token, {
                "id": feature_id, "project": project_name, "model_target": "windows",
                "instruction": instruction,
                "validation": f'"{sys.executable}" -B tests/test_site.py'})
            feature = next(item for item in state["queue"] if item["id"] == feature_id)
            api("control", {"action": "resume", "expected_feature_id": feature_id,
                "expected_model_target": feature["model_target"],
                "expected_status": feature["status"],
                "expected_checkpoint": feature["checkpoint"]})
            workflow_timeout = 240 if (
                args.protected_staged_retry_only or args.empty_staged_retry_only
            ) else 150
            completed = wait(lambda state: not state["running"] and
                next(item for item in state["queue"] if item["id"] == feature_id)["status"]
                == "succeeded", timeout=workflow_timeout)
            public = next(item for item in completed["queue"] if item["id"] == feature_id)
            expected_repairs = 0 if (args.protected_staged_retry_only or
                args.empty_staged_retry_only) else 1
            expected_escalations = 2 if (args.protected_staged_retry_only or
                args.empty_staged_retry_only) else 1
            assert public["repair_attempts"] == expected_repairs, public
            assert public["escalation_count"] == expected_escalations, public
            expected_review_attempts = 2 if (
                args.candidate_rejection_route_only or args.empty_staged_retry_only
            ) else 3
            assert public["review_attempts"] == expected_review_attempts, public
            if args.candidate_rejection_route_only:
                assert phases == ["candidate_route_initial", "ordinary_protected_candidate",
                    "candidate_rejection_staged_automatic"], phases
            elif args.protected_staged_retry_only:
                assert phases == ["protected_retry_initial", "protected_staged_1",
                    "protected_staged_2"], phases
            elif args.empty_staged_retry_only:
                assert phases == ["empty_retry_initial", "empty_staged_1",
                    "empty_staged_2"], phases
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
                if args.candidate_rejection_route_only or args.protected_staged_retry_only or
                args.empty_staged_retry_only
                else "failed")
            assert proposal["status"] == expected_proposal_status and proposal["staged_binding"]
            assert proposal["application_state_sha256"]
            receipts = [item for item in feature["escalation_history"]
                if item["proposal_id"] == proposal["proposal_id"]]
            assert [item["outcome"] for item in receipts] == [
                "ready", "policy_authorized", expected_proposal_status], receipts
            if not args.protected_staged_retry_only and not args.empty_staged_retry_only:
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
            elif args.protected_staged_retry_only:
                assert feature["repair_history"] == []
                assert proposal["feature_checkpoint"] == "review_2_rejected"
                assert [item["outcome"] for item in feature["review_history"]] == [
                    "rejected", "rejected", "approved"]
                assert [item["blocking_findings"][0]["path"]
                    for item in feature["review_history"][:2]] == [
                        "tests/test_site.py", "tests/test_site.py"]
                proposal_receipts = {}
                for receipt in feature["escalation_history"]:
                    proposal_receipts.setdefault(receipt["proposal_id"], []).append(
                        receipt["outcome"])
                assert list(proposal_receipts.values()) == [
                    ["ready", "policy_authorized", "failed"],
                    ["ready", "policy_authorized", "succeeded"]], proposal_receipts
            elif args.empty_staged_retry_only:
                assert feature["repair_history"] == []
                assert proposal["feature_checkpoint"] == "escalation_1_no_op"
                assert [item["outcome"] for item in feature["review_history"]] == [
                    "rejected", "not_run", "approved"]
                proposal_receipts = {}
                for receipt in feature["escalation_history"]:
                    proposal_receipts.setdefault(receipt["proposal_id"], []).append(
                        receipt["outcome"])
                assert list(proposal_receipts.values()) == [
                    ["no_op", "authorization_not_run", "application_not_run"],
                    ["ready", "policy_authorized", "succeeded"]], proposal_receipts
                with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
                    stage_rows = database.execute(
                        "SELECT status,mutation_count,text_bytes,asset_bytes,serialized_bytes "
                        "FROM developer_tool_stage WHERE project=? ORDER BY rowid",
                        (project_name,)).fetchall()
                    payload_count = database.execute(
                        "SELECT COUNT(*) FROM developer_tool_stage_mutation").fetchone()[0]
                assert stage_rows[0] == ("compacted", 0, 0, 0, 0), stage_rows
                assert stage_rows[1][0] == "compacted" and stage_rows[1][1] > 0, stage_rows
                assert payload_count == 0
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
                "source_only_review_routes_to_ordinary_successor":
                    not args.candidate_rejection_route_only and
                    not args.protected_staged_retry_only and
                    not args.empty_staged_retry_only,
                "ordinary_successor_revalidated_and_freshly_reviewed":
                    not args.candidate_rejection_route_only and
                    not args.protected_staged_retry_only and
                    not args.empty_staged_retry_only,
                "protected_staged_rejection_autonomously_retries_staged":
                    args.protected_staged_retry_only,
                "completed_empty_stage_autonomously_retries_staged":
                    args.empty_staged_retry_only,
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
