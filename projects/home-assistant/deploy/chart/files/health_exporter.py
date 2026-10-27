"""Operational TCP checks only; no commands, credentials, or conversation content."""
from http.server import BaseHTTPRequestHandler, HTTPServer
import os
import socket
import time

ENDPOINTS = {"ha": 8123, "music_assistant": 8095,
             "piper": 10200, "whisper": 10300, "gigaam": 10305,
             "hermes": 8642, "voice_proxy": 8643}


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != "/metrics":
            self.send_error(404)
            return
        lines = ["# HELP ricardo_endpoint_up Whether a configured TCP endpoint accepts connections.",
                 "# TYPE ricardo_endpoint_up gauge",
                 "# HELP ricardo_endpoint_connect_seconds TCP connection time, not voice latency.",
                 "# TYPE ricardo_endpoint_connect_seconds gauge"]
        for service, port in ENDPOINTS.items():
            start = time.monotonic()
            try:
                with socket.create_connection((os.environ.get(f"ENDPOINT_HOST_{service.upper()}", "127.0.0.1"), port), timeout=1):
                    up = 1
            except OSError:
                up = 0
            lines += [f'ricardo_endpoint_up{{endpoint="{service}"}} {up}',
                      f'ricardo_endpoint_connect_seconds{{endpoint="{service}"}} {time.monotonic()-start:.6f}']
        body = ("\n".join(lines) + "\n").encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/plain; version=0.0.4")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


if __name__ == "__main__":
    HTTPServer(("0.0.0.0", 9116), Handler).serve_forever()
