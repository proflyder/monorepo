from __future__ import annotations

import contextlib
import io
import json
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))

import ha_snapshot  # noqa: E402
import migrate  # noqa: E402

SERVICE = "/ricardo.v1.SchedulerService"
TOKEN = "test-only-token-not-a-secret"


class FakeScheduler(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self) -> None:
        super().__init__(("127.0.0.1", 0), FakeHandler)
        self.requests: list[dict[str, Any]] = []
        self.schedules: dict[str, dict[str, Any]] = {}
        self.timers: dict[str, dict[str, Any]] = {}
        self.idempotency: dict[str, tuple[str, str]] = {}
        self.mutate_label = False
        self.timer_due_drift = 0
        self.timer_remaining_drift = 0
        self.auth_reject_with_token = False
        self.lock = threading.Lock()

    @property
    def endpoint(self) -> str:
        return f"http://127.0.0.1:{self.server_address[1]}"

    def writes(self) -> list[str]:
        return [
            request["method"]
            for request in self.requests
            if request["method"] in {"CreateSchedule", "StartTimer", "PauseTimer"}
        ]


class FakeHandler(BaseHTTPRequestHandler):
    server: FakeScheduler

    def log_message(self, format: str, *args: Any) -> None:  # noqa: A003
        return

    def do_POST(self) -> None:  # noqa: N802
        path = self.path
        method = path.removeprefix(SERVICE + "/")
        try:
            length = int(self.headers.get("Content-Length", "0"))
            body = json.loads(self.rfile.read(length) or b"{}")
        except (ValueError, json.JSONDecodeError):
            self._send(400, {"code": "invalid_argument", "message": "bad json"})
            return
        if not isinstance(body, dict):
            body = {}
        state = self.server
        with state.lock:
            state.requests.append(
                {
                    "method": method,
                    "path": path,
                    "body": body,
                    "authorization": self.headers.get("Authorization"),
                    "content_type": self.headers.get("Content-Type"),
                }
            )

        expected = "Bearer " + TOKEN
        if self.headers.get("Authorization") != expected or state.auth_reject_with_token:
            supplied = self.headers.get("Authorization", "")
            self._send(
                401,
                {
                    "code": "unauthenticated",
                    "message": f"supplied {supplied} rejected",
                },
            )
            return
        if path != SERVICE + "/" + method:
            self._send(404, {"code": "unimplemented", "message": "unknown path"})
            return
        try:
            response = self._handle(method, body)
        except Conflict as exc:
            self._send(409, {"code": "already_exists", "message": str(exc)})
            return
        except LookupError as exc:
            self._send(404, {"code": "not_found", "message": str(exc)})
            return
        self._send(200, response)

    def _handle(self, method: str, body: dict[str, Any]) -> dict[str, Any]:
        state = self.server
        with state.lock:
            if method == "GetStatus":
                return {
                    "ready": True,
                    "version": "fake-1",
                    "schemaVersion": 3,
                    "nowUnix": "1893455000",
                    "enabledSchedules": len(
                        [item for item in state.schedules.values() if item.get("enabled")]
                    ),
                    "runningTimers": len(
                        [item for item in state.timers.values() if item.get("state") == "RUNNING"]
                    ),
                    "pendingEvents": 0,
                    "scenarioExecutionSupported": False,
                }
            if method == "CreateSchedule":
                return {"schedule": self._create_schedule(body)}
            if method == "ListSchedules":
                schedules = []
                for schedule in state.schedules.values():
                    item = dict(schedule)
                    if state.mutate_label:
                        item["label"] = item["label"] + " drift"
                    schedules.append(item)
                return {"schedules": schedules}
            if method == "GetTimer":
                timer = state.timers.get(str(body.get("id")))
                if timer is None:
                    raise LookupError("timer not found")
                return {"timer": dict(timer)}
            if method == "StartTimer":
                return {"timer": self._start_timer(body)}
            if method == "PauseTimer":
                timer = state.timers.get(str(body.get("id")))
                if timer is None:
                    raise LookupError("timer not found")
                if timer["state"] == "RUNNING":
                    timer["state"] = "PAUSED"
                    timer["remainingSeconds"] = timer["durationSeconds"]
                    timer["dueUnix"] = "0"
                return {"timer": dict(timer)}
            if method == "ListTimers":
                timers = []
                for timer in state.timers.values():
                    item = dict(timer)
                    if state.timer_due_drift:
                        item["dueUnix"] = str(
                            int(item["dueUnix"]) + state.timer_due_drift
                        )
                    if state.timer_remaining_drift:
                        item["remainingSeconds"] = str(
                            int(item["remainingSeconds"])
                            + state.timer_remaining_drift
                        )
                    timers.append(item)
                return {"timers": timers}
        raise LookupError("method not found")

    def _create_schedule(self, body: dict[str, Any]) -> dict[str, Any]:
        state = self.server
        key = str(body.get("idempotencyKey") or "")
        canonical = json.dumps(body, sort_keys=True, separators=(",", ":"))
        if key:
            existing = state.idempotency.get(key)
            if existing:
                kind, resource_id = existing
                if kind != "schedule" or resource_id not in state.schedules:
                    raise Conflict("idempotency key was reused")
                stored = state.schedules[resource_id]
                if stored.get("_canonical") != canonical:
                    raise Conflict("idempotency key reused with a different request")
                return {key: value for key, value in stored.items() if key != "_canonical"}
        scheduler_id = f"sched-{len(state.schedules) + 1}"
        schedule = {
            "id": scheduler_id,
            "label": body.get("label", ""),
            "kind": body.get("kind", ""),
            "timezone": body.get("timezone", ""),
            "at": body.get("at", {}),
            "onceDate": body.get("onceDate", ""),
            "repeat": body.get("repeat", ""),
            "weekdays": body.get("weekdays", []),
            "enabled": bool(body.get("enabled")),
            "payloadJson": body.get("payloadJson", "{}"),
            "nextDueUnix": "0" if not body.get("enabled") else "1893456000",
            "createdUnix": "1893455000",
            "updatedUnix": "1893455000",
            "revision": "1",
        }
        state.schedules[scheduler_id] = {**schedule, "_canonical": canonical}
        if key:
            state.idempotency[key] = ("schedule", scheduler_id)
        return schedule

    def _start_timer(self, body: dict[str, Any]) -> dict[str, Any]:
        state = self.server
        key = str(body.get("idempotencyKey") or "")
        canonical = json.dumps(body, sort_keys=True, separators=(",", ":"))
        if key:
            existing = state.idempotency.get(key)
            if existing:
                kind, resource_id = existing
                if kind != "timer" or resource_id not in state.timers:
                    raise Conflict("idempotency key was reused")
                stored = state.timers[resource_id]
                if stored.get("_canonical") != canonical:
                    raise Conflict("idempotency key reused with a different request")
                return {
                    key: value for key, value in stored.items() if key != "_canonical"
                }
        timer_id = f"timer-{len(state.timers) + 1}"
        duration = int(body.get("durationSeconds") or 0)
        timer = {
            "id": timer_id,
            "label": body.get("label", ""),
            "durationSeconds": str(duration),
            "remainingSeconds": "0",
            "dueUnix": str(1893455000 + duration),
            "state": "RUNNING",
            "createdUnix": "1893455000",
            "updatedUnix": "1893455000",
            "revision": "1",
        }
        state.timers[timer_id] = {**timer, "_canonical": canonical}
        if key:
            state.idempotency[key] = ("timer", timer_id)
        return timer

    def _send(self, status: int, payload: dict[str, Any]) -> None:
        raw = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)


