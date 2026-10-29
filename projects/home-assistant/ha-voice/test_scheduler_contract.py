#!/usr/bin/env python3
"""Contract tests for the canonical SchedulerService voice integration.

These tests are static and hermetic. They load the real HA YAML, render the
actual Jinja expressions with a fake clock/timezone and fake Connect JSON
responses, and assert that requests/speech follow the protobuf contract.

No Home Assistant, scheduler, bridge, network or device is contacted.
"""
from __future__ import annotations

import itertools
import json
import re
import unittest
from datetime import datetime, timedelta
from pathlib import Path
from zoneinfo import ZoneInfo

import yaml
from jinja2.nativetypes import NativeEnvironment

ROOT = Path(__file__).resolve().parents[3]
HA = ROOT / "projects" / "home-assistant" / "ha-voice"


class UniqueKeyLoader(yaml.SafeLoader):
    def construct_mapping(self, node, deep=False):  # type: ignore[no-untyped-def]
        mapping = {}
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


UniqueKeyLoader.add_constructor("!secret", lambda loader, node: "__SECRET__")
UniqueKeyLoader.add_constructor("!include", lambda loader, node: ["__INCLUDE__"])


def load(path: Path):
    return yaml.load(path.read_text(encoding="utf-8"), Loader=UniqueKeyLoader)


SCRIPTS = load(HA / "scripts.yaml")
INTENTS = load(HA / "intent_scripts.yaml")
AUTOMATIONS = load(HA / "automations.yaml")
REST = load(HA / "rest_commands.yaml")
INPUT_TEXT = load(HA / "input_text.yaml")
DASHBOARD = load(HA / "schedules_dashboard.yaml")
SENTENCES = yaml.safe_load(
    (HA / "custom_sentences" / "ru" / "station_alarm.yaml").read_text(encoding="utf-8")
)

NOW = datetime(2026, 10, 29, 6, 0, 0, tzinfo=ZoneInfo("Asia/Almaty"))


def ts_at(day_offset=0, hour=7, minute=30):
    d = (NOW + timedelta(days=day_offset)).replace(hour=hour, minute=minute, second=0, microsecond=0)
    return str(int(d.timestamp()))


def _timestamp_custom(value, fmt="%Y-%m-%d", local=True, default=""):
    try:
        tz = NOW.tzinfo if local else None
        return datetime.fromtimestamp(int(float(value)), tz=tz).strftime(fmt)
    except (TypeError, ValueError, OSError):
        return default


def _timezone_custom(value, fmt="%Y-%m-%d", local=True, default=""):
    return _timestamp_custom(value, fmt, local, default)


def _from_json(value, default=None):
    try:
        return json.loads(value)
    except (TypeError, ValueError, json.JSONDecodeError):
        return default


_RANDOM_COUNTER = itertools.count(1)


def _bool_filter(value, default=False):
    if isinstance(value, bool):
        return value
    if value is None:
        return default
    if isinstance(value, str):
        lowered = value.strip().lower()
        if lowered in ("1", "true", "yes", "on"):
            return True
        if lowered in ("0", "false", "no", "off", ""):
            return False
        return default
    return bool(value)


def _random_filter(sequence):
    # Deterministic test stand-in for HA's random filter: unique per render.
    next(_RANDOM_COUNTER)
    return next(_RANDOM_COUNTER)


ENV = NativeEnvironment()
ENV.filters.update(
    timestamp_custom=_timestamp_custom,
    timezone_custom=_timezone_custom,
    to_json=lambda value: json.dumps(value, ensure_ascii=False, separators=(",", ":")),
    from_json=_from_json,
    bool=_bool_filter,
    random=_random_filter,
    regex_replace=lambda value, find, repl="", ignorecase=False: re.sub(
        find, repl, str(value), flags=re.I if ignorecase else 0
    ),
)
ENV.globals.update(now=lambda: NOW, timedelta=timedelta)


class _Context:
    def __init__(self, context_id="ctx-test-1"):
        self.id = context_id


BASE_CONTEXT = {
    "context": _Context(),
    "states": lambda entity_id: "",
    "is_state": lambda entity_id, state: False,
    "as_timestamp": lambda value: 0,
    "as_datetime": lambda value: NOW,
}


def render(template, context):
    return ENV.from_string(str(template)).render(**context)


def render_variables(script, wanted: str, context: dict) -> dict:
    """Render the variables step containing `wanted` in declaration order."""
    for step in script["sequence"]:
        variables = step.get("variables")
        if not isinstance(variables, dict) or wanted not in variables:
            continue
        rendered = dict(context)
        for name, value in variables.items():
            rendered[name] = render(value, rendered) if isinstance(value, str) else value
        return rendered
    raise KeyError(f"{wanted} not found")


def render_first_variables(script, marker: str, context: dict) -> dict:
    for step in script["sequence"]:
        variables = step.get("variables") or {}
        if marker in variables:
            rendered = dict(context)
            for name, value in variables.items():
                rendered[name] = render(value, rendered) if isinstance(value, str) else value
            return rendered
    raise KeyError(marker)


def render_result(script_name: str, result_name: str, context: dict):
    for step in SCRIPTS[script_name]["sequence"]:
        variables = step.get("variables") or {}
        if result_name in variables:
            return render(variables[result_name], context)
    raise KeyError(result_name)


def services(value):
    out = []
    if isinstance(value, dict):
        if isinstance(value.get("service"), str):
            out.append(value["service"])
        for nested in value.values():
            out.extend(services(nested))
    elif isinstance(value, list):
        for nested in value:
            out.extend(services(nested))
    return out


def dump(value) -> str:
    return yaml.safe_dump(value, allow_unicode=True)


def as_json(value):
    return value if isinstance(value, dict) else json.loads(value)


def find_variable_template(value, name: str, needle: str = ""):
    if isinstance(value, dict):
        variables = value.get("variables")
        if isinstance(variables, dict) and name in variables and needle in str(variables[name]):
            return variables[name]
        for nested in value.values():
            found = find_variable_template(nested, name, needle)
            if found is not None:
                return found
    elif isinstance(value, list):
        for nested in value:
            found = find_variable_template(nested, name, needle)
            if found is not None:
                return found
    return None


