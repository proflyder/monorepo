#!/usr/bin/env python3
"""Opt-in live checks for the Hermes fallback API.

Read-only by default.  A real model turn is opt-in because it costs quota:

    STATION_E2E=1 HERMES_API_KEY=... python3 -m unittest tests.e2e.live_hermes
    STATION_E2E=1 HERMES_API_KEY=... HERMES_E2E_CHAT=1 \
        python3 -m unittest tests.e2e.live_hermes

The API key is supplied only through the environment and never printed.
"""
from __future__ import annotations

import json
import os
import unittest
import urllib.error
import urllib.request

HERMES_URL = os.environ.get("HERMES_API_URL", "http://127.0.0.1:8642").rstrip("/")
HERMES_KEY = os.environ.get("HERMES_API_KEY", "").strip()
E2E = os.environ.get("STATION_E2E") == "1"
CHAT = E2E and os.environ.get("HERMES_E2E_CHAT") == "1"


def api(path: str, method: str = "GET", payload: dict | None = None) -> tuple[int, dict]:
    data = None if payload is None else json.dumps(payload).encode()
    req = urllib.request.Request(
        HERMES_URL + path,
        data=data,
        method=method,
        headers={
            "Authorization": f"Bearer {HERMES_KEY}",
            "Content-Type": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=120) as resp:
            body = resp.read().decode()
            return resp.status, (json.loads(body) if body else {})
    except urllib.error.HTTPError as exc:
        body = exc.read().decode(errors="replace")
        return exc.code, {"error": body[:500]}


@unittest.skipUnless(E2E and HERMES_KEY, "set STATION_E2E=1 and HERMES_API_KEY")
class LiveHermesReadOnlyTests(unittest.TestCase):
    def test_health_and_model(self):
        code, data = api("/health")
        self.assertEqual(code, 200)
        self.assertEqual(data.get("status"), "ok")
        code, models = api("/v1/models")
        self.assertEqual(code, 200)
        ids = {m.get("id") for m in models.get("data", [])}
        self.assertIn("hermes-agent", ids)

    def test_capabilities_advertise_chat_and_tools(self):
        code, data = api("/v1/capabilities")
        self.assertEqual(code, 200)
        features = data.get("features", {})
        self.assertTrue(features.get("chat_completions"))
        self.assertTrue(features.get("run_submission"))

    def test_toolsets_are_curated(self):
        code, data = api("/v1/toolsets")
        self.assertEqual(code, 200)
        toolsets = data.get("data", []) if isinstance(data, dict) else []
        enabled = {t.get("name") for t in toolsets if t.get("enabled")}
        disabled = {t.get("name") for t in toolsets if not t.get("enabled")}
        self.assertIn("web", enabled)
        for name in ("terminal", "file", "browser", "code_execution"):
            self.assertIn(name, disabled)

    @unittest.skipUnless(CHAT, "set HERMES_E2E_CHAT=1 for a real model turn")
    def test_one_real_chat_turn(self):
        code, data = api(
            "/v1/chat/completions",
            "POST",
            {
                "model": "hermes-agent",
                "messages": [{"role": "user", "content": "Ответь одним словом: работает?"}],
            },
        )
        self.assertEqual(code, 200)
        content = data.get("choices", [{}])[0].get("message", {}).get("content", "")
        self.assertTrue(content)
        self.assertNotIn("couldn't connect", content.lower())


if __name__ == "__main__":
    unittest.main(verbosity=2)
