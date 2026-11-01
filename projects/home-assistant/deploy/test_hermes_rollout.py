#!/usr/bin/env python3
"""Unit tests for the Hermes digest rollout (no cluster access)."""
from __future__ import annotations

import io
import pathlib
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from unittest import mock

import yaml

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import hermes_rollout as rollout  # noqa: E402

HERMES = "ghcr.io/proflyder/hermes@sha256:" + "a" * 64
PROXY = "ghcr.io/proflyder/voice-proxy@sha256:" + "b" * 64

VALUES = {
    "nodeName": "lab-host",
    "storage": {"hermes": {"size": "10Gi", "path": "/data"}},
    "workloads": {
        "hermes": {
            "replicas": 1,
            "containers": [
                {"name": "gateway", "image": HERMES},
                {"name": "voice-proxy", "image": PROXY},
            ],
        }
    },
}


class DigestTests(unittest.TestCase):
    def test_accepts_bare_sha256(self):
        self.assertEqual(rollout.normalize_digest("hermes", "sha256:" + "c" * 64),
                         "ghcr.io/proflyder/hermes@sha256:" + "c" * 64)

    def test_accepts_pinned_reference(self):
        self.assertEqual(rollout.normalize_digest("voice-proxy", PROXY), PROXY)

    def test_rejects_tag(self):
        with self.assertRaises(ValueError):
            rollout.normalize_digest("hermes", "develop")
        with self.assertRaises(ValueError):
            rollout.normalize_digest("hermes", "ghcr.io/proflyder/hermes:develop")

    def test_image_digest(self):
        self.assertEqual(rollout.image_digest(HERMES), "sha256:" + "a" * 64)


class ValuesTests(unittest.TestCase):
    def test_workload_values_drops_storage(self):
        self.assertNotIn("storage", rollout.workload_values(VALUES))
        self.assertEqual(rollout.workload_values(VALUES)["workloads"], VALUES["workloads"])

    def test_write_values_round_trips_without_storage(self):
        path = rollout.write_values(rollout.workload_values(VALUES))
        try:
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            loaded = yaml.safe_load(path.read_text())
            self.assertNotIn("storage", loaded)
            self.assertEqual(loaded["workloads"]["hermes"]["replicas"], 1)
        finally:
            path.unlink()

    def test_load_values_requires_two_containers(self):
        with tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False) as handle:
            yaml.safe_dump({"workloads": {"hermes": {"containers": [{"name": "gateway"}]}}}, handle)
            path = pathlib.Path(handle.name)
        try:
            with self.assertRaises(ValueError):
                rollout.load_values(path)
        finally:
            path.unlink()


class RolloutDecisionTests(unittest.TestCase):
    def test_no_new_digest_skips(self):
        self.assertFalse(rollout.rollout_needed({"gateway": HERMES, "voice-proxy": PROXY},
                                                HERMES, PROXY))

    def test_one_new_digest_rolls(self):
        newer = "ghcr.io/proflyder/voice-proxy@sha256:" + "d" * 64
        self.assertTrue(rollout.rollout_needed({"gateway": HERMES, "voice-proxy": PROXY},
                                               HERMES, newer))

    def test_set_args_target_container_indexes(self):
        args = rollout.set_args(HERMES, PROXY)
        self.assertIn(f"workloads.hermes.containers[0].image={HERMES}", args)
        self.assertIn(f"workloads.hermes.containers[1].image={PROXY}", args)
        self.assertNotIn("--reuse-values", args)


class HealthTests(unittest.TestCase):
    def metrics(self, values: dict[str, int]) -> str:
        return "\n".join(f'ricardo_endpoint_up{{endpoint="{k}"}} {v}' for k, v in values.items())

    def test_all_endpoints_up(self):
        rollout.check_health(self.metrics({name: 1 for name in rollout.EXPECTED_ENDPOINTS}))

    def test_down_endpoint_fails(self):
        values = {name: 1 for name in rollout.EXPECTED_ENDPOINTS}
        values["hermes"] = 0
        with self.assertRaises(rollout.RolloutError):
            rollout.check_health(self.metrics(values))

    def test_missing_endpoint_fails(self):
        values = {name: 1 for name in rollout.EXPECTED_ENDPOINTS if name != "voice_proxy"}
        with self.assertRaises(rollout.RolloutError):
            rollout.check_health(self.metrics(values))


class VoiceLogTests(unittest.TestCase):
    def test_clean_log(self):
        rollout.check_voice_logs("listening on 0.0.0.0:8643\nrequest ok")

    def test_error_log_fails(self):
        with self.assertRaises(rollout.RolloutError):
            rollout.check_voice_logs("Traceback (most recent call last):\nValueError")

    def test_word_not_matched_inside_other_words(self):
        self.assertEqual(rollout.voice_proxy_log_errors("errors=0; no problem"), [])


