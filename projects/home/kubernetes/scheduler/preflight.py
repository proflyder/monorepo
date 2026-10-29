#!/usr/bin/env python3
"""Safety preflight for `deploy.sh --execute` (start the executor).

Checks (all read-only):
1. retired legacy HA schedule writers are absent or off (never on/unknown);
2. no alarm is currently ringing locally;
3. the scheduler API answers authenticated and ready;
4. HA's scheduler endpoint points at this scheduler.

The HA token is read from HA_TOKEN_FILE and never printed. Exit 1 on any
failed check.

Usage:
  preflight.py --ha-url URL --ha-token-file FILE \
               --scheduler-url URL --scheduler-token-file FILE \
               [--expected-endpoint URL]
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys
import urllib.error
import urllib.request

LEGACY_WRITERS = [
    "automation.alarm_scheduler",
    "automation.schedules_alarm_dispatcher",
    "automation.schedules_reminder_dispatcher",
    "automation.schedules_timer_finished",
]


def get(url: str, token: str) -> tuple[int, object]:
    req = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}"})
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            body = resp.read().decode()
            return resp.status, json.loads(body) if body else None
    except urllib.error.HTTPError as exc:
        return exc.code, None


def post(url: str, token: str, payload: dict) -> tuple[int, object]:
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode(),
        method="POST",
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            body = resp.read().decode()
            return resp.status, json.loads(body) if body else None
    except urllib.error.HTTPError as exc:
        return exc.code, None


def legacy_writer_failures(states: dict[str, str]) -> list[str]:
    """Return failures for writers that could still execute schedules.

    A missing entity is safe: the retired writer was removed from the shipped
    HA YAML on an upgraded install. An entity that exists must be exactly
    "off"; "on", "unknown", "unavailable" and any other value fail closed.
    """
    failures: list[str] = []
    for entity in LEGACY_WRITERS:
        state = states.get(entity)
        if state is None or state == "off":
            continue
        failures.append(
            f"legacy writer {entity} is {state!r}, expected absent or 'off'"
        )
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ha-url", required=True)
    parser.add_argument("--ha-token-file", required=True, type=pathlib.Path)
    parser.add_argument("--scheduler-url", required=True)
    parser.add_argument("--scheduler-token-file", required=True, type=pathlib.Path)
    parser.add_argument("--expected-endpoint")
    args = parser.parse_args()

    ha_token = args.ha_token_file.read_text().strip()
    scheduler_token = args.scheduler_token_file.read_text().strip()
    failures: list[str] = []

    code, states = get(args.ha_url.rstrip("/") + "/api/states", ha_token)
    if code != 200 or not isinstance(states, list):
        print(f"FAIL: cannot read HA states (HTTP {code})")
        return 1
    by_id = {s["entity_id"]: s["state"] for s in states}

    failures.extend(legacy_writer_failures(by_id))

    if by_id.get("input_boolean.alarm_ringing") == "on":
        failures.append("an alarm is ringing in HA; stop it before starting the executor")

    if args.expected_endpoint:
        actual = by_id.get("input_text.scheduler_endpoint")
        if actual != args.expected_endpoint:
            failures.append(
                "input_text.scheduler_endpoint is "
                f"{actual!r}, expected {args.expected_endpoint!r}"
            )

    code, status = post(
        args.scheduler_url.rstrip("/")
        + "/ricardo.v1.SchedulerService/GetStatus",
        scheduler_token,
        {},
    )
    if code != 200 or not isinstance(status, dict):
        failures.append(f"scheduler GetStatus failed (HTTP {code})")
    elif not status.get("ready"):
        failures.append("scheduler reports ready=false")

    if failures:
        for failure in failures:
            print("FAIL:", failure)
        return 1

    print("preflight ok: retired writers absent/off, no ringing alarm, scheduler ready")
    return 0


if __name__ == "__main__":
    sys.exit(main())
