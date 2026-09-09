#!/usr/bin/env python3
"""Native process/HTTP proof for mandatory developer brainstorming and plan binding."""

from developer_planning_fixture import enqueue_with_plan
from developer_review_fixture import reviewer_arguments

import argparse
from contextlib import closing
import hashlib
import http.server
import json
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    args = parser.parse_args()
    model_prompts = []

    class Model(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            prompt = request["messages"][1]["content"]
            model_prompts.append(prompt)
            value = 1 if "Repair attempt:" in prompt else 0
            content = json.dumps({"files": [{"path": "app.py", "content": f"VALUE = {value}\n"}]})
            body = json.dumps({"choices": [{"message": {"content": content}, "finish_reason": "stop"}]}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *unused):
            pass

    model = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
    threading.Thread(target=model.serve_forever, daemon=True).start()
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]

    with tempfile.TemporaryDirectory(prefix="assemblywright-developer-planning-e2e-") as temp:
        root = Path(temp)
        data, projects = root / "state", root / "projects"
        data.mkdir(); projects.mkdir()

        def command(port_value):
            return [str(Path(args.binary).resolve()), "--data-dir", str(data),
                    "--workspace-root", str(projects), "--bind", f"127.0.0.1:{port_value}",
                    "--model-url", f"http://127.0.0.1:{model.server_port}/v1"] + reviewer_arguments(root)

        output = (root / "runner.log").open("ab")
        process = subprocess.Popen(command(port), stdin=subprocess.DEVNULL, stdout=output, stderr=output)
        token = ""

        def api(path, body=None, timeout=10):
            request = urllib.request.Request(f"http://127.0.0.1:{port}/{path}",
                data=json.dumps(body).encode() if body is not None else None,
                headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
            return json.load(urllib.request.urlopen(request, timeout=timeout))

        def rejected(path, body):
            try:
                api(path, body)
                raise AssertionError("mutation unexpectedly succeeded: " + json.dumps(body))
            except urllib.error.HTTPError as error:
                assert error.code == 409, error.code

        def wait(predicate, timeout=45):
            deadline, value = time.monotonic() + timeout, None
            while time.monotonic() < deadline:
                try:
                    value = api("status")
                    if predicate(value): return value
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(.05)
            raise AssertionError("Timed out: " + json.dumps(value) + "\n" + (root / "runner.log").read_text(errors="replace"))

        try:
            deadline = time.monotonic() + 15
            while not (data / "developer-token").exists() and time.monotonic() < deadline: time.sleep(.05)
            token = (data / "developer-token").read_text().strip()
            initial = wait(lambda value: not value["running"])
            assert initial["planning_required"] is True
            assert initial["planning_provider"] == "openai.codex"
            assert initial["planning_model"] == "gpt-5.6-sol"

            raw_id = str(uuid.uuid4())
            rejected("control", {"action":"enqueue", "id":raw_id, "project":"raw",
                "instruction":"Bypass planning", "validation":"true"})

            waiting_id, waiting_request = str(uuid.uuid4()), str(uuid.uuid4())
            waiting = {"action":"start", "feature_id":waiting_id, "request_id":waiting_request,
                "expected_revision":0, "project":"waiting", "instruction":"Wait [planning:wait]",
                "validation":"true", "model_target":"mac"}
            accepted = api("planning", waiting)
            assert accepted["running"] is True
            replay = api("planning", waiting)
            assert replay["revision"] == accepted["revision"]
            changed = dict(waiting, instruction="Changed [planning:wait]")
            rejected("planning", changed)
            api("control", {"action":"emergency"})
            stopped = wait(lambda value: not value["planning_running"])
            interrupted = api("planning?id=" + waiting_id)
            assert interrupted["availability"] == "unavailable" and not interrupted["running"]
            api("control", {"action":"clear_emergency"})

            restart_id = str(uuid.uuid4())
            marker = root / "review-fixture/planning-started.pid"
            marker.unlink(missing_ok=True)
            restart_request = {"action":"start", "feature_id":restart_id, "request_id":str(uuid.uuid4()),
                "expected_revision":0, "project":"restart", "instruction":"Restart [planning:wait]",
                "validation":"true", "model_target":"mac"}
            restart_started = api("planning", restart_request)
            assert restart_started["running"]
            deadline = time.monotonic() + 10
            while not marker.exists() and time.monotonic() < deadline: time.sleep(.05)
            assert marker.exists()
            process.terminate(); process.wait(timeout=10)
            with socket.socket() as reservation:
                reservation.bind(("127.0.0.1", 0)); port = reservation.getsockname()[1]
            process = subprocess.Popen(command(port), stdin=subprocess.DEVNULL, stdout=output, stderr=output)
            restarted = wait(lambda value: not value["planning_running"])
            restart_state = api("planning?id=" + restart_id)
            assert restart_state["availability"] == "unavailable" and not restart_state["running"]
            retried = api("planning", {"action":"retry", "feature_id":restart_id,
                "request_id":str(uuid.uuid4()), "expected_revision":restart_state["revision"]})
            assert retried["running"]
            cancelled = api("planning", {"action":"cancel", "feature_id":restart_id,
                "request_id":str(uuid.uuid4()), "expected_revision":retried["revision"]})
            assert cancelled["stage"] == "cancelled" and not cancelled["running"]
            wait(lambda value: not value["planning_running"])

            feature_id = str(uuid.uuid4())
            validation = f'"{sys.executable}" -B -c "import app; assert app.VALUE == 1"'
            outside = root / "outside-context"
            outside.mkdir()
            (outside / "private.txt").write_text("token = abcdefgh\n")
            planned_project = projects / "planned"
            planned_project.mkdir()
            if sys.platform == "win32":
                subprocess.run(["cmd.exe", "/d", "/c", "mklink", "/J",
                    str(planned_project / "outside-junction"), str(outside)], check=True,
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            else:
                (planned_project / "outside-junction").symlink_to(outside, target_is_directory=True)
            feature = {"id":feature_id, "project":"planned", "instruction":"Implement the planned fixture",
                       "validation":validation, "model_target":"mac"}
            queued = enqueue_with_plan(f"http://127.0.0.1:{port}", token, feature)
            session = api("planning?id=" + feature_id)
            assert session["stage"] == "enqueued"
            documents = session["documents"]
            assert len(documents["plan_sha256"]) == 64
            assert session["history"][-1]["response_kind"] == "ready"
            assert session["history"][-1]["packet_sha256"]
            assert session["history"][-1]["output_sha256"]
            queued_feature = next(item for item in queued["queue"] if item["id"] == feature_id)
            assert queued_feature["planning"]["plan_sha256"] == documents["plan_sha256"]
            assert queued_feature["planning"]["skill_sha256"]

            api("control", {"action":"start", "expected_feature_id":feature_id,
                "expected_model_target":"mac", "expected_status":"queued", "expected_checkpoint":"not_started"})
            failed = wait(lambda value: not value["running"] and next(
                item for item in value["queue"] if item["id"] == feature_id)["status"] == "failed")
            failed_feature = next(item for item in failed["queue"] if item["id"] == feature_id)
            api("control", {"action":"repair", "id":feature_id,
                "expected_attempts":failed_feature["repair_attempts"]})
            completed = wait(lambda value: not value["running"] and next(
                item for item in value["queue"] if item["id"] == feature_id)["status"] == "succeeded")
            assert len(model_prompts) == 2

            combined = ("# Understanding\n" + documents["understanding"] + "\n\n# Assumptions\n" +
                documents["assumptions"] + "\n\n# Decision Log\n" + documents["decision_log"] +
                "\n\n# Design\n" + documents["design"] + "\n\n# Implementation Plan\n" +
                documents["implementation_plan"])
            assert all(combined in prompt for prompt in model_prompts), "approved plan missing from initial or repair prompt"
            evidence_path = root / "review-fixture/review-input-evidence.jsonl"
            evidence = [json.loads(line) for line in evidence_path.read_text().splitlines()]
            planning_rows = [row for row in evidence if row["kind"] == "planning"]
            review_row = [row for row in evidence if row["kind"] == "review"][-1]
            assert planning_rows and all(row["skill_present"] for row in planning_rows)
            assert all(row["skill_sha256"] == queued_feature["planning"]["skill_sha256"] for row in planning_rows)
            assert review_row["approved_plan_sha256"] == documents["plan_sha256"]
            assert review_row["approved_plan_text_sha256"] == hashlib.sha256(combined.encode()).hexdigest()

            with closing(__import__("sqlite3").connect(data / "developer.sqlite3")) as database:
                durable = json.loads(database.execute(
                    "SELECT state FROM developer_state WHERE id=1").fetchone()[0])
            assert "queue_v8" in durable and "queue_v5" not in durable
            print(json.dumps({"raw_enqueue_rejected":True, "exact_replay_safe":True,
                "emergency_invalidated_planning":True, "durable_skill_and_provider_evidence":True,
                "restart_invalidated_and_retry_cancelled":True,
                "reparse_context_not_disclosed":True,
                "approved_plan_reached_initial_repair_and_review":True, "durable_queue_schema":"queue_v8"}))
        finally:
            try: api("control", {"action":"emergency"})
            except Exception: pass
            if process.poll() is None:
                process.terminate(); process.wait(timeout=10)
            output.close()
    model.shutdown()


if __name__ == "__main__":
    main()
