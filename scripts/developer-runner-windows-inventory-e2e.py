#!/usr/bin/env python3
"""Native process/HTTP proof for read-only Windows inventory dot metadata."""

from developer_planning_fixture import enqueue_with_plan
from developer_review_fixture import reviewer_arguments

import argparse
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
    model_calls = []

    class Model(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            prompt = request["messages"][1]["content"]
            model_calls.append(prompt)
            content = json.dumps({"files": [{"path": "app.py", "content": "VALUE = 1\n"}]})
            body = json.dumps({
                "choices": [{"message": {"content": content}, "finish_reason": "stop"}]
            }).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *_unused):
            pass

    model = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
    model.daemon_threads = True
    threading.Thread(target=model.serve_forever, daemon=True).start()
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]

    with tempfile.TemporaryDirectory(prefix="assemblywright-windows-inventory-e2e-") as temp:
        root = Path(temp)
        data = root / "state"
        projects = root / "projects"
        project = projects / "published-baseline"
        workflow = project / ".github" / "workflows" / "validate.yml"
        attributes = project / ".gitattributes"
        workflow.parent.mkdir(parents=True)
        data.mkdir()
        attributes.write_bytes(b"* text=auto eol=lf\n")
        workflow.write_bytes(b"name: Validate\n")
        metadata_before = {
            str(path.relative_to(project)).replace("\\", "/"): (
                path.read_bytes(), hashlib.sha256(path.read_bytes()).hexdigest()
            )
            for path in (attributes, workflow)
        }
        output = (root / "runner.log").open("wb")
        process = subprocess.Popen([
            str(Path(args.binary).resolve()),
            "--data-dir", str(data),
            "--workspace-root", str(projects),
            "--bind", f"127.0.0.1:{port}",
            "--model-url", f"http://127.0.0.1:{model.server_port}/v1",
            *reviewer_arguments(root),
        ], stdin=subprocess.DEVNULL, stdout=output, stderr=output)
        token = ""

        def api(action=None, **values):
            if action in ("start", "resume") and "expected_feature_id" not in values:
                current = api()
                feature = next(
                    item for item in current["queue"]
                    if item["status"] not in ("succeeded", "removed")
                )
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
                headers={
                    "Authorization": "Bearer " + token,
                    "Content-Type": "application/json",
                },
            )
            return json.load(urllib.request.urlopen(request, timeout=10))

        def wait(predicate, timeout=30):
            deadline = time.monotonic() + timeout
            snapshot = None
            while time.monotonic() < deadline:
                try:
                    snapshot = api()
                    if predicate(snapshot):
                        return snapshot
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(0.05)
            raise AssertionError(
                "Timed out: " + json.dumps(snapshot) + "\n"
                + (root / "runner.log").read_text(errors="replace")
            )

        try:
            deadline = time.monotonic() + 20
            while not (data / "developer-token").exists() and time.monotonic() < deadline:
                time.sleep(0.05)
            token = (data / "developer-token").read_text().strip()
            wait(lambda snapshot: not snapshot["running"])
            feature_id = str(uuid.uuid4())
            validation = (
                f'"{sys.executable}" -B -c '
                '"import app; assert app.VALUE == 1"'
            )
            enqueue_with_plan(
                f"http://127.0.0.1:{port}",
                token,
                {
                    "id": feature_id,
                    "project": "published-baseline",
                    "instruction": "Implement the bounded inventory fixture",
                    "validation": validation,
                },
            )
            api("start")
            completed = wait(lambda snapshot: (
                not snapshot["running"]
                and next(item for item in snapshot["queue"] if item["id"] == feature_id)[
                    "status"
                ] == "succeeded"
            ))
            feature = next(item for item in completed["queue"] if item["id"] == feature_id)
            assert feature["review_status"] == "approved", feature
            assert feature["changed_files"] == ["app.py"], feature
            assert len(model_calls) == 1, model_calls
            prompt = model_calls[0]
            assert prompt.count("excluded_metadata") >= 2, prompt
            for relative, (_, content_sha256) in metadata_before.items():
                path_sha256 = hashlib.sha256(relative.encode()).hexdigest()
                assert path_sha256 in prompt, (relative, path_sha256)
                assert content_sha256 in prompt, (relative, content_sha256)
            for forbidden in (
                ".gitattributes", ".github", "workflows/validate.yml", "name: Validate"
            ):
                assert forbidden not in prompt, forbidden
            assert (project / "app.py").read_bytes() == b"VALUE = 1\n"
            for relative, (before_bytes, before_sha256) in metadata_before.items():
                path = project / relative
                assert path.read_bytes() == before_bytes
                assert hashlib.sha256(path.read_bytes()).hexdigest() == before_sha256
            print(json.dumps({
                "native_platform": sys.platform,
                "model_call_reached_after_inventory": True,
                "dot_metadata_hashed_without_context_disclosure": True,
                "dot_metadata_bytes_preserved": True,
                "review_approved_after_validation": True,
                "live_credentials_used": False,
            }))
        finally:
            try:
                api("shutdown")
            except Exception:
                pass
            if process.poll() is None:
                process.terminate()
            process.wait(timeout=10)
            output.close()
    model.shutdown()
    model.server_close()


if __name__ == "__main__":
    main()
