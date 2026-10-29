#!/usr/bin/env python3
"""Build a normalized, read-only snapshot of Home Assistant alarm/timer state.

The input is either a Home Assistant ``.storage`` directory containing
``local_todo.*.ics`` files and a ``timer`` JSON file, or a JSON document that
was produced earlier from the live API.  This module never talks to Home
Assistant and never writes to the scheduler.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

SCHEMA = "ricardo.ha_snapshot.v1"
DEFAULT_TIMEZONE = "Asia/Almaty"
KNOWN_REPEATS = {
    "alarms_once": "once",
    "alarms_daily": "daily",
    "alarms_weekdays": "weekdays",
    "alarms_weekends": "weekends",
    "reminders": "once",
}
WEEKDAYS = {
    "weekdays": [1, 2, 3, 4, 5],
    "weekends": [6, 7],
    "daily": [],
    "once": [],
}
_LIST_RE = re.compile(r"^local_todo\.(?P<list>.+)\.ics$")


class SnapshotError(Exception):
    """A snapshot input cannot be read or normalized."""


def zone(name: str | None, fallback: str = DEFAULT_TIMEZONE) -> ZoneInfo:
    """Return a validated IANA zone.  Offsets are deliberately rejected."""
    candidate = (name or fallback or "").strip()
    if not candidate or candidate[0] in "+-":
        raise SnapshotError(f"invalid IANA timezone {candidate!r}")
    try:
        return ZoneInfo(candidate)
    except (ZoneInfoNotFoundError, ValueError) as exc:
        raise SnapshotError(f"invalid IANA timezone {candidate!r}") from exc


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


def _unfold_ics(text: str) -> list[str]:
    lines: list[str] = []
    for raw in text.replace("\r\n", "\n").replace("\r", "\n").split("\n"):
        if raw.startswith((" ", "\t")) and lines:
            lines[-1] += raw[1:]
        elif raw:
            lines.append(raw)
    return lines


def _split_property(line: str) -> tuple[str, dict[str, str], str] | None:
    if ":" not in line:
        return None
    head, value = line.split(":", 1)
    parts = head.split(";")
    name = parts[0].strip().upper()
    params: dict[str, str] = {}
    for part in parts[1:]:
        if "=" in part:
            key, val = part.split("=", 1)
            params[key.strip().upper()] = val.strip().strip('"')
    return name, params, value


def _unescape(value: str) -> str:
    out: list[str] = []
    i = 0
    while i < len(value):
        char = value[i]
        if char == "\\" and i + 1 < len(value):
            nxt = value[i + 1]
            out.append({"n": "\n", "N": "\n", ",": ",", ";": ";", "\\": "\\"}.get(nxt, nxt))
            i += 2
        else:
            out.append(char)
            i += 1
    return "".join(out)


def _parse_ics_datetime(
    value: str, tz: ZoneInfo
) -> tuple[dt.datetime | None, str | None]:
    """Parse an ICS date/date-time.  A date without a time is not a clock."""
    text = value.strip()
    if not text:
        return None, "unparsable_due_time"
    if re.fullmatch(r"\d{8}", text):
        return None, "missing_due_time"
    utc = text.endswith("Z")
    body = text[:-1] if utc else text
    parsed: dt.datetime | None = None
    for fmt in ("%Y%m%dT%H%M%S", "%Y%m%dT%H%M"):
        try:
            parsed = dt.datetime.strptime(body, fmt)
            break
        except ValueError:
            continue
    if parsed is None:
        return None, "unparsable_due_time"
    if utc:
        return parsed.replace(tzinfo=dt.timezone.utc).astimezone(tz), None
    return parsed.replace(tzinfo=tz), None


@dataclass
class _IcsItem:
    uid: str
    label: str
    status: str
    dtstart: str | None
    dtstart_tzid: str | None
    due: str | None
    due_tzid: str | None
    description: str
    raw: str


def _parse_vtodos(text: str) -> list[_IcsItem]:
    lines = _unfold_ics(text)
    items: list[_IcsItem] = []
    current: list[str] | None = None
    for line in lines:
        if line.upper() == "BEGIN:VTODO":
            current = []
            continue
        if line.upper() == "END:VTODO":
            if current is not None:
                items.append(_parse_vtodo_block(current))
            current = None
            continue
        if current is not None:
            current.append(line)
    return items


def _parse_vtodo_block(lines: list[str]) -> _IcsItem:
    values: dict[str, tuple[dict[str, str], str]] = {}
    for line in lines:
        parsed = _split_property(line)
        if parsed is None:
            continue
        name, params, value = parsed
        values[name] = (params, value)
    uid = (values.get("UID", ({}, ""))[1] or "").strip()
    if not uid:
        digest = hashlib.sha256("\n".join(lines).encode("utf-8")).hexdigest()[:16]
        uid = f"missing-uid-{digest}"
    summary = values.get("SUMMARY", ({}, ""))[1]
    status = (values.get("STATUS", ({}, "NEEDS-ACTION"))[1] or "NEEDS-ACTION").upper()
    dtstart_params, dtstart = values.get("DTSTART", ({}, ""))
    due_params, due = values.get("DUE", ({}, ""))
    description = values.get("DESCRIPTION", ({}, ""))[1]
    return _IcsItem(
        uid=uid,
        label=_unescape(summary),
        status=status,
        dtstart=dtstart or None,
        dtstart_tzid=dtstart_params.get("TZID"),
        due=due or None,
        due_tzid=due_params.get("TZID"),
        description=_unescape(description),
        raw="\n".join(lines),
    )


def _alarm_kind(source_list: str) -> str:
    return "reminder" if source_list.startswith("reminders") else "alarm"


def _item_from_ics(
    source: _IcsItem, source_list: str, file_name: str, fallback_tz: str
) -> dict[str, Any]:
    repeat = KNOWN_REPEATS.get(source_list)
    tz_name = source.due_tzid or source.dtstart_tzid or fallback_tz
    quarantine: str | None = None
    tz: ZoneInfo | None
    try:
        tz = zone(tz_name)
    except SnapshotError:
        tz = None
        quarantine = "invalid_timezone"

    due_dt: dt.datetime | None = None
    if repeat is None:
        quarantine = "unknown_repeat_list"
    elif source.due is None:
        quarantine = quarantine or "missing_due_time"
    elif tz is not None:
        due_dt, reason = _parse_ics_datetime(source.due, tz)
        if reason:
            quarantine = quarantine or reason

    weekday_values = WEEKDAYS.get(repeat or "", [])
    at = None
    once_date = None
    if due_dt is not None:
        at = f"{due_dt.hour:02d}:{due_dt.minute:02d}:{due_dt.second:02d}"
        if repeat == "once":
            once_date = due_dt.date().isoformat()

    completed = source.status == "COMPLETED"
    item = {
        "source": file_name,
        "source_list": source_list,
        "uid": source.uid,
        "label": source.label,
        "kind": _alarm_kind(source_list),
        "repeat": repeat,
        "weekdays": list(weekday_values),
        "at": at,
        "once_date": once_date,
        "enabled": bool(not completed and quarantine is None),
        "completed": completed,
        "timezone": tz_name,
        "quarantine_reason": quarantine,
        "dtstart": source.dtstart,
        "due": source.due,
        "description": source.description,
    }
    if repeat == "custom":
        item["weekdays"] = []
    return item


def _parse_seconds(value: Any, *, allow_zero: bool = True) -> int | None:
    """Parse an int or HA's ``H:MM:SS`` duration/remaining string."""
    if isinstance(value, bool):
        return None
    if isinstance(value, (int, float)):
        seconds = int(value)
    else:
        text = str(value or "").strip()
        if not text:
            return None
        if text.isdigit():
            seconds = int(text)
        else:
            match = re.fullmatch(r"(?:(\d+):)?(\d{1,2}):(\d{2})(?:\.\d+)?", text)
            if not match:
                return None
            seconds = (
                int(match.group(1) or 0) * 3600
                + int(match.group(2)) * 60
                + int(match.group(3))
            )
    if seconds < 0 or (seconds == 0 and not allow_zero):
        return None
    return seconds


