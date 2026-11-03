#!/usr/bin/env node
/** Focused tests for the CI unit-map validator. */

import assert from "node:assert/strict";
import { test } from "node:test";

import * as affected from "./affected.mjs";
import * as checkUnits from "./check_units.mjs";

test("check_units: tracked source/test files need an owner", () => {
  for (const owned of [
    "solutions/x/main.go",
    "platform/ci/affected.test.mjs",
    "projects/mobile/android/app/BUILD.bazel",
    "scripts/run.sh",
  ]) {
    assert.equal(checkUnits.needsOwner(owned), true, owned);
  }
  for (const ignored of ["README.md", "docs/STATE.md", "data.json"]) {
    assert.equal(checkUnits.needsOwner(ignored), false, ignored);
  }
});

test("check_units: schema rejects duplicate ids and unknown platforms", () => {
  const config = new affected.Config({
    units: [
      new affected.Unit({ id: "same", platform: "go", paths: ["a/**"], goPackages: ["./a/..."] }),
      new affected.Unit({ id: "same", platform: "rust", paths: ["b/**"] }),
    ],
    invalidations: [],
  });
  const problems = checkUnits.validateSchema(config, process.cwd());
  assert.ok(problems.some((problem) => problem.includes("duplicate unit ids")));
  assert.ok(problems.some((problem) => problem.includes("unknown platform")));
});

test("check_units: unowned tracked file is reported", () => {
  const config = new affected.Config({
    units: [new affected.Unit({ id: "u", platform: "quality", paths: ["owned/**"] })],
    invalidations: [],
  });
  const problems = checkUnits.coverageProblems(config, ["owned/x.go", "stray/y.go"]);
  assert.ok(problems[0].includes("1 tracked file(s)"));
  assert.ok(problems.some((problem) => problem.includes("stray/y.go")));
});

test("check_units: committed unit map and tree pass", async () => {
  const result = await checkUnits.check([]);
  assert.deepEqual(result.problems, []);
  assert.ok(checkUnits.formatSuccess(result.config).includes("go="));
});