class RestTransportTests(unittest.TestCase):
    def test_connect_command_uses_secret_bearer_and_rpc_path(self):
        command = REST["scheduler_rpc"]
        self.assertEqual(command["method"], "POST")
        self.assertIn("/ricardo.v1.SchedulerService/{{ method }}", command["url"])
        self.assertIn("input_text.scheduler_endpoint", command["url"])
        self.assertEqual(command["headers"]["Authorization"], "__SECRET__")
        self.assertIn("payload_json", command["payload"])
        self.assertEqual(command["timeout"], 10)

    def test_no_endpoint_or_token_is_embedded_in_scripts_or_speech(self):
        text = dump(SCRIPTS)
        self.assertNotIn("scheduler_bearer", text)
        self.assertNotIn("Bearer ", text)
        self.assertIn("input_text.scheduler_endpoint", text)
        self.assertNotIn("http://192.0.2.10:8098/ricardo", text)

    def test_scheduler_endpoint_initial_comes_from_secret(self):
        self.assertEqual(INPUT_TEXT["scheduler_endpoint"]["initial"], "__SECRET__")
        readme = (HA / "README.md").read_text(encoding="utf-8")
        self.assertIn("scheduler_endpoint: \"http://<scheduler-node>:30098\"", readme)

    def test_endpoint_and_active_occurrence_helpers_are_declared(self):
        for name in (
            "scheduler_endpoint",
            "scheduler_active_schedule_id",
            "scheduler_active_event_id",
            "scheduler_active_label",
            "scheduler_active_kind",
        ):
            self.assertIn(name, INPUT_TEXT)

    def test_rpc_helper_normalizes_success_and_connect_error(self):
        success = render_first_variables(
            SCRIPTS["scheduler_rpc"],
            "rpc_parsed",
            {
                **BASE_CONTEXT,
                "rpc_http": {
                    "status": 200,
                    "content": '{"schedules":[{"id":"s1","label":"Работа","nextDueUnix":"1761700000"}]}',
                },
            },
        )
        self.assertIsInstance(success["rpc_parsed"], dict)
        for step in SCRIPTS["scheduler_rpc"]["sequence"]:
            variables = step.get("variables") or {}
            for name, value in variables.items():
                if name in {"rpc_code", "rpc_message", "rpc_ok", "rpc_error_text", "rpc_result"}:
                    success[name] = render(value, success)
        self.assertEqual(success["rpc_result"]["status"], "ok")
        self.assertEqual(success["rpc_result"]["data"]["schedules"][0]["id"], "s1")

        failure = render_first_variables(
            SCRIPTS["scheduler_rpc"],
            "rpc_parsed",
            {
                **BASE_CONTEXT,
                "rpc_http": {
                    "status": 401,
                    "content": '{"code":"unauthenticated","message":"bad token"}',
                },
            },
        )
        for step in SCRIPTS["scheduler_rpc"]["sequence"]:
            variables = step.get("variables") or {}
            for name, value in variables.items():
                if name in {"rpc_code", "rpc_message", "rpc_ok", "rpc_error_text", "rpc_result"}:
                    failure[name] = render(value, failure)
        self.assertEqual(failure["rpc_result"]["status"], "error")
        self.assertEqual(failure["rpc_result"]["error_text"], "Планировщик отклонил доступ.")
        self.assertNotIn("bad token", failure["rpc_result"]["error_text"])

    def test_empty_success_response_is_an_empty_list_not_an_error(self):
        context = {**BASE_CONTEXT, "rpc_http": {"status": 200, "content": "{}"}}
        rendered = render_first_variables(SCRIPTS["scheduler_rpc"], "rpc_parsed", context)
        for step in SCRIPTS["scheduler_rpc"]["sequence"]:
            variables = step.get("variables") or {}
            for name, value in variables.items():
                if name in {"rpc_code", "rpc_message", "rpc_ok", "rpc_error_text", "rpc_result"}:
                    rendered[name] = render(value, rendered)
        self.assertEqual(rendered["rpc_result"]["status"], "ok")
        self.assertEqual(rendered["rpc_result"]["data"], {})

    def test_malformed_truncated_array_and_empty_bodies_are_protocol_errors(self):
        for content in ("{bad", '{"schedules": [', "[]", "", "   ", "<html>ok</html>"):
            with self.subTest(content=content):
                context = {**BASE_CONTEXT, "rpc_http": {"status": 200, "content": content}}
                rendered = render_first_variables(SCRIPTS["scheduler_rpc"], "rpc_parsed", context)
                for step in SCRIPTS["scheduler_rpc"]["sequence"]:
                    variables = step.get("variables") or {}
                    for name, value in variables.items():
                        if name in {"rpc_code", "rpc_message", "rpc_ok", "rpc_error_text", "rpc_result"}:
                            rendered[name] = render(value, rendered)
                self.assertEqual(rendered["rpc_result"]["status"], "error")
                self.assertIn("некорректный ответ", rendered["rpc_result"]["error_text"])
                self.assertEqual(rendered["rpc_result"]["data"], {})