def _parse_duration(value: Any) -> int | None:
    seconds = _parse_seconds(value, allow_zero=False)
    return seconds if seconds and seconds > 0 else None


def _parse_int(value: Any) -> int:
    if value is None or value == "":
        return 0
    try:
        return int(str(value).strip())
    except (TypeError, ValueError):
        return 0


def _timer_state(value: Any) -> str:
    text = str(value or "idle").strip().lower().replace("timer_state_", "")
    return {"active": "running", "started": "running"}.get(text, text)


def _timer_from_dict(
    raw: dict[str, Any], index: int, fallback_tz: str
) -> dict[str, Any]:
    slot = str(
        raw.get("slot")
        or raw.get("id")
        or raw.get("uid")
        or raw.get("object_id")
        or f"slot-{index + 1}"
    )
    label = str(raw.get("label") or raw.get("name") or "")
    duration = _parse_duration(
        raw.get(
            "duration_seconds",
            raw.get("durationSeconds", raw.get("duration", raw.get("initial"))),
        )
    )
    state = _timer_state(raw.get("state"))
    remaining = _parse_seconds(
        raw.get("remaining_seconds", raw.get("remainingSeconds", raw.get("remaining", 0)))
    )
    if remaining is None:
        remaining = 0
    due_unix = _parse_int(raw.get("due_unix", raw.get("dueUnix", 0)))
    finishes_at = raw.get("finishes_at") or raw.get("finishesAt")
    if not due_unix and finishes_at:
        try:
            parsed = dt.datetime.fromisoformat(str(finishes_at).replace("Z", "+00:00"))
            due_unix = int(parsed.timestamp())
        except ValueError:
            due_unix = 0
    quarantine = None
    if duration is None:
        quarantine = "invalid_duration"
    elif state not in {"running", "paused", "idle", "fired", "cancelled"}:
        quarantine = "unknown_timer_state"
    return {
        "source": "timer",
        "source_list": "timer",
        "slot": slot,
        "uid": slot,
        "label": label,
        "kind": "timer",
        "duration_seconds": duration or 0,
        "state": state,
        "remaining_seconds": max(0, remaining),
        "due_unix": max(0, due_unix),
        "last_triggered": raw.get("last_triggered") or raw.get("lastTriggered"),
        "enabled": state in {"running", "paused"} and quarantine is None,
        "timezone": fallback_tz,
        "quarantine_reason": quarantine,
    }


