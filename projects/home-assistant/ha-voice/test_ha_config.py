#!/usr/bin/env python3
"""Static, fast tests for the Home Assistant voice package.

These tests do not need a running HA instance.  They catch the failures that
are expensive to discover through the satellite: broken YAML, unknown
entities/scripts, malformed Jinja, intent/sentence drift and regressions in the
alarm/music state machine.
"""
from __future__ import annotations

import json
import re
import unittest
from collections import Counter
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any
from zoneinfo import ZoneInfo

import yaml

ROOT = Path(__file__).resolve().parents[3]
HA = ROOT / "projects" / "home-assistant" / "ha-voice"


class UniqueKeyLoader(yaml.SafeLoader):
    """SafeLoader that rejects duplicate mapping keys (PyYAML silently wins)."""

    def construct_mapping(self, node, deep=False):  # type: ignore[no-untyped-def]
        mapping: dict[Any, Any] = {}
        for key_node, value_node in node.value:
            key = self.construct_object(key_node, deep=deep)
            if key in mapping:
                raise yaml.constructor.ConstructorError(
                    "while constructing a mapping",
                    node.start_mark,
                    f"found duplicate key {key!r}",
                    key_node.start_mark,
                )
            mapping[key] = self.construct_object(value_node, deep=deep)
        return mapping


def _secret(loader, node):  # type: ignore[no-untyped-def]
    return "__SECRET__"


def _include(loader, node):  # type: ignore[no-untyped-def]
    return [f"__INCLUDE__{node.value}"]


for _tag in (
    "!secret",
    "!include",
    "!include_dir_list",
    "!include_dir_named",
    "!include_dir_merge_list",
    "!include_dir_merge_named",
):
    if _tag == "!secret":
        UniqueKeyLoader.add_constructor(_tag, _secret)
    else:
        UniqueKeyLoader.add_constructor(_tag, _include)


def load_yaml(path: Path) -> Any:
    with path.open(encoding="utf-8") as fh:
        return yaml.load(fh, Loader=UniqueKeyLoader)


SCRIPTS = load_yaml(HA / "scripts.yaml")
INTENTS = load_yaml(HA / "intent_scripts.yaml")
AUTOMATIONS = load_yaml(HA / "automations.yaml")
REST = load_yaml(HA / "rest_commands.yaml")


def yaml_files() -> list[Path]:
    files = list(HA.rglob("*.yaml"))
    files += [
        ROOT / "projects" / "home-assistant" / "configs" / "00-installer-config.yaml",
        ROOT / "projects" / "home-assistant" / "stack" / "docker-compose.yml",
    ]
    return sorted(p for p in files if p.exists())


def walk(value: Any):
    """Yield every nested scalar/container in a YAML tree."""
    yield value
    if isinstance(value, dict):
        for k, v in value.items():
            yield from walk(k)
            yield from walk(v)
    elif isinstance(value, list):
        for v in value:
            yield from walk(v)


def strings(value: Any) -> list[str]:
    return [v for v in walk(value) if isinstance(v, str)]


def services(value: Any) -> list[str]:
    out: list[str] = []
    if isinstance(value, dict):
        svc = value.get("service")
        if isinstance(svc, str):
            out.append(svc)
        for v in value.values():
            out.extend(services(v))
    elif isinstance(value, list):
        for v in value:
            out.extend(services(v))
    return out


def service_targets(value: Any) -> set[str]:
    out: set[str] = set()
    if isinstance(value, dict):
        target = value.get("target")
        if isinstance(target, dict):
            entity_id = target.get("entity_id")
            if isinstance(entity_id, str):
                out.add(entity_id)
            elif isinstance(entity_id, list):
                out.update(x for x in entity_id if isinstance(x, str))
        for v in value.values():
            out |= service_targets(v)
    elif isinstance(value, list):
        for v in value:
            out |= service_targets(v)
    return out


def defined_entities() -> dict[str, set[str]]:
    result: dict[str, set[str]] = {}
    for prefix in ("input_boolean", "input_datetime", "input_number", "input_select", "input_text"):
        path = HA / f"{prefix}.yaml"
        result[prefix] = set(load_yaml(path).keys())
    result["script"] = set(SCRIPTS.keys())
    result["rest_command"] = set(REST.keys())
    result["automation"] = {a["id"] for a in AUTOMATIONS if "id" in a}
    return result


ENTITY_RE = re.compile(
    r"\b(input_boolean|input_datetime|input_number|input_select|input_text|script|automation|"
    r"rest_command|media_player|event|tts)\.([A-Za-z0-9_]+)"
)
EXTERNAL_ENTITIES = {
    "media_player.iandeks_stantsiia_midi",
    "media_player.iandeks_stantsiia_midi_media_player",
    "event.iandeks_stantsiia_midi_button",
    "select.iandeks_stantsiia_midi_voice_color",
    "tts.piper",
}
SCRIPT_SERVICES = {"turn_on", "turn_off", "reload", "toggle"}
INPUT_SERVICES = {"turn_on", "turn_off", "toggle", "set_value", "select_option", "set_datetime", "reload"}


class YamlTests(unittest.TestCase):
    def test_all_first_party_yaml_parses_without_duplicate_keys(self):
        files = yaml_files()
        self.assertGreaterEqual(len(files), 10)
        for path in files:
            with self.subTest(path=str(path)):
                load_yaml(path)

    def test_ha_yaml_has_no_unexpected_empty_files(self):
        for path in HA.rglob("*.yaml"):
            with self.subTest(path=str(path)):
                self.assertIsNotNone(load_yaml(path))


class MediaUrlTests(unittest.TestCase):
    def test_alarm_media_is_fetchable_from_home_assistant(self):
        # Live incident: the chime/ringtone URL used a documentation TEST-NET
        # host, so the ESPHome ffmpeg proxy timed out and the alarm was silent.
        # HA must fetch its own local media from loopback.
        text = "\n".join(strings(SCRIPTS))
        self.assertNotIn("192.0.2.", text)
        self.assertIn("http://127.0.0.1:8123/local/alarm_chime.wav", text)
        self.assertIn("http://127.0.0.1:8123/local/alarm_ringtone.wav", text)


class EntityAndServiceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.defined = defined_entities()
        cls.all_text = "\n".join(
            s for path in yaml_files() for s in strings(load_yaml(path))
        )
        cls.all_data = [load_yaml(path) for path in yaml_files()]

    def test_all_known_input_entities_exist(self):
        for match in ENTITY_RE.finditer(self.all_text):
            domain, name = match.groups()
            if domain not in self.defined:
                continue
            if f"{domain}.{name}" in EXTERNAL_ENTITIES:
                continue
            if domain.startswith("input_") and name in INPUT_SERVICES:
                continue
            if "{{" in name or name.endswith("_"):
                continue
            if domain == "script" and name in SCRIPT_SERVICES:
                continue
            self.assertIn(name, self.defined[domain], f"unknown entity {domain}.{name}")

    def test_all_script_service_calls_exist(self):
        for data in self.all_data:
            for svc in services(data):
                if not svc.startswith("script."):
                    continue
                name = svc.split(".", 1)[1]
                if name in SCRIPT_SERVICES:
                    continue
                self.assertIn(name, self.defined["script"], f"unknown script service {svc}")

    def test_all_rest_command_calls_exist(self):
        for data in self.all_data:
            for svc in services(data):
                if svc.startswith("rest_command."):
                    name = svc.split(".", 1)[1]
                    self.assertIn(name, self.defined["rest_command"], f"unknown rest command {svc}")

    def test_all_target_entity_ids_exist(self):
        for data in self.all_data:
            for entity_id in service_targets(data):
                domain, _, name = entity_id.partition(".")
                if entity_id in EXTERNAL_ENTITIES:
                    continue
                if "{{" in entity_id or name.endswith("_"):
                    continue
                if domain in self.defined:
                    self.assertIn(name, self.defined[domain], f"unknown target {entity_id}")

    def test_script_and_automation_shapes(self):
        valid_modes = {"single", "restart", "queued", "parallel"}
        for name, script in SCRIPTS.items():
            with self.subTest(script=name):
                self.assertIsInstance(script, dict)
                self.assertTrue(script.get("alias"))
                self.assertIn(script.get("mode", "single"), valid_modes)
                self.assertIn("sequence", script)
                self.assertIsInstance(script["sequence"], list)
                self.assertTrue(script["sequence"])
        ids = [a.get("id") for a in AUTOMATIONS]
        self.assertEqual(len(ids), len(set(ids)))
        for automation in AUTOMATIONS:
            self.assertIn("trigger", automation)
            self.assertIn("action", automation)


