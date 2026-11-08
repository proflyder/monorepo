import base64
import gzip
import io
import json
import pathlib
import stat
import tarfile
import tempfile
import unittest
from unittest import mock

import workstation as ws

CATALOG = ws.load_catalog()
REAL_BAO = ws.Bao


class FakeBao:
    """In-memory OpenBao speaking the endpoints secrets.py uses."""

    def __init__(self, kv, mfa=True):
        self.kv, self.mfa, self.calls, self.revoked, self.created = kv, mfa, [], [], []

    def __call__(self, addr, token=None):
        bao = REAL_BAO(addr, token)
        bao.call = lambda m, p, b=None, missing_ok=False: self.call(bao, m, p, b, missing_ok)
        return bao

    def call(self, bao, method, path, body, missing_ok):
        self.calls.append((method, path, bao.token))
        if path.startswith("auth/userpass/login/"):
            assert body["password"] == "pw"
            if self.mfa:
                return {"auth": {"mfa_requirement": {
                    "mfa_request_id": "req", "mfa_constraints": {"c": {"any": [{"id": "m1"}]}}}}}
            return {"auth": {"client_token": "human"}}
        if path == "sys/mfa/validate":
            assert body == {"mfa_request_id": "req", "mfa_payload": {"m1": ["123456"]}}
            return {"auth": {"client_token": "human"}}
        if path == "auth/token/create-orphan":
            assert bao.token == "human" and body["policies"] == ["operator-kv"]
            self.created.append(body)
            return {"auth": {"client_token": "machine"}}
        if path == "auth/token/revoke-self":
            self.revoked.append(bao.token)
            return {}
        if path == "auth/token/renew-self":
            return {"auth": {"policies": ["default", "operator-kv"], "lease_duration": 3600 * 720}}
        if path.startswith("secret/data/"):
            key = path[len("secret/data/"):]
            if key in self.kv:
                return {"data": {"data": self.kv[key]}}
            return None if missing_ok else self._missing(path)
        if path.startswith("secret/metadata/"):
            prefix = path[len("secret/metadata/"):]
            keys = sorted({k[len(prefix):].split("/")[0] + ("/" if "/" in k[len(prefix):] else "")
                           for k in self.kv if k.startswith(prefix)})
            return {"data": {"keys": keys}} if keys else None
        raise AssertionError(path)

    @staticmethod
    def _missing(path):
        raise ws.Error(f"missing {path}")


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = pathlib.Path(tempfile.mkdtemp())
        self.secrets = self.tmp / "repo" / "secrets"
        self.home = self.tmp / "home"
        (self.secrets / "openbao").mkdir(parents=True)
        (self.secrets / "openbao" / "addr").write_text("https://vault.example.test\n")
        (self.secrets / "openbao" / "operator.token").write_text("machine\n")
        self.out = []
        env = mock.patch.dict("os.environ", {"HOME": str(self.home)})
        env.start()
        self.addCleanup(env.stop)
        import os
        for var in ("BAO_ADDR", "BAO_TOKEN"):
            os.environ.pop(var, None)  # restored by the patch above
        checkout = mock.patch.object(ws, "main_checkout", return_value=None)
        checkout.start()
        self.addCleanup(checkout.stop)

    def args(self, **kw):
        base = dict(secrets_dir=self.secrets, catalog=self.catalog_path())
        base.update(kw)
        return mock.Mock(**base)

    def catalog_path(self, catalog=None):
        path = self.tmp / "catalog.json"
        path.write_text(json.dumps(catalog or self.catalog()))
        return path

    def catalog(self):
        return {"paths": {"infra/ssh/a": {}, "infra/docs/n": {}, "infra/f": {}},
                "pull": [{"path": "infra/ssh/a", "field": "key", "dest": "~/.ssh/home-lab/a"},
                         {"path": "infra/docs/n", "field": "md", "dest": "secrets/NETWORK.md"}],
                "files": [{"file": "home/a.env", "path": "infra/f", "field": "env", "encoding": "text+nl"},
                          {"file": "vpn/ca", "path": "infra/f", "field": "tar", "encoding": "tar_b64"}]}

    def kv(self):
        archive = io.BytesIO()
        with tarfile.open(fileobj=archive, mode="w:gz") as t:
            info = tarfile.TarInfo("ca/k.pem")
            info.size = 1
            t.addfile(info, io.BytesIO(b"k"))
        return {"infra/ssh/a": {"key": "PRIVATE\n"}, "infra/docs/n": {"md": "# net\n"},
                "infra/f": {"env": "A=1", "tar": base64.b64encode(archive.getvalue()).decode()}}


