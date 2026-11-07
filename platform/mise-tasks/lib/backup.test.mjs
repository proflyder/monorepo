#!/usr/bin/env node
// Unit tests for the backup front door and its registry; run with `node --test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import {
  SSH_HOSTS,
  TARGETS,
  byId,
  resolveTargets,
  slugifyHeading,
  validateTargets,
} from "../../ops/backups.mjs";
import {
  actionCommand,
  listLines,
  nowPlanLines,
  parseBackupArgs,
  runList,
  runNow,
  runStatus,
  runVerify,
  verifyPlanLines,
} from "./backup.mjs";

function buffer() {
  return {
    chunks: [],
    write(value) {
      this.chunks.push(String(value));
      return true;
    },
    text() {
      return this.chunks.join("");
    },
  };
}

/** Fake spawn: answers are consumed in call order, calls are recorded. */
function fakeSpawn(answers = []) {
  const calls = [];
  const spawn = (command, args, options = {}) => {
    calls.push({ command, args, options });
    const answer = answers.shift() ?? { status: 0, stdout: "", stderr: "" };
    return { status: 0, stdout: "", stderr: "", ...answer };
  };
  spawn.calls = calls;
  return spawn;
}

const opts = (extra = {}) => ({
  root: "/repo",
  sshConfig: "/home/u/.ssh/home-lab/config",
  ...extra,
});

test("backup: registry validates against the real docs", () => {
  assert.deepEqual(validateTargets(), []);
  const ids = TARGETS.map((target) => target.id);
  assert.equal(new Set(ids).size, ids.length);
});

test("backup: documented targets are present", () => {
  for (const id of [
    "pve-vzdump",
    "ha-supervisor",
    "offsite-restic",
    "bb-db",
    "scheduler-db",
    "openbao-raft",
  ]) {
    assert.ok(byId(TARGETS, id), id);
  }
});