def _timers_from_storage(data: dict[str, Any], fallback_tz: str) -> list[dict[str, Any]]:
    payload = data.get("data", data)
    if isinstance(payload, list):
        items = payload
    elif isinstance(payload, dict):
        items = payload.get("items", payload.get("timers", []))
    else:
        items = []
    if not isinstance(items, list):
        raise SnapshotError("timer file has no list of items")
    return [
        _timer_from_dict(item, index, fallback_tz)
        for index, item in enumerate(items)
        if isinstance(item, dict)
    ]


def _timers_from_ha_states(states: Iterable[Any], fallback_tz: str) -> list[dict[str, Any]]:
    timers: list[dict[str, Any]] = []
    for state in states:
        if not isinstance(state, dict):
            continue
        entity_id = str(state.get("entity_id", ""))
        if not entity_id.startswith("timer."):
            continue
        attributes = state.get("attributes") or {}
        timers.append(
            _timer_from_dict(
                {
                    "id": entity_id.split(".", 1)[1],
                    "name": attributes.get("friendly_name") or entity_id,
                    "duration": attributes.get("duration"),
                    "state": state.get("state"),
                    "remaining": attributes.get("remaining"),
                    "finishes_at": attributes.get("finishes_at"),
                },
                len(timers),
                fallback_tz,
            )
        )
    return timers


def _timers_from_restore_state(
    path: Path, fallback_tz: str
) -> list[dict[str, Any]]:
    """Extract timer runtime from HA ``core.restore_state``.

    HA stores the definitions (id/name/duration) in ``timer`` and the runtime
    state in restore entries.  Both the wrapped ``state`` object and a flat
    entity object are accepted.
    """
    data = _read_storage_file(path)
    if isinstance(data, list):
        payload: Any = data
    elif isinstance(data, dict):
        payload = data.get("data", data)
    else:
        return []
    # Real HA Store[list] shape is {"data": [entry, ...]}.  Keep accepting
    # older/alternate dict wrappers such as {"data": {"state": [...]}}.
    if isinstance(payload, list):
        entries = payload
    elif isinstance(payload, dict):
        entries = payload.get("state", payload.get("states", []))
    else:
        return []
    timers: list[dict[str, Any]] = []
    if not isinstance(entries, list):
        return timers
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        state_obj = entry.get("state")
        if not isinstance(state_obj, dict) or "entity_id" not in state_obj:
            state_obj = entry
        entity_id = str(state_obj.get("entity_id", ""))
        if not entity_id.startswith("timer."):
            continue
        attributes = state_obj.get("attributes") or {}
        timers.append(
            _timer_from_dict(
                {
                    "id": entity_id.split(".", 1)[1],
                    "name": attributes.get("friendly_name") or entity_id,
                    "duration": attributes.get("duration"),
                    "state": state_obj.get("state"),
                    "remaining": attributes.get("remaining"),
                    "finishes_at": attributes.get("finishes_at"),
                    "last_triggered": attributes.get("last_triggered"),
                },
                len(timers),
                fallback_tz,
            )
        )
    return timers


