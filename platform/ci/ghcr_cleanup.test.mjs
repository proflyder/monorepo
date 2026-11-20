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

function version(id, created, tags, name) {
  return {
    id,
    name,
    created_at: created,
    metadata: { container: { tags } },
  };
}

const SHA40 = (char) => char.repeat(40);
const DIGEST = (char) => `sha256:${char.repeat(64)}`;
const REFERRER = (char, suffix = "") => `sha256-${char.repeat(64)}${suffix}`;

function summary(plan) {
  return Object.fromEntries(plan.decisions.map((item) => [item.id, item]));
}

test("deploy images and native publish repositories of units.mjs are considered", () => {
  assert.deepEqual(deployImagePackages(), [
    "currency-bot",
    "ha-bridge",
    "ha-config-sync",
    "ha-control",
    "ha-scenario",
    "health-exporter",
    "hermes",
    "hermes-config",
    "home-deploy",
    "intercom",
    "mcp-gateway",
    "phone-push",
    "pulsar-firmware",
    "scheduler",
    "voice-proxy",
    "wakegrant",
    "windows-logon-provider",
  ]);
});

test("native release tags survive and stale tree tags are deleted", () => {
  const plan = planPackageCleanup(
    [
      version(6, "2026-11-03T00:00:00Z", [TREE("f")]),
      version(5, "2026-08-01T00:00:00Z", ["v0.5.1008", "latest"]),
      version(4, "2026-08-01T00:00:00Z", ["1.2.3"]),
      version(3, "2026-08-01T00:00:00Z", ["sha-0123456789ab"]),
      version(2, "2026-08-01T00:00:00Z", [TREE("b")]),
      version(1, "2026-08-01T00:00:00Z", []),
    ],
    { now: NOW },
  );
  assert.deepEqual(plan.delete.map((item) => item.id), [2, 1]);
  assert.equal(summary(plan)[6].reason, "newest version of the package");
  for (const id of [5, 4, 3]) {
    assert.equal(summary(plan)[id].action, "keep");
    assert.match(summary(plan)[id].reason, /protected tag/);
  }
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

test("develop, latest and semver tags are never deleted", () => {
  const plan = planPackageCleanup(
    [
      version(5, "2026-10-20T00:00:00Z", [TREE("e")]),
      version(4, "2026-08-01T00:00:00Z", ["develop"]),
      version(3, "2026-08-01T00:00:00Z", ["develop", TREE("d")]),
      version(2, "2026-08-01T00:00:00Z", ["v1.2.3"]),
      version(1, "2026-08-01T00:00:00Z", ["v2.0.0-rc.1"]),
      version(6, "2026-08-01T00:00:00Z", ["latest"]),
    ],
    { now: NOW },
  );
  assert.deepEqual(plan.delete.map((item) => item.id), []);
  assert.match(summary(plan)[4].reason, /protected tag 'develop'/);
  assert.match(summary(plan)[3].reason, /protected tag 'develop'/);
  assert.match(summary(plan)[2].reason, /protected tag 'v1\.2\.3'/);
  assert.match(summary(plan)[1].reason, /protected tag 'v2\.0\.0-rc\.1'/);
  assert.match(summary(plan)[6].reason, /protected tag 'latest'/);
});

test("only the five newest deploy versions are kept", () => {
  const versions = [];
  for (let day = 1; day <= 7; day += 1) {
    versions.push(version(day, `2026-09-0${day}T00:00:00Z`, [SHA40(String(day))]));
  }
  const plan = planPackageCleanup(versions, { now: NOW });
  assert.deepEqual(plan.delete.map((item) => item.id), [1, 2]);
  assert.equal(plan.keep.length, 5);
  assert.match(summary(plan)[3].reason, /one of the 5 newest deploy versions/);
  assert.match(summary(plan)[2].reason, /beyond the newest 5/);
});

test("a single commit-sha version is kept inside the deploy window", () => {
  const plan = planPackageCleanup(
    [
      version(2, "2026-11-03T00:00:00Z", [TREE("b")]),
      version(1, "2026-08-01T00:00:00Z", [SHA40("a")]),
    ],
    { now: NOW },
  );
  assert.deepEqual(plan.delete.map((item) => item.id), []);
  assert.match(summary(plan)[1].reason, /one of the 5 newest deploy versions/);
});

test("a young deploy version is kept and still occupies one of the five slots", () => {
  const versions = [];
  for (let day = 1; day <= 6; day += 1) {
    versions.push(version(day, `2026-09-0${day}T00:00:00Z`, [SHA40(String(day))]));
  }
  versions.push(version(7, "2026-11-03T00:00:00Z", [SHA40("7")]));
  versions.push(version(8, "2026-11-03T12:00:00Z", [TREE("8")]));
  const plan = planPackageCleanup(versions, { now: NOW });
  assert.deepEqual(plan.delete.map((item) => item.id), [1, 2]);
  assert.match(summary(plan)[7].reason, /younger than 7 days/);
});

test("referrers are deleted with their subject digest", () => {
  const plan = planPackageCleanup(
    [
      version(3, "2026-11-03T00:00:00Z", [TREE("c")]),
      version(2, "2026-09-01T00:00:00Z", [REFERRER("a", ".att")], DIGEST("b")),
      version(1, "2026-09-01T00:00:00Z", [TREE("a")], DIGEST("a")),
    ],
    { now: NOW },
  );
  assert.deepEqual(summary(plan)[1].action, "delete");
  assert.equal(summary(plan)[2].action, "delete");
  assert.match(summary(plan)[2].reason, /subject digest sha256:aaaa+ is deleted in this run/);
});

test("referrers are kept when their subject is kept", () => {
  const plan = planPackageCleanup(
    [
      version(3, "2026-11-03T00:00:00Z", [TREE("c")]),
      version(2, "2026-09-01T00:00:00Z", [REFERRER("a")], DIGEST("b")),
      version(1, "2026-08-01T00:00:00Z", ["develop"], DIGEST("a")),
    ],
    { now: NOW },
  );
  assert.equal(summary(plan)[1].action, "keep");
  assert.equal(summary(plan)[2].action, "keep");
  assert.match(summary(plan)[2].reason, /subject digest sha256:aaaa+ is kept/);
});

test("referrers whose subject no longer exists are deleted", () => {
  const plan = planPackageCleanup(
    [
      version(2, "2026-11-03T00:00:00Z", [TREE("b")]),
      version(1, "2026-09-01T00:00:00Z", [REFERRER("f")], DIGEST("b")),
    ],
    { now: NOW },
  );
  assert.equal(summary(plan)[1].action, "delete");
  assert.match(summary(plan)[1].reason, /no longer exists in the package/);
});

test("referrers do not count toward the deploy window", () => {
  const versions = [];
  for (let day = 1; day <= 6; day += 1) {
    versions.push(version(day, `2026-09-0${day}T00:00:00Z`, [SHA40(String(day))], DIGEST(String(day))));
  }
  versions.push(version(20, "2026-09-01T00:00:00Z", [REFERRER("6")], DIGEST("f")));
  const plan = planPackageCleanup(versions, { now: NOW });
  assert.deepEqual(plan.delete.map((item) => item.id), [1]);
  assert.equal(summary(plan)[20].action, "keep");
});

test("PROTECTED_DIGESTS keeps a version and its referrer", () => {
  const protectedDigest = "sha256:238e0a0f3afe5f152e3d8b1de7a35452c016d68368e3bac24c5fde882869fca9";
  const plan = planPackageCleanup(
    [
      version(3, "2026-11-03T00:00:00Z", [TREE("c")]),
      version(2, "2026-09-01T00:00:00Z", [`sha256-${protectedDigest.slice(7)}`], DIGEST("b")),
      version(1, "2026-09-01T00:00:00Z", [TREE("a")], protectedDigest),
    ],
    {
      now: NOW,
      protectedDigests: [{ package: "voice-proxy", digest: protectedDigest, reason: "still pinned by a workload" }],
    },
  );
  assert.equal(summary(plan)[1].action, "keep");
  assert.match(summary(plan)[1].reason, /protected digest \(still pinned by a workload/);
  assert.equal(summary(plan)[2].action, "keep");
  assert.match(summary(plan)[2].reason, /referrer of protected digest/);
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
  assert.match(output, /\*\*delete\*\*.*102|102.*\*\*delete\*\*/);
  assert.match(output, /dry run: 8 version\(s\) would be deleted/);
  assert.doesNotMatch(output, /103.*\*\*delete\*\*/);
  assert.doesNotMatch(output, /202.*\*\*delete\*\*/);
  assert.match(output, /303.*\*\*delete\*\*/);
  assert.match(output, /304.*\*\*delete\*\*/);
  assert.doesNotMatch(output, /115.*\*\*delete\*\*/);
});

test("the local CLI refuses to delete", () => {
  assert.throws(() => main(["--fixture", "unused.json", "--delete"]), /dry run only/);
  assert.throws(() => main([]), /--fixture is required/);
  assert.ok(fs.existsSync(path.join(HERE, "testdata", "ghcr-versions.json")));
});

test("no digest is protected by default once the exporter left the Python image", async () => {
  const { PROTECTED_DIGESTS } = await import("./ghcr_cleanup.mjs");
  assert.deepEqual(PROTECTED_DIGESTS, []);
});
