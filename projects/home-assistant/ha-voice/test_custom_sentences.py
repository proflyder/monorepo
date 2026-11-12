#!/usr/bin/env python3
"""Sentence-routing contract for the Russian voice intents.

The time domain no longer parses phrases in Home Assistant. These tests load the
real ``custom_sentences/ru/*.yaml`` and the real ``intent_scripts.yaml`` and
prove the split that replaced the closed alarm/timer templates:

1. 30+ Russian time phrases (absolute, relative, repeats, timers, reminders,
   status and cancellation) resolve to ``StationTimeRoute`` and never to the
   built-in ``HassMediaSearchAndPlay`` sentence;
2. ``StationTimeRoute`` is a pure forwarder: it calls
   ``conversation.hermes_agent`` and never a ``script.schedule_*`` writer;
3. the only local time intents left are the parameterless ring commands
   (``StationAlarmStop`` and ``StationAlarmSnooze``);
4. music, lights and the plain "поставь queen" media request keep their own
   paths, so the router does not steal unrelated commands.
"""
from __future__ import annotations

import re
import unittest
from pathlib import Path

import yaml

HA = Path(__file__).resolve().parent
SENTENCES_DIR = HA / "custom_sentences" / "ru"

# The built-in media search sentence that swallowed alarm phrases. Real
# templates come from the home-assistant-intents package; this mirror is enough
# to prove the ranking. `_common.yaml` in that package defines <play>.
BUILTIN_PLAY = {
    "intent": "HassMediaSearchAndPlay",
    "template": "<play> {search_query}",
    "play": "(включи|поставь|запусти|вруби|играй)",
}

TIME_INTENTS = (
    "StationTimeRouteAlarm",
    "StationTimeRouteTimer",
    "StationTimeRouteReminder",
    "StationTimeRouteWake",
    "StationTimeRouteRemind",
)


def normalize(text: str) -> str:
    text = text.lower().replace("ё", "е")
    text = re.sub(r"[,.!?]", " ", text)
    return " ".join(text.split())


def load_sentences() -> tuple[dict, dict]:
    lists: dict[str, dict] = {}
    intents: dict[str, list[str]] = {}
    for path in sorted(SENTENCES_DIR.glob("*.yaml")):
        data = yaml.safe_load(path.read_text(encoding="utf-8"))
        lists.update(data.get("lists") or {})
        for intent, body in (data.get("intents") or {}).items():
            for item in body.get("data", []):
                intents.setdefault(intent, []).extend(item.get("sentences", []))
    return lists, intents


LISTS, INTENTS = load_sentences()


def _slot_regex(slot: str, alias: str | None) -> tuple[str, bool]:
    name = alias or slot
    spec = LISTS.get(slot)
    if spec is None or spec.get("wildcard"):
        return rf"(?P<{name}>.+?)", True
    values = [normalize(str(value["in"])) for value in spec.get("values", [])]
    if not values:
        raise AssertionError(f"list {slot!r} has no values and is not a wildcard")
    values.sort(key=len, reverse=True)
    return rf"(?P<{name}>(?:{'|'.join(re.escape(v) for v in values)}))", False


def _normalized_values() -> dict[str, str]:
    mapping: dict[str, str] = {}
    for slot, spec in LISTS.items():
        for value in spec.get("values", []):
            key = normalize(str(value["in"]))
            mapped = str(value["out"])
            if key in mapping and mapping[key] != mapped:
                raise AssertionError(
                    f"conflicting list values for {key!r}: {mapping[key]!r} vs {mapped!r}"
                )
            mapping[key] = mapped
    return mapping


IN_TO_OUT = _normalized_values()


def compile_template(template: str) -> tuple[re.Pattern, list[str]]:
    template = template.replace("<play>", BUILTIN_PLAY["play"])
    pattern: list[str] = []
    wildcards: list[str] = []
    index = 0
    while index < len(template):
        char = template[index]
        if char == "[":
            pattern.append("(?:")
        elif char == "]":
            pattern.append(")?")
        elif char == "(":
            pattern.append("(?:")
        elif char in "|)":
            pattern.append(char)
        elif char == "{":
            end = template.index("}", index)
            raw = template[index + 1 : end]
            slot, _, alias = raw.partition(":")
            regex, is_wildcard = _slot_regex(slot, alias or None)
            pattern.append(regex)
            if is_wildcard:
                wildcards.append(alias or slot)
            index = end
        elif char == "<":
            raise AssertionError(f"unexpanded expansion rule in {template!r}")
        elif char == " ":
            # HassIL is token-based: an omitted optional word («[и]») also drops
            # its separator, so a literal space must be allowed to collapse.
            pattern.append(r"\s*")
        else:
            pattern.append(re.escape(char))
        index += 1
    return re.compile("^" + "".join(pattern) + "$"), wildcards