class ScenarioScriptTests(unittest.TestCase):
    def test_false_wake_scenario_presses_the_station_button(self):
        script = SCRIPTS["wake_false_alarm"]
        self.assertEqual(script["alias"], "Рикардо: ложное срабатывание")
        self.assertEqual(script["mode"], "single")
        self.assertEqual(services(script), ["button.press"])
        self.assertEqual(
            service_targets(script),
            {"button.iandeks_stantsiia_midi_wake_lozhnoe_srabatyvanie"},
        )
        self.assertEqual(len(script["sequence"]), 1, "false-wake scenario must only press the button")

    def test_missed_wake_scenario_presses_the_station_button(self):
        script = SCRIPTS["wake_missed"]
        self.assertEqual(script["alias"], "Рикардо: не услышала")
        self.assertEqual(script["mode"], "single")
        self.assertEqual(services(script), ["button.press"])
        self.assertEqual(
            service_targets(script),
            {"button.iandeks_stantsiia_midi_wake_ne_uslyshala"},
        )
        self.assertEqual(len(script["sequence"]), 1, "missed-wake scenario must only press the button")


class TemplateTests(unittest.TestCase):
    def test_every_template_parses(self):
        try:
            from jinja2 import Environment, TemplateSyntaxError
        except ImportError:  # pragma: no cover - optional local dependency
            self.skipTest("jinja2 is not installed")
        env = Environment()
        count = 0
        for path in yaml_files():
            for value in strings(load_yaml(path)):
                if "{{" not in value and "{%" not in value:
                    continue
                count += 1
                with self.subTest(path=str(path), template=value[:80]):
                    try:
                        env.parse(value)
                    except TemplateSyntaxError as exc:
                        self.fail(f"{path}:{exc.lineno}: {exc.message}: {value!r}")
        self.assertGreater(count, 50, "expected a meaningful number of templates")


class IntentTests(unittest.TestCase):
    def setUp(self):
        self.sentence_files = list((HA / "custom_sentences").rglob("*.yaml"))
        self.sentence_data = [yaml.safe_load(p.read_text()) for p in self.sentence_files]

    def test_every_sentence_intent_has_an_intent_script(self):
        declared: set[str] = set()
        for data in self.sentence_data:
            declared.update(data.get("intents", {}).keys())
        self.assertEqual(declared, set(INTENTS.keys()))

    def test_every_intent_has_sentences_and_every_slot_is_defined(self):
        for path, data in zip(self.sentence_files, self.sentence_data):
            lists = data.get("lists", {})
            for intent, body in data.get("intents", {}).items():
                phrases = []
                for item in body.get("data", []):
                    phrases.extend(item.get("sentences", []))
                self.assertTrue(phrases, f"{intent} has no sentences in {path}")
                for phrase in phrases:
                    for raw in re.findall(r"\{([^{}]+)\}", phrase):
                        list_name = raw.split(":", 1)[0]
                        # Free-text wildcards (for example a music query) are
                        # intentionally not backed by a closed list.
                        if list_name in {"query", "timer_name", "text"}:
                            continue
                        self.assertIn(list_name, lists, f"{intent}: undefined list {list_name} in {phrase!r}")

    def test_no_duplicate_phrases_across_intents(self):
        seen: dict[str, str] = {}
        for data in self.sentence_data:
            for intent, body in data.get("intents", {}).items():
                for item in body.get("data", []):
                    for phrase in item.get("sentences", []):
                        normalized = " ".join(phrase.lower().split())
                        self.assertNotIn(normalized, seen, f"{phrase!r}: {seen.get(normalized)} and {intent}")
                        seen[normalized] = intent

    def test_work_mode_intent_runs_the_native_scenario(self):
        intent = INTENTS["StationWorkMode"]
        self.assertTrue(
            any(
                step.get("service") == "script.scenario_run"
                and step.get("data", {}).get("label") == "Сел работать"
                for step in intent["action"]
            ),
            "StationWorkMode must queue the native «Сел работать» scenario",
        )
        # The native definitions live in the scheduler, not in HA YAML.
        self.assertNotIn("work_mode", SCRIPTS)
        definitions = load_yaml(
            ROOT / "projects" / "home" / "kubernetes" / "scheduler" / "scenarios" / "work_mode.json"
        )
        self.assertEqual(definitions["label"], "Сел работать")
        applied = {
            step["catalog"]["entryId"]: json.loads(step["catalog"]["paramsJson"])
            for step in definitions["steps"]
        }
        self.assertEqual(
            applied,
            {
                "light.spalnia_lenta": {"brightness": 100, "kelvin": 2816},
                "light.yeelink_cn_827036414_bslamp2_s_2_light": {
                    "brightness": 100,
                    "kelvin": 2800,
                    "effect": "Day",
                },
                "light.yeelink_cn_919937191_lamp22_s_2": {
                    "brightness": 100,
                    "kelvin": 2800,
                },
            },
        )
        phrases = {
            phrase
            for item in yaml.safe_load(
                (HA / "custom_sentences" / "ru" / "station_control.yaml").read_text()
            )["intents"]["StationWorkMode"]["data"]
            for phrase in item["sentences"]
        }
        self.assertIn("я сел работать", phrases)
        self.assertIn("включи рабочий режим", phrases)

    def test_blackout_intent_runs_the_native_scenario(self):
        intent = INTENTS["StationBlackout"]
        self.assertTrue(
            any(
                step.get("service") == "script.scenario_run"
                and step.get("data", {}).get("label") == "Блэкаут"
                for step in intent["action"]
            ),
            "StationBlackout must queue the native «Блэкаут» scenario",
        )
        self.assertNotIn("blackout", SCRIPTS)
        definitions = load_yaml(
            ROOT / "projects" / "home" / "kubernetes" / "scheduler" / "scenarios" / "blackout.json"
        )
        self.assertEqual(definitions["label"], "Блэкаут")
        self.assertEqual(
            [
                (step["catalog"]["entryId"], step["catalog"]["action"])
                for step in definitions["steps"]
            ],
            [
                ("light.spalnia_lenta", "turn_off"),
                ("light.yeelink_cn_827036414_bslamp2_s_2_light", "turn_off"),
                ("light.yeelink_cn_919937191_lamp22_s_2", "turn_off"),
            ],
        )
        phrases = {
            phrase
            for item in yaml.safe_load(
                (HA / "custom_sentences" / "ru" / "station_control.yaml").read_text()
            )["intents"]["StationBlackout"]["data"]
            for phrase in item["sentences"]
        }
        for phrase in (
            "black out",
            "blackout",
            "блэкаут",
            "блэк аут",
            "блек аут",
            "блок аут",
            "блак аут",
        ):
            self.assertIn(phrase, phrases)

    def test_scenario_run_resolves_label_and_queues_a_manual_run(self):
        script = SCRIPTS["scenario_run"]
        dump = yaml.safe_dump(script, allow_unicode=True)
        self.assertIn("label", script["fields"])
        self.assertIn("script.scenario_run", str(services(INTENTS["StationWorkMode"])))
        self.assertIn("rest_command.scenario_rpc", services(script))
        self.assertIn("ListScenarios", dump)
        self.assertIn("RunScenario", dump)
        # Definitions live in the scheduler: HA must not edit or create them.
        for forbidden in ("CreateScenario", "UpdateScenario", "DeleteScenario"):
            self.assertNotIn(forbidden, dump)
        # An unknown label is a warning, never a guess or a silent HA action.
        self.assertIn("system_log.write", services(script))
        self.assertIn("idempotencyKey", dump)
        self.assertIn("context.id", dump)

    def test_light_commands_are_local_and_target_known_lights(self):
        control = yaml.safe_load(
            (HA / "custom_sentences" / "ru" / "station_control.yaml").read_text()
        )
        targets = {v["out"] for v in control["lists"]["light_target"]["values"]}
        self.assertEqual(
            targets,
            {
                "light.yeelink_cn_827036414_bslamp2_s_2_light",
                "light.yeelink_cn_919937191_lamp22_s_2",
                "light.spalnia_lenta",
            },
        )
        for intent, service in (
            ("StationLightOn", "light.turn_on"),
            ("StationLightOff", "light.turn_off"),
        ):
            self.assertIn(service, services(INTENTS[intent]))
        phrases = {
            phrase
            for item in control["intents"]["StationLightOff"]["data"]
            for phrase in item["sentences"]
        }
        self.assertIn("выключи {light_target}", phrases)
        self.assertIn("погаси {light_target}", phrases)

    def test_music_resume_accepts_bare_continue(self):
        self.assertIn("media_player.media_play", services(INTENTS["StationMusicResume"]))
        phrases: set[str] = set()
        for data in self.sentence_data:
            for item in data.get("intents", {}).get("StationMusicResume", {}).get("data", []):
                phrases.update(item.get("sentences", []))
        for phrase in ("продолжи", "продолжить", "продолжи музыку", "возобнови"):
            self.assertIn(phrase, phrases)

    def test_cancel_has_bare_phrases_and_no_side_effects(self):
        cancel = INTENTS["StationCancel"]
        self.assertEqual(services(cancel), [])
        # Recognized "отмена" ends locally: no spoken answer and no Hermes fallback.
        self.assertNotIn("speech", cancel)
        phrases = set()
        for data in self.sentence_data:
            for item in data.get("intents", {}).get("StationCancel", {}).get("data", []):
                phrases.update(item.get("sentences", []))
        for word in ("отмена", "передумал", "не надо"):
            self.assertIn(word, phrases)

    def test_alarm_stop_has_bare_stop_and_is_unique(self):
        stop = INTENTS["StationAlarmStop"]
        dump = yaml.safe_dump(stop, allow_unicode=True)
        self.assertIn("script.schedule_alarm_stop", dump)
        self.assertIn("media_player.media_stop", dump)
        confirm = yaml.safe_dump(SCRIPTS["scheduler_confirm_ring_stopped"], allow_unicode=True)
        self.assertIn("script.alarm_stop", confirm)
        self.assertIn("wait_template", confirm)
        phrases = set()
        for data in self.sentence_data:
            for item in data.get("intents", {}).get("StationAlarmStop", {}).get("data", []):
                phrases.update(item.get("sentences", []))
        for word in ("стоп", "остановись", "хватит"):
            self.assertIn(word, phrases)

    def test_time_lists_cover_ranges(self):
        alarm = yaml.safe_load((HA / "custom_sentences" / "ru" / "station_alarm.yaml").read_text())
        hours = {int(v["out"]) for v in alarm["lists"]["alarm_hour"]["values"]}
        minutes = {int(v["out"]) for v in alarm["lists"]["alarm_minute"]["values"]}
        self.assertEqual(hours, set(range(24)))
        self.assertEqual(minutes, set(range(60)))


