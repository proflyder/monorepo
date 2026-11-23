import base64
import io
import json
import pathlib
import stat
import tempfile
import unittest
from contextlib import redirect_stdout
from unittest import mock

import yaml

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
            if getattr(self, "fail_post", False):
                raise RuntimeError("cas mismatch")
            assert body["options"]["cas"] == 7
            self.values = dict(body["data"])
            return {}
        raise AssertionError((method, path))


class ModuleCatalogTests(unittest.TestCase):
    def test_python_and_rust_module_lists_match(self):
        import pathlib
        import re

        root = pathlib.Path(__file__).resolve().parents[2]
        rust = (root / "packages/rust/home-app-auth/src/lib.rs").read_text()
        body = rust.split("pub fn id(self)", 1)[1].split("pub fn title", 1)[0]
        modules = set(re.findall(r'Self::\w+ => "([a-z_]+)"', body))
        self.assertEqual(modules, set(app_tokens.MODULES))


class FakeCluster:
    def __init__(self, bao, deployments=None, hermes_key="service_token", lag=0):
        self.bao = bao
        self.deployments = set(
            deployments
            if deployments is not None
            else [app_tokens.SCHEDULER, *app_tokens.SERVICE_CONSUMERS]
        )
        self.hermes_key = hermes_key
        self.lag = lag
        self.pending = {}
        self.refresh = {name: 1 for name in app_tokens.SERVICE_SECRETS}
        self.secrets = {name: self.snapshot(name) for name in app_tokens.SERVICE_SECRETS}
        self.loaded = self.secrets["scheduler-auth"]["service_token"]
        self.restarts = []
        self.commands = []

    def snapshot(self, name):
        values = self.bao.values
        if name == "scheduler-auth":
            keys = ["token", "service_token", "service_token_previous", "app_tokens"]
            return {key: values.get(key, "") for key in keys}
        return {"service_token": values.get("service_token", "")}

    def tick(self):
        for name in list(self.pending):
            self.pending[name] -= 1
            if self.pending[name] <= 0:
                del self.pending[name]
                self.secrets[name] = self.snapshot(name)
                self.refresh[name] += 1

    def __call__(self, argv, check=False, capture_output=True, text=True):
        args = argv[4:]
        self.commands.append(args)
        self.tick()
        done = lambda stdout="": mock.Mock(returncode=0, stdout=stdout, stderr="")
        missing = mock.Mock(returncode=1, stdout="", stderr="NotFound")
        if args[:2] == ["get", "secret"]:
            data = {
                key: base64.b64encode(value.encode()).decode()
                for key, value in self.secrets[args[2]].items()
            }
            return done(json.dumps({"data": data}))
        if args[0] == "get" and args[1].startswith("externalsecret/"):
            name = args[1].split("/", 1)[1]
            return done(json.dumps({"status": {
                "refreshTime": str(self.refresh[name]),
                "conditions": [{"type": "Ready", "status": "True"}],
            }}))
        if args[0] == "annotate":
            self.pending[args[1].split("/", 1)[1]] = self.lag + 1
            return done()
        if args[0] == "get" and args[1] == "deployment/hermes" and len(args) > 2:
            ref = {"secretKeyRef": {"name": "scheduler-auth", "key": self.hermes_key}}
            container = {"name": "voice-proxy", "env": [{"name": "HERMES_SCHEDULER_TOKEN", "valueFrom": ref}]}
            return done(json.dumps({"spec": {"template": {"spec": {"containers": [container]}}}}))
        if args[0] == "get" and args[1].startswith("deployment/"):
            return done() if args[1].split("/", 1)[1] in self.deployments else missing
        if args[:2] == ["get", "secrets"] or args[:2] == ["get", "configmaps"]:
            return done(json.dumps({"items": []}))
        if args[:2] == ["rollout", "restart"]:
            name = args[2].split("/", 1)[1]
            self.restarts.append(name)
            if name == app_tokens.SCHEDULER:
                self.loaded = self.secrets["scheduler-auth"]["service_token"]
            return done()
        if args[:2] == ["rollout", "status"]:
            return done()
        if args[0] == "exec":
            return done(app_tokens.digest(self.loaded) + "\n")
        raise AssertionError(args)


def kube_for(cluster):
    return app_tokens.Kube(run=cluster, sleep=lambda _: None, clock=iter(range(10**6)).__next__, out=lambda *_: None)


def initial_values():
    return {
        "token": "mama",
        "service_token": "service-1",
        "service_token_previous": "",
        "app_tokens": app_tokens.EMPTY_SEED,
        "vpn.json": "{}",
    }


class BootstrapTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.token_file = pathlib.Path(self.directory.name) / "owner.token"

    def tearDown(self):
        self.directory.cleanup()

    def run_bootstrap(self, bao, cluster, **flags):
        output = []
        app_tokens.bootstrap(
            bao, kube_for(cluster), app_tokens.PATH, "Стас", self.token_file,
            out=output.append, **flags,
        )
        return output

    def test_first_bootstrap_issues_the_owner_without_touching_service_consumers(self):
        bao = FakeBao(initial_values())
        cluster = FakeCluster(bao, lag=3)
        output = self.run_bootstrap(bao, cluster)
        token = self.token_file.read_text().strip()
        self.assertEqual(stat.S_IMODE(self.token_file.stat().st_mode), 0o600)
        seed = json.loads(bao.values["app_tokens"])
        self.assertEqual(seed["tokens"][0]["id"], "owner")
        self.assertEqual(seed["tokens"][0]["name"], "Стас")
        self.assertEqual(seed["tokens"][0]["sha256"], app_tokens.digest(token))
        self.assertEqual(bao.values["service_token"], "service-1")
        self.assertEqual(cluster.restarts, ["scheduler"])
        self.assertEqual(cluster.secrets["scheduler-auth"]["app_tokens"], bao.values["app_tokens"])
        self.assertTrue(all(token not in line for line in output))

    def test_a_repeated_bootstrap_keeps_the_owner_token(self):
        bao = FakeBao(initial_values())
        cluster = FakeCluster(bao)
        self.run_bootstrap(bao, cluster)
        token = self.token_file.read_text()
        seed = bao.values["app_tokens"]
        self.run_bootstrap(bao, cluster)
        self.assertEqual(self.token_file.read_text(), token)
        self.assertEqual(bao.values["app_tokens"], seed)

    def test_a_lost_token_file_requires_an_explicit_rotation(self):
        bao = FakeBao(initial_values())
        cluster = FakeCluster(bao)
        self.run_bootstrap(bao, cluster)
        self.token_file.unlink()
        with self.assertRaises(SystemExit) as error:
            self.run_bootstrap(bao, cluster)
        self.assertIn("--rotate-owner", str(error.exception))
        self.run_bootstrap(bao, cluster, rotate_owner=True)
        self.assertTrue(self.token_file.exists())

    def test_the_file_is_not_replaced_when_openbao_rejects_the_write(self):
        self.token_file.write_text("previous\n")
        bao = FakeBao(initial_values())
        bao.fail_post = True
        with self.assertRaises(RuntimeError):
            self.run_bootstrap(bao, FakeCluster(bao), rotate_owner=True)
        self.assertEqual(self.token_file.read_text(), "previous\n")
        self.assertEqual(list(pathlib.Path(self.directory.name).iterdir()), [self.token_file])

    def test_rotation_restarts_existing_consumers_and_finalizes_with_an_empty_previous(self):
        bao = FakeBao(initial_values())
        cluster = FakeCluster(bao, deployments=["scheduler", "phone-push", "hermes"], lag=2)
        self.run_bootstrap(bao, cluster, rotate_service=True)
        self.assertNotEqual(bao.values["service_token"], "service-1")
        self.assertIn("service_token_previous", bao.values)
        self.assertEqual(bao.values["service_token_previous"], "")
        self.assertEqual(cluster.restarts, ["scheduler", "phone-push", "hermes", "scheduler"])
        self.assertEqual(cluster.secrets["scheduler-auth"]["service_token_previous"], "")
        for name in app_tokens.SERVICE_SECRETS:
            self.assertEqual(cluster.secrets[name]["service_token"], bao.values["service_token"])

    def test_an_interrupted_rotation_is_finished_without_a_second_rotation(self):
        values = initial_values()
        values["service_token"] = "service-2"
        values["service_token_previous"] = "service-1"
        bao = FakeBao(values)
        cluster = FakeCluster(bao)
        self.run_bootstrap(bao, cluster)
        self.assertEqual(bao.values["service_token"], "service-2")
        self.assertEqual(bao.values["service_token_previous"], "")

    def test_force_sync_waits_for_a_new_sync_not_the_old_ready_condition(self):
        bao = FakeBao(initial_values())
        cluster = FakeCluster(bao, lag=5)
        kube = kube_for(cluster)
        bao.values["app_tokens"] = '{"revision":0,"tokens":[]} '
        kube.force_sync({"scheduler-auth": {"app_tokens": bao.values["app_tokens"]}})
        self.assertEqual(cluster.secrets["scheduler-auth"]["app_tokens"], bao.values["app_tokens"])

    def test_force_sync_times_out_when_eso_never_syncs(self):
        bao = FakeBao(initial_values())
        cluster = FakeCluster(bao, lag=10**9)
        with self.assertRaises(SystemExit) as error:
            kube_for(cluster).force_sync({"scheduler-auth": {"service_token": "x"}})
        self.assertIn("timed out", str(error.exception))

    def test_verification_fails_when_the_scheduler_runs_an_old_token(self):
        bao = FakeBao(initial_values())
        cluster = FakeCluster(bao)
        cluster.loaded = "stale"
        cluster.deployments.discard("scheduler")
        with self.assertRaises(SystemExit) as error:
            self.run_bootstrap(bao, cluster)
        self.assertIn("Pod/scheduler", str(error.exception))

    def test_preflight_requires_hermes_on_the_service_token(self):
        bao = FakeBao(initial_values())
        cluster = FakeCluster(bao, hermes_key="token")
        with self.assertRaises(SystemExit) as error:
            self.run_bootstrap(bao, cluster)
        self.assertIn("HERMES_SCHEDULER_TOKEN", str(error.exception))
        self.assertEqual([call[0] for call in bao.calls], ["GET"])
        self.assertFalse(self.token_file.exists())

    def test_dry_run_changes_nothing(self):
        bao = FakeBao(initial_values())
        cluster = FakeCluster(bao)
        output = self.run_bootstrap(bao, cluster, dry_run=True)
        self.assertEqual([call[0] for call in bao.calls], ["GET"])
        self.assertFalse(self.token_file.exists())
        self.assertEqual(cluster.restarts, [])
        self.assertIn("dry run", output[0])


