#!/usr/bin/env python3
"""Opt-in live check: HA voice intents -> canonical SchedulerService.

Runs real Assistant `conversation.process` calls through the BUILTIN Home
Assistant agent (not the Hermes LLM agent) and verifies the resulting
scheduler state through the Connect JSON API.

Safety:
- read-only by default; mutations require STATION_E2E_MUTATE=1;
- every run uses a unique label prefix and cleans up only resources it created;
- the test alarm is always scheduled for TOMORROW at least 2 hours from now in
  the HA timezone, then deleted immediately; nothing rings or speaks;
- no token or URL credentials are printed.

Usage:
  HA_URL=http://<ha>:8123 HA_TOKEN=... SCHEDULER_URL=http://<node>:30098 \
  SCHEDULER_TOKEN_FILE=/path/token STATION_E2E=1 STATION_E2E_MUTATE=1 \
  python3 projects/home-assistant/e2e/live_scheduler_voice.py -v

Env:
  HA_URL (required in practice; default is a TEST-NET placeholder)
  HA_TOKEN (required)
  HA_VOICE_AGENT (default conversation.home_assistant)
  SCHEDULER_URL, SCHEDULER_TOKEN_FILE (required for verification/cleanup)
"""
from __future__ import annotations

import json
import os
import pathlib
import random
import unittest
import urllib.error
import urllib.request
from datetime import datetime, timedelta
from zoneinfo import ZoneInfo

HA_URL = os.environ.get("HA_URL", "http://192.0.2.28:8123").rstrip("/")
HA_TOKEN = os.environ.get("HA_TOKEN", "").strip()
AGENT = os.environ.get("HA_VOICE_AGENT", "conversation.home_assistant").strip()
SCHEDULER_URL = os.environ.get("SCHEDULER_URL", "").rstrip("/")
TOKEN_FILE = os.environ.get("SCHEDULER_TOKEN_FILE", "").strip()
LIVE = os.environ.get("STATION_E2E") == "1" and bool(HA_TOKEN)
MUTATE = LIVE and os.environ.get("STATION_E2E_MUTATE") == "1"

# Unique per process so cleanup can never touch another session's resources.
# Cyrillic only: the Russian sentence slots do not reliably match Latin labels.
_SUFFIX = "".join(random.choice("абвгдежзиклмнопрстуфхцчшэюя") for _ in range(6))
RUN_PREFIX = f"тестголос{_SUFFIX}"


def ha_post(path: str, payload: dict) -> dict:
    req = urllib.request.Request(
        HA_URL + path,
        data=json.dumps(payload).encode(),
        method="POST",
        headers={
            "Authorization": f"Bearer {HA_TOKEN}",
            "Content-Type": "application/json",
        },
    )
    with urllib.request.urlopen(req, timeout=45) as resp:
        body = resp.read().decode()
        return json.loads(body) if body else {}


def ha_get(path: str) -> dict:
    req = urllib.request.Request(
        HA_URL + path, headers={"Authorization": f"Bearer {HA_TOKEN}"}
    )
    with urllib.request.urlopen(req, timeout=20) as resp:
        return json.loads(resp.read().decode())


def say(text: str) -> str:
    data = ha_post(
        "/api/services/conversation/process?return_response=true",
        {"text": text, "language": "ru", "agent_id": AGENT},
    )
    response = data.get("service_response", {}).get("response", {})
    return (response.get("speech") or {}).get("plain", {}).get("speech") or ""


def scheduler_token() -> str:
    return pathlib.Path(TOKEN_FILE).read_text().strip()