test("backup: every restore_doc names an existing docs file and heading", () => {
  for (const target of TARGETS) {
    assert.match(target.restore_doc, /^docs\/[^#]+\.md#.+$/, target.id);
  }
});

test("backup: ssh actions use only documented aliases", () => {
  for (const target of TARGETS) {
    for (const field of ["status", "run", "verify"]) {
      const action = target[field];
      if (action?.ssh) {
        assert.ok(SSH_HOSTS.includes(action.ssh), `${target.id}.${field}`);
      }
    }
  }
});

test("backup: slugifyHeading follows GitHub anchors", () => {
  assert.equal(slugifyHeading("Backup / restore"), "backup--restore");
  assert.equal(
    slugifyHeading("Offsite backup (Google Drive + restic)"),
    "offsite-backup-google-drive--restic",
  );
  assert.equal(slugifyHeading("Scheduler из SQLite"), "scheduler-из-sqlite");
});

test("backup: validation rejects duplicates, missing fields and bad docs links", () => {
  const readFile = () => "# Backup\n\n## Offsite backup (Google Drive + restic)\n";
  const fs = {
    existsSync: (target) => target.endsWith("OPERATIONS.md"),
    readFileSync: readFile,
  };
  const base = {
    id: "one",
    title: "t",
    schedule: "s",
    where: "w",
    status: null,
    run: null,
    verify: null,
    manual_note: "by hand",
    restore_doc: "docs/OPERATIONS.md#backup",
  };
  assert.deepEqual(validateTargets([base], { fs }), []);

  const problems = validateTargets(
    [
      base,
      { ...base, id: "one" },
      { ...base, id: "two", title: "" },
      { ...base, id: "three", restore_doc: "docs/OPERATIONS.md#nope" },
      { ...base, id: "four", restore_doc: "docs/MISSING.md#backup" },
      { ...base, id: "five", run: null, manual_note: "" },
      { ...base, id: "six", status: { ssh: "router", shell: "true" } },
      { ...base, id: "seven", verify: { shell: "" } },
    ],
    { fs },
  );
  assert.ok(problems.some((problem) => problem.includes("duplicate target ids: one")));
  assert.ok(problems.some((problem) => problem.includes("two: missing 'title'")));
  assert.ok(problems.some((problem) => problem.includes("three: restore_doc anchor")));
  assert.ok(problems.some((problem) => problem.includes("four: restore_doc file is missing")));
  assert.ok(problems.some((problem) => problem.includes("five: run is null but manual_note is empty")));
  assert.ok(problems.some((problem) => problem.includes("six: 'status' must be null")));
  assert.ok(problems.some((problem) => problem.includes("seven: 'verify' must be null")));
});

test("backup: resolveTargets defaults, expands all and rejects unknown ids", () => {
  assert.deepEqual(
    resolveTargets(TARGETS, [], { defaultAll: true }).map((target) => target.id),
    TARGETS.map((target) => target.id),
  );
  assert.throws(() => resolveTargets(TARGETS, []), /name a target/);
  assert.deepEqual(
    resolveTargets(TARGETS, ["bb-db,pve-vzdump"]).map((target) => target.id),
    ["pve-vzdump", "bb-db"],
  );
  assert.deepEqual(resolveTargets(TARGETS, ["all"]), TARGETS);
  assert.throws(() => resolveTargets(TARGETS, ["nope"]), /unknown target/);
  assert.throws(() => resolveTargets(TARGETS, ["all", "bb-db"]), /cannot be combined/);
});

test("backup: parseBackupArgs rejects unknown flags", () => {
  assert.deepEqual(parseBackupArgs(["bb-db", "--yes", "--dry-run"]), {
    targets: ["bb-db"],
    yes: true,
    dryRun: true,
    help: false,
  });
  assert.throws(() => parseBackupArgs(["--nope"]), /unknown flag/);
});

test("backup: actionCommand builds ssh and local commands", () => {
  assert.deepEqual(
    actionCommand({ ssh: "k3s", shell: "systemctl list-timers" }, opts()),
    {
      command: "ssh",
      args: [
        "-F",
        "/home/u/.ssh/home-lab/config",
        "k3s",
        "systemctl list-timers",
      ],
    },
  );
  assert.deepEqual(actionCommand({ shell: "true" }, opts()), {
    command: "bash",
    args: ["-c", "true"],
    cwd: "/repo",
  });
  assert.equal(actionCommand(null), null);
});

test("backup: list prints an offline table with every target", () => {
  const stdout = buffer();
  assert.equal(runList([], { stdout }), 0);
  const text = stdout.text();
  for (const target of TARGETS) {
    assert.ok(text.includes(target.id), target.id);
  }
  assert.ok(text.includes("schedule"));
  assert.ok(text.includes("where"));
});

test("backup: status --dry-run prints commands and never spawns", () => {
  const spawn = fakeSpawn();
  const stdout = buffer();
  assert.equal(runStatus(["--dry-run"], { spawn, stdout, ...opts() }), 0);
  assert.equal(spawn.calls.length, 0);
  assert.match(stdout.text(), /\[dry-run\] offsite-restic: ssh -F/);
  assert.match(stdout.text(), /\[dry-run\] openbao-raft: bash -c/);
  assert.match(stdout.text(), /\[manual\] ha-supervisor/);
  assert.match(stdout.text(), /dry-run: not probing/);
});

test("backup: status reports ok/fail/manual and exits non-zero on failure", () => {
  const stdout = buffer();
  const okSpawn = fakeSpawn([{ status: 0, stdout: "NEXT LAST\nlast line\n" }]);
  assert.equal(runStatus(["pve-vzdump"], { spawn: okSpawn, stdout, ...opts() }), 0);
  assert.deepEqual(okSpawn.calls[0].args.slice(-1), [
    "pvesh get /cluster/backup; pvesm list backup | tail",
  ]);
  assert.match(stdout.text(), /\[ok\] {3}pve-vzdump: last line/);

  const failStdout = buffer();
  const failSpawn = fakeSpawn([{ status: 1, stderr: "ssh: connect failed\n" }]);
  assert.equal(runStatus(["bb-db"], { spawn: failSpawn, stdout: failStdout, ...opts() }), 1);
  assert.match(failStdout.text(), /\[fail\] bb-db: ssh: connect failed/);
  assert.match(failStdout.text(), /1 fail/);

  const manualStdout = buffer();
  const manualSpawn = fakeSpawn();
  assert.equal(
    runStatus(["ha-supervisor"], { spawn: manualSpawn, stdout: manualStdout, ...opts() }),
    0,
  );
  assert.equal(manualSpawn.calls.length, 0);
  assert.match(manualStdout.text(), /1 manual/);
});

test("backup: now --dry-run prints the documented command and doesn't spawn", async () => {
  const spawn = fakeSpawn();
  const stdout = buffer();
  assert.equal(
    await runNow(["offsite-restic", "--dry-run"], { spawn, stdout, ...opts() }),
    0,
  );
  assert.equal(spawn.calls.length, 0);
  const text = stdout.text();
  assert.match(text, /backup now offsite-restic/);
  assert.match(text, /systemctl start offsite-backup\.service/);
  assert.match(text, /restore: docs\/OPERATIONS\.md#/);
  assert.match(text, /dry-run: nothing executed/);
});

test("backup: now runs only with --yes and never runs a UI-only target", async () => {
  const spawn = fakeSpawn([{ status: 0 }]);
  const stdout = buffer();
  assert.equal(
    await runNow(["pve-vzdump", "--yes"], { spawn, stdout, ...opts() }),
    0,
  );
  assert.equal(spawn.calls.length, 1);
  assert.equal(spawn.calls[0].command, "ssh");
  assert.match(spawn.calls[0].args.at(-1), /^vzdump 101 /);

  const declinedSpawn = fakeSpawn();
  const declinedStdout = buffer();
  assert.equal(
    await runNow(["pve-vzdump"], {
      spawn: declinedSpawn,
      stdout: declinedStdout,
      confirm: async () => false,
      ...opts(),
    }),
    1,
  );
  assert.equal(declinedSpawn.calls.length, 0);

  const manualSpawn = fakeSpawn();
  const manualStdout = buffer();
  assert.equal(
    await runNow(["ha-supervisor", "--yes"], {
      spawn: manualSpawn,
      stdout: manualStdout,
      ...opts(),
    }),
    0,
  );
  assert.equal(manualSpawn.calls.length, 0);
  assert.match(manualStdout.text(), /UI-only target; nothing was executed/);
});

test("backup: now needs exactly one known target", async () => {
  const stderr = buffer();
  assert.equal(
    await runNow([], { stdout: buffer(), stderr, spawn: fakeSpawn(), ...opts() }),
    2,
  );
  assert.match(stderr.text(), /name a target/);

  const unknownStderr = buffer();
  assert.equal(
    await runNow(["nope"], {
      stdout: buffer(),
      stderr: unknownStderr,
      spawn: fakeSpawn(),
      ...opts(),
    }),
    2,
  );
  assert.match(unknownStderr.text(), /unknown target/);
});

test("backup: now plan lists manual-only and restore docs", () => {
  const lines = nowPlanLines(byId(TARGETS, "ha-supervisor"), opts());
  assert.ok(lines.some((line) => line.includes("manual:")));
  assert.ok(lines.some((line) => line.includes("restore: docs/OPERATIONS.md#")));
});

test("backup: verify --dry-run prints checks and the restore doc, never spawns", async () => {
  const spawn = fakeSpawn();
  const stdout = buffer();
  assert.equal(
    await runVerify(["--dry-run"], { spawn, stdout, ...opts() }),
    0,
  );
  assert.equal(spawn.calls.length, 0);
  const text = stdout.text();
  assert.match(text, /restic check --read-data-subset=5%/);
  assert.match(text, /no automated check/);
  assert.match(text, /restore: docs\/REBUILD\.md#/);
  assert.match(text, /dry-run: nothing executed/);
});

test("backup: verify executes only documented checks after --yes", async () => {
  const spawn = fakeSpawn([{ status: 0 }, { status: 1 }]);
  const stdout = buffer();
  assert.equal(
    await runVerify(["bb-db,offsite-restic", "--yes"], {
      spawn,
      stdout,
      ...opts(),
    }),
    1,
  );
  // Registry order: offsite-restic first, then bb-db.
  assert.equal(spawn.calls.length, 2);
  assert.equal(spawn.calls[0].command, "ssh");
  assert.match(spawn.calls[0].args.at(-1), /restic check/);
  assert.match(spawn.calls[1].args.at(-1), /integrity_check/);
  assert.match(stdout.text(), /1 ok, 1 fail/);
});

test("backup: verify without executable checks does nothing", async () => {
  const spawn = fakeSpawn();
  const stdout = buffer();
  assert.equal(
    await runVerify(["ha-supervisor"], { spawn, stdout, ...opts() }),
    0,
  );
  assert.equal(spawn.calls.length, 0);
  assert.match(stdout.text(), /no automated verification/);
});

test("backup: verify plan marks missing checks per target", () => {
  const lines = verifyPlanLines(
    [byId(TARGETS, "scheduler-db"), byId(TARGETS, "openbao-raft")],
    opts(),
  );
  assert.ok(lines.some((line) => line.includes("scheduler-db: bash -c")));
  assert.ok(lines.some((line) => line.includes("openbao-raft: no automated check")));
});