class RequestContractTests(unittest.TestCase):
    def _alarm_request(self, **fields):
        context = {**BASE_CONTEXT, **fields}
        context = render_first_variables(SCRIPTS["schedule_alarm_create"], "hour_num", context)
        context = render_first_variables(SCRIPTS["schedule_alarm_create"], "create_request_json", context)
        return as_json(context["create_request_json"])

    def test_weekday_alarm_preserves_wall_time_timezone_and_repeat(self):
        request = self._alarm_request(
            hour="7", minute="30", period="", day="", repeat="По будням", name="Работа"
        )
        self.assertEqual(request["kind"], "SCHEDULE_KIND_ALARM")
        self.assertEqual(request["repeat"], "REPEAT_KIND_WEEKDAYS")
        self.assertEqual(request["timezone"], "Asia/Almaty")
        self.assertEqual(request["at"], {"hour": 7, "minute": 30, "second": 0})
        self.assertEqual(request["onceDate"], "")
        self.assertEqual(request["label"], "Работа")
        self.assertTrue(request["idempotencyKey"].startswith("ha:alarm:"))
        self.assertIs(request["enabled"], True)

    def test_daily_alarm_uses_daily_repeat(self):
        request = self._alarm_request(
            hour="6", minute="15", period="утра", day="", repeat="Каждый день", name=""
        )
        self.assertEqual(request["repeat"], "REPEAT_KIND_DAILY")
        self.assertEqual(request["onceDate"], "")

    def test_once_alarm_in_the_past_rolls_to_tomorrow(self):
        request = self._alarm_request(
            hour="5", minute="0", period="", day="", repeat="Однократно", name=""
        )
        self.assertEqual(request["repeat"], "REPEAT_KIND_ONCE")
        self.assertEqual(request["onceDate"], "2026-10-30")

    def test_once_alarm_tomorrow_keeps_requested_date(self):
        request = self._alarm_request(
            hour="7", minute="30", period="", day="завтра", repeat="Однократно", name=""
        )
        self.assertEqual(request["onceDate"], "2026-10-30")

    def test_period_afternoon_converts_to_24_hour_clock(self):
        request = self._alarm_request(
            hour="8", minute="0", period="вечера", day="", repeat="Однократно", name=""
        )
        self.assertEqual(request["at"]["hour"], 20)

    def test_reminder_is_once_only(self):
        context = {**BASE_CONTEXT, "text": "проверить духовку", "hour": "9", "minute": "0", "period": "", "day": "завтра"}
        context = render_first_variables(SCRIPTS["schedule_reminder_create"], "reminder_hour_num", context)
        context = render_first_variables(SCRIPTS["schedule_reminder_create"], "reminder_request_json", context)
        request = as_json(context["reminder_request_json"])
        self.assertEqual(request["kind"], "SCHEDULE_KIND_REMINDER")
        self.assertEqual(request["repeat"], "REPEAT_KIND_ONCE")
        self.assertEqual(request["onceDate"], "2026-10-30")
        self.assertEqual(request["label"], "проверить духовку")

    def test_reminder_tomorrow_boundary_uses_tomorrow_date(self):
        early = datetime(2026, 10, 29, 5, 0, 0, tzinfo=ZoneInfo("Asia/Almaty"))
        context = {
            **BASE_CONTEXT,
            "text": "тестнапоминание проверить духовку",
            "hour": "9",
            "minute": "0",
            "period": "",
            "day": "завтра",
            "now": lambda: early,
        }
        context = render_first_variables(SCRIPTS["schedule_reminder_create"], "reminder_hour_num", context)
        context = render_first_variables(SCRIPTS["schedule_reminder_create"], "reminder_request_json", context)
        request = as_json(context["reminder_request_json"])
        self.assertEqual(request["onceDate"], "2026-10-30")
        self.assertEqual(request["at"]["hour"], 9)

    def test_reminder_tomorrow_sentence_captures_day_before_generic(self):
        intents = yaml.safe_load(
            (HA / "custom_sentences" / "ru" / "station_schedules.yaml").read_text()
        )["intents"]
        phrases = [
            phrase
            for item in intents["StationReminderCreate"]["data"]
            for phrase in item["sentences"]
        ]
        specific = "напомни {text} [на] {alarm_day:day} в {alarm_hour:hour}:{alarm_minute:minute} [{alarm_period:period}]"
        generic = "напомни {text} в {alarm_hour:hour}:{alarm_minute:minute} [{alarm_period:period}] [на {alarm_day:day}]"
        self.assertIn(specific, phrases)
        self.assertLess(phrases.index(specific), phrases.index(generic))

    def test_timer_request_uses_int64_string_and_idempotency(self):
        context = {
            **BASE_CONTEXT,
            "hours": "0",
            "minutes": "10",
            "seconds": "0",
            "name": "пицца",
        }
        context = render_first_variables(SCRIPTS["schedule_timer_start"], "timer_total_seconds", context)
        context = render_first_variables(SCRIPTS["schedule_timer_start"], "timer_request_json", context)
        request = as_json(context["timer_request_json"])
        self.assertEqual(request["label"], "пицца")
        self.assertEqual(request["durationSeconds"], "600")
        self.assertIsInstance(request["durationSeconds"], str)
        self.assertTrue(request["idempotencyKey"].startswith("ha:timer:"))

    def test_timer_name_slot_maps_to_intent_name_and_request_label(self):
        phrases = [
            phrase
            for item in yaml.safe_load(
                (HA / "custom_sentences" / "ru" / "station_schedules.yaml").read_text()
            )["intents"]["StationTimerStart"]["data"]
            for phrase in item["sentences"]
        ]
        self.assertIn(
            "поставь таймер {timer_name:name} на {timer_minutes:minutes} минут[у|ы]",
            phrases,
        )
        self.assertFalse(any("{timer_name}" in phrase for phrase in phrases))
        context = {
            **BASE_CONTEXT,
            "hours": "0",
            "minutes": "30",
            "seconds": "0",
            "name": "тесттаймер",
        }
        context = render_first_variables(SCRIPTS["schedule_timer_start"], "timer_total_seconds", context)
        context = render_first_variables(SCRIPTS["schedule_timer_start"], "timer_request_json", context)
        self.assertEqual(as_json(context["timer_request_json"])["label"], "тесттаймер")

    def _key_template(self, script_name, field):
        for step in SCRIPTS[script_name]["sequence"]:
            variables = step.get("variables") or {}
            if field in variables:
                return variables[field]
        raise KeyError(field)

    def test_create_does_not_preempt_an_existing_ringing_alarm(self):
        text = dump(SCRIPTS["schedule_alarm_create"])
        self.assertNotIn("script.alarm_stop", text)
        self.assertNotIn("script.schedule_alarm_stop", text)

    def test_no_template_uses_a_range_above_ha_sandbox_limit(self):
        # HA's Jinja sandbox rejects range() sizes above MAX_RANGE (100000).
        for value in re.findall(r"range\(\s*0\s*,\s*(\d+)\s*\)", dump(SCRIPTS)):
            self.assertLessEqual(int(value), 100000)

    def test_idempotency_fallback_is_unique_per_invocation(self):
        # HA documents that `context` is unavailable in a script's variables
        # section, so the key is captured there with wall-clock microseconds plus
        # a random suffix instead of relying on context.id.
        cases = (
            ("schedule_alarm_create", "alarm_idempotency_key", "ha:alarm:"),
            ("schedule_timer_start", "timer_idempotency_key", "ha:timer:"),
            ("schedule_reminder_create", "reminder_idempotency_key", "ha:reminder:"),
        )
        for script_name, field, prefix in cases:
            with self.subTest(script=script_name):
                template = self._key_template(script_name, field)
                self.assertNotIn("context.id", template)
                same_second = {**BASE_CONTEXT}
                same_second.pop("context", None)
                same_second["now"] = lambda: NOW
                first = render(template, same_second)
                second = render(template, same_second)
                self.assertTrue(first.startswith(prefix))
                self.assertTrue(second.startswith(prefix))
                self.assertIn("20261029T060000000000", first)
                self.assertNotEqual(first, second)


