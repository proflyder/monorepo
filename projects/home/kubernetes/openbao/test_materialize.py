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

import materialize


class Decode(unittest.TestCase):
    def test_round_trips(self):
        raw = b"KEY=value\n"
        self.assertEqual(materialize.decode("KEY=value", "text+nl"), raw)
        self.assertEqual(materialize.decode(base64.b64encode(raw).decode(), "b64"), raw)
        self.assertEqual(materialize.decode("abc", "to_b64"), b"YWJj")
        packed = base64.b64encode(gzip.compress(raw)).decode()
        self.assertEqual(materialize.decode(packed, "gzip_b64"), raw)
        with self.assertRaises(ValueError):
            materialize.decode("x", "rot13")


class Materialize(unittest.TestCase):
    def setUp(self):
        self.tmp = pathlib.Path(tempfile.mkdtemp())
        bio = io.BytesIO()
        with tarfile.open(fileobj=bio, mode="w:gz") as archive:
            info = tarfile.TarInfo("ca/ca-key.pem")
            info.size = 3
            archive.addfile(info, io.BytesIO(b"key"))
        self.values = {("infra/a", "env"): "A=1",
                       ("infra/b", "dir"): base64.b64encode(bio.getvalue()).decode()}
        catalog = {"entries": [
            {"file": "home/a.env", "path": "infra/a", "field": "env", "encoding": "text+nl"},
            {"file": "vpn/ca", "path": "infra/b", "field": "dir", "encoding": "tar_b64"},
            {"path": "infra/c", "field": "pw", "note": "no local file"}]}
        self.catalog = self.tmp / "catalog.json"
        self.catalog.write_text(json.dumps(catalog))
        self.dest = self.tmp / "secrets"

    def run_main(self, *args):
        fake = mock.Mock()
        fake.field.side_effect = lambda p, f: self.values[(p, f)]
        with mock.patch.object(materialize, "Bao", return_value=fake), \
             mock.patch.object(materialize, "token", return_value="t"), \
             mock.patch.object(materialize, "renew") as self.renewed, \
             mock.patch.dict("os.environ", {"BAO_ADDR": "http://bao"}), \
             mock.patch("sys.stdout", new=io.StringIO()) as out:
            materialize.main(["--catalog", str(self.catalog), "--dest", str(self.dest), *args])
        return out.getvalue()

    def test_all_then_clean(self):
        out = self.run_main("--all")
        env = self.dest / "home/a.env"
        self.assertEqual(env.read_bytes(), b"A=1\n")
        self.assertEqual(stat.S_IMODE(env.stat().st_mode), 0o600)
        self.assertEqual((self.dest / "vpn/ca/ca-key.pem").read_bytes(), b"key")
        self.assertNotIn("A=1", out)
        self.run_main("--clean", "--all")
        self.assertFalse(env.exists())
        self.assertFalse((self.dest / "vpn/ca").exists())

    def test_write_renews_token_and_clean_does_not(self):
        self.run_main("--all")
        self.renewed.assert_called_once_with("http://bao", "t")
        self.run_main("--clean", "--all")
        self.renewed.assert_not_called()

    def test_address_falls_back_to_secrets_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            (pathlib.Path(tmp) / "openbao").mkdir()
            (pathlib.Path(tmp) / "openbao" / "addr").write_text("https://vault.example.test\n")
            with mock.patch.object(materialize, "SECRETS", pathlib.Path(tmp)), \
                 mock.patch.dict("os.environ", {}, clear=True):
                self.assertEqual(materialize.address(), "https://vault.example.test")

    def test_prefix_selects_directory(self):
        self.run_main("home")
        self.assertTrue((self.dest / "home/a.env").exists())
        self.assertFalse((self.dest / "vpn").exists())

    def test_unknown_and_escaping_paths_rejected(self):
        with self.assertRaises(SystemExit):
            self.run_main("nope")
        with self.assertRaises(SystemExit):
            materialize.safe_target(self.dest, "../outside")


if __name__ == "__main__":
    unittest.main()
