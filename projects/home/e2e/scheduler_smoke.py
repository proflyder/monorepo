#!/usr/bin/env python3
"""Local process E2E: real scheduler + real ha-bridge + fake Home Assistant.

This test is hermetic: it builds both Go binaries into a temporary directory,
starts the scheduler with a temporary SQLite database and dummy bearer tokens,
and serves a fake HA on loopback. It never contacts live HA, ADB or
the station, and it never touches a production scheduler database.

What it verifies end to end:

* scheduler rejects a missing and a wrong bearer token;
* bridge dry-run makes only a read-only HA probe and leases nothing;
* a real timer FIRE travels scheduler -> bridge -> fake HA script and is
  acknowledged as DELIVERED;
* CancelTimer produces STOP (not snooze) and is delivered;
* a generic task stays on the ``tasks`` queue: the bridge leases only ``home``;
* both processes shut down gracefully on SIGTERM.

Audible/live acceptance and HA shadow import are explicitly out of scope.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import signal
import subprocess
import sys
import tempfile
import threading
import time
import traceback
import urllib.error
import urllib.request
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SERVICE_PREFIX = "/ricardo.v1.SchedulerService/"
SCHEDULER_TOKEN = "e2e-scheduler-token"
WRONG_TOKEN = "e2e-wrong-token"
HA_TOKEN = "e2e-ha-token"
CONSUMER = "e2e-ha-bridge"
POLL = 0.05
# Deployment configuration must never leak into this hermetic test: a local
# *_TOKEN_FILE, consumer allowlist, DB path or bridge queue would change what
# is being exercised. Keep the normal process environment (PATH, Go caches,
# proxies, locale) and drop only scheduler/bridge/HA settings.
CONFIG_PREFIXES = ("SCHEDULER_", "ALARM_", "BRIDGE_", "HA_")


def clean_env() -> dict[str, str]:
    return {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(CONFIG_PREFIXES)
    }


class Failure(RuntimeError):
    pass


def check(condition: bool, message: str) -> None:
    if not condition:
        raise Failure(message)


def wait_until(getter, timeout: float, description: str):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        last = getter()
        if last:
            return last
        time.sleep(POLL)
    raise Failure(f"timed out after {timeout:.0f}s waiting for {description} (last={last!r})")


def rpc(base: str, method: str, payload: dict, token: str | None = SCHEDULER_TOKEN, timeout: float = 5.0):
    body = json.dumps(payload).encode()
    request = urllib.request.Request(
        base + SERVICE_PREFIX + method,
        data=body,
        method="POST",
        headers={"Content-Type": "application/json", "Connect-Protocol-Version": "1"},
    )
    if token:
        request.add_header("Authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            raw = response.read()
            return response.status, json.loads(raw) if raw else {}
    except urllib.error.HTTPError as error:
        raw = error.read()
        try:
            decoded = json.loads(raw) if raw else {}
        except json.JSONDecodeError:
            decoded = {}
        return error.code, decoded


class FakeHA:
    """Minimal HA REST surface: read-only probe and script.turn_on."""

    def __init__(self) -> None:
        self.records: list[dict] = []
        self._lock = threading.Lock()
        self._server: ThreadingHTTPServer | None = None
        self._thread: threading.Thread | None = None
        self.url = ""

    def start(self) -> None:
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args) -> None:  # keep test output clean
                pass

            def _authorized(self) -> bool:
                return self.headers.get("Authorization") == "Bearer " + HA_TOKEN

            def _reply(self, status: int, payload=None) -> None:
                data = b"" if payload is None else json.dumps(payload).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                if data:
                    self.wfile.write(data)

            def do_GET(self) -> None:
                if self.path != "/api/":
                    self._reply(404, {"message": "not found"})
                    return
                if not self._authorized():
                    self._reply(401, {"message": "unauthorized"})
                    return
                self._reply(200, {"message": "API running."})

            def do_POST(self) -> None:
                if self.path != "/api/services/script/turn_on":
                    self._reply(404, {"message": "not found"})
                    return
                if not self._authorized():
                    self._reply(401, {"message": "unauthorized"})
                    return
                length = int(self.headers.get("Content-Length") or 0)
                try:
                    body = json.loads(self.rfile.read(length) or b"{}")
                except json.JSONDecodeError:
                    self._reply(400, {"message": "invalid json"})
                    return
                with owner._lock:
                    owner.records.append(body)
                self._reply(200, [])

        self._server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)
        self._thread.start()
        self.url = f"http://127.0.0.1:{self._server.server_address[1]}"

    def stop(self) -> None:
        if self._server is not None:
            self._server.shutdown()
            self._server.server_close()
            self._server = None
        if self._thread is not None:
            self._thread.join(timeout=5)
            self._thread = None

    def script_calls(self) -> list[dict]:
        with self._lock:
            return list(self.records)

    def find_script(self, entity: str):
        return next((call for call in self.script_calls() if call.get("entity_id") == entity), None)


class Process:
    """Subprocess with a bounded log buffer and process-group cleanup."""

    def __init__(self, name: str, argv: list[str], env: dict[str, str]) -> None:
        self.name = name
        self.lines: list[str] = []
        self.proc = subprocess.Popen(
            argv,
            cwd=ROOT,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
            start_new_session=True,
        )
        self._thread = threading.Thread(target=self._pump, daemon=True)
        self._thread.start()

    def _pump(self) -> None:
        assert self.proc.stdout is not None
        for line in self.proc.stdout:
            self.lines.append(line.rstrip())
            if len(self.lines) > 400:
                del self.lines[:100]

    def output(self) -> str:
        return "\n".join(self.lines[-25:])

    def stop(self, timeout: float = 10.0) -> int:
        if self.proc.poll() is None:
            self.proc.send_signal(signal.SIGTERM)
        try:
            return self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            if self.proc.poll() is None:
                os.killpg(self.proc.pid, signal.SIGKILL)
            return self.proc.wait(timeout=5)


def build_binaries(target: Path) -> tuple[Path, Path]:
    binaries = {
        "scheduler": "./solutions/scheduler/cmd/scheduler",
        "ha-bridge": "./solutions/ha-bridge/cmd/ha-bridge",
    }
    built: dict[str, Path] = {}
    for name, package in binaries.items():
        destination = target / name
        result = subprocess.run(
            ["go", "build", "-o", str(destination), package],
            cwd=ROOT,
            env=clean_env(),
            text=True,
            capture_output=True,
            timeout=300,
        )
        if result.returncode != 0:
            raise Failure(f"go build {package} failed:\n{result.stdout}\n{result.stderr}")
        built[name] = destination
    return built["scheduler"], built["ha-bridge"]


def scheduler_base(process: Process, timeout: float = 30.0) -> str:
    pattern = re.compile(r"addr=([0-9.]+:\d+)")
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for line in list(process.lines):
            match = pattern.search(line)
            if match:
                return "http://" + match.group(1)
        if process.proc.poll() is not None:
            raise Failure(f"scheduler exited early:\n{process.output()}")
        time.sleep(POLL)
    raise Failure(f"scheduler did not report a listen address:\n{process.output()}")


def queue_state(status: dict, name: str) -> tuple[int, int]:
    for queue in status.get("queues") or []:
        if queue.get("name") == name:
            return int(queue.get("pending") or 0), int(queue.get("leased") or 0)
    return 0, 0


def consumer_status(status: dict) -> tuple[int, int]:
    for consumer in status.get("consumers") or []:
        if consumer.get("consumerId") == CONSUMER:
            return int(consumer.get("lastAckUnix") or 0), int(consumer.get("lastSeenUnix") or 0)
    return 0, 0


def run(verbose: bool) -> None:
    with tempfile.TemporaryDirectory(prefix="scheduler-e2e-") as tmp_name:
        tmp = Path(tmp_name)
        fake = FakeHA()
        fake.start()
        scheduler = bridge = None
        try:
            scheduler_bin, bridge_bin = build_binaries(tmp)
            scheduler_env = clean_env()
            scheduler_env.update(
                {
                    "SCHEDULER_LISTEN": "127.0.0.1:0",
                    "SCHEDULER_DB_PATH": str(tmp / "scheduler.db"),
                    "SCHEDULER_TIMEZONE": "UTC",
                    "SCHEDULER_TOKEN": SCHEDULER_TOKEN,
                    "SCHEDULER_TICK_MS": "100",
                }
            )
            scheduler = Process("scheduler", [str(scheduler_bin)], scheduler_env)
            base = scheduler_base(scheduler)
            if verbose:
                print(f"scheduler listening at {base}")

            code, _ = rpc(base, "GetStatus", {})
            check(code == 200, f"GetStatus returned HTTP {code}")
            for label, token in (("missing", None), ("wrong", WRONG_TOKEN)):
                code, _ = rpc(base, "ListDueEvents", {"consumerId": "e2e-probe", "queue": "home"}, token=token)
                check(code == 401, f"{label} token must be rejected with 401, got {code}")

            bridge_env = clean_env()
            bridge_env.update(
                {
                    "HA_URL": fake.url,
                    "HA_TOKEN": HA_TOKEN,
                    "SCHEDULER_ENDPOINT": base,
                    "SCHEDULER_TOKEN": SCHEDULER_TOKEN,
                    "BRIDGE_CONSUMER_ID": CONSUMER,
                    "BRIDGE_QUEUE": "home",
                    "BRIDGE_BATCH_SIZE": "5",
                    "BRIDGE_LEASE_SECONDS": "60",
                    "BRIDGE_POLL_INTERVAL": "100ms",
                    "BRIDGE_MAX_BACKOFF": "2s",
                    "BRIDGE_HA_TIMEOUT": "2s",
                    "BRIDGE_SCHEDULER_TIMEOUT": "2s",
                    "BRIDGE_MAX_ATTEMPTS": "5",
                    "BRIDGE_ALARM_SCRIPT": "alarm_start",
                    "BRIDGE_ANNOUNCE_SCRIPT": "schedule_announce",
                    "BRIDGE_STOP_SCRIPT": "alarm_stop",
                }
            )

            # 1. Dry-run: only the read-only probe, no lease, no script call.
            dry_env = dict(bridge_env)
            dry_env.pop("BRIDGE_EXECUTE", None)
            dry_run = subprocess.run(
                [str(bridge_bin)], cwd=ROOT, env=dry_env, text=True, capture_output=True, timeout=60
            )
            check(
                dry_run.returncode == 0,
                f"bridge dry-run failed ({dry_run.returncode}):\n{dry_run.stdout}\n{dry_run.stderr}",
            )
            check(not fake.script_calls(), "bridge dry-run called an HA script")

            # 2. Execute: a real timer must ring through the fake HA script.
            bridge_env["BRIDGE_EXECUTE"] = "true"
            bridge_env["BRIDGE_LEDGER_PATH"] = str(tmp / "ledger.json")
            bridge = Process("ha-bridge", [str(bridge_bin)], bridge_env)

            def home_idle() -> bool:
                code, state = rpc(base, "GetStatus", {})
                if code != 200:
                    return False
                pending, leased = queue_state(state, "home")
                # No pending *and* no leased event: an earlier FIRE ack must not
                # mask a still-leased STOP.
                return pending == 0 and leased == 0

            def home_idle_after(ack_floor: int) -> bool:
                code, state = rpc(base, "GetStatus", {})
                if code != 200:
                    return False
                last_ack, _ = consumer_status(state)
                return home_idle() and last_ack > ack_floor

            def ack_now() -> int:
                code, state = rpc(base, "GetStatus", {})
                if code != 200:
                    return -1
                return consumer_status(state)[0]

            ack_floor = ack_now()
            code, response = rpc(base, "StartTimer", {
                "label": "E2E timer",
                "durationSeconds": "1",
                "idempotencyKey": "e2e-timer",
            })
            check(code == 200, f"StartTimer returned HTTP {code}: {response}")
            timer_id = (response.get("timer") or {}).get("id")
            check(bool(timer_id), f"StartTimer returned no timer id: {response}")

            announce = wait_until(
                lambda: fake.find_script("script.schedule_announce"), 30, "timer announce script call"
            )
            variables = announce.get("variables") or {}
            check(variables.get("message") == "E2E timer", f"announce message = {variables.get('message')!r}")
            check(variables.get("chime") is True, f"announce chime = {variables.get('chime')!r}")
            check(variables.get("kind") == "timer", f"announce kind = {variables.get('kind')!r}")

            wait_until(lambda: home_idle_after(ack_floor), 20, "timer FIRE DELIVERED (home pending=0, leased=0)")
            if verbose:
                print("timer FIRE delivered and acknowledged")

            # 3. Cancel the fired timer: backend state is cancelled, but a
            # one-shot announcement has no safe global audio stop, so the
            # bridge must not call alarm_stop.
            ack_floor = ack_now()
            code, response = rpc(base, "CancelTimer", {"id": timer_id})
            check(code == 200, f"CancelTimer returned HTTP {code}: {response}")
            # ACK timestamps have second granularity, so for a STOP the proof is
            # that the queued event is consumed (pending=0, leased=0) and no
            # audio stop was sent; an earlier FIRE ack cannot satisfy a lease.
            wait_until(home_idle, 20, "timer STOP consumed without audio stop")
            check(
                fake.find_script("script.alarm_stop") is None,
                "timer cancellation called the global alarm stop; it may silence an unrelated alarm",
            )

            # 4. A real one-shot alarm: FIRE starts the alarm script, then
            # StopSchedule must stop exactly that active alarm.
            ack_floor = ack_now()
            now = datetime.now(timezone.utc)
            target = (now + timedelta(seconds=60)).replace(second=0, microsecond=0)
            if target < now + timedelta(seconds=5):
                target += timedelta(minutes=1)
            code, response = rpc(base, "CreateSchedule", {
                "label": "E2E alarm",
                "kind": "SCHEDULE_KIND_ALARM",
                "timezone": "UTC",
                "at": {"hour": target.hour, "minute": target.minute, "second": 0},
                "onceDate": target.strftime("%Y-%m-%d"),
                "repeat": "REPEAT_KIND_ONCE",
                "enabled": True,
                "payloadJson": "{}",
            })
            check(code == 200, f"CreateSchedule returned HTTP {code}: {response}")
            schedule = response.get("schedule") or {}
            schedule_id = schedule.get("id")
            check(bool(schedule_id), f"CreateSchedule returned no id: {response}")
            due = int(schedule.get("nextDueUnix") or 0)
            alarm_wait = max(30.0, due - time.time() + 30.0)

            alarm = wait_until(
                lambda: fake.find_script("script.alarm_start"), alarm_wait, "alarm_start script call"
            )
            check((alarm.get("variables") or {}).get("label") == "E2E alarm", "alarm_start call mismatch")
            wait_until(lambda: home_idle_after(ack_floor), 20, "alarm FIRE DELIVERED")

            ack_floor = ack_now()
            code, response = rpc(base, "StopSchedule", {"id": schedule_id})
            check(code == 200, f"StopSchedule returned HTTP {code}: {response}")
            stop = wait_until(lambda: fake.find_script("script.alarm_stop"), 20, "alarm STOP script call")
            stop_variables = stop.get("variables") or {}
            check(
                stop_variables.get("reason") == "scheduler_stop",
                f"stop reason = {stop_variables.get('reason')!r}",
            )
            check(
                stop_variables.get("source_id") == schedule_id,
                f"stop source = {stop_variables.get('source_id')!r}, want {schedule_id}",
            )
            wait_until(home_idle, 20, "alarm STOP consumed (home pending=0, leased=0)")
            if verbose:
                print("one-shot alarm fired and StopSchedule stopped it")

            # 4. Generic task must stay on the tasks queue.
            run_at = int(time.time()) + 1
            code, response = rpc(base, "EnqueueTask", {
                "taskType": "e2e.noop",
                "payloadJson": json.dumps({"note": "must-not-execute"}),
                "runAtUnix": str(run_at),
                "queue": "tasks",
                "idempotencyKey": "e2e-task",
            })
            check(code == 200, f"EnqueueTask returned HTTP {code}: {response}")
            event = response.get("event") or {}
            check(event.get("queue") == "tasks", f"task queue = {event.get('queue')!r}")

            def bridge_saw_due_task() -> bool:
                code, state = rpc(base, "GetStatus", {})
                if code != 200:
                    return False
                now = int(state.get("nowUnix") or 0)
                _, last_seen = consumer_status(state)
                # The bridge must have completed a lease poll strictly after
                # the task's due second, so it had a real chance to steal it.
                return now >= run_at and last_seen >= run_at + 1

            wait_until(bridge_saw_due_task, 25, "task due and bridge polling after it")
            code, state = rpc(base, "GetStatus", {})
            check(code == 200, f"GetStatus returned HTTP {code}")
            pending, leased = queue_state(state, "tasks")
            check(pending == 1, f"tasks pending = {pending}, want exactly the queued task")
            check(leased == 0, f"tasks leased = {leased}; the bridge leased a generic task")
            scripts = [call.get("entity_id") for call in fake.script_calls()]
            check(
                scripts == ["script.schedule_announce", "script.alarm_start", "script.alarm_stop"],
                f"unexpected HA script calls: {scripts}",
            )
            check(
                not any("snooze" in (entity or "") for entity in scripts),
                "bridge called a snooze script; snooze must stay in the scheduler",
            )

            if verbose:
                print("generic task stayed on tasks; bridge leased only home")

            # 5. Graceful shutdown.
            code = bridge.stop()
            check(code == 0, f"bridge exited with {code}:\n{bridge.output()}")
            bridge = None
            code = scheduler.stop()
            check(code == 0, f"scheduler exited with {code}:\n{scheduler.output()}")
            scheduler = None
        finally:
            if bridge is not None:
                print("bridge log:\n" + bridge.output(), file=sys.stderr)
                bridge.stop()
            if scheduler is not None:
                print("scheduler log:\n" + scheduler.output(), file=sys.stderr)
                scheduler.stop()
            fake.stop()


def main() -> int:
    parser = argparse.ArgumentParser(description="Local scheduler + ha-bridge process E2E")
    parser.add_argument("-v", "--verbose", action="store_true")
    args = parser.parse_args()
    try:
        run(args.verbose)
    except Failure as failure:
        print(f"E2E FAILED: {failure}", file=sys.stderr)
        return 1
    except Exception:  # noqa: BLE001 - report any unexpected error as a test failure
        traceback.print_exc()
        return 1
    print("scheduler + ha-bridge E2E passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