class Login(Base):
    def run_login(self, fake, previous=True):
        if not previous:
            (self.secrets / "openbao" / "operator.token").unlink()
        answers = iter(["pw", "123456"])
        with mock.patch.object(ws, "Bao", fake):
            ws.cmd_login(self.args(addr=None, user="owner", label="laptop", revoke_previous=True),
                         out=self.out.append, ask=lambda _: "", secret=lambda _: next(answers))

    def test_totp_login_mints_machine_token_and_revokes_human(self):
        fake = FakeBao({})
        self.run_login(fake)
        token = self.secrets / "openbao" / "operator.token"
        self.assertEqual(token.read_text().strip(), "machine")
        self.assertEqual(stat.S_IMODE(token.stat().st_mode), 0o600)
        self.assertEqual(fake.created[0]["display_name"], "workstation-laptop")
        self.assertEqual(fake.created[0]["period"], "720h")
        self.assertIn("human", fake.revoked)  # human session never outlives login
        self.assertIn("machine", fake.revoked)  # previous token of this machine
        self.assertNotIn("machine", " ".join(self.out))

    def test_login_without_mfa_and_without_previous_token(self):
        fake = FakeBao({}, mfa=False)
        answers = iter(["pw"])
        (self.secrets / "openbao" / "operator.token").unlink()
        with mock.patch.object(ws, "Bao", fake):
            ws.cmd_login(self.args(addr=None, user="owner", label="x", revoke_previous=True),
                         out=self.out.append, ask=lambda _: "", secret=lambda _: next(answers))
        self.assertEqual(fake.revoked, ["human"])


class PullFileCheck(Base):
    def test_pull_writes_private_files_and_is_idempotent(self):
        fake = FakeBao(self.kv())
        with mock.patch.object(ws, "Bao", fake), mock.patch.object(ws, "ROOT", self.tmp / "repo"):
            ws.cmd_pull(self.args(dry_run=False), out=self.out.append)
            key = self.home / ".ssh/home-lab/a"
            self.assertEqual(key.read_text(), "PRIVATE\n")
            self.assertEqual(stat.S_IMODE(key.stat().st_mode), 0o600)
            self.assertEqual((self.secrets / "NETWORK.md").read_text(), "# net\n")
            self.out.clear()
            ws.cmd_pull(self.args(dry_run=False), out=self.out.append)
        self.assertEqual(self.out, ["pull: 0 changed"])
        self.assertTrue(any(c[1] == "auth/token/renew-self" for c in fake.calls))

    def test_file_and_clean_round_trip(self):
        fake = FakeBao(self.kv())
        with mock.patch.object(ws, "Bao", fake):
            ws.cmd_file(self.args(names=[], all=True, clean=False), out=self.out.append)
            self.assertEqual((self.secrets / "home/a.env").read_bytes(), b"A=1\n")
            self.assertEqual((self.secrets / "vpn/ca/k.pem").read_bytes(), b"k")
            self.assertNotIn("A=1", "\n".join(self.out))
            ws.cmd_file(self.args(names=["home"], all=False, clean=True), out=self.out.append)
        self.assertFalse((self.secrets / "home").exists())  # empty dirs pruned
        self.assertTrue((self.secrets / "vpn/ca/k.pem").exists())

    def test_unknown_and_escaping_names_rejected(self):
        with self.assertRaises(SystemExit):
            ws.select(self.catalog()["files"], ["nope"], False)
        with self.assertRaises(SystemExit):
            ws.safe_target(self.secrets, "../outside")

    def test_check_reports_drift(self):
        kv = self.kv()
        kv["infra/new"] = {"x": "1"}
        del kv["infra/docs/n"]
        with mock.patch.object(ws, "Bao", FakeBao(kv)):
            code = ws.cmd_check(self.args(), out=self.out.append)
        self.assertEqual(code, 1)
        text = "\n".join(self.out)
        self.assertIn("in OpenBao, missing in catalog: infra/new", text)
        self.assertIn("in catalog, missing in OpenBao: infra/docs/n", text)

    def test_missing_token_points_to_login(self):
        (self.secrets / "openbao" / "operator.token").unlink()
        with self.assertRaises(SystemExit) as caught:
            ws.operator(self.secrets)
        self.assertIn("secrets:login", str(caught.exception))


