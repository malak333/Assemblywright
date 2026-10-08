#!/usr/bin/env python3
"""Native process/HTTP proof for Developer branch, PR, checks, merge, cancellation, and recovery."""

from developer_planning_fixture import enqueue_with_plan
from developer_review_fixture import reviewer_arguments

import argparse
from contextlib import closing
import hashlib
import http.server
import io
import json
import os
from pathlib import Path
import shutil
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
        output = None
        process = None

        def start_runner(append=False):
            nonlocal output, process
            output = (root / "runner.log").open("ab" if append else "wb")
            process = subprocess.Popen(
                command, env=runner_env, stdin=subprocess.DEVNULL,
                stdout=output, stderr=output,
            )

        start_runner()
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
            # The verified remote receipt is persisted before cancellation-sensitive
            # local completion. Wait for the worker to finish before asserting it.
            merged = wait(lambda value: not value["github_publication_running"] and next(item for item in value["queue"] if item["id"] == feature_id)["publication_status"] == "succeeded")
            feature = next(item for item in merged["queue"] if item["id"] == feature_id)
            assert feature["status"] == "succeeded"
            assert feature["checkpoint"] == "publication_merged"
            assert feature["publication_commit_sha"] == feature["publication_merged_sha"]
            assert git("--git-dir", str(remote), "show", "main:app.py", capture=True) == "VALUE = 1"
            successful_publication_state = json.loads(fixture_state.read_text())
            assert successful_publication_state["merged"] == feature["publication_merged_sha"]

            # Reproduce the historical ledger bug as persisted state: a later
            # unrelated project-tool revision interrupted an already-complete
            # feature even though its last substantive review was the exact
            # approval. Startup must retain the contradiction, and Resume must
            # observe the remote receipt without replaying any effect.
            api("control", {"action": "shutdown"})
            process.wait(timeout=10)
            output.close()
            with closing(sqlite3.connect(data / "developer.sqlite3")) as connection_db, connection_db:
                durable = json.loads(connection_db.execute(
                    "SELECT state FROM developer_state WHERE id=1"
                ).fetchone()[0])
                interrupted = next(item for item in durable["queue_v13"] if item["id"] == feature_id)
                interrupted["status"] = "paused"
                interrupted["checkpoint"] = "review_tool_workspace_changed"
                interrupted["review_status"] = "interrupted"
                interrupted["review_summary"] = "Project tools changed the workspace; immutable validation and Codex review must run again."
                interrupted["message"] = interrupted["review_summary"]
                retained_ordinary_history = interrupted["review_history"]
                retained_ordinary_publication = interrupted["publication"]
                durable["revision"] += 1
                connection_db.execute(
                    "UPDATE developer_state SET state=? WHERE id=1",
                    (json.dumps(durable, separators=(",", ":")),),
                )
            calls_path = fixture_state.with_suffix(".calls")
            ordinary_calls_before = calls_path.read_text().splitlines()
            start_runner(append=True)
            restarted = wait(lambda value: not value["running"])
            restarted_feature = next(item for item in restarted["queue"] if item["id"] == feature_id)
            assert restarted_feature["status"] == "failed"
            assert restarted_feature["checkpoint"] == "publication_completion_reverify"
            control("resume")
            ordinary_recovered = wait(lambda value: (
                not value["github_publication_running"]
                and next(item for item in value["queue"] if item["id"] == feature_id)["status"] == "succeeded"
            ))
            ordinary_feature = next(item for item in ordinary_recovered["queue"] if item["id"] == feature_id)
            assert ordinary_feature["checkpoint"] == "publication_merged"
            assert ordinary_feature["review_status"] == "approved"
            with closing(sqlite3.connect(data / "developer.sqlite3")) as connection_db, connection_db:
                ordinary_durable = json.loads(connection_db.execute(
                    "SELECT state FROM developer_state WHERE id=1"
                ).fetchone()[0])
            ordinary_record = next(item for item in ordinary_durable["queue_v13"] if item["id"] == feature_id)
            assert ordinary_record["review_history"] == retained_ordinary_history
            for receipt_field in (
                "status", "stage", "repository_url", "repository_slug", "base_branch",
                "feature_branch", "base_sha", "candidate_tree_sha", "commit_sha", "pr_number",
                "pr_url", "merged_sha",
            ):
                assert ordinary_record["publication"][receipt_field] == retained_ordinary_publication[receipt_field]
            ordinary_retry_calls = [json.loads(line) for line in calls_path.read_text().splitlines()[len(ordinary_calls_before):]]
            assert not any(
                call["tool"] == "git" and "push" in call["arguments"]
                or call["tool"] == "gh" and call["arguments"][:2] in (["pr", "create"], ["pr", "merge"])
                for call in ordinary_retry_calls
            ), ordinary_retry_calls

            # A still later ledger revision may advance the completed feature's
            # cursor, but cannot reopen or rewrite the durable completion.
            api("control", {"action": "shutdown"})
            process.wait(timeout=10)
            output.close()
            late_revision = ordinary_record["tool_workspace_revision"] + 1
            late_request = str(uuid.uuid4())
            late_mutation = {
                "revision": late_revision,
                "request_id": late_request,
                "feature_id": None,
                "edits": [],
                "unreviewable_paths": [],
            }
            with closing(sqlite3.connect(data / "developer.sqlite3")) as connection_db, connection_db:
                connection_db.execute(
                    "INSERT OR REPLACE INTO developer_tool_workspace(project,revision) VALUES(?,?)",
                    ("connected", late_revision),
                )
                connection_db.execute(
                    "INSERT OR REPLACE INTO developer_tool_mutation(project,revision,request_id,feature_id,evidence) VALUES(?,?,?,?,?)",
                    ("connected", late_revision, late_request, None, json.dumps(late_mutation)),
                )
            start_runner(append=True)
            terminal = wait(lambda value: next(
                item for item in value["queue"] if item["id"] == feature_id
            )["tool_workspace_revision"] == late_revision)
            terminal_feature = next(item for item in terminal["queue"] if item["id"] == feature_id)
            assert terminal_feature["status"] == "succeeded"
            assert terminal_feature["checkpoint"] == "publication_merged"
            assert terminal_feature["review_status"] == "approved"

            # Advance main with an unrelated descendant, then reproduce the legacy
            # crash window where the complete publication receipt was durable but
            # the local feature promotion was not. Resume must only observe the
            # exact old PR/merge/tree plus retained candidate bytes at current main.
            descendant = root / "descendant"
            git("clone", str(remote), str(descendant), cwd=root)
            git("config", "user.name", "Publication Owner", cwd=descendant)
            git("config", "user.email", "owner@example.invalid", cwd=descendant)
            (descendant / "unrelated.txt").write_text("later publication\n")
            git("add", "unrelated.txt", cwd=descendant)
            git("commit", "-m", "later unrelated publication", cwd=descendant)
            git("push", "origin", "main", cwd=descendant)
            descendant_sha = git("rev-parse", "HEAD", cwd=descendant, capture=True)
            calls_path = fixture_state.with_suffix(".calls")
            calls_before_retry = calls_path.read_text().splitlines()

            api("control", {"action": "shutdown"})
            process.wait(timeout=10)
            output.close()
            with closing(sqlite3.connect(data / "developer.sqlite3")) as connection_db, connection_db:
                state_text = connection_db.execute(
                    "SELECT state FROM developer_state WHERE id=1"
                ).fetchone()[0]
                durable = json.loads(state_text)
                retained = next(item for item in durable["queue_v13"] if item["id"] == feature_id)
                assert retained["publication"]["status"] == "succeeded"
                assert retained["publication"]["stage"] == "complete"
                assert retained["escalation_history"] == []
                proposal_id = "23ae09d8-67a6-4c9b-ab0b-c2314e23a291"
                diagnosis_sha256 = "d" * 64
                candidate_sha256 = hashlib.sha256(b'{"files":[]}').hexdigest()
                marker_summary = "Proposal preparation did not produce an authorized application; independent review was not run"
                retained["escalation_count"] = 1
                retained["escalation_evidence_reserved"] = 3
                retained["review_evidence_reserved"] += 1
                retained["escalation_pending"] = False
                retained["escalation_proposal"] = {
                    "proposal_id": proposal_id,
                    "attempt": 1,
                    "feature_id": feature_id,
                    "feature_checkpoint": "review_1_approved",
                    "binding_revision": durable["revision"] + 1,
                    "model_target": retained["model_target"],
                    "model": "fixture-model",
                    "chat_id": "9b93aed7-a70e-4b94-985a-0e7f09c88bfb",
                    "chat_request_id": "e6c289ac-a382-4632-902a-a4bb81942bfe",
                    "chat_model_target": retained["model_target"],
                    "chat_model": "fixture-model",
                    "diagnosis": "The completed publication needs observation, not a file correction",
                    "diagnosis_sha256": diagnosis_sha256,
                    "status": "unavailable",
                    "summary": "The selected local AI could not prepare a repair proposal",
                    "error": "Expected 1 to 40 proposed files",
                    "files": [],
                    "protected_inputs": {},
                    "applied_paths": [],
                    "apply_request_id": None,
                    "source": "manual_chat",
                    "automatic_epoch": None,
                    "policy_revision": None,
                    "limit_snapshot": retained["auto_ai_repair_limit"],
                    "project_state_sha256": None,
                    "review_slot_terminal": True,
                }
                lineage = {
                    "proposal_id": proposal_id,
                    "attempt": 1,
                    "model_target": retained["model_target"],
                    "model": "fixture-model",
                    "chat_id": "9b93aed7-a70e-4b94-985a-0e7f09c88bfb",
                    "chat_request_id": "e6c289ac-a382-4632-902a-a4bb81942bfe",
                    "diagnosis_sha256": diagnosis_sha256,
                    "proposal_sha256": None,
                    "source": "manual_chat",
                    "automatic_epoch": None,
                    "policy_revision": None,
                    "limit_snapshot": retained["auto_ai_repair_limit"],
                    "project_state_sha256": None,
                    "authorization_revision": None,
                    "apply_request_id": None,
                }
                retained["escalation_history"] = [
                    dict(lineage, outcome="unavailable", candidate_sha256=None,
                         summary="The selected local AI could not prepare a repair proposal"),
                    dict(lineage, outcome="authorization_not_run", candidate_sha256=candidate_sha256,
                         summary=marker_summary),
                    dict(lineage, outcome="application_not_run", candidate_sha256=candidate_sha256,
                         summary=marker_summary),
                ]
                retained["review_history"].append({
                    "attempt": 1,
                    "packet_sha256": candidate_sha256,
                    "validation_evidence_sha256": diagnosis_sha256,
                    "outcome": "not_run",
                    "decision_sha256": None,
                    "binding_version": 0,
                    "batch_packet_sha256s": [],
                    "batch_receipt_sha256s": [],
                    "blocking_findings": [],
                    "summary": marker_summary,
                })
                retained_review_history = retained["review_history"]
                retained["status"] = "failed"
                retained["checkpoint"] = "review_binding_changed"
                retained["review_status"] = "interrupted"
                retained["review_summary"] = "Prior completion retry could not find the retained approval"
                retained["message"] = retained["review_summary"]
                durable["revision"] += 1
                connection_db.execute(
                    "UPDATE developer_state SET state=? WHERE id=1",
                    (json.dumps(durable, separators=(",", ":")),),
                )
            start_runner(append=True)
            wait(lambda value: not value["running"])
            control("resume")
            recovered = wait(lambda value: (
                not value["github_publication_running"]
                and next(item for item in value["queue"] if item["id"] == feature_id)["status"] == "succeeded"
            ))
            recovered_feature = next(item for item in recovered["queue"] if item["id"] == feature_id)
            assert recovered_feature["checkpoint"] == "publication_merged"
            assert recovered_feature["review_status"] == "approved"
            assert recovered_feature["publication_merged_sha"] == feature["publication_merged_sha"]
            assert git("--git-dir", str(remote), "rev-parse", "main", capture=True) == descendant_sha
            with closing(sqlite3.connect(data / "developer.sqlite3")) as connection_db, connection_db:
                recovered_durable = json.loads(connection_db.execute(
                    "SELECT state FROM developer_state WHERE id=1"
                ).fetchone()[0])
            recovered_record = next(item for item in recovered_durable["queue_v13"] if item["id"] == feature_id)
            assert recovered_record["review_history"] == retained_review_history
            retry_calls = [json.loads(line) for line in calls_path.read_text().splitlines()[len(calls_before_retry):]]
            assert not any(
                call["tool"] == "git" and "push" in call["arguments"]
                or call["tool"] == "gh" and call["arguments"][:2] in (["pr", "create"], ["pr", "merge"])
                for call in retry_calls
            ), retry_calls

            # Abandonment is an explicit observation-only recovery for an exact
            # retained PR that the owner has already closed without merging.
            # Reset the fixture base content so this second feature has a real
            # reviewed edit, while retaining the earlier publication history.
            (descendant / "app.py").write_bytes(b"VALUE = 0\n")
            git("add", "app.py", cwd=descendant)
            git("commit", "-m", "prepare abandonment fixture", cwd=descendant)
            git("push", "origin", "main", cwd=descendant)
            abandonment_fixture = json.loads(fixture_state.read_text())
            abandonment_fixture.update(
                pr_number=None, head=None, base=None, merged=None, closed=False,
            )
            fixture_state.write_text(json.dumps(abandonment_fixture))
            mode.write_text("pending")
            abandoned_id = enqueue("connected", "[publication:closed-unmerged-abandon]")
            control("start")
            waiting = wait(lambda value: next(
                item for item in value["queue"] if item["id"] == abandoned_id
            )["publication_stage"] == "wait_required_checks")
            waiting_feature = next(item for item in waiting["queue"] if item["id"] == abandoned_id)
            exact_head = waiting_feature["publication_commit_sha"]
            control("stop")
            attention = wait(lambda value: not value["github_publication_running"] and next(
                item for item in value["queue"] if item["id"] == abandoned_id
            )["checkpoint"] == "publication_attention")
            attention_feature = next(item for item in attention["queue"] if item["id"] == abandoned_id)
            assert attention_feature["can_abandon_publication"] is True

            def abandon(snapshot, checkpoint="publication_attention"):
                return api("publication", dict(
                    action="abandon", feature_id=abandoned_id,
                    expected_revision=snapshot["revision"], expected_checkpoint=checkpoint,
                ))

            # An open, merged, missing, or head-drifted PR cannot clear the
            # unresolved-publication barrier.
            rejected("publication", dict(
                action="abandon", feature_id=abandoned_id,
                expected_revision=attention["revision"],
                expected_checkpoint="publication_attention",
            ))
            fixture = json.loads(fixture_state.read_text())
            fixture["closed"] = True
            fixture["head"] = "f" * 40
            fixture_state.write_text(json.dumps(fixture))
            rejected("publication", dict(
                action="abandon", feature_id=abandoned_id,
                expected_revision=attention["revision"],
                expected_checkpoint="publication_attention",
            ))
            fixture["head"] = exact_head
            fixture["merged"] = exact_head
            fixture_state.write_text(json.dumps(fixture))
            rejected("publication", dict(
                action="abandon", feature_id=abandoned_id,
                expected_revision=attention["revision"],
                expected_checkpoint="publication_attention",
            ))
            fixture["merged"] = None
            fixture["pr_number"] = None
            fixture_state.write_text(json.dumps(fixture))
            rejected("publication", dict(
                action="abandon", feature_id=abandoned_id,
                expected_revision=attention["revision"],
                expected_checkpoint="publication_attention",
            ))
            fixture["pr_number"] = 17
            fixture_state.write_text(json.dumps(fixture))

            # Stop cancels an in-flight observation and leaves the exact durable
            # attention checkpoint unchanged for a later owner retry.
            mode.write_text("abandon-wait")
            cancellation = {}
            def request_abandonment():
                try:
                    cancellation["value"] = abandon(api())
                except urllib.error.HTTPError as error:
                    error.response_body = error.read(16 * 1024)
                    cancellation["error"] = error
            abandoning = threading.Thread(target=request_abandonment)
            abandoning.start()
            wait(lambda value: value["github_publication_running"] is True)
            control("stop")
            abandoning.join(timeout=15)
            assert not abandoning.is_alive()
            assert cancellation.get("error") is not None
            assert cancellation["error"].code == 409
            after_cancel = wait(lambda value: value["github_publication_running"] is False)
            cancelled_feature = next(item for item in after_cancel["queue"] if item["id"] == abandoned_id)
            assert cancelled_feature["status"] == "failed"
            assert cancelled_feature["checkpoint"] == "publication_attention"
            assert cancelled_feature["publication_status"] == "attention"

            with closing(sqlite3.connect(data / "developer.sqlite3")) as connection_db:
                before_abandonment = json.loads(connection_db.execute(
                    "SELECT state FROM developer_state WHERE id=1"
                ).fetchone()[0])
            retained_before = next(item for item in before_abandonment["queue_v13"]
                                   if item["id"] == abandoned_id)
            publication_before = retained_before["publication"]
            review_before = retained_before["review_history"]
            candidate_before = retained_before["publication_candidate"]
            calls_before_abandonment = calls_path.read_text().splitlines()
            mode.write_text("pending")
            abandoned = abandon(after_cancel)
            assert abandoned["revision"] == after_cancel["revision"] + 1
            assert abandoned["github_publication_unresolved"] is False
            assert all(item["id"] != abandoned_id for item in abandoned["queue"])
            with closing(sqlite3.connect(data / "developer.sqlite3")) as connection_db:
                durable_abandoned = json.loads(connection_db.execute(
                    "SELECT state FROM developer_state WHERE id=1"
                ).fetchone()[0])
            retained_after = next(item for item in durable_abandoned["queue_v13"]
                                  if item["id"] == abandoned_id)
            assert retained_after["status"] == "removed"
            assert retained_after["checkpoint"] == "publication_abandoned"
            assert retained_after["review_history"] == review_before
            assert retained_after["publication_candidate"] == candidate_before
            publication_after = retained_after["publication"]
            for field in (
                "repository_url", "repository_slug", "base_branch", "feature_branch",
                "base_sha", "candidate_tree_sha", "commit_sha", "pr_number", "pr_url",
                "merged_sha", "required_checks", "strict_required_checks",
            ):
                assert publication_after[field] == publication_before[field]
            assert publication_after["status"] == "abandoned"
            assert publication_after["stage"] == "abandoned"
            assert publication_after["events"][:-1] == publication_before["events"]
            assert publication_after["events"][-1]["kind"] == "receipt"
            assert publication_after["events"][-1]["stage"] == "abandoned"
            abandonment_calls = [json.loads(line) for line in calls_path.read_text().splitlines()[len(calls_before_abandonment):]]
            assert any(call["tool"] == "gh" and call["arguments"][:2] == ["pr", "view"]
                       for call in abandonment_calls)
            assert not any(
                call["tool"] == "git" and "push" in call["arguments"]
                or call["tool"] == "gh" and call["arguments"][:2] in (["pr", "create"], ["pr", "merge"])
                for call in abandonment_calls
            ), abandonment_calls

            idempotent = abandon(abandoned, checkpoint="publication_abandoned")
            assert idempotent["revision"] == abandoned["revision"]
            api("control", {"action": "shutdown"})
            process.wait(timeout=10)
            output.close()
            start_runner(append=True)
            restarted_abandoned = wait(lambda value: not value["running"])
            assert restarted_abandoned["github_publication_unresolved"] is False
            assert all(item["id"] != abandoned_id for item in restarted_abandoned["queue"])
            with closing(sqlite3.connect(data / "developer.sqlite3")) as connection_db:
                restarted_durable = json.loads(connection_db.execute(
                    "SELECT state FROM developer_state WHERE id=1"
                ).fetchone()[0])
            restarted_record = next(item for item in restarted_durable["queue_v13"]
                                    if item["id"] == abandoned_id)
            assert restarted_record["status"] == "removed"
            assert restarted_record["checkpoint"] == "publication_abandoned"
            assert restarted_record["publication"] == publication_after

            local_id = enqueue("local-only", "[publication:local-only]")
            control("start")
            local = wait(lambda value: next(item for item in value["queue"] if item["id"] == local_id)["status"] == "succeeded")
            local_feature = next(item for item in local["queue"] if item["id"] == local_id)
            assert local_feature["publication_status"] == "local_only"
            assert local_feature["publication_pr_url"] is None

            state = json.loads(fixture_state.read_text())
            assert state["pr_number"] == 17
            assert state["head"] == exact_head and state["closed"] is True
            assert state["merged"] is None
            assert state["check_run_observations"] >= 3
            print(json.dumps({
                "native_platform": sys.platform,
                "real_bare_git_transport": True,
                "canonical_existing_repository_connection": True,
                "unbound_or_non_strict_policy_rejected": True,
                "required_checks_exact_commit_and_app": True,
                "required_check_registration_delay_tolerated": True,
                "stop_blocks_queue_and_reconcile_merges": True,
                "exact_reviewed_commit_merged": True,
                "completed_receipt_descendant_reverified_without_effect_replay": True,
                "completed_receipt_workspace_interruption_recovered_after_restart": True,
                "terminal_publication_survives_late_tool_revision": True,
                "closed_unmerged_publication_abandoned_without_remote_effect": True,
                "abandonment_rejects_open_merged_missing_and_drifted_pr": True,
                "abandonment_cancellation_and_restart_preserve_attention_or_terminal_history": True,
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
            if process is not None and process.poll() is None:
                process.terminate()
            if process is not None:
                process.wait(timeout=10)
            if output is not None and not output.closed:
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
