# Home Assistant scheduler migration

Read-only snapshot and resumable import of Home Assistant alarm, reminder and
timer state into `ricardo.v1.SchedulerService`.

## What it reads

`ha_snapshot.py` reads a copy of the HA `.storage` directory:

- `local_todo.alarms_once.ics`
- `local_todo.alarms_daily.ics`
- `local_todo.alarms_weekdays.ics`
- `local_todo.reminders.ics`
- `timer` (timer definitions)
- `core.restore_state` (timer runtime: `state`, `finishes_at`, `remaining`,
  `last_triggered`)

Timer definitions are merged with restore-state runtime. It parses VTODO
`UID`, `SUMMARY`, `STATUS`, `DUE` (including `TZID`), `DTSTART`, and
`DESCRIPTION`. It also accepts an existing JSON snapshot, or an HA states JSON
containing `timer.*` entities. The timezone must be an IANA name; fixed offsets
are rejected.

The normalized snapshot keeps raw `dtstart` and `due` evidence and adds
`source_list`, `uid`, `label`, `kind`, `repeat`, `weekdays`, `at`, `once_date`,
`enabled`, `timezone`, and `quarantine_reason` per item. Timer slots carry
`duration_seconds`, `state`, `remaining_seconds`, and `due_unix`. HA's
`H:MM:SS` remaining format is supported.

## Commands

Build the snapshot before touching the scheduler:

```bash
python3 projects/home/scheduler-migration/ha_snapshot.py \
  --storage /path/to/ha/.storage \
  --out /secure/local/ha-scheduler-snapshot.json
```

Dry-run is the default and performs **no scheduler writes**:

```bash
python3 projects/home/scheduler-migration/migrate.py \
  --snapshot /secure/local/ha-scheduler-snapshot.json \
  --endpoint http://192.0.2.10:8080 \
  --token-file /secure/local/scheduler-token \
  --dry-run
```

Execute, then re-list and verify:

```bash
python3 projects/home/scheduler-migration/migrate.py \
  --snapshot /secure/local/ha-scheduler-snapshot.json \
  --endpoint http://192.0.2.10:8080 \
  --token-file /secure/local/scheduler-token \
  --execute \
  --map-out /secure/local/scheduler-migration.map.json
```

Resume after an interruption:

```bash
python3 projects/home/scheduler-migration/migrate.py \
  --snapshot /secure/local/ha-scheduler-snapshot.json \
  --endpoint http://192.0.2.10:8080 \
  --token-file /secure/local/scheduler-token \
  --execute --resume \
  --map-out /secure/local/scheduler-migration.map.json
```

The process exits non-zero on a verification mismatch. Unresolved quarantined
items are reported as `unresolved=N`; they do not fail a healthy import by
default. Add `--strict` to make any unresolved item exit non-zero. The token
file is read only; its value is never printed.

A snapshot with zero source items is rejected by both tools so an empty or
stale directory cannot look like a successful import. `--allow-empty` is only
for deliberate empty checks.

## Import rules

Alarm and reminder idempotency keys are `ha:<source_list>:<uid>`; timers use
`ha:timer:<slot>`. Re-running an execute replays existing resources instead of
creating duplicates. The mapping JSON records every source item, its scheduler
id, action, reason, expected fields, and verification result.

- A source item without `DUE` is **never written to the scheduler**, not even as
  a disabled schedule: a disabled schedule would still carry a fabricated wall
  time that a client could later enable. It stays only in the mapping ledger as
  `status=quarantined_unresolved`, reason `missing_due_time`, with source list,
  uid, label, repeat, timezone, `DTSTART`, and `due=null` preserved for audit.
- Unparsable `DUE` is quarantined the same way. Unknown repeat lists and invalid
  timezones are recorded as skipped because they cannot be represented.
- Completed one-shot items are recorded as skipped, not recreated.
- Reminders are imported as one-shot schedules; the scheduler contract rejects
  recurring reminders.
- Idle, fired and cancelled timers are skipped. Only running and paused timers
  are started, using their remaining time.
- For `REPEAT_KIND_WEEKDAYS`, the snapshot keeps ISO weekdays `[1..5]` as audit
  evidence, but the wire omits `weekdays` because the scheduler only accepts an
  explicit weekday list for `REPEAT_KIND_CUSTOM`.

## Cutover order

1. Quiesce HA alarm/reminder/timer writes and the old execution path.
2. Take a snapshot.
3. Run the default dry-run and inspect the plan.
4. Run `--execute`; wait for verification to pass.
5. Switch the executor only after the mapping and verification table are
   accepted.

## Rollback

The scheduler keeps its data. Stop the new bridge/scheduler before re-enabling
HA execution, so two executors never own one live schedule. Do not delete the
mapping: scheduler ids are new, so the mapping file is the source for reverse
reconciliation to HA uids.

## Timer countdown semantics

`StartTimer` only accepts a duration and derives `due_unix` from the server
clock. Migration therefore imports the **remaining** time, never the original
HA duration:

- Running: `remaining = source due_unix - server now_unix` from `GetStatus`.
  Expired, missing-due, or over-limit timers are skipped and never written.
- Paused: the source `remaining_seconds` is started and immediately paused, then
  the timer is re-read and must be `PAUSED` with the expected remaining time.
- The original HA duration is preserved only as `source_duration_seconds` in the
  mapping ledger. Verification compares running `dueUnix` and paused
  `remainingSeconds` within a measured latency tolerance, not the original
  duration.
- A mapped timer id that still exists is re-read on a later execute, not
  restarted with a changed duration. Active timers are still short-lived, so
  recheck them at cutover.

## Tests

```bash
python3 -m unittest discover -s projects/home/scheduler-migration -p 'test_*.py' -v
python3 -m py_compile projects/home/scheduler-migration/ha_snapshot.py \
  projects/home/scheduler-migration/migrate.py \
  projects/home/scheduler-migration/test_migration.py
```

The tests use only synthetic labels and a fake Connect JSON server on loopback.
