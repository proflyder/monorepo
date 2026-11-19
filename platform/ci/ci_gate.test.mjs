#!/usr/bin/env node
/**
 * Tests for the aggregate `ci-ok` gate.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import { test } from "node:test";

import * as ciGate from "./ci_gate.mjs";

test("gate: all success and skipped is green", () => {
  const { ok, failing } = ciGate.evaluate({
    changes: { result: "success" },
    bazel: { result: "success" },
    android: { result: "skipped" },
    python: { result: "skipped" },
  });
  assert.equal(ok, true);
  assert.deepEqual(failing, {});
});

test("gate: failure is red", () => {
  const { ok, failing } = ciGate.evaluate({
    changes: { result: "success" },
    bazel: { result: "failure" },
  });
  assert.equal(ok, false);
  assert.deepEqual(failing, { bazel: "failure" });
});

test("gate: cancelled is red", () => {
  const { ok, failing } = ciGate.evaluate({
    changes: { result: "success" },
    android: { result: "cancelled" },
  });
  assert.equal(ok, false);
  assert.deepEqual(failing, { android: "cancelled" });
});

test("gate: empty needs is red", () => {
  const { ok, results } = ciGate.evaluate({});
  assert.equal(ok, false);
  assert.deepEqual(results, {});
});

function runWithNeeds(needs) {
  const saved = process.env.NEEDS;
  process.env.NEEDS = JSON.stringify(needs);
  const out = [];
  const err = [];
  const log = console.log;
  const error = console.error;
  console.log = (...args) => out.push(args.join(" "));
  console.error = (...args) => err.push(args.join(" "));
  try {
    return { code: ciGate.main(), out: out.join("\n"), err: err.join("\n") };
  } finally {
    console.log = log;
    console.error = error;
    if (saved === undefined) {
      delete process.env.NEEDS;
    } else {
      process.env.NEEDS = saved;
    }
  }
}

function runRaw(raw) {
  const saved = process.env.NEEDS;
  process.env.NEEDS = raw;
  const err = [];
  const error = console.error;
  console.error = (...args) => err.push(args.join(" "));
  try {
    return { code: ciGate.main(), err: err.join("\n") };
  } finally {
    console.error = error;
    if (saved === undefined) {
      delete process.env.NEEDS;
    } else {
      process.env.NEEDS = saved;
    }
  }
}

test("gate: missing needs fails", () => {
  assert.equal(runRaw("").code, 1);
});

test("gate: invalid json fails", () => {
  assert.equal(runRaw("not-json").code, 1);
});

test("gate: skipped units pass", () => {
  const result = runWithNeeds({
    changes: { result: "success" },
    bazel: { result: "skipped" },
  });
  assert.equal(result.code, 0);
  assert.ok(result.out.includes("legitimately skipped"));
});

test("gate: failed unit reports the name", () => {
  const result = runWithNeeds({
    changes: { result: "success" },
    bazel: { result: "failure" },
  });
  assert.equal(result.code, 1);
  assert.ok(result.err.includes("bazel"));
  assert.ok(result.err.includes("failure"));
});

// `ci.yml` is now a thin caller: the aggregate answer is computed in the
// reusable pipeline (`--github-output`) and the caller's `ci-ok` check fails on
// `ok=false`. The failing job names must still reach the operator.
function runOutput(needs) {
  const saved = process.env.NEEDS;
  const outputFile = `${os.tmpdir()}/ci-gate-${process.pid}-${Math.random().toString(16).slice(2)}`;
  fs.writeFileSync(outputFile, "");
  process.env.NEEDS = JSON.stringify(needs);
  process.env.GITHUB_OUTPUT = outputFile;
  const savedOutput = arguments[1];
  const log = console.log;
  console.log = () => {};
  try {
    const code = ciGate.main(["--github-output"]);
    return { code, body: fs.readFileSync(outputFile, "utf8") };
  } finally {
    console.log = log;
    fs.rmSync(outputFile, { force: true });
    if (saved === undefined) delete process.env.NEEDS;
    else process.env.NEEDS = saved;
    if (savedOutput === undefined) delete process.env.GITHUB_OUTPUT;
    else process.env.GITHUB_OUTPUT = savedOutput;
  }
}

test("gate: pipeline output is green when every job passed or was skipped", () => {
  const result = runOutput({
    changes: { result: "success" },
    android: { result: "skipped" },
  });
  assert.equal(result.code, 0);
  assert.match(result.body, /^ok=true$/m);
  assert.match(result.body, /^failing=$/m);
});

test("gate: pipeline output names the failing jobs", () => {
  const result = runOutput({
    changes: { result: "success" },
    quality: { result: "failure" },
    native: { result: "cancelled" },
  });
  assert.equal(result.code, 0, "the reusable step reports, the caller check fails");
  assert.match(result.body, /^ok=false$/m);
  assert.match(result.body, /quality=failure/);
  assert.match(result.body, /native=cancelled/);
});

test("gate: a cancelled pipeline with no results reports red", () => {
  const result = runOutput({});
  assert.match(result.body, /^ok=false$/m);
});
