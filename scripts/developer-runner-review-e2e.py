#!/usr/bin/env python3
"""Native process/HTTP proof for the mandatory developer Codex review gate."""

from developer_review_fixture import reviewer_arguments
from developer_planning_fixture import enqueue_with_plan

import argparse
from contextlib import closing
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
import urllib.request
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    args = parser.parse_args()
    model_calls = []

    class Model(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            prompt = request["messages"][1]["content"]
            model_calls.append(prompt)
            repair = re.search(r"Repair attempt: (\d+) of 3", prompt)
            value = 1 if repair else 0
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

    with tempfile.TemporaryDirectory(prefix="assemblywright-developer-review-e2e-") as temp:
        root = Path(temp)
        data = root / "state"
        projects = root / "projects"
        data.mkdir()
        def runner_command(port_value):
            return [
                str(Path(args.binary).resolve()),
                "--data-dir", str(data),
                "--workspace-root", str(projects),
                "--bind", f"127.0.0.1:{port_value}",
                "--model-url", f"http://127.0.0.1:{model.server_port}/v1",
            ] + reviewer_arguments(root)

        command = runner_command(port)
        output = (root / "runner.log").open("wb")
        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=output, stderr=output)
        token = ""

        def call(action=None, **values):
            if action in ("start", "resume") and "expected_feature_id" not in values:
                current = call()
                feature = next(f for f in current["queue"] if f["status"] not in ("succeeded", "removed"))
                values.update(
                    expected_feature_id=feature["id"],
                    expected_model_target=feature["model_target"],
                    expected_status=feature["status"],
                    expected_checkpoint=feature["checkpoint"],
                )
            body = json.dumps(dict(action=action, **values)).encode() if action else None
            request = urllib.request.Request(
                f"http://127.0.0.1:{port}/" + ("control" if action else "status"),
                data=body,
                headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
            )
            return json.load(urllib.request.urlopen(request, timeout=5))

        def rejected(action, **values):
            try:
                call(action, **values)
                raise AssertionError(f"{action} unexpectedly succeeded")
            except urllib.error.HTTPError as error:
                assert error.code == 409, error.code

        def wait(predicate, timeout=30):
            deadline = time.monotonic() + timeout
            snapshot = None
            while time.monotonic() < deadline:
                try:
                    snapshot = call()
                    if predicate(snapshot):
                        return snapshot
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(0.05)
            raise AssertionError("Timed out: " + json.dumps(snapshot) + "\n" + (root / "runner.log").read_text(errors="replace"))

        def enqueue(project, marker):
            feature_id = str(uuid.uuid4())
            validation = f'"{sys.executable}" -B -c "import app; assert hasattr(app, \'VALUE\')"'
            enqueue_with_plan(
                f"http://127.0.0.1:{port}", token,
                dict(id=feature_id, project=project, instruction=f"Implement the fixture {marker}", validation=validation),
            )
            return feature_id

        def start_waiting_review(feature_id):
            started_marker = root / "review-fixture/started.pid"
            started_marker.unlink(missing_ok=True)
            call("start")
            wait(
                lambda snapshot: next(f for f in snapshot["queue"] if f["id"] == feature_id)["review_status"] == "reviewing"
            )
            deadline = time.monotonic() + 10
            while not started_marker.exists() and time.monotonic() < deadline:
                time.sleep(0.05)
            assert started_marker.exists(), "review fixture never received the candidate packet"

        try:
            deadline = time.monotonic() + 15
            while not (data / "developer-token").exists() and time.monotonic() < deadline:
                time.sleep(0.05)
            token = (data / "developer-token").read_text().strip()
            initial = wait(lambda snapshot: not snapshot["running"])
            assert initial["review_required"] is True
            assert initial["review_provider"] == "openai.codex"
            assert initial["review_model"] == "gpt-5.6-sol"

            repaired_id = enqueue("review-repair", "[fixture:reject-zero]")
            call("start")
            repaired = wait(
                lambda snapshot: not snapshot["running"]
                and snapshot["queue"][-1]["id"] == repaired_id
                and snapshot["queue"][-1]["status"] == "succeeded"
            )
            feature = repaired["queue"][-1]
            assert feature["review_status"] == "approved"
            assert feature["repair_attempts"] == 1
            assert feature["review_attempts"] == 2
            assert (projects / "review-repair/app.py").read_text() == "VALUE = 1\n"

            malformed_id = enqueue("review-malformed", "[fixture:malformed]")
            blocked_id = enqueue("review-blocked", "[fixture:reject-zero]")
            call("start")
            malformed = wait(
                lambda snapshot: not snapshot["running"]
                and next(f for f in snapshot["queue"] if f["id"] == malformed_id)["status"] == "failed"
            )
            feature = next(f for f in malformed["queue"] if f["id"] == malformed_id)
            blocked = next(f for f in malformed["queue"] if f["id"] == blocked_id)
            assert feature["review_status"] == "unavailable"
            assert feature["repair_attempts"] == 0
            assert feature["review_attempts"] == 1
            assert blocked["status"] == "queued"
            rejected("repair", id=malformed_id, expected_attempts=0)
            model_count = len(model_calls)
            call("resume")
            retried = wait(
                lambda snapshot: not snapshot["running"]
                and next(f for f in snapshot["queue"] if f["id"] == malformed_id)["review_attempts"] == 2
            )
            feature = next(f for f in retried["queue"] if f["id"] == malformed_id)
            assert feature["review_status"] == "unavailable"
            assert feature["repair_attempts"] == 0
            assert len(model_calls) == model_count, "review retry regenerated code"
            call("remove", id=malformed_id)
            call("remove", id=blocked_id)

            waiting_id = enqueue("review-cancel", "[fixture:wait]")
            start_waiting_review(waiting_id)
            call("emergency")
            stopped = wait(lambda snapshot: not snapshot["running"])
            feature = next(f for f in stopped["queue"] if f["id"] == waiting_id)
            assert feature["status"] == "paused"
            assert feature["review_status"] == "interrupted"
            assert feature["repair_attempts"] == 0
            assert stopped["emergency_paused"] is True
            call("clear_emergency")
            call("remove", id=waiting_id)

            drift_id = enqueue("review-drift", "[fixture:wait]")
            start_waiting_review(drift_id)
            (projects / "review-drift/app.py").write_text("VALUE = 77\n")
            drifted = wait(
                lambda snapshot: not snapshot["running"]
                and next(f for f in snapshot["queue"] if f["id"] == drift_id)["status"] == "failed",
                30,
            )
            feature = next(f for f in drifted["queue"] if f["id"] == drift_id)
            assert feature["review_status"] == "interrupted"
            assert feature["repair_attempts"] == 0
            assert "changed" in feature["review_summary"]
            call("remove", id=drift_id)

            restart_id = enqueue("review-restart", "[fixture:wait]")
            start_waiting_review(restart_id)
            process.terminate()
            process.wait(timeout=5)
            with socket.socket() as restart_reservation:
                restart_reservation.bind(("127.0.0.1", 0))
                port = restart_reservation.getsockname()[1]
            command = runner_command(port)
            process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=output, stderr=output)
            restarted = wait(lambda snapshot: not snapshot["running"])
            feature = next(f for f in restarted["queue"] if f["id"] == restart_id)
            assert feature["status"] == "paused"
            assert feature["review_status"] == "interrupted"
            assert feature["review_attempts"] == 1
            assert feature["repair_attempts"] == 0

            with closing(sqlite3.connect(data / "developer.sqlite3")) as database:
                durable = json.loads(database.execute("SELECT state FROM developer_state WHERE id=1").fetchone()[0])
            assert "queue_v8" in durable
            durable_repaired = next(f for f in durable["queue_v8"] if f["id"] == repaired_id)
            assert [attempt["outcome"] for attempt in durable_repaired["review_history"]] == ["rejected", "approved"]
            assert all(attempt["decision_sha256"] for attempt in durable_repaired["review_history"])
            durable_restarted = next(f for f in durable["queue_v8"] if f["id"] == restart_id)
            assert durable_restarted["review_pending"] is None
            assert durable_restarted["review_history"][-1]["outcome"] == "interrupted"
            assert durable_restarted["review_history"][-1]["decision_sha256"] is None
            print(json.dumps({
                "native_platform": sys.platform,
                "review_rejection_auto_repaired": True,
                "approval_bound_after_validation": True,
                "malformed_review_no_repair_or_advance": True,
                "review_retry_no_regeneration": True,
                "emergency_review_invalidated": True,
                "late_approval_rejected_after_file_drift": True,
                "restart_invalidated_pending_review": True,
                "durable_queue_schema": "queue_v8",
            }))
        finally:
            try:
                call("emergency")
                wait(lambda snapshot: not snapshot["running"], 5)
            except Exception:
                pass
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
            output.close()
    model.shutdown()


if __name__ == "__main__":
    main()
