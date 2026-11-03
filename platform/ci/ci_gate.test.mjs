#!/usr/bin/env node
/**
 * Tests for the aggregate `ci-ok` gate.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import * as ciGate from "./ci_gate.mjs";

test("gate: all success and skipped is green", () => {
  const { ok, failing } = ciGate.evaluate({
    changes: { result: "success" },
    go: { result: "success" },
    android: { result: "skipped" },
    python: { result: "skipped" },
  });
  assert.equal(ok, true);
  assert.deepEqual(failing, {});
});

test("gate: failure is red", () => {
  const { ok, failing } = ciGate.evaluate({
    changes: { result: "success" },
    go: { result: "failure" },
  });
  assert.equal(ok, false);
  assert.deepEqual(failing, { go: "failure" });
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
    go: { result: "skipped" },
  });
  assert.equal(result.code, 0);
  assert.ok(result.out.includes("legitimately skipped"));
});

test("gate: failed unit reports the name", () => {
  const result = runWithNeeds({
    changes: { result: "success" },
    go: { result: "failure" },
  });
  assert.equal(result.code, 1);
  assert.ok(result.err.includes("go"));
  assert.ok(result.err.includes("failure"));
});