class Match:
    def __init__(self, intent: str, template: str, text: str) -> None:
        pattern, self.wildcards = compile_template(template)
        match = pattern.match(text)
        assert match is not None
        slots: dict[str, str] = {}
        literal = len(text)
        wildcard_chars = 0
        for key, value in match.groupdict().items():
            if value is None:
                continue
            literal -= len(value)
            if key in self.wildcards:
                wildcard_chars += len(value)
                slots[key] = value
                continue
            slots[key] = IN_TO_OUT[normalize(value)]
        self.intent = intent
        self.slots = slots
        self.score = (len(self.wildcards), -literal, wildcard_chars, intent)


def best_match(phrase: str) -> Match | None:
    text = normalize(phrase)
    candidates = [(intent, template) for intent, templates in INTENTS.items() for template in templates]
    candidates.append((BUILTIN_PLAY["intent"], BUILTIN_PLAY["template"]))
    matches: list[Match] = []
    for intent, template in candidates:
        pattern, _ = compile_template(template)
        if pattern.match(text):
            matches.append(Match(intent, template, text))
    if not matches:
        return None
    return sorted(matches, key=lambda match: match.score)[0]


# 35+ live STT formulations covering every time operation, including the
# 2026-11-12 incidents «через час» and «на двадцать два ноль ноль».
TIME_PHRASES: dict[str, dict[str, str]] = {
    # Absolute alarms
    "поставь будильник на двадцать два ноль ноль": {},
    "поставь будильник на двадцать два ноль-ноль": {},
    "поставь будильник на 22:00": {},
    "разбуди меня в двадцать два ноль ноль": {},
    "разбуди меня в семь утра": {},
    "будильник на семь утра": {},
    "поставь будильник работа на 7:00": {},
    "поставь будильник на десять вечера": {},
    "поставь будильник на полночь": {},
    "поставь будильник пораньше": {},
    "сделай будильник в десять": {},
    # Relative alarms
    "поставь будильник через час": {},
    "поставь будильник через полчаса": {},
    "поставь будильник через полтора часа": {},
    "поставь будильник через 2 часа 30 минут": {},
    "поставь будильник через сутки": {},
    "разбуди меня через полчаса": {},
    "разбуди меня через пару часов": {},
    "будильник через 10 минут": {},
    # Repeats
    "поставь будильник на семь по будням": {},
    "поставь будильник на 8:00 каждый день": {},
    "поставь будильник на девять по выходным": {},
    # Status / list / cancel / move
    "какие будильники": {"command": "какие"},
    "какой у меня будильник": {"command": "какой"},
    "когда будильник": {"command": "когда"},
    "во сколько будильник": {"command": "во сколько"},
    "покажи будильники": {"command": "покажи"},
    "отмени будильник": {"command": "отмени"},
    "удали будильники": {"command": "удали"},
    "перенеси будильник на 8:00": {"command": "перенеси"},
    "выключи будильник на семь": {"command": "выключи"},
    # Timers
    "поставь таймер на 5 минут": {"command": "поставь"},
    "поставь таймер пицца на 10 минут": {"command": "поставь"},
    "таймер на семь минут": {},
    "поставь таймер через час": {"command": "поставь"},
    "какие таймеры": {"command": "какие"},
    "сколько осталось на таймере": {},
    "отмени таймер": {"command": "отмени"},
    "поставь таймер на паузу": {"command": "поставь"},
    "продолжи таймер": {"command": "продолжи"},
    # Reminders
    "напомни завтра в 9:00 позвонить маме": {},
    "напомни мне через два часа выключить духовку": {},
    "какие напоминания": {"command": "какие"},
    "покажи напоминания": {"command": "покажи"},
    "удали напоминание про молоко": {"command": "удали"},
}


