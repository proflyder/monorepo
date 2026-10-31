#!/usr/bin/env python3
"""Hermetic tests for the station egress-guard rule set.

The script is security-critical and intentionally only uses iptables/ip6tables;
fake those binaries and assert the allowlist instead of touching the host.
"""
from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "projects" / "station" / "scripts" / "station-egress-guard.sh"

FAKE_IPTABLES = r"""#!/bin/sh
printf '%s %s\n' "$(basename "$0")" "$*" >> "$IPT_LOG"
case "$*" in
  "-C EGRESS_GUARD -j DROP"|"-C EGRESS_GUARD6 -j DROP") exit 1 ;;
  "-C OUTPUT -j EGRESS_GUARD"|"-C OUTPUT -j EGRESS_GUARD6") exit 1 ;;
  "-C EGRESS_GUARD -d "*) exit 1 ;;
  *) exit 0 ;;
esac
"""


class EgressGuardTests(unittest.TestCase):
    def run_guard(self, allow_ntp: str = "1", allow_github: str = "1") -> str:
        tmp = Path(tempfile.mkdtemp(prefix="egress-test-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(tmp, ignore_errors=True))
        for name in ("iptables", "ip6tables"):
            path = tmp / name
            path.write_text(FAKE_IPTABLES)
            path.chmod(0o755)
        log = tmp / "iptables.log"
        env = os.environ.copy()
        env["PATH"] = f"{tmp}:/usr/bin:/bin:/usr/sbin:/sbin"
        env["IPT_LOG"] = str(log)
        env["ALLOW_NTP"] = allow_ntp
        env["ALLOW_GITHUB"] = allow_github
        res = subprocess.run(["sh", str(SCRIPT)], env=env, text=True, capture_output=True, timeout=15)
        self.assertEqual(res.returncode, 0, res.stderr)
        return log.read_text()

    def test_ipv4_and_ipv6_allowlist(self):
        log = self.run_guard()
        for rule in (
            "-o lo -j ACCEPT",
            "-d 10.0.0.0/8 -j ACCEPT",
            "-d 172.16.0.0/12 -j ACCEPT",
            "-d 192.168.0.0/16 -j ACCEPT",
            "-d 169.254.0.0/16 -j ACCEPT",
            "-d 224.0.0.0/4 -j ACCEPT",
            "-d 255.255.255.255 -j ACCEPT",
            "-p udp --sport 68 --dport 67 -j ACCEPT",
            "-p udp --sport 546 --dport 547 -j ACCEPT",
            "-A EGRESS_GUARD -j DROP",
            "-I OUTPUT 1 -j EGRESS_GUARD",
        ):
            self.assertIn(rule, log)
        for rule in (
            "-o lo -j ACCEPT",
            "-d fc00::/7 -j ACCEPT",
            "-d fe80::/10 -j ACCEPT",
            "-d ff00::/8 -j ACCEPT",
            "-p udp --sport 546 --dport 547 -j ACCEPT",
            "-A EGRESS_GUARD6 -j DROP",
            "-I OUTPUT 1 -j EGRESS_GUARD6",
        ):
            self.assertIn(rule, log)

    def test_ntp_toggle(self):
        self.assertIn("-p udp --dport 123 -d 216.239.35.0/24 -j ACCEPT", self.run_guard("1"))
        self.assertNotIn("--dport 123", self.run_guard("0"))

    def test_github_https_only(self):
        log = self.run_guard()
        for net in ("140.82.112.0/20", "143.55.64.0/20", "185.199.108.0/22", "192.30.252.0/22"):
            self.assertIn(f"-I EGRESS_GUARD 1 -d {net} -p tcp --dport 443 -j ACCEPT", log)
        self.assertNotIn("EGRESS_GUARD6 1 -d", log)
        self.assertNotIn("--dport 443", self.run_guard(allow_github="0"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