class AlarmMusicStateMachineTests(unittest.TestCase):
    def test_alarm_start_saves_source_volume_and_playback(self):
        dump = yaml.safe_dump(SCRIPTS["alarm_start"], allow_unicode=True)
        for marker in (
            "input_text.alarm_previous_source",
            "input_number.alarm_previous_volume",
            "input_boolean.alarm_music_was_playing",
            "media_player.media_pause",
            "script.alarm_ramp",
            "script.alarm_loop",
        ):
            self.assertIn(marker, dump)

    def test_alarm_audio_uses_underlying_player_to_preserve_source(self):
        dump = yaml.safe_dump(SCRIPTS["alarm_announce"], allow_unicode=True)
        self.assertIn("media_player.iandeks_stantsiia_midi_media_player", dump)
        self.assertIn("tts.speak", dump)
        self.assertNotIn("media_player_entity_id: media_player.iandeks_stantsiia_midi\n", dump)

    def test_alarm_stop_restores_then_resumes_music(self):
        svcs = services(SCRIPTS["alarm_stop"])
        self.assertIn("script.alarm_restore", svcs)
        self.assertIn("script.alarm_resume", svcs)
        self.assertIn("media_player.iandeks_stantsiia_midi_media_player", yaml.safe_dump(SCRIPTS["alarm_stop"]))
        self.assertLess(svcs.index("script.alarm_restore"), svcs.index("script.alarm_resume"))

    def test_resume_is_guarded_by_flag_and_clears_it(self):
        dump = yaml.safe_dump(SCRIPTS["alarm_resume"], allow_unicode=True)
        self.assertIn("input_boolean.alarm_music_was_playing", dump)
        self.assertIn("media_player.media_play", dump)
        self.assertIn("input_boolean.turn_off", dump)

    def test_alarm_stop_does_not_write_legacy_slot_state(self):
        dump = yaml.safe_dump(SCRIPTS["alarm_stop"], allow_unicode=True)
        for legacy in ("input_boolean.alarm_enabled", "input_boolean.alarm_snooze_active",
                       "input_select.alarm_repeat", "input_datetime.alarm_next"):
            self.assertNotIn(legacy, dump)

    def test_legacy_schedule_authority_is_gone(self):
        for name in ("alarm_set", "alarm_snooze", "alarm_cancel",
                     "alarm_schedule_next", "alarm_startup_reconcile",
                     "schedule_alarm_tick", "schedule_reminder_tick"):
            self.assertNotIn(name, SCRIPTS)
        automation_ids = {a["id"] for a in AUTOMATIONS}
        self.assertNotIn("alarm_scheduler", automation_ids)
        self.assertIn("schedule_alarm_create", SCRIPTS)

    def test_button_stop_uses_the_same_resume_path(self):
        automation = next(a for a in AUTOMATIONS if a["id"] == "alarm_button_stop")
        svcs = services(automation)
        self.assertIn("script.schedule_alarm_stop", svcs)
        self.assertIn(
            "script.scheduler_confirm_ring_stopped",
            yaml.safe_dump(SCRIPTS["schedule_alarm_stop"], allow_unicode=True),
        )
        self.assertIn(
            "script.alarm_stop",
            yaml.safe_dump(SCRIPTS["scheduler_confirm_ring_stopped"], allow_unicode=True),
        )
        dump = yaml.safe_dump(automation, allow_unicode=True)
        self.assertIn("event_type", dump)
        self.assertIn("action", dump)
        self.assertIn("alarm_ringing", dump)

    def test_normal_volume_commands_do_not_pause_music(self):
        for name in ("StationVolumeUp", "StationVolumeDown", "StationVolumeSet", "StationVolumeThreeAsr"):
            svcs = services(INTENTS[name])
            self.assertNotIn("media_player.media_pause", svcs)
            self.assertNotIn("media_player.media_stop", svcs)

    def test_connect_volume_sets_local_volume_before_optional_spotify_mirror(self):
        svcs = services(SCRIPTS["spotify_connect_set_volume"])
        self.assertIn("media_player.volume_set", svcs)
        self.assertIn("rest_command.spotify_volume", svcs)
        self.assertLess(
            svcs.index("media_player.volume_set"),
            svcs.index("rest_command.spotify_volume"),
        )
        dump = yaml.safe_dump(SCRIPTS["spotify_connect_set_volume"], allow_unicode=True)
        self.assertIn("volume_clamped", dump)
        self.assertIn("continue_on_error: true", dump)

    def test_intentional_music_commands_still_pause_and_stop(self):
        self.assertIn("media_player.media_pause", services(INTENTS["StationMusicPause"]))
        self.assertIn("media_player.media_stop", services(INTENTS["StationMusicStop"]))

    def test_next_and_previous_are_local_media_commands(self):
        self.assertIn("media_player.media_next_track", services(INTENTS["StationMusicNext"]))
        self.assertIn("media_player.media_previous_track", services(INTENTS["StationMusicPrevious"]))
        sentences = yaml.safe_dump(
            load_yaml(HA / "custom_sentences" / "ru" / "station_control.yaml"),
            allow_unicode=True,
        )
        for phrase in ("следующая", "следующая песня", "предыдущая", "предыдущий трек"):
            self.assertIn(phrase, sentences)

    def test_generic_play_music_is_handled_locally(self):
        self.assertIn("StationMusicPlay", INTENTS)
        sentences = yaml.safe_dump(
            load_yaml(HA / "custom_sentences" / "ru" / "station_control.yaml"),
            allow_unicode=True,
        )
        self.assertIn("включи музыку", sentences)
        self.assertIn("script.music_play_favorites", services(INTENTS["StationMusicPlay"]))