class Conflict(Exception):
    pass


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def _ics(*blocks: str) -> str:
    body = "\n".join(blocks)
    return (
        "BEGIN:VCALENDAR\r\n"
        "PRODID:-//homeassistant.io//local_todo 2.0//EN\r\n"
        "VERSION:2.0\r\n"
        f"{body}\r\n"
        "END:VCALENDAR\r\n"
    )


def _vtodo(uid: str, summary: str, status: str, due: str | None, dtstart: str | None) -> str:
    lines = ["BEGIN:VTODO", f"UID:{uid}", f"SUMMARY:{summary}", f"STATUS:{status}"]
    if dtstart:
        lines.append(f"DTSTART;TZID=Asia/Almaty:{dtstart}")
    if due:
        lines.append(f"DUE;TZID=Asia/Almaty:{due}")
    lines.append("DESCRIPTION:synthetic fixture")
    lines.append("END:VTODO")
    return "\n".join(lines)


def make_storage(root: Path) -> Path:
    _write(
        root / "local_todo.alarms_weekdays.ics",
        _ics(
            _vtodo(
                "weekday-alarm-1",
                "Synthetic weekday alarm",
                "NEEDS-ACTION",
                "20261028T073000",
                "20261027T200041",
            ),
            _vtodo(
                "no-due-alarm-1",
                "Synthetic no due alarm",
                "NEEDS-ACTION",
                None,
                "20261027T200041",
            ),
        ),
    )
    _write(
        root / "local_todo.alarms_daily.ics",
        _ics(
            _vtodo(
                "daily-alarm-1",
                "Synthetic daily alarm",
                "NEEDS-ACTION",
                "20261028T090000",
                "20261027T090000",
            )
        ),
    )
    _write(
        root / "local_todo.alarms_once.ics",
        _ics(
            _vtodo(
                "once-completed-1",
                "Synthetic completed once",
                "COMPLETED",
                "20300101T120000",
                "20261231T120000",
            )
        ),
    )
    _write(
        root / "local_todo.reminders.ics",
        _ics(
            _vtodo(
                "reminder-1",
                "Synthetic reminder",
                "NEEDS-ACTION",
                "20300101T180000",
                "20261231T180000",
            )
        ),
    )
    timer = {
        "version": 1,
        "minor_version": 1,
        "key": "timer",
        "data": {
            "items": [
                {
                    "id": "slot_running",
                    "name": "Synthetic running timer",
                    "duration": "0:10:00",
                    "restore": True,
                },
                {
                    "id": "slot_paused",
                    "name": "Synthetic paused timer",
                    "duration": "0:05:00",
                    "restore": True,
                },
                {
                    "id": "slot_expired",
                    "name": "Synthetic expired timer",
                    "duration": "0:10:00",
                    "restore": True,
                },
                {
                    "id": "slot_idle",
                    "name": "Synthetic idle timer",
                    "duration": "0:05:00",
                    "restore": True,
                },
            ]
        },
    }
    _write(root / "timer", json.dumps(timer, indent=2))
    restore = {
        "version": 1,
        "minor_version": 1,
        "key": "core.restore_state",
        "data": [
                {
                    "state": {
                        "entity_id": "timer.slot_running",
                        "state": "active",
                        "attributes": {
                            "friendly_name": "Synthetic running timer",
                            "duration": "0:10:00",
                            "finishes_at": "2029-12-31T23:44:20+00:00",
                        },
                        "last_changed": "2029-12-31T23:33:20+00:00",
                        "last_updated": "2029-12-31T23:33:20+00:00",
                    },
                    "extra_data": None,
                },
                {
                    "state": {
                        "entity_id": "timer.slot_paused",
                        "state": "paused",
                        "attributes": {
                            "friendly_name": "Synthetic paused timer",
                            "duration": "0:05:00",
                            "remaining": "0:01:00",
                        },
                        "last_changed": "2029-12-31T23:33:20+00:00",
                        "last_updated": "2029-12-31T23:33:20+00:00",
                    },
                    "extra_data": None,
                },
                {
                    "state": {
                        "entity_id": "timer.slot_expired",
                        "state": "active",
                        "attributes": {
                            "friendly_name": "Synthetic expired timer",
                            "duration": "0:10:00",
                            "finishes_at": "2029-12-31T23:43:19+00:00",
                        },
                        "last_changed": "2029-12-31T23:33:19+00:00",
                        "last_updated": "2029-12-31T23:33:19+00:00",
                    },
                    "extra_data": None,
                },
                {
                    "state": {
                        "entity_id": "timer.slot_idle",
                        "state": "idle",
                        "attributes": {
                            "friendly_name": "Synthetic idle timer",
                            "duration": "0:05:00",
                        },
                        "last_changed": "2029-12-31T23:33:20+00:00",
                        "last_updated": "2029-12-31T23:33:20+00:00",
                    },
                    "extra_data": None,
                },
        ],
    }
    _write(root / "core.restore_state", json.dumps(restore, indent=2))
    return root


class MigrationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.storage = make_storage(self.root / "storage")
        self.snapshot = ha_snapshot.build_snapshot_from_storage(
            self.storage, "Asia/Almaty"
        )
        self.snapshot_path = self.root / "snapshot.json"
        self.snapshot_path.write_text(
            json.dumps(self.snapshot, indent=2), encoding="utf-8"
        )
        self.token_path = self.root / "token"
        self.token_path.write_text(TOKEN + "\n", encoding="utf-8")
        self.map_path = self.root / "mapping.json"
        self.server = FakeScheduler()
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)
        self.temp.cleanup()

    def run_cli(self, *extra: str) -> tuple[int, str, str]:
        argv = [
            "--snapshot",
            str(self.snapshot_path),
            "--endpoint",
            self.server.endpoint,
            "--token-file",
            str(self.token_path),
            "--map-out",
            str(self.map_path),
            *extra,
        ]
        out = io.StringIO()
        err = io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = migrate.main(argv)
        return code, out.getvalue(), err.getvalue()

    def create_requests(self) -> list[dict[str, Any]]:
        return [
            request["body"]
            for request in self.server.requests
            if request["method"] == "CreateSchedule"
        ]

    def test_snapshot_parses_ics_and_quarantine(self) -> None:
        items = {item["label"]: item for item in self.snapshot["items"]}
        weekday = items["Synthetic weekday alarm"]
        self.assertEqual(weekday["source_list"], "alarms_weekdays")
        self.assertEqual(weekday["repeat"], "weekdays")
        self.assertEqual(weekday["weekdays"], [1, 2, 3, 4, 5])
        self.assertEqual(weekday["at"], "07:30:00")
        self.assertTrue(weekday["enabled"])
        self.assertEqual(weekday["timezone"], "Asia/Almaty")
        self.assertEqual(weekday["due"], "20261028T073000")
        self.assertEqual(weekday["dtstart"], "20261027T200041")

        missing = items["Synthetic no due alarm"]
        self.assertEqual(missing["quarantine_reason"], "missing_due_time")
        self.assertFalse(missing["enabled"])
        self.assertIsNone(missing["at"])
        self.assertEqual(missing["repeat"], "weekdays")

        completed = items["Synthetic completed once"]
        self.assertTrue(completed["completed"])
        self.assertFalse(completed["enabled"])

        timers = {timer["label"]: timer for timer in self.snapshot["timers"]}
        self.assertEqual(timers["Synthetic running timer"]["state"], "running")
        self.assertEqual(timers["Synthetic running timer"]["due_unix"], 1893455060)
        self.assertEqual(timers["Synthetic paused timer"]["state"], "paused")
        self.assertEqual(timers["Synthetic paused timer"]["remaining_seconds"], 60)
        self.assertEqual(timers["Synthetic expired timer"]["state"], "running")
        self.assertEqual(timers["Synthetic expired timer"]["due_unix"], 1893454999)
        self.assertEqual(timers["Synthetic idle timer"]["state"], "idle")
        self.assertEqual(timers["Synthetic paused timer"]["duration_seconds"], 300)

    def test_restore_state_running_and_paused_timer_shapes(self) -> None:
        raw = json.loads(
            (self.storage / "core.restore_state").read_text(encoding="utf-8")
        )
        self.assertIsInstance(raw["data"], list)
        self.assertEqual(raw["data"][0]["state"]["entity_id"], "timer.slot_running")
        timers = {timer["label"]: timer for timer in self.snapshot["timers"]}
        self.assertEqual(
            timers["Synthetic running timer"]["slot"], "slot_running"
        )
        self.assertEqual(timers["Synthetic running timer"]["due_unix"], 1893455060)
        self.assertEqual(timers["Synthetic paused timer"]["remaining_seconds"], 60)
        self.assertEqual(timers["Synthetic expired timer"]["due_unix"], 1893454999)
        self.assertEqual(timers["Synthetic idle timer"]["remaining_seconds"], 0)

    def test_restore_state_dict_wrapper_and_flat_entries_still_parse(self) -> None:
        raw = json.loads(
            (self.storage / "core.restore_state").read_text(encoding="utf-8")
        )
        wrapped_path = self.root / "wrapped_restore.json"
        wrapped_path.write_text(
            json.dumps({"version": 1, "data": {"state": raw["data"]}}),
            encoding="utf-8",
        )
        wrapped = {
            timer["slot"]: timer
            for timer in ha_snapshot._timers_from_restore_state(
                wrapped_path, "Asia/Almaty"
            )
        }
        self.assertEqual(wrapped["slot_running"]["due_unix"], 1893455060)
        self.assertEqual(wrapped["slot_paused"]["remaining_seconds"], 60)

        flat_path = self.root / "flat_restore.json"
        flat_path.write_text(
            json.dumps(
                {
                    "version": 1,
                    "data": [
                        {
                            "entity_id": "timer.flat_timer",
                            "state": "paused",
                            "attributes": {
                                "duration": "0:02:00",
                                "remaining": "0:00:30",
                            },
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )
        flat = ha_snapshot._timers_from_restore_state(flat_path, "Asia/Almaty")
        self.assertEqual(len(flat), 1)
        self.assertEqual(flat[0]["slot"], "flat_timer")
        self.assertEqual(flat[0]["state"], "paused")
        self.assertEqual(flat[0]["remaining_seconds"], 30)

    def test_empty_storage_fails_closed_without_allow_empty(self) -> None:
        empty = self.root / "empty-storage"
        empty.mkdir()
        output = self.root / "empty-snapshot.json"
        stdout = io.StringIO()
        stderr = io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = ha_snapshot.main(
                ["--storage", str(empty), "--out", str(output)]
            )
        self.assertEqual(code, 2)
        self.assertIn("no source items found", stderr.getvalue())
        self.assertFalse(output.exists())

        stdout = io.StringIO()
        stderr = io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            code = ha_snapshot.main(
                [
                    "--storage",
                    str(empty),
                    "--out",
                    str(output),
                    "--allow-empty",
                ]
            )
        self.assertEqual(code, 0, stderr.getvalue())
        self.assertTrue(output.exists())
        saved = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(saved["items"], [])
        self.assertEqual(saved["timers"], [])

    def test_empty_snapshot_fails_closed_without_allow_empty(self) -> None:
        self.snapshot_path.write_text(
            json.dumps({"schema": "ricardo.ha_snapshot.v1", "items": [], "timers": []}),
            encoding="utf-8",
        )
        code, out, err = self.run_cli("--dry-run")
        self.assertEqual(code, 2)
        self.assertIn("snapshot has no source items", err)
        self.assertFalse(self.map_path.exists())
        self.assertEqual(self.server.writes(), [])

        code, out, err = self.run_cli("--dry-run", "--allow-empty")
        self.assertEqual(code, 0, err)

    def test_dry_run_makes_no_writes_and_never_prints_token(self) -> None:
        code, out, err = self.run_cli("--dry-run")
        self.assertEqual(code, 0, err)
        self.assertIn("dry-run", out)
        self.assertIn("scheduler status", out)
        self.assertNotIn(TOKEN, out + err)
        self.assertFalse(self.map_path.exists())
        self.assertEqual(self.server.writes(), [])
        self.assertEqual(
            [request["method"] for request in self.server.requests], ["GetStatus"]
        )

    def test_execute_sends_expected_payloads(self) -> None:
        code, out, err = self.run_cli("--execute")
        self.assertEqual(code, 0, err + out)
        bodies = {body["label"]: body for body in self.create_requests()}
        self.assertEqual(len(bodies), 3)
        self.assertNotIn("Synthetic completed once", bodies)
        self.assertNotIn("Synthetic no due alarm", bodies)

        weekday = bodies["Synthetic weekday alarm"]
        self.assertEqual(weekday["kind"], "SCHEDULE_KIND_ALARM")
        self.assertEqual(weekday["repeat"], "REPEAT_KIND_WEEKDAYS")
        self.assertEqual(weekday["at"], {"hour": 7, "minute": 30, "second": 0})
        self.assertEqual(weekday["timezone"], "Asia/Almaty")
        self.assertTrue(weekday["enabled"])
        self.assertNotIn("weekdays", weekday)
        self.assertNotIn("onceDate", weekday)

        daily = bodies["Synthetic daily alarm"]
        self.assertEqual(daily["repeat"], "REPEAT_KIND_DAILY")
        self.assertEqual(daily["at"], {"hour": 9, "minute": 0, "second": 0})
        self.assertNotIn("onceDate", daily)

        reminder = bodies["Synthetic reminder"]
        self.assertEqual(reminder["kind"], "SCHEDULE_KIND_REMINDER")
        self.assertEqual(reminder["repeat"], "REPEAT_KIND_ONCE")
        self.assertEqual(reminder["onceDate"], "2030-01-01")
        self.assertEqual(reminder["at"], {"hour": 18, "minute": 0, "second": 0})

        starts = [
            request["body"]
            for request in self.server.requests
            if request["method"] == "StartTimer"
        ]
        self.assertEqual(len(starts), 2)
        by_label = {body["label"]: body for body in starts}
        # A 10-minute timer with 9 minutes elapsed is imported as ~60 seconds,
        # never as its original 600-second duration.
        self.assertEqual(by_label["Synthetic running timer"]["durationSeconds"], "60")
        self.assertEqual(by_label["Synthetic paused timer"]["durationSeconds"], "60")
        self.assertNotIn("Synthetic idle timer", by_label)
        self.assertNotIn("Synthetic expired timer", by_label)
        self.assertEqual(
            len(
                [
                    request
                    for request in self.server.requests
                    if request["method"] == "PauseTimer"
                ]
            ),
            1,
        )
        self.assertEqual(
            self.server.timers["timer-1"]["dueUnix"], "1893455060"
        )
        self.assertEqual(self.server.timers["timer-2"]["state"], "PAUSED")
        self.assertEqual(self.server.timers["timer-2"]["remainingSeconds"], "60")

        mapping = json.loads(self.map_path.read_text(encoding="utf-8"))
        records = {record["label"]: record for record in mapping["items"].values()}
        self.assertEqual(records["Synthetic completed once"]["action"], "skip")
        self.assertEqual(records["Synthetic idle timer"]["action"], "skip")
        self.assertEqual(records["Synthetic expired timer"]["action"], "skip")
        self.assertEqual(
            records["Synthetic expired timer"]["reason"], "skipped_expired"
        )
        malformed = records["Synthetic no due alarm"]
        self.assertEqual(malformed["status"], "quarantined_unresolved")
        self.assertEqual(malformed["reason"], "missing_due_time")
        self.assertEqual(malformed["uid"], "no-due-alarm-1")
        self.assertEqual(malformed["source_list"], "alarms_weekdays")
        self.assertEqual(malformed["repeat"], "weekdays")
        self.assertEqual(malformed["source_dtstart"], "20261027T200041")
        self.assertIsNone(malformed["source_due"])
        self.assertIsNone(malformed["scheduler_id"])
        self.assertEqual(
            records["Synthetic running timer"]["source_duration_seconds"], 600
        )
        self.assertEqual(
            records["Synthetic running timer"]["expected"]["imported_duration_seconds"],
            60,
        )
        self.assertTrue(records["Synthetic weekday alarm"]["verified"])
        self.assertEqual(mapping["summary"]["created"], 5)
        self.assertEqual(mapping["summary"]["unresolved"], 1)
        self.assertEqual(mapping["summary"]["skipped"], 3)
        self.assertEqual(mapping["summary"]["verification_failures"], 0)
        self.assertIn("unresolved=1", out)
        self.assertIn("verification:", out)
        self.assertNotIn(TOKEN, out + err)

    def test_second_execute_and_resume_create_nothing(self) -> None:
        self.assertEqual(self.run_cli("--execute")[0], 0)
        first_counts = (len(self.server.schedules), len(self.server.timers))
        timer_writes = [
            request["method"]
            for request in self.server.requests
            if request["method"] in {"StartTimer", "PauseTimer"}
        ]
        code, out, err = self.run_cli("--execute")
        self.assertEqual(code, 0, err + out)
        self.assertEqual((len(self.server.schedules), len(self.server.timers)), first_counts)
        self.assertEqual(
            [
                request["method"]
                for request in self.server.requests
                if request["method"] in {"StartTimer", "PauseTimer"}
            ],
            timer_writes,
        )
        mapping = json.loads(self.map_path.read_text(encoding="utf-8"))
        self.assertEqual(mapping["summary"]["created"], 0)
        self.assertGreater(mapping["summary"]["replayed"], 0)

        writes_before = len(self.server.writes())
        code, out, err = self.run_cli("--execute", "--resume")
        self.assertEqual(code, 0, err + out)
        self.assertEqual(len(self.server.writes()), writes_before)
        mapping = json.loads(self.map_path.read_text(encoding="utf-8"))
        self.assertEqual(mapping["summary"]["created"], 0)
        self.assertEqual(mapping["summary"]["resumed"], 9)
        self.assertEqual(mapping["summary"]["unresolved"], 1)
        self.assertEqual((len(self.server.schedules), len(self.server.timers)), first_counts)

    def test_verification_catches_mismatch(self) -> None:
        self.server.mutate_label = True
        code, out, err = self.run_cli("--execute")
        self.assertEqual(code, 1)
        self.assertIn("MISMATCH", out)
        self.assertIn("verification failed", err)
        mapping = json.loads(self.map_path.read_text(encoding="utf-8"))
        self.assertGreater(mapping["summary"]["verification_failures"], 0)
        self.assertFalse(
            mapping["items"]["alarms_weekdays:weekday-alarm-1"]["verified"]
        )

    def test_verification_catches_timer_due_mismatch(self) -> None:
        self.server.timer_due_drift = 30
        code, out, err = self.run_cli("--execute")
        self.assertEqual(code, 1)
        self.assertIn("dueUnix", out)
        self.assertIn("verification failed", err)
        mapping = json.loads(self.map_path.read_text(encoding="utf-8"))
        record = mapping["items"]["timer:slot_running"]
        self.assertFalse(record["verified"])
        self.assertIn("dueUnix", record["verification_detail"])

    def test_verification_catches_timer_remaining_mismatch(self) -> None:
        self.server.timer_remaining_drift = 30
        code, out, err = self.run_cli("--execute")
        self.assertEqual(code, 1)
        self.assertIn("remainingSeconds", out)
        self.assertIn("verification failed", err)
        mapping = json.loads(self.map_path.read_text(encoding="utf-8"))
        record = mapping["items"]["timer:slot_paused"]
        self.assertFalse(record["verified"])
        self.assertIn("remainingSeconds", record["verification_detail"])

    def test_strict_only_fails_on_unresolved_items(self) -> None:
        code, out, err = self.run_cli("--execute", "--strict")
        self.assertEqual(code, 1)
        self.assertIn("unresolved=1", out)
        self.assertIn("strict mode: 1 unresolved", err)
        mapping = json.loads(self.map_path.read_text(encoding="utf-8"))
        self.assertEqual(mapping["summary"]["unresolved"], 1)
        self.assertEqual(mapping["summary"]["verification_failures"], 0)

    def test_token_is_redacted_from_http_errors(self) -> None:
        self.server.auth_reject_with_token = True
        code, out, err = self.run_cli("--dry-run")
        self.assertEqual(code, 2)
        self.assertNotIn(TOKEN, out + err)
        self.assertIn("HTTP 401", err)
        self.assertFalse(self.map_path.exists())


if __name__ == "__main__":
    unittest.main()
