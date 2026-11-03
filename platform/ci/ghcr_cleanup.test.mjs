// GHCR cleanup policy tests (DECISIONS 36).
//
// The selection is a pure function, so these tests pin exactly which package
// versions may be deleted: stale `tree-*` tags and stale untagged versions, and
// nothing else. The package list itself is derived from platform/ci/units.mjs,
// so a non-deploy package (the Windows logon provider) can never be touched.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  DAY_MS,
  deployImagePackages,
  formatPackageSummary,
  main,
  planPackageCleanup,
} from "./ghcr_cleanup.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const NOW = Date.parse("2026-11-04T00:00:00Z");
const TREE = (char) => `tree-${char.repeat(40)}`;

function version(id, created, tags) {
  return {
    id,
    created_at: created,
    metadata: { container: { tags } },
  };
}

function summary(plan) {
  return Object.fromEntries(plan.decisions.map((item) => [item.id, item]));
}

test("only the deploy images of units.mjs are considered", () => {
  assert.deepEqual(deployImagePackages(), [
    "currency-bot",
    "ha-bridge",
    "ha-control",
    "hermes",
    "mcp-gateway",
    "scheduler",
    "voice-proxy",
    "wakegrant",
  ]);
  assert.ok(!deployImagePackages().includes("windows-logon-provider"));
});

test("stale tree-tagged versions are deleted, the newest is kept", () => {
  const plan = planPackageCleanup(
    [
      version(3, "2026-11-03T00:00:00Z", [TREE("c")]),
      version(2, "2026-10-01T00:00:00Z", [TREE("b")]),
      version(1, "2026-09-01T00:00:00Z", [TREE("a")]),
    ],
    { now: NOW },
  );
  assert.deepEqual(plan.delete.map((item) => item.id), [2, 1]);
  assert.equal(summary(plan)[3].reason, "newest version of the package");
});

test("versions younger than the window are kept", () => {
  const plan = planPackageCleanup(
    [
      version(2, "2026-11-04T00:00:00Z", [TREE("b")]),
      version(1, "2026-10-31T00:00:00Z", [TREE("a")]),
    ],
    { now: NOW },
  );
  assert.equal(plan.delete.length, 0);
  assert.match(summary(plan)[1].reason, /younger than 7 days/);
});

test("exactly seven days old is old enough", () => {
  const sevenDays = new Date(NOW - 7 * DAY_MS).toISOString();
  const plan = planPackageCleanup(
    [
      version(2, "2026-11-03T00:00:00Z", [TREE("b")]),
      version(1, sevenDays, [TREE("a")]),
    ],
    { now: NOW },
  );
  assert.deepEqual(plan.delete.map((item) => item.id), [1]);
});

test("deploy tags, commit shas and semver releases are never deleted", () => {
  const plan = planPackageCleanup(
    [
      version(5, "2026-10-20T00:00:00Z", [TREE("e")]),
      version(4, "2026-08-01T00:00:00Z", ["develop"]),
      version(3, "2026-08-01T00:00:00Z", ["develop", TREE("d")]),
      version(2, "2026-08-01T00:00:00Z", ["0123456789abcdef0123456789abcdef01234567"]),
      version(1, "2026-08-01T00:00:00Z", ["v1.2.3"]),
      version(6, "2026-08-01T00:00:00Z", ["latest"]),
    ],
    { now: NOW },
  );
  assert.deepEqual(plan.delete.map((item) => item.id), []);
  // The newest version (id 5) is kept by age/position, every tagged version by
  // its protected tag.
  assert.match(summary(plan)[4].reason, /protected tag 'develop'/);
  assert.match(summary(plan)[3].reason, /protected tag 'develop'/);
  assert.match(summary(plan)[2].reason, /protected tag '0123456789abcdef/);
  assert.match(summary(plan)[1].reason, /protected tag 'v1\.2\.3'/);
  assert.match(summary(plan)[6].reason, /protected tag 'latest'/);
});

test("stale untagged versions are deleted", () => {
  const plan = planPackageCleanup(
    [
      version(3, "2026-11-02T00:00:00Z", [TREE("c")]),
      version(2, "2026-09-01T00:00:00Z", []),
      version(1, "2026-09-01T00:00:00Z", undefined),
    ],
    { now: NOW },
  );
  assert.deepEqual(plan.delete.map((item) => item.id), [2, 1]);
  assert.match(summary(plan)[1].reason, /untagged/);
});

test("unknown creation dates are kept", () => {
  const plan = planPackageCleanup(
    [
      version(2, "2026-11-03T00:00:00Z", [TREE("b")]),
      version(1, "not-a-date", [TREE("a")]),
    ],
    { now: NOW },
  );
  assert.deepEqual(plan.delete.map((item) => item.id), []);
  assert.match(summary(plan)[1].reason, /unknown creation date/);
});

test("the tie-break uses the newest id", () => {
  const same = "2026-09-01T00:00:00Z";
  const plan = planPackageCleanup(
    [
      version(7, same, [TREE("a")]),
      version(9, same, [TREE("b")]),
    ],
    { now: NOW },
  );
  assert.deepEqual(plan.keep.map((item) => item.id), [9]);
  assert.deepEqual(plan.delete.map((item) => item.id), [7]);
});

test("an empty package is a no-op", () => {
  const plan = planPackageCleanup([], { now: NOW });
  assert.equal(plan.newest, null);
  assert.deepEqual(plan.decisions, []);
});

test("the summary lists every decision and the dry-run mode", () => {
  const plan = planPackageCleanup(
    [
      version(2, "2026-11-03T00:00:00Z", [TREE("b")]),
      version(1, "2026-09-01T00:00:00Z", [TREE("a")]),
    ],
    { now: NOW },
  );
  const text = formatPackageSummary("scheduler", plan, { dryRun: true });
  assert.match(text, /GHCR cleanup \/ scheduler \(dry run\)/);
  assert.match(text, /`1` `tree-a+` — \*\*delete\*\*/);
  assert.match(text, /delete: 1, keep: 1/);
});

test("the CLI dry run reports the fixture decisions without GitHub", () => {
  const lines = [];
  const code = main(
    ["--fixture", path.join(HERE, "testdata", "ghcr-versions.json"), "--now", "2026-11-04T00:00:00Z"],
    (line) => lines.push(String(line)),
  );
  assert.equal(code, 0);
  const output = lines.join("\n");
  // scheduler 102, 106 (untagged); voice-proxy 302; hermes 202 is protected by
  // `latest`, 101/201/301 are the newest of their package, 103/104/105 carry a
  // deploy, commit or semver tag.
  assert.match(output, /\*\*delete\*\*.*102|102.*\*\*delete\*\*/);
  assert.match(output, /dry run: 3 version\(s\) would be deleted/);
  assert.doesNotMatch(output, /103.*\*\*delete\*\*/);
  assert.doesNotMatch(output, /202.*\*\*delete\*\*/);
});

test("the local CLI refuses to delete", () => {
  assert.throws(() => main(["--fixture", "unused.json", "--delete"]), /dry run only/);
  assert.throws(() => main([]), /--fixture is required/);
  assert.ok(fs.existsSync(path.join(HERE, "testdata", "ghcr-versions.json")));
});