class SpeechTruthTests(unittest.TestCase):
    def _render_result(self, script_name, result_name, context):
        for step in SCRIPTS[script_name]["sequence"]:
            variables = step.get("variables") or {}
            if result_name in variables:
                return render(variables[result_name], context)
        raise KeyError(result_name)

    @staticmethod
    def _ts(day_offset=0, hour=7, minute=30):
        d = (NOW + timedelta(days=day_offset)).replace(hour=hour, minute=minute, second=0, microsecond=0)
        return str(int(d.timestamp()))

    def _created_speech(self, schedule, alarm_label="Работа"):
        context = {
            **BASE_CONTEXT,
            "create_rpc": {"status": "ok", "code": "", "error_text": "", "data": {"schedule": schedule}},
            "alarm_label": alarm_label,
        }
        context = render_variables(
            SCRIPTS["schedule_alarm_create"], "created_due_hm", context
        )
        return render_result("schedule_alarm_create", "create_result", context)

    def test_alarm_success_speech_reports_created_schedule(self):
        schedule = {
            "id": "s1",
            "label": "Работа",
            "nextDueUnix": ts_at(1, 7, 30),
            "repeat": "REPEAT_KIND_WEEKDAYS",
        }
        result = self._created_speech(schedule)
        self.assertIsInstance(result, dict)
        self.assertEqual(result["status"], "ok")
        self.assertIn("Готово.", result["text"])
        self.assertIn("«Работа»", result["text"])
        self.assertIn("07:30", result["text"])
        self.assertIn("по будням", result["text"])
        self.assertNotIn("29.10", result["text"])
        self.assertNotIn("Будильник «", result["text"])
        self.assertNotIn("Не удалось", result["text"])

    def test_auto_label_is_not_repeated(self):
        schedule = {
            "id": "s2",
            "label": "Будильник 10:52",
            "nextDueUnix": ts_at(0, 10, 52),
            "repeat": "REPEAT_KIND_ONCE",
        }
        text = self._created_speech(schedule, alarm_label="Будильник 10:52")["text"]
        self.assertEqual(text, "Готово. Будильник на 10:52.")
        self.assertNotIn("«", text)

    def test_custom_label_starting_with_budilnik_is_preserved(self):
        schedule = {
            "id": "s3",
            "label": "Будильник люкс",
            "nextDueUnix": ts_at(0, 10, 52),
            "repeat": "REPEAT_KIND_ONCE",
        }
        text = self._created_speech(schedule, alarm_label="Будильник люкс")["text"]
        self.assertIn("«Будильник люкс»", text)

    def test_tomorrow_and_far_dates_are_short_and_truthful(self):
        tomorrow = (NOW.date() + timedelta(days=1)).strftime("%Y-%m-%d")
        schedule = {
            "id": "s4",
            "label": "Будильник 09:00",
            "nextDueUnix": ts_at(1, 9, 0),
            "onceDate": tomorrow,
            "repeat": "REPEAT_KIND_ONCE",
        }
        self.assertIn("завтра, 09:00", self._created_speech(schedule, "Будильник 09:00")["text"])
        far = (NOW.date() + timedelta(days=2)).strftime("%Y-%m-%d")
        schedule["onceDate"] = far
        text = self._created_speech(schedule, "Будильник 09:00")["text"]
        self.assertIn(f"{far[8:10]}.{far[5:7]}, 09:00", text)
        self.assertNotIn("завтра", text)

    def test_alarm_failure_speech_never_claims_success(self):
        context = {
            **BASE_CONTEXT,
            "create_rpc": {
                "status": "error",
                "code": "unauthenticated",
                "error_text": "Планировщик отклонил доступ.",
                "data": {},
            },
            "alarm_label": "Работа",
        }
        result = render_result("schedule_alarm_create", "create_result", context)
        self.assertEqual(result["status"], "error")
        self.assertEqual(result["text"], "Планировщик отклонил доступ.")
        self.assertNotIn("установлен", result["text"])

    def test_empty_200_create_response_is_not_reported_as_success(self):
        context = {
            **BASE_CONTEXT,
            "create_rpc": {"status": "ok", "code": "", "error_text": "", "data": {}},
            "created_schedule": {},
            "created_repeat_text": "",
            "created_due_text": "",
            "alarm_label": "Работа",
        }
        result = render_result("schedule_alarm_create", "create_result", context)
        self.assertEqual(result["status"], "error")
        self.assertIn("пустой ответ", result["text"])
        self.assertNotIn("установлен", result["text"])

    def test_timer_failure_speech_never_claims_timer_started(self):
        context = {
            **BASE_CONTEXT,
            "timer_start_rpc": {
                "status": "error",
                "code": "unavailable",
                "error_text": "Планировщик не отвечает.",
                "data": {},
            },
            "created_timer": {},
            "timer_label": "пицца",
            "timer_duration": "600",
            "timer_duration_text": "10 мин",
        }
        result = render_result("schedule_timer_start", "timer_result", context)
        self.assertEqual(result["status"], "error")
        self.assertNotIn("установлен", result["text"])

    def test_stop_and_snooze_use_scheduler_scripts(self):
        stop = dump(INTENTS["StationAlarmStop"])
        self.assertIn("script.schedule_alarm_stop", stop)
        self.assertIn("media_player.media_stop", stop)
        snooze = dump(INTENTS["StationAlarmSnooze"])
        self.assertIn("script.schedule_alarm_snooze", snooze)
        for name in ("schedule_alarm_stop", "schedule_alarm_snooze"):
            self.assertIn(
                "script.scheduler_confirm_ring_stopped",
                dump(SCRIPTS[name]),
                name,
            )
        self.assertIn("SnoozeSchedule", dump(SCRIPTS["schedule_alarm_snooze"]))
        self.assertIn("StopSchedule", dump(SCRIPTS["schedule_alarm_stop"]))
        self.assertNotIn("script.alarm_snooze", dump(INTENTS["StationAlarmSnooze"]))
        self.assertNotIn("script.alarm_snooze", dump(SCRIPTS["schedule_alarm_snooze"]))
        confirm = dump(SCRIPTS["scheduler_confirm_ring_stopped"])
        self.assertIn("wait_template", confirm)
        self.assertIn("script.alarm_stop", confirm)
        self.assertIn("confirm_same_source", confirm)

    def test_snooze_lost_response_does_not_claim_repeat_was_not_scheduled(self):
        template = find_variable_template(
            SCRIPTS["schedule_alarm_snooze"],
            "snooze_result",
            "Звонок остановлен локально. Перенос",
        )
        self.assertIsNotNone(template)
        result = render(
            template,
            {
                **BASE_CONTEXT,
                "snooze_outcome": "local",
                "snooze_rpc": {
                    "status": "error",
                    "code": "unavailable",
                    "error_text": "Планировщик не отвечает.",
                },
            },
        )
        self.assertEqual(result["status"], "error")
        self.assertIn("Звонок остановлен локально", result["text"])
        self.assertIn("Перенос не подтверждён", result["text"])
        self.assertNotIn("не запланирован", result["text"])

        no_local = render(
            template,
            {
                **BASE_CONTEXT,
                "snooze_outcome": "executor",
                "snooze_rpc": {
                    "status": "error",
                    "code": "unavailable",
                    "error_text": "Планировщик не отвечает.",
                },
            },
        )
        self.assertIn("Перенос не подтверждён", no_local["text"])
        self.assertNotIn("Звонок остановлен", no_local["text"])

    def test_snooze_success_with_unconfirmed_executor_reports_local_stop(self):
        template = find_variable_template(
            SCRIPTS["schedule_alarm_snooze"],
            "snooze_result",
            "Будильник отложен. Звонок остановлен локально",
        )
        self.assertIsNotNone(template)
        result = render(
            template,
            {
                **BASE_CONTEXT,
                "snooze_outcome": "local",
                "snooze_rpc": {"status": "ok", "code": "", "error_text": ""},
            },
        )
        self.assertEqual(result["status"], "ok")
        self.assertIn("Будильник отложен", result["text"])
        self.assertIn("Звонок остановлен локально", result["text"])

    def test_stop_captures_ringing_before_api_and_survives_state_change(self):
        actions = INTENTS["StationAlarmStop"]["action"]
        capture = actions[0]["variables"]["was_ringing"]
        self.assertEqual(actions[1]["service"], "script.schedule_alarm_stop")
        was_ringing = render(
            capture,
            {
                **BASE_CONTEXT,
                "is_state": lambda entity_id, state: entity_id == "input_boolean.alarm_ringing"
                and state == "on",
            },
        )
        self.assertIs(was_ringing, True)

        choose = next(action["choose"] for action in actions if "choose" in action)
        music_branch = choose[0]["conditions"][0]["value_template"]
        self.assertIn("not was_ringing", music_branch)
        self.assertIn("not_found", music_branch)

        # The bridge may stop the alarm and resume music while StopSchedule is
        # still in flight. Captured invocation state must prevent music fallback.
        after_fast_stop = {
            **BASE_CONTEXT,
            "was_ringing": was_ringing,
            "was_music_playing": True,
            "stop_result": {"status": "ok"},
            "is_state": lambda entity_id, state: False,
        }
        self.assertFalse(render(music_branch, after_fast_stop))

        # A command that started with no ringing alarm may stop music only when
        # the canonical API reports that no active/snoozed alarm exists.
        after_music_only = {
            **BASE_CONTEXT,
            "was_ringing": False,
            "was_music_playing": True,
            "stop_result": {"status": "not_found"},
            "is_state": lambda entity_id, state: False,
        }
        self.assertTrue(render(music_branch, after_music_only))
        station_result = next(
            action["variables"]["station_stop_result"]
            for action in actions
            if "variables" in action and "station_stop_result" in action["variables"]
        )
        self.assertEqual(render(station_result, after_music_only)["text"], "Останавливаю музыку.")

    def test_transport_timeout_uses_same_source_guard_and_acknowledges_local_stop(self):
        script = dump(SCRIPTS["schedule_alarm_stop"])
        self.assertIn("scheduler_confirm_ring_stopped", script)
        self.assertNotIn("stop_rpc.http_status", script)
        template = find_variable_template(
            SCRIPTS["schedule_alarm_stop"], "stop_result", "исполнитель не подтвердил"
        )
        self.assertIsNotNone(template)
        local = render(
            template,
            {
                **BASE_CONTEXT,
                "stop_rpc": {"status": "error", "http_status": 0, "error_text": "Планировщик недоступен."},
                "stop_outcome": "local",
            },
        )
        self.assertIn("Звонок остановлен локально", local["text"])
        self.assertNotIn("не подтверждён", local["text"])

        # A newer alarm B started while A's network call timed out: the helper
        # reports "other", so no local stop and no false acknowledgment.
        other = render(
            template,
            {
                **BASE_CONTEXT,
                "stop_rpc": {"status": "error", "http_status": 0, "error_text": "Планировщик недоступен."},
                "stop_outcome": "other",
            },
        )
        self.assertIn("другой звонок", other["text"])
        self.assertNotIn("Звонок остановлен локально", other["text"])

    def test_transport_timeout_marker_change_does_not_silence_newer_alarm(self):
        same_source = find_variable_template(
            SCRIPTS["scheduler_confirm_ring_stopped"], "confirm_same_source"
        )
        self.assertFalse(
            render(
                same_source,
                {
                    **BASE_CONTEXT,
                    "confirm_schedule_id": "A",
                    "confirm_event_id": "eA",
                    "is_state": lambda entity_id, state: entity_id == "input_boolean.alarm_ringing"
                    and state == "on",
                    "states": lambda entity_id: {
                        "input_text.scheduler_active_schedule_id": "B",
                        "input_text.scheduler_active_event_id": "eB",
                    }.get(entity_id, ""),
                },
            )
        )

    def test_transport_down_snooze_uses_same_source_outcome_for_speech(self):
        script = dump(SCRIPTS["schedule_alarm_snooze"])
        self.assertIn("scheduler_confirm_ring_stopped", script)
        self.assertNotIn("snooze_rpc.http_status", script)
        template = find_variable_template(
            SCRIPTS["schedule_alarm_snooze"], "snooze_result", "Будильник отложен"
        )
        self.assertIsNotNone(template)
        local = render(
            template,
            {
                **BASE_CONTEXT,
                "snooze_rpc": {"status": "error", "http_status": 0, "error_text": "Планировщик недоступен."},
                "snooze_outcome": "local",
            },
        )
        self.assertIn("Звонок остановлен локально", local["text"])
        self.assertIn("Перенос не подтверждён", local["text"])
        other = render(
            template,
            {
                **BASE_CONTEXT,
                "snooze_rpc": {"status": "error", "http_status": 0, "error_text": "Планировщик недоступен."},
                "snooze_outcome": "other",
            },
        )
        self.assertIn("другой звонок", other["text"])
        self.assertNotIn("Звонок остановлен локально", other["text"])

    def test_stop_success_but_ringing_stays_on_uses_same_source_local_stop(self):
        confirm = SCRIPTS["scheduler_confirm_ring_stopped"]
        confirm_dump = dump(confirm)
        self.assertIn("wait_template", confirm_dump)
        self.assertIn("script.alarm_stop", confirm_dump)
        same_source = find_variable_template(confirm, "confirm_same_source")
        self.assertIsNotNone(same_source)
        self.assertTrue(
            render(
                same_source,
                {
                    **BASE_CONTEXT,
                    "confirm_schedule_id": "s1",
                    "confirm_event_id": "e1",
                    "is_state": lambda entity_id, state: entity_id == "input_boolean.alarm_ringing"
                    and state == "on",
                    "states": lambda entity_id: {
                        "input_text.scheduler_active_schedule_id": "s1",
                        "input_text.scheduler_active_event_id": "e1",
                    }.get(entity_id, ""),
                },
            )
        )
        self.assertFalse(
            render(
                same_source,
                {
                    **BASE_CONTEXT,
                    "confirm_schedule_id": "s1",
                    "confirm_event_id": "e1",
                    "is_state": lambda entity_id, state: entity_id == "input_boolean.alarm_ringing"
                    and state == "on",
                    "states": lambda entity_id: {
                        "input_text.scheduler_active_schedule_id": "s2",
                        "input_text.scheduler_active_event_id": "e2",
                    }.get(entity_id, ""),
                },
            )
        )
        result_template = find_variable_template(
            SCRIPTS["schedule_alarm_stop"], "stop_result", "исполнитель не подтвердил"
        )
        self.assertIsNotNone(result_template)
        result = render(
            result_template,
            {
                **BASE_CONTEXT,
                "stop_rpc": {"status": "ok", "code": "", "error_text": ""},
                "stop_outcome": "local",
            },
        )
        self.assertEqual(result["status"], "ok")
        self.assertIn("Звонок остановлен локально", result["text"])
        self.assertNotIn("Останавливаю.", result["text"])


