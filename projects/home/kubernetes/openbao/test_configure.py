import os
import sys
import re
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest import mock

DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(DIR))

import configure

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
        for forbidden in ("ci/test", "k8s/", '"*"'):
            self.assertNotIn(forbidden, configure.STATION_POLICY)


class AndroidRole(unittest.TestCase):
    def test_android_role_payload(self):
        bao = Recorder()
        with redirect_stdout(StringIO()):
            configure.ensure_github(bao, AUDIENCE, HOME_REPOSITORY)
        self.assertEqual(bao.payload("/v1/auth/github/role/android-distribute"), {
            "role_type": "jwt",
            "user_claim": "job_workflow_ref",
            "bound_audiences": [AUDIENCE],
            "bound_claims_type": "glob",
            "bound_claims": {
                "repository": HOME_REPOSITORY,
                "environment": "Android",
                "ref": "refs/pull/*/merge",
                "job_workflow_ref":
                    "example/home/.github/workflows/_android-release.yml"
                    "@refs/pull/*/merge",
                "event_name": "pull_request",
            },
            "token_policies": ["ci-android"],
            "token_ttl": "10m",
            "token_max_ttl": "15m",
            "clock_skew": 60,
        })

    def test_android_globs_only_match_pr_ci_runs(self):
        import fnmatch

        claims = configure.ensure_github
        bao = Recorder()
        with redirect_stdout(StringIO()):
            claims(bao, AUDIENCE, HOME_REPOSITORY)
        roles = {
            "android-distribute": ".github/workflows/_android-release.yml",
        }
        for role, own_workflow in roles.items():
            bound = bao.payload(f"/v1/auth/github/role/{role}")["bound_claims"]
            ref, workflow = bound["ref"], bound["job_workflow_ref"]

            self.assertTrue(fnmatch.fnmatch("refs/pull/42/merge", ref))
            self.assertTrue(
                fnmatch.fnmatch(
                    f"example/home/{own_workflow}@refs/pull/42/merge",
                    workflow))
            for other in ("refs/heads/develop", "refs/heads/feature/x",
                          "refs/pull/42/head", "refs/tags/v1"):
                self.assertFalse(fnmatch.fnmatch(other, ref), other)
            self.assertFalse(
                fnmatch.fnmatch(
                    "example/home/.github/workflows/deploy-home.yml@refs/pull/42/merge",
                    workflow))
            self.assertFalse(
                fnmatch.fnmatch(
                    "example/other/.github/workflows/ci.yml@refs/pull/42/merge",
                    workflow))
            self.assertFalse(
                fnmatch.fnmatch(
                    "example/home/.github/workflows/_android.yml@refs/pull/42/merge",
                    workflow))

    def test_android_release_role_payload(self):
        bao = Recorder()
        with redirect_stdout(StringIO()):
            configure.ensure_github(bao, AUDIENCE, HOME_REPOSITORY)
        self.assertEqual(bao.payload("/v1/auth/github/role/android-release"), {
            "role_type": "jwt",
            "user_claim": "job_workflow_ref",
            "bound_audiences": [AUDIENCE],
            "bound_claims_type": "glob",
            "bound_claims": {
                "repository": HOME_REPOSITORY,
                "environment": "Android",
                "ref": "refs/heads/develop",
                "job_workflow_ref":
                    "example/home/.github/workflows/_android-release.yml@refs/heads/develop",
                "event_name": "workflow_dispatch",
            },
            "token_policies": ["ci-android"],
            "token_ttl": "10m",
            "token_max_ttl": "15m",
            "clock_skew": 60,
        })

    def test_android_release_role_is_not_a_pr_role(self):
        import fnmatch

        bao = Recorder()
        with redirect_stdout(StringIO()):
            configure.ensure_github(bao, AUDIENCE, HOME_REPOSITORY)
        bound = bao.payload("/v1/auth/github/role/android-release")["bound_claims"]
        self.assertEqual(bound["ref"], "refs/heads/develop")
        self.assertEqual(bound["event_name"], "workflow_dispatch")
        self.assertFalse(fnmatch.fnmatch(
            "refs/pull/42/merge", bound["ref"]))
        self.assertFalse(fnmatch.fnmatch(
            "example/home/.github/workflows/_android-release.yml@refs/pull/42/merge",
            bound["job_workflow_ref"]))

    def test_android_cleanup_role_payload(self):
        bao = Recorder()
        with redirect_stdout(StringIO()):
            configure.ensure_github(bao, AUDIENCE, HOME_REPOSITORY)
        self.assertEqual(bao.payload("/v1/auth/github/role/android-cleanup"), {
            "role_type": "jwt",
            "user_claim": "job_workflow_ref",
            "bound_audiences": [AUDIENCE],
            "bound_claims_type": "glob",
            "bound_claims": {
                "repository": HOME_REPOSITORY,
                "environment": "Android",
                "ref": "refs/heads/develop",
                "job_workflow_ref":
                    "example/home/.github/workflows/_cleanup.yml@refs/heads/develop",
                "event_name": ["schedule", "workflow_dispatch"],
            },
            "token_policies": ["ci-android"],
            "token_ttl": "10m",
            "token_max_ttl": "15m",
            "clock_skew": 60,
        })

    def test_android_cleanup_globs_only_match_the_scheduled_workflow(self):
        import fnmatch

        bao = Recorder()
        with redirect_stdout(StringIO()):
            configure.ensure_github(bao, AUDIENCE, HOME_REPOSITORY)
        bound = bao.payload("/v1/auth/github/role/android-cleanup")["bound_claims"]
        self.assertEqual(bound["ref"], "refs/heads/develop")
        self.assertEqual(bound["event_name"], ["schedule", "workflow_dispatch"])
        workflow = bound["job_workflow_ref"]
        self.assertTrue(fnmatch.fnmatch(
            "example/home/.github/workflows/_cleanup.yml@refs/heads/develop",
            workflow))
        for other in (
            "example/home/.github/workflows/ci.yml@refs/heads/develop",
            "example/home/.github/workflows/_cleanup.yml@refs/pull/42/merge",
            "example/other/.github/workflows/_cleanup.yml@refs/heads/develop",
        ):
            self.assertFalse(fnmatch.fnmatch(other, workflow), other)

    def test_no_home_deploy_role_is_created(self):
        bao = Recorder()
        with redirect_stdout(StringIO()):
            configure.ensure_github(bao, AUDIENCE, HOME_REPOSITORY)
        self.assertNotIn("/v1/auth/github/role/deploy-home", bao.paths())
        self.assertEqual(
            [path for path in bao.paths() if path.startswith("/v1/auth/github/role/")],
            [
                "/v1/auth/github/role/android-distribute",
                "/v1/auth/github/role/android-release",
                "/v1/auth/github/role/android-cleanup",
            ],
        )

    def test_android_policy_is_scoped_to_its_entry(self):
        self.assertIn('path "secret/data/ci/android-distribute"',
                      configure.CI_ANDROID_POLICY)
        self.assertIn('path "secret/metadata/ci/android-distribute"',
                      configure.CI_ANDROID_POLICY)
        for forbidden in ("ci/deploy-station", "ci/test",
                          "k8s/", '"*"'):
            self.assertNotIn(forbidden, configure.CI_ANDROID_POLICY)


