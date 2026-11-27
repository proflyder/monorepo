#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[4]
FRAGMENT = ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "config.fragment.yaml"


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

    def test_mcp_exposes_every_ha_tool_except_time_writers(self):
        tools = self.data["mcp_servers"]["homeassistant"]["tools"]
        assert "include" not in tools, "tools.include would narrow the HA MCP surface"
        excluded = set(tools["exclude"])
        for pattern in (
            "StationAlarm*",
            "StationTimer*",
            "StationReminder*",
            "StationTimeRoute*",
            "script.schedule_alarm_*",
            "script.schedule_timer_*",
            "script.schedule_reminder_*",
            "script.alarm_*",
        ):
            assert pattern in excluded, pattern
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

    def test_telegram_strangers_are_silently_ignored(self):
        assert self.data["unauthorized_dm_behavior"] == "ignore"
        assert self.data["allow_all_users"] is False
        extra = self.data["platforms"]["telegram"]["extra"]
        assert extra["unauthorized_dm_behavior"] == "ignore"
        assert extra["group_allowed_chats"] == []
        assert extra["group_allow_from"] == []
        text = FRAGMENT.read_text()
        assert "GATEWAY_ALLOW_ALL_USERS" not in text

    def test_telegram_report_command_is_visible_and_voice_safe(self):
        telegram = self.data["platforms"]["telegram"]
        menu = telegram["extra"]["command_menu"]
        self.assertEqual(menu["max_commands"], 60)
        self.assertEqual(menu["priority_mode"], "prepend")
        self.assertEqual(menu["priority"], ["report"])
        self.assertEqual(self.data["skills"]["platform_disabled"]["api_server"], ["report"])

    def test_telegram_prompt_covers_the_report_command(self):
        prompt = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "telegram_prompt.txt").read_text()
        for marker in ("Рикардо", "Telegram", "infra_status", "/report", "infra_report", "details.text"):
            assert marker in prompt
        assert "180" not in prompt
        assert len(prompt) > 1000

    def test_telegram_uses_polling_and_owner_allowlist(self):
        text = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "README.md").read_text()
        assert "TELEGRAM_BOT_TOKEN" in text
        assert "TELEGRAM_WEBHOOK_URL" in text
        assert "webhook" not in yaml.safe_dump(self.data["platforms"]).lower()

    def test_web_search_uses_chatgpt_codex_native_backend(self):
        assert self.data["web"]["search_backend"] == "openai-native"

    def test_voice_prompt_keeps_answers_tts_safe(self):
        prompt = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "voice_prompt.txt").read_text()
        for marker in ("Рикардо", "Никогда не называй", "URL", "web_search", "music_search", "Будильники", "искажённым", "Похоже", "180", "Комп включается 1–2 минуты"):
            assert marker in prompt

    def test_voice_prompt_forbids_guessed_alarm_durations(self):
        prompt = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "voice_prompt.txt").read_text()
        soul = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "SOUL.md").read_text()
        for marker in ("time_now", "alarm_set", "timer_set", "reminder_set", "одну минуту", "label", "вопрос"):
            assert marker in prompt, marker
        assert "один короткий вопрос" in prompt
        assert "time_now" in soul
        assert "ничего не создавай наугад" in soul

    def test_voice_layers_require_infra_confirmation(self):
        prompt = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "voice_prompt.txt").read_text()
        for marker in ("infra_status", "infra_report", "infra_prepare_restart",
                       "infra_confirm", "infra_cancel"):
            assert marker in prompt
        assert "подтвержд" in prompt.lower()

    def test_voice_prompt_ends_confirmation_with_a_question(self):
        prompt = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "voice_prompt.txt").read_text()
        assert "последнее предложение ответа — сам вопрос" in prompt
        assert "Подтверждаете?" in prompt

    def test_voice_prompt_covers_bb_agent_spawn(self):
        prompt = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "voice_prompt.txt").read_text()
        for marker in ("bb_status", "bb_prepare_spawn", "bb_confirm", "bb_cancel",
                       "подтверди", "режима агента", "рабочий профиль"):
            assert marker in prompt, marker
        assert "этом же разговоре" in prompt
        assert "Не выдумывай проект" in prompt
        assert "дословно, как услышал" in prompt
        assert "без переспроса" in prompt
        assert "проект не найден" in prompt

    def test_phone_tools_reach_voice_and_telegram(self):
        for platform in ("api_server", "telegram"):
            assert "mcp-infra" in self.data["platform_toolsets"][platform]
        tools = self.data["mcp_servers"]["infra"]["tools"]
        assert "include" not in tools, "tools.include would hide the phone group"
        for name in ("voice_prompt.txt", "telegram_prompt.txt", "SOUL.md"):
            text = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / name).read_text()
            for marker in ("phone_find", "phone_notify", "phone_list", "дословно", "список имён"):
                assert marker in text, f"{name}: {marker}"
        telegram = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "telegram_prompt.txt").read_text()
        assert "один короткий вопрос" in telegram
        assert "name" in telegram

    def test_telegram_prompt_covers_bb_agent_spawn(self):
        prompt = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "telegram_prompt.txt").read_text()
        for marker in ("bb_status", "bb_prepare_spawn", "bb_confirm", "bb_cancel",
                       "тред", "режима агента", "рабочий профиль"):
            assert marker in prompt, marker
        assert "двухшаговый" in prompt
        assert "без подтверждения" in prompt
        assert "дословно, как услышал" in prompt
        assert "без переспроса" in prompt
        assert "проект не найден" in prompt

    def test_voice_personality_has_a_single_source(self):
        assert "agent" not in self.data or "personalities" not in self.data["agent"]
        prompt = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "voice_prompt.txt").read_text()
        assert "Рикардо" in prompt
        src = ROOT / "platform" / "deploy-rs" / "src"
        assert "voice_prompt.txt" in (src / "cli.rs").read_text()
        renderer = (src / "hermesconfig.rs").read_text()
        assert 'personalities.set("voice"' in renderer

    def test_persona_is_ricardo_not_hermes(self):
        prompt = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "voice_prompt.txt").read_text()
        soul = (ROOT / "projects" / "home-assistant" / "hermes" / "personal" / "SOUL.md").read_text()
        for text in (prompt, soul):
            assert "Рикардо" in text
            assert "Гермес" in text or "Hermes" in text
        assert self.data["display"]["personality"] == "voice"

    def test_no_literal_secrets(self):
        text = FRAGMENT.read_text()
        for marker in ("Bearer ey", "sk-", "ghp_", "refresh_token=", "password="):
            assert marker not in text


if __name__ == "__main__":
    unittest.main(verbosity=2)
