#!/usr/bin/env python3
"""Opt-in end-to-end tests against the real Home Assistant.

Default mode is read-only: it checks API health, required entities, and safe
conversation intents.  Mutating/audio tests require explicit flags:

    STATION_E2E=1 HA_TOKEN=... python3 projects/home-assistant/e2e/live_ha.py -v
    STATION_E2E=1 STATION_E2E_MUTATE=1 HA_TOKEN=... ...
    STATION_E2E=1 STATION_E2E_AUDIO=1 HA_TOKEN=... ...

The alarm test is deliberately small (low volume, a few seconds) and always
restores the pre-test alarm volume, source, volume and flags.  If the user has
an alarm enabled, it skips rather than overwrite it.
"""
from __future__ import annotations

import json
import os
import time
import unittest
import urllib.error
import urllib.request

HA_URL = os.environ.get("HA_URL", "http://192.0.2.28:8123").rstrip("/")
HA_TOKEN = os.environ.get("HA_TOKEN", "").strip()
HA_DEVICE_ID = os.environ.get("HA_ASSIST_DEVICE_ID", "").strip()
E2E = os.environ.get("STATION_E2E") == "1"
MUTATE = E2E and os.environ.get("STATION_E2E_MUTATE") == "1"
AUDIO = E2E and os.environ.get("STATION_E2E_AUDIO") == "1"

PLAYER = "media_player.iandeks_stantsiia_midi"


