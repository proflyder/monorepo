#!/usr/bin/env python3
"""Hermetic contract tests for the Hermes voice-fallback profile."""
from __future__ import annotations

from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[3]
FRAGMENT = ROOT / "projects" / "home-assistant" / "hermes" / "config.fragment.yaml"


class HermesConfigTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.data = yaml.safe_load(FRAGMENT.read_text())

    def test_model_uses_chatgpt_codex_provider(self):
        model = self.data["model"]
        assert model["provider"] == "openai-codex"
        assert model["default"] == "gpt-5.6-luna"

    def test_mcp_is_local_and_uses_env_token(self):
        server = self.data["mcp_servers"]["homeassistant"]
        assert server["url"] == "http://192.0.2.28:8123/api/mcp"
        assert server["headers"]["Authorization"] == "Bearer ${HERMES_HA_TOKEN}"
        assert server["trust"] == "full"
        assert server["tools"]["resources"] is True
        assert server["tools"]["prompts"] is True

    def test_mcp_exposes_all_ha_tools(self):
        tools = self.data["mcp_servers"]["homeassistant"]["tools"]
        assert "include" not in tools, "tools.include would narrow the HA MCP surface"
        assert "exclude" not in tools
        assert tools["resources"] is True
        assert tools["prompts"] is True

    def test_music_assistant_mcp_is_configured(self):
        server = self.data["mcp_servers"]["music-assistant"]
        assert server["url"] == "http://192.0.2.28:8095/mcp/v1"
        assert server["headers"]["Authorization"] == "Bearer ${HERMES_MA_TOKEN}"
        assert server["trust"] == "full"
        assert "include" not in server["tools"]
        assert "exclude" not in server["tools"]
        assert server["tools"]["resources"] is True
        assert server["tools"]["prompts"] is True

    def test_api_server_toolset_is_the_full_mcp_bundle(self):
        assert self.data["platform_toolsets"]["api_server"] == [
            "mcp-homeassistant",
            "mcp-music-assistant",
            "web",
            "mcp-infra",
        ]

    def test_infra_mcp_points_at_the_cluster_gateway_with_env_token(self):
        server = self.data["mcp_servers"]["infra"]
        assert server["url"] == (
            "http://mcp-gateway.home-assistant.svc.cluster.local:8080/mcp"
        )
        assert server["headers"]["Authorization"] == "Bearer ${HERMES_INFRA_TOKEN}"
        # The voice path cannot answer approval prompts, so the restart
        # confirmation is a gateway tool pair instead of a Hermes approval.
        assert server["trust"] == "full"
        assert server["connect_timeout"] == 10
        assert server["timeout"] == 30
        assert server["tools"]["resources"] is True
        assert server["tools"]["prompts"] is True

    def test_telegram_channel_shares_the_curated_surface(self):
        toolsets = self.data["platform_toolsets"]
        assert toolsets["telegram"] == [
            "mcp-homeassistant",
            "mcp-music-assistant",
            "web",
            "mcp-infra",
        ]
        assert self.data["platforms"]["telegram"]["enabled"] is True

    def test_no_platform_gets_a_shell_like_toolset(self):
        forbidden = {
            "terminal", "shell", "file", "files", "browser", "computer",
            "computer_use", "computer-use", "code_execution", "skills",
            "hermes-telegram", "hermes-agent", "webhook",
        }
        for platform, enabled in self.data["platform_toolsets"].items():
            with self.subTest(platform=platform):
                assert not (set(enabled) & forbidden), enabled

    def test_telegram_uses_polling_and_owner_allowlist(self):
        # Telegram is enabled by TELEGRAM_BOT_TOKEN from hermes-environment and
        # limited by TELEGRAM_ALLOWED_USERS; a webhook would move it off polling
        # and needs a public ingress, which this bot must not have.
        text = FRAGMENT.read_text()
        assert "TELEGRAM_BOT_TOKEN" in text
        assert "TELEGRAM_WEBHOOK_URL" in text
        assert "webhook" not in yaml.safe_dump(self.data["platforms"]).lower()

    def test_web_search_uses_chatgpt_codex_native_backend(self):
        assert self.data["web"]["search_backend"] == "openai-native"

    def test_voice_prompt_keeps_answers_tts_safe(self):
        prompt = (ROOT / "projects" / "home-assistant" / "hermes" / "voice_prompt.txt").read_text()
        for marker in ("Рикардо", "Никогда не называй", "URL", "web_search", "music_search", "Будильники", "искажённым", "Похоже", "180"):
            assert marker in prompt

    def test_voice_layers_require_infra_confirmation(self):
        prompt = (ROOT / "projects" / "home-assistant" / "hermes" / "voice_prompt.txt").read_text()
        voice = self.data["agent"]["personalities"]["voice"]
        for text in (prompt, voice):
            with self.subTest(target=text[:20]):
                assert "infra_status" in text
                assert "infra_prepare_restart" in text
                assert "infra_confirm" in text
                assert "infra_cancel" in text
                assert "подтвержд" in text.lower()

    def test_persona_is_ricardo_not_hermes(self):
        prompt = (ROOT / "projects" / "home-assistant" / "hermes" / "voice_prompt.txt").read_text()
        soul = (ROOT / "projects" / "home-assistant" / "hermes" / "SOUL.md").read_text()
        voice = self.data["agent"]["personalities"]["voice"]
        for text in (prompt, soul, voice):
            assert "Рикардо" in text
            assert "Гермес" in text or "Hermes" in text
        assert self.data["display"]["personality"] == "voice"

    def test_no_literal_secrets(self):
        text = FRAGMENT.read_text()
        for marker in ("Bearer ey", "sk-", "ghp_", "refresh_token=", "password="):
            assert marker not in text


if __name__ == "__main__":
    unittest.main(verbosity=2)
