#!/usr/bin/env python3
"""Rollback bookkeeping: only restore what this job actually applied."""
from __future__ import annotations

import json
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import rollback  # noqa: E402


class FakeKubectl:
    def __init__(self, generations: dict[str, int | None]):
        self.generations = generations
        self.calls: list[tuple[str, ...]] = []

    def __call__(self, kubectl, *args):
        self.calls.append(args)
        if args[:3] == ("-n", "home-assistant", "get"):
            name = args[4]
            value = self.generations.get(name)
            return mock.Mock(returncode=0 if value is not None else 1, stdout=str(value or ""), stderr="")
        if args[2] == "patch":
            path = pathlib.Path(args[args.index("--patch-file") + 1])
            self.patch = json.loads(path.read_text())
            return mock.Mock(returncode=0, stdout="patched", stderr="")
        if args[2] == "delete":
            return mock.Mock(returncode=0, stdout="deleted", stderr="")
        if args[2] == "rollout":
            return mock.Mock(returncode=0, stdout="ready", stderr="")
        raise AssertionError(f"unexpected call: {args}")

    def verbs(self) -> list[str]:
        return [args[2] for args in self.calls]


def before_object(generation: int, image: str = "ghcr.io/proflyder/scheduler@sha256:" + "a" * 64) -> dict:
    return {
        "metadata": {"name": "scheduler", "generation": generation},
        "spec": {
            "replicas": 1,
            "template": {"spec": {"containers": [{"name": "scheduler", "image": image}]}},
        },
    }


class RollbackTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.state = pathlib.Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_no_markers_means_no_calls(self):
        fake = FakeKubectl({})
        with mock.patch.object(rollback, "_run", side_effect=fake):
            messages = rollback.rollback("kubectl", "home-assistant", self.state)
        self.assertEqual(fake.calls, [])
        self.assertIn("nothing to roll back", messages[0])

    def test_apply_failure_before_marker_does_not_roll_back(self):
        # deploy.sh records `before` but only marks `.applied` after the apply;
        # a push/render failure leaves no marker.
        (self.state / "scheduler.before.json").write_text(json.dumps(before_object(3)))
        fake = FakeKubectl({"scheduler": 4})
        with mock.patch.object(rollback, "_run", side_effect=fake):
            rollback.rollback("kubectl", "home-assistant", self.state)
        self.assertEqual(fake.calls, [])

    def test_unchanged_generation_is_not_rolled_back(self):
        (self.state / "scheduler.before.json").write_text(json.dumps(before_object(3)))
        (self.state / "scheduler.applied").write_text("")
        fake = FakeKubectl({"scheduler": 3})
        with mock.patch.object(rollback, "_run", side_effect=fake):
            messages = rollback.rollback("kubectl", "home-assistant", self.state)
        self.assertNotIn("patch", fake.verbs())
        self.assertIn("did not change", messages[0])

    def test_applied_change_is_restored_to_the_recorded_spec(self):
        before = before_object(7, "ghcr.io/proflyder/scheduler@sha256:" + "b" * 64)
        (self.state / "scheduler.before.json").write_text(json.dumps(before))
        (self.state / "scheduler.applied").write_text("")
        fake = FakeKubectl({"scheduler": 8})
        with mock.patch.object(rollback, "_run", side_effect=fake):
            messages = rollback.rollback("kubectl", "home-assistant", self.state)
        self.assertIn("patch", fake.verbs())
        self.assertEqual(fake.patch, {"spec": before["spec"]})
        self.assertIn("rollout", fake.verbs())
        self.assertIn("restored to generation 7", messages[0])

    def test_first_install_is_deleted(self):
        (self.state / "ha-bridge.absent").write_text("")
        (self.state / "ha-bridge.applied").write_text("")
        fake = FakeKubectl({"ha-bridge": 1})
        with mock.patch.object(rollback, "_run", side_effect=fake):
            messages = rollback.rollback("kubectl", "home-assistant", self.state)
        self.assertIn("delete", fake.verbs())
        self.assertIn("deleted", messages[0])

    def test_marker_without_state_is_skipped(self):
        (self.state / "scheduler.applied").write_text("")
        fake = FakeKubectl({"scheduler": 4})
        with mock.patch.object(rollback, "_run", side_effect=fake):
            messages = rollback.rollback("kubectl", "home-assistant", self.state)
        self.assertNotIn("patch", fake.verbs())
        self.assertIn("no recorded state", messages[0])


if __name__ == "__main__":
    unittest.main()
