"""Contracts for the optional station (firmware) deploy identity.

`configure.py` must keep the existing deploy-home configuration untouched and
add the `ci-station` policy plus the `deploy-station` JWT role only when a
station repository is supplied. These tests record API calls, so no OpenBao is
needed.
"""
import os
import sys
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest import mock

DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(DIR))

import configure  # noqa: E402

AUDIENCE = "https://vault.example.test"
HOME_REPOSITORY = "example/home"
STATION_REPOSITORY = "example/station-fork"


class Recorder:
    def __init__(self) -> None:
        self.calls: list[tuple[str, str, dict | None]] = []

    def call(self, method, path, data=None):
        self.calls.append((method, path, data))
        return 200, {}

    def expect(self, method, path, data=None):
        self.calls.append((method, path, data))
        return {}

    def payload(self, path):
        for _, called, data in self.calls:
            if called == path:
                return data
        return None

    def paths(self):
        return [path for _, path, _ in self.calls]


class StationRole(unittest.TestCase):
    def test_without_station_repository_no_station_role(self):
        bao = Recorder()
        with redirect_stdout(StringIO()):
            configure.ensure_github(bao, AUDIENCE, HOME_REPOSITORY)
        self.assertNotIn("/v1/auth/github/role/deploy-station", bao.paths())

    def test_station_role_payload(self):
        bao = Recorder()
        with redirect_stdout(StringIO()):
            configure.ensure_github(bao, AUDIENCE, HOME_REPOSITORY,
                                   STATION_REPOSITORY)
        self.assertEqual(bao.payload("/v1/auth/github/role/deploy-station"), {
            "role_type": "jwt",
            "user_claim": "job_workflow_ref",
            "bound_audiences": [AUDIENCE],
            "bound_claims": {
                "repository": STATION_REPOSITORY,
                "environment": "Station",
            },
            "token_policies": ["ci-station"],
            "token_ttl": "10m",
            "token_max_ttl": "15m",
            "clock_skew": 60,
        })

    def test_station_policy_is_scoped_to_its_entry(self):
        self.assertIn('path "secret/data/ci/deploy-station"',
                      configure.STATION_POLICY)
        self.assertIn('path "secret/metadata/ci/deploy-station"',
                      configure.STATION_POLICY)
        for forbidden in ("ci/deploy-home", "ci/test", "k8s/", '"*"'):
            self.assertNotIn(forbidden, configure.STATION_POLICY)


def run_main(*extra_args, env=None):
    """Run main() with the API surface replaced by recorders."""
    policies: list[tuple[str, str]] = []
    github: list[tuple[str, str, str | None]] = []

    with mock.patch.dict(os.environ, env or {}, clear=True), \
            mock.patch.object(configure, "Bao", lambda *a: Recorder()), \
            mock.patch.object(configure, "ensure_kv"), \
            mock.patch.object(configure, "ensure_audit"), \
            mock.patch.object(configure, "ensure_userpass"), \
            mock.patch.object(configure, "ensure_kubernetes"), \
            mock.patch.object(configure, "ensure_secret"), \
            mock.patch.object(configure, "ensure_totp"), \
            mock.patch.object(
                configure, "ensure_policy",
                side_effect=lambda bao, name, policy: policies.append((name, policy))), \
            mock.patch.object(
                configure, "ensure_github",
                side_effect=lambda bao, audience, repository,
                station_repository=None: github.append(
                    (audience, repository, station_repository))), \
            mock.patch.object(sys, "argv", [
                "configure.py", "--addr", "https://127.0.0.1:8200",
                "--token", "admin-token", "--user", "operator",
                "--audience", AUDIENCE, "--repository", HOME_REPOSITORY,
                *extra_args]):
        with redirect_stdout(StringIO()):
            configure.main()
    return [name for name, _ in policies], policies, github


class MainGating(unittest.TestCase):
    def test_no_flag_keeps_existing_configuration(self):
        names, _, github = run_main()
        self.assertNotIn("ci-station", names)
        self.assertEqual(names, ["admin", "ci-deploy", "k8s-read", "backup"])
        self.assertEqual(github, [(AUDIENCE, HOME_REPOSITORY, None)])

    def test_flag_adds_policy_and_role(self):
        names, policies, github = run_main(
            "--station-repository", STATION_REPOSITORY)
        self.assertIn("ci-station", names)
        self.assertEqual(dict(policies)["ci-station"], configure.STATION_POLICY)
        self.assertEqual(github, [(AUDIENCE, HOME_REPOSITORY, STATION_REPOSITORY)])

    def test_env_fallback(self):
        names, _, github = run_main(
            env={"OPENBAO_STATION_REPOSITORY": STATION_REPOSITORY})
        self.assertIn("ci-station", names)
        self.assertEqual(github, [(AUDIENCE, HOME_REPOSITORY, STATION_REPOSITORY)])

    def test_empty_flag_does_not_create_the_role(self):
        names, _, github = run_main("--station-repository", "  ")
        self.assertNotIn("ci-station", names)
        self.assertEqual(github, [(AUDIENCE, HOME_REPOSITORY, None)])


if __name__ == "__main__":
    unittest.main()
