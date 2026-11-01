#!/usr/bin/env python3
"""Hermetic tests for the Hermes overlay renderer and its context files."""
from __future__ import annotations

import contextlib
import io
from pathlib import Path
import tempfile
import unittest

import yaml

import render_hermes_config as renderer

HERE = Path(__file__).resolve().parent
FRAGMENT = HERE / "config.fragment.yaml"
CONTEXT = HERE / "HERMES.md"
SOUL = HERE / "SOUL.md"
VOICE_PROMPT = HERE / "voice_prompt.txt"

EXISTING_CONFIG = """\
model:
  provider: "openai-codex"
  default: "gpt-5.6-luna"
gateway:
  write_sessions_json: false
sessions_dir: /opt/data/sessions
platforms:
  telegram:
    token: "123456:placeholder-secret-value"
agent:
  personalities:
    voice: "старый текст"
platform_toolsets:
  api_server: [web]
"""

# What a live home looks like: real endpoints, an extra MCP server the fragment
# does not mention, and the reviewed toolset list.
LIVE_CONFIG = """\
model:
  provider: "openai-codex"
  default: "gpt-5.6-luna"
mcp_servers:
  homeassistant:
    url: "http://10.1.2.3:8123/api/mcp"
    trust: full
  music-assistant:
    url: "http://10.1.2.3:8095/mcp/v1"
    trust: full
  manual-extra:
    url: "http://10.1.2.3:9999/mcp"
"""

PRIVATE_VALUES = """\
mcp_servers:
  homeassistant:
    url: "http://10.1.2.3:8123/api/mcp"
  music-assistant:
    url: "http://10.1.2.3:8095/mcp/v1"
"""


def render_plan(home, private=None, allow_placeholders=True):
    return renderer.build_plan(
        home, FRAGMENT, CONTEXT, SOUL,
        private_values=private,
        voice_prompt=VOICE_PROMPT,
        allow_placeholders=allow_placeholders,
    )


class DeepMergeTests(unittest.TestCase):
    def test_nested_mapping_merges_and_scalar_wins(self):
        merged = renderer.deep_merge(
            {"a": {"x": 1, "y": 2}, "keep": True},
            {"a": {"y": 3, "z": 4}},
        )
        self.assertEqual(merged, {"a": {"x": 1, "y": 3, "z": 4}, "keep": True})

    def test_lists_are_replaced_not_concatenated(self):
        merged = renderer.deep_merge({"list": [1, 2, 3]}, {"list": ["web"]})
        self.assertEqual(merged, {"list": ["web"]})

    def test_inputs_are_not_mutated(self):
        base = {"a": {"b": 1}}
        overlay = {"a": {"c": 2}}
        renderer.deep_merge(base, overlay)
        self.assertEqual(base, {"a": {"b": 1}})
        self.assertEqual(overlay, {"a": {"c": 2}})


class RedactionTests(unittest.TestCase):
    def test_token_shaped_values_are_masked(self):
        text = 'platforms:\n  telegram:\n    token: "123456:placeholder"\n'
        self.assertIn("token: <redacted>", renderer.redact(text))
        self.assertNotIn("placeholder", renderer.redact(text))

    def test_ordinary_lines_survive(self):
        self.assertEqual(renderer.redact("mcp-infra: enabled\n"), "mcp-infra: enabled\n")


class PlaceholderGuardTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.home = Path(self.tmp.name)
        self.config = self.home / "config.yaml"

    def tearDown(self):
        self.tmp.cleanup()

    def test_refuses_to_put_test_net_addresses_over_live_endpoints(self):
        self.config.write_text(LIVE_CONFIG, encoding="utf-8")
        with self.assertRaises(SystemExit) as caught:
            render_plan(self.home, allow_placeholders=False)
        message = str(caught.exception)
        self.assertIn("mcp_servers.homeassistant.url", message)
        self.assertIn("mcp_servers.music-assistant.url", message)
        self.assertIn("--private-values", message)

    def test_refuses_placeholders_in_a_fresh_home(self):
        with self.assertRaises(SystemExit) as caught:
            render_plan(self.home, allow_placeholders=False)
        self.assertIn("mcp_servers.homeassistant.url", str(caught.exception))

    def test_private_values_make_the_render_safe(self):
        self.config.write_text(LIVE_CONFIG, encoding="utf-8")
        private = self.home / "private.yaml"
        private.write_text(PRIVATE_VALUES, encoding="utf-8")
        plan = render_plan(self.home, private=private, allow_placeholders=False)
        merged = yaml.safe_load(plan[self.config])
        self.assertEqual(merged["mcp_servers"]["homeassistant"]["url"],
                         "http://10.1.2.3:8123/api/mcp")
        self.assertEqual(merged["mcp_servers"]["music-assistant"]["url"],
                         "http://10.1.2.3:8095/mcp/v1")
        self.assertNotIn("192.0.2.", plan[self.config])

    def test_private_values_are_merged_after_the_fragment(self):
        self.config.write_text(LIVE_CONFIG, encoding="utf-8")
        private = self.home / "private.yaml"
        private.write_text(PRIVATE_VALUES + 'model:\n  default: "private-model"\n',
                           encoding="utf-8")
        merged = yaml.safe_load(render_plan(self.home, private=private)[self.config])
        self.assertEqual(merged["model"]["default"], "private-model")
        self.assertEqual(merged["model"]["provider"], "openai-codex")

    def test_missing_private_values_file_is_an_error(self):
        with self.assertRaises(SystemExit):
            render_plan(self.home, private=self.home / "nope.yaml")

    def test_escape_hatch_allows_placeholders(self):
        plan = render_plan(self.home, allow_placeholders=True)
        self.assertIn("192.0.2.", plan[self.config])

    def test_placeholder_already_live_is_not_a_violation(self):
        self.config.write_text(
            'mcp_servers:\n  homeassistant:\n    url: "http://192.0.2.28:8123/api/mcp"\n',
            encoding="utf-8",
        )
        violations = renderer.placeholder_violations(
            {"mcp_servers": {"homeassistant": {"url": "http://192.0.2.28:8123/api/mcp"}}},
            {"mcp_servers": {"homeassistant": {"url": "http://192.0.2.28:8123/api/mcp"}}},
        )
        self.assertEqual(violations, [])


class PlanTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.home = Path(self.tmp.name)
        self.config = self.home / "config.yaml"
        self.config.write_text(EXISTING_CONFIG, encoding="utf-8")

    def tearDown(self):
        self.tmp.cleanup()

    def test_merge_keeps_unrelated_keys_and_installs_context(self):
        plan = render_plan(self.home)
        merged = yaml.safe_load(plan[self.config])
        # Unrelated keys survive a fragment that does not mention them.
        self.assertEqual(merged["gateway"], {"write_sessions_json": False})
        self.assertEqual(merged["sessions_dir"], "/opt/data/sessions")
        self.assertEqual(merged["platforms"]["telegram"]["token"],
                         "123456:placeholder-secret-value")
        # Overlay wins for the keys it owns.
        self.assertEqual(merged["platform_toolsets"]["api_server"],
                         ["mcp-homeassistant", "mcp-music-assistant", "web", "mcp-infra"])
        self.assertIn("infra", merged["mcp_servers"])
        # Context files install into the home.
        self.assertEqual(plan[self.home / ".hermes.md"], CONTEXT.read_text(encoding="utf-8"))
        self.assertEqual(plan[self.home / "SOUL.md"], SOUL.read_text(encoding="utf-8"))

    def test_voice_personality_comes_from_voice_prompt(self):
        plan = render_plan(self.home)
        merged = yaml.safe_load(plan[self.config])
        self.assertEqual(merged["agent"]["personalities"]["voice"],
                         VOICE_PROMPT.read_text(encoding="utf-8").strip())
        self.assertEqual(merged["display"]["personality"], "voice")
        self.assertNotIn("старый текст", plan[self.config])

    def test_existing_mcp_servers_are_not_deleted(self):
        self.config.write_text(LIVE_CONFIG, encoding="utf-8")
        private = self.home / "private.yaml"
        private.write_text(PRIVATE_VALUES, encoding="utf-8")
        merged = yaml.safe_load(render_plan(self.home, private=private)[self.config])
        self.assertEqual(merged["mcp_servers"]["manual-extra"]["url"],
                         "http://10.1.2.3:9999/mcp")

    def test_second_render_is_a_no_op(self):
        for path, text in render_plan(self.home).items():
            path.write_text(text, encoding="utf-8")
        self.assertEqual(render_plan(self.home), {})

    def test_missing_config_is_created_from_the_fragment(self):
        self.config.unlink()
        plan = render_plan(self.home, allow_placeholders=True)
        self.assertIn("mcp_servers", yaml.safe_load(plan[self.config]))

    def test_non_mapping_config_is_rejected(self):
        self.config.write_text("- not\n- a mapping\n", encoding="utf-8")
        with self.assertRaises(SystemExit):
            render_plan(self.home)


class CliTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.home = Path(self.tmp.name)
        self.config = self.home / "config.yaml"
        self.config.write_text(EXISTING_CONFIG, encoding="utf-8")

    def tearDown(self):
        self.tmp.cleanup()

    def _main(self, *args):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = renderer.main(["--home", str(self.home), *args])
        return code, out.getvalue()

    def test_dry_run_writes_nothing_and_redacts_secrets(self):
        before = self.config.read_bytes()
        code, out = self._main("--dry-run", "--allow-placeholders")
        self.assertEqual(code, 0)
        self.assertEqual(self.config.read_bytes(), before)
        self.assertFalse((self.home / ".hermes.md").exists())
        self.assertIn("+", out)
        self.assertNotIn("placeholder-secret-value", out)
        self.assertIn("<redacted>", out)

    def test_check_reports_pending_render_without_writing(self):
        code, _ = self._main("--check", "--allow-placeholders")
        self.assertEqual(code, 1)
        self.assertFalse((self.home / "SOUL.md").exists())

    def test_render_keeps_one_step_backup_and_then_reports_clean(self):
        code, out = self._main("--allow-placeholders")
        self.assertEqual(code, 0)
        self.assertIn("rendered", out)
        previous = self.config.with_name("config.yaml.prev").read_text(encoding="utf-8")
        self.assertEqual(previous, EXISTING_CONFIG)
        self.assertTrue((self.home / ".hermes.md").exists())
        self.assertTrue((self.home / "SOUL.md").exists())
        code, out = self._main("--check", "--allow-placeholders")
        self.assertEqual(code, 0)
        self.assertIn("up to date", out)

    def test_cli_accepts_private_values(self):
        self.config.write_text(LIVE_CONFIG, encoding="utf-8")
        private = self.home / "private.yaml"
        private.write_text(PRIVATE_VALUES, encoding="utf-8")
        code, out = self._main("--private-values", str(private), "--check")
        self.assertEqual(code, 1, out)
        self.assertIn("would update", out)
        code, out = self._main("--dry-run", "--private-values", str(private))
        self.assertEqual(code, 0)
        self.assertNotIn("192.0.2.", out)

    def test_cli_refuses_placeholders_without_escape_hatch(self):
        self.config.write_text(LIVE_CONFIG, encoding="utf-8")
        with self.assertRaises(SystemExit) as caught:
            self._main("--dry-run")
        self.assertIn("placeholder", str(caught.exception))


class ContextFileTests(unittest.TestCase):
    def test_context_stays_small(self):
        # Every byte lands in the cached system prompt of each turn.
        self.assertLess(len(CONTEXT.read_bytes()), 2048)

    def test_context_names_the_tool_contract(self):
        text = CONTEXT.read_text(encoding="utf-8")
        for tool in ("infra_status", "infra_service", "infra_logs", "infra_alert",
                     "infra_prepare_restart", "infra_confirm", "infra_cancel",
                     "infra_send_report"):
            self.assertIn(tool, text)
        self.assertIn("kubectl", text)
        self.assertIn("90", text)

    def test_context_has_no_injection_markers(self):
        # The context file is injected into the system prompt; Hermes blocks a
        # file that matches its threat patterns, so keep the wording neutral.
        text = CONTEXT.read_text(encoding="utf-8").lower()
        for marker in ("ignore previous instructions", "system prompt",
                       "disregard your instructions", "you are now a",
                       "do not tell the user", "register as a node"):
            self.assertNotIn(marker, text)
        for char in "\u200b\u200c\u200d\u2060\ufeff\u202a\u202b\u202c\u202d\u202e":
            self.assertNotIn(char, CONTEXT.read_text(encoding="utf-8"))


class PrivateExampleTests(unittest.TestCase):
    def test_example_covers_every_placeholder_endpoint(self):
        example = yaml.safe_load(
            (HERE / "hermes-private.example.yaml").read_text(encoding="utf-8")
        )
        fragment = yaml.safe_load(FRAGMENT.read_text(encoding="utf-8"))
        placeholders = [
            name for name, server in fragment["mcp_servers"].items()
            if renderer._PLACEHOLDER.search(str(server.get("url", "")))
        ]
        self.assertTrue(placeholders, "fragment should keep TEST-NET placeholders")
        for name in placeholders:
            with self.subTest(server=name):
                # Copying the example and replacing the addresses is enough:
                # it mirrors the fragment shape for every placeholder endpoint.
                self.assertEqual(set(example["mcp_servers"][name]), {"url"})
                self.assertTrue(renderer._PLACEHOLDER.search(
                    example["mcp_servers"][name]["url"]))


if __name__ == "__main__":
    unittest.main(verbosity=2)