class MusicCascadeTests(unittest.TestCase):
    """Contract tests for search -> play, route safety and candidate memory."""

    @classmethod
    def setUpClass(cls):
        cls.search = SCRIPTS["music_search"]
        cls.play_uri = SCRIPTS["music_play_uri"]
        cls.play_query = SCRIPTS["music_play_query"]
        cls.play_candidate = SCRIPTS["music_play_candidate"]
        cls.status = SCRIPTS["music_status"]
        cls.play_query_dump = yaml.safe_dump(cls.play_query, allow_unicode=True)
        cls.play_uri_dump = yaml.safe_dump(cls.play_uri, allow_unicode=True)
        cls.search_dump = yaml.safe_dump(cls.search, allow_unicode=True)

    def test_every_music_script_is_documented_for_the_llm(self):
        for name in (
            "music_search",
            "music_play_uri",
            "music_play_query",
            "music_play_candidate",
            "music_status",
        ):
            with self.subTest(script=name):
                self.assertTrue(SCRIPTS[name].get("description"), f"{name} needs a description")

    def test_search_keeps_route_and_provider_separate(self):
        for marker in (
            "rest_command.spotify_search",
            "music_assistant.search",
            "input_select.music_route",
            "spotify_connect",
            "music_assistant",
            "is_playable",
            "is_local",
            "playable",
        ):
            self.assertIn(marker, self.search_dump)

    def test_search_handles_auth_rate_limit_and_empty_results(self):
        for status in ("auth_error", "rate_limited", "search_error", "no_results"):
            self.assertIn(status, self.search_dump)

    def test_search_reads_spotify_items_as_mapping_keys(self):
        self.assertNotIn(".tracks.items", self.search_dump)
        self.assertNotIn(".albums.items", self.search_dump)
        self.assertNotIn(".playlists.items", self.search_dump)
        self.assertNotIn(".artists.items", self.search_dump)
        self.assertIn("content.get", self.search_dump)
        self.assertIn("get('items'", self.search_dump)

    def test_play_uri_uses_connect_for_spotify_and_ma_for_library(self):
        for marker in (
            "rest_command.spotify_play_track",
            "rest_command.spotify_play_context",
            "music_assistant.play_media",
            "spotify_connect",
            "music_assistant",
            "media_id",
        ):
            self.assertIn(marker, self.play_uri_dump)

    def test_play_uri_handles_spotify_failures(self):
        for status in ("rate_limited", "auth_error", "premium_required", "device_unavailable", "play_error"):
            self.assertIn(status, self.play_uri_dump)

    def test_play_query_searches_before_playing_and_saves_candidates(self):
        svcs = services(self.play_query)
        self.assertIn("script.music_search", svcs)
        self.assertIn("script.music_play_uri", svcs)
        self.assertLess(svcs.index("script.music_search"), svcs.index("script.music_play_uri"))
        self.assertIn("input_text.music_candidates", self.play_query_dump)
        self.assertIn("parsed[0]", self.play_query_dump)
        self.assertIn("candidates_text[:255]", self.play_query_dump)

    def test_play_query_returns_selected_and_candidates(self):
        self.assertIn("selected:", self.play_query_dump)
        self.assertIn("candidates:", self.play_query_dump)
        self.assertIn("candidates:", self.play_query_dump)
        self.assertIn("candidates_json", self.play_query_dump)
        self.assertIn("retry_after", self.play_query_dump)

    def test_candidate_playback_uses_saved_route_and_uri(self):
        dump = yaml.safe_dump(self.play_candidate, allow_unicode=True)
        for marker in (
            "input_text.music_candidates",
            "script.music_play_uri",
            "part_route",
            "part_type",
            "part_uri",
            "parts_json",
            "no_candidates",
            "no_candidate",
            "candidate_corrupt",
        ):
            self.assertIn(marker, dump)

    def test_music_status_is_read_only_and_complete(self):
        dump = yaml.safe_dump(self.status, allow_unicode=True)
        for marker in ("media_title", "media_artist", "volume_level", "source", "music_route"):
            self.assertIn(marker, dump)
        self.assertNotIn("media_player.media_pause", dump)
        self.assertNotIn("media_player.media_stop", dump)

    def test_music_candidates_are_bounded_to_state_limit(self):
        inputs = load_yaml(HA / "input_text.yaml")
        self.assertIn("music_candidates", inputs)
        self.assertEqual(inputs["music_candidates"]["max"], 255)

    def test_music_does_not_touch_alarm_audio_or_source_regression(self):
        for dump in (self.play_query_dump, self.play_uri_dump, self.search_dump):
            self.assertNotIn("alarm_", dump)
            self.assertNotIn("media_player.iandeks_stantsiia_midi_media_player", dump)
            self.assertNotIn("tts.speak", dump)

    def test_spotify_search_is_read_only_and_templated(self):
        command = REST["spotify_search"]
        self.assertEqual(command["method"], "GET")
        self.assertIn("/v1/search", command["url"])
        self.assertIn("query | urlencode", command["url"])
        self.assertIn("media_type", command["url"])
        self.assertIn("Authorization", command["headers"])

    def test_spotify_play_commands_are_put_and_use_exact_uri(self):
        for name, payload_key in (("spotify_play_track", "uris"), ("spotify_play_context", "context_uri")):
            with self.subTest(command=name):
                command = REST[name]
                self.assertEqual(command["method"], "PUT")
                self.assertIn(payload_key, command["payload"])
                self.assertIn("{{ uri }}", command["payload"])
                self.assertIn("Authorization", command["headers"])

    def test_music_intents_cover_media_types_and_candidate_views(self):
        music = yaml.safe_load((HA / "custom_sentences" / "ru" / "station_music.yaml").read_text())
        self.assertTrue(music["lists"]["query"].get("wildcard"))
        media_type = {v["out"] for v in music["lists"]["media_type"]["values"]}
        self.assertEqual(media_type, {"track", "album", "playlist", "artist"})
        index = {v["out"] for v in music["lists"]["index"]["values"]}
        self.assertEqual(index, {"1", "2", "3"})
        phrases = []
        for intent in ("StationMusicPlayQuery", "StationMusicPlayCandidate", "StationMusicStatus"):
            for item in music["intents"][intent]["data"]:
                phrases.extend(item["sentences"])
        self.assertIn("включи {media_type} {query}", phrases)
        self.assertIn("{index}", phrases)
        self.assertIn("что играет", phrases)

    def test_music_speech_covers_user_visible_failures(self):
        dump = yaml.safe_dump(INTENTS["StationMusicPlayQuery"], allow_unicode=True)
        for marker in ("no_results", "rate_limited", "auth_error", "device_unavailable", "premium_required"):
            self.assertIn(marker, dump)
        candidate_dump = yaml.safe_dump(INTENTS["StationMusicPlayCandidate"], allow_unicode=True)
        for marker in ("no_candidates", "no_candidate", "candidate_corrupt"):
            self.assertIn(marker, candidate_dump)


class AssistSatelliteRecoveryTests(unittest.TestCase):
    def test_responding_watchdog_reloads_esphome_only_when_alarm_is_off(self):
        automation = next(
            a for a in AUTOMATIONS if a["id"] == "assist_satellite_responding_watchdog"
        )
        dump = yaml.safe_dump(automation, allow_unicode=True)
        self.assertIn("assist_satellite.iandeks_stantsiia_midi_assist_satellite", dump)
        self.assertIn("responding", dump)
        self.assertIn("00:00:25", dump)
        self.assertIn("input_boolean.alarm_ringing", dump)
        self.assertIn("homeassistant.reload_config_entry", services(automation))
        self.assertIn("01M5TAMM4T0CH5RASFJR8MSKT7", dump)


# Options HA accepts for a `text` selector; anything else disables the script
# at load time ("not a valid option"), which no other check here would catch.
TEXT_SELECTOR_OPTIONS = {
    "multiline", "type", "prefix", "suffix", "autocomplete", "multiple",
}