class RelativeAlarmTemplateTests(unittest.TestCase):
    """Regression: HA script variables are rendered to strings.

    `relative_due: "{{ now() + timedelta(...) }}"` becomes a string in a real
    HA variables step, so calling `.date()` on it raised
    "UndefinedError: 'str object' has no attribute 'date'" and the live
    StationAlarmRelative intent failed. The fix derives fields from one epoch
    integer; these tests render with a non-native environment to reproduce the
    stringification.
    """

    @staticmethod
    def _ha_value(value):
        # HA keeps JSON-ish native types but renders a datetime to str; that is
        # why the old `.date()` on a stored datetime variable failed live.
        if isinstance(value, datetime):
            return str(value)
        return value

    def _ha_variables(self, hours: int, minutes: int, now=None) -> dict:
        now_value = now or NOW
        context = {
            "now": lambda: now_value,
            "timedelta": timedelta,
            "hours": hours,
            "minutes": minutes,
        }
        rendered = dict(context)
        step = SCRIPTS["schedule_alarm_create_relative"]["sequence"][0]
        for name, value in step["variables"].items():
            result = render(value, rendered) if isinstance(value, str) else value
            rendered[name] = self._ha_value(result)
        return rendered

    def _service_data(self, variables: dict) -> dict:
        step = SCRIPTS["schedule_alarm_create_relative"]["sequence"][1]
        data = step["data"]
        return {
            key: render(data[key], variables)
            for key in ("hour", "minute", "second", "day")
        }

    def test_variables_are_strings_not_datetimes(self) -> None:
        variables = self._ha_variables(1, 30)
        self.assertIsInstance(variables["relative_day"], str)
        self.assertNotIn("relative_due", variables)
        self.assertNotIn(".date()", json.dumps(
            SCRIPTS["schedule_alarm_create_relative"], ensure_ascii=False
        ))

    def test_same_day_minutes(self) -> None:
        data = self._service_data(self._ha_variables(1, 30))
        self.assertEqual(
            (int(data["hour"]), int(data["minute"]), int(data["second"]), data["day"]),
            (7, 30, 0, ""),
        )

    def test_crossing_midnight_sets_tomorrow(self) -> None:
        data = self._service_data(self._ha_variables(20, 0))
        self.assertEqual(
            (int(data["hour"]), int(data["minute"]), data["day"]), (2, 0, "завтра")
        )

    def test_minimum_one_minute(self) -> None:
        data = self._service_data(self._ha_variables(0, 0))
        self.assertEqual((int(data["hour"]), int(data["minute"])), (6, 1))

    def test_seconds_are_preserved(self) -> None:
        # Server now 12:34:45 + 5 min must target 12:39:45, not 12:39:00.
        now = NOW.replace(hour=12, minute=34, second=45)
        data = self._service_data(self._ha_variables(0, 5, now=now))
        self.assertEqual(
            (int(data["hour"]), int(data["minute"]), int(data["second"]), data["day"]),
            (12, 39, 45, ""),
        )

    def test_canonical_payload_uses_second(self) -> None:
        payload = json.dumps(SCRIPTS["schedule_alarm_create"], ensure_ascii=False)
        self.assertIn("'second': second_num | int", payload)

    def test_second_field_is_owned_by_canonical_create(self) -> None:
        create_fields = SCRIPTS["schedule_alarm_create"]["fields"]
        self.assertIn("second", create_fields)
        self.assertEqual(create_fields["second"].get("default"), 0)

    def test_relative_parser_cannot_exceed_24h(self) -> None:
        # The relative intent slots reuse the alarm_hour/alarm_minute lists, so
        # the maximum is 23h59m and the target can only be today or tomorrow.
        sentence_doc = yaml.safe_load(
            (HA / "custom_sentences" / "ru" / "station_alarm.yaml").read_text(encoding="utf-8")
        )
        hours = [int(v["out"]) for v in sentence_doc["lists"]["alarm_hour"]["values"]]
        minutes = [int(v["out"]) for v in sentence_doc["lists"]["alarm_minute"]["values"]]
        self.assertEqual(max(hours), 23)
        self.assertEqual(max(minutes), 59)
        self.assertLess(max(hours) * 60 + max(minutes), 24 * 60)