def run_main(*extra_args, env=None):
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
        self.assertEqual(names, ["admin", "ci-android",
                                 "k8s-read", "backup", "secrets-gateway",
                                 "operator-kv", "hermes-work-eso"])
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


class HermesInstanceIsolationTests(unittest.TestCase):
    def test_instance_policy_reads_only_its_own_path(self):
        policy = configure.hermes_instance_policy("work")
        paths = re.findall(r'path "([^"]+)"', policy)
        self.assertEqual(paths, ["secret/data/k8s/hermes-work/*",
                                 "secret/metadata/k8s/hermes-work/*"])
        self.assertNotIn('"list"', policy)
        self.assertNotIn('"update"', policy)

    def test_shared_k8s_policy_denies_the_isolated_instance(self):
        for kind in ("data", "metadata"):
            block = re.search(
                r'path "secret/%s/k8s/hermes-work/\*" \{(.*?)\}' % kind,
                configure.K8S_POLICY, re.S)
            self.assertIsNotNone(block, kind)
            self.assertIn('capabilities = ["deny"]', block.group(1))

    def test_instance_role_is_bound_to_its_namespace(self):
        roles = configure.hermes_instance_roles()
        self.assertEqual(roles, {"hermes-work-eso": (
            ["hermes-secrets"], ["hermes-work"], ["hermes-work-eso"])})

    def test_secrets_gateway_cannot_touch_the_instance(self):
        self.assertIn('path "secret/data/k8s/hermes-work/*"', configure.SECRETS_GATEWAY_POLICY)


