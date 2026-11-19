/**
 * Contracts for the deterministic native release version.
 *
 * The version is `<VERSION major.minor>.<commits touching the unit>`, computed
 * from `HEAD^1` so a PR merge ref and its squash commit agree. These tests pin
 * the arithmetic, the git contract and the two properties the station updater
 * depends on: monotone across the old hand-bumped releases and no manual patch
 * in `VERSION` any more.
 */

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import * as version from "./version.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const { config } = await import("./units.mjs");

/** Numeric semver comparison, the same order the updater uses. */
function compare(left, right) {
  const parse = (value) =>
    String(value)
      .replace(/^v/, "")
      .split(".")
      .map((part) => Number(part));
  const a = parse(left);
  const b = parse(right);
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) {
      return a[index] < b[index] ? -1 : 1;
    }
  }
  return 0;
}

test("version: a major.minor base starts at patch 0", () => {
  const base = version.parseBase("v0.6");
  assert.deepEqual(base, { prefix: "v", major: 0, minor: 6, floor: 0 });
  assert.equal(version.formatVersion(base, 0), "v0.6.0");
});

test("version: the counter is added to the base floor", () => {
  assert.equal(version.computeVersion("v0.6", 50).version, "v0.6.50");
  assert.equal(version.computeVersion("v0.5.1020", 3).version, "v0.5.1023");
  assert.equal(version.computeVersion("1.4", 31).version, "1.4.31");
  assert.equal(version.computeVersion("  v0.6\n", 0).version, "v0.6.0");
});

test("version: the counter is reported for the summary", () => {
  const result = version.computeVersion("v0.6", 12);
  assert.equal(result.count, 12);
  assert.equal(result.patch, 12);
});

test("version: malformed bases are rejected", () => {
  for (const bad of ["", "0.6.1.2", "v0", "0", "v0.6.1-rc1", "v0.6.x"]) {
    assert.throws(() => version.parseBase(bad), /VERSION/, `accepted '${bad}'`);
  }
});

test("version: a negative counter is rejected", () => {
  assert.throws(() => version.computeVersion("v0.6", -1), /commit count/);
  assert.throws(() => version.computeVersion("v0.6", 1.5), /commit count/);
});

test("version: the new sequence stays above the last hand-bumped release", () => {
  // v0.5.1020 is what the station runs today. A base of v0.6 with a small
  // counter must compare greater, or the updater would refuse every release.
  const next = version.computeVersion("v0.6", 50).version;
  assert.equal(compare(next, "v0.5.1020"), 1, `${next} must be newer than v0.5.1020`);
  assert.equal(compare(version.computeVersion("1.4", 31).version, "1.3.17"), 1);
});

test("version: the counter reads the first parent and the unit paths", () => {
  const calls = [];
  const exec = (command, args) => {
    calls.push([command, ...args]);
    return "  42\n";
  };
  const count = version.readCommitCount(exec, {
    rev: "HEAD^1",
    paths: ["projects/station/pulsar/**"],
  });
  assert.equal(count, 42);
  assert.deepEqual(calls, [
    ["git", "rev-list", "--count", "HEAD^1", "--", "projects/station/pulsar/**"],
  ]);
});

test("version: a shallow clone without a parent fails instead of guessing", () => {
  const seen = [];
  const exec = (command, args) => {
    seen.push([command, ...args]);
    throw new Error("fatal: ambiguous argument 'HEAD^1'");
  };
  assert.throws(() => version.counterRevision(exec), /fetch-depth: 0/);
  assert.deepEqual(
    seen.map((call) => call.at(-1)),
    ["HEAD^1^{commit}"],
  );
});

test("version: --github-output appends the step outputs to the file", () => {
  const dir = fs.mkdtempSync(path.join(require_tmpdir(), "version-out-"));
  const file = path.join(dir, "out");
  const saved = process.env.GITHUB_OUTPUT;
  process.env.GITHUB_OUTPUT = file;
  const out = [];
  const write = process.stdout.write.bind(process.stdout);
  process.stdout.write = (chunk) => {
    out.push(String(chunk));
    return true;
  };
  try {
    version.emit(["version=v0.6.50", "count=50"].join("\n"), { githubOutput: true });
    assert.equal(fs.readFileSync(file, "utf8"), "version=v0.6.50\ncount=50\n");
    assert.match(out.join(""), /version=v0\.6\.50/);
  } finally {
    process.stdout.write = write;
    if (saved === undefined) delete process.env.GITHUB_OUTPUT;
    else process.env.GITHUB_OUTPUT = saved;
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("version: the published annotation is the authority", () => {
  const exec = () =>
    JSON.stringify({
      schemaVersion: 2,
      artifactType: "application/vnd.proflyder.pulsar.release.v1",
      annotations: { "org.opencontainers.image.version": "v0.6.51" },
      layers: [],
    });
  assert.equal(
    version.publishedVersion(exec, "ghcr.io/proflyder/pulsar-firmware:latest"),
    "v0.6.51",
  );
});

test("version: a published artifact without the annotation is an error", () => {
  const exec = () => JSON.stringify({ layers: [], annotations: {} });
  assert.throws(
    () => version.publishedVersion(exec, "ghcr.io/proflyder/unknown:latest"),
    /annotation/,
  );
});

test("version: every native unit computes a monotone version from its own paths", () => {
  for (const [id, previous] of [
    ["pulsar", "v0.5.1020"],
    ["logon-provider", "1.3.17"],
  ]) {
    const unit = nativeUnit(id);
    const result = version.resolveVersion({ unit, exec: realExec, cwd: ROOT });
    assert.match(result.version, /^v?\d+\.\d+\.\d+$/, `${id}: ${result.version}`);
    assert.equal(
      compare(result.version, previous),
      1,
      `${id}: ${result.version} must stay newer than ${previous}`,
    );
    assert.ok(result.patch >= result.count, `${id}: patch includes the base floor`);
  }
});

test("version: VERSION holds only the major.minor base", () => {
  for (const id of ["pulsar", "logon-provider"]) {
    const unit = nativeUnit(id);
    const text = fs.readFileSync(path.join(ROOT, unit.workdir, "VERSION"), "utf8").trim();
    assert.match(text, /^v?\d+\.\d+$/, `${id}: VERSION is '${text}', expected MAJOR.MINOR`);
  }
});

/** The unit map is the one source of `paths` and `workdir`, so read it here. */
function require_tmpdir() {
  return os.tmpdir();
}

function nativeUnit(id) {
  const unit = config.units.find((entry) => entry.platform === "native" && entry.id === id);
  assert.ok(unit, `unknown native unit '${id}'`);
  return { id: unit.id, workdir: unit.native.workdir, paths: unit.paths };
}

function realExec(command, args) {
  return execFileSync(command, args, { cwd: ROOT, encoding: "utf8" });
}
