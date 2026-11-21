import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SYNC = HERE / "wg-peer-sync.sh"
OCSERV = HERE / "install-ocserv.sh"
INSTALL = HERE / "install-wireguard.sh"
SERVER_KEY = "A" * 43 + "="
PHONE_KEY = "B" * 42 + "A="
LAPTOP_KEY = "C" * 42 + "A="


@unittest.skipUnless(shutil.which("jq"), "jq is required")
class PeerSync(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp(prefix="wg-sync-"))
        (self.dir / "server.key").write_text(SERVER_KEY + "\n")
        (self.dir / "sync.env").write_text(
            "WG_IFACE=wgtest\nWG_ADDRESS=10.77.2.1/24\nWG_PORT=51820\n"
            "WG_SYNC_URL=https://alarms.example.com/v1/vpn/peers\n"
        )

    def tearDown(self):
        shutil.rmtree(self.dir)

    def sync(self, *args):
        env = dict(os.environ, WG_DIR=str(self.dir), PATH="/usr/bin:/bin:" + os.environ["PATH"])
        return subprocess.run(["bash", str(SYNC), *args], env=env,
                              capture_output=True, text=True)

    def peers_file(self, payload):
        path = self.dir / "incoming.json"
        path.write_text(json.dumps(payload))
        return str(path)

    def test_render_only_without_cache_has_no_peers(self):
        result = self.sync("--render-only")
        self.assertEqual(result.returncode, 0, result.stderr)
        conf = (self.dir / "wgtest.conf").read_text()
        self.assertIn("Address = 10.77.2.1/24", conf)
        self.assertIn("ListenPort = 51820", conf)
        self.assertIn(f"PrivateKey = {SERVER_KEY}", conf)
        self.assertNotIn("[Peer]", conf)

    def test_valid_peers_are_rendered_sorted_and_cached(self):
        payload = {"address": "10.77.2.1/24", "peers": [
            {"publicKey": LAPTOP_KEY, "allowedIp": "10.77.2.3/32"},
            {"publicKey": PHONE_KEY, "allowedIp": "10.77.2.2/32"},
        ]}
        result = self.sync("--from-file", self.peers_file(payload))
        self.assertEqual(result.returncode, 0, result.stderr)
        conf = (self.dir / "wgtest.conf").read_text()
        self.assertLess(conf.index(PHONE_KEY), conf.index(LAPTOP_KEY))
        self.assertIn("AllowedIPs = 10.77.2.2/32", conf)
        self.assertEqual(oct((self.dir / "wgtest.conf").stat().st_mode & 0o777), "0o600")
        cached = json.loads((self.dir / "peers.json").read_text())
        self.assertEqual(len(cached["peers"]), 2)

    def test_invalid_lists_keep_the_last_good_config(self):
        good = {"peers": [{"publicKey": PHONE_KEY, "allowedIp": "10.77.2.2/32"}]}
        self.assertEqual(self.sync("--from-file", self.peers_file(good)).returncode, 0)
        before = (self.dir / "wgtest.conf").read_text()
        for bad in (
            {"peers": [{"publicKey": "short", "allowedIp": "10.77.2.2/32"}]},
            {"peers": [{"publicKey": PHONE_KEY, "allowedIp": "0.0.0.0/0"}]},
            {"peers": [{"publicKey": PHONE_KEY, "allowedIp": "10.77.2.2/32\nEndpoint = x"}]},
            {"peers": [{"publicKey": PHONE_KEY, "allowedIp": "10.77.2.2/32"},
                       {"publicKey": PHONE_KEY, "allowedIp": "10.77.2.3/32"}]},
            {"error": "unauthenticated"},
        ):
            with self.subTest(bad=bad):
                result = self.sync("--from-file", self.peers_file(bad))
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual((self.dir / "wgtest.conf").read_text(), before)

    def test_an_empty_list_removes_every_peer(self):
        good = {"peers": [{"publicKey": PHONE_KEY, "allowedIp": "10.77.2.2/32"}]}
        self.sync("--from-file", self.peers_file(good))
        result = self.sync("--from-file", self.peers_file({"peers": []}))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("[Peer]", (self.dir / "wgtest.conf").read_text())


class ScriptContracts(unittest.TestCase):
    def test_scripts_parse(self):
        for script in (SYNC, OCSERV, INSTALL):
            with self.subTest(script=script.name):
                subprocess.run(["bash", "-n", str(script)], check=True)

    def test_wireguard_never_takes_the_ocserv_port(self):
        for script in (OCSERV, INSTALL):
            text = script.read_text()
            with self.subTest(script=script.name):
                self.assertIn("!= 443", text)

    def test_ocserv_firewall_gives_the_wireguard_pool_the_lan_full_rules(self):
        text = OCSERV.read_text()
        self.assertIn('iifname \\"$IFACE\\" udp dport $WG_PORT accept', text)
        self.assertIn('iifname \\"$WG_IFACE\\" oifname \\"$IFACE\\" ip saddr $WG_CIDR accept', text)
        self.assertIn('ip saddr $WG_CIDR oifname \\"$IFACE\\" masquerade', text)

    def test_the_sync_token_never_reaches_argv(self):
        text = SYNC.read_text()
        self.assertIn('-H @"$WG_DIR/sync.header"', text)
        self.assertNotIn("Bearer", text)


if __name__ == "__main__":
    unittest.main()