class SecretsGatewayPolicyTests(unittest.TestCase):
    def test_the_tracked_policy_is_generated_from_the_deny_file(self):
        policy = (
            configure.ROOT / "projects" / "home" / "kubernetes"
            / "secrets-gateway" / "policy.hcl"
        ).read_text(encoding="utf-8")
        self.assertEqual(policy, configure.secrets_gateway_policy())

    def test_policy_allows_kv_v2_data_and_metadata_only(self):
        policy = configure.SECRETS_GATEWAY_POLICY
        self.assertIn('path "secret/data/*"', policy)
        self.assertIn('path "secret/metadata/*"', policy)
        self.assertIn('path "secret/metadata"', policy)
        self.assertIn('capabilities = ["read", "update", "delete"]', policy)
        self.assertIn('capabilities = ["read", "list"]', policy)
        self.assertNotIn('"destroy"', policy)
        self.assertNotIn('"sudo"', policy)
        metadata_blocks = re.findall(
            r'path "secret/metadata[^"]*" \{(.*?)\}', policy, re.S)
        for block in metadata_blocks:
            self.assertNotIn('"delete"', block)
            self.assertNotIn('"destroy"', block)
            self.assertNotIn('"undelete"', block)

    def test_core_paths_and_openbao_internals_are_denied(self):
        policy = configure.SECRETS_GATEWAY_POLICY
        for path in (
            "secret/data/openbao",
            "secret/data/openbao/*",
            "secret/data/infra/vpn/client-ca",
            "secret/data/infra/vpn/client-ca/*",
            "secret/data/k8s/flux-system",
            "secret/data/k8s/flux-system/*",
            "secret/data/k8s/home-assistant/scheduler-auth",
            "secret/data/k8s/home-assistant/secrets-gateway",
            "secret/destroy/*",
            "sys/*",
        ):
            with self.subTest(path=path):
                block = re.search(
                    rf'path "{re.escape(path)}" \{{(.*?)\}}', policy, re.S)
                self.assertIsNotNone(block, path)
                self.assertIn('capabilities = ["deny"]', block.group(1))

    def test_non_core_infrastructure_and_ci_are_reachable(self):
        policy = configure.SECRETS_GATEWAY_POLICY
        for path in (
            "secret/data/infra",
            "secret/data/ci",
            "secret/data/k8s/home-assistant/phone-push",
        ):
            with self.subTest(path=path):
                block = re.search(
                    rf'path "{re.escape(path)}" \{{(.*?)\}}', policy, re.S)
                self.assertIsNone(block, path)

    def test_the_role_binds_the_service_account_to_the_policy(self):
        bao = Recorder()
        with redirect_stdout(StringIO()):
            configure.ensure_kubernetes(bao)
        self.assertEqual(bao.payload("/v1/auth/kubernetes/role/secrets-gateway"), {
            "bound_service_account_names": ["secrets-gateway"],
            "bound_service_account_namespaces": ["home-assistant"],
            "token_policies": ["secrets-gateway"],
            "token_ttl": "20m",
            "token_max_ttl": "1h",
            "audience": "secrets-gateway",
        })


class OperatorPolicyTests(unittest.TestCase):
    def test_operator_policy_is_kv_only_and_cannot_delete(self):
        policy = configure.OPERATOR_KV_POLICY
        paths = re.findall(r'path "([^"]+)"', policy)
        self.assertTrue(all(p.startswith("secret/") for p in paths), paths)
        rules = "\n".join(l for l in policy.splitlines() if not l.lstrip().startswith("#"))
        self.assertNotIn("sys/", rules)
        self.assertNotIn("auth/", rules)
        for forbidden in ("delete", "destroy", "sudo", "deny"):
            self.assertNotIn(f'"{forbidden}"', policy)
        self.assertIn('path "secret/data/ci/*"', policy)
        self.assertIn('path "secret/data/k8s/*"', policy)
        self.assertIn('path "secret/data/infra/*"', policy)


if __name__ == "__main__":
    unittest.main()


class TotpState(unittest.TestCase):

    def run_totp(self, enforcement_code):
        class Bao(Recorder):
            def call(self, method, path, data=None):
                self.calls.append((method, path, data))
                if path == "/v1/identity/mfa/method":
                    return 200, {"data": {"keys": ["m1"]}}
                if path == "/v1/identity/mfa/method/m1":
                    return 200, {"data": {"name": "human-totp", "id": "m1"}}
                if path.endswith("/login-enforcement/userpass-totp"):
                    return enforcement_code, {}
                return 200, {}

        bao, out = Bao(), StringIO()
        with mock.patch.object(configure, "SECRETS", Path(self.tmp)), redirect_stdout(out):
            configure.ensure_totp(bao, enforce=False)
        return bao, out.getvalue()

    def setUp(self):
        import tempfile
        self.tmp = tempfile.mkdtemp()

    def test_existing_enforcement_is_reported_and_kept(self):
        bao, out = self.run_totp(200)
        self.assertIn("already enforced", out)
        self.assertFalse([c for c in bao.calls if c[0] in ("POST", "DELETE")
                          and "login-enforcement" in c[1]])

    def test_missing_enforcement_is_reported(self):
        _, out = self.run_totp(404)
        self.assertIn("enforcement not enabled", out)
