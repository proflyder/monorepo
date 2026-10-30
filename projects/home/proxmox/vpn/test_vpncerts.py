#!/usr/bin/env python3
"""Unit and integration tests for vpncerts.py (no network, no live VPN)."""
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

import vpncerts

OPENSSL = shutil.which("openssl")


class HelpersTest(unittest.TestCase):
    def test_validate_user(self):
        self.assertEqual(vpncerts.validate_user("alice"), "alice")
        for bad in ("Alice", "1alice", "al ice", ""):
            with self.assertRaises(vpncerts.CertError):
                vpncerts.validate_user(bad)

    def test_validate_device(self):
        self.assertEqual(vpncerts.validate_device("phone-2"), "phone-2")
        for bad in ("Phone", "-phone", "ph one", ""):
            with self.assertRaises(vpncerts.CertError):
                vpncerts.validate_device(bad)

    def test_client_basename(self):
        self.assertEqual(vpncerts.client_basename("alice", "phone"), "alice-phone")

    def test_normalize_serial(self):
        self.assertEqual(vpncerts.normalize_serial("0xab12"), "AB12")
        self.assertEqual(vpncerts.normalize_serial("deadBEEF"), "DEADBEEF")
        for bad in ("", "xyz", "0x", "ab12\ncd"):
            with self.assertRaises(vpncerts.CertError):
                vpncerts.normalize_serial(bad)

    def test_parse_groups(self):
        self.assertEqual(vpncerts.parse_groups("lan, full"), ["lan", "full"])
        with self.assertRaises(vpncerts.CertError):
            vpncerts.parse_groups("")
        with self.assertRaises(vpncerts.CertError):
            vpncerts.parse_groups("lan,Bad Group")

    def test_cert_subject_uses_cn_for_user_and_ou_for_groups(self):
        self.assertEqual(
            vpncerts.cert_subject("alice", ["lan", "full"]),
            "/CN=alice/OU=lan/OU=full",
        )

    def test_openssl_config_marks_client_certificate(self):
        conf = vpncerts.openssl_config(Path("/tmp/ca"), org="home VPN")
        self.assertIn("dir = /tmp/ca", conf)
        self.assertIn("database = $dir/index.txt", conf)
        self.assertIn("certificate = $dir/ca-cert.pem", conf)
        self.assertIn("private_key = $dir/ca-key.pem", conf)
        self.assertIn("serial = $dir/serial", conf)
        self.assertIn("crlnumber = $dir/crlnumber", conf)
        self.assertIn("basicConstraints = critical,CA:FALSE", conf)
        self.assertIn("extendedKeyUsage = clientAuth", conf)
        self.assertIn("basicConstraints = critical,CA:true", conf)

    def test_parse_index(self):
        text = (
            "V\t270101000000Z\t\t0A1B\tunknown\t/CN=alice/OU=phone\n"
            "R\t270101000000Z\t250101000000Z\t0A1C\tunknown\t/CN=bob/OU=laptop\n"
        )
        entries = vpncerts.parse_index(text)
        self.assertEqual(len(entries), 2)
        self.assertEqual(entries[0]["status"], "V")
        self.assertEqual(entries[0]["serial"], "0A1B")
        self.assertEqual(entries[1]["status"], "R")
        self.assertEqual(entries[1]["revocation"], "250101000000Z")
        with self.assertRaises(vpncerts.CertError):
            vpncerts.parse_index("broken\n")

    def test_index_common_name(self):
        self.assertEqual(
            vpncerts.index_common_name("/CN=alice/OU=phone"), "alice"
        )
        self.assertEqual(vpncerts.index_common_name("/O=none"), "/O=none")

    def test_asn1_time_to_date(self):
        self.assertEqual(vpncerts.asn1_time_to_date("270101000000Z"), "2027-01-01")
        self.assertEqual(vpncerts.asn1_time_to_date("20500101000000Z"), "2050-01-01")
        self.assertEqual(vpncerts.asn1_time_to_date(""), "-")