class SelectorOptionTests(unittest.TestCase):
    def test_text_selectors_use_only_known_options(self):
        for name, script in SCRIPTS.items():
            for field, spec in (script.get("fields") or {}).items():
                text = (spec.get("selector") or {}).get("text")
                if isinstance(text, dict):
                    unknown = set(text) - TEXT_SELECTOR_OPTIONS
                    self.assertFalse(unknown, f"{name}.{field}: {sorted(unknown)}")


class StationSpeakTests(unittest.TestCase):
    """The catalog "speak on the station" action calls this script."""

    SATELLITE = "assist_satellite.iandeks_stantsiia_midi_assist_satellite"

    def test_speak_text_is_a_bounded_queued_script(self):
        script = SCRIPTS["speak_text"]
        self.assertEqual(script["mode"], "queued")
        self.assertIn("description", script)
        field = script["fields"]["message"]
        self.assertTrue(field["required"])
        self.assertEqual(field["selector"]["text"], {"multiline": True})
        dump = yaml.safe_dump(script, allow_unicode=True)
        self.assertIn("1-500", dump)

    def test_speak_text_never_talks_over_the_alarm(self):
        script = SCRIPTS["speak_text"]
        first = script["sequence"][0]
        self.assertEqual(first["condition"], "template")
        self.assertIn("input_boolean.alarm_ringing", first["value_template"])

    def test_speak_text_uses_the_assist_satellite_and_falls_back(self):
        script = SCRIPTS["speak_text"]
        calls = services(script)
        self.assertIn("assist_satellite.announce", calls)
        self.assertIn("script.schedule_announce", calls)
        # tts.speak on a plain media player leaves the Sendspin group; the
        # announce path is the duck-safe one.
        self.assertNotIn("tts.speak", calls)
        dump = yaml.safe_dump(script, allow_unicode=True)
        self.assertIn(self.SATELLITE, dump)
        self.assertIn("preannounce: false", dump)

        announce = next(
            step
            for step in _walk(script["sequence"])
            if isinstance(step, dict) and step.get("service") == "assist_satellite.announce"
        )
        self.assertTrue(announce.get("continue_on_error"))
        fallback = next(
            step
            for step in _walk(script["sequence"])
            if isinstance(step, dict) and step.get("service") == "script.schedule_announce"
        )
        self.assertTrue(fallback.get("continue_on_error"))


def _walk(value: Any) -> list[Any]:
    """Every dict/list node of a parsed script, in source order."""
    if isinstance(value, dict):
        return [value] + [node for child in value.values() for node in _walk(child)]
    if isinstance(value, list):
        return [node for child in value for node in _walk(child)]
    return []


class VoiceStackTests(unittest.TestCase):
    def test_gigaam_wyoming_service_is_configured(self):
        compose = yaml.safe_load((ROOT / "projects" / "home-assistant" / "stack" / "docker-compose.yml").read_text())
        service = compose["services"]["onnx-stt"]
        self.assertEqual(service["network_mode"], "host")
        self.assertIn("./data/onnx-asr:/data", service["volumes"])
        env = service["environment"]
        self.assertEqual(env["STT_MODEL"], "${ONNX_STT_MODEL:-gigaam-v3-ctc}")
        self.assertEqual(env["STT_LANGUAGES"], "${ONNX_STT_LANGUAGES:-ru}")
        self.assertEqual(env["STT_DEVICE"], "cpu")
        self.assertEqual(env["STT_URI"], "tcp://0.0.0.0:10305")


class InputContractTests(unittest.TestCase):
    def test_alarm_and_route_inputs_are_consistent(self):
        booleans = yaml.safe_load((HA / "input_boolean.yaml").read_text())
        numbers = yaml.safe_load((HA / "input_number.yaml").read_text())
        selects = yaml.safe_load((HA / "input_select.yaml").read_text())
        dates = yaml.safe_load((HA / "input_datetime.yaml").read_text())
        for name in ("alarm_ringing", "alarm_music_was_playing"):
            self.assertIn(name, booleans)
        for name in ("alarm_enabled", "alarm_snooze_active"):
            self.assertNotIn(name, booleans)
        self.assertEqual(numbers["alarm_volume"]["min"], 0.1)
        self.assertEqual(numbers["alarm_volume"]["max"], 1)
        self.assertEqual(selects["music_route"]["options"], ["Music Assistant", "Spotify Connect"])
        self.assertNotIn("alarm_repeat", selects)
        # No legacy alarm datetimes remain; the only helper is the komp
        # shutdown request marker.
        self.assertEqual(set(dates), {"komp_off_requested"})
        self.assertNotIn("alarm_next", dates)
        self.assertNotIn("alarm_snooze_until", dates)


class VoiceProfileTests(unittest.TestCase):
    """Pin the voice-profile state, its catalogue switch and local intents."""

    PROFILE = "input_select.ricardo_voice_profile"
    SWITCH = "switch.ricardo_work_profile"
    STATION_COLOR = "select.iandeks_stantsiia_midi_voice_color"

    def _sentences(self) -> dict[str, list[str]]:
        control = yaml.safe_load(
            (HA / "custom_sentences" / "ru" / "station_control.yaml").read_text()
        )
        return {
            intent: [phrase for item in body.get("data", []) for phrase in item.get("sentences", [])]
            for intent, body in control["intents"].items()
        }

    def test_profile_input_is_the_single_source_of_truth(self):
        selects = yaml.safe_load((HA / "input_select.yaml").read_text())
        profile = selects["ricardo_voice_profile"]
        self.assertEqual(profile["options"], ["Личный", "Рабочий"])
        self.assertEqual(profile["initial"], "Личный")
        self.assertTrue(profile["name"])

    def test_profile_drives_the_station_listening_colour(self):
        automation = next(
            a for a in AUTOMATIONS if a["id"] == "voice_profile_station_color"
        )
        trigger_states = [
            t for t in automation["trigger"] if t.get("platform") == "state"
        ]
        self.assertIn(self.PROFILE, [t.get("entity_id") for t in trigger_states])
        self.assertIn(
            {"platform": "homeassistant", "event": "start"}, automation["trigger"]
        )
        # The station select may not exist yet: the condition skips the call instead of
        # failing the run.
        cond = automation["condition"][0]["value_template"]
        self.assertIn(self.STATION_COLOR, cond)
        self.assertIn("'unknown', 'unavailable'", cond)
        step = automation["action"][0]
        self.assertEqual(step["service"], "select.select_option")
        self.assertEqual(step["target"]["entity_id"], self.STATION_COLOR)
        self.assertEqual(
            step["data"]["option"],
            "{{ 'Blue' if is_state('input_select.ricardo_voice_profile', 'Рабочий') else 'Violet' }}",
        )

    def test_work_profile_switch_wraps_the_input_select(self):
        templates = load_yaml(HA / "templates.yaml")
        switches = [item for group in templates for item in group.get("switch", [])]
        switch = next(s for s in switches if s.get("default_entity_id") == self.SWITCH)
        self.assertEqual(switch["unique_id"], "ricardo_work_profile")
        self.assertIn(self.PROFILE, switch["state"])
        self.assertIn("Рабочий", switch["state"])

        def options(step_list):
            return [step.get("data", {}).get("option") for step in step_list]

        self.assertEqual(options(switch["turn_on"]), ["Рабочий"])
        self.assertEqual(options(switch["turn_off"]), ["Личный"])
        for steps in (switch["turn_on"], switch["turn_off"]):
            self.assertEqual(services(steps), ["input_select.select_option"])
            self.assertEqual(service_targets(steps), {self.PROFILE})

    def test_voice_profile_intents_are_local_and_short(self):
        for intent, option in (
            ("StationVoiceProfileWork", "Рабочий"),
            ("StationVoiceProfilePersonal", "Личный"),
        ):
            body = INTENTS[intent]
            self.assertEqual(services(body), ["input_select.select_option"])
            self.assertEqual(service_targets(body), {self.PROFILE})
            self.assertIn(option, yaml.safe_dump(body, allow_unicode=True))
            self.assertNotIn("conversation.hermes", yaml.safe_dump(body, allow_unicode=True))

        query = INTENTS["StationVoiceProfileQuery"]
        self.assertEqual(services(query), [])
        self.assertIn(self.PROFILE, yaml.safe_dump(query, allow_unicode=True))

        for intent in (
            "StationVoiceProfileWork",
            "StationVoiceProfilePersonal",
            "StationVoiceProfileQuery",
        ):
            text = INTENTS[intent]["speech"]["text"].strip()
            self.assertTrue(text)
            self.assertFalse(text.endswith("?"), f"{intent}: answer must not ask a question")

        try:
            from jinja2 import Environment
        except ImportError:  # pragma: no cover - optional local dependency
            self.skipTest("jinja2 is not installed")
        env = Environment()
        env.globals["is_state"] = lambda entity, value: entity == self.PROFILE and value == "Рабочий"
        rendered = env.from_string(
            INTENTS["StationVoiceProfileQuery"]["speech"]["text"]
        ).render().strip()
        self.assertLessEqual(len(rendered), 40, "profile query answer should be short")

    def test_profile_sentences_cover_phrases_and_do_not_collide_with_work_mode(self):
        sentences = self._sentences()
        expected = {
            "StationVoiceProfileWork": {
                "рабочий профиль",
                "включи рабочий профиль",
                "переключи на рабочий профиль",
            },
            "StationVoiceProfilePersonal": {
                "личный профиль",
                "включи личный профиль",
                "переключи на личный профиль",
            },
            "StationVoiceProfileQuery": {
                "какой профиль",
                "какой сейчас профиль",
                "какой профиль включён",
            },
        }
        for intent, phrases_wanted in expected.items():
            self.assertTrue(phrases_wanted.issubset(set(sentences[intent])), intent)

        work_mode = set(sentences["StationWorkMode"])
        for intent in expected:
            self.assertFalse(work_mode & set(sentences[intent]), f"{intent} overlaps StationWorkMode")

    def test_profile_query_answer_reflects_the_current_option(self):
        try:
            from jinja2 import Environment
        except ImportError:  # pragma: no cover - optional local dependency
            self.skipTest("jinja2 is not installed")
        template = INTENTS["StationVoiceProfileQuery"]["speech"]["text"]
        cases = (
            ("Рабочий", "Сейчас рабочий профиль."),
            ("Личный", "Сейчас личный профиль."),
        )
        for state, expected in cases:
            env = Environment()
            env.globals["is_state"] = lambda entity, value, state=state: entity == self.PROFILE and value == state
            self.assertEqual(env.from_string(template).render().strip(), expected)


