import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import cloudflare_dns  # noqa: E402


class FakeApi:
    def __init__(self, records):
        self.records, self.calls = records, []

    def call(self, method, path, body=None):
        self.calls.append((method, path, body))
        if path.startswith("/zones?"):
            return {"result": [{"id": "z1", "name": "example.com"},
                               {"id": "z2", "name": "home.example.com"}]}
        if method == "GET":
            return {"result": self.records}
        return {"result": {}}


class EnsureRecord(unittest.TestCase):
    def test_creates_in_longest_zone(self):
        api = FakeApi([])
        msg = cloudflare_dns.ensure_a_record(api, "passwords.home.example.com", "192.0.2.27")
        self.assertIn("created", msg)
        method, path, body = api.calls[-1]
        self.assertEqual((method, path), ("POST", "/zones/z2/dns_records"))
        self.assertFalse(body["proxied"])

    def test_noop_and_update(self):
        same = [{"id": "r", "content": "192.0.2.27", "proxied": False}]
        self.assertIn("already", cloudflare_dns.ensure_a_record(FakeApi(same), "a.home.example.com", "192.0.2.27"))
        api = FakeApi([{"id": "r", "content": "192.0.2.9", "proxied": True}])
        self.assertIn("updated", cloudflare_dns.ensure_a_record(api, "a.home.example.com", "192.0.2.27"))
        self.assertEqual(api.calls[-1][0], "PUT")

    def test_unknown_zone_and_token_file(self):
        with self.assertRaises(SystemExit):
            cloudflare_dns.ensure_a_record(FakeApi([]), "x.other.test", "192.0.2.1")
        with tempfile.NamedTemporaryFile("w", suffix=".env") as handle:
            handle.write("OTHER=1\nCLOUDFLARE_API_TOKEN='abc'\n")
            handle.flush()
            self.assertEqual(cloudflare_dns.read_token(handle.name), "abc")


if __name__ == "__main__":
    unittest.main()
