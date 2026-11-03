#!/usr/bin/env python3
"""Contract for the private voice-routing values of the Hermes Deployment.

The routing env lives in the private `runtime_values` (OpenBao), not in the
tracked chart. The deploy README is therefore the reviewed contract: this test
keeps its snippet parseable, placeholder-only, wired to existing Secrets, and
in sync with the variables the proxy actually reads.
"""
from __future__ import annotations

import re
import unittest
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
README = HERE / "README.md"
HERMES_README = HERE.parent / "hermes" / "README.md"
PROXY = HERE.parent / "hermes" / "voice_proxy.py"

REQUIRED_ENV = (
    "HERMES_HA_URL",
    "HERMES_HA_TOKEN",
    "HERMES_VOICE_GATEWAY_URL",
    "HERMES_VOICE_GATEWAY_TOKEN",
    "HERMES_SCHEDULER_URL",
    "HERMES_SCHEDULER_TOKEN",
    "HERMES_VOICE_QUEUE",
    "HERMES_VOICE_MEDIA_PLAYER",
)
EXPECTED_SECRETS = {
    "HERMES_HA_TOKEN": ("hermes-environment", "HERMES_HA_TOKEN"),
    "HERMES_VOICE_GATEWAY_TOKEN": ("mcp-gateway-voice", "token"),
    "HERMES_SCHEDULER_TOKEN": ("scheduler-auth", "token"),
}


def values_snippet() -> str:
    text = README.read_text(encoding="utf-8")
    match = re.search(r"## Voice routing.*?```yaml\n(.*?)```", text, re.S)
    if not match:
        raise AssertionError("deploy README has no voice-routing values snippet")
    return match.group(1)


def proxy_container() -> dict:
    values = yaml.safe_load(values_snippet())
    containers = values["workloads"]["hermes"]["containers"]
    return next(item for item in containers if item.get("name") == "voice-proxy")


class VoiceRoutingValuesContract(unittest.TestCase):
    def test_snippet_has_every_required_variable(self):
        names = {item["name"] for item in proxy_container()["env"]}
        missing = set(REQUIRED_ENV) - names
        self.assertEqual(missing, set(), f"deploy README misses {sorted(missing)}")

    def test_tokens_come_from_existing_secrets(self):
        env = {item["name"]: item for item in proxy_container()["env"]}
        for name, (secret, key) in EXPECTED_SECRETS.items():
            ref = env[name].get("valueFrom", {}).get("secretKeyRef", {})
            self.assertEqual((ref.get("name"), ref.get("key")), (secret, key), name)
            self.assertNotIn("value", env[name], "a token must never be inline")

    def test_snippet_uses_only_documentation_addresses(self):
        text = values_snippet()
        addresses = re.findall(r"\b\d{1,3}(?:\.\d{1,3}){3}\b", text)
        self.assertTrue(addresses, "the HA address placeholder is missing")
        for address in addresses:
            self.assertTrue(address.startswith("192.0.2."), address)
        self.assertNotRegex(text, r"\b[0-9a-fA-F]{32,}\b")

    def test_proxy_reads_the_documented_variables(self):
        source = PROXY.read_text(encoding="utf-8")
        for name in REQUIRED_ENV:
            self.assertIn(f'"{name}"', source)
        herm = HERMES_README.read_text(encoding="utf-8")
        for name in REQUIRED_ENV:
            self.assertIn(name, herm)

    def test_routing_gate_is_documented_and_off_by_default(self):
        source = PROXY.read_text(encoding="utf-8")
        self.assertIn("def routing_enabled()", source)
        # Empty defaults are what makes the rollout safe before the values land.
        for name in ("HERMES_VOICE_GATEWAY_URL", "HERMES_HA_URL"):
            self.assertIn(f'os.environ.get("{name}", "")', source)
        herm = HERMES_README.read_text(encoding="utf-8")
        self.assertIn("disabled unless", herm)


if __name__ == "__main__":
    unittest.main(verbosity=2)