def _merge_timers(
    definitions: list[dict[str, Any]], runtime: list[dict[str, Any]]
) -> list[dict[str, Any]]:
    by_slot = {str(timer.get("slot")): timer for timer in runtime}
    merged: list[dict[str, Any]] = []
    seen: set[str] = set()
    for definition in definitions:
        slot = str(definition.get("slot"))
        item = dict(definition)
        state = by_slot.get(slot)
        if state is not None:
            seen.add(slot)
            item["state"] = state["state"]
            item["remaining_seconds"] = state["remaining_seconds"]
            item["due_unix"] = state["due_unix"]
            item["last_triggered"] = state.get("last_triggered")
            if not item.get("label"):
                item["label"] = state.get("label", "")
            if not item.get("duration_seconds"):
                item["duration_seconds"] = state.get("duration_seconds", 0)
        duration = int(item.get("duration_seconds") or 0)
        item_state = str(item.get("state") or "idle")
        if duration <= 0:
            item["quarantine_reason"] = "invalid_duration"
        elif item_state not in {"running", "paused", "idle", "fired", "cancelled"}:
            item["quarantine_reason"] = "unknown_timer_state"
        else:
            item["quarantine_reason"] = None
        item["enabled"] = (
            item_state in {"running", "paused"} and item["quarantine_reason"] is None
        )
        merged.append(item)
    for timer in runtime:
        if str(timer.get("slot")) not in seen:
            merged.append(dict(timer))
    return merged