class TimeRouteTests(unittest.TestCase):
    def test_at_least_thirty_time_phrases_route_to_hermes(self) -> None:
        self.assertGreaterEqual(len(TIME_PHRASES), 30)
        for phrase, slots in TIME_PHRASES.items():
            with self.subTest(phrase=phrase):
                match = best_match(phrase)
                self.assertIsNotNone(match, phrase)
                assert match is not None
                self.assertIn(match.intent, TIME_INTENTS, f"{phrase}: {match.intent} {match.slots}")
                self.assertNotEqual(match.intent, BUILTIN_PLAY["intent"], phrase)
                for key, value in slots.items():
                    self.assertEqual(match.slots.get(key), value, phrase)

    def test_router_never_parses_a_clock_time(self) -> None:
        # The router only captures the tail as free text; there are no hour,
        # minute, period or repeat slots. Time parsing belongs to Hermes.
        for phrase in TIME_PHRASES:
            match = best_match(phrase)
            assert match is not None
            for slot in ("hour", "minute", "hours", "minutes", "period", "repeat", "day"):
                self.assertNotIn(slot, match.slots, phrase)

    def test_ring_commands_stay_local(self) -> None:
        for phrase, intent in {
            "стоп": "StationAlarmStop",
            "хватит": "StationAlarmStop",
            "остановись": "StationAlarmStop",
            "выключи звонок": "StationAlarmStop",
            "останови будильник": "StationAlarmStop",
            "отложи будильник": "StationAlarmSnooze",
            "отложи на девять минут": "StationAlarmSnooze",
        }.items():
            with self.subTest(phrase=phrase):
                match = best_match(phrase)
                assert match is not None
                self.assertEqual(match.intent, intent)

    def test_music_and_lights_are_not_stolen_by_the_router(self) -> None:
        for phrase, intent in {
            "включи музыку": "StationMusicPlay",
            "включи песню белые розы": "StationMusicPlayQuery",
            "выключи лампу": "StationLightOff",
            "какой профиль": "StationVoiceProfileQuery",
            "отмена": "StationCancel",
        }.items():
            with self.subTest(phrase=phrase):
                match = best_match(phrase)
                assert match is not None
                self.assertEqual(match.intent, intent)

        # A plain media request still reaches the built-in play sentence.
        match = best_match("поставь queen")
        assert match is not None
        self.assertEqual(match.intent, BUILTIN_PLAY["intent"])


class TimeRouterScriptTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.intents = yaml.safe_load((HA / "intent_scripts.yaml").read_text(encoding="utf-8"))

    def test_router_forwards_to_hermes_and_never_writes_a_schedule(self) -> None:
        dump = yaml.safe_dump({name: self.intents[name] for name in TIME_INTENTS}, allow_unicode=True)
        self.assertIn("conversation.process", dump)
        self.assertIn("conversation.hermes_agent", dump)
        for forbidden in (
            "script.schedule_alarm_create",
            "script.schedule_alarm_create_relative",
            "script.schedule_timer_start",
            "script.schedule_reminder_create",
        ):
            self.assertNotIn(forbidden, dump)

    def test_removed_time_intent_scripts_are_gone(self) -> None:
        for name in (
            "StationAlarmSet",
            "StationAlarmRelative",
            "StationAlarmRelativeUnclear",
            "StationAlarmUnclear",
            "StationAlarmStatus",
            "StationAlarmCancel",
            "StationAlarmDelete",
            "StationAlarmUpdate",
            "StationAlarmEnableOne",
            "StationAlarmDeleteOne",
            "StationAlarmDisableOne",
            "StationTimerStart",
            "StationTimerRelativeUnclear",
            "StationTimerUnclear",
            "StationTimerStatus",
            "StationTimerCancel",
            "StationTimerPause",
            "StationTimerResume",
            "StationReminderStatus",
            "StationReminderCreate",
            "StationReminderDelete",
        ):
            with self.subTest(intent=name):
                self.assertNotIn(name, self.intents)
                self.assertNotIn(name, INTENTS)

    def test_no_alarm_or_timer_time_lists_remain(self) -> None:
        for name in ("alarm_hour", "alarm_minute", "alarm_period", "alarm_day", "alarm_repeat", "timer_minutes", "timer_hours", "timer_seconds"):
            with self.subTest(list=name):
                self.assertNotIn(name, LISTS)


class SharedSentenceListTests(unittest.TestCase):
    """Home Assistant merges `lists` by name; later files win."""

    def test_lists_with_the_same_name_are_identical(self) -> None:
        seen: dict[str, tuple[str, object]] = {}
        for path in sorted(SENTENCES_DIR.glob("*.yaml")):
            data = yaml.safe_load(path.read_text(encoding="utf-8"))
            for name, spec in (data.get("lists") or {}).items():
                if name not in seen:
                    seen[name] = (path.name, spec)
                    continue
                first_path, first_spec = seen[name]
                with self.subTest(list=name):
                    self.assertEqual(
                        spec,
                        first_spec,
                        f"{name} differs between {first_path} and {path.name}; "
                        "the last loaded file silently wins",
                    )


if __name__ == "__main__":
    unittest.main(verbosity=2)
