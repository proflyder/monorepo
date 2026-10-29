#!/usr/bin/env python3
"""Static, fast tests for the Home Assistant voice package.

These tests do not need a running HA instance.  They catch the failures that
are expensive to discover through the satellite: broken YAML, unknown
entities/scripts, malformed Jinja, intent/sentence drift and regressions in the
alarm/music state machine.
"""
from __future__ import annotations

import re
import unittest
from collections import Counter
from pathlib import Path
from typing import Any

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
        ROOT / "projects" / "station" / "configs" / "updates-config.yml",
        ROOT / "projects" / "station" / "configs" / "docker-compose.yml",
        ROOT / "projects" / "station" / "configs" / "stack" / "docker-compose.yml",
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


UI_CREATED_INPUT_TEXT = {
    "tekushchii_budilnik_uid",
    "tekushchii_budilnik_nazvanie",
    "tekushchii_budilnik_spisok",
    *{f"taimer_{i}_nazvanie" for i in range(1, 9)},
}


def defined_entities() -> dict[str, set[str]]:
    result: dict[str, set[str]] = {}
    for prefix in ("input_boolean", "input_datetime", "input_number", "input_select", "input_text"):
        path = HA / f"{prefix}.yaml"
        result[prefix] = set(load_yaml(path).keys())
    result["input_text"] |= UI_CREATED_INPUT_TEXT
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

    def test_work_mode_turns_on_all_bedroom_lights(self):
        self.assertIn("script.work_mode", services(INTENTS["StationWorkMode"]))
        script = SCRIPTS["work_mode"]
        dump = yaml.safe_dump(script, allow_unicode=True)
        self.assertIn("light.turn_on", services(script))
        for entity_id in (
            "light.spalnia_lenta",
            "light.yeelink_cn_827036414_bslamp2_s_2_light",
            "light.yeelink_cn_919937191_lamp22_s_2",
        ):
            self.assertIn(entity_id, dump)
        phrases = {
            phrase
            for item in yaml.safe_load(
                (HA / "custom_sentences" / "ru" / "station_control.yaml").read_text()
            )["intents"]["StationWorkMode"]["data"]
            for phrase in item["sentences"]
        }
        self.assertIn("я сел работать", phrases)
        self.assertIn("включи рабочий режим", phrases)

    def test_blackout_turns_off_all_bedroom_lights(self):
        self.assertIn("script.blackout", services(INTENTS["StationBlackout"]))
        script = SCRIPTS["blackout"]
        dump = yaml.safe_dump(script, allow_unicode=True)
        self.assertIn("light.turn_off", services(script))
        for entity_id in (
            "light.spalnia_lenta",
            "light.yeelink_cn_827036414_bslamp2_s_2_light",
            "light.yeelink_cn_919937191_lamp22_s_2",
        ):
            self.assertIn(entity_id, dump)
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

    def test_cancel_also_restores_and_resumes(self):
        svcs = services(SCRIPTS["alarm_cancel"])
        self.assertIn("script.alarm_restore", svcs)
        self.assertIn("script.alarm_resume", svcs)

    def test_snooze_keeps_music_paused(self):
        svcs = services(SCRIPTS["alarm_snooze"])
        self.assertIn("media_player.media_stop", svcs)
        self.assertNotIn("script.alarm_resume", svcs)

    def test_resume_is_guarded_by_flag_and_clears_it(self):
        dump = yaml.safe_dump(SCRIPTS["alarm_resume"], allow_unicode=True)
        self.assertIn("input_boolean.alarm_music_was_playing", dump)
        self.assertIn("media_player.media_play", dump)
        self.assertIn("input_boolean.turn_off", dump)

    def test_new_alarm_clears_old_playback_state(self):
        dump = yaml.safe_dump(SCRIPTS["alarm_set"], allow_unicode=True)
        self.assertIn("input_boolean.alarm_music_was_playing", dump)
        self.assertIn("input_text.alarm_previous_source", dump)

    def test_scheduler_calls_start_for_regular_and_snooze(self):
        dump = yaml.safe_dump(AUTOMATIONS, allow_unicode=True)
        self.assertIn("script.alarm_start", dump)
        self.assertIn("alarm_schedule_next", dump)
        self.assertIn("alarm_startup_reconcile", dump)

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
        for name in ("alarm_enabled", "alarm_ringing", "alarm_snooze_active", "alarm_music_was_playing"):
            self.assertIn(name, booleans)
        self.assertEqual(numbers["alarm_volume"]["min"], 0.1)
        self.assertEqual(numbers["alarm_volume"]["max"], 1)
        self.assertEqual(selects["music_route"]["options"], ["Music Assistant", "Spotify Connect"])
        self.assertEqual(
            selects["alarm_repeat"]["options"],
            ["Однократно", "Каждый день", "По будням", "По выходным"],
        )
        self.assertTrue(dates["alarm_next"]["has_date"])
        self.assertTrue(dates["alarm_next"]["has_time"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
