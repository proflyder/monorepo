#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { test } from "node:test";

import * as bazelReport from "./bazel_report.mjs";

function line(event) {
  return JSON.stringify(event);
}

function fixture(dir) {
  const log = (name, text) => {
    const file = path.join(dir, name);
    fs.writeFileSync(file, text);
    return pathToFileURL(file).href;
  };
  const unitLog = log("unit.log", "Executing tests\nexpected 1 but was 2\n");
  const lintLog = log("lint.log", "MainActivity.kt:12: Error: HardcodedText\n");
  const detektLog = log("detekt.log", "Feed.kt:3:1 MagicNumber\n");
  const stderr = log("stderr", "Foo.kt:7: error: unresolved reference\n");
  const configured = (label) => ({
    id: { targetConfigured: { label } },
    configured: { targetKind: "sh_test rule", testSize: "MEDIUM" },
  });
  const completed = (label, success) => ({
    id: { targetCompleted: { label } },
    completed: success ? { success: true } : {},
  });
  const summary = (label, status, uri) => ({
    id: { testSummary: { label } },
    testSummary: { overallStatus: status, failed: uri ? [{ uri }] : [] },
  });
  const result = (label, status, uri) => ({
    id: { testResult: { label, run: 1, shard: 1, attempt: 1 } },
    testResult: { status, testActionOutput: uri ? [{ name: "test.log", uri }] : [] },
  });
  return [
    configured("//app:unit_test"),
    configured("//app:pass_test"),
    configured("//:android_lint"),
    configured("//:detekt"),
    configured("//app:needs_broken_test"),
    {
      id: { actionCompleted: { label: "//app:lib" } },
      action: { exitCode: 1, type: "KotlinCompile", label: "//app:lib", stderr: { uri: stderr } },
    },
    completed("//app:lib", false),
    completed("//app:apk", true),
    completed("//app:unit_test", true),
    result("//app:unit_test", "FAILED", unitLog),
    summary("//app:unit_test", "FAILED", unitLog),
    completed("//app:pass_test", true),
    result("//app:pass_test", "PASSED"),
    summary("//app:pass_test", "PASSED"),
    completed("//:android_lint", true),
    result("//:android_lint", "FAILED", lintLog),
    summary("//:android_lint", "FAILED", lintLog),
    completed("//:detekt", true),
    result("//:detekt", "FAILED", detektLog),
    summary("//:detekt", "FAILED", detektLog),
    {
      id: { targetCompleted: { label: "//app:needs_broken_test" } },
      aborted: { reason: "ANALYSIS_FAILURE", description: "dependency //app:lib failed" },
    },
    { id: { buildFinished: {} }, finished: { exitCode: { name: "TESTS_FAILED", code: 3 } } },
  ]
    .map(line)
    .join("\n");
}

test("checkKind groups targets by the check they run", () => {
  assert.equal(bazelReport.checkKind("//projects/mobile/android:android_lint", true), "lint");
  assert.equal(bazelReport.checkKind("//projects/mobile/android:detekt", true), "detekt");
  assert.equal(bazelReport.checkKind("//solutions/x/crates/x:clippy", true), "clippy");
  assert.equal(bazelReport.checkKind("//solutions/x/crates/x:rustfmt", true), "format");
  assert.equal(bazelReport.checkKind("//app:robolectric_test", true), "test");
  assert.equal(bazelReport.checkKind("//app:app", false), "build");
  assert.equal(bazelReport.checkKind("//app:lint_baseline_test", true), "test");
});

test("report lists every failing test, lint, detekt and build at once", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "bazel-report-"));
  try {
    const outcome = bazelReport.report({ text: fixture(dir), title: "android / android-app" });
    assert.deepEqual(
      outcome.failed.map((entry) => [entry.kind, entry.label, entry.status]),
      [
        ["build", "//app:lib", "KotlinCompile failed"],
        ["test", "//app:needs_broken_test", "NOT_BUILT"],
        ["test", "//app:unit_test", "FAILED"],
        ["lint", "//:android_lint", "FAILED"],
        ["detekt", "//:detekt", "FAILED"],
      ],
    );
    assert.equal(outcome.exitCode, 3);
    assert.match(outcome.summary, /\| build \| 1 \|/);
    assert.match(outcome.summary, /\| test \| 2 \|/);
    assert.match(outcome.summary, /\| lint \| 1 \|/);
    assert.match(outcome.summary, /\| detekt \| 1 \|/);
    assert.match(outcome.summary, /unresolved reference/);
    assert.match(outcome.summary, /expected 1 but was 2/);
    assert.match(outcome.summary, /HardcodedText/);
    assert.match(outcome.summary, /MagicNumber/);
    assert.match(outcome.summary, /dependency \/\/app:lib failed/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("report is green when every target passed", () => {
  const text = [
    {
      id: { targetConfigured: { label: "//app:pass_test" } },
      configured: { testSize: "SMALL" },
    },
    { id: { targetCompleted: { label: "//app:pass_test" } }, completed: { success: true } },
    { id: { testSummary: { label: "//app:pass_test" } }, testSummary: { overallStatus: "PASSED" } },
    { id: { buildFinished: {} }, finished: { exitCode: { name: "SUCCESS" } } },
  ]
    .map(line)
    .join("\n");
  const outcome = bazelReport.report({ text, title: "t" });
  assert.deepEqual(outcome.failed, []);
  assert.match(outcome.summary, /All 1 test targets and 1 built targets passed/);
});

test("targets skipped as incompatible with the host platform are not failures", () => {
  const text = [
    {
      id: { targetCompleted: { label: "//station:station_binary" } },
      aborted: { reason: "SKIPPED", description: "incompatible with the target platform" },
    },
    { id: { buildFinished: {} }, finished: { exitCode: { name: "SUCCESS" } } },
  ]
    .map(line)
    .join("\n");
  assert.deepEqual(bazelReport.report({ text, title: "t" }).failed, []);
});

test("report says so when there is no event file", () => {
  const outcome = bazelReport.report({ text: "", title: "t" });
  assert.deepEqual(outcome.failed, []);
  assert.equal(outcome.exitCode, null);
  assert.match(outcome.summary, /did not report a result/);
});

test("main writes the summary and fails on failing targets", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "bazel-report-"));
  const log = console.log;
  console.log = () => {};
  try {
    const events = path.join(dir, "bep.json");
    const summary = path.join(dir, "summary.md");
    fs.writeFileSync(events, fixture(dir));
    const code = bazelReport.main(["--events", events, "--summary", summary, "--title", "x"]);
    assert.equal(code, 1);
    assert.match(fs.readFileSync(summary, "utf8"), /#### lint: 1 failed/);
  } finally {
    console.log = log;
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
