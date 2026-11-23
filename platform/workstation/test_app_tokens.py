import io
import json
import unittest
from contextlib import redirect_stdout
from unittest import mock

import app_tokens
import workstation as ws


class FakeBao:
    def __init__(self, values):
        self.values = values
        self.calls = []

    def call(self, method, path, body=None, missing_ok=False):
        self.calls.append((method, path, body))
        if method == "GET":
            return {
                "data": {
                    "data": dict(self.values),
                    "metadata": {"version": 7},
                }
            }
        if method == "POST":
            assert body["options"]["cas"] == 7
            self.values = dict(body["data"])
            return {}
        raise AssertionError((method, path))


class IssueTests(unittest.TestCase):
    def test_issue_stores_only_the_hash_and_keeps_the_other_fields(self):
        bao = FakeBao({"token": "mama", "vpn.json": "{}"})
        values = app_tokens.issue(bao, app_tokens.PATH, "owner", "home_secret")
        self.assertEqual(values["token"], "mama")
        self.assertEqual(values["vpn.json"], "{}")
        self.assertNotIn("home_secret", values["app_tokens"])
        seed = json.loads(values["app_tokens"])
        entry = seed["tokens"][0]
        self.assertEqual(entry["sha256"], app_tokens.digest("home_secret"))
        self.assertTrue(entry["owner"])
        self.assertIn("admin", entry["modules"])
        self.assertTrue(values["service_token"])
        self.assertEqual(bao.calls[0][0], "GET")
        self.assertEqual(bao.calls[1][0], "POST")

    def test_issue_replaces_the_same_name_and_starts_empty(self):
        first = app_tokens.merge_token({"tokens": []}, "owner", "one")
        second = app_tokens.merge_token(first, "owner", "two")
        self.assertEqual(len(second["tokens"]), 1)
        self.assertEqual(second["tokens"][0]["sha256"], app_tokens.digest("two"))

    def test_dry_run_never_writes_or_prints_a_token(self):
        bao = FakeBao({"token": "mama"})
        app_tokens.issue(bao, app_tokens.PATH, "owner", "hidden", dry_run=True)
        self.assertEqual([call[0] for call in bao.calls], ["GET"])
        with mock.patch.object(ws, "operator", return_value=bao):
            output = io.StringIO()
            with redirect_stdout(output):
                app_tokens.main(["--dry-run"])
        self.assertNotIn("hidden", output.getvalue())
        self.assertIn("dry run", output.getvalue())

    def test_main_prints_the_token_once_and_never_the_service_token(self):
        bao = FakeBao({"token": "mama"})
        with mock.patch.object(ws, "operator", return_value=bao):
            output = io.StringIO()
            with redirect_stdout(output):
                app_tokens.main([])
        text = output.getvalue()
        tokens = [line for line in text.splitlines() if line.startswith("home_")]
        self.assertEqual(len(tokens), 1)
        seed = json.loads(bao.values["app_tokens"])
        self.assertEqual(seed["tokens"][0]["sha256"], app_tokens.digest(tokens[0]))
        self.assertNotIn(bao.values["service_token"], text)
        self.assertNotIn("mama", text)


if __name__ == "__main__":
    unittest.main()