class CurrencyRateTests(unittest.TestCase):
    """Pin the currency-bot API field mapping and the Russian TTS text.

    currency-bot stores kurs.kz `sell` as the rate when YOU BUY currency and
    `buy` as the rate when YOU SELL. The API keeps those names, so the voice
    text must say «купить — sell», «продать — buy». The JSON shapes here match
    `GET /api/latest` from solutions/currency-bot/crates/currency-bot/src/api/history.rs.
    """

    NOW = datetime(2026, 10, 31, 17, 0, 0, tzinfo=ZoneInfo("Asia/Almaty"))  # 12:00Z

    SAMPLE = {
        "timestamp": "2026-10-31T12:00:00Z",
        "rates": {
            "usdToKzt": {"buy": 509.8, "sell": 512.4},
            "rubToKzt": {"buy": 6.21, "sell": 6.35},
        },
    }

    @staticmethod
    def _as_timestamp(value, default=0):
        try:
            if isinstance(value, (int, float)):
                return float(value)
            text = str(value).strip()
            if text.endswith("Z"):
                text = text[:-1] + "+00:00"
            return datetime.fromisoformat(text).timestamp()
        except (TypeError, ValueError):
            return default

    def _timestamp_custom(self, value, fmt="%Y-%m-%d", local=True, default=""):
        try:
            tz = self.NOW.tzinfo if local else timezone.utc
            return datetime.fromtimestamp(int(float(value)), tz=tz).strftime(fmt)
        except (TypeError, ValueError, OSError):
            return default

    @staticmethod
    def _from_json(value, default=None):
        try:
            return json.loads(value)
        except (TypeError, ValueError, json.JSONDecodeError):
            return default

    def _environment(self):
        from jinja2.nativetypes import NativeEnvironment

        env = NativeEnvironment()
        env.filters.update(
            as_timestamp=self._as_timestamp,
            timestamp_custom=self._timestamp_custom,
            from_json=self._from_json,
            regex_replace=lambda value, find, repl="", ignorecase=False: re.sub(
                find, repl, str(value), flags=re.I if ignorecase else 0
            ),
        )
        env.globals.update(now=lambda: self.NOW)
        return env

    def render_currency(self, content, status=200, currency="both"):
        script = SCRIPTS["currency_rates"]
        variables = next(
            step["variables"]
            for step in script["sequence"]
            if isinstance(step.get("variables"), dict) and "currency_result" in step["variables"]
        )
        env = self._environment()
        context = {
            "currency": currency,
            "currency_http": {"status": status, "content": content},
        }
        for name, value in variables.items():
            rendered = env.from_string(value).render(**context) if isinstance(value, str) else value
            context[name] = rendered
        return context["currency_result"]

    def test_usd_speech_uses_sell_for_buy_and_buy_for_sell(self):
        result = self.render_currency(self.SAMPLE, currency="usd")
        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["text"], "Доллар: купить — 512,4, продать — 509,8 тенге.")

    def test_rub_speech_uses_two_decimals_and_the_same_mapping(self):
        result = self.render_currency(self.SAMPLE, currency="rub")
        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["text"], "Рубль: купить — 6,35, продать — 6,21 тенге.")

    def test_both_currencies_in_one_phrase(self):
        result = self.render_currency(self.SAMPLE, currency="both")
        self.assertEqual(result["status"], "ok")
        self.assertEqual(
            result["text"],
            "Доллар: купить — 512,4, продать — 509,8 тенге. "
            "Рубль: купить — 6,35, продать — 6,21 тенге.",
        )

    def test_number_formatting_has_no_trailing_zero_noise(self):
        content = {
            "timestamp": "2026-10-31T12:00:00Z",
            "rates": {
                "usdToKzt": {"buy": 509.0, "sell": 512.0},
                "rubToKzt": {"buy": 6.20, "sell": 6.35},
            },
        }
        result = self.render_currency(content, currency="both")
        self.assertIn("Доллар: купить — 512, продать — 509 тенге.", result["text"])
        self.assertIn("Рубль: купить — 6,35, продать — 6,2 тенге.", result["text"])

    def test_json_string_content_is_parsed(self):
        result = self.render_currency(json.dumps(self.SAMPLE), currency="usd")
        self.assertEqual(result["text"], "Доллар: купить — 512,4, продать — 509,8 тенге.")

    def test_fresh_data_has_no_staleness_note(self):
        result = self.render_currency(self.SAMPLE, currency="usd")
        self.assertNotIn("Данные от", result["text"])

    def test_old_data_appends_almaty_time(self):
        content = dict(self.SAMPLE, timestamp="2026-10-31T08:00:00Z")  # 13:00 Almaty
        result = self.render_currency(content, currency="usd")
        self.assertEqual(result["status"], "ok")
        self.assertTrue(result["text"].endswith(" Данные от 13:00."), result["text"])

    def test_api_error_is_short_and_has_no_technical_details(self):
        result = self.render_currency({}, status=503, currency="both")
        self.assertEqual(result, {"status": "error", "text": "Не удалось получить курс."})

    def test_missing_requested_currency_is_an_error(self):
        content = {
            "timestamp": "2026-10-31T12:00:00Z",
            "rates": {"usdToKzt": {"buy": 509.8, "sell": 512.4}},
        }
        result = self.render_currency(content, currency="rub")
        self.assertEqual(result, {"status": "error", "text": "Не удалось получить курс."})

    def test_incomplete_rate_fields_are_an_error(self):
        content = {
            "timestamp": "2026-10-31T12:00:00Z",
            "rates": {"usdToKzt": {"buy": 509.8}},
        }
        result = self.render_currency(content, currency="usd")
        self.assertEqual(result, {"status": "error", "text": "Не удалось получить курс."})

    def test_intent_is_local_and_sentences_cover_all_phrases(self):
        self.assertIn("StationCurrencyRate", INTENTS)
        dump = yaml.safe_dump(INTENTS["StationCurrencyRate"], allow_unicode=True)
        self.assertIn("script.currency_rates", dump)
        self.assertNotIn("conversation.hermes", dump)

        sentences = yaml.safe_load(
            (HA / "custom_sentences" / "ru" / "station_currency.yaml").read_text()
        )
        phrases = {
            phrase
            for item in sentences["intents"]["StationCurrencyRate"]["data"]
            for phrase in item["sentences"]
        }
        for template in (
            "курс {currency_target:currency}",
            "какой курс {currency_target:currency}",
            "курс {currency_target:currency} к тенге",
            "сколько стоит {currency_target:currency}",
        ):
            self.assertIn(template, phrases)
        targets = {value["in"]: value["out"] for value in sentences["lists"]["currency_target"]["values"]}
        self.assertEqual(targets["доллара"], "usd")
        self.assertEqual(targets["рубля"], "rub")
        self.assertEqual(targets["валют"], "both")
        self.assertEqual(targets["доллара и рубля"], "both")

        rest = REST["currency_latest"]
        self.assertIn("/api/latest", rest["url"])
        self.assertEqual(rest["method"], "GET")
        self.assertEqual(rest["headers"]["Authorization"], "__SECRET__")
        inputs = load_yaml(HA / "input_text.yaml")
        self.assertEqual(inputs["currency_bot_endpoint"]["initial"], "__SECRET__")


