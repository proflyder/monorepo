#!/usr/bin/env python3
"""Read-only live checks for the Station BLE gateway HTTP control.

Requires the control token in STATION_CONTROL_TOKEN (never commit it):

    STATION_E2E=1 STATION_CONTROL_TOKEN=... \
      python3 projects/station/e2e/live_gateway.py -v
"""
from __future__ import annotations

import json
import os
import unittest
import urllib.error
import urllib.request

URL = os.environ.get("STATION_GATEWAY_URL", "http://192.0.2.13:8099").rstrip("/")
TOKEN = os.environ.get("STATION_CONTROL_TOKEN", "").strip()
E2E = os.environ.get("STATION_E2E") == "1"


def request(path: str, method: str = "GET", token: str | None = TOKEN) -> tuple[int, dict]:
    headers = {"Authorization": f"Bearer {token}"} if token else {}
    req = urllib.request.Request(URL + path, method=method, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=8) as resp:
            body = resp.read().decode()
            return resp.status, (json.loads(body) if body else {})
    except urllib.error.HTTPError as exc:
        return exc.code, {"error": exc.read().decode(errors="replace")[:300]}


@unittest.skipUnless(E2E and TOKEN, "set STATION_E2E=1 and STATION_CONTROL_TOKEN to run live gateway tests")
class GatewayTests(unittest.TestCase):
    def test_auth_required(self):
        code, _ = request("/status", token=None)
        self.assertEqual(code, 401)

    def test_status_shape(self):
        code, data = request("/status")
        self.assertEqual(code, 200, data)
        for key in ("status", "manual", "bluetooth", "wifi"):
            self.assertIn(key, data)
        self.assertIn(data["wifi"], {"COMPLETED", "DISCONNECTED", "SCANNING", "INACTIVE", "UNKNOWN"})

    def test_method_guards_do_not_mutate(self):
        code, _ = request("/status", method="POST")
        self.assertEqual(code, 405)
        code, _ = request("/gateway/on", method="GET")
        self.assertEqual(code, 405)
        code, _ = request("/gateway/off", method="GET")
        self.assertEqual(code, 405)
        code, _ = request("/bluetooth/on", method="GET")
        self.assertEqual(code, 405)
        code, _ = request("/bluetooth/off", method="GET")
        self.assertEqual(code, 405)


if __name__ == "__main__":
    unittest.main(verbosity=2)