class ConsumerNamesTests(unittest.TestCase):
    def test_restarted_deployments_and_secrets_exist_in_the_repository(self):
        root = pathlib.Path(__file__).resolve().parents[2]
        deployments = set()
        externalsecrets = set()
        for path in root.glob("projects/home/kubernetes/*/manifests/*.yaml"):
            for document in yaml.safe_load_all(path.read_text(encoding="utf-8")):
                if not isinstance(document, dict):
                    continue
                name = (document.get("metadata") or {}).get("name")
                if document.get("kind") == "Deployment":
                    deployments.add(name)
                if document.get("kind") == "ExternalSecret":
                    externalsecrets.add(name)
        helm = (root / "projects/home-assistant/deploy/flux/helmrelease.yaml").read_text()
        if "name: hermes" in helm:
            deployments.add("hermes")
        self.assertLessEqual({app_tokens.SCHEDULER, *app_tokens.SERVICE_CONSUMERS}, deployments)
        self.assertLessEqual(set(app_tokens.SERVICE_SECRETS), externalsecrets)
        self.assertNotIn("voice-proxy", app_tokens.SERVICE_CONSUMERS)


class PreflightTests(unittest.TestCase):
    def test_legacy_value_is_found_everywhere_except_service_secrets(self):
        legacy = "legacy-token"
        secrets_doc = {
            "items": [
                {
                    "metadata": {"name": "scheduler-auth"},
                    "data": {"token": base64.b64encode(legacy.encode()).decode()},
                },
                {
                    "metadata": {"name": "runtime-values"},
                    "data": {"values": base64.b64encode(f"x: {legacy}".encode()).decode()},
                },
            ]
        }
        configmaps = {"items": [{"metadata": {"name": "ha-config"}, "data": {"token": legacy}}]}
        found = app_tokens.legacy_consumers(legacy, secrets_doc, configmaps)
        self.assertEqual(found, ["ConfigMap/ha-config:token", "Secret/runtime-values:values"])


class InitTests(unittest.TestCase):
    def test_init_creates_every_key_once_and_never_prints(self):
        bao = FakeBao({"token": "mama", "vpn.json": "{}"})
        output = io.StringIO()
        with redirect_stdout(output):
            self.assertTrue(app_tokens.init_service_token(bao, app_tokens.PATH))
        created = bao.values["service_token"]
        self.assertTrue(created.startswith("home_"))
        self.assertEqual(bao.values["service_token_previous"], "")
        self.assertEqual(json.loads(bao.values["app_tokens"]), {"revision": 0, "tokens": []})
        self.assertEqual(bao.values["vpn.json"], "{}")
        self.assertFalse(app_tokens.init_service_token(bao, app_tokens.PATH))
        self.assertEqual(bao.values["service_token"], created)
        self.assertEqual(output.getvalue(), "")

    def test_init_fills_missing_keys_without_rotating_an_existing_token(self):
        bao = FakeBao({"token": "mama", "service_token": "kept"})
        self.assertTrue(app_tokens.init_service_token(bao, app_tokens.PATH))
        self.assertEqual(bao.values["service_token"], "kept")
        self.assertEqual(bao.values["service_token_previous"], "")
        self.assertIn("app_tokens", bao.values)


if __name__ == "__main__":
    unittest.main()