class SmokeJobTests(unittest.TestCase):
    def test_job_mounts_existing_secret_and_has_no_token(self):
        job = rollout.smoke_job("hermes-smoke-1", "home-assistant", PROXY)
        pod = job["spec"]["template"]["spec"]
        self.assertEqual(pod["volumes"][0]["secret"]["secretName"], rollout.HA_SECRET)
        text = yaml.safe_dump(job)
        self.assertNotIn("HA_TOKEN", text)
        self.assertIn("conversation.hermes_agent", text)

    def test_job_is_hardened(self):
        pod = rollout.smoke_job("x", "home-assistant", PROXY)["spec"]["template"]["spec"]
        self.assertFalse(pod["automountServiceAccountToken"])
        container = pod["containers"][0]
        self.assertFalse(container["securityContext"]["allowPrivilegeEscalation"])
        self.assertTrue(container["securityContext"]["readOnlyRootFilesystem"])


class AssistSmokeRetryTests(unittest.TestCase):
    """A warm-up 5xx is retried; a real failure is not."""

    def run_smoke(self, results, *, retry_window=90, retry_delay=5):
        now = [0]

        def clock():
            return now[0]

        def sleep(seconds):
            now[0] += seconds

        with mock.patch.object(rollout, "assist_attempt", side_effect=results) as attempt:
            speech = rollout.assist_smoke(
                "kubectl", "home-assistant", PROXY,
                retry_window=retry_window, retry_delay=retry_delay,
                sleep=sleep, clock=clock,
            )
        return speech, attempt, now[0]

    def test_transient_500_is_retried(self):
        speech, attempt, slept = self.run_smoke([
            {"ok": False, "status": 500, "error": "HTTPError: 500 runtime_draining"},
            {"ok": True, "speech": "Готов."},
        ])
        self.assertEqual(speech, "Готов.")
        self.assertEqual(attempt.call_count, 2)
        self.assertEqual(slept, rollout.SMOKE_RETRY_DELAY)

    def test_connection_error_is_retried(self):
        speech, attempt, _ = self.run_smoke([
            {"ok": False, "error": "URLError: [Errno 111] Connection refused"},
            {"ok": True, "speech": "Готов."},
        ])
        self.assertEqual(speech, "Готов.")
        self.assertEqual(attempt.call_count, 2)

    def test_4xx_is_not_retried(self):
        with mock.patch.object(rollout, "assist_attempt",
                               return_value={"ok": False, "status": 401}) as attempt:
            with self.assertRaises(rollout.RolloutError):
                rollout.assist_smoke("kubectl", "home-assistant", PROXY,
                                     retry_window=90, retry_delay=5, sleep=mock.Mock())
        self.assertEqual(attempt.call_count, 1)

    def test_empty_speech_is_not_retried(self):
        with mock.patch.object(rollout, "assist_attempt",
                               return_value={"ok": False, "status": 200, "speech": ""}) as attempt:
            with self.assertRaises(rollout.RolloutError):
                rollout.assist_smoke("kubectl", "home-assistant", PROXY,
                                     retry_window=90, retry_delay=5, sleep=mock.Mock())
        self.assertEqual(attempt.call_count, 1)

    def test_window_is_bounded(self):
        now = [0]

        def clock():
            return now[0]

        def sleep(seconds):
            now[0] += seconds

        with mock.patch.object(
            rollout, "assist_attempt", return_value={"ok": False, "status": 503}
        ) as attempt:
            with self.assertRaises(rollout.RolloutError) as caught:
                rollout.assist_smoke(
                    "kubectl", "home-assistant", PROXY,
                    retry_window=10, retry_delay=5, sleep=sleep, clock=clock,
                )
        self.assertIn("still failing after 10s", str(caught.exception))
        self.assertEqual(attempt.call_count, 3)

    def test_transient_classification(self):
        self.assertTrue(rollout.transient_smoke_result({"ok": False, "status": 502}))
        self.assertTrue(
            rollout.transient_smoke_result(
                {"ok": False, "error": "TimeoutError: timed out"}
            )
        )
        self.assertFalse(rollout.transient_smoke_result({"ok": False, "status": 404}))
        self.assertFalse(rollout.transient_smoke_result({"ok": True, "speech": "ok"}))

    def test_smoke_script_reports_http_status(self):
        self.assertIn("urllib.error.HTTPError", rollout.SMOKE_SCRIPT)
        self.assertIn('"status": exc.code', rollout.SMOKE_SCRIPT)


class RollbackTests(unittest.TestCase):
    def test_rollback_only_is_noop_without_applied_marker(self):
        with tempfile.TemporaryDirectory() as directory:
            with redirect_stdout(io.StringIO()) as out:
                code = rollout.rollback_only("helm", "kubectl", "home-assistant",
                                             pathlib.Path(directory), "1m")
            self.assertEqual(code, 0)
            self.assertIn("nothing to roll back", out.getvalue())

    def test_rollback_only_uses_recorded_revision(self):
        with tempfile.TemporaryDirectory() as directory:
            state = pathlib.Path(directory)
            (state / "upgrade-applied").write_text("1")
            (state / "previous-revision").write_text("14")
            with mock.patch.object(rollout, "rollback") as rollback, \
                    mock.patch.object(rollout, "_checked") as checked, \
                    redirect_stdout(io.StringIO()):
                code = rollout.rollback_only("helm", "kubectl", "home-assistant", state, "1m")
            self.assertEqual(code, 0)
            rollback.assert_called_once_with("helm", "home-assistant", "runtime", 14, "1m")
            checked.assert_called_once()


if __name__ == "__main__":
    unittest.main(verbosity=2)
