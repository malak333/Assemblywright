#!/usr/bin/env python3
"""Native HTTP/process proof for durable, project-grouped Developer chat history.

The runner and both model endpoints are disposable fixtures. The test does not
touch an owner project, account, queue entry, or external model service.
"""
from developer_review_fixture import reviewer_arguments

import argparse
import base64
from contextlib import closing
import hashlib
import http.server
import json
from pathlib import Path
import socket
import sqlite3
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def text_attachment(name, contents):
    return {
        "name": name,
        "media_type": "text/plain",
        "data_base64": base64.b64encode(contents.encode()).decode(),
    }


def compact_json(value):
    """Match serde_json's deterministic map-key ordering for binding fixtures."""
    return json.dumps(value, separators=(",", ":"), sort_keys=True)


def sha256_json(value):
    return hashlib.sha256(compact_json(value).encode()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    binary = str(Path(parser.parse_args().binary).resolve())
    fixture = {"block": False, "calls": []}
    entered = threading.Event()
    release = threading.Event()

    class Model(http.server.BaseHTTPRequestHandler):
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

        def do_GET(self):
            if self.path == "/props":
                return self.reply({"total_slots": 1, "n_ctx": 262144,
                    "modalities": {"vision": False},
                    "default_generation_settings": {"n_ctx": 262144}})
            self.reply({"error": "unexpected route"}, 404)

        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            fixture["calls"].append((self.path, request))
            if self.path == "/apply-template":
                return self.reply({"prompt": json.dumps(request["messages"])})
            if self.path == "/tokenize":
                return self.reply({"count": max(1, len(request["content"]) // 4)})
            if self.path != "/v1/chat/completions":
                return self.reply({"error": "unexpected route"}, 404)
            if fixture["block"]:
                entered.set()
                release.wait(15)
            messages = request["messages"]
            last = next(message for message in reversed(messages)
                        if message.get("role") == "user")
            content = last.get("content")
            if isinstance(content, list):
                content = next(part.get("text", "") for part in content
                               if part.get("type") == "text")
            marker = str(content).split()[0] if str(content).split() else "attachment"
            return self.reply({"choices": [{"message": {
                "content": "fixture reply for " + marker, "tool_calls": None},
                "finish_reason": "stop"}]})

        def log_message(self, *unused):
            pass

    windows = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
    mac = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Model)
    for server in (windows, mac):
        server.daemon_threads = True
        threading.Thread(target=server.serve_forever, daemon=True).start()

    with tempfile.TemporaryDirectory(prefix="assemblywright-chat-history-e2e-") as temporary:
        root = Path(temporary)
        projects = root / "projects"
        for project in ("alpha", "beta"):
            (projects / project).mkdir(parents=True)
        (projects / "alpha" / "README.md").write_text("ALPHA_PROJECT_ONLY\n")
        (projects / "beta" / "README.md").write_text("BETA_PROJECT_ONLY\n")
        state = root / "state"
        log = root / "runner.log"
        process = None
        listen = free_port()
        token = ""
        legacy_attachment = text_attachment("legacy-notes.txt", "LEGACY_ATTACHMENT_EVIDENCE")
        legacy_request_id = str(uuid.uuid4())
        legacy_message = "LEGACY_ALPHA first"
        legacy_response = "retained legacy response"
        legacy_content_sha256 = hashlib.sha256(legacy_response.encode()).hexdigest()
        legacy_provenance_v1 = sha256_json({"project": "alpha",
            "request_id": legacy_request_id, "model_target": "windows",
            "model": "windows-fixture", "content": legacy_response})
        legacy_state = {"messages": [
            {"role": "user", "content": legacy_message,
             "attachments": [legacy_attachment], "request_id": legacy_request_id,
             "model_target": "windows"},
            {"role": "assistant", "content": legacy_response, "attachments": [],
             "request_id": legacy_request_id, "model_target": "windows",
             "model": "windows-fixture", "content_sha256": legacy_content_sha256,
             "provenance_sha256": legacy_provenance_v1}],
            "request_id": legacy_request_id, "error": None, "context_limit": 262144,
            "context_tokens": 64, "context_files": ["README.md"], "omitted_files": 0,
            "omitted_messages": 0, "history_omitted": 0, "pending_request_id": None,
            "selected_model_target": "windows"}
        legacy_state_json = compact_json(legacy_state)
        legacy_request_sha256 = sha256_json({"project": "alpha", "message": legacy_message,
            "attachments": [legacy_attachment]})
        state.mkdir()
        with closing(sqlite3.connect(state / "developer.sqlite3")) as database, database:
            database.executescript(
                "CREATE TABLE developer_chat_project("
                "project TEXT PRIMARY KEY,state TEXT NOT NULL);"
                "CREATE TABLE developer_chat_request("
                "id TEXT PRIMARY KEY,project TEXT NOT NULL,message TEXT NOT NULL,"
                "model_target TEXT NOT NULL DEFAULT 'windows',"
                "pending INTEGER NOT NULL DEFAULT 0 CHECK(pending IN(0,1)),"
                "payload_sha256 TEXT);")
            database.execute("INSERT INTO developer_chat_project(project,state) VALUES(?,?)",
                ("alpha", legacy_state_json))
            database.execute(
                "INSERT INTO developer_chat_request("
                "id,project,message,model_target,pending,payload_sha256) VALUES(?,?,?,?,0,?)",
                (legacy_request_id, "alpha", legacy_message, "windows", legacy_request_sha256))

        legacy_chat_id = "legacy-" + hashlib.sha256(
            b"assemblywright-developer-legacy-chat-v1\0alpha").hexdigest()

        def verify_legacy_database():
            with closing(sqlite3.connect(state / "developer.sqlite3")) as database, database:
                backed_up_state = database.execute(
                    "SELECT state FROM developer_chat_project_v1_backup WHERE project='alpha'"
                ).fetchone()
                assert backed_up_state == (legacy_state_json,)
                backed_up_request = database.execute(
                    "SELECT id,project,message,model_target,pending,payload_sha256,"
                    "chat_id,reserved_bytes FROM developer_chat_request_v1_backup WHERE id=?",
                    (legacy_request_id,)).fetchone()
                assert backed_up_request == (legacy_request_id, "alpha", legacy_message,
                    "windows", 0, legacy_request_sha256, None, 0)
                migrated_request = database.execute(
                    "SELECT project,chat_id,message,pending,payload_sha256 "
                    "FROM developer_chat_request WHERE id=?", (legacy_request_id,)).fetchone()
                assert migrated_request == ("alpha", legacy_chat_id, legacy_message, 0,
                    legacy_request_sha256)

        def api(path="status", body=None, authenticated=True, timeout=10):
            request = urllib.request.Request(
                f"http://127.0.0.1:{listen}/{path}",
                data=None if body is None else json.dumps(body).encode(),
                headers={"Content-Type": "application/json", "Authorization":
                         "Bearer " + (token if authenticated else "invalid")})
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return json.load(response)

        def rejected(path, body=None, authenticated=True):
            try:
                api(path, body, authenticated)
            except urllib.error.HTTPError as error:
                assert error.code in (400, 401, 409, 422), (
                    path, error.code, error.read().decode(errors="replace"))
                return
            raise AssertionError("Unexpected acceptance: " + path)

        def wait(path, predicate, timeout=20):
            deadline = time.monotonic() + timeout
            last = None
            while time.monotonic() < deadline:
                try:
                    last = api(path)
                    if predicate(last):
                        return last
                except (OSError, urllib.error.URLError):
                    pass
                if process is not None and process.poll() is not None:
                    break
                time.sleep(.04)
            raise AssertionError(
                f"Timed out waiting for {path}: {last}\n"
                + log.read_text(errors="replace")[-6000:])

        def launch():
            nonlocal process, listen, token
            listen = free_port()
            command = [binary, "--data-dir", str(state), "--workspace-root", str(projects),
                "--bind", f"127.0.0.1:{listen}", "--model-url",
                f"http://127.0.0.1:{mac.server_port}/v1", "--model", "mac-fixture",
                "--windows-model-url", f"http://127.0.0.1:{windows.server_port}/v1",
                "--windows-model", "windows-fixture", *reviewer_arguments(root)]
            output = log.open("ab")
            process = subprocess.Popen(command, stdin=subprocess.DEVNULL,
                stdout=output, stderr=output)
            output.close()
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline and not (state / "developer-token").exists():
                if process.poll() is not None:
                    raise AssertionError(log.read_text(errors="replace"))
                time.sleep(.04)
            token = (state / "developer-token").read_text().strip()
            wait("status", lambda value: not value["running"])

        def terminate():
            if process is not None and process.poll() is None:
                process.terminate()
                process.wait(timeout=5)

        def file_fingerprints():
            return {str(path.relative_to(projects)): hashlib.sha256(path.read_bytes()).hexdigest()
                    for path in projects.rglob("*") if path.is_file()}

        def create(project, creation_id=None, reuse_chat_id=None):
            body = {"id": creation_id or str(uuid.uuid4()), "project": project}
            if reuse_chat_id is not None:
                body["reuse_chat_id"] = reuse_chat_id
            return body, api("chat/conversations", body)

        def rename(project, snapshot, title):
            return api("chat/rename", {"project": project, "chat_id": snapshot["chat_id"],
                "title": title, "expected_revision": snapshot["revision"]})

        def send(project, chat_id, message, attachments=None, request_id=None):
            body = {"project": project, "chat_id": chat_id, "message": message,
                "id": request_id or str(uuid.uuid4()), "attachments": attachments or [],
                "model_target": "windows"}
            return body, api("chat", body)

        def complete(project, chat_id):
            path = "chat?" + urllib.parse.urlencode({"project": project, "chat_id": chat_id})
            return wait(path, lambda value: not value["running"])

        def read(project, chat_id, before=None):
            query = {"project": project, "chat_id": chat_id}
            if before is not None:
                query["before"] = before
            return api("chat?" + urllib.parse.urlencode(query))

        def generation_prompts():
            return [json.dumps(body["messages"]) for path, body in fixture["calls"]
                    if path == "/v1/chat/completions"]

        try:
            launch()
            rejected("chat/conversations?project=", authenticated=False)
            before_files = file_fingerprints()
            before_queue = api()["queue"]

            # A genuine pre-history database migrates to one Previous conversation.
            verify_legacy_database()
            migrated_rows = api("chat/conversations?project=alpha")["conversations"]
            assert migrated_rows == [{"id": legacy_chat_id, "project": "alpha",
                "title": "Previous conversation", "revision": 1,
                "updated_at": migrated_rows[0]["updated_at"]}]
            legacy = read("alpha", legacy_chat_id)
            assert legacy["history_supported"] is True
            assert legacy["messages"][-2]["attachments"] == [legacy_attachment]
            assert legacy["messages"][-1]["content"] == legacy_response
            assert legacy["messages"][-1]["content_sha256"] == legacy_content_sha256
            assert legacy["messages"][-1]["provenance_sha256"] == legacy_provenance_v1
            assert [message["sequence"] for message in legacy["messages"]] == [1, 2]
            legacy_request = {"project": "alpha", "message": legacy_message,
                "id": legacy_request_id, "attachments": [legacy_attachment],
                "model_target": "windows"}
            model_calls = len(generation_prompts())
            assert api("chat", legacy_request)["chat_id"] == legacy_chat_id
            assert len(generation_prompts()) == model_calls
            legacy = rename("alpha", legacy, "Legacy setup")

            # Repeated New Chat presses atomically reuse a selected pristine chat.
            create_id = str(uuid.uuid4())
            create_request, second_alpha = create("alpha", create_id)
            replay = api("chat/conversations", create_request)
            assert replay["chat_id"] == second_alpha["chat_id"]
            reused_request, reused = create("alpha", reuse_chat_id=second_alpha["chat_id"])
            assert reused["chat_id"] == second_alpha["chat_id"]
            rejected("chat/conversations", dict(reused_request, project="beta"))
            rejected("chat/conversations", dict(create_request, reuse_chat_id=legacy_chat_id))
            second_alpha = rename("alpha", second_alpha, "Implementation notes")
            _, beta = create("beta")
            beta = rename("beta", beta, "Beta research")

            request_a, _ = send("alpha", second_alpha["chat_id"], "ALPHA_TWO private")
            alpha_answer = complete("alpha", second_alpha["chat_id"])
            request_b, _ = send("beta", beta["chat_id"], "BETA_ONE private")
            beta_answer = complete("beta", beta["chat_id"])
            _, _ = send("alpha", second_alpha["chat_id"], "ALPHA_FOLLOWUP context")
            alpha_followup = complete("alpha", second_alpha["chat_id"])
            prompts = generation_prompts()
            assert "ALPHA_TWO" in prompts[-1] and "BETA_ONE" not in prompts[-1]
            assert "LEGACY_ALPHA" not in prompts[-1]
            assert all(message["request_id"] != request_b["id"]
                       for message in alpha_followup["messages"])
            assert all(message["request_id"] != request_a["id"]
                       for message in beta_answer["messages"])

            # Tool evidence and approvals remain visible only in their exact chat.
            tool_action_id = str(uuid.uuid4())
            tool_request_id = str(uuid.uuid4())
            access_revision = alpha_followup["tool_access"]["revision"]
            with closing(sqlite3.connect(state / "developer.sqlite3")) as database, database:
                database.execute(
                    "INSERT INTO developer_tool_action("
                    "id,request_id,project,chat_id,access_revision,tool,summary,details,"
                    "status,output,updated_unix,feature_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
                    (tool_action_id, tool_request_id, "alpha", second_alpha["chat_id"],
                     access_revision, "bash", "chat A only",
                     compact_json({"opencode_permission_id": "fixture"}),
                     "pending_approval", None, 1, None))
            tool_chat = read("alpha", second_alpha["chat_id"])
            other_chat = read("alpha", legacy_chat_id)
            assert [action["id"] for action in tool_chat["tool_actions"]] == [tool_action_id]
            assert tool_chat["pending_approval"]["id"] == tool_action_id
            assert tool_chat["pending_approval"]["chat_id"] == second_alpha["chat_id"]
            assert other_chat["tool_actions"] == []
            assert other_chat["pending_approval"] is None
            rejected("chat/approval", {"project": "alpha", "chat_id": legacy_chat_id,
                "request_id": tool_request_id, "approval_id": tool_action_id,
                "access_revision": access_revision, "decision": "approve"})
            assert read("alpha", second_alpha["chat_id"])["pending_approval"]["id"] == tool_action_id
            with closing(sqlite3.connect(state / "developer.sqlite3")) as database, database:
                database.execute("DELETE FROM developer_tool_action WHERE id=?", (tool_action_id,))

            all_rows = api("chat/conversations?project=")
            rows = all_rows["conversations"]
            assert len(rows) == 3 and all_rows["next_cursor"] is None
            assert {(row["project"], row["id"]) for row in rows} == {
                ("alpha", legacy_chat_id), ("alpha", second_alpha["chat_id"]),
                ("beta", beta["chat_id"])}
            assert all(isinstance(row["updated_at"], int) and isinstance(row["revision"], int)
                       and row["title"] for row in rows)
            alpha_rows = api("chat/conversations?project=alpha")["conversations"]
            assert len(alpha_rows) == 2 and all(row["project"] == "alpha" for row in alpha_rows)
            rejected("chat?" + urllib.parse.urlencode(
                {"project": "beta", "chat_id": second_alpha["chat_id"]}))

            # Browsing and creating chats are effect-free for project files and queue state.
            navigation_files, navigation_queue = file_fingerprints(), api()["queue"]
            read("alpha", legacy_chat_id)
            _, pristine_beta = create("beta")
            assert file_fingerprints() == navigation_files
            assert api()["queue"] == navigation_queue

            # Persist more than one message page while keeping model context bounded.
            for index in range(26):
                send("alpha", second_alpha["chat_id"], f"PAGE_{index:02d} bounded")
                page_result = complete("alpha", second_alpha["chat_id"])
                assert not page_result["error"], page_result
            first_page = read("alpha", second_alpha["chat_id"])
            assert len(first_page["messages"]) == 50 and first_page["next_before"]
            assert all(isinstance(message["sequence"], int) for message in first_page["messages"])
            older_page = read("alpha", second_alpha["chat_id"], first_page["next_before"])
            assert older_page["messages"] and not ({m["sequence"] for m in first_page["messages"]}
                & {m["sequence"] for m in older_page["messages"]})
            assert min(m["sequence"] for m in first_page["messages"]) > max(
                m["sequence"] for m in older_page["messages"])
            assert "ALPHA_TWO" not in generation_prompts()[-1]

            # One active reply may be browsed around, while all further work stays bound.
            fixture["block"] = True
            entered.clear(); release.clear()
            active_request, active = send("alpha", second_alpha["chat_id"], "BLOCK_ACTIVE wait")
            active_binding = active["active_chat"]
            assert active_binding["project"] == "alpha"
            assert active_binding["chat_id"] == second_alpha["chat_id"]
            assert active_binding["request_id"] == active_request["id"]
            if "title" in active_binding:
                assert active_binding["title"] == "Implementation notes"
            assert entered.wait(5)
            browsed = read("beta", beta["chat_id"])
            assert browsed["active_chat"]["chat_id"] == second_alpha["chat_id"]
            _, during_active = create("beta", reuse_chat_id=pristine_beta["chat_id"])
            assert during_active["chat_id"] == pristine_beta["chat_id"]
            rejected("chat", {"project": "beta", "chat_id": beta["chat_id"],
                "message": "SECOND_SEND rejected", "id": str(uuid.uuid4()),
                "attachments": [], "model_target": "windows"})
            rejected("chat/cancel", {"project": "alpha", "chat_id": legacy_chat_id,
                "id": active_request["id"]})
            rejected("chat/cancel", {"project": "alpha", "chat_id": second_alpha["chat_id"],
                "id": str(uuid.uuid4())})
            api("chat/cancel", {"project": "alpha", "chat_id": second_alpha["chat_id"],
                "id": active_request["id"]})
            release.set()
            cancelled = complete("alpha", second_alpha["chat_id"])
            assert cancelled["error"]
            fixture["block"] = False

            # Conversation browsing itself is fixed at 50 rows with an opaque cursor.
            for _ in range(47):
                create("beta")
            conversation_page = api("chat/conversations?project=")
            assert len(conversation_page["conversations"]) == 50
            assert isinstance(conversation_page["next_cursor"], str)
            following_page = api("chat/conversations?" + urllib.parse.urlencode(
                {"project": "", "cursor": conversation_page["next_cursor"]}))
            assert len(following_page["conversations"]) == 1
            assert following_page["next_cursor"] is None
            assert not ({row["id"] for row in conversation_page["conversations"]}
                & {row["id"] for row in following_page["conversations"]})

            # Request replay cannot move across chats or projects.
            call_count = len(generation_prompts())
            replayed = api("chat", request_a)
            assert replayed["chat_id"] == second_alpha["chat_id"]
            assert len(generation_prompts()) == call_count
            rejected("chat", dict(request_a, chat_id=legacy_chat_id))
            rejected("chat", dict(request_a, project="beta", chat_id=beta["chat_id"]))
            rejected("chat/approval", {"project": "alpha", "chat_id": legacy_chat_id,
                "request_id": request_a["id"], "approval_id": str(uuid.uuid4()),
                "access_revision": legacy["tool_access"]["revision"], "decision": "approve"})

            terminate(); launch()
            verify_legacy_database()
            durable_legacy = read("alpha", legacy_chat_id)
            durable_alpha = read("alpha", second_alpha["chat_id"])
            durable_beta = read("beta", beta["chat_id"])
            assert durable_legacy["title"] == "Legacy setup"
            assert durable_alpha["title"] == "Implementation notes"
            assert durable_beta["title"] == "Beta research"
            assert any(message["attachments"] == [legacy_attachment]
                       for message in durable_legacy["messages"])
            assert any(message.get("provenance_sha256")
                       for message in durable_legacy["messages"])
            assert durable_legacy["tool_access"]["mode"] == legacy["tool_access"]["mode"]
            assert any(message["request_id"] == request_b["id"]
                       for message in durable_beta["messages"])
            send("beta", beta["chat_id"], "BETA_CONTINUE restart")
            continued = complete("beta", beta["chat_id"])
            assert continued["messages"][-1]["content"] == "fixture reply for BETA_CONTINUE"
            assert file_fingerprints() == before_files and api()["queue"] == before_queue
            print("PASS: durable grouped chat creation, isolation, paging, restart, old-schema "
                  "migration/backups, attachments/provenance, effect-free browsing, and "
                  "active-chat bindings")
        finally:
            release.set()
            terminate()
            for server in (windows, mac):
                server.shutdown()
                server.server_close()


if __name__ == "__main__":
    main()
