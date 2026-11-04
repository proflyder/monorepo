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

test("check_units: a mixed unit needs an explicit Bazel image map", () => {
  const base = {
    id: "mixed",
    platform: "go",
    paths: ["solutions/x/**"],
    goPackages: ["./solutions/x/..."],
    deployImages: ["a", "b"],
    deployWorkloads: ["w"],
    deployBuilder: "mixed",
    deployNamespace: "ns",
    deployScript: "projects/home-assistant/deploy/rollout.sh",
    deployEnvKey: "k",
    deployEnvFile: "f",
    deployExternalsecrets: ["s"],
  };
  const validate = (extra) =>
    checkUnits.validateSchema(
      new affected.Config({ units: [new affected.Unit({ ...base, ...extra })], invalidations: [] }),
      process.cwd(),
    );
  assert.ok(validate({}).some((problem) => problem.includes("deploy.bazel_images")));
  assert.ok(
    validate({ deployTargetRoot: "//x", deployBazelImages: { a: "//x" } }).some((problem) =>
      problem.includes("ambiguous"),
    ),
  );
  assert.ok(
    validate({ deployBazelImages: { a: "//x", c: "//y" } }).some((problem) =>
      problem.includes("is not in deploy.images"),
    ),
  );
  assert.deepEqual(validate({ deployBazelImages: { a: "//x", b: "//y" } }), []);
});

test("check_units: committed unit map and tree pass", async () => {
  const result = await checkUnits.check([]);
  assert.deepEqual(result.problems, []);
  assert.ok(checkUnits.formatSuccess(result.config).includes("go="));
});
