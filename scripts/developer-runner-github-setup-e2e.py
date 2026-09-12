#!/usr/bin/env python3
"""Native runner HTTP proof for Developer GitHub sign-in, discovery, creation, and recovery."""

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
            body = json.dumps({"choices": [{"message": {"content": "{}"}, "finish_reason": "stop"}]}).encode()
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

    with tempfile.TemporaryDirectory(prefix="assemblywright-developer-github-setup-e2e-") as temp:
        root = Path(temp)
        data = root / "state"
        projects = root / "projects"
        fixture_dir = root / "fixture tools"
        git_home = root / "git-home"
        remote = root / "remote.git"
        mode = root / "fixture-mode"
        fixture_state = root / "github-state.json"
        for path in [data, projects, fixture_dir, git_home, remote]:
            path.mkdir()
        marker = projects / "must-not-upload.txt"
        marker.write_text("PROJECT-CONTENTS-MUST-STAY-LOCAL\n")
        mode.write_text("setup-signed-out")
        real_git = Path(shutil.which("git") or "")
        assert real_git.is_file(), "git is required for the GitHub setup E2E"
        suffix = ".exe" if os.name == "nt" else ""
        git_fixture = fixture_dir / ("git" + suffix)
        gh_fixture = fixture_dir / ("gh" + suffix)
        shutil.copy2(Path(args.github_fixture).resolve(), git_fixture)
        shutil.copy2(Path(args.github_fixture).resolve(), gh_fixture)
        if os.name != "nt":
            git_fixture.chmod(0o700)
            gh_fixture.chmod(0o700)

        runner_env = dict(
            os.environ,
            HOME=str(git_home),
            USERPROFILE=str(git_home),
            ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_REAL_GIT=str(real_git.resolve()),
            ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_REMOTE=str(remote.resolve()),
            ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_STATE=str(fixture_state.resolve()),
            ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_SLUG="owner/project",
            ASSEMBLYWRIGHT_DEVELOPER_GITHUB_FIXTURE_MODE=str(mode.resolve()),
        )
        runner_env.pop("GH_TOKEN", None)
        runner_env.pop("GITHUB_TOKEN", None)
        command = [
            str(Path(args.binary).resolve()),
            "--data-dir", str(data),
            "--workspace-root", str(projects),
            "--bind", f"127.0.0.1:{port}",
            "--model-url", f"http://127.0.0.1:{model.server_port}/v1",
            "--git-executable", str(git_fixture),
            "--gh-executable", str(gh_fixture),
        ] + reviewer_arguments(root)
        log_path = root / "runner.log"
        process = None
        log_handles = []
        token = ""

        def start_runner():
            nonlocal process
            output = log_path.open("ab")
            log_handles.append(output)
            process = subprocess.Popen(
                command, env=runner_env, stdin=subprocess.DEVNULL, stdout=output, stderr=output,
            )

        def stop_runner():
            nonlocal process
            if process is not None and process.poll() is None:
                process.terminate()
                process.wait(timeout=10)
            process = None

        def request(path="github", body=None, authorization=True, timeout=60):
            headers = {"Content-Type": "application/json"}
            if authorization:
                headers["Authorization"] = "Bearer " + token
            call = urllib.request.Request(
                f"http://127.0.0.1:{port}/{path}",
                data=json.dumps(body).encode() if body is not None else None,
                headers=headers,
            )
            try:
                return json.load(urllib.request.urlopen(call, timeout=timeout))
            except urllib.error.HTTPError as error:
                response = error.read(16 * 1024)
                error.fp = io.BytesIO(response)
                error.response_body = response
                error.add_note("bounded response: " + response.decode(errors="replace"))
                raise

        def rejected(path, body, code=409, authorization=True):
            try:
                request(path, body, authorization=authorization)
                raise AssertionError("mutation unexpectedly succeeded")
            except urllib.error.HTTPError as error:
                assert error.code == code, error.code
                response = json.loads(error.response_body)
                assert isinstance(response.get("error"), str) and response["error"]
                assert "token" not in response["error"].lower()
                return response

        def wait_github(predicate, timeout=30):
            deadline = time.monotonic() + timeout
            value = None
            while time.monotonic() < deadline:
                try:
                    value = request()
                    if predicate(value):
                        return value
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(0.05)
            raise AssertionError(
                "Timed out waiting for GitHub setup: " + json.dumps(value)
                + "\n" + log_path.read_text(errors="replace")
            )

        def post(action, current=None, **values):
            current = current or request()
            return request("github", dict(
                action=action, expected_revision=current["revision"], **values,
            ))

        def set_mode(value):
            mode.write_text(value)

        def fixture_calls():
            calls_path = fixture_state.with_suffix(".calls")
            if not calls_path.exists():
                return []
            return [json.loads(line) for line in calls_path.read_text().splitlines()]

        def wait_sign_in(operation, state):
            return wait_github(lambda value: (value.get("sign_in") or {}).get("operation_id") == operation
                and value["sign_in"]["state"] == state)

        def begin_waiting_sign_in(fixture_mode):
            set_mode(fixture_mode)
            current = request()
            operation = str(uuid.uuid4())
            accepted = post("begin_sign_in", current, operation_id=operation)
            assert accepted["revision"] > current["revision"]
            assert accepted["sign_in"]["operation_id"] == operation
            assert accepted["sign_in"]["state"] in ("starting", "waiting")
            waiting = wait_sign_in(operation, "waiting")
            assert waiting["sign_in"]["user_code"] == "ABCD-9XYZ"
            assert waiting["sign_in"]["verification_url"] == "https://github.com/login/device"
            return operation, waiting

        failure = None
        failure_traceback = None
        try:
            start_runner()
            deadline = time.monotonic() + 20
            while not (data / "developer-token").exists() and time.monotonic() < deadline:
                time.sleep(0.05)
            token = (data / "developer-token").read_text().strip()
            initial = wait_github(lambda value: value["account"]["state"] == "unknown")
            assert initial["repository_page"] == 0 and initial["repositories"] == []

            rejected("github", {"action": "refresh_account", "expected_revision": initial["revision"]},
                code=401, authorization=False)
            rejected("github", {"action": "refresh_account", "expected_revision": initial["revision"] + 50})
            rejected("github", {"action": "unknown", "expected_revision": initial["revision"]})

            set_mode("setup-expired")
            expired = post("refresh_account")
            assert expired["account"]["state"] == "signed_out"
            assert expired["account"]["login"] is None
            assert "401" not in expired["account"]["message"]

            operation, waiting = begin_waiting_sign_in("setup-sign-in-wait")
            rejected("github", {
                "action": "refresh_account", "expected_revision": waiting["revision"],
            })
            cancelled = post("cancel_sign_in", waiting, operation_id=operation)
            assert cancelled["sign_in"]["operation_id"] == operation
            assert cancelled["sign_in"]["state"] == "cancelled"
            assert cancelled["account"]["state"] == "signed_out"
            assert cancelled["sign_in"]["user_code"] is None

            operation, waiting = begin_waiting_sign_in("setup-sign-in-wait")
            stop_runner()
            start_runner()
            attention = wait_sign_in(operation, "attention")
            status = request("status")
            assert status["github_setup_unresolved"] is True
            assert attention["sign_in"]["user_code"] is None
            assert attention["sign_in"]["verification_url"] is None
            set_mode("setup-signed-out")
            reconciled = post("reconcile_sign_in", attention, operation_id=operation)
            assert reconciled["sign_in"]["state"] == "failed"
            assert reconciled["account"]["state"] == "signed_out"
            assert request("status")["github_setup_unresolved"] is False

            set_mode("setup-sign-in-malformed")
            malformed_id = str(uuid.uuid4())
            malformed = post("begin_sign_in", operation_id=malformed_id)
            assert malformed["sign_in"]["operation_id"] == malformed_id
            malformed = wait_github(lambda value: (value.get("sign_in") or {}).get("operation_id") == malformed_id
                and value["sign_in"]["state"] in ("failed", "attention"))
            assert malformed["sign_in"]["user_code"] is None
            assert malformed["sign_in"]["verification_url"] is None
            if malformed["sign_in"]["state"] == "attention":
                set_mode("setup-signed-out")
                malformed = post("reconcile_sign_in", malformed, operation_id=malformed_id)
                assert malformed["sign_in"]["state"] == "failed"

            set_mode("setup-auth-unavailable")
            unavailable = post("refresh_account")
            assert unavailable["account"]["state"] == "unavailable"
            assert unavailable["account"]["login"] is None
            set_mode("setup-expired")
            assert post("refresh_account")["account"]["state"] == "signed_out"

            operation, waiting = begin_waiting_sign_in("setup-sign-in-saved-then-wait")
            postcheck = post("cancel_sign_in", waiting, operation_id=operation)
            assert postcheck["sign_in"]["state"] == "succeeded"
            assert postcheck["account"]["state"] == "signed_in"
            assert postcheck["account"]["login"].lower() == "owner"

            set_mode("setup-expired")
            assert post("refresh_account")["account"]["state"] == "signed_out"
            stop_runner()
            runner_env["GH_TOKEN"] = "fixture-ambient-token"
            set_mode("setup-sign-in-success")
            start_runner()
            masked_current = wait_github(lambda value: value["account"]["state"] == "signed_out")
            masked_id = str(uuid.uuid4())
            masked = post("begin_sign_in", masked_current, operation_id=masked_id)
            assert masked["sign_in"]["operation_id"] == masked_id
            masked = wait_sign_in(masked_id, "attention")
            assert masked["account"]["state"] == "signed_in"
            assert masked["account"]["login"] == "ambient"
            still_masked = post("reconcile_sign_in", masked, operation_id=masked_id)
            assert still_masked["sign_in"]["state"] == "attention"
            assert still_masked["account"]["login"] == "ambient"
            stop_runner()
            runner_env.pop("GH_TOKEN")
            start_runner()
            unmasked_current = wait_github(lambda value: (value.get("sign_in") or {}).get("state") == "attention")
            unmasked = post("reconcile_sign_in", unmasked_current, operation_id=masked_id)
            assert unmasked["sign_in"]["state"] == "succeeded"
            assert unmasked["account"]["state"] == "signed_in"
            assert unmasked["account"]["login"] == "owner"

            set_mode("setup-pages")
            page_one = post("list_repositories", page=1)
            assert page_one["repository_page"] == 1 and page_one["has_more"] is True
            empty = next(item for item in page_one["repositories"] if item["default_branch"] == "")
            assert empty["url"].startswith("https://github.com/owner/")
            assert empty["can_push"] is True
            page_one_names = {item["name_with_owner"] for item in page_one["repositories"]}
            page_two = post("list_repositories", page_one, page=2)
            assert page_two["repository_page"] == 2
            assert len(page_two["repositories"]) == 1 and not page_one_names.intersection(
                item["name_with_owner"] for item in page_two["repositories"]
            )

            set_mode("setup-create-collision")
            collision_id = str(uuid.uuid4())
            collision = post("create_repository", operation_id=collision_id,
                expected_login="owner", name="collision", visibility="private")
            assert collision["creation"]["operation_id"] == collision_id
            collision = wait_github(lambda value: (value.get("creation") or {}).get("state") == "existing")
            assert collision["creation"]["name_with_owner"].lower() == "owner/collision"

            set_mode("setup-create-ambiguous")
            ambiguous_id = str(uuid.uuid4())
            ambiguous = post("create_repository", operation_id=ambiguous_id,
                expected_login="owner", name="ambiguous", visibility="private")
            assert ambiguous["creation"]["operation_id"] == ambiguous_id
            ambiguous = wait_github(lambda value: (value.get("creation") or {}).get("state") == "attention")
            ambiguous_receipt = ambiguous["creation"].get("repository_id")
            assert isinstance(ambiguous_receipt, str) and ambiguous_receipt.isdecimal()
            assert request("status")["github_setup_unresolved"] is True
            set_mode("setup-create-absent")
            absent = post("reconcile_creation", ambiguous, operation_id=ambiguous_id)
            assert absent["creation"]["state"] == "absent"
            assert absent["creation"]["operation_id"] == ambiguous_id
            assert absent["creation"]["repository_url"] is None
            assert request("status")["github_setup_unresolved"] is False

            set_mode("setup-create-success")
            success_id = str(uuid.uuid4())
            success = post("create_repository", operation_id=success_id,
                expected_login="owner", name="created", visibility="private")
            assert success["creation"]["operation_id"] == success_id
            success = wait_github(lambda value: (value.get("creation") or {}).get("state") == "succeeded")
            receipt = success["creation"]
            assert receipt["name_with_owner"].lower() == "owner/created"
            assert receipt["repository_url"].lower() == "https://github.com/owner/created"
            assert receipt["repository_id"] == "9001"
            assert receipt["default_branch"] == "main"
            assert receipt["visibility"] == "private"

            set_mode("setup-create-redirected")
            redirected_id = str(uuid.uuid4())
            redirected = post("create_repository", operation_id=redirected_id,
                expected_login="owner", name="redirected", visibility="private")
            assert redirected["creation"]["operation_id"] == redirected_id
            redirected = wait_github(lambda value: (value.get("creation") or {}).get("state") == "attention")
            assert redirected["creation"]["name_with_owner"].lower() == "owner/redirected"
            assert redirected["creation"]["repository_url"] is None
            set_mode("setup-create-absent")
            redirected = post("reconcile_creation", redirected, operation_id=redirected_id)
            assert redirected["creation"]["state"] == "absent"

            set_mode("setup-create-saved-then-wait")
            interrupted_id = str(uuid.uuid4())
            interrupted = post("create_repository", operation_id=interrupted_id,
                expected_login="owner", name="interrupted", visibility="private")
            assert interrupted["creation"]["operation_id"] == interrupted_id
            wait_github(lambda value: sum(
                call["tool"] == "gh"
                and call["arguments"][:3] == ["repo", "create", "owner/interrupted"]
                for call in fixture_calls()
            ) == 1)
            stop_runner()
            set_mode("setup-signed-in")
            start_runner()
            interrupted = wait_github(lambda value: (value.get("creation") or {}).get("operation_id") == interrupted_id
                and value["creation"]["state"] == "attention")
            interrupted = post("reconcile_creation", interrupted, operation_id=interrupted_id)
            assert interrupted["creation"]["state"] == "attention"
            assert interrupted["creation"]["repository_url"].lower() == "https://github.com/owner/interrupted"
            assert interrupted["creation"]["repository_id"] == "9003"

            calls = fixture_calls()
            create_calls = [call for call in calls
                if call["tool"] == "gh" and call["arguments"][:2] == ["repo", "create"]]
            assert not any(call["arguments"][2].lower() == "owner/collision" for call in create_calls)
            assert not any(call["arguments"][2].lower() == "owner/redirected" for call in create_calls)
            assert sum(call["arguments"][2].lower() == "owner/interrupted" for call in create_calls) == 1
            assert all("--add-readme" in call["arguments"] for call in create_calls)
            assert all("--source" not in call["arguments"] and "--push" not in call["arguments"]
                for call in create_calls)
            assert all(str(projects) not in json.dumps(call) and marker.name not in json.dumps(call)
                for call in calls)
            assert marker.read_text() == "PROJECT-CONTENTS-MUST-STAY-LOCAL\n"

            print(json.dumps({
                "native_platform": sys.platform,
                "authenticated_runner_http": True,
                "stale_and_unknown_actions_rejected": True,
                "signed_out_diagnostic": True,
                "malformed_challenge_and_unavailable_account": True,
                "ambient_credentials_reconciled": True,
                "device_challenge_cancel_and_postcheck": True,
                "restart_attention_reconciled": True,
                "repository_pages_and_empty_default_branch": True,
                "creation_collision_ambiguous_absent_and_success": True,
                "creation_receipt_immutable_id": True,
                "redirected_identity_rejected_without_create": True,
                "creation_restart_reconciled_without_replay": True,
                "project_contents_uploaded": False,
                "live_github_credentials_used": False,
            }))
        except BaseException as error:
            failure = error
            failure_traceback = error.__traceback__
        finally:
            try:
                set_mode("setup-signed-in")
                request("control", {"action": "shutdown"})
            except Exception:
                pass
            stop_runner()
            for handle in log_handles:
                handle.close()
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
