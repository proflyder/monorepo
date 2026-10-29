#!/usr/bin/env python3
"""Resumable, idempotent HA alarm/reminder/timer migration.

Dry-run is the default.  ``--execute`` performs Connect JSON calls against the
canonical ricardo.v1 SchedulerService and then re-lists resources to verify
the import.  No Home Assistant endpoint is contacted by this tool.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import os
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

import ha_snapshot

SERVICE_PATH = "/ricardo.v1.SchedulerService"
MAX_RESPONSE_BYTES = 4 << 20
MAX_TIMER_SECONDS = 604800
TIMER_TOLERANCE_FLOOR = 5
MAP_SCHEMA = "ricardo.ha_scheduler_migration.map.v1"
KIND_NAMES = {
    "alarm": "SCHEDULE_KIND_ALARM",
    "reminder": "SCHEDULE_KIND_REMINDER",
}
REPEAT_NAMES = {
    "once": "REPEAT_KIND_ONCE",
    "daily": "REPEAT_KIND_DAILY",
    "weekdays": "REPEAT_KIND_WEEKDAYS",
    "weekends": "REPEAT_KIND_WEEKENDS",
    "custom": "REPEAT_KIND_CUSTOM",
}
REPEAT_ORDER = {
    "REPEAT_KIND_UNSPECIFIED": 0,
    "REPEAT_KIND_ONCE": 1,
    "REPEAT_KIND_DAILY": 2,
    "REPEAT_KIND_WEEKDAYS": 3,
    "REPEAT_KIND_WEEKENDS": 4,
    "REPEAT_KIND_CUSTOM": 5,
}
KIND_ORDER = {
    "SCHEDULE_KIND_UNSPECIFIED": 0,
    "SCHEDULE_KIND_ALARM": 1,
    "SCHEDULE_KIND_REMINDER": 2,
    "SCHEDULE_KIND_SCENARIO": 3,
}
TIMER_STATE_ORDER = {
    "TIMER_STATE_UNSPECIFIED": 0,
    "TIMER_STATE_RUNNING": 1,
    "TIMER_STATE_PAUSED": 2,
    "TIMER_STATE_FIRED": 3,
    "TIMER_STATE_CANCELLED": 4,
}


class MigrationError(Exception):
    pass


class SchedulerError(MigrationError):
    pass


def _atomic_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(prefix=path.name + ".", dir=str(path.parent))
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, ensure_ascii=False, indent=2, sort_keys=False)
            handle.write("\n")
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def _redact(text: Any, token: str) -> str:
    value = str(text)
    return value.replace(token, "[redacted]") if token else value


def read_token(path: str | os.PathLike[str]) -> str:
    """Read a bearer token.  The value is never logged or included in errors."""
    token_path = Path(path)
    if not token_path.is_file():
        raise MigrationError(f"token file does not exist: {token_path}")
    try:
        token = token_path.read_text(encoding="utf-8").strip()
    except OSError as exc:
        raise MigrationError(f"cannot read token file {token_path}: {exc}") from exc
    if not token:
        raise MigrationError(f"token file is empty: {token_path}")
    if any(char in token for char in "\r\n"):
        raise MigrationError("token file must contain exactly one token")
    return token


def normalize_endpoint(endpoint: str) -> str:
    base = endpoint.strip().rstrip("/")
    if base.endswith(SERVICE_PATH):
        base = base[: -len(SERVICE_PATH)]
    if not base.startswith(("http://", "https://")):
        raise MigrationError("endpoint must start with http:// or https://")
    return base.rstrip("/")


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):  # noqa: ANN001
        return None


class ConnectClient:
    """Minimal Connect unary JSON client for SchedulerService."""

    def __init__(self, endpoint: str, token: str, timeout: float = 15.0):
        self.base = normalize_endpoint(endpoint)
        self.token = token
        self.timeout = timeout
        self.opener = urllib.request.build_opener(_NoRedirect())

    def _sanitize(self, text: Any) -> str:
        return _redact(text, self.token)

    def call(self, method: str, payload: dict[str, Any]) -> dict[str, Any]:
        url = self.base + SERVICE_PATH + "/" + method
        data = json.dumps(payload, separators=(",", ":")).encode("utf-8")
        request = urllib.request.Request(url, data=data, method="POST")
        request.add_header("Content-Type", "application/json")
        request.add_header("Accept", "application/json")
        request.add_header("Connect-Protocol-Version", "1")
        request.add_header("Authorization", "Bearer " + self.token)
        try:
            with self.opener.open(request, timeout=self.timeout) as response:
                raw = response.read(MAX_RESPONSE_BYTES + 1)
                status = response.status
        except urllib.error.HTTPError as exc:
            raw = exc.read(MAX_RESPONSE_BYTES + 1)
            code = ""
            message = ""
            try:
                parsed = json.loads(raw.decode("utf-8", "replace"))
                if isinstance(parsed, dict):
                    code = str(parsed.get("code") or "")
                    message = str(parsed.get("message") or "")
            except (json.JSONDecodeError, UnicodeDecodeError):
                message = raw.decode("utf-8", "replace")[:300]
            detail = self._sanitize(message or "request failed")
            code_text = self._sanitize(code)
            raise SchedulerError(
                f"{method}: HTTP {exc.code}"
                + (f" code={code_text}" if code_text else "")
                + f": {detail}"
            ) from None
        except urllib.error.URLError as exc:
            reason = self._sanitize(getattr(exc, "reason", exc))
            raise SchedulerError(f"{method}: network error: {reason}") from None
        except OSError as exc:
            raise SchedulerError(f"{method}: network error: {self._sanitize(exc)}") from None
        if len(raw) > MAX_RESPONSE_BYTES:
            raise SchedulerError(f"{method}: response exceeds {MAX_RESPONSE_BYTES} bytes")
        if status != 200:
            raise SchedulerError(f"{method}: HTTP {status}")
        if not raw:
            return {}
        try:
            parsed = json.loads(raw.decode("utf-8"))
        except (json.JSONDecodeError, UnicodeDecodeError) as exc:
            raise SchedulerError(f"{method}: invalid JSON response: {exc}") from None
        if not isinstance(parsed, dict):
            raise SchedulerError(f"{method}: response is not a JSON object")
        return parsed

    def get_status(self) -> dict[str, Any]:
        return self.call("GetStatus", {})

    def list_schedules(self) -> list[dict[str, Any]]:
        response = self.call("ListSchedules", {"includeDisabled": True})
        items = response.get("schedules", [])
        return [item for item in items if isinstance(item, dict)]

    def create_schedule(self, payload: dict[str, Any]) -> dict[str, Any]:
        response = self.call("CreateSchedule", payload)
        schedule = response.get("schedule")
        if not isinstance(schedule, dict) or not schedule.get("id"):
            raise SchedulerError("CreateSchedule: response has no schedule.id")
        return schedule

    def list_timers(self) -> list[dict[str, Any]]:
        response = self.call("ListTimers", {"includeFinished": True})
        items = response.get("timers", [])
        return [item for item in items if isinstance(item, dict)]

    def get_timer(self, timer_id: str) -> dict[str, Any]:
        response = self.call("GetTimer", {"id": timer_id})
        timer = response.get("timer")
        if not isinstance(timer, dict) or not timer.get("id"):
            raise SchedulerError("GetTimer: response has no timer.id")
        return timer

    def start_timer(self, payload: dict[str, Any]) -> dict[str, Any]:
        response = self.call("StartTimer", payload)
        timer = response.get("timer")
        if not isinstance(timer, dict) or not timer.get("id"):
            raise SchedulerError("StartTimer: response has no timer.id")
        return timer

    def pause_timer(self, timer_id: str) -> dict[str, Any]:
        response = self.call("PauseTimer", {"id": timer_id})
        timer = response.get("timer")
        if not isinstance(timer, dict) or not timer.get("id"):
            raise SchedulerError("PauseTimer: response has no timer.id")
        return timer


def _parse_at(value: Any) -> tuple[int, int, int] | None:
    if isinstance(value, dict):
        try:
            wall = (
                int(value.get("hour", 0)),
                int(value.get("minute", 0)),
                int(value.get("second", 0)),
            )
        except (TypeError, ValueError):
            return None
    elif isinstance(value, str):
        parts = value.split(":")
        if len(parts) not in {2, 3} or not all(part.isdigit() for part in parts):
            return None
        wall = (
            int(parts[0]),
            int(parts[1]),
            int(parts[2]) if len(parts) == 3 else 0,
        )
    else:
        return None
    hour, minute, second = wall
    if not (0 <= hour <= 23 and 0 <= minute <= 59 and 0 <= second <= 59):
        return None
    return wall


def _format_at(wall: tuple[int, int, int]) -> str:
    return f"{wall[0]:02d}:{wall[1]:02d}:{wall[2]:02d}"


def _source_key(item: dict[str, Any]) -> str:
    return f"{item.get('source_list', 'snapshot')}:{item.get('uid', '')}"


def _idempotency_key(item: dict[str, Any]) -> str:
    return f"ha:{item.get('source_list', 'snapshot')}:{item.get('uid', '')}"


def _timer_key(timer: dict[str, Any]) -> str:
    return f"timer:{timer.get('slot', timer.get('uid', ''))}"


def _timer_idempotency_key(timer: dict[str, Any]) -> str:
    return f"ha:timer:{timer.get('slot', timer.get('uid', ''))}"


def _valid_timezone(name: Any) -> bool:
    text = str(name or "")
    if not text or text[0] in "+-":
        return False
    try:
        ZoneInfo(text)
        return True
    except (ZoneInfoNotFoundError, ValueError):
        return False


def _payload_json(item: dict[str, Any]) -> str:
    provenance = {
        "source": "home-assistant-local-todo",
        "sourceList": item.get("source_list"),
        "uid": item.get("uid"),
        "sourceDue": item.get("due"),
        "sourceDtstart": item.get("dtstart"),
        "quarantineReason": item.get("quarantine_reason"),
    }
    return json.dumps(provenance, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def _schedule_plan(item: dict[str, Any]) -> dict[str, Any]:
    key = _source_key(item)
    repeat = item.get("repeat")
    kind = item.get("kind") or "alarm"
    label = str(item.get("label") or "")
    timezone = item.get("timezone") or ha_snapshot.DEFAULT_TIMEZONE
    quarantine = item.get("quarantine_reason")
    completed = bool(item.get("completed"))
    plan: dict[str, Any] = {
        "type": "schedule",
        "key": key,
        "kind": kind,
        "label": label,
        "source": item,
    }

    if completed and repeat == "once":
        return {**plan, "action": "skip", "reason": "completed", "verification_required": False}
    if kind == "reminder" and repeat != "once":
        return {
            **plan,
            "action": "skip",
            "reason": "unsupported_repeat_for_reminder",
            "verification_required": False,
        }
    if repeat not in REPEAT_NAMES:
        return {
            **plan,
            "action": "skip",
            "reason": quarantine or "unknown_repeat_list",
            "verification_required": False,
        }
    if not _valid_timezone(timezone):
        return {
            **plan,
            "action": "skip",
            "reason": quarantine or "invalid_timezone",
            "verification_required": False,
        }

    wall = _parse_at(item.get("at"))
    once_date = item.get("once_date")
    if quarantine:
        # Never create even a disabled schedule: a disabled schedule still
        # carries a wall time that a client could later enable, silently
        # inventing an unknown clock.  Keep it only in the ledger.
        return {
            **plan,
            "action": "quarantine_unresolved",
            "reason": quarantine,
            "verification_required": False,
        }
    if wall is None:
        return {
            **plan,
            "action": "skip",
            "reason": "invalid_at",
            "verification_required": False,
        }
    if repeat == "once" and not once_date:
        return {
            **plan,
            "action": "skip",
            "reason": "missing_once_date",
            "verification_required": False,
        }

    enabled = bool(item.get("enabled")) and not completed
    payload: dict[str, Any] = {
        "label": label,
        "kind": KIND_NAMES[kind],
        "timezone": timezone,
        "at": {"hour": wall[0], "minute": wall[1], "second": wall[2]},
        "repeat": REPEAT_NAMES[repeat],
        "enabled": enabled,
        "payloadJson": _payload_json(item),
        "idempotencyKey": _idempotency_key(item),
    }
    if repeat == "once":
        payload["onceDate"] = str(once_date)
    if repeat == "custom":
        payload["weekdays"] = [int(day) for day in item.get("weekdays") or []]

    expected = {
        "type": "schedule",
        "label": label,
        "kind": kind,
        "repeat": repeat,
        "at": _format_at(wall),
        "timezone": timezone,
        "once_date": str(once_date) if repeat == "once" else None,
        "weekdays": payload.get("weekdays", []),
        "enabled": enabled,
    }
    return {
        **plan,
        "action": "create",
        "reason": None,
        "payload": payload,
        "expected": expected,
        "verification_required": True,
    }


def _timer_plan(
    timer: dict[str, Any], server_now_unix: int, status_monotonic: float
) -> dict[str, Any]:
    key = _timer_key(timer)
    slot = timer.get("slot", timer.get("uid", ""))
    plan: dict[str, Any] = {
        "type": "timer",
        "key": key,
        "kind": "timer",
        "label": str(timer.get("label") or ""),
        "source": timer,
    }
    if timer.get("quarantine_reason"):
        return {
            **plan,
            "action": "skip",
            "reason": timer.get("quarantine_reason"),
            "verification_required": False,
        }
    state = str(timer.get("state") or "idle").lower()
    if state not in {"running", "paused"}:
        return {
            **plan,
            "action": "skip",
            "reason": state if state != "idle" else "idle",
            "verification_required": False,
        }
    try:
        source_duration = int(timer.get("duration_seconds") or 0)
    except (TypeError, ValueError):
        source_duration = 0
    if source_duration <= 0:
        return {
            **plan,
            "action": "skip",
            "reason": "invalid_duration",
            "verification_required": False,
        }
    if server_now_unix <= 0:
        return {
            **plan,
            "action": "skip",
            "reason": "missing_scheduler_clock",
            "verification_required": False,
        }

    if state == "running":
        try:
            source_due = int(timer.get("due_unix") or 0)
        except (TypeError, ValueError):
            source_due = 0
        if source_due <= 0:
            return {
                **plan,
                "action": "skip",
                "reason": "missing_due_unix",
                "verification_required": False,
            }
        remaining = source_due - server_now_unix
    else:
        try:
            remaining = int(timer.get("remaining_seconds") or 0)
        except (TypeError, ValueError):
            remaining = 0
        source_due = 0

    if remaining <= 0:
        return {
            **plan,
            "action": "skip",
            "reason": "skipped_expired",
            "verification_required": False,
        }
    if remaining > source_duration + TIMER_TOLERANCE_FLOOR:
        return {
            **plan,
            "action": "skip",
            "reason": "clock_skew_or_invalid_remaining",
            "verification_required": False,
        }
    if remaining > MAX_TIMER_SECONDS:
        return {
            **plan,
            "action": "skip",
            "reason": "remaining_exceeds_max_timer_duration",
            "verification_required": False,
        }

    elapsed = max(0.0, time.monotonic() - status_monotonic)
    tolerance = max(TIMER_TOLERANCE_FLOOR, math.ceil(elapsed) + 2)
    payload = {
        "label": plan["label"],
        # The canonical countdown is the remaining time, never the original HA
        # duration.  protobuf JSON encodes int64 as a decimal string.
        "durationSeconds": str(remaining),
        "idempotencyKey": _timer_idempotency_key(timer),
    }
    expected = {
        "type": "timer",
        "label": plan["label"],
        "state": state,
        "source_duration_seconds": source_duration,
        "source_due_unix": source_due,
        "source_remaining_seconds": remaining,
        "imported_duration_seconds": remaining,
        "tolerance_seconds": tolerance,
    }
    return {
        **plan,
        "action": "create",
        "reason": None,
        "payload": payload,
        "expected": expected,
        "timer_state": state,
        "slot": slot,
        "verification_required": True,
        "status_monotonic": status_monotonic,
    }


def build_plans(
    snapshot: dict[str, Any],
    server_now_unix: int = 0,
    status_monotonic: float | None = None,
) -> list[dict[str, Any]]:
    clock = time.monotonic() if status_monotonic is None else status_monotonic
    plans = [_schedule_plan(item) for item in snapshot.get("items", [])]
    plans.extend(
        _timer_plan(timer, server_now_unix, clock)
        for timer in snapshot.get("timers", [])
    )
    return plans


def _load_mapping(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise MigrationError(f"cannot read mapping file {path}: {exc}") from exc
    if not isinstance(data, dict) or not isinstance(data.get("items", {}), dict):
        raise MigrationError(f"mapping file has an unsupported shape: {path}")
    return data


def _new_mapping(snapshot_path: str, snapshot: dict[str, Any]) -> dict[str, Any]:
    return {
        "schema": MAP_SCHEMA,
        "snapshot": snapshot_path,
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "timezone": snapshot.get("timezone"),
        "items": {},
        "summary": {},
    }


def _record(
    mapping: dict[str, Any],
    plan: dict[str, Any],
    action: str,
    *,
    scheduler_id: str | None = None,
    detail: str | None = None,
    replayed: bool = False,
) -> dict[str, Any]:
    prior = mapping["items"].get(plan["key"], {})
    source = plan["source"]
    record = {
        "source_list": source.get("source_list"),
        "uid": source.get("uid") or source.get("slot"),
        "kind": plan.get("kind"),
        "label": plan.get("label"),
        "repeat": source.get("repeat"),
        "timezone": source.get("timezone"),
        "source_dtstart": source.get("dtstart"),
        "source_due": source.get("due"),
        "source_duration_seconds": source.get("duration_seconds")
        if plan["type"] == "timer"
        else None,
        "source_due_unix": source.get("due_unix") if plan["type"] == "timer" else None,
        "source_remaining_seconds": source.get("remaining_seconds")
        if plan["type"] == "timer"
        else None,
        "idempotency_key": (
            _timer_idempotency_key(source)
            if plan["type"] == "timer"
            else _idempotency_key(source)
        ),
        "scheduler_id": scheduler_id or prior.get("scheduler_id"),
        "status": "quarantined_unresolved"
        if action == "quarantine_unresolved"
        else action,
        "action": action,
        "reason": plan.get("reason")
        if action in {"skip", "quarantine_unresolved"}
        else None,
        "verification_required": bool(plan.get("verification_required")),
        "expected": plan.get("expected"),
        "verified": None,
        "verification_detail": detail or "",
        "last_run": "replayed" if replayed else action,
    }
    mapping["items"][plan["key"]] = record
    return record


def _summarize(
    mapping: dict[str, Any], snapshot: dict[str, Any], run_counts: dict[str, int]
) -> dict[str, Any]:
    mapped = sum(1 for record in mapping["items"].values() if record.get("scheduler_id"))
    unresolved = sum(
        1
        for record in mapping["items"].values()
        if record.get("status") == "quarantined_unresolved"
    )
    summary = {
        "source_items": run_counts.get(
            "source_items", len(snapshot.get("items", [])) + len(snapshot.get("timers", []))
        ),
        "created": run_counts.get("created", 0),
        "replayed": run_counts.get("replayed", 0),
        "unresolved": unresolved,
        "skipped": run_counts.get("skip", 0),
        "resumed": run_counts.get("resumed", 0),
        "mapped_resources": mapped,
        "verification_failures": run_counts.get("verification_failures", 0),
        "errors": run_counts.get("errors", 0),
    }
    mapping["summary"] = summary
    return summary


def _print_status(status: dict[str, Any]) -> None:
    print(
        "scheduler status: "
        f"ready={bool(status.get('ready'))} "
        f"version={status.get('version') or 'unknown'} "
        f"schemaVersion={status.get('schemaVersion') or 0} "
        f"enabledSchedules={status.get('enabledSchedules') or 0} "
        f"runningTimers={status.get('runningTimers') or 0} "
        f"pendingEvents={status.get('pendingEvents') or 0}"
    )


def _print_plan(plan: dict[str, Any], prefix: str = "plan") -> None:
    reason = f" ({plan['reason']})" if plan.get("reason") else ""
    print(f"[{prefix}] {plan['kind']} {plan['label']!r}: {plan['action']}{reason}")


def _terminal_resume(record: dict[str, Any]) -> bool:
    return bool(record.get("scheduler_id")) or record.get("action") in {
        "skip",
        "quarantine_unresolved",
    }


def execute_plans(
    client: ConnectClient,
    plans: list[dict[str, Any]],
    mapping: dict[str, Any],
    *,
    resume: bool,
    map_path: Path,
) -> dict[str, int]:
    run_counts: dict[str, int] = {"source_items": len(plans)}
    for plan in plans:
        prior = mapping["items"].get(plan["key"])
        if resume and prior and _terminal_resume(prior):
            run_counts["resumed"] = run_counts.get("resumed", 0) + 1
            print(f"[resume] {plan['kind']} {plan['label']!r}: already mapped")
            continue

        if plan["action"] == "quarantine_unresolved":
            _record(mapping, plan, "quarantine_unresolved")
            run_counts["unresolved"] = run_counts.get("unresolved", 0) + 1
            print(
                f"[quarantine_unresolved] {plan['kind']} {plan['label']!r}: "
                f"{plan.get('reason')}"
            )
            _summarize(mapping, {"items": [], "timers": []}, run_counts)
            _atomic_json(map_path, mapping)
            continue

        prior_id = prior.get("scheduler_id") if prior else None
        if plan["type"] == "timer" and prior_id:
            # StartTimer is not safely idempotent when the remaining time has
            # changed.  A mapped timer id is re-read, never re-created with a
            # different duration (even if the source now looks expired).
            try:
                existing = _observed_timer(client.get_timer(str(prior_id)))
            except SchedulerError as exc:
                _record(mapping, plan, "error", detail=str(exc))
                run_counts["errors"] = run_counts.get("errors", 0) + 1
                _summarize(mapping, {"items": [], "timers": []}, run_counts)
                _atomic_json(map_path, mapping)
                raise
            effective_plan = dict(plan)
            effective_plan["expected"] = plan.get("expected") or prior.get("expected")
            effective_plan["verification_required"] = bool(
                effective_plan.get("expected")
            )
            _record(
                mapping,
                effective_plan,
                "replayed",
                scheduler_id=str(prior_id),
                replayed=True,
            )
            run_counts["replayed"] = run_counts.get("replayed", 0) + 1
            print(
                f"[replayed] timer {plan['label']!r} -> {prior_id} "
                f"(state={existing.get('state')}, no write)"
            )
            _summarize(mapping, {"items": [], "timers": []}, run_counts)
            _atomic_json(map_path, mapping)
            continue

        if plan["action"] == "skip":
            _record(mapping, plan, "skip")
            run_counts["skip"] = run_counts.get("skip", 0) + 1
            print(f"[skip] {plan['kind']} {plan['label']!r}: {plan.get('reason')}")
            _summarize(mapping, {"items": [], "timers": []}, run_counts)
            _atomic_json(map_path, mapping)
            continue

        try:
            if plan["type"] == "schedule":
                schedule = client.create_schedule(plan["payload"])
                scheduler_id = str(schedule["id"])
                if prior_id == scheduler_id:
                    action = "replayed"
                    run_counts["replayed"] = run_counts.get("replayed", 0) + 1
                else:
                    action = "created"
                    run_counts["created"] = run_counts.get("created", 0) + 1
            else:
                timer = client.start_timer(plan["payload"])
                scheduler_id = str(timer["id"])
                if plan.get("timer_state") == "paused":
                    client.pause_timer(scheduler_id)
                observed = _observed_timer(client.get_timer(scheduler_id))
                problems = _timer_problems(plan["expected"], observed)
                if problems:
                    raise SchedulerError(
                        "timer post-create verification failed: " + "; ".join(problems)
                    )
                if prior_id == scheduler_id:
                    action = "replayed"
                    run_counts["replayed"] = run_counts.get("replayed", 0) + 1
                else:
                    action = "created"
                    run_counts["created"] = run_counts.get("created", 0) + 1
        except SchedulerError as exc:
            _record(mapping, plan, "error", detail=str(exc))
            run_counts["errors"] = run_counts.get("errors", 0) + 1
            _summarize(mapping, {"items": [], "timers": []}, run_counts)
            _atomic_json(map_path, mapping)
            raise

        _record(
            mapping,
            plan,
            action,
            scheduler_id=scheduler_id,
            replayed=(action == "replayed"),
        )
        print(f"[{action}] {plan['kind']} {plan['label']!r} -> {scheduler_id}")
        _summarize(mapping, {"items": [], "timers": []}, run_counts)
        _atomic_json(map_path, mapping)
    return run_counts


def _enum_name(value: Any, prefix: str, order: dict[str, int]) -> str:
    if isinstance(value, bool):
        text = ""
    elif isinstance(value, int):
        for name, number in order.items():
            if number == value:
                return name
        return ""
    else:
        text = str(value or "").strip()
        if text.isdigit():
            return _enum_name(int(text), prefix, order)
    if not text:
        return ""
    if text in order:
        return text
    return prefix + text.removeprefix(prefix).upper()


def _observed_at(value: Any) -> str | None:
    parsed = _parse_at(value)
    return _format_at(parsed) if parsed else None


def _observed_schedule(schedule: dict[str, Any]) -> dict[str, Any]:
    return {
        "id": str(schedule.get("id") or ""),
        "label": str(schedule.get("label") or ""),
        "kind": _enum_name(schedule.get("kind"), "SCHEDULE_KIND_", KIND_ORDER)
        .removeprefix("SCHEDULE_KIND_")
        .lower(),
        "repeat": _enum_name(schedule.get("repeat"), "REPEAT_KIND_", REPEAT_ORDER)
        .removeprefix("REPEAT_KIND_")
        .lower(),
        "at": _observed_at(schedule.get("at")),
        "timezone": str(schedule.get("timezone") or ""),
        "once_date": str(schedule.get("onceDate") or schedule.get("once_date") or ""),
        "weekdays": [int(day) for day in schedule.get("weekdays") or []],
        "enabled": bool(schedule.get("enabled")),
    }


def _observed_timer(timer: dict[str, Any]) -> dict[str, Any]:
    return {
        "id": str(timer.get("id") or ""),
        "label": str(timer.get("label") or ""),
        "duration_seconds": int(
            timer.get("durationSeconds") or timer.get("duration_seconds") or 0
        ),
        "remaining_seconds": int(
            timer.get("remainingSeconds") or timer.get("remaining_seconds") or 0
        ),
        "due_unix": int(timer.get("dueUnix") or timer.get("due_unix") or 0),
        "state": _enum_name(timer.get("state"), "TIMER_STATE_", TIMER_STATE_ORDER)
        .removeprefix("TIMER_STATE_")
        .lower(),
    }


def _timer_problems(expected: dict[str, Any], observed: dict[str, Any]) -> list[str]:
    """Compare canonical timer timing, not the original HA duration."""
    problems: list[str] = []
    if observed.get("state") != expected.get("state"):
        problems.append(
            f"state: expected {expected.get('state')!r}, got {observed.get('state')!r}"
        )
        return problems
    tolerance = int(expected.get("tolerance_seconds") or TIMER_TOLERANCE_FLOOR)
    if expected.get("state") == "running":
        source_due = int(expected.get("source_due_unix") or 0)
        actual_due = int(observed.get("due_unix") or 0)
        if source_due <= 0:
            problems.append("source due_unix is missing")
        elif abs(actual_due - source_due) > tolerance:
            problems.append(
                f"dueUnix: expected about {source_due}, got {actual_due} "
                f"(tolerance {tolerance}s)"
            )
    elif expected.get("state") == "paused":
        source_remaining = int(expected.get("source_remaining_seconds") or 0)
        actual_remaining = int(observed.get("remaining_seconds") or 0)
        if abs(actual_remaining - source_remaining) > tolerance:
            problems.append(
                f"remainingSeconds: expected about {source_remaining}, "
                f"got {actual_remaining} (tolerance {tolerance}s)"
            )
    return problems


def verify(
    client: ConnectClient, snapshot: dict[str, Any], mapping: dict[str, Any]
) -> tuple[list[tuple[str, str, str, str]], list[str]]:
    schedules = [_observed_schedule(item) for item in client.list_schedules()]
    timers = [_observed_timer(item) for item in client.list_timers()]
    schedules_by_id: dict[str, list[dict[str, Any]]] = {}
    for schedule in schedules:
        schedules_by_id.setdefault(schedule["id"], []).append(schedule)
    timers_by_id: dict[str, list[dict[str, Any]]] = {}
    for timer in timers:
        timers_by_id.setdefault(timer["id"], []).append(timer)

    rows: list[tuple[str, str, str, str]] = []
    failures: list[str] = []

    def compare(record: dict[str, Any], observed: dict[str, Any] | None) -> list[str]:
        expected = record.get("expected") or {}
        if observed is None:
            return ["resource not found"]
        problems: list[str] = []
        for field in ("label", "kind", "repeat", "at", "timezone", "enabled"):
            if field in expected and observed.get(field) != expected.get(field):
                problems.append(
                    f"{field}: expected {expected.get(field)!r}, got {observed.get(field)!r}"
                )
        if expected.get("type") == "schedule" and expected.get("repeat") == "once":
            if observed.get("once_date") != expected.get("once_date"):
                problems.append(
                    "onceDate: expected "
                    f"{expected.get('once_date')!r}, got {observed.get('once_date')!r}"
                )
        if expected.get("type") == "schedule" and expected.get("repeat") == "custom":
            if observed.get("weekdays") != expected.get("weekdays"):
                problems.append(
                    "weekdays: expected "
                    f"{expected.get('weekdays')!r}, got {observed.get('weekdays')!r}"
                )
        if expected.get("type") == "timer":
            problems.extend(_timer_problems(expected, observed))
        return problems

    for record in mapping["items"].values():
        if not record.get("verification_required") or not record.get("scheduler_id"):
            continue
        scheduler_id = str(record["scheduler_id"])
        expected = record.get("expected") or {}
        if expected.get("type") == "timer":
            matches = timers_by_id.get(scheduler_id, [])
        else:
            matches = schedules_by_id.get(scheduler_id, [])
        if len(matches) != 1:
            detail = f"expected exactly one resource, found {len(matches)}"
            rows.append((record.get("source_list") or "", scheduler_id, "MISMATCH", detail))
            failures.append(detail)
            record["verified"] = False
            record["verification_detail"] = detail
            continue
        problems = compare(record, matches[0])
        if problems:
            detail = "; ".join(problems)
            rows.append((record.get("source_list") or "", scheduler_id, "MISMATCH", detail))
            failures.append(f"{scheduler_id}: {detail}")
            record["verified"] = False
            record["verification_detail"] = detail
        else:
            rows.append((record.get("source_list") or "", scheduler_id, "OK", ""))
            record["verified"] = True
            record["verification_detail"] = ""

    for item in snapshot.get("items", []):
        if item.get("quarantine_reason") or (
            item.get("completed") and item.get("repeat") == "once"
        ):
            continue
        if item.get("kind") == "reminder" and item.get("repeat") != "once":
            # The scheduler rejects recurring reminders; the plan records this
            # as an intentional skip, not a silent import failure.
            continue
        record = mapping["items"].get(_source_key(item))
        if not record or not record.get("scheduler_id"):
            detail = "non-quarantined source item has no scheduler mapping"
            rows.append((item.get("source_list") or "", "-", "MISMATCH", detail))
            failures.append(detail)
    for timer in snapshot.get("timers", []):
        if timer.get("quarantine_reason") or timer.get("state") not in {"running", "paused"}:
            continue
        record = mapping["items"].get(_timer_key(timer))
        if record and record.get("action") == "skip" and record.get("reason") in {
            "skipped_expired",
            "missing_due_unix",
            "remaining_exceeds_max_timer_duration",
            "missing_scheduler_clock",
            "clock_skew_or_invalid_remaining",
        }:
            continue
        if not record or not record.get("scheduler_id"):
            detail = "active timer has no scheduler mapping"
            rows.append((timer.get("source_list") or "timer", "-", "MISMATCH", detail))
            failures.append(detail)

    print("verification:")
    print(f"{'SOURCE':32} {'SCHEDULER ID':24} {'RESULT':10} DETAIL")
    for source, scheduler_id, result, detail in rows:
        print(f"{source:32} {scheduler_id:24} {result:10} {detail}")
    return rows, failures


def _add_run_summary_to_mapping(
    mapping: dict[str, Any], snapshot: dict[str, Any], run_counts: dict[str, int]
) -> None:
    _summarize(mapping, snapshot, run_counts)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Migrate HA alarms/reminders/timers into ricardo.v1 SchedulerService."
    )
    parser.add_argument("--snapshot", required=True, help="normalized snapshot JSON")
    parser.add_argument("--endpoint", required=True, help="scheduler base URL")
    parser.add_argument("--token-file", required=True, help="file containing one bearer token")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--dry-run", action="store_true", help="plan only (default)")
    mode.add_argument("--execute", action="store_true", help="create resources and verify")
    parser.add_argument("--resume", action="store_true", help="skip already-mapped items")
    parser.add_argument(
        "--strict",
        action="store_true",
        help="exit non-zero when unresolved quarantined items remain",
    )
    parser.add_argument(
        "--allow-empty",
        action="store_true",
        help="permit a snapshot with zero source items",
    )
    parser.add_argument(
        "--map-out",
        default="scheduler-migration.map.json",
        help="mapping/audit JSON path (default: scheduler-migration.map.json)",
    )
    args = parser.parse_args(argv)
    execute = bool(args.execute)

    token = ""
    try:
        token = read_token(args.token_file)
        client = ConnectClient(args.endpoint, token)
        status = client.get_status()
        status_monotonic = time.monotonic()
        _print_status(status)
        try:
            server_now = int(status.get("nowUnix") or 0)
        except (TypeError, ValueError):
            server_now = 0
        snapshot = ha_snapshot.load_snapshot_json(args.snapshot)
        if (
            not snapshot.get("items")
            and not snapshot.get("timers")
            and not args.allow_empty
        ):
            raise MigrationError(
                "snapshot has no source items; refusing to treat an empty or "
                "stale snapshot as a successful import (pass --allow-empty only "
                "if this is intentional)"
            )
        plans = build_plans(snapshot, server_now, status_monotonic)
    except (MigrationError, ha_snapshot.SnapshotError, OSError) as exc:
        print(f"error: {_redact(exc, token)}", file=sys.stderr)
        return 2

    if not execute:
        print(f"dry-run: {len(plans)} source items; no writes")
        for plan in plans:
            _print_plan(plan)
        counts: dict[str, int] = {}
        for plan in plans:
            counts[plan["action"]] = counts.get(plan["action"], 0) + 1
        print(
            "dry-run summary: "
            f"create={counts.get('create', 0)} "
            f"unresolved={counts.get('quarantine_unresolved', 0)} "
            f"skip={counts.get('skip', 0)}"
        )
        return 0

    map_path = Path(args.map_out)
    try:
        mapping = _load_mapping(map_path) or _new_mapping(args.snapshot, snapshot)
        if not mapping.get("items"):
            mapping = _new_mapping(args.snapshot, snapshot)
        run_counts = execute_plans(
            client, plans, mapping, resume=bool(args.resume), map_path=map_path
        )
        rows, failures = verify(client, snapshot, mapping)
        run_counts["verification_failures"] = len(failures)
        _add_run_summary_to_mapping(mapping, snapshot, run_counts)
        _atomic_json(map_path, mapping)
    except (MigrationError, ha_snapshot.SnapshotError, OSError) as exc:
        print(f"error: {_redact(exc, token)}", file=sys.stderr)
        return 2

    summary = mapping["summary"]
    print(
        "summary: "
        f"created={summary['created']} replayed={summary['replayed']} "
        f"unresolved={summary['unresolved']} skipped={summary['skipped']} "
        f"resumed={summary['resumed']} mapped={summary['mapped_resources']} "
        f"verification_failures={summary['verification_failures']}"
    )
    print(f"mapping: {map_path}")
    if failures:
        print(f"migration verification failed: {len(failures)} mismatch(es)", file=sys.stderr)
        return 1
    if args.strict and summary["unresolved"]:
        print(
            f"strict mode: {summary['unresolved']} unresolved quarantined item(s)",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
