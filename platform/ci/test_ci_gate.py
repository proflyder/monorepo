#!/usr/bin/env python3
"""Tests for the aggregate `ci-ok` gate."""
from __future__ import annotations

import contextlib
import io
import json
import os
import sys
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

import ci_gate  # noqa: E402


class EvaluateTests(unittest.TestCase):
    def test_all_success_and_skipped_is_green(self):
        ok, _, failing = ci_gate.evaluate(
            {
                "changes": {"result": "success"},
                "go": {"result": "success"},
                "android": {"result": "skipped"},
                "python": {"result": "skipped"},
            }
        )
        self.assertTrue(ok)
        self.assertEqual(failing, {})

    def test_failure_is_red(self):
        ok, _, failing = ci_gate.evaluate(
            {"changes": {"result": "success"}, "go": {"result": "failure"}}
        )
        self.assertFalse(ok)
        self.assertEqual(failing, {"go": "failure"})

    def test_cancelled_is_red(self):
        ok, _, failing = ci_gate.evaluate(
            {"changes": {"result": "success"}, "android": {"result": "cancelled"}}
        )
        self.assertFalse(ok)
        self.assertEqual(failing, {"android": "cancelled"})

    def test_empty_needs_is_red(self):
        ok, results, _ = ci_gate.evaluate({})
        self.assertFalse(ok)
        self.assertEqual(results, {})


class MainTests(unittest.TestCase):
    def _run(self, needs):
        out, err = io.StringIO(), io.StringIO()
        with mock.patch.dict(os.environ, {"NEEDS": json.dumps(needs)}):
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = ci_gate.main()
        return code, out.getvalue(), err.getvalue()

    def test_missing_needs_fails(self):
        with mock.patch.dict(os.environ, {"NEEDS": ""}, clear=False):
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(ci_gate.main(), 1)

    def test_invalid_json_fails(self):
        with mock.patch.dict(os.environ, {"NEEDS": "not-json"}, clear=False):
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(ci_gate.main(), 1)

    def test_skipped_units_pass(self):
        code, out, _ = self._run({"changes": {"result": "success"}, "go": {"result": "skipped"}})
        self.assertEqual(code, 0)
        self.assertIn("legitimately skipped", out)

    def test_failed_unit_reports_the_name(self):
        code, _, err = self._run({"changes": {"result": "success"}, "go": {"result": "failure"}})
        self.assertEqual(code, 1)
        self.assertIn('"go": "failure"', err)


if __name__ == "__main__":
    unittest.main()
