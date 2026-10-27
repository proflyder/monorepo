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
        ]

    def test_web_search_uses_chatgpt_codex_native_backend(self):
        assert self.data["web"]["search_backend"] == "openai-native"

    def test_voice_prompt_keeps_answers_tts_safe(self):
        prompt = (ROOT / "projects" / "home-assistant" / "hermes" / "voice_prompt.txt").read_text()
        for marker in ("Рикардо", "Никогда не называй", "URL", "web_search", "music_search", "Будильники", "искажённым", "Похоже", "180"):
            assert marker in prompt

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