class Worktree(Base):
    def test_state_falls_back_to_main_checkout(self):
        worktree = self.tmp / "wt" / "secrets"
        with mock.patch.object(ws, "main_checkout", return_value=self.tmp / "repo"):
            self.assertEqual(ws.state_dir(worktree), self.tmp / "repo" / "secrets")
            self.assertEqual(ws.address(worktree), "https://vault.example.test")


class Decode(unittest.TestCase):
    def test_encodings(self):
        self.assertEqual(ws.decode("x", "text+nl"), b"x\n")
        self.assertEqual(ws.decode("abc", "to_b64"), b"YWJj")
        packed = base64.b64encode(gzip.compress(b"z")).decode()
        self.assertEqual(ws.decode(packed, "gzip_b64"), b"z")
        with self.assertRaises(ValueError):
            ws.decode("x", "rot13")


class Catalog(unittest.TestCase):
    def test_every_used_path_is_documented_and_public_safe(self):
        documented = set(CATALOG["paths"])
        for entry in CATALOG["pull"] + CATALOG["files"]:
            self.assertIn(entry["path"], documented)
        for entry in CATALOG["pull"]:
            self.assertTrue(entry["dest"].startswith(("~/.ssh/home-lab/", "secrets/")), entry)
        for meta in CATALOG["paths"].values():
            self.assertTrue(meta["purpose"] and meta["consumer"])

    def test_fnox_references_documented_paths(self):
        refs = ws.fnox_refs()
        self.assertTrue(refs)
        documented = set(CATALOG["paths"])
        for ref in refs:
            self.assertIn(ref["path"], documented, ref)
        # nothing outside a profile: a bare `fnox exec` must resolve no secret
        self.assertFalse([r for r in refs if r["profile"] == "default"])

    def test_admin_tasks_use_defined_profiles(self):
        import re
        profiles = {r["profile"] for r in ws.fnox_refs()}
        tasks = sorted((ws.ROOT / "platform/mise-tasks").rglob("*.sh"))
        used = set()
        for task in tasks:
            for profile in re.findall(r"fnox\.sh\" -P ([a-z-]+) exec", task.read_text()):
                used.add(profile)
                self.assertIn(profile, profiles, task)
        self.assertGreaterEqual(len(used), 7)
        service = (ws.ROOT / "platform/mise-tasks/admin/service.sh").read_text()
        units = re.search(r"\n  ([a-z|-]+)\) ;;", service).group(1).split("|")
        for unit in units:
            self.assertIn(unit, profiles, unit)
            self.assertTrue((ws.ROOT / f"projects/home/kubernetes/{unit}/deploy.sh").is_file())
        for script in re.findall(r'"\$ROOT/(projects/[^"]+)"',
                                 "".join(t.read_text() for t in tasks if "admin" in t.parts)):
            if "$" not in script:  # admin:service builds its path from the unit
                self.assertTrue((ws.ROOT / script).is_file(), script)

    def test_docs_table_lists_every_path(self):
        doc = (ws.ROOT / "docs" / "SECRETS.md").read_text()
        for path in CATALOG["paths"]:
            self.assertIn(f"`{path}`", doc)


if __name__ == "__main__":
    unittest.main()
