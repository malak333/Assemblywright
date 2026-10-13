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
                prompt = request["messages"][-1]["content"]
                if "src/slugify.py" in prompt and "src/cli.py" in prompt:
                    content = {"edits": [
                        {"path": "src/slugify.py", "content": "import re\nimport unicodedata\n\n\ndef slugify(text, max_length=80):\n    if not isinstance(text, str):\n        raise TypeError(\"text must be a string\")\n    if isinstance(max_length, bool) or not isinstance(max_length, int) or max_length <= 0:\n        raise ValueError(\"max_length must be a positive integer\")\n    normalized = unicodedata.normalize(\"NFKD\", text).encode(\"ascii\", \"ignore\").decode(\"ascii\").lower()\n    value = re.sub(r\"[^a-z0-9]+\", \"-\", normalized).strip(\"-\")\n    return value[:max_length].rstrip(\"-\")\n"},
                        {"path": "src/cli.py", "content": "import argparse\n\nfrom slugify import slugify\n\n\ndef positive_integer(value):\n    try:\n        number = int(value)\n    except ValueError:\n        raise argparse.ArgumentTypeError(\"must be a positive integer\")\n    if number <= 0:\n        raise argparse.ArgumentTypeError(\"must be a positive integer\")\n    return number\n\n\nparser = argparse.ArgumentParser()\nparser.add_argument(\"text\")\nparser.add_argument(\"--max-length\", type=positive_integer, default=80)\narguments = parser.parse_args()\nprint(slugify(arguments.text, arguments.max_length))\n"},
                    ]}
                else:
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
