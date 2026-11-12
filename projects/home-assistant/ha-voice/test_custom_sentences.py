#!/usr/bin/env python3
"""Sentence-matching contract for the Russian voice intents.

These tests do not need a running Home Assistant.  They load the real
`custom_sentences/ru/*.yaml`, compile the HassIL sentence templates into
regular expressions and apply the same "best match" ranking as
`hassil.recognize_best` (fewer wildcards, more literal text, less text captured
by a wildcard, intent name):

1. every relative-alarm phrase promised in the voice docs resolves to
   `StationAlarmRelative` with the expected hours/minutes slots;
2. an alarm/timer phrase the closed lists cannot parse resolves to the local
   clarification guard instead of the built-in `HassMediaSearchAndPlay`
   sentence «<play> {search_query}» (live regression 2026-11-12: «поставь
   будильник через час» answered «Воспроизвожу» and started music);
3. the guard does not steal explicit timer or music commands.
"""
from __future__ import annotations

import re
import unittest
from pathlib import Path

import yaml

HA = Path(__file__).resolve().parent
SENTENCES_DIR = HA / "custom_sentences" / "ru"

# The built-in media search sentence that used to swallow alarm phrases.
# Real templates come from the home-assistant-intents package; this mirror is
# enough to prove the ranking. `_common.yaml` in that package defines <play>.
BUILTIN_PLAY = {
    "intent": "HassMediaSearchAndPlay",
    "template": "<play> {search_query}",
    "play": "(включи|поставь|запусти|вруби|играй)",
}


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


def _candidates() -> list[tuple[str, str]]:
    candidates = [(intent, template) for intent, templates in INTENTS.items() for template in templates]
    candidates.append((BUILTIN_PLAY["intent"], BUILTIN_PLAY["template"]))
    return candidates


def best_match(phrase: str) -> Match | None:
    text = normalize(phrase)
    matches: list[Match] = []
    for intent, template in _candidates():
        pattern, _ = compile_template(template)
        if pattern.match(text):
            matches.append(Match(intent, template, text))
    if not matches:
        return None
    return sorted(matches, key=lambda match: match.score)[0]


class RelativeAlarmPhraseTests(unittest.TestCase):
    def assert_alarm(self, phrase: str, **expected: str) -> None:
        match = best_match(phrase)
        self.assertIsNotNone(match, phrase)
        assert match is not None
        self.assertEqual(match.intent, "StationAlarmRelative", f"{phrase}: {match.slots}")
        self.assertEqual(match.slots, expected, phrase)

    def test_relative_alarm_phrases(self) -> None:
        cases = {
            "поставь будильник через час": {"hours": "1"},
            "поставь будильник через полчаса": {"minutes": "30"},
            "поставь будильник через минуту": {"minutes": "1"},
            "поставь будильник через полтора часа": {"hours": "1.5"},
            "поставь будильник через пару часов": {"hours": "2"},
            "поставь будильник через пару минут": {"minutes": "2"},
            "поставь будильник через 2 часа 30 минут": {"hours": "2", "minutes": "30"},
            "поставь будильник через двадцать минут": {"minutes": "20"},
            "поставь будильник через 1 час": {"hours": "1"},
            "заведи будильник через три часа": {"hours": "3"},
            "разбуди меня через час": {"hours": "1"},
            "разбуди меня через полчаса": {"minutes": "30"},
            "разбуди меня через 2 часа 15 минут": {"hours": "2", "minutes": "15"},
            "будильник через час": {"hours": "1"},
            "будильник через 10 минут": {"minutes": "10"},
        }
        for phrase, slots in cases.items():
            with self.subTest(phrase=phrase):
                self.assert_alarm(phrase, **slots)

    def test_named_alarm_keeps_its_label(self) -> None:
        match = best_match("поставь будильник работа через 2 часа")
        self.assertIsNotNone(match)
        assert match is not None
        self.assertEqual(match.intent, "StationAlarmRelative")
        self.assertEqual(match.slots["hours"], "2")
        self.assertEqual(match.slots["name"], "работа")

    def test_alarm_phrases_never_become_a_media_search(self) -> None:
        for phrase in (
            "поставь будильник через час",
            "разбуди меня через полчаса",
            "будильник через полтора часа",
        ):
            with self.subTest(phrase=phrase):
                match = best_match(phrase)
                assert match is not None
                self.assertNotEqual(match.intent, BUILTIN_PLAY["intent"])


class RelativeGuardTests(unittest.TestCase):
    def assert_guard(self, phrase: str, intent: str) -> None:
        match = best_match(phrase)
        self.assertIsNotNone(match, phrase)
        assert match is not None
        self.assertEqual(match.intent, intent, match.slots)

    def test_unparsed_alarm_duration_asks_instead_of_playing_music(self) -> None:
        for phrase in (
            "поставь будильник через сутки",
            "поставь будильник через 90 минут",
            "поставь будильник через полтора суток",
            "разбуди меня через двое суток",
            "будильник через несколько дней",
        ):
            with self.subTest(phrase=phrase):
                self.assert_guard(phrase, "StationAlarmRelativeUnclear")
                match = best_match(phrase)
                assert match is not None
                self.assertNotEqual(match.intent, BUILTIN_PLAY["intent"])

    def test_unparsed_timer_duration_asks_instead_of_playing_music(self) -> None:
        for phrase in (
            "поставь таймер через час",
            "запусти таймер через полчаса",
            "таймер через 5 минут",
        ):
            with self.subTest(phrase=phrase):
                self.assert_guard(phrase, "StationTimerRelativeUnclear")
                match = best_match(phrase)
                assert match is not None
                self.assertNotEqual(match.intent, BUILTIN_PLAY["intent"])

    def test_explicit_timers_and_music_stay_on_their_intents(self) -> None:
        match = best_match("поставь таймер на 5 минут")
        assert match is not None
        self.assertEqual(match.intent, "StationTimerStart")
        self.assertEqual(match.slots["minutes"], "5")

        match = best_match("поставь таймер пицца на 10 минут")
        assert match is not None
        self.assertEqual(match.intent, "StationTimerStart")
        self.assertEqual(match.slots["minutes"], "10")

        match = best_match("включи песню белые розы")
        assert match is not None
        self.assertEqual(match.intent, "StationMusicPlayQuery")

        # A plain media request still reaches the built-in play sentence, so
        # the guard did not take over music.
        match = best_match("включи битлз")
        assert match is not None
        self.assertEqual(match.intent, BUILTIN_PLAY["intent"])

    def test_guard_intents_are_local_and_do_not_call_the_scheduler(self) -> None:
        intent_scripts = yaml.safe_load((HA / "intent_scripts.yaml").read_text(encoding="utf-8"))
        for intent in ("StationAlarmRelativeUnclear", "StationTimerRelativeUnclear"):
            with self.subTest(intent=intent):
                body = intent_scripts[intent]
                self.assertEqual(body.get("action"), [])
                text = body["speech"]["text"]
                self.assertTrue(text.strip())
                self.assertNotIn("schedule_", text)


if __name__ == "__main__":
    unittest.main(verbosity=2)