@unittest.skipUnless(OPENSSL, "openssl is not installed")
class LocalCaTest(unittest.TestCase):
    """Real openssl CA in a temporary directory; nothing touches the network."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.ca = self.root / "ca"
        self.clients = self.root / "clients"
        self.ssl = vpncerts.Openssl(OPENSSL)

    def tearDown(self):
        self.tmp.cleanup()

    def _init(self):
        vpncerts.ensure_ca(self.ca, "test", self.ssl)
        self.assertTrue((self.ca / "ca-key.pem").exists())
        self.assertEqual((self.ca / "ca-key.pem").stat().st_mode & 0o777, 0o600)
        self.assertTrue((self.ca / "crl.pem").exists())

    def test_init_ca_is_idempotent_guard(self):
        self._init()
        with self.assertRaises(vpncerts.CertError):
            vpncerts.ensure_ca(self.ca, "test", self.ssl)
        vpncerts.ensure_ca(self.ca, "test", self.ssl, force=True)

    def test_issue_creates_modern_and_legacy_p12(self):
        self._init()
        result = vpncerts.issue(
            self.ca, self.clients, "alice", "phone", self.ssl
        )
        modern = Path(result["p12"])
        legacy = Path(result["legacy_p12"])
        password_file = Path(result["password_file"])
        self.assertEqual(modern.name, "alice-phone.p12")
        self.assertEqual(legacy.name, "alice-phone.legacy.p12")
        self.assertEqual(password_file.stat().st_mode & 0o777, 0o600)
        self.assertEqual(vpncerts.normalize_serial(result["serial"]), result["serial"])

        password = password_file.read_text(encoding="utf-8").strip()
        for archive in (modern, legacy):
            pem = subprocess.run(
                ["openssl", "pkcs12", "-in", str(archive), "-clcerts", "-nokeys",
                 "-passin", "stdin", "-provider", "default", "-provider", "legacy"],
                input=password + "\n", text=True, capture_output=True, check=True,
            ).stdout
            cert = self.root / "extracted.pem"
            cert.write_text(pem, encoding="utf-8")
            info = subprocess.run(
                ["openssl", "x509", "-in", str(cert), "-noout", "-serial", "-subject",
                 "-ext", "extendedKeyUsage"],
                text=True, capture_output=True, check=True,
            ).stdout
            # OpenSSL 3 prints "CN = alice", LibreSSL "CN=alice".
            subject = info.replace(" = ", "=")
            self.assertIn("CN=alice", subject)
            self.assertIn("OU=lan", subject)
            self.assertIn("OU=full", subject)
            self.assertIn("TLS Web Client Authentication", info)

    def test_serials_are_unique_and_listed(self):
        self._init()
        first = vpncerts.issue(self.ca, self.clients, "alice", "phone", self.ssl)
        second = vpncerts.issue(self.ca, self.clients, "alice", "tablet", self.ssl)
        self.assertNotEqual(first["serial"], second["serial"])
        entries = vpncerts.list_entries(self.ca)
        serials = {entry["serial"] for entry in entries}
        self.assertEqual(serials, {first["serial"], second["serial"]})
        self.assertTrue(all(entry["status"] == "V" for entry in entries))

    def test_revoke_by_serial_updates_crl_and_list(self):
        self._init()
        issued = vpncerts.issue(self.ca, self.clients, "alice", "phone", self.ssl)
        before = subprocess.run(
            ["openssl", "crl", "-in", str(self.ca / "crl.pem"), "-noout", "-text"],
            text=True, capture_output=True, check=True,
        ).stdout
        self.assertNotIn(f"Serial Number: {issued['serial']}", before)

        result = vpncerts.revoke(self.ca, self.clients, issued["serial"], self.ssl)
        self.assertEqual(result["serial"], issued["serial"])
        after = subprocess.run(
            ["openssl", "crl", "-in", str(self.ca / "crl.pem"), "-noout", "-text"],
            text=True, capture_output=True, check=True,
        ).stdout
        self.assertIn(f"Serial Number: {issued['serial']}", after)
        revoked = [e for e in vpncerts.list_entries(self.ca) if e["status"] == "R"]
        self.assertEqual([e["serial"] for e in revoked], [issued["serial"]])

    def test_revoke_by_p12(self):
        self._init()
        issued = vpncerts.issue(self.ca, self.clients, "bob", "laptop", self.ssl)
        result = vpncerts.revoke(self.ca, self.clients, issued["p12"], self.ssl)
        self.assertEqual(result["serial"], issued["serial"])

    def test_revoke_unknown_serial_fails(self):
        self._init()
        with self.assertRaises(vpncerts.CertError):
            vpncerts.revoke(self.ca, self.clients, "DEADBEEF", self.ssl)


if __name__ == "__main__":
    unittest.main()