def rpc(method: str, payload: dict) -> dict:
    req = urllib.request.Request(
        f"{SCHEDULER_URL}/ricardo.v1.SchedulerService/{method}",
        data=json.dumps(payload).encode(),
        method="POST",
        headers={
            "Authorization": f"Bearer {scheduler_token()}",
            "Content-Type": "application/json",
        },
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        body = resp.read().decode()
        return json.loads(body) if body else {}


def list_schedules() -> list[dict]:
    return rpc("ListSchedules", {"includeDisabled": True}).get("schedules", [])


def list_timers() -> list[dict]:
    return rpc("ListTimers", {"includeFinished": False}).get("timers", [])


def created_schedules(label: str) -> list[dict]:
    return [s for s in list_schedules() if s.get("label") == label]


def created_timers(label: str) -> list[dict]:
    return [t for t in list_timers() if t.get("label") == label]


def delete_exact(schedule_ids: list[str], timer_ids: list[str]) -> None:
    for schedule_id in schedule_ids:
        try:
            rpc("DeleteSchedule", {"id": schedule_id})
        except urllib.error.HTTPError:
            pass
    for timer_id in timer_ids:
        try:
            rpc("CancelTimer", {"id": timer_id})
        except urllib.error.HTTPError:
            pass


def ha_timezone() -> ZoneInfo:
    return ZoneInfo(ha_get("/api/config")["time_zone"])


class VoiceRouting(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        if not SCHEDULER_URL or not TOKEN_FILE:
            raise unittest.SkipTest("SCHEDULER_URL and SCHEDULER_TOKEN_FILE are required")

    def test_list_is_canonical(self) -> None:
        self.assertTrue(say("какие будильники"))

    @unittest.skipUnless(MUTATE, "set STATION_E2E_MUTATE=1 to create test resources")
    def test_alarm_create_list_delete(self) -> None:
        label = RUN_PREFIX
        schedule_ids: list[str] = []
        try:
            tz = ha_timezone()
            # now + 3h is always at least 2h away; HA rolls onceDate to the
            # next day when the wall time has already passed today.
            target = (datetime.now(tz) + timedelta(hours=3)).replace(second=0, microsecond=0)
            wall = f"{target.hour}:{target.minute:02d}"
            speech = say(f"поставь будильник {label} на {wall}")
            self.assertIn("установлен", speech.lower(), speech)
            matches = created_schedules(label)
            self.assertEqual(len(matches), 1, f"expected one canonical alarm: {speech}")
            schedule_ids = [m["id"] for m in matches]
            alarm = matches[0]
            self.assertEqual(alarm["kind"], "SCHEDULE_KIND_ALARM")
            self.assertEqual(alarm["repeat"], "REPEAT_KIND_ONCE")
            self.assertEqual(alarm["timezone"], str(tz))
            self.assertEqual(alarm["onceDate"], target.date().isoformat())
            self.assertTrue(alarm["enabled"])
            due = datetime.fromtimestamp(int(alarm["nextDueUnix"]), tz)
            self.assertEqual(due.date(), target.date())
            self.assertEqual(due.strftime("%H:%M"), f"{target.hour:02d}:{target.minute:02d}")
            self.assertGreater(due - datetime.now(tz), timedelta(hours=2))
        finally:
            delete_exact(schedule_ids, [])
        self.assertFalse(created_schedules(label))

    @unittest.skipUnless(MUTATE, "set STATION_E2E_MUTATE=1 to create test resources")
    def test_timer_keeps_name_and_roundtrip(self) -> None:
        label = f"{RUN_PREFIX} таймер"
        timer_ids: list[str] = []
        try:
            speech = say(f"поставь таймер {label} на 30 минут")
            self.assertIn("установлен", speech.lower(), speech)
            timers = created_timers(label)
            self.assertEqual(len(timers), 1, f"timer label was not preserved: {speech}")
            timer_ids = [t["id"] for t in timers]
            say(f"поставь таймер {label} на паузу")
            self.assertEqual(created_timers(label)[0]["state"], "TIMER_STATE_PAUSED")
            say(f"продолжи таймер {label}")
            self.assertEqual(created_timers(label)[0]["state"], "TIMER_STATE_RUNNING")
            say(f"отмени таймер {label}")
        finally:
            delete_exact([], timer_ids)
        self.assertFalse(created_timers(label))

    @unittest.skipUnless(MUTATE, "set STATION_E2E_MUTATE=1 to create test resources")
    def test_reminder_tomorrow(self) -> None:
        label = f"{RUN_PREFIX} напоминание"
        schedule_ids: list[str] = []
        try:
            tz = ha_timezone()
            tomorrow = (datetime.now(tz) + timedelta(days=1)).date()
            speech = say(f"напомни завтра в 9:00 {label} проверить духовку")
            self.assertIn("создан", speech.lower(), speech)
            matches = created_schedules(f"{label} проверить духовку")
            self.assertEqual(len(matches), 1, f"reminder not created: {speech}")
            schedule_ids = [m["id"] for m in matches]
            reminder = matches[0]
            self.assertEqual(reminder["kind"], "SCHEDULE_KIND_REMINDER")
            self.assertEqual(reminder["repeat"], "REPEAT_KIND_ONCE")
            self.assertEqual(reminder["onceDate"], tomorrow.isoformat())
            due = datetime.fromtimestamp(int(reminder["nextDueUnix"]), tz)
            self.assertEqual(due.date(), tomorrow)
            self.assertEqual(due.strftime("%H:%M"), "09:00")
        finally:
            delete_exact(schedule_ids, [])
        self.assertFalse(created_schedules(f"{label} проверить духовку"))


if __name__ == "__main__":
    if not LIVE:
        raise SystemExit("set STATION_E2E=1 and HA_TOKEN to run live checks")
    unittest.main()
