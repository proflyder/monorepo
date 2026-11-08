#!/usr/bin/env node
/**
 * Unit tests for the Android target plan (`plan-targets.mjs`).
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import * as plan from "./plan-targets.mjs";

test("splitLabels drops empty entries and extra whitespace", () => {
  assert.deepEqual(plan.splitLabels("  //a:b   //a:c\n//a:d "), [
    "//a:b",
    "//a:c",
    "//a:d",
  ]);
  assert.deepEqual(plan.splitLabels(""), []);
  assert.deepEqual(plan.splitLabels(undefined), []);
});

test("narrow keeps every candidate when the affected set is empty", () => {
  // The plan job may be skipped (workflow_dispatch) or fail; narrowing then
  // means "run everything declared", never "run nothing".
  assert.deepEqual(plan.narrow("//a:b //a:c", ""), ["//a:b", "//a:c"]);
  assert.deepEqual(plan.narrow("", ""), []);
});

test("narrow intersects candidates with the affected set", () => {
  assert.deepEqual(
    plan.narrow("//a:b //a:c //a:d", "//a:c //elsewhere:e"),
    ["//a:c"],
  );
  assert.deepEqual(plan.narrow("//a:b //a:c", "//elsewhere:e"), []);
});

test("narrow matches labels exactly, not as prefixes or patterns", () => {
  assert.deepEqual(plan.narrow("//apps/one:app", "//apps/one:app_lib"), []);
  assert.deepEqual(plan.narrow("//apps/one:app", "//apps/one/..."), []);
});

test("metadata covers every android unit of the real unit map", () => {
  const lines = plan.metadataLines();
  const values = Object.fromEntries(
    lines.map((line) => [line.slice(0, line.indexOf("=")), line.slice(line.indexOf("=") + 1)]),
  );
  assert.equal(
    values.universe,
    "//packages/kotlin/... + //projects/mobile/android/...",
  );
  const candidates = plan.splitLabels(values.candidates);
  for (const target of [
    "//projects/mobile/android:app",
    "//projects/mobile/android:android_lint",
    "//projects/mobile/android:android_tests",
    "//packages/kotlin/core:core_test",
  ]) {
    assert.ok(candidates.includes(target), `${target} is a candidate`);
  }
});

test("narrowLines emits GitHub outputs the workflow can read", () => {
  assert.deepEqual(
    plan.narrowLines({
      affected: "//a:b",
      tests: "//a:b //a:c",
      builds: "//a:d",
    }),
    ["test_targets=//a:b", "build_targets="],
  );
});