class MediaUrlReachabilityTests(unittest.TestCase):
    """Live incident: the ringtone/chime URL used the sanitized TEST-NET host.

    `http://192.0.2.28:8123/local/...` is unreachable from HA, so the ESPHome
    ffmpeg proxy timed out and the alarm stayed silent even though the service
    call returned 2xx and the bridge ACKed DELIVERED. Local media must be
    fetched from the HA loopback address, never a documentation placeholder.
    """

    def test_local_media_uses_reachable_base(self) -> None:
        text = (HA / "scripts.yaml").read_text(encoding="utf-8")
        urls = re.findall(r"media_content_id:\s*\"([^\"]+)\"", text)
        media = [u for u in urls if "/local/alarm_" in u]
        self.assertGreaterEqual(len(media), 3)
        for url in media:
            self.assertTrue(
                url.startswith("http://127.0.0.1:8123/local/"),
                f"unreachable media URL: {url}",
            )

    def test_no_test_net_in_media_urls(self) -> None:
        text = (HA / "scripts.yaml").read_text(encoding="utf-8")
        self.assertNotIn("192.0.2.", text)

    def test_station_control_base_is_not_test_net(self) -> None:
        # The same copy step had also replaced the real station control URL with
        # a TEST-NET placeholder, breaking gateway/bluetooth HTTP commands.
        rest = (HA / "rest_commands.yaml").read_text(encoding="utf-8")
        self.assertNotIn("192.0.2.", rest)
        for path in ("gateway/on", "gateway/off", "bluetooth/on", "bluetooth/off"):
            self.assertIn(
                f"states('input_text.station_base_url')"
                f" | trim | regex_replace('/+$', '') }}}}/{path}",
                rest,
            )
        inputs = (HA / "input_text.yaml").read_text(encoding="utf-8")
        self.assertIn("station_base_url:", inputs)
        self.assertIn("initial: !secret station_base_url", inputs)


