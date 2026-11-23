#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

import * as stepReport from "./step_report.mjs";

test("parseCheck splits id and title", () => {
  assert.deepEqual(stepReport.parseCheck("audit=Repository audit"), {
    id: "audit",
    title: "Repository audit",
  });
  assert.throws(() => stepReport.parseCheck("=x"), /step-id/);
});

test("evaluate reports every failed, cancelled or missing step", () => {
  const { rows, failed } = stepReport.evaluate(
    {
      audit: { outcome: "success", conclusion: "success" },
      comments: { outcome: "failure", conclusion: "success" },
      gitleaks: { outcome: "failure", conclusion: "success" },
      shell: { outcome: "skipped" },
      actions: { outcome: "cancelled" },
    },
    ["audit", "comments", "gitleaks", "shell", "actions", "units"].map((id) => ({ id, title: id })),
  );
  assert.equal(rows.length, 6);
  assert.deepEqual(
    failed.map((row) => [row.id, row.outcome]),
    [
      ["comments", "failure"],
      ["gitleaks", "failure"],
      ["actions", "cancelled"],
      ["units", "missing"],
    ],
  );
});

test("main writes a summary table and fails with the full list", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "step-report-"));
  const out = [];
  const log = console.log;
  console.log = (...args) => out.push(args.join(" "));
  try {
    const summary = path.join(dir, "summary.md");
    const code = stepReport.main(
      ["--title", "quality", "--check", "a=Audit", "--check", "b=Gitleaks", "--summary", summary],
      { STEPS: JSON.stringify({ a: { outcome: "failure" }, b: { outcome: "failure" } }) },
    );
    assert.equal(code, 1);
    assert.match(out.join("\n"), /2 of 2 checks failed: Audit, Gitleaks/);
    assert.match(fs.readFileSync(summary, "utf8"), /\| Gitleaks \| ❌ failure \|/);
  } finally {
    console.log = log;
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("main is green when every step passed or was skipped", () => {
  const log = console.log;
  console.log = () => {};
  try {
    const code = stepReport.main(["--check", "a=A", "--check", "b=B", "--summary", ""], {
      STEPS: JSON.stringify({ a: { outcome: "success" }, b: { outcome: "skipped" } }),
    });
    assert.equal(code, 0);
  } finally {
    console.log = log;
  }
});
