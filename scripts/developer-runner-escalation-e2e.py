#!/usr/bin/env python3
"""Native runner proof for explicit chat-to-local-AI repair escalation."""

from developer_review_fixture import reviewer_arguments

import argparse
from contextlib import closing
import hashlib
import http.server
import json
from pathlib import Path
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid


def free_port():
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        return reservation.getsockname()[1]


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    binary = str(Path(parser.parse_args().binary).resolve())
    fixture = {"mode": "normal", "mac_calls": [], "chat_calls": []}
    entered = threading.Event()
    release = threading.Event()

    before_test = (
        "from pathlib import Path\n"
        "import sys\n"
        "sys.path.insert(0, str(Path(__file__).parents[1]))\n"
        "import app\n"
        "assert app.VALUE == 9\n"
    )
    after_test = before_test.replace("== 9", "== 1")

    class Handler(http.server.BaseHTTPRequestHandler):
        def reply(self, value, status=200):
            body = json.dumps(value).encode()
            try:
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                pass

        def log_message(self, *unused):
            pass

    class WindowsModel(Handler):
        def do_GET(self):
            if self.path == "/props":
                return self.reply({"total_slots": 1, "modalities": {"vision": True},
                    "default_generation_settings": {"n_ctx": 262144}})
            self.reply({"error": "unexpected route"}, 404)

        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            fixture["chat_calls"].append((self.path, request))
            if self.path == "/apply-template":
                return self.reply({"prompt": json.dumps(request["messages"])})
            if self.path == "/tokenize":
                return self.reply({"count": max(1, len(request["content"]) // 4)})
            if self.path == "/v1/chat/completions":
                return self.reply({"choices": [{"message": {"tool_calls": None,
                    "content": "The protected test still expects 9, but the approved feature and app use VALUE 1."},
                    "finish_reason": "stop"}]})
            self.reply({"error": "unexpected route"}, 404)

    class MacModel(Handler):
        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            fixture["mac_calls"].append(request)
            if fixture["mode"] == "slow":
                entered.set()
                release.wait(20)
            prompt = json.dumps(request)
            if "PARTIAL_BLOCKER" in prompt:
                proposal = {"summary": "Exercise bounded partial application.", "files": [
                    {"path": "app.py", "content": "VALUE = 2\n"},
                    {"path": "block/second.py", "content": "CREATED = True\n"}]}
            elif "[fixture:validation-failure]" in prompt:
                proposal = {"summary": "Deliberately retain a failing validation fixture.",
                    "files": [{"path": "tests/test_app.py",
                    "content": before_test.replace("== 9", "== 2")}]}
            elif "[fixture:reject-zero]" in prompt:
                proposal = {"summary": "Exercise independent reviewer rejection.", "files": [
                    {"path": "app.py", "content": "VALUE = 0\n"},
                    {"path": "tests/test_app.py",
                    "content": before_test.replace("== 9", "== 0")}]}
            else:
                proposal = {"summary": "Correct the stale protected expectation.",
                    "files": [{"path": "tests/test_app.py", "content": after_test}]}
            content = json.dumps(proposal)
            self.reply({"choices": [{"message": {"content": content},
                "finish_reason": "stop"}]})

    windows = http.server.ThreadingHTTPServer(("127.0.0.1", 0), WindowsModel)
    mac = http.server.ThreadingHTTPServer(("127.0.0.1", 0), MacModel)
    for server in (windows, mac):
        server.daemon_threads = True
        threading.Thread(target=server.serve_forever, daemon=True).start()

    with tempfile.TemporaryDirectory(prefix="assemblywright-escalation-e2e-") as temp:
        root = Path(temp)
        projects = root / "projects"
        data = root / "state"
        projects.mkdir()
        data.mkdir()
        feature_ids = {name: str(uuid.uuid4()) for name in
            ("primary", "cancel", "restart", "partial", "validation-failure",
             "review-rejection", "blocked-successor", "legacy")}
        validation = f'"{sys.executable}" -B tests/test_app.py'
        for name in feature_ids:
            project = projects / name
            (project / "tests").mkdir(parents=True)
            (project / "app.py").write_bytes(b"VALUE = 1\n")
            (project / "tests/test_app.py").write_bytes(before_test.encode("utf-8"))
        (projects / "legacy/tests/test_app.py").write_bytes(after_test.encode("utf-8"))
        (projects / "partial/block").write_bytes(b"PARTIAL_BLOCKER\n")

        def old_feature(name):
            repair_attempts = 0 if name == "partial" else 3
            app_edit = {"path": "app.py", "before": None, "content": "VALUE = 1\n"}
            app_hash = sha256(app_edit["content"].encode())
            history = [{"attempt": attempt, "prior_checkpoint": f"repair_{attempt}_prepared",
                "prior_message": f"repair {attempt} failed", "prior_edits": ([] if
                    name not in ("primary", "legacy") else [{"path": "app.py",
                    "before": None if attempt == 1 else app_hash,
                    "content_hash": app_hash}])}
                for attempt in range(1, repair_attempts + 1)]
            instruction = "Keep app.VALUE equal to 1 and correct contradictory validation evidence."
            if name == "review-rejection":
                instruction += " [fixture:reject-zero]"
            if name == "validation-failure":
                instruction += " [fixture:validation-failure]"
            return {"id": feature_ids[name], "project": name,
                "instruction": instruction,
                "validation": validation, "status": "failed",
                "checkpoint": "validation_failed" if name == "partial" else
                    "repair_3_validation_failed",
                "message": "Validation failed: test expected 9 while app.VALUE is 1" if
                    name == "partial" else
                    "Repair stopped after 3 attempts: test expected 9 while app.VALUE is 1",
                "edits": [app_edit] if name in ("primary", "legacy") else None,
                "repair_attempts": repair_attempts, "repair_pending": False,
                "repair_history": history, "model_target": "windows",
                "review_status": "pending", "review_attempts": 0,
                "review_summary": "Required ChatGPT Codex review has not started",
                "review_pending": None, "review_history": [], "planning": None}

        prior_features = [old_feature(name) for name in
            ("primary", "cancel", "restart", "validation-failure",
             "review-rejection", "blocked-successor", "partial", "legacy")]
        successor = next(item for item in prior_features if item["project"] == "blocked-successor")
        successor.update({"status": "queued", "checkpoint": "queued", "message": "",
            "repair_attempts": 0, "repair_history": [], "edits": None})
        legacy_prior = next(item for item in prior_features if item["project"] == "legacy")
        legacy_current = json.loads(json.dumps(legacy_prior))
        legacy_current.update({"status": "failed", "checkpoint": "review_1_rejected",
            "message": "old reviewer rejected incomplete cumulative evidence",
            "edits": [{"path": "tests/test_app.py", "before": sha256(before_test.encode()),
                "content": after_test}], "escalation_count": 1,
            "escalation_pending": False, "review_status": "rejected",
            "review_attempts": 1, "review_summary": "old incomplete review",
            "review_history": [{"attempt": 1, "packet_sha256": "4" * 64,
                "validation_evidence_sha256": "5" * 64, "outcome": "rejected",
                "decision_sha256": "6" * 64, "blocking_findings": [],
                "summary": "old incomplete review"}]})
        legacy_proposal_id = str(uuid.uuid4())
        legacy_current["escalation_proposal"] = {"proposal_id": legacy_proposal_id,
            "attempt": 1, "feature_id": legacy_current["id"],
            "feature_checkpoint": "repair_3_validation_failed",
            "binding_revision": 39, "model_target": "mac", "model": "mac-fixture",
            "chat_request_id": str(uuid.uuid4()), "chat_model_target": "windows",
            "chat_model": "windows-fixture", "diagnosis": "test is stale",
            "diagnosis_sha256": "7" * 64, "status": "failed", "summary": "applied",
            "error": None, "files": [{"path": "tests/test_app.py", "before": before_test,
                "after": after_test, "protected": True}], "protected_inputs": {},
            "applied_paths": ["tests/test_app.py"], "apply_request_id": str(uuid.uuid4())}
        legacy_current["escalation_history"] = [{"proposal_id": legacy_proposal_id,
            "attempt": 1, "model_target": "mac", "model": "mac-fixture",
            "chat_request_id": legacy_current["escalation_proposal"]["chat_request_id"],
            "diagnosis_sha256": "7" * 64, "outcome": "approved_to_apply",
            "proposal_sha256": "8" * 64, "summary": "approved"}]
        legacy_prior["checkpoint"] = "repair_3_validation_failed"
        legacy = {"revision": 40, "auto_run": False, "emergency_paused": False,
            "queue_v7": prior_features[:-1] + [legacy_current],
            "planning_sessions": []}
        v6_backup = {"revision": 39, "auto_run": False, "emergency_paused": False,
            "queue_v6": prior_features, "planning_sessions": []}
        with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
            database.execute("CREATE TABLE developer_state(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL)")
            database.execute("CREATE TABLE developer_state_v6_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL)")
            database.execute("INSERT INTO developer_state(id,state) VALUES(1,?)",
                (json.dumps(legacy),))
            database.execute("INSERT INTO developer_state_v6_backup(id,state) VALUES(1,?)",
                (json.dumps(v6_backup),))
            database.commit()

        output = (root / "runner.log").open("ab")
        process = None
        listen = free_port()
        token = ""

        def runner_command():
            return [binary, "--data-dir", str(data), "--workspace-root", str(projects),
                "--bind", f"127.0.0.1:{listen}", "--model-url",
                f"http://127.0.0.1:{mac.server_port}/v1", "--model", "mac-fixture",
                "--windows-model-url", f"http://127.0.0.1:{windows.server_port}/v1",
                "--windows-model", "windows-fixture"] + reviewer_arguments(root)

        def launch():
            nonlocal process, token
            process = subprocess.Popen(runner_command(), stdin=subprocess.DEVNULL,
                stdout=output, stderr=output)
            deadline = time.monotonic() + 20
            while not (data / "developer-token").exists() and time.monotonic() < deadline:
                time.sleep(.03)
            token = (data / "developer-token").read_text().strip()
            wait_status(lambda state: not state["running"] and not state["escalation_running"])

        def terminate():
            if process is not None and process.poll() is None:
                process.terminate()
                process.wait(timeout=8)

        def api(path="status", body=None, authenticated=True, timeout=8):
            request = urllib.request.Request(f"http://127.0.0.1:{listen}/{path}",
                data=None if body is None else json.dumps(body).encode(),
                headers={"Content-Type": "application/json", "Authorization":
                    "Bearer " + (token if authenticated else "invalid")})
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return json.load(response)

        def rejected(path, body=None, code=409, authenticated=True):
            try:
                api(path, body, authenticated)
            except urllib.error.HTTPError as error:
                assert error.code == code, (path, error.code, error.read())
                return
            raise AssertionError("Unexpected acceptance: " + path)

        def wait_status(predicate, timeout=30):
            deadline = time.monotonic() + timeout
            last = None
            while time.monotonic() < deadline:
                try:
                    last = api()
                    if predicate(last):
                        return last
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(.04)
            raise AssertionError(f"Timed out: {last}\n" + (root / "runner.log").read_text(errors="replace"))

        def escalation(feature_id):
            return api("repair/escalation?id=" + urllib.parse.quote(feature_id))

        def wait_escalation(feature_id, status, timeout=25):
            deadline = time.monotonic() + timeout
            last = None
            while time.monotonic() < deadline:
                last = escalation(feature_id)
                if last["status"] == status:
                    return last
                time.sleep(.04)
            raise AssertionError(f"Timed out waiting for {status}: {last}")

        def diagnose(project):
            request_id = str(uuid.uuid4())
            api("chat", {"project": project, "message": "Why did this feature fail?",
                "id": request_id, "attachments": [], "model_target": "windows"})
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                state = api("chat?project=" + urllib.parse.quote(project))
                if not state["running"]:
                    assert not state["error"], state
                    reply = state["messages"][-1]
                    assert reply["request_id"] == request_id
                    assert reply["model_target"] == "windows"
                    return request_id, reply["content_sha256"], reply["content"]
                time.sleep(.04)
            raise AssertionError("Chat diagnosis timed out")

        def prepare_body(feature_id, chat_id, diagnosis_sha, state=None, **changes):
            state = state or api()
            feature = next(item for item in state["queue"] if item["id"] == feature_id)
            body = {"action": "prepare", "feature_id": feature_id,
                "expected_revision": state["revision"],
                "expected_checkpoint": feature["checkpoint"], "model_target": "mac",
                "chat_request_id": chat_id, "diagnosis_sha256": diagnosis_sha}
            body.update(changes)
            return body

        def control(action, **values):
            return api("control", dict(action=action, **values))

        def reviewer_calls():
            evidence = root / "review-fixture/review-input-evidence.jsonl"
            if not evidence.exists():
                return []
            records = [json.loads(line) for line in evidence.read_text().splitlines()]
            return [record for record in records if record["kind"] == "review"]

        def durable_feature(feature_id):
            with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
                state = json.loads(database.execute(
                    "SELECT state FROM developer_state WHERE id=1").fetchone()[0])
            return next(item for item in state["queue_v10"] if item["id"] == feature_id)

        try:
            launch()
            initial = api()
            assert {item["project"]: item["repair_attempts"] for item in initial["queue"]} == {
                "primary": 3, "cancel": 3, "restart": 3,
                "validation-failure": 3, "review-rejection": 3,
                "blocked-successor": 0, "partial": 0, "legacy": 3}
            assert all(item["model_target"] == "windows" for item in initial["queue"])
            migrated_legacy = next(item for item in initial["queue"]
                if item["id"] == feature_ids["legacy"])
            assert migrated_legacy["checkpoint"] == "review_binding_changed"
            assert migrated_legacy["review_status"] == "interrupted"
            primary = feature_ids["primary"]
            rejected("repair/escalation?id=" + feature_ids["primary"], code=401,
                authenticated=False)
            rejected("repair/escalation", {"action": "prepare", "feature_id": primary,
                "expected_revision": initial["revision"],
                "expected_checkpoint": "repair_3_validation_failed",
                "model_target": "mac", "chat_request_id": str(uuid.uuid4()),
                "diagnosis_sha256": "0" * 64}, code=401, authenticated=False)
            chat_id, diagnosis_sha, diagnosis = diagnose("primary")
            current = api()
            body = prepare_body(primary, chat_id, diagnosis_sha, current)
            rejected("repair/escalation", dict(body, expected_revision=current["revision"] - 1))
            rejected("repair/escalation", dict(body, diagnosis_sha256="0" * 64))
            before_hashes = {path.name: sha256(path.read_bytes()) for path in
                (projects / "primary").rglob("*") if path.is_file()}
            preparing = api("repair/escalation", body)
            assert preparing["status"] == "preparing"
            ready = wait_escalation(primary, "ready")
            ready_state = api()
            assert ready["binding"]["revision"] == ready_state["revision"]
            assert ready["binding"]["checkpoint"] == "repair_3_validation_failed"
            assert ready["model_target"] == "mac" and ready["model"] == "mac-fixture"
            assert ready["chat_request_id"] == chat_id
            assert ready["chat_model_target"] == "windows"
            assert ready["diagnosis"] == diagnosis and ready["diagnosis_sha256"] == diagnosis_sha
            assert ready["files"] == [{"path": "tests/test_app.py", "before": before_test,
                "after": after_test, "protected": True}]
            assert {path.name: sha256(path.read_bytes()) for path in
                (projects / "primary").rglob("*") if path.is_file()} == before_hashes
            rejected("repair/escalation", prepare_body(primary, chat_id, diagnosis_sha))

            proposal_id = ready["proposal_id"]
            approve = {"action": "approve_and_apply", "feature_id": primary,
                "expected_revision": ready_state["revision"],
                "expected_checkpoint": ready["binding"]["checkpoint"],
                "proposal_id": proposal_id}
            rejected("repair/escalation", dict(approve,
                expected_revision=ready_state["revision"] - 1))
            (projects / "primary/tests/test_app.py").write_bytes(
                (before_test + "# owner drift\n").encode("utf-8"))
            rejected("repair/escalation", approve)
            (projects / "primary/tests/test_app.py").write_bytes(before_test.encode("utf-8"))

            refreshed = api()
            cancel = {"action": "cancel", "feature_id": primary,
                "expected_revision": refreshed["revision"],
                "expected_checkpoint": ready["binding"]["checkpoint"],
                "proposal_id": proposal_id}
            assert api("repair/escalation", cancel)["status"] == "cancelled"
            rejected("repair/escalation", cancel)
            second_body = prepare_body(primary, chat_id, diagnosis_sha)
            api("repair/escalation", second_body)
            second = wait_escalation(primary, "ready")
            assert second["proposal_id"] != proposal_id and second["count"] == 2
            second_state = api()
            rejected("repair/escalation", {"action": "approve_and_apply",
                "feature_id": primary, "expected_revision": second_state["revision"],
                "expected_checkpoint": second["binding"]["checkpoint"],
                "proposal_id": str(uuid.uuid4())})

            with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
                durable_ready = json.loads(database.execute(
                    "SELECT state FROM developer_state WHERE id=1").fetchone()[0])
            ready_feature = next(item for item in durable_ready["queue_v10"]
                if item["id"] == primary)
            approved_digest = next(item["proposal_sha256"] for item in
                reversed(ready_feature["escalation_history"])
                if item["proposal_id"] == second["proposal_id"] and item["outcome"] == "ready")
            apply_body = {"action": "approve_and_apply", "feature_id": primary,
                "expected_revision": second_state["revision"],
                "expected_checkpoint": second["binding"]["checkpoint"],
                "proposal_id": second["proposal_id"]}
            api("repair/escalation", apply_body)
            completed = wait_status(lambda state: not state["running"] and
                next(item for item in state["queue"] if item["id"] == primary)["status"] == "succeeded")
            completed_feature = next(item for item in completed["queue"] if item["id"] == primary)
            assert completed_feature["validation"] == validation
            assert completed_feature["repair_attempts"] == 3
            assert completed_feature["model_target"] == "windows"
            assert completed_feature["review_status"] == "approved"
            assert (projects / "primary/tests/test_app.py").read_bytes() == after_test.encode("utf-8")
            final_escalation = escalation(primary)
            assert final_escalation["status"] == "succeeded"
            rejected("repair/escalation", apply_body)
            rejected("control", {"action": "repair", "id": primary,
                "expected_attempts": 3})

            # Stop and Emergency Pause cancel slow proposal preparation without writes.
            cancel_id = feature_ids["cancel"]
            cancel_chat, cancel_sha, unused = diagnose("cancel")
            fixture["mode"] = "slow"; entered.clear(); release.clear()
            api("repair/escalation", prepare_body(cancel_id, cancel_chat, cancel_sha))
            assert entered.wait(5)
            control("stop")
            assert wait_escalation(cancel_id, "cancelled")["files"] == []
            release.set(); fixture["mode"] = "normal"
            wait_status(lambda state: not state["escalation_running"])

            fixture["mode"] = "slow"; entered.clear(); release.clear()
            api("repair/escalation", prepare_body(cancel_id, cancel_chat, cancel_sha))
            assert entered.wait(5)
            control("emergency")
            assert wait_escalation(cancel_id, "cancelled")["files"] == []
            release.set(); fixture["mode"] = "normal"
            paused = wait_status(lambda state: not state["escalation_running"])
            assert paused["emergency_paused"] is True
            control("clear_emergency")
            control("remove", id=cancel_id)

            # Restart turns a durable preparing record into an interrupted terminal record.
            restart_id = feature_ids["restart"]
            restart_chat, restart_sha, unused = diagnose("restart")
            fixture["mode"] = "slow"; entered.clear(); release.clear()
            api("repair/escalation", prepare_body(restart_id, restart_chat, restart_sha))
            assert entered.wait(5)
            terminate(); release.set(); fixture["mode"] = "normal"
            listen = free_port()
            launch()
            interrupted = escalation(restart_id)
            assert interrupted["status"] == "interrupted"
            assert not api()["escalation_running"]
            control("remove", id=restart_id)

            # Auto-run must stop after approved escalation bytes fail validation.
            # The reviewer is downstream of validation and must never see this packet.
            validation_id = feature_ids["validation-failure"]
            control("auto_run", enabled=True)
            validation_chat, validation_sha, unused = diagnose("validation-failure")
            api("repair/escalation", prepare_body(
                validation_id, validation_chat, validation_sha))
            validation_ready = wait_escalation(validation_id, "ready")
            validation_state = api()
            reviewer_count = len(reviewer_calls())
            api("repair/escalation", {"action": "approve_and_apply",
                "feature_id": validation_id,
                "expected_revision": validation_state["revision"],
                "expected_checkpoint": validation_ready["binding"]["checkpoint"],
                "proposal_id": validation_ready["proposal_id"]})
            validation_failed = wait_status(lambda state: not state["running"] and
                next(item for item in state["queue"] if item["id"] == validation_id)[
                    "status"] == "failed")
            failed_feature = next(item for item in validation_failed["queue"]
                if item["id"] == validation_id)
            successor = next(item for item in validation_failed["queue"]
                if item["id"] == feature_ids["blocked-successor"])
            assert failed_feature["review_attempts"] == 0
            assert failed_feature["review_status"] == "pending"
            assert escalation(validation_id)["status"] == "failed"
            assert len(reviewer_calls()) == reviewer_count
            assert successor["status"] == "queued" and successor["checkpoint"] == "queued"
            assert (projects / "blocked-successor/app.py").read_bytes() == b"VALUE = 1\n"

            validation_history_count = len(
                durable_feature(validation_id)["escalation_history"])
            terminate(); listen = free_port(); launch()
            validation_restarted = api()
            failed_feature = next(item for item in validation_restarted["queue"]
                if item["id"] == validation_id)
            successor = next(item for item in validation_restarted["queue"]
                if item["id"] == feature_ids["blocked-successor"])
            assert failed_feature["status"] == "failed" and failed_feature["review_attempts"] == 0
            assert escalation(validation_id)["status"] == "failed"
            assert len(durable_feature(validation_id)[
                "escalation_history"]) == validation_history_count
            assert len(reviewer_calls()) == reviewer_count
            assert successor["status"] == "queued"
            control("remove", id=validation_id)

            # A reviewer rejection after successful validation is terminal for the
            # explicitly approved escalation and cannot advance the queued successor.
            rejection_id = feature_ids["review-rejection"]
            rejection_chat, rejection_sha, unused = diagnose("review-rejection")
            api("repair/escalation", prepare_body(
                rejection_id, rejection_chat, rejection_sha))
            rejection_ready = wait_escalation(rejection_id, "ready")
            rejection_state = api()
            reviewer_count = len(reviewer_calls())
            api("repair/escalation", {"action": "approve_and_apply",
                "feature_id": rejection_id,
                "expected_revision": rejection_state["revision"],
                "expected_checkpoint": rejection_ready["binding"]["checkpoint"],
                "proposal_id": rejection_ready["proposal_id"]})
            review_failed = wait_status(lambda state: not state["running"] and
                next(item for item in state["queue"] if item["id"] == rejection_id)[
                    "review_status"] == "rejected")
            rejected_feature = next(item for item in review_failed["queue"]
                if item["id"] == rejection_id)
            successor = next(item for item in review_failed["queue"]
                if item["id"] == feature_ids["blocked-successor"])
            assert rejected_feature["status"] == "failed"
            assert rejected_feature["review_attempts"] == 1
            assert rejected_feature["repair_attempts"] == 3
            assert escalation(rejection_id)["status"] == "failed"
            assert len(reviewer_calls()) == reviewer_count + 1
            assert successor["status"] == "queued" and successor["checkpoint"] == "queued"

            rejection_history_count = len(
                durable_feature(rejection_id)["escalation_history"])
            reviewer_count = len(reviewer_calls())
            terminate(); listen = free_port(); launch()
            rejection_restarted = api()
            rejected_feature = next(item for item in rejection_restarted["queue"]
                if item["id"] == rejection_id)
            successor = next(item for item in rejection_restarted["queue"]
                if item["id"] == feature_ids["blocked-successor"])
            assert rejected_feature["status"] == "failed"
            assert rejected_feature["review_status"] == "rejected"
            assert rejected_feature["review_attempts"] == 1
            assert len(durable_feature(rejection_id)[
                "escalation_history"]) == rejection_history_count
            assert len(reviewer_calls()) == reviewer_count
            assert successor["status"] == "queued"
            control("auto_run", enabled=False)
            control("remove", id=rejection_id)
            control("remove", id=feature_ids["blocked-successor"])

            # A deterministic second-path failure proves partial application is
            # quarantined: recorded approved bytes remain, and no replay is allowed.
            partial_id = feature_ids["partial"]
            partial_chat, partial_sha, unused = diagnose("partial")
            api("repair/escalation", prepare_body(partial_id, partial_chat, partial_sha))
            partial_ready = wait_escalation(partial_id, "ready")
            assert [item["path"] for item in partial_ready["files"]] == [
                "app.py", "block/second.py"]
            partial_state = api()
            api("repair/escalation", {"action": "approve_and_apply",
                "feature_id": partial_id, "expected_revision": partial_state["revision"],
                "expected_checkpoint": partial_ready["binding"]["checkpoint"],
                "proposal_id": partial_ready["proposal_id"]})
            quarantined = wait_status(lambda state: not state["running"] and
                next(item for item in state["queue"] if item["id"] == partial_id)[
                    "checkpoint"].endswith("_apply_interrupted"))
            partial_feature = next(item for item in quarantined["queue"]
                if item["id"] == partial_id)
            assert partial_feature["status"] == "failed"
            assert (projects / "partial/app.py").read_bytes() == b"VALUE = 2\n"
            assert (projects / "partial/block").is_file()
            assert not (projects / "partial/block/second.py").exists()
            partial_binding = {"expected_feature_id": partial_id,
                "expected_model_target": partial_feature["model_target"],
                "expected_status": partial_feature["status"],
                "expected_checkpoint": partial_feature["checkpoint"]}
            rejected("control", dict(action="resume", **partial_binding))
            rejected("control", {"action": "repair", "id": partial_id,
                "expected_attempts": 0})

            with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
                durable = json.loads(database.execute(
                    "SELECT state FROM developer_state WHERE id=1").fetchone()[0])
                v7_backup = json.loads(database.execute(
                    "SELECT state FROM developer_state_v7_backup WHERE id=1").fetchone()[0])
            assert "queue_v10" in durable and "queue_v7" not in durable
            assert "queue_v7" in v7_backup and "queue_v10" not in v7_backup
            durable_primary = next(item for item in durable["queue_v10"]
                if item["id"] == primary)
            assert durable_primary["repair_attempts"] == 3
            assert len(durable_primary["repair_history"]) == 3
            assert durable_primary["validation"] == validation
            assert durable_primary["model_target"] == "windows"
            assert durable_primary["escalation_proposal"]["model_target"] == "mac"
            assert durable_primary["cumulative_evidence_version"] == 1
            assert [item["path"] for item in durable_primary["edits"]] == [
                "app.py", "tests/test_app.py"]
            review = durable_primary["review_history"][-1]
            expected_packet = {"schema_version": 1, "feature_id": primary,
                "project": "primary",
                "instruction": durable_primary["instruction"],
                "approved_plan_sha256": None, "approved_plan": None,
                "validation_command": validation,
                "validation_evidence_sha256": review["validation_evidence_sha256"],
                "provider_id": "openai.codex", "model_id": "gpt-5.6-sol",
                "reasoning_effort": "high",
                "files": [{"path": "app.py", "before_sha256": None,
                    "content_sha256": sha256(b"VALUE = 1\n"), "content": "VALUE = 1\n"},
                    {"path": "tests/test_app.py",
                    "before_sha256": sha256(before_test.encode()),
                    "content_sha256": sha256(after_test.encode()), "content": after_test}]}
            expected_packet_sha = sha256(json.dumps(expected_packet,
                separators=(",", ":")).encode())
            assert review["packet_sha256"] == expected_packet_sha
            final_evidence = [item for item in durable_primary["escalation_history"]
                if item["proposal_id"] == second["proposal_id"]]
            assert [item["outcome"] for item in final_evidence] == [
                "ready", "approved_to_apply", "succeeded"]
            assert all(item["proposal_sha256"] == approved_digest for item in final_evidence)
            durable_partial = next(item for item in durable["queue_v10"]
                if item["id"] == partial_id)
            assert durable_partial["escalation_pending"] is False
            assert durable_partial["edits"] == [{"path": "app.py", "content": "VALUE = 2\n",
                "before": sha256(b"VALUE = 1\n")}]
            assert durable_partial["checkpoint"].endswith("_apply_interrupted")
            assert durable_partial["escalation_proposal"]["status"] == "interrupted"
            assert durable_partial["escalation_proposal"]["applied_paths"] == ["app.py"]
            assert durable_partial["escalation_history"][-1]["outcome"] == "interrupted"
            durable_validation = next(item for item in durable["queue_v10"]
                if item["id"] == validation_id)
            assert durable_validation["status"] == "removed"
            assert durable_validation["review_attempts"] == 0
            assert durable_validation["escalation_history"][-1]["outcome"] == "failed"
            durable_rejection = next(item for item in durable["queue_v10"]
                if item["id"] == rejection_id)
            assert durable_rejection["status"] == "removed"
            assert durable_rejection["review_status"] == "rejected"
            assert [item["outcome"] for item in durable_rejection["review_history"]] == [
                "rejected"]
            assert durable_rejection["escalation_history"][-1]["outcome"] == "failed"
            durable_legacy = next(item for item in durable["queue_v10"]
                if item["id"] == feature_ids["legacy"])
            assert durable_legacy["cumulative_evidence_version"] == 1
            assert durable_legacy["checkpoint"] == "review_binding_changed"
            assert [item["path"] for item in durable_legacy["edits"]] == [
                "app.py", "tests/test_app.py"]
            print(json.dumps({"native_platform": sys.platform,
                "queue_v7_migrated_with_backup": True,
                "legacy_cumulative_review_evidence_recovered": True,
                "exact_chat_diagnosis_bound": True,
                "protected_test_requires_explicit_apply": True,
                "stale_state_file_digest_and_replay_rejected": True,
                "validation_and_old_repair_evidence_preserved": True,
                "fixed_reviewer_approved_exact_cumulative_result": True,
                "stop_and_emergency_cancel_preparation": True,
                "restart_interrupts_preparation": True,
                "validation_failure_skips_reviewer_and_blocks_auto_run": True,
                "review_rejection_blocks_auto_run": True,
                "terminal_escalation_failures_do_not_replay_after_restart": True,
                "partial_apply_is_durably_quarantined": True}))
        finally:
            release.set()
            terminate()
            output.close()
            for server in (windows, mac):
                server.shutdown()
                server.server_close()


if __name__ == "__main__":
    main()
