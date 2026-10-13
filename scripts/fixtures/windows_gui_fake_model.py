"""Loopback fixture for native GUI-to-runner E2E; never used by the product."""

import argparse
import json
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port-file", required=True, type=Path)
    parser.add_argument("--requests-file", required=True, type=Path)
    args = parser.parse_args()

    class ModelHandler(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path.rstrip('/') != '/v1/models':
                self.send_error(404)
                return
            body = json.dumps({'object': 'list', 'data': [
                {'id': 'gui-fixture', 'object': 'model'},
                {'id': 'gui-review-fixture', 'object': 'model'},
            ]}).encode('utf-8')
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self):
            size = int(self.headers.get("Content-Length", "0"))
            request = json.loads(self.rfile.read(size))
            with args.requests_file.open("a", encoding="utf-8") as output:
                output.write(json.dumps(request) + "\n")
            system = request["messages"][0]["content"].lower()
            if "reviewer" in system:
                content = {"approve": True, "findings": []}
                reply = json.dumps(content)
            elif "coding worker" in system:
                content = {
                    "edits": [{"path": "feature.py", "content": "def answer():\n    return 42\n"}]
                }
                reply = json.dumps(content)
            else:
                if "__WAIT_FOR_CANCEL__" in request["messages"][-1]["content"]:
                    time.sleep(15)
                reply = "Implement answer() in feature.py to return the integer 42. Verify it with the existing acceptance test."
            body = json.dumps(
                {"choices": [{"message": {"content": reply}}]}
            ).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass  # A real chat cancellation may close the fixture connection.

        def log_message(self, *unused):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), ModelHandler)
    args.port_file.write_text(str(server.server_port), encoding="ascii")
    server.serve_forever()


if __name__ == "__main__":
    main()