class KompPowerTests(unittest.TestCase):
    """Pin the wired desktop «комп»: WoL, ping state and restricted SSH.

    Real host/MAC/user never appear in tracked files: they only live in HA
    `secrets.yaml`, so every assertion here uses the `__SECRET__` placeholder.
    """

    BINARY_SENSOR = "binary_sensor.komp_v_seti"
    SWITCH = "switch.komp"
    TARGET_INPUT = "input_text.komp_ssh_target"
    OFF_INPUT = "input_datetime.komp_off_requested"

    @staticmethod
    def _squash(text: str) -> str:
        return " ".join(text.split())

    def _command_line(self) -> dict[str, Any]:
        groups = load_yaml(HA / "command_line.yaml")
        for group in groups:
            if "binary_sensor" in group:
                return group["binary_sensor"]
        self.fail("command_line.yaml has no binary_sensor group")

    def _switch(self) -> dict[str, Any]:
        templates = load_yaml(HA / "templates.yaml")
        switches = [item for group in templates for item in group.get("switch", [])]
        return next(s for s in switches if s.get("default_entity_id") == self.SWITCH)

    def _sentences(self) -> dict[str, list[str]]:
        komp = yaml.safe_load((HA / "custom_sentences" / "ru" / "station_komp.yaml").read_text())
        return {
            intent: [p for item in body.get("data", []) for p in item.get("sentences", [])]
            for intent, body in komp["intents"].items()
        }

    def _render(self, template: str, online: bool, off_requested_seconds: int | None = None) -> str:
        try:
            from jinja2 import Environment
        except ImportError:  # pragma: no cover - optional local dependency
            self.skipTest("jinja2 is not installed")
        env = Environment()
        base = datetime(2026, 11, 3, 12, 0, 0, tzinfo=timezone.utc)
        if off_requested_seconds is None:
            off_value = "unknown"
        else:
            off_value = (base - timedelta(seconds=off_requested_seconds)).strftime(
                "%Y-%m-%d %H:%M:%S"
            )
        env.globals["is_state"] = (
            lambda entity, value: entity == self.BINARY_SENSOR and value == ("on" if online else "off")
        )
        env.globals["states"] = (
            lambda entity: off_value if entity == self.OFF_INPUT else "unknown"
        )
        env.globals["now"] = lambda: base

        def _as_timestamp(value):
            if isinstance(value, datetime):
                return value.timestamp()
            return datetime.strptime(str(value), "%Y-%m-%d %H:%M:%S").replace(
                tzinfo=timezone.utc
            ).timestamp()

        env.globals["as_timestamp"] = _as_timestamp
        return self._squash(env.from_string(template).render())

    def test_ping_binary_sensor_uses_a_secret_command(self):
        sensor = self._command_line()
        self.assertEqual(sensor["name"], "Комп в сети")
        self.assertEqual(sensor["unique_id"], "komp_online")
        self.assertEqual(sensor["device_class"], "connectivity")
        # The whole command (busybox ping + the 1/0 wrapper) comes from secrets:
        # command_line has no way to splice `!secret` into a longer command.
        self.assertEqual(sensor["command"], "__SECRET__")
        self.assertEqual(sensor["payload_on"], "1")
        self.assertEqual(sensor["payload_off"], "0")
        # 10 s: Windows stays pingable for 1–2 minutes after shutdown, and a
        # 30 s poll made «включи комп» answer «Комп уже включён».
        self.assertEqual(sensor["scan_interval"], 10)
        self.assertNotIn("platform", sensor, "legacy platform YAML is not supported anymore")

    def test_off_request_helper_is_a_date_time(self):
        helpers = load_yaml(HA / "input_datetime.yaml")
        helper = helpers["komp_off_requested"]
        self.assertTrue(helper["has_date"])
        self.assertTrue(helper["has_time"])
        self.assertTrue(helper["name"])

    def test_switch_maps_state_and_actions_to_wol_and_ssh(self):
        switch = self._switch()
        self.assertEqual(switch["name"], "Комп")
        self.assertEqual(switch["unique_id"], "komp")
        self.assertEqual(switch["state"], "{{ is_state('binary_sensor.komp_v_seti', 'on') }}")

        # A one-time logon grant is issued first, then WoL. The grant step is
        # best-effort: a wakegrant outage must still wake the machine.
        self.assertEqual(services(switch["turn_on"]),
                         ["rest_command.wakegrant_issue", "wake_on_lan.send_magic_packet"])
        # The grant is issued only when the PC is down or shutting down, never
        # for an already running session: a second line of defence after the
        # KompOn intent.
        on_dump = yaml.safe_dump(switch["turn_on"], allow_unicode=True)
        self.assertIn(self.BINARY_SENSOR, on_dump)
        self.assertIn("continue_on_error: true", on_dump)
        wol = next(
            item for item in switch["turn_on"]
            if item.get("service") == "wake_on_lan.send_magic_packet"
        )
        mac = wol["data"]["mac"]
        self.assertEqual(mac, "__SECRET__", "MAC must come from secrets.yaml")
        # Compact catalogue/Android control: no role label, plain switch.
        self.assertNotIn("label", yaml.safe_dump(switch, allow_unicode=True))
        self.assertEqual(services(switch["turn_off"]), ["script.komp_shutdown"])

    def test_wakegrant_issue_uses_the_lan_endpoint_and_a_secret_bearer(self):
        rest = REST["wakegrant_issue"]
        self.assertEqual(rest["method"], "POST")
        self.assertIn("input_text.wakegrant_endpoint", rest["url"])
        self.assertTrue(rest["url"].endswith("/v1/grants"))
        self.assertEqual(rest["headers"]["Authorization"], "__SECRET__")
        self.assertEqual(rest["content_type"], "application/json")
        self.assertEqual(
            json.loads(rest["payload"]),
            {"machine": "komp", "purpose": "logon", "ttlSeconds": 180},
        )
        unlock = REST["wakegrant_issue_unlock"]
        self.assertEqual(unlock["method"], "POST")
        self.assertEqual(unlock["url"], rest["url"])
        self.assertEqual(unlock["headers"]["Authorization"], "__SECRET__")
        self.assertEqual(unlock["content_type"], "application/json")
        self.assertEqual(
            json.loads(unlock["payload"]),
            {"machine": "komp", "purpose": "unlock", "ttlSeconds": 60},
        )
        inputs = load_yaml(HA / "input_text.yaml")
        self.assertEqual(inputs["wakegrant_endpoint"]["initial"], "__SECRET__")
        self.assertTrue(inputs["wakegrant_endpoint"]["name"])

    def test_shutdown_shell_commands_keep_the_host_in_secrets(self):
        commands = load_yaml(HA / "shell_commands.yaml")
        target = "{{ states('input_text.komp_ssh_target') }}"
        for name, word in (("komp_shutdown", "shutdown"), ("komp_cancel_shutdown", "cancel")):
            with self.subTest(command=name):
                command = self._squash(commands[name])
                for option in (
                    "ssh -i /config/.ssh/komp_ed25519",
                    "-o BatchMode=yes",
                    "-o ConnectTimeout=5",
                    "-o UserKnownHostsFile=/config/.ssh/komp_known_hosts",
                    "-o StrictHostKeyChecking=accept-new",
                    target,
                ):
                    self.assertIn(option, command)
                self.assertTrue(command.endswith(" " + word), f"{name} must end with {word!r}")
                self.assertNotIn("@", command, "user@host must come from the secret target")
                # Only the two words the Windows wrapper accepts.
                self.assertNotIn("shutdown", commands["komp_cancel_shutdown"].split(target)[-1])

    def test_ssh_target_helper_is_initialised_from_secrets(self):
        inputs = load_yaml(HA / "input_text.yaml")
        self.assertEqual(inputs["komp_ssh_target"]["initial"], "__SECRET__")
        self.assertTrue(inputs["komp_ssh_target"]["name"])

    def test_scripts_call_the_restricted_commands(self):
        shutdown = SCRIPTS["komp_shutdown"]
        cancel = SCRIPTS["komp_cancel_shutdown"]
        # The request timestamp is written before SSH: the ping sensor lags, and
        # voice intents rely on this value to answer «выключается».
        self.assertEqual(
            services(shutdown),
            ["input_datetime.set_datetime", "shell_command.komp_shutdown"],
        )
        shutdown_dump = yaml.safe_dump(shutdown, allow_unicode=True)
        self.assertIn(self.OFF_INPUT, shutdown_dump)
        self.assertIn("now().strftime", shutdown_dump)
        self.assertEqual(
            services(cancel),
            ["shell_command.komp_cancel_shutdown", "input_datetime.set_datetime"],
        )
        # Empty/missing target secret must not launch a half-built ssh command.
        for script in (shutdown, cancel):
            dump = yaml.safe_dump(script, allow_unicode=True)
            self.assertIn(self.TARGET_INPUT, dump)
            self.assertIn("unavailable", dump)
        self.assertEqual(shutdown["mode"], "single")
        self.assertEqual(cancel["mode"], "single")

    def test_intents_are_local_and_reference_the_switch_entities(self):
        for intent in ("KompOn", "KompOff", "KompUnlock", "KompStatus"):
            dump = yaml.safe_dump(INTENTS[intent], allow_unicode=True)
            self.assertNotIn("conversation.hermes", dump)
            self.assertIn(self.BINARY_SENSOR, dump)
        self.assertNotIn(
            "conversation.hermes",
            yaml.safe_dump(INTENTS["KompCancelOff"], allow_unicode=True),
        )

        self.assertEqual(services(INTENTS["KompOn"]), ["switch.turn_on"])
        on_dump = yaml.safe_dump(INTENTS["KompOn"], allow_unicode=True)
        self.assertIn(self.SWITCH, on_dump)
        self.assertIn(self.OFF_INPUT, on_dump)
        # Voice must go through the switch: direct WoL skipped the wakegrant.
        self.assertNotIn("wake_on_lan.send_magic_packet", on_dump)
        self.assertEqual(services(INTENTS["KompOff"]), ["script.komp_shutdown"])
        self.assertIn(self.OFF_INPUT, yaml.safe_dump(INTENTS["KompOff"], allow_unicode=True))
        self.assertEqual(services(INTENTS["KompCancelOff"]), ["script.komp_cancel_shutdown"])
        self.assertEqual(services(INTENTS["KompUnlock"]), ["rest_command.wakegrant_issue_unlock"])
        self.assertNotIn(
            "wake_on_lan.send_magic_packet",
            yaml.safe_dump(INTENTS["KompUnlock"], allow_unicode=True),
        )
        self.assertEqual(services(INTENTS["KompStatus"]), [])

    def test_speech_matches_the_confirmed_wording(self):
        on = INTENTS["KompOn"]["speech"]["text"]
        off = INTENTS["KompOff"]["speech"]["text"]
        status = INTENTS["KompStatus"]["speech"]["text"]

        starting = "Включаю комп. Он загрузится примерно за минуту."
        self.assertEqual(self._render(on, False), starting)
        self.assertEqual(self._render(on, True), "Комп уже включён.")
        # Sensor still on, but a shutdown was requested 2 minutes ago: the PC is
        # going off, so wake it (the live «Комп уже включён» defect).
        self.assertEqual(self._render(on, True, off_requested_seconds=120), starting)
        self.assertEqual(self._render(on, True, off_requested_seconds=600), "Комп уже включён.")

        self.assertEqual(self._render(status, True), "Комп включён.")
        self.assertEqual(self._render(status, False), "Комп выключен.")
        self.assertEqual(self._render(status, True, off_requested_seconds=60), "Комп выключается.")
        self.assertEqual(self._render(status, True, off_requested_seconds=600), "Комп включён.")

        self.assertEqual(self._render(off, False), "Комп уже выключен.")
        self.assertEqual(
            self._render(off, True),
            "Выключу комп через минуту. Скажите «отмени выключение компа», если передумали.",
        )
        # A second request inside the 60 s window must not reschedule shutdown.
        self.assertEqual(self._render(off, True, off_requested_seconds=30), "Комп уже выключается.")
        self.assertEqual(
            self._render(off, True, off_requested_seconds=180),
            "Выключу комп через минуту. Скажите «отмени выключение компа», если передумали.",
        )
        self.assertEqual(
            self._render(INTENTS["KompCancelOff"]["speech"]["text"], False),
            "Отменил выключение компа.",
        )

        unlock = INTENTS["KompUnlock"]["speech"]["text"]
        self.assertEqual(self._render(unlock, True), "Разблокирую.")
        self.assertEqual(self._render(unlock, False), "Комп выключен.")

    def test_sentences_cover_the_required_phrases_without_collisions(self):
        sentences = self._sentences()
        self.assertEqual(
            set(sentences["KompOn"]),
            {"включи комп", "включи мой комп", "включи компьютер"},
        )
        self.assertEqual(
            set(sentences["KompOff"]),
            {"выключи комп", "выключи мой комп", "выключи компьютер"},
        )
        self.assertEqual(
            set(sentences["KompCancelOff"]),
            {"отмени выключение компа", "не выключай комп"},
        )
        self.assertEqual(
            set(sentences["KompUnlock"]),
            {"разблокируй комп", "разблокируй мой комп", "открой комп"},
        )
        self.assertTrue(
            {"комп включён", "мой комп включён", "включён ли комп", "комп работает"}
            <= set(sentences["KompStatus"])
        )
        self.assertEqual(
            set(sentences),
            {"KompOn", "KompOff", "KompCancelOff", "KompUnlock", "KompStatus"},
        )

        # No phrase is reused by another intent (duplicate detection also runs
        # globally), and the light intents cannot swallow «комп».
        others: dict[str, str] = {}
        for data in self.sentence_files_data():
            for intent, body in data.get("intents", {}).items():
                if intent.startswith("Komp"):
                    continue
                for item in body.get("data", []):
                    for phrase in item.get("sentences", []):
                        others[self._squash(phrase).lower()] = intent
        for phrases in sentences.values():
            for phrase in phrases:
                self.assertNotIn(phrase, others, f"{phrase!r} also belongs to {others.get(phrase)}")

        light_targets = {
            value["in"]
            for value in yaml.safe_load(
                (HA / "custom_sentences" / "ru" / "station_control.yaml").read_text()
            )["lists"]["light_target"]["values"]
        }
        self.assertFalse({"комп", "мой комп", "компьютер"} & light_targets)

    def sentence_files_data(self) -> list[dict[str, Any]]:
        return [yaml.safe_load(p.read_text()) for p in sorted((HA / "custom_sentences").rglob("*.yaml"))]

    def test_no_literal_ip_or_mac_in_tracked_ha_yaml(self):
        ipv4 = re.compile(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b")
        mac = re.compile(r"\b(?:[0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}\b")
        for path in sorted(HA.rglob("*.yaml")):
            text = path.read_text()
            for match in ipv4.finditer(text):
                address = match.group(0)
                with self.subTest(path=str(path), address=address):
                    # Loopback (local media), wildcard bind and TEST-NET only.
                    self.assertTrue(
                        address.startswith(("127.", "192.0.2.")) or address == "0.0.0.0",
                        f"unexpected literal address {address}",
                    )
            self.assertFalse(mac.search(text), f"literal MAC address in {path}")


if __name__ == "__main__":
    unittest.main(verbosity=2)