class SchedulerVerificationTests(unittest.TestCase):
    def test_update_speech_includes_repeat(self):
        schedule = {
            "id": "s1",
            "label": "Работа",
            "nextDueUnix": ts_at(0, 8, 45),
            "repeat": "REPEAT_KIND_WEEKDAYS",
        }
        context = {
            **BASE_CONTEXT,
            "update_rpc": {
                "status": "ok",
                "code": "",
                "error_text": "",
                "data": {"schedule": schedule},
            },
        }
        context = render_variables(
            SCRIPTS["schedule_alarm_update"], "updated_due_hm", context
        )
        result = render_result("schedule_alarm_update", "update_result", context)
        self.assertEqual(result["status"], "ok")
        self.assertIn("08:45", result["text"])
        self.assertIn("по будням", result["text"])
        self.assertIn("Готово.", result["text"])

    def test_disabled_alarm_stays_disabled_when_protojson_omits_enabled(self):
        template = find_variable_template(
            SCRIPTS["schedule_alarm_update"], "update_request_json"
        )
        self.assertIsNotNone(template)
        disabled_schedule = {
            "id": "s1",
            "label": "Работа",
            "kind": "SCHEDULE_KIND_ALARM",
            "timezone": "Asia/Almaty",
            "at": {"hour": 7, "minute": 30, "second": 0},
            "repeat": "REPEAT_KIND_DAILY",
            "weekdays": [],
            "payloadJson": "{}",
            "revision": "4",
            # enabled: false is omitted by protojson
        }
        request = as_json(
            render(
                template,
                {
                    **BASE_CONTEXT,
                    "update_target": {"status": "ok", "schedule": disabled_schedule},
                    "update_adjusted_hour": 8,
                    "update_once_date": "2026-10-30",
                    "minute": "45",
                },
            )
        )
        self.assertIs(request["schedule"]["enabled"], False)

    def _set_all_result(self, total, matched_states, flag=True):
        template = find_variable_template(
            SCRIPTS["schedule_alarm_set_enabled"], "set_all_result", "Изменено"
        )
        self.assertIsNotNone(template)
        rows = [{"id": f"s{i}"} for i in range(total)]
        after = [
            {"id": f"s{i}", "enabled": enabled}
            for i, enabled in enumerate(matched_states)
        ]
        context = {
            **BASE_CONTEXT,
            "set_all_rows": rows,
            "set_all_total": total,
            "set_enabled_flag": flag,
            "set_after_list": {"status": "ok", "schedules": after},
        }
        matched_template = find_variable_template(
            SCRIPTS["schedule_alarm_set_enabled"], "set_all_matched"
        )
        context["set_all_matched"] = render(matched_template, context)
        return render(template, context)

    def test_enable_all_success_is_not_inverted(self):
        result = self._set_all_result(2, [True, True], flag=True)
        self.assertEqual(result["status"], "ok")
        self.assertIn("Все будильники включены", result["text"])

    def test_enable_all_total_failure_is_not_false_success(self):
        result = self._set_all_result(2, [False, False], flag=True)
        self.assertEqual(result["status"], "error")
        self.assertIn("Изменено 0 из 2", result["text"])

    def test_enable_all_partial_failure_reports_real_count(self):
        result = self._set_all_result(3, [True, False, False], flag=True)
        self.assertEqual(result["status"], "error")
        self.assertIn("Изменено 1 из 3", result["text"])
        self.assertIn("не удалось изменить 2", result["text"])

    def test_disable_all_success_and_partial_failure(self):
        success = self._set_all_result(2, [False, False], flag=False)
        self.assertEqual(success["status"], "ok")
        self.assertIn("Все будильники выключены", success["text"])
        partial = self._set_all_result(2, [False, True], flag=False)
        self.assertEqual(partial["status"], "error")
        self.assertIn("Изменено 1 из 2", partial["text"])