def api(path: str, method: str = "GET", payload: dict | None = None) -> tuple[int, dict]:
    data = None if payload is None else json.dumps(payload).encode()
    req = urllib.request.Request(
        HA_URL + path,
        data=data,
        method=method,
        headers={
            "Authorization": f"Bearer {HA_TOKEN}",
            "Content-Type": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            body = resp.read().decode()
            return resp.status, (json.loads(body) if body else {})
    except urllib.error.HTTPError as exc:
        body = exc.read().decode(errors="replace")
        return exc.code, {"error": body[:500]}


def state(entity_id: str) -> dict:
    code, data = api("/api/states/" + entity_id)
    if code != 200:
        raise AssertionError(f"state {entity_id}: HTTP {code}: {data}")
    return data


def state_value(entity_id: str) -> str:
    return str(state(entity_id)["state"])


def call(domain: str, service: str, data: dict | None = None) -> tuple[int, dict]:
    return api(f"/api/services/{domain}/{service}", "POST", data or {})


def wait_for(entity_id: str, value: str, timeout: float = 12.0) -> str:
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        last = state_value(entity_id)
        if last == value:
            return last
        time.sleep(0.25)
    raise AssertionError(f"{entity_id} did not become {value!r}; last={last!r}")


def conversation(text: str) -> str:
    payload = {"text": text, "language": "ru"}
    if HA_DEVICE_ID:
        payload["device_id"] = HA_DEVICE_ID
    code, data = api("/api/conversation/process", "POST", payload)
    if code != 200:
        raise AssertionError(f"conversation {text!r}: HTTP {code}: {data}")
    response = data.get("response", {})
    speech = response.get("speech", {})
    plain = speech.get("plain", {}).get("speech") if isinstance(speech, dict) else None
    return plain or response.get("response_type", "")


@unittest.skipUnless(E2E and HA_TOKEN, "set STATION_E2E=1 and HA_TOKEN to run live tests")
class LiveReadOnlyTests(unittest.TestCase):
    def test_api_and_required_entities(self):
        code, data = api("/api/")
        self.assertEqual(code, 200)
        self.assertEqual(data.get("message"), "API running.")
        required = [
            "input_boolean.alarm_enabled",
            "input_boolean.alarm_ringing",
            "input_boolean.alarm_snooze_active",
            "input_boolean.alarm_music_was_playing",
            "input_datetime.alarm_next",
            "input_datetime.alarm_snooze_until",
            "input_number.alarm_volume",
            "input_number.alarm_previous_volume",
            "input_select.music_route",
            "input_select.alarm_repeat",
            "input_text.alarm_previous_source",
            "script.alarm_set",
            "script.alarm_start",
            "script.alarm_stop",
            "script.alarm_cancel",
            "script.alarm_snooze",
            "script.alarm_resume",
            "script.alarm_restore",
            "script.music_play_favorites",
            "script.spotify_connect_set_volume",
            "automation.alarm_scheduler",
            "automation.alarm_stop_with_pulsar_action_button",
            "automation.music_route_follows_player_source",
            "media_player.iandeks_stantsiia_midi",
            "event.iandeks_stantsiia_midi_button",
        ]
        for entity_id in required:
            with self.subTest(entity=entity_id):
                self.assertEqual(state(entity_id)["state"] in {"unknown", "unavailable"}, False, entity_id)

    def test_alarm_status_and_timer_status_intents(self):
        alarm = conversation("какой будильник")
        self.assertTrue(alarm, "empty alarm status response")
        self.assertIn("будильник", alarm.lower())
        timer = conversation("что с таймером")
        self.assertTrue(timer, "empty timer status response")

    def test_source_route_shape(self):
        route = state_value("input_select.music_route")
        self.assertIn(route, {"Spotify Connect", "Music Assistant"})
        player = state(PLAYER)
        attrs = player.get("attributes", {})
        self.assertIn("volume_level", attrs)
        self.assertIn("source", attrs)


@unittest.skipUnless(MUTATE, "set STATION_E2E_MUTATE=1 to allow volume mutation")
class LiveMutationTests(unittest.TestCase):
    def setUp(self):
        self.player = state(PLAYER)
        self.was_playing = self.player["state"] == "playing"
        self.old_volume = float(self.player.get("attributes", {}).get("volume_level", 0.3))
        self.old_source = self.player.get("attributes", {}).get("source")
        if not self.was_playing:
            self.skipTest("no active playback to verify non-destructive volume command")

    def tearDown(self):
        # Prefer Spotify Web API when Connect is active; it is the source that
        # actually owns the app-side gain. Fall back to the MA player.
        if (self.old_source or "").startswith("Spotify Connect"):
            call("script", "spotify_connect_set_volume", {"volume_percent": round(self.old_volume * 100)})
        else:
            call("media_player", "volume_set", {"entity_id": PLAYER, "volume_level": self.old_volume})

    def test_volume_command_keeps_playback(self):
        conversation("громкость 3")
        time.sleep(1)
        self.assertEqual(state_value(PLAYER), "playing")


@unittest.skipUnless(MUTATE, "set STATION_E2E_MUTATE=1 to allow alarm schedule mutation")
class LiveAlarmScheduleTests(unittest.TestCase):
    def setUp(self):
        if state_value("input_boolean.alarm_enabled") == "on":
            self.skipTest("an alarm is already enabled; refusing to overwrite it")
        self.old_repeat = state_value("input_select.alarm_repeat")
        self.old_next = state_value("input_datetime.alarm_next")
        self.player_before = state(PLAYER)
        self.was_playing = self.player_before["state"] == "playing"
        self.old_volume = float(self.player_before.get("attributes", {}).get("volume_level", 0.3))
        self.old_source = self.player_before.get("attributes", {}).get("source")

    def tearDown(self):
        # alarm_cancel() intentionally media_stop()s before restoring.  Snapshot
        # and restore the user's playback so this scheduling test is non-destructive.
        try:
            if (
                state_value("input_boolean.alarm_enabled") == "on"
                or state_value("input_boolean.alarm_ringing") == "on"
                or state_value("input_boolean.alarm_snooze_active") == "on"
            ):
                call("script", "alarm_cancel", {})
            if self.old_source:
                call("media_player", "select_source", {"entity_id": PLAYER, "source": self.old_source})
            if self.was_playing and state_value(PLAYER) != "playing":
                call("media_player", "media_play", {"entity_id": PLAYER})
        finally:
            call("input_select", "select_option", {"entity_id": "input_select.alarm_repeat", "option": self.old_repeat})
            if self.old_next not in {"unknown", "unavailable"}:
                call("input_datetime", "set_datetime", {"entity_id": "input_datetime.alarm_next", "datetime": self.old_next})
            if (self.old_source or "").startswith("Spotify Connect"):
                call("script", "spotify_connect_set_volume", {"volume_percent": round(self.old_volume * 100)})
            else:
                call("media_player", "volume_set", {"entity_id": PLAYER, "volume_level": self.old_volume})

    def test_alarm_set_and_cancel_without_ringing(self):
        call(
            "script",
            "alarm_set",
            {"hour": 7, "minute": 30, "day": "завтра", "repeat": "По будням"},
        )
        self.assertEqual(state_value("input_select.alarm_repeat"), "По будням")
        self.assertEqual(state_value("input_boolean.alarm_enabled"), "on")
        next_value = state_value("input_datetime.alarm_next")
        self.assertRegex(next_value, r"^\d{4}-\d{2}-\d{2} 07:30:00$")
        self.assertEqual(state_value("input_boolean.alarm_ringing"), "off")
        call("script", "alarm_cancel", {})
        self.assertEqual(state_value("input_boolean.alarm_enabled"), "off")


@unittest.skipUnless(AUDIO, "set STATION_E2E_AUDIO=1 to allow a short low-volume alarm test")
class LiveAudioAlarmTests(unittest.TestCase):
    def setUp(self):
        if state_value("input_boolean.alarm_enabled") == "on":
            self.skipTest("an alarm is already enabled; refusing to overwrite it")
        self.old_alarm_volume = float(state("input_number.alarm_volume")["state"])
        self.player_before = state(PLAYER)
        self.was_playing = self.player_before["state"] == "playing"
        self.old_volume = float(self.player_before.get("attributes", {}).get("volume_level", 0.3))
        self.old_source = self.player_before.get("attributes", {}).get("source")
        call("input_number", "set_value", {"entity_id": "input_number.alarm_volume", "value": 0.10})

    def tearDown(self):
        # Only stop when this test actually left an alarm ringing or paused
        # playback.  Calling alarm_stop/cancel unconditionally would stop a
        # healthy Spotify session (they intentionally media_stop first).
        try:
            if (
                state_value("input_boolean.alarm_ringing") == "on"
                or state_value("input_boolean.alarm_music_was_playing") == "on"
            ):
                call("script", "alarm_stop", {})
            if state_value("input_boolean.alarm_enabled") == "on":
                call("script", "alarm_cancel", {})
            if self.was_playing and state_value(PLAYER) != "playing":
                call("media_player", "media_play", {"entity_id": PLAYER})
        finally:
            call(
                "input_number",
                "set_value",
                {"entity_id": "input_number.alarm_volume", "value": self.old_alarm_volume},
            )
            if self.old_source:
                call("media_player", "select_source", {"entity_id": PLAYER, "source": self.old_source})
            if (self.old_source or "").startswith("Spotify Connect"):
                call("script", "spotify_connect_set_volume", {"volume_percent": round(self.old_volume * 100)})
            else:
                call("media_player", "volume_set", {"entity_id": PLAYER, "volume_level": self.old_volume})

    def test_alarm_start_stop_and_voice_stop_resume(self):
        call("script", "alarm_start", {})
        wait_for("input_boolean.alarm_ringing", "on")
        # alarm_start is a multi-step script.  Do not race its source/volume
        # capture with the stop request: ring goes on before the sequence ends.
        time.sleep(1.0)
        expected_flag = "on" if self.was_playing else "off"
        self.assertEqual(state_value("input_boolean.alarm_music_was_playing"), expected_flag)
        # The user's real phrase is wake word + "стоп"; the intent text itself is "стоп".
        conversation("стоп")
        wait_for("input_boolean.alarm_ringing", "off")
        # Let the queued announce/TTS teardown and resume settle.
        time.sleep(2.0)
        self.assertEqual(state_value("input_boolean.alarm_music_was_playing"), "off")
        if self.was_playing:
            self.assertEqual(state_value(PLAYER), "playing")
            if self.old_source:
                self.assertEqual(state(PLAYER)["attributes"].get("source"), self.old_source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
