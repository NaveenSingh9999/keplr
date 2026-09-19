"""Tiny JSON API demo — edit me, then watch keplr's git panel light up."""

import json
from http.server import BaseHTTPRequestHandler, HTTPServer

HOST, PORT = "127.0.0.1", 8123

USERS = {
    "ada": {"name": "Ada Lovelace", "role": "admin"},
    "grace": {"name": "Grace Hopper", "role": "member"},
}


class Handler(BaseHTTPRequestHandler):
    def _send(self, payload, status=200):
        body = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):  # noqa: N802 (http.server convention)
        if self.path == "/users":
            self._send(sorted(USERS))
        elif self.path.startswith("/users/"):
            user = USERS.get(self.path.rsplit("/", 1)[-1])
            self._send(user or {"error": "not found"}, 200 if user else 404)
        else:
            self._send({"error": "try /users"}, 404)

    def log_message(self, *args):  # keep demo output clean
        pass


if __name__ == "__main__":
    print(f"serving demo API on http://{HOST}:{PORT} (Ctrl-C to stop)")
    HTTPServer((HOST, PORT), Handler).serve_forever()