class ResolutionTests(unittest.TestCase):
    def _resolve(self, query, schedules, kind="SCHEDULE_KIND_ALARM"):
        context = {
            **BASE_CONTEXT,
            "query": query,
            "kind": kind,
            "include_disabled": True,
            "resolve_list": {"status": "ok", "schedules": schedules, "code": "", "error_text": ""},
        }
        for step in SCRIPTS["scheduler_resolve_schedule"]["sequence"]:
            variables = step.get("variables") or {}
            for name, value in variables.items():
                context[name] = render(value, context) if isinstance(value, str) else value
        return context

    def test_exact_label_wins(self):
        context = self._resolve(
            "Работа",
            [
                {"id": "s1", "label": "Работа", "nextDueUnix": "1761733200"},
                {"id": "s2", "label": "Работа 2", "nextDueUnix": "1761736800"},
            ],
        )
        self.assertEqual(context["resolve_status"], "ok")
        self.assertEqual(context["resolve_result"]["schedule"]["id"], "s1")

    def test_partial_label_with_multiple_matches_is_ambiguous(self):
        context = self._resolve(
            "работ",
            [
                {"id": "s1", "label": "Работа", "nextDueUnix": "1761733200"},
                {"id": "s2", "label": "Работа 2", "nextDueUnix": "1761736800"},
            ],
        )
        self.assertEqual(context["resolve_status"], "ambiguous")
        self.assertEqual(context["resolve_result"]["count"], 2)

    def test_empty_query_with_multiple_schedules_is_not_destructive(self):
        context = self._resolve(
            "",
            [
                {"id": "s1", "label": "Работа", "nextDueUnix": "1761733200"},
                {"id": "s2", "label": "Спорт", "nextDueUnix": "1761736800"},
            ],
        )
        self.assertEqual(context["resolve_status"], "ambiguous")

    def test_time_query_matches_next_due_clock(self):
        context = self._resolve(
            "06:01",
            [{"id": "s1", "label": "Работа", "nextDueUnix": "1761699660"}],
        )
        self.assertEqual(context["resolve_status"], "ok")
        self.assertEqual(context["resolve_result"]["schedule"]["id"], "s1")

    def test_no_match_is_not_guessed(self):
        context = self._resolve("нет такого", [{"id": "s1", "label": "Работа", "nextDueUnix": "1761733200"}])
        self.assertEqual(context["resolve_status"], "not_found")
        self.assertEqual(context["resolve_result"]["schedule"], {})

    def test_reminder_delete_single_and_ambiguous_resolution(self):
        single = self._resolve(
            "духов",
            [{"id": "r1", "label": "проверить духовку", "nextDueUnix": "1761733200"}],
            kind="SCHEDULE_KIND_REMINDER",
        )
        self.assertEqual(single["resolve_status"], "ok")
        self.assertEqual(single["resolve_result"]["schedule"]["id"], "r1")
        ambiguous = self._resolve(
            "тест",
            [
                {"id": "r1", "label": "тестнапоминание один", "nextDueUnix": "1761733200"},
                {"id": "r2", "label": "тестнапоминание два", "nextDueUnix": "1761736800"},
            ],
            kind="SCHEDULE_KIND_REMINDER",
        )
        self.assertEqual(ambiguous["resolve_status"], "ambiguous")
        self.assertEqual(ambiguous["resolve_result"]["count"], 2)

    def test_reminder_delete_all_result_and_partial_failure(self):
        template = find_variable_template(
            SCRIPTS["schedule_reminder_delete"], "reminder_delete_result", "Все напоминания"
        )
        self.assertIsNotNone(template)
        success = render(
            template,
            {
                **BASE_CONTEXT,
                "reminder_delete_after": {"status": "ok", "schedules": []},
                "reminder_delete_remaining": 0,
            },
        )
        self.assertEqual(success["status"], "ok")
        self.assertIn("Все напоминания удалены", success["text"])
        partial = render(
            template,
            {
                **BASE_CONTEXT,
                "reminder_delete_after": {"status": "ok", "schedules": [{"id": "r1"}]},
                "reminder_delete_remaining": 1,
            },
        )
        self.assertEqual(partial["status"], "error")
        self.assertIn("Не удалось удалить 1", partial["text"])

    def test_reminder_delete_all_requires_explicit_all_slot(self):
        script = dump(SCRIPTS["schedule_reminder_delete"])
        self.assertIn("DeleteSchedule", script)
        self.assertIn("reminder_delete_all", script)
        self.assertIn("scheduler_resolve_schedule", script)
        sentences = yaml.safe_load(
            (HA / "custom_sentences" / "ru" / "station_schedules.yaml").read_text()
        )["intents"]["StationReminderDelete"]["data"][0]["sentences"]
        self.assertIn("удали напоминание {query:query}", sentences)
        self.assertIn("удали {reminder_all:all} напоминания", sentences)
        intent = dump(INTENTS["StationReminderDelete"])
        self.assertIn("script.schedule_reminder_delete", intent)
        self.assertIn("== ''все''", intent)


class SingleWriterTests(unittest.TestCase):
    CANONICAL = (
        "schedule_alarm_create",
        "schedule_alarm_status",
        "schedule_alarm_set_enabled",
        "schedule_alarm_delete",
        "schedule_alarm_update",
        "schedule_alarm_stop",
        "schedule_alarm_snooze",
        "schedule_timer_start",
        "schedule_timer_status",
        "schedule_timer_cancel",
        "schedule_timer_pause",
        "schedule_timer_resume",
        "schedule_reminder_create",
        "schedule_reminder_delete",
        "schedule_reminder_status",
    )

    def test_canonical_scripts_do_not_write_legacy_todo_or_timer_helpers(self):
        for name in self.CANONICAL:
            text = dump(SCRIPTS[name])
            self.assertNotIn("todo.add_item", text, name)
            self.assertNotIn("todo.update_item", text, name)
            self.assertNotIn("todo.remove_item", text, name)
            self.assertNotIn("timer.start", text, name)
            self.assertNotIn("timer.pause", text, name)
            self.assertNotIn("timer.cancel", text, name)

    def test_every_canonical_write_calls_a_scheduler_rpc(self):
        for name in self.CANONICAL:
            text = dump(SCRIPTS[name])
            self.assertTrue(
                "script.scheduler_rpc" in text or "script.scheduler_list" in text,
                name,
            )

    def test_notes_remain_home_assistant_todo(self):
        for name in ("schedule_note_add", "schedule_note_search", "schedule_note_delete"):
            self.assertIn("todo.", dump(SCRIPTS[name]))
        dashboard = dump(DASHBOARD)
        self.assertIn("todo.notes", dashboard)
        self.assertNotIn("todo.alarms", dashboard)
        self.assertNotIn("todo.reminders", dashboard)
        self.assertNotIn("timer.taimer_", dashboard)
        self.assertNotIn("alarm_ui_create", SCRIPTS)

    def test_legacy_schedule_automations_are_removed(self):
        automation_ids = {automation["id"] for automation in AUTOMATIONS}
        for legacy in (
            "alarm_scheduler",
            "schedule_alarm_tick",
            "schedule_reminder_tick",
            "schedule_timer_finished",
        ):
            self.assertNotIn(legacy, automation_ids)

    def test_button_stop_uses_the_confirming_scheduler_script(self):
        automation = next(a for a in AUTOMATIONS if a["id"] == "alarm_button_stop")
        self.assertEqual(services(automation), ["script.schedule_alarm_stop"])
        self.assertIn(
            "script.scheduler_confirm_ring_stopped",
            dump(SCRIPTS["schedule_alarm_stop"]),
        )
        self.assertIn(
            "script.alarm_stop",
            dump(SCRIPTS["scheduler_confirm_ring_stopped"]),
        )

    def test_explicit_all_is_a_separate_sentence_slot(self):
        self.assertIn("alarm_all", SENTENCES["lists"])
        cancel_phrases = [
            phrase
            for item in SENTENCES["intents"]["StationAlarmCancel"]["data"]
            for phrase in item["sentences"]
        ]
        delete_phrases = [
            phrase
            for item in SENTENCES["intents"]["StationAlarmDelete"]["data"]
            for phrase in item["sentences"]
        ]
        self.assertTrue(any("{alarm_all:all}" in phrase for phrase in cancel_phrases))
        self.assertTrue(any("{alarm_all:all}" in phrase for phrase in delete_phrases))
        self.assertIn("отмени будильник", cancel_phrases)
        self.assertIn("удали будильник", delete_phrases)


if __name__ == "__main__":
    unittest.main(verbosity=2)
