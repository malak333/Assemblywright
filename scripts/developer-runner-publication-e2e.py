#!/usr/bin/env python3
"""Native process/HTTP proof for Developer branch, PR, checks, merge, cancellation, and recovery."""

from developer_planning_fixture import enqueue_with_plan
from developer_review_fixture import reviewer_arguments

import argparse
import http.server
import io
import json
import os
from pathlib import Path
import shutil
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
    parser.add_argument("--github-fixture", required=True)
    parser.add_argument("--output", help="Copy the disposable fixture and logs here if the E2E fails")
    args = parser.parse_args()

    class Model(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            prompt = request["messages"][1]["content"]
            assert "github.com/owner/project" not in prompt.lower(), prompt
            assert "gh_token" not in prompt.lower(), prompt
            content = json.dumps({"files": [{"path": "app.py", "content": "VALUE = 1\n"}]})
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

    with tempfile.TemporaryDirectory(prefix="assemblywright-developer-publication-e2e-") as temp:
        root = Path(temp)
        data = root / "state"
        projects = root / "projects"
        source = root / "source"
        remote = root / "remote.git"
        fixture_dir = root / "fixture tools"
        git_home = root / "git-home"
        mode = root / "fixture-mode"
        fixture_state = root / "github-state.json"
        for path in [data, projects, source, fixture_dir, git_home]:
            path.mkdir()
        mode.write_text("pending")
        real_git = Path(shutil.which("git") or "")
        assert real_git.is_file(), "git is required for the publication E2E"
        git_env = dict(os.environ, HOME=str(git_home), USERPROFILE=str(git_home))

        def git(*values, cwd=None, capture=False):
            result = subprocess.run(
                [str(real_git), *values], cwd=cwd, env=git_env,
                check=True, text=True, capture_output=capture,
            )
            return result.stdout.strip() if capture else ""

        git("init", cwd=source)
        git("checkout", "-b", "main", cwd=source)
        git("config", "user.name", "Publication Owner", cwd=source)
        git("config", "user.email", "owner@example.invalid", cwd=source)
        git("config", "core.autocrlf", "false", cwd=source)
        (source / "app.py").write_bytes(b"VALUE = 0\n")
        git("add", "app.py", cwd=source)
        git("commit", "-m", "remote baseline", cwd=source)
        git("clone", "--bare", str(source), str(remote), cwd=root)
        git("config", "--global", "user.name", "Publication Owner")
        git("config", "--global", "user.email", "owner@example.invalid")
        git("config", "--global", "core.autocrlf", "false")

        suffix = ".exe" if os.name == "nt" else ""
        git_fixture = fixture_dir / ("git" + suffix)
        gh_fixture = fixture_dir / ("gh" + suffix)
        shutil.copy2(Path(args.github_fixture).resolve(), git_fixture)
        shutil.copy2(Path(args.github_fixture).resolve(), gh_fixture)
        if os.name != "nt":
            git_fixture.chmod(0o700)
            gh_fixture.chmod(0o700)

        runner_env = dict(
            git_env,
            ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_REAL_GIT=str(real_git.resolve()),
            ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_REMOTE=str(remote.resolve()),
            ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_STATE=str(fixture_state.resolve()),
            ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_SLUG="owner/project",
            ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_MODE=str(mode.resolve()),
        )
        command = [
            str(Path(args.binary).resolve()),
            "--data-dir", str(data),
            "--workspace-root", str(projects),
            "--bind", f"127.0.0.1:{port}",
            "--model-url", f"http://127.0.0.1:{model.server_port}/v1",
            "--git-executable", str(git_fixture),
            "--gh-executable", str(gh_fixture),
        ] + reviewer_arguments(root)
        output = (root / "runner.log").open("wb")
        process = subprocess.Popen(command, env=runner_env, stdin=subprocess.DEVNULL, stdout=output, stderr=output)
        token = ""

        def api(path="status", body=None, timeout=60):
            request = urllib.request.Request(
                f"http://127.0.0.1:{port}/{path}",
                data=json.dumps(body).encode() if body is not None else None,
                headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
            )
            try:
                return json.load(urllib.request.urlopen(request, timeout=timeout))
            except urllib.error.HTTPError as error:
                response = error.read(16 * 1024)
                error.fp = io.BytesIO(response)
                error.response_body = response
                error.add_note("bounded response: " + response.decode(errors="replace"))
                raise

        def rejected(path, body):
            try:
                api(path, body)
                raise AssertionError("mutation unexpectedly succeeded")
            except urllib.error.HTTPError as error:
                assert error.code == 409, error.code
                response = json.loads(error.response_body)
                assert "error" in response
                assert "token" not in response["error"].lower()

        def control(action, **values):
            if action in ("start", "resume") and "expected_feature_id" not in values:
                current = api()
                feature = next(item for item in current["queue"] if item["status"] not in ("succeeded", "removed"))
                values.update(
                    expected_feature_id=feature["id"],
                    expected_model_target=feature["model_target"],
                    expected_status=feature["status"],
                    expected_checkpoint=feature["checkpoint"],
                )
            return api("control", dict(action=action, **values))

        def wait(predicate, timeout=60):
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
            raise AssertionError("Timed out: " + json.dumps(snapshot) + "\n" + (root / "runner.log").read_text(errors="replace"))

        def reconcile_after_stop(feature_id, timeout=10):
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                current = api()
                feature = next(item for item in current["queue"] if item["id"] == feature_id)
                assert feature["checkpoint"] == "publication_attention"
                try:
                    return api("publication", dict(
                        action="reconcile", feature_id=feature_id,
                        expected_revision=current["revision"],
                        expected_checkpoint="publication_attention",
                    ))
                except urllib.error.HTTPError as error:
                    body = json.loads(error.response_body)
                    if error.code != 409 or body.get("error") != "Runner revision changed; refresh before reconciling publication":
                        raise
                time.sleep(0.05)
            raise AssertionError("Timed out waiting to bind reconcile to the final stopped revision")

        def enqueue(project, marker):
            feature_id = str(uuid.uuid4())
            project_path = projects / project
            project_path.mkdir(exist_ok=True)
            (project_path / "app.py").write_bytes(b"VALUE = 0\n")
            validation = f'"{sys.executable}" -B -c "import app; assert app.VALUE == 1"'
            enqueue_with_plan(
                f"http://127.0.0.1:{port}", token,
                dict(id=feature_id, project=project, instruction=f"Implement publication fixture {marker}", validation=validation),
            )
            return feature_id

        failure = None
        failure_traceback = None
        try:
            deadline = time.monotonic() + 20
            while not (data / "developer-token").exists() and time.monotonic() < deadline:
                time.sleep(0.05)
            token = (data / "developer-token").read_text().strip()
            initial = wait(lambda value: not value["running"])
            assert initial["github_publication_supported"] is True
            (projects / "connected").mkdir()
            (projects / "connected" / "app.py").write_bytes(b"VALUE = 0\n")
            rejected("publication", dict(action="save_connection", project="connected", repository_url="https://evil.example/owner/project", base_branch="main", expected_revision=initial["revision"]))

            mode.write_text("unbound-policy")
            rejected("publication", dict(
                action="save_connection", project="connected",
                repository_url="https://github.com/owner/project", base_branch="main",
                expected_revision=api()["revision"],
            ))
            mode.write_text("stale-policy")
            rejected("publication", dict(
                action="save_connection", project="connected",
                repository_url="https://github.com/owner/project", base_branch="main",
                expected_revision=api()["revision"],
            ))
            mode.write_text("pending")

            saved = api("publication", dict(
                action="save_connection", project="connected",
                repository_url="https://github.com/owner/project", base_branch="main",
                expected_revision=api()["revision"],
            ))
            connection = next(item for item in saved["github_connections"] if item["project"] == "connected")
            assert connection == {
                "project": "connected", "repository_url": "https://github.com/owner/project.git",
                "base_branch": "main", "automatic_merge": True,
            }

            feature_id = enqueue("connected", "[publication:cancel-reconcile]")
            control("start")
            publishing = wait(lambda value: next(item for item in value["queue"] if item["id"] == feature_id)["publication_stage"] == "wait_required_checks")
            feature = next(item for item in publishing["queue"] if item["id"] == feature_id)
            assert feature["publication_status"] == "running"
            assert feature["publication_pr_url"] == "https://github.com/owner/project/pull/17"
            control("stop")
            stopped = wait(lambda value: not value["github_publication_running"] and next(item for item in value["queue"] if item["id"] == feature_id)["publication_status"] == "attention")
            feature = next(item for item in stopped["queue"] if item["id"] == feature_id)
            assert feature["checkpoint"] == "publication_attention"
            rejected("control", dict(action="start", expected_feature_id=feature_id, expected_model_target=feature["model_target"], expected_status=feature["status"], expected_checkpoint=feature["checkpoint"]))

            mode.write_text("pass")
            accepted = reconcile_after_stop(feature_id)
            accepted_feature = next(item for item in accepted["queue"] if item["id"] == feature_id)
            assert accepted["github_publication_running"] is True
            assert accepted_feature["checkpoint"] == "publication_reconciling"
            merged = wait(lambda value: next(item for item in value["queue"] if item["id"] == feature_id)["publication_status"] == "succeeded")
            feature = next(item for item in merged["queue"] if item["id"] == feature_id)
            assert feature["status"] == "succeeded"
            assert feature["checkpoint"] == "publication_merged"
            assert feature["publication_commit_sha"] == feature["publication_merged_sha"]
            assert git("--git-dir", str(remote), "show", "main:app.py", capture=True) == "VALUE = 1"

            local_id = enqueue("local-only", "[publication:local-only]")
            control("start")
            local = wait(lambda value: next(item for item in value["queue"] if item["id"] == local_id)["status"] == "succeeded")
            local_feature = next(item for item in local["queue"] if item["id"] == local_id)
            assert local_feature["publication_status"] == "local_only"
            assert local_feature["publication_pr_url"] is None

            state = json.loads(fixture_state.read_text())
            assert state["pr_number"] == 17 and state["merged"] == feature["publication_merged_sha"]
            print(json.dumps({
                "native_platform": sys.platform,
                "real_bare_git_transport": True,
                "canonical_existing_repository_connection": True,
                "unbound_or_non_strict_policy_rejected": True,
                "required_checks_exact_commit_and_app": True,
                "stop_blocks_queue_and_reconcile_merges": True,
                "exact_reviewed_commit_merged": True,
                "unconnected_project_local_only": True,
                "live_github_credentials_used": False,
            }))
        except BaseException as error:
            failure = error
            failure_traceback = error.__traceback__
        finally:
            try:
                api("control", {"action": "shutdown"})
            except Exception:
                pass
            if process.poll() is None:
                process.terminate()
            process.wait(timeout=10)
            output.close()
            model.shutdown()
            if failure is not None and args.output:
                destination = Path(args.output).resolve()
                if destination.exists():
                    shutil.rmtree(destination)
                shutil.copytree(root, destination)
        if failure is not None:
            raise failure.with_traceback(failure_traceback)


if __name__ == "__main__":
    main()