def _read_storage_file(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise SnapshotError(f"cannot read JSON file {path.name}: {exc}") from exc


def build_snapshot_from_storage(
    storage_dir: str | os.PathLike[str], fallback_tz: str = DEFAULT_TIMEZONE
) -> dict[str, Any]:
    """Read HA ``.storage`` files without modifying them."""
    root = Path(storage_dir)
    if not root.is_dir():
        raise SnapshotError(f"storage directory does not exist: {root}")
    zone(fallback_tz)  # fail early on a bad CLI timezone

    items: list[dict[str, Any]] = []
    for path in sorted(root.glob("local_todo.*.ics")):
        match = _LIST_RE.match(path.name)
        if not match:
            continue
        source_list = match.group("list")
        if source_list.startswith("notes"):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as exc:
            raise SnapshotError(f"cannot read {path.name}: {exc}") from exc
        for vt in _parse_vtodos(text):
            items.append(_item_from_ics(vt, source_list, path.name, fallback_tz))

    definitions: list[dict[str, Any]] = []
    for path in (root / "timer", root / ".storage" / "timer"):
        if path.is_file():
            definitions = _timers_from_storage(
                _read_storage_file(path), fallback_tz
            )
            break

    restore: list[dict[str, Any]] = []
    for path in (root / "core.restore_state", root / ".storage" / "core.restore_state"):
        if path.is_file():
            restore = _timers_from_restore_state(path, fallback_tz)
            break
    timers = _merge_timers(definitions, restore)

    return {
        "schema": SCHEMA,
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "timezone": fallback_tz,
        "items": items,
        "timers": timers,
    }


def _normalize_at(value: Any) -> str | None:
    if value is None or value == "":
        return None
    if isinstance(value, str):
        parts = value.split(":")
        if len(parts) in {2, 3} and all(part.isdigit() for part in parts):
            hour, minute = int(parts[0]), int(parts[1])
            second = int(parts[2]) if len(parts) == 3 else 0
            if 0 <= hour <= 23 and 0 <= minute <= 59 and 0 <= second <= 59:
                return f"{hour:02d}:{minute:02d}:{second:02d}"
    if isinstance(value, dict):
        try:
            hour = int(value.get("hour", 0))
            minute = int(value.get("minute", 0))
            second = int(value.get("second", 0))
        except (TypeError, ValueError):
            return None
        if 0 <= hour <= 23 and 0 <= minute <= 59 and 0 <= second <= 59:
            return f"{hour:02d}:{minute:02d}:{second:02d}"
    return None


def _normalize_item(raw: dict[str, Any], fallback_tz: str) -> dict[str, Any]:
    item = dict(raw)
    item.setdefault("source", "snapshot")
    source_list = str(item.get("source_list") or item.get("list") or "snapshot")
    item["source_list"] = source_list
    item.setdefault("uid", "")
    item["uid"] = str(item.get("uid") or "")
    item.setdefault("label", "")
    item["label"] = str(item.get("label") or "")
    item.setdefault("kind", _alarm_kind(source_list))
    repeat = item.get("repeat")
    item["repeat"] = str(repeat).lower().replace("repeat_kind_", "") if repeat else None
    item["weekdays"] = [int(day) for day in item.get("weekdays") or []]
    item["at"] = _normalize_at(item.get("at"))
    item["once_date"] = item.get("once_date") or item.get("onceDate") or None
    item["completed"] = bool(item.get("completed", False))
    item["timezone"] = str(item.get("timezone") or fallback_tz)
    item["enabled"] = bool(
        item.get("enabled", not item["completed"] and not item.get("quarantine_reason"))
    )
    item["quarantine_reason"] = item.get("quarantine_reason") or None
    item.setdefault("dtstart", None)
    item.setdefault("due", None)
    item.setdefault("description", "")
    return item


def normalize_snapshot(data: Any, fallback_tz: str = DEFAULT_TIMEZONE) -> dict[str, Any]:
    """Accept our snapshot schema or a small set of live-API JSON shapes."""
    zone(fallback_tz)
    if isinstance(data, list):
        data = {"items": data}
    if not isinstance(data, dict):
        raise SnapshotError("snapshot JSON must be an object or list")

    if isinstance(data.get("states"), list):
        items = []
        timers = _timers_from_ha_states(data["states"], fallback_tz)
    else:
        items = data.get("items")
        if items is None:
            items = data.get("alarms", [])
            if isinstance(data.get("reminders"), list):
                items = list(items) + list(data["reminders"])
        timers = data.get("timers", [])
    if not isinstance(items, list) or not isinstance(timers, list):
        raise SnapshotError("snapshot JSON needs list fields 'items' and 'timers'")

    normalized_items = [
        _normalize_item(item, fallback_tz) for item in items if isinstance(item, dict)
    ]
    normalized_timers = [
        _timer_from_dict(item, index, fallback_tz)
        for index, item in enumerate(timers)
        if isinstance(item, dict)
    ]
    return {
        "schema": SCHEMA,
        "generated_at": data.get("generated_at")
        or dt.datetime.now(dt.timezone.utc).isoformat(),
        "timezone": str(data.get("timezone") or fallback_tz),
        "items": normalized_items,
        "timers": normalized_timers,
    }


def load_snapshot_json(
    path: str | os.PathLike[str], fallback_tz: str = DEFAULT_TIMEZONE
) -> dict[str, Any]:
    raw = _read_storage_file(Path(path))
    return normalize_snapshot(raw, fallback_tz)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Build a normalized, read-only HA scheduler snapshot."
    )
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--storage", help="HA .storage directory (local_todo*.ics + timer)")
    source.add_argument("--json", help="previously produced JSON snapshot or live-API JSON")
    parser.add_argument("--out", help="write snapshot JSON here (default: stdout)")
    parser.add_argument(
        "--allow-empty",
        action="store_true",
        help="allow a snapshot with zero alarm/reminder/timer source items",
    )
    parser.add_argument(
        "--timezone",
        default=DEFAULT_TIMEZONE,
        help=f"IANA fallback timezone (default: {DEFAULT_TIMEZONE})",
    )
    args = parser.parse_args(argv)
    try:
        if args.storage:
            snapshot = build_snapshot_from_storage(args.storage, args.timezone)
        else:
            snapshot = load_snapshot_json(args.json, args.timezone)
        if not snapshot["items"] and not snapshot["timers"] and not args.allow_empty:
            raise SnapshotError(
                "no source items found (no local_todo*.ics and no timer state); "
                "check that --storage points at HA's .storage directory, "
                "or pass --allow-empty explicitly"
            )
        if args.out:
            _atomic_json(Path(args.out), snapshot)
            print(
                f"wrote snapshot: {len(snapshot['items'])} alarm/reminder items, "
                f"{len(snapshot['timers'])} timer slots"
            )
        else:
            print(json.dumps(snapshot, ensure_ascii=False, indent=2))
    except (SnapshotError, OSError) as exc:
        print(f"snapshot error: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
