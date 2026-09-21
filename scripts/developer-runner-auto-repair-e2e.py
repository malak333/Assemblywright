#!/usr/bin/env python3
"""Native process/API/filesystem proof for Windows-owned Auto AI repair."""

from developer_planning_fixture import enqueue_with_plan
from developer_review_fixture import reviewer_arguments

import argparse
from contextlib import closing
import hashlib
import http.server
import json
from pathlib import Path
import re
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
    parser.add_argument("--binary", default=str(
        Path(__file__).resolve().parents[1] / "target/debug/assemblywright-developer"))
    binary = str(Path(parser.parse_args().binary).resolve())
    lock = threading.Lock()
    calls = []
    chat_completions = []
    entered = {name: threading.Event() for name in ("disable", "stop", "emergency", "restart")}
    released = {name: threading.Event() for name in entered}

    def generated_files(marker, phase, attempt):
        if phase == "initial":
            if marker == "source-test-config":
                return [
                    {"path": "app.py", "content": "VALUE = 0\n"},
                    {"path": "config.json", "content": '{"expected": 9}\n'},
                    {"path": "tests/test_app.py", "content":
                        "import json\nfrom pathlib import Path\nimport sys\n"
                        "sys.path.insert(0,str(Path(__file__).parents[1]))\nimport app\n"
                        "assert app.VALUE == json.loads(Path('config.json').read_text())['expected']\n"},
                ]
            return [
                {"path": "app.py", "content": "VALUE = 0\n"},
                {"path": "tests/test_app.py", "content":
                    "from pathlib import Path\nimport sys\n"
                    "sys.path.insert(0,str(Path(__file__).parents[1]))\nimport app\n"
                    "assert app.VALUE == 1\n"},
            ]
        if phase == "ordinary":
            return [{"path": "app.py", "content": f"VALUE = {attempt + 1}\n"}]
        if phase == "manual":
            # Real manual chat escalation: changed bytes that still fail the
            # immutable validation command, so the shared counter stays consumed
            # and a later automatic-repair opportunity remains on this feature.
            assert marker == "shared-limit", marker
            return [{"path": "app.py", "content": "VALUE = 5\n"}]
        if marker == "source-test-config":
            return [
                {"path": "app.py", "content": "VALUE = 7\n"},
                {"path": "config.json", "content": '{"expected": 7, "repaired": true}\n'},
                {"path": "tests/test_app.py", "content":
                    "import json\nfrom pathlib import Path\nimport sys\n"
                    "sys.path.insert(0,str(Path(__file__).parents[1]))\nimport app\n"
                    "config=json.loads(Path('config.json').read_text())\n"
                    "assert config['repaired'] and app.VALUE == config['expected'] == 7\n"},
            ]
        if marker == "review-rejection":
            value = 0 if attempt == 1 else 1
            return [
                {"path": "app.py", "content": f"VALUE = {value}\n"},
                {"path": "tests/test_app.py", "content":
                    "from pathlib import Path\nimport sys\n"
                    "sys.path.insert(0,str(Path(__file__).parents[1]))\nimport app\n"
                    f"assert app.VALUE == {value}\n"},
            ]
        if marker in ("cap-configured", "cap-100", "operational-hold"):
            return [{"path": "app.py", "content": "VALUE = 4\n"}]
        return [{"path": "app.py", "content": "VALUE = 1\n"}]

    class Model(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            prompt = request["messages"][1]["content"]
            marker = next(name for name in (
                "shared-limit", "source-test-config", "review-rejection", "cap-configured",
                "cap-100", "operational-hold",
                "enable-after-failure", "disable", "stop", "emergency", "restart",
                "validation-quarantine", "review-quarantine", "auto-run-first", "auto-run-second",
            ) if name in prompt)
            ordinary = re.search(r"Repair attempt: (\d+) of 3", prompt)
            automatic = re.search(r"Automatic escalation: (\d+) of (\d+)", prompt)
            manual = "Untrusted project-chat diagnosis from" in prompt
            phase = ("ordinary" if ordinary else "automatic" if automatic
                else "manual" if manual else "initial")
            attempt = int((ordinary or automatic).group(1)) if (ordinary or automatic) else 0
            with lock:
                calls.append({"marker": marker, "phase": phase, "attempt": attempt,
                    "validation": next((line for line in prompt.splitlines()
                        if "validation command:" in line.lower()), "")})
            if phase == "automatic" and marker in entered and attempt == 1:
                entered[marker].set()
                released[marker].wait(20)
            if marker == "operational-hold" and phase == "automatic":
                content = "{"
            else:
                files = generated_files(marker, phase, attempt)
                if marker in ("cap-configured", "cap-100") and phase == "automatic":
                    files = [{"path": "app.py", "content": "VALUE = 4\n"}]
                content = json.dumps({"summary": f"{marker} {phase} {attempt}", "files": files})
            body = json.dumps({"choices": [{"message": {"content": content},
                "finish_reason": "stop"}]}).encode()
            try:
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
                pass

        def log_message(self, *unused):
            pass

    class WindowsChatModel(http.server.BaseHTTPRequestHandler):
        """Deterministic project-chat diagnosis target for the manual escalation."""

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

        def do_GET(self):
            if self.path == "/props":
                return self.reply({"total_slots": 1, "modalities": {"vision": True},
                    "default_generation_settings": {"n_ctx": 262144}})
            self.reply({"error": "unexpected route"}, 404)

        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            if self.path == "/v1/chat/completions":
                with lock:
                    chat_completions.append((self.path, request["messages"]))
                return self.reply({"choices": [{"message": {"tool_calls": None,
                    "content": "The immutable validation command still fails because "
                        "app.VALUE is not 1, while the approved plan requires VALUE 1."},
                    "finish_reason": "stop"}]})
            if self.path == "/apply-template":
                return self.reply({"prompt": json.dumps(request["messages"])})
            if self.path == "/tokenize":
                return self.reply({"count": max(1, len(request["content"]) // 4)})
            self.reply({"error": "unexpected route"}, 404)

    model = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
    model.daemon_threads = True
    threading.Thread(target=model.serve_forever, daemon=True).start()
    windows = http.server.ThreadingHTTPServer(("127.0.0.1", 0), WindowsChatModel)
    windows.daemon_threads = True
    threading.Thread(target=windows.serve_forever, daemon=True).start()

    with tempfile.TemporaryDirectory(prefix="assemblywright-auto-repair-e2e-") as temp:
        root = Path(temp)
        data = root / "state"
        projects = root / "projects"
        data.mkdir()
        projects.mkdir()
        legacy = json.dumps({"revision": 4, "auto_run": False,
            "emergency_paused": False, "queue_v2": []}, separators=(",", ":"))
        with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
            database.execute("CREATE TABLE developer_state(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL)")
            database.execute("INSERT INTO developer_state(id,state) VALUES(1,?)", (legacy,))
            database.commit()

        process = None
        output = (root / "runner.log").open("ab")
        port = free_port()
        token = ""

        def command():
            return [binary, "--data-dir", str(data), "--workspace-root", str(projects),
                "--bind", f"127.0.0.1:{port}", "--model-url",
                f"http://127.0.0.1:{model.server_port}/v1", "--model", "fixture",
                "--windows-model-url", f"http://127.0.0.1:{windows.server_port}/v1",
                "--windows-model", "windows-fixture"] + reviewer_arguments(root)

        def api(path="status", body=None, timeout=8, credential=None):
            request = urllib.request.Request(f"http://127.0.0.1:{port}/{path}",
                data=None if body is None else json.dumps(body).encode(),
                headers={"Content-Type": "application/json", "Authorization":
                    "Bearer " + (token if credential is None else credential)})
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return json.load(response)

        def wait(predicate, timeout=35):
            deadline = time.monotonic() + timeout
            last = None
            while time.monotonic() < deadline:
                try:
                    last = api()
                    if predicate(last):
                        return last
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(.025)
            raise AssertionError(f"Timed out: {last}\n" + (root / "runner.log").read_text(errors="replace"))

        def launch():
            nonlocal process, token
            process = subprocess.Popen(command(), stdin=subprocess.DEVNULL, stdout=output, stderr=output)
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise AssertionError((root / "runner.log").read_text(errors="replace"))
                if (data / "developer-token").exists():
                    token = (data / "developer-token").read_text().strip()
                    try:
                        return api()
                    except (OSError, urllib.error.URLError):
                        pass
                time.sleep(.03)
            raise AssertionError("runner did not start")

        def terminate():
            nonlocal process
            if process is not None and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=8)
            process = None

        def restart():
            nonlocal port
            terminate()
            port = free_port()
            return launch()

        def rejected(path, body, codes=(400, 409, 422), credential=None):
            try:
                api(path, body, credential=credential)
            except urllib.error.HTTPError as error:
                assert error.code in codes, (error.code, error.read())
                return
            raise AssertionError(f"unexpected acceptance: {path} {body}")

        def policy(enabled, maximum, state=None):
            state = state or api()
            try:
                return api("auto-ai-repair", {"enabled": enabled,
                    "max_escalations": maximum, "expected_revision": state["revision"]})
            except urllib.error.HTTPError as error:
                raise AssertionError((enabled, maximum, error.code,
                    error.read().decode(errors="replace"))) from error

        def enqueue(marker, validation, instruction=None):
            feature_id = str(uuid.uuid4())
            state = enqueue_with_plan(f"http://127.0.0.1:{port}", token, {
                "id": feature_id, "project": marker,
                "instruction": instruction or f"Auto repair fixture {marker}",
                "validation": validation,
            })
            return feature_id, next(feature for feature in state["queue"] if feature["id"] == feature_id)

        def control(action, **values):
            if action in ("start", "resume") and "expected_feature_id" not in values:
                state = api()
                feature = next(feature for feature in state["queue"]
                    if feature["status"] not in ("succeeded", "removed"))
                values.update(expected_feature_id=feature["id"],
                    expected_model_target=feature["model_target"],
                    expected_status=feature["status"], expected_checkpoint=feature["checkpoint"])
            return api("control", {"action": action, **values})

        def start_feature(marker, validation, instruction=None):
            feature_id, queued = enqueue(marker, validation, instruction)
            control("start", expected_feature_id=feature_id,
                expected_model_target=queued["model_target"], expected_status=queued["status"],
                expected_checkpoint=queued["checkpoint"])
            return feature_id

        def feature(state, feature_id):
            return next(item for item in state["queue"] if item["id"] == feature_id)

        def durable_feature(feature_id):
            with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
                state = json.loads(database.execute(
                    "SELECT state FROM developer_state WHERE id=1").fetchone()[0])
            return next(item for item in state["queue_v11"] if item["id"] == feature_id)

        def reviewer_call_count():
            evidence = root / "review-fixture/review-input-evidence.jsonl"
            if not evidence.exists():
                return 0
            return sum(json.loads(line).get("kind") == "review"
                for line in evidence.read_text().splitlines())

        def remove(feature_id):
            control("remove", id=feature_id)

        validation = f'"{sys.executable}" -B tests/test_app.py'
        try:
            initial = launch()
            assert initial["auto_ai_repair_enabled"] is False
            assert initial["auto_ai_repair_max_escalations"] == 100
            assert initial["auto_ai_repair_policy_revision"] == 0
            rejected("auto-ai-repair", {"enabled": True, "max_escalations": 3,
                "expected_revision": initial["revision"]}, codes=(401,), credential="invalid")
            for invalid in (0, 101):
                rejected("auto-ai-repair", {"enabled": False, "max_escalations": invalid,
                    "expected_revision": initial["revision"]})
            changed = policy(False, 4, initial)
            assert changed["revision"] == initial["revision"] + 1
            assert changed["auto_ai_repair_max_escalations"] == 4
            replayed = api("auto-ai-repair", {"enabled": False, "max_escalations": 4,
                "expected_revision": initial["revision"]})
            assert replayed["revision"] == changed["revision"]
            rejected("auto-ai-repair", {"enabled": False, "max_escalations": 5,
                "expected_revision": initial["revision"]})
            enable_id = start_feature("enable-after-failure", validation)
            failed_before_enable = wait(lambda state: not state["running"] and
                feature(state, enable_id)["status"] == "failed")
            failed_feature = feature(failed_before_enable, enable_id)
            assert failed_feature["repair_attempts"] == 0
            assert failed_feature["escalation_count"] == 0
            policy(True, 4, failed_before_enable)
            enabled_result = wait(lambda state: not state["running"] and
                feature(state, enable_id)["status"] == "succeeded", 50)
            enabled_feature = feature(enabled_result, enable_id)
            assert enabled_feature["repair_attempts"] == 3
            assert enabled_feature["escalation_count"] == 1
            assert enabled_feature["auto_ai_repair_limit"] == 4
            assert [call["phase"] for call in calls
                if call["marker"] == "enable-after-failure"] == [
                    "initial", "ordinary", "ordinary", "ordinary", "automatic"]
            assert any(item["outcome"] == "policy_authorized"
                for item in durable_feature(enable_id)["escalation_history"])

            success_id = start_feature("source-test-config", validation)
            success = wait(lambda state: not state["running"] and (
                feature(state, success_id)["status"] == "succeeded" or
                feature(state, success_id)["auto_repair_lifecycle"] == "held"), 50)
            assert feature(success, success_id)["status"] == "succeeded", api(
                "repair/escalation?id=" + success_id)
            succeeded = feature(success, success_id)
            assert succeeded["repair_attempts"] == 3 and succeeded["escalation_count"] == 1
            assert succeeded["auto_ai_repair_limit"] == 4
            assert succeeded["validation"] == validation
            assert (projects / "source-test-config/app.py").read_text() == "VALUE = 7\n"
            assert json.loads((projects / "source-test-config/config.json").read_text()) == {
                "expected": 7, "repaired": True}
            assert "config['repaired']" in (projects / "source-test-config/tests/test_app.py").read_text()
            scenario_calls = [call for call in calls if call["marker"] == "source-test-config"]
            assert [call["phase"] for call in scenario_calls] == [
                "initial", "ordinary", "ordinary", "ordinary", "automatic"]
            assert all(validation in call["validation"] for call in scenario_calls)

            review_id = start_feature("review-rejection", validation,
                "Implement VALUE 1 [fixture:reject-zero] review-rejection")
            reviewed = wait(lambda state: not state["running"] and
                feature(state, review_id)["status"] == "succeeded", 50)
            reviewed_feature = feature(reviewed, review_id)
            assert reviewed_feature["repair_attempts"] == 3
            assert reviewed_feature["escalation_count"] == 2
            assert reviewed_feature["review_attempts"] == 2
            assert reviewed_feature["review_status"] == "approved"
            assert any(item["outcome"] == "rejected"
                for item in durable_feature(review_id)["review_history"])

            policy(False, 4)
            policy(True, 2)
            configured_cap_id = start_feature("cap-configured", validation)
            configured_capped = wait(lambda state: not state["running"] and
                feature(state, configured_cap_id)["auto_repair_lifecycle"] == "limit_reached", 50)
            configured_cap_feature = feature(configured_capped, configured_cap_id)
            assert configured_cap_feature["escalation_count"] == 2
            assert configured_cap_feature["auto_ai_repair_limit"] == 2
            assert len([call for call in calls if call["marker"] == "cap-configured"
                and call["phase"] == "automatic"]) == 2
            remove(configured_cap_id)

            policy(False, 2)
            cap_enabled = policy(True, 100)
            assert cap_enabled["auto_ai_repair_max_escalations"] == 100
            cap_id = start_feature("cap-100", validation)
            capped = wait(lambda state: not state["running"] and
                feature(state, cap_id)["auto_repair_lifecycle"] == "limit_reached", 90)
            cap_feature = feature(capped, cap_id)
            assert cap_feature["repair_attempts"] == 3
            assert cap_feature["escalation_count"] == 100
            assert cap_feature["status"] == "failed"
            assert len([call for call in calls if call["marker"] == "cap-100"
                and call["phase"] == "automatic"]) == 100
            cap_bytes = (projects / "cap-100/app.py").read_bytes()
            policy(False, 100, capped)
            policy(True, 100)
            time.sleep(.25)
            cap_after_reenable = api()
            assert not cap_after_reenable["running"]
            assert feature(cap_after_reenable, cap_id)["auto_repair_lifecycle"] == "limit_reached"
            assert feature(cap_after_reenable, cap_id)["escalation_count"] == 100
            assert (projects / "cap-100/app.py").read_bytes() == cap_bytes
            assert len([call for call in calls if call["marker"] == "cap-100"
                and call["phase"] == "automatic"]) == 100
            restarted = restart()
            restarted_cap = feature(restarted, cap_id)
            assert restarted_cap["auto_repair_lifecycle"] == "limit_reached"
            assert restarted_cap["escalation_count"] == 100
            remove(cap_id)

            policy(False, 100)
            policy(True, 3)
            operational_id = start_feature("operational-hold", validation)
            operational = wait(lambda state: not state["running"] and
                feature(state, operational_id)["auto_repair_lifecycle"] == "held", 50)
            assert feature(operational, operational_id)["escalation_count"] == 1
            assert "could not produce" in feature(operational, operational_id)["auto_repair_reason"]
            remove(operational_id)

            disable_id = start_feature("disable", validation)
            assert entered["disable"].wait(30)
            before_disable = api()
            disabled = policy(False, 3, before_disable)
            released["disable"].set()
            disabled = wait(lambda state: not state["running"] and
                feature(state, disable_id)["auto_repair_lifecycle"] == "inactive", 30)
            disabled_feature = feature(disabled, disable_id)
            assert disabled_feature["status"] == "paused", disabled_feature
            assert disabled_feature["escalation_status"] == "cancelled"
            assert (projects / "disable/app.py").read_text() == "VALUE = 4\n"
            assert disabled_feature["escalation_count"] == 1
            remove(disable_id)

            policy(True, 3)
            stop_id = start_feature("stop", validation)
            assert entered["stop"].wait(30)
            control("stop")
            released["stop"].set()
            stopped = wait(lambda state: not state["running"] and
                feature(state, stop_id)["status"] == "paused", 30)
            assert feature(stopped, stop_id)["escalation_count"] == 1
            assert feature(stopped, stop_id)["escalation_status"] == "cancelled"
            assert (projects / "stop/app.py").read_text() == "VALUE = 4\n"
            policy(False, 3)
            remove(stop_id)

            policy(True, 3)
            emergency_id = start_feature("emergency", validation)
            assert entered["emergency"].wait(30)
            emergency_before = feature(api(), emergency_id)
            emergency_epoch = emergency_before["auto_repair_epoch"]
            emergency_bytes = (projects / "emergency/app.py").read_bytes()
            control("emergency")
            released["emergency"].set()
            emergency_stopped = wait(lambda state: not state["running"] and
                state["emergency_paused"] and feature(state, emergency_id)["status"] == "failed" and
                feature(state, emergency_id)["auto_repair_lifecycle"] == "inactive", 30)
            emergency_feature = feature(emergency_stopped, emergency_id)
            assert emergency_feature["auto_repair_epoch"] == emergency_epoch + 1
            assert emergency_feature["escalation_count"] == 1
            assert emergency_feature["escalation_status"] == "cancelled"
            assert (projects / "emergency/app.py").read_bytes() == emergency_bytes
            time.sleep(.25)
            assert len([call for call in calls if call["marker"] == "emergency"
                and call["phase"] == "automatic"]) == 1
            assert [call["phase"] for call in calls if call["marker"] == "emergency"] == [
                "initial", "ordinary", "ordinary", "ordinary", "automatic"]
            # The late automatic proposal result must leave the emergency
            # terminal projection exactly as the cancellation intent recorded it.
            emergency_after_late_output = feature(api(), emergency_id)
            assert emergency_after_late_output["status"] == "failed"
            assert emergency_after_late_output["auto_repair_lifecycle"] == "inactive"
            assert emergency_after_late_output["escalation_status"] == "cancelled"
            assert emergency_after_late_output["escalation_count"] == 1
            assert emergency_after_late_output["auto_repair_epoch"] == emergency_epoch + 1
            assert (projects / "emergency/app.py").read_bytes() == emergency_bytes
            control("clear_emergency")
            policy(False, 3)
            remove(emergency_id)

            policy(True, 3)
            restart_id = start_feature("restart", validation)
            assert entered["restart"].wait(30)
            restart()
            released["restart"].set()
            recovered = wait(lambda state: not state["running"] and
                feature(state, restart_id)["status"] == "succeeded", 50)
            recovered_feature = feature(recovered, restart_id)
            assert recovered_feature["escalation_count"] == 2
            durable_restarted = durable_feature(restart_id)
            interrupted_proposals = [item for item in durable_restarted["escalation_history"]
                if item["outcome"] == "proposal_interrupted"]
            assert len(interrupted_proposals) == 1
            interrupted_proposal_id = interrupted_proposals[0]["proposal_id"]
            assert [item["outcome"] for item in durable_restarted["escalation_history"]
                if item["proposal_id"] == interrupted_proposal_id] == [
                    "proposal_interrupted", "authorization_not_run", "application_not_run"]
            assert all(item.get("candidate_sha256") is None
                for item in durable_restarted["escalation_history"]
                if item["proposal_id"] == interrupted_proposal_id)
            interrupted_reviews = [item for item in durable_restarted["review_history"]
                if item["outcome"] == "not_run" and
                "proposal generation was interrupted" in item["summary"]]
            assert len(interrupted_reviews) == 1

            sleep_validation = (
                f'"{sys.executable}" -B -c "from pathlib import Path; import app,time; '
                "Path('validation-count').open('a').write('x'); "
                'time.sleep(20) if app.VALUE == 1 else None; assert app.VALUE == 1"'
            )
            validation_id = start_feature("validation-quarantine", sleep_validation)
            validation_counter = projects / "validation-quarantine/validation-count"
            wait(lambda state: state["running"] and
                feature(state, validation_id)["checkpoint"] == "escalation_1_applied" and
                validation_counter.exists() and len(validation_counter.read_text()) >= 5, 50)
            validation_count_before = validation_counter.read_text()
            validation_restarted = restart()
            validation_stopped = wait(lambda state:
                feature(state, validation_id)["auto_repair_lifecycle"] == "quarantined", 30)
            assert feature(validation_restarted, validation_id)["status"] == "failed"
            assert feature(validation_stopped, validation_id)["status"] == "failed"
            assert feature(validation_stopped, validation_id)[
                "auto_repair_lifecycle"] == "quarantined"
            quarantine_calls = len([call for call in calls
                if call["marker"] == "validation-quarantine"])
            quarantine_bytes = (projects / "validation-quarantine/app.py").read_bytes()
            quarantine_count = feature(validation_stopped, validation_id)["escalation_count"]
            policy(False, 3)
            policy(True, 3)
            time.sleep(.25)
            assert validation_counter.read_text() == validation_count_before
            quarantine_after_reenable = api()
            assert not quarantine_after_reenable["running"]
            assert feature(quarantine_after_reenable, validation_id)["auto_repair_lifecycle"] == "quarantined"
            assert feature(quarantine_after_reenable, validation_id)["escalation_count"] == quarantine_count
            assert (projects / "validation-quarantine/app.py").read_bytes() == quarantine_bytes
            assert len([call for call in calls
                if call["marker"] == "validation-quarantine"]) == quarantine_calls
            policy(False, 3)
            remove(validation_id)

            policy(True, 3)
            reviewer_calls_baseline = reviewer_call_count()
            review_quarantine_id = start_feature("review-quarantine", validation,
                "Implement VALUE 1 [fixture:wait] review-quarantine")
            reviewing = wait(lambda state: state["running"] and
                feature(state, review_quarantine_id)["review_status"] == "reviewing" and
                feature(state, review_quarantine_id)["checkpoint"].endswith("_pending"), 50)
            wait(lambda _state: reviewer_call_count() > reviewer_calls_baseline)
            review_attempts_before = feature(reviewing, review_quarantine_id)["review_attempts"]
            reviewer_calls_before = reviewer_call_count()
            review_restarted = restart()
            review_stopped = wait(lambda state: not state["running"] and
                feature(state, review_quarantine_id)["auto_repair_lifecycle"] == "quarantined", 30)
            assert feature(review_restarted, review_quarantine_id)["status"] == "failed"
            assert feature(review_stopped, review_quarantine_id)["review_status"] == "interrupted"
            assert feature(review_stopped, review_quarantine_id)["review_attempts"] == review_attempts_before
            review_quarantine_calls = len([call for call in calls
                if call["marker"] == "review-quarantine"])
            review_quarantine_bytes = (
                projects / "review-quarantine/app.py").read_bytes()
            review_quarantine_count = feature(
                review_stopped, review_quarantine_id)["escalation_count"]
            policy(False, 3)
            policy(True, 3)
            time.sleep(.25)
            assert reviewer_call_count() == reviewer_calls_before
            review_after_reenable = api()
            review_feature_after_reenable = feature(
                review_after_reenable, review_quarantine_id)
            assert not review_after_reenable["running"]
            assert review_feature_after_reenable["auto_repair_lifecycle"] == "quarantined"
            assert review_feature_after_reenable["escalation_count"] == review_quarantine_count
            assert (projects / "review-quarantine/app.py").read_bytes() == review_quarantine_bytes
            assert len([call for call in calls
                if call["marker"] == "review-quarantine"]) == review_quarantine_calls
            policy(False, 3)
            remove(review_quarantine_id)

            # Shared-limit proof: one real manual chat repair escalation consumes
            # the same feature escalation counter that a later automatic repair
            # opportunity must honor, the shared maximum then stops that
            # opportunity at limit_reached without any repair-model call, and the
            # existing validation-only Resume route recovers owner-corrected
            # bytes through fresh validation and one fresh independent review.
            policy(False, 1)
            shared_validation_counter = root / "shared-limit-validation-runs"
            shared_validation = (
                f'"{sys.executable}" -B -c "from pathlib import Path; import app; '
                f"Path({str(shared_validation_counter)!r}).open('a').write('x'); "
                'assert app.VALUE == 1"'
            )
            shared_id = start_feature("shared-limit", shared_validation)
            shared_failed = wait(lambda state: not state["running"] and
                feature(state, shared_id)["status"] == "failed")
            shared_failed_feature = feature(shared_failed, shared_id)
            assert shared_failed_feature["repair_attempts"] == 0
            assert shared_failed_feature["escalation_count"] == 0
            assert shared_failed_feature["auto_ai_repair_limit"] == 1
            assert shared_failed_feature["last_failure_kind"] == "validation_failure"
            assert [call["phase"] for call in calls
                if call["marker"] == "shared-limit"] == ["initial"]
            assert shared_validation_counter.read_text() == "x"

            chat_request = str(uuid.uuid4())
            api("chat", {"project": "shared-limit",
                "message": "Why does the immutable validation command still fail?",
                "id": chat_request, "attachments": [], "model_target": "windows"})
            deadline = time.monotonic() + 20
            while True:
                chat_state = api("chat?project=" + urllib.parse.quote("shared-limit"))
                if not chat_state["running"]:
                    break
                assert time.monotonic() < deadline, chat_state
                time.sleep(.04)
            assert not chat_state["error"], chat_state
            diagnosis = chat_state["messages"][-1]
            assert diagnosis["request_id"] == chat_request
            assert diagnosis["model_target"] == "windows"
            assert len(chat_completions) == 1
            reviewer_calls_before_manual = reviewer_call_count()
            shared_state = api()
            prepared = api("repair/escalation", {
                "action": "prepare", "feature_id": shared_id,
                "expected_revision": shared_state["revision"],
                "expected_checkpoint": feature(shared_state, shared_id)["checkpoint"],
                "model_target": "mac", "chat_request_id": chat_request,
                "diagnosis_sha256": diagnosis["content_sha256"]})
            assert prepared["status"] == "preparing"
            deadline = time.monotonic() + 25
            while True:
                manual = api("repair/escalation?id=" + urllib.parse.quote(shared_id))
                if manual["status"] == "ready":
                    break
                assert time.monotonic() < deadline, manual
                time.sleep(.04)
            assert manual["count"] == 1
            assert manual["model_target"] == "mac" and manual["model"] == "fixture"
            assert manual["chat_request_id"] == chat_request
            assert manual["diagnosis_sha256"] == diagnosis["content_sha256"]
            assert manual["diagnosis"] == diagnosis["content"]
            assert [item["path"] for item in manual["files"]] == ["app.py"]
            assert manual["files"][0]["before"] == "VALUE = 0\n"
            assert manual["files"][0]["after"] == "VALUE = 5\n"
            assert manual["files"][0]["protected"] is False
            assert len([call for call in calls
                if call["marker"] == "shared-limit"]) == 2
            apply_state = api()
            api("repair/escalation", {"action": "approve_and_apply",
                "feature_id": shared_id,
                "expected_revision": apply_state["revision"],
                "expected_checkpoint": manual["binding"]["checkpoint"],
                "proposal_id": manual["proposal_id"]})
            manual_applied = wait(lambda state: not state["running"] and
                feature(state, shared_id)["status"] == "failed" and
                feature(state, shared_id)["escalation_status"] == "failed")
            manual_feature = feature(manual_applied, shared_id)
            assert manual_feature["repair_attempts"] == 0
            assert manual_feature["escalation_count"] == 1
            assert manual_feature["review_attempts"] == 0
            assert manual_feature["review_status"] == "pending"
            assert manual_feature["auto_ai_repair_limit"] == 1
            assert (projects / "shared-limit/app.py").read_text() == "VALUE = 5\n"
            assert shared_validation_counter.read_text() == "xx"
            assert len([call for call in calls
                if call["marker"] == "shared-limit"]) == 2
            assert reviewer_call_count() == reviewer_calls_before_manual
            durable_manual = durable_feature(shared_id)
            assert [item["outcome"] for item in durable_manual["escalation_history"]] == [
                "ready", "approved_to_apply", "failed"]
            assert all(item["source"] == "manual_chat"
                for item in durable_manual["escalation_history"])
            assert all(item["limit_snapshot"] == 1
                for item in durable_manual["escalation_history"])
            assert durable_manual["escalation_proposal"]["status"] == "failed"
            assert durable_manual["escalation_proposal"]["attempt"] == 1
            assert durable_manual["escalation_proposal"]["source"] == "manual_chat"
            assert durable_manual["escalation_proposal"]["applied_paths"] == ["app.py"]
            assert durable_manual["auto_ai_repair_limit"] == 1
            assert durable_manual["escalation_count"] == 1
            assert [item["outcome"] for item in durable_manual["review_history"]] == [
                "not_run"]

            # The same shared maximum arms the later automatic-repair
            # opportunity, which must stop at limit_reached with no additional
            # repair-model call and no reviewer call.
            reviewer_calls_before_limit = reviewer_call_count()
            policy(True, 1)
            shared_capped = wait(lambda state: not state["running"] and
                feature(state, shared_id)["auto_repair_lifecycle"] == "limit_reached", 60)
            capped_feature = feature(shared_capped, shared_id)
            assert capped_feature["status"] == "failed"
            assert capped_feature["repair_attempts"] == 3
            assert capped_feature["escalation_count"] == 1
            assert capped_feature["auto_ai_repair_limit"] == 1
            assert capped_feature["escalation_status"] == "failed"
            assert capped_feature["auto_repair_reason"].startswith(
                "1 of 1 AI escalations used")
            assert capped_feature["can_revalidate_after_auto_repair_limit"] is True
            assert [call["phase"] for call in calls
                if call["marker"] == "shared-limit"] == [
                    "initial", "manual", "ordinary", "ordinary", "ordinary"]
            assert not [call for call in calls
                if call["marker"] == "shared-limit" and call["phase"] == "automatic"]
            assert shared_validation_counter.read_text() == "xxxxx"
            assert reviewer_call_count() == reviewer_calls_before_limit
            durable_capped = durable_feature(shared_id)
            assert durable_capped["auto_repair_lifecycle"] == "limit_reached"
            assert durable_capped["escalation_count"] == 1
            assert durable_capped["auto_ai_repair_limit"] == 1
            assert len(durable_capped["escalation_history"]) == 3
            assert durable_capped["escalation_proposal"]["status"] == "failed"
            assert durable_capped["escalation_proposal"]["source"] == "manual_chat"
            assert durable_capped["auto_repair_limit_project_baseline"][
                "app.py"] == sha256(b"VALUE = 4\n")

            # Owner correction without AI, then the existing validation-only
            # Resume route at limit_reached: fresh validation run, one fresh
            # independent review, success, and no repair-model call.
            (projects / "shared-limit/app.py").write_bytes(b"VALUE = 1\n")
            control("resume", expected_feature_id=shared_id,
                expected_model_target=capped_feature["model_target"],
                expected_status=capped_feature["status"],
                expected_checkpoint=capped_feature["checkpoint"])
            shared_recovered = wait(lambda state: not state["running"] and
                feature(state, shared_id)["status"] == "succeeded", 50)
            recovered_feature = feature(shared_recovered, shared_id)
            assert recovered_feature["escalation_count"] == 1
            assert recovered_feature["auto_ai_repair_limit"] == 1
            assert recovered_feature["review_attempts"] == 1
            assert recovered_feature["review_status"] == "approved"
            assert recovered_feature["can_revalidate_after_auto_repair_limit"] is False
            assert (projects / "shared-limit/app.py").read_bytes() == b"VALUE = 1\n"
            assert shared_validation_counter.read_text() == "xxxxxx"
            assert [call["phase"] for call in calls
                if call["marker"] == "shared-limit"] == [
                    "initial", "manual", "ordinary", "ordinary", "ordinary"]
            assert reviewer_call_count() == reviewer_calls_before_limit + 1
            durable_recovered = durable_feature(shared_id)
            assert durable_recovered["auto_repair_lifecycle"] == "inactive"
            assert durable_recovered["escalation_count"] == 1
            assert durable_recovered["auto_ai_repair_limit"] == 1
            assert [item["outcome"] for item in durable_recovered["review_history"]] == [
                "not_run", "approved"]
            recovered_review = durable_recovered["review_history"][-1]
            assert recovered_review["decision_sha256"]
            assert recovered_review["validation_evidence_sha256"] != \
                durable_recovered["review_history"][0]["validation_evidence_sha256"]
            assert recovered_review["packet_sha256"] != \
                durable_recovered["review_history"][0]["packet_sha256"]
            assert {item["path"]: item["content"]
                for item in durable_recovered["edits"]}["app.py"] == "VALUE = 1\n"
            policy(False, 1)

            policy(True, 3)
            control("auto_run", enabled=True)
            first_auto_id, first_auto = enqueue("auto-run-first", validation)
            second_auto_id, _ = enqueue("auto-run-second", validation)
            control("start", expected_feature_id=first_auto_id,
                expected_model_target=first_auto["model_target"],
                expected_status=first_auto["status"], expected_checkpoint=first_auto["checkpoint"])
            auto_run_completed = wait(lambda state: not state["running"] and
                feature(state, first_auto_id)["status"] == "succeeded" and
                feature(state, second_auto_id)["status"] == "succeeded", 75)
            for marker, feature_id in (("auto-run-first", first_auto_id),
                    ("auto-run-second", second_auto_id)):
                completed_feature = feature(auto_run_completed, feature_id)
                assert completed_feature["repair_attempts"] == 3
                assert completed_feature["escalation_count"] == 1
                assert completed_feature["auto_ai_repair_limit"] == 3
                assert [call["phase"] for call in calls if call["marker"] == marker] == [
                    "initial", "ordinary", "ordinary", "ordinary", "automatic"]
                durable_completed = durable_feature(feature_id)
                assert durable_completed["escalation_proposal"]["feature_id"] == feature_id
            control("auto_run", enabled=False)

            with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
                durable = json.loads(database.execute(
                    "SELECT state FROM developer_state WHERE id=1").fetchone()[0])
                backup = database.execute(
                    "SELECT state FROM developer_state_v2_backup WHERE id=1").fetchone()[0]
            assert backup == legacy
            assert "queue_v11" in durable and "queue_v2" not in durable
            print(json.dumps({
                "native_platform": sys.platform,
                "migration_defaults_and_atomic_control": True,
                "enable_after_existing_failure_starts_authorized_repair": True,
                "ordinary_three_then_automatic_escalation": True,
                "source_test_config_and_immutable_validation": True,
                "review_rejection_continues": True,
                "configured_cap_stops_without_extra_call": True,
                "no_op_continues_to_absolute_cap_without_101st_call": True,
                "app_independent_polling": True,
                "operational_failure_holds": True,
                "disable_and_stop_reject_late_proposal_results": True,
                "emergency_rejects_late_proposal_without_replay": True,
                "clean_pre_effect_restart_continues_with_next_attempt": True,
                "restart_quarantines_applied_validation_without_replay": True,
                "restart_quarantines_pending_review_without_replay": True,
                "quarantine_and_limit_reenable_do_not_restart": True,
                "manual_chat_escalation_consumes_the_shared_feature_counter": True,
                "shared_cap_blocks_every_later_automatic_repair_model_call": True,
                "limit_recovery_revalidates_owner_bytes_without_repair_model": True,
                "limit_recovery_produces_fresh_validation_and_independent_review": True,
                "auto_run_arms_and_repairs_each_feature_independently": True,
            }, sort_keys=True))
        finally:
            for event in released.values():
                event.set()
            try:
                if process is not None and process.poll() is None:
                    control("emergency")
                    wait(lambda state: not state["running"], 5)
            except Exception:
                pass
            terminate()
            output.close()
    model.shutdown()
    windows.shutdown()


if __name__ == "__main__":
    main()
