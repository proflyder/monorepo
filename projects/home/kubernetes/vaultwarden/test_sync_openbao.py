import base64
import json
import pathlib
import tempfile
import unittest
import unittest.mock

import sync_openbao as s


class FakeBw(s.Bw):
    """In-memory vault speaking the subset of the bw CLI the sync uses."""

    def __init__(self):
        self.folders, self.items, self.calls, self.files = [], {}, [], {}
        self.seq = 0

    def _id(self):
        self.seq += 1
        return f"id{self.seq}"

    def run(self, *args, stdin=None):
        self.calls.append(args)
        cmd = args[:2]
        body = json.loads(base64.b64decode(stdin)) if stdin else None
        if args[0] == "sync":
            return ""
        if cmd == ("list", "folders"):
            return json.dumps(self.folders)
        if cmd == ("create", "folder"):
            folder = dict(body, id=self._id()); self.folders.append(folder)
            return json.dumps(folder)
        if cmd == ("list", "items"):
            fid = args[3]
            return json.dumps([i for i in self.items.values() if i["folderId"] == fid])
        if cmd == ("create", "item"):
            item = dict(body, id=self._id(), attachments=[]); self.items[item["id"]] = item
            return json.dumps(item)
        if cmd == ("edit", "item"):
            item = dict(body, id=args[2]); self.items[args[2]] = item
            return json.dumps(item)
        if cmd == ("delete", "attachment"):
            item = self.items[args[4]]
            item["attachments"] = [a for a in item["attachments"] if a["id"] != args[2]]
            return ""
        if cmd == ("create", "attachment"):
            path = pathlib.Path(args[3])
            item = self.items[args[5]]
            self.files[(args[5], path.name)] = path.read_text()
            item.setdefault("attachments", []).append({"id": self._id(), "fileName": path.name})
            return "{}"
        raise AssertionError(args)


# Split so repository scanners do not mistake the fixture for a real key.
PEM = "-----BEGIN OPENSSH " + "PRIVATE KEY-----\nAAA\n-----END OPENSSH " + "PRIVATE KEY-----"
SNAPSHOT = {
    "infra/ssh/demo": {"key": PEM,
                       "key_pub": "ssh-ed25519 AAAA demo", "known_hosts": "h ssh-ed25519 X"},
    "k8s/app/big": {"token": "t" * 10, "blob": "b" * (s.FIELD_LIMIT + 1)},
}


class Sync(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.patcher = unittest.mock.patch.object(
            s, "public_and_fingerprint", lambda private, public: (public or "derived", "SHA256:fp"))
        cls.patcher.start()

    @classmethod
    def tearDownClass(cls):
        cls.patcher.stop()

    def setUp(self):
        self.bw = FakeBw()
        self.log = []

    def run_sync(self, snapshot=SNAPSHOT, dry=False):
        return s.sync(self.bw, s.desired(snapshot, None), dry, log=self.log.append)

    def test_first_run_creates_notes_ssh_items_and_attachments(self):
        stats = self.run_sync()
        self.assertEqual(stats["created"], 3)
        names = {i["name"]: i for i in self.bw.items.values()}
        self.assertEqual(set(names), {"openbao:infra/ssh/demo", "openbao:k8s/app/big",
                                      "ssh:infra/ssh/demo/key"})
        big = names["openbao:k8s/app/big"]
        self.assertEqual({f["name"] for f in big["fields"]}, {"token", "_digest"})
        self.assertTrue(all(f["type"] == s.HIDDEN for f in big["fields"] if f["name"] != "_digest"))
        self.assertEqual(self.bw.files[(big["id"], "blob.txt")], "b" * (s.FIELD_LIMIT + 1))
        ssh = names["ssh:infra/ssh/demo/key"]
        self.assertEqual(ssh["type"], 5)
        self.assertEqual(ssh["sshKey"]["publicKey"], "ssh-ed25519 AAAA demo")
        self.assertEqual(ssh["sshKey"]["keyFingerprint"], "SHA256:fp")

    def test_rerun_is_noop_and_change_rewrites_in_place(self):
        self.run_sync()
        before = len(self.bw.items)
        self.assertEqual(self.run_sync()["unchanged"], 3)
        changed = json.loads(json.dumps(SNAPSHOT))
        changed["k8s/app/big"]["blob"] = "c" * (s.FIELD_LIMIT + 5)
        stats = self.run_sync(changed)
        self.assertEqual((stats["updated"], stats["unchanged"]), (1, 2))
        self.assertEqual(len(self.bw.items), before)
        big = next(i for i in self.bw.items.values() if i["name"] == "openbao:k8s/app/big")
        self.assertEqual(len(big["attachments"]), 1)

    def test_removed_path_is_reported_not_deleted(self):
        self.run_sync()
        stats = self.run_sync({"k8s/app/big": SNAPSHOT["k8s/app/big"]})
        self.assertEqual(stats["orphaned"], 2)
        self.assertEqual(len(self.bw.items), 3)
        self.assertFalse(any(c[0] == "delete" and c[1] == "item" for c in self.bw.calls))

    def test_dry_run_changes_nothing_and_logs_no_values(self):
        self.run_sync(dry=True)
        self.assertEqual(self.bw.items, {})
        self.assertNotIn("ttttt", "\n".join(self.log))

    def test_values_never_in_argv(self):
        self.run_sync()
        argv = json.dumps(self.bw.calls)
        self.assertNotIn("ttttt", argv)
        self.assertNotIn("OPENSSH", argv)

    def test_real_fingerprint_of_generated_key(self):
        import subprocess
        with tempfile.TemporaryDirectory() as tmp:
            key = pathlib.Path(tmp) / "k"
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(key)], check=True)
            self.patcher.stop()
            try:
                public, fp = s.public_and_fingerprint(key.read_text(), "")
            finally:
                self.patcher.start()
            self.assertEqual(public.split()[:2], key.with_suffix(".pub").read_text().split()[:2])
            self.assertTrue(fp.startswith("SHA256:"))

    def test_bootstrap_item(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = pathlib.Path(tmp) / "openbao"
            base.mkdir()
            (base / "operator.token").write_text("s.op\n")
            (base / "seal.key").write_bytes(b"\x00" * 32)
            (base / "init.json").write_text(json.dumps({"recovery_keys_b64": ["r1", "r2"]}))
            spec = s.bootstrap(pathlib.Path(tmp), "https://vault.example.test")
        self.assertEqual(spec["name"], "openbao:bootstrap")
        values = {**spec["fields"], **spec["attachments"]}
        for key in ("operator_token", "seal_key_b64", "recovery_key_1_b64", "recovery_key_2_b64",
                    "init_json", "bao_addr"):
            self.assertIn(key, values)
        self.assertEqual(base64.b64decode(values["seal_key_b64"]), b"\x00" * 32)


if __name__ == "__main__":
    unittest.main()
