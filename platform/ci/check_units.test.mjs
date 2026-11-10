#!/usr/bin/env node
/** Focused tests for the CI unit-map validator. */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

import * as affected from "./affected.mjs";
import * as checkUnits from "./check_units.mjs";

test("check_units: tracked source/test files need an owner", () => {
  for (const owned of [
    "solutions/x/main.rs",
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

test("check_units: Kotlin Android build tools are routed to the android platform", () => {
  const config = affected.loadConfig();
  const tool =
    "platform/android/tools/gen_view_binding/src/main/kotlin/dev/proflyder/home/tools/genviewbinding/Main.kt";
  assert.equal(checkUnits.needsOwner(tool), true);
  assert.deepEqual(checkUnits.coverageProblems(config, [tool]), []);
  assert.ok(config.byPlatform("android").some((unit) => unit.id === "android-app"));
});

test("check_units: schema rejects duplicate ids and unknown platforms", () => {
  const config = new affected.Config({
    units: [
      new affected.Unit({ id: "same", platform: "rust", paths: ["a/**"] }),
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
  const problems = checkUnits.coverageProblems(config, ["owned/x.rs", "stray/y.rs"]);
  assert.ok(problems[0].includes("1 tracked file(s)"));
  assert.ok(problems.some((problem) => problem.includes("stray/y.rs")));
});

test("check_units: a mixed unit needs an explicit Bazel image map", () => {
  const base = {
    id: "mixed",
    platform: "quality",
    paths: ["solutions/x/**"],
    deployImages: ["a", "b"],
    deployWorkloads: ["w"],
    deployBuilder: "mixed",
    deployNamespace: "ns",
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

function workflowFixture(files) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "check-units-workflows-"));
  fs.mkdirSync(path.join(root, ".github/workflows"), { recursive: true });
  for (const [name, body] of Object.entries(files)) {
    fs.writeFileSync(path.join(root, ".github/workflows", name), body);
  }
  return root;
}

test("check_units: a job that promotes without a login is rejected", () => {
  // The incident shape: promote.mjs re-tags with crane, and the public package
  // reads anonymously, so only the manifest PUT fails.
  const root = workflowFixture({
    "_native.yml": [
      "on: workflow_call",
      "jobs:",
      "  promote:",
      "    runs-on: ubuntu-24.04",
      "    steps:",
      "      - name: Promote",
      "        run: node platform/ci/promote.mjs --native --tree \"$tree\"",
      "",
    ].join("\n"),
  });
  const problems = checkUnits.workflowRegistryProblems(root);
  assert.equal(problems.length, 1);
  assert.ok(problems[0].includes("_native.yml job 'promote'"));
  assert.ok(problems[0].includes("without a login step"));
});

test("check_units: a login before the registry write is accepted", () => {
  const root = workflowFixture({
    "workflow.yml": [
      "jobs:",
      "  promote:",
      "    steps:",
      "      - uses: docker/login-action@81c6f93d4bc991e510b342a00b419b3bb4efc5c6",
      "      - name: Promote",
      "        run: node platform/ci/promote.mjs --native",
      "  publish:",
      "    steps:",
      "      - name: Log in",
      "        run: echo \"$token\" | crane auth login ghcr.io -u me --password-stdin",
      "      - run: oras push ghcr.io/acme/artifact:latest ./artifact",
      "",
    ].join("\n"),
  });
  assert.deepEqual(checkUnits.workflowRegistryProblems(root), []);
});

test("check_units: a login after the first registry write is rejected", () => {
  const root = workflowFixture({
    "workflow.yml": [
      "jobs:",
      "  publish:",
      "    steps:",
      "      - run: crane tag ghcr.io/acme/artifact@sha256:abc latest",
      "      - uses: docker/login-action@81c6f93d4bc991e510b342a00b419b3bb4efc5c6",
      "",
    ].join("\n"),
  });
  const problems = checkUnits.workflowRegistryProblems(root);
  assert.ok(problems.some((problem) => problem.includes("logs in after its first registry write")));
});

test("check_units: jobs without registry writes need no login", () => {
  const root = workflowFixture({
    "ci.yml": [
      "jobs:",
      "  test:",
      "    steps:",
      "      - run: cargo test",
      "",
    ].join("\n"),
  });
  assert.deepEqual(checkUnits.workflowRegistryProblems(root), []);
});

test("check_units: committed workflows log in before every registry write", () => {
  assert.deepEqual(checkUnits.workflowRegistryProblems(), []);
});

test("check_units: committed unit map and tree pass", async () => {
  const result = await checkUnits.check([]);
  assert.deepEqual(result.problems, []);
  assert.ok(checkUnits.formatSuccess(result.config).includes("bazel="));
});

test("check_units: rollback kinds are validated", () => {
  const config = new affected.Config({
    units: [
      new affected.Unit({
        id: "bad-kind",
        platform: "quality",
        paths: ["a/**"],
        rollbackKind: "nonsense",
      }),
    ],
    invalidations: [],
  });
  const problems = checkUnits.validateSchema(config, process.cwd());
  assert.ok(problems.some((problem) => problem.includes("unknown rollback.kind")));
});

test("check_units: forward-only rollback needs a note", () => {
  const config = new affected.Config({
    units: [
      new affected.Unit({
        id: "fwd",
        platform: "quality",
        paths: ["a/**"],
        rollbackKind: "forward-only",
      }),
    ],
    invalidations: [],
  });
  const problems = checkUnits.validateSchema(config, process.cwd());
  assert.ok(problems.some((problem) => problem.includes("forward-only needs a note")));
});

test("check_units: deploy-state rollback needs state_dir and namespace", () => {
  const config = new affected.Config({
    units: [
      new affected.Unit({
        id: "state",
        platform: "quality",
        paths: ["a/**"],
        rollbackKind: "deploy-state",
      }),
    ],
    invalidations: [],
  });
  const problems = checkUnits.validateSchema(config, process.cwd());
  assert.ok(problems.some((problem) => problem.includes("deploy-state needs state_dir")));
  assert.ok(problems.some((problem) => problem.includes("deploy-state needs namespace")));
});

test("check_units: committed rollback blocks are documented and valid", () => {
  const config = affected.loadConfig();
  const withRollback = config.units.filter((unit) => unit.rollbackKind);
  assert.ok(withRollback.length >= 7);
  for (const unit of config.units.filter((candidate) => candidate.deployImages.length > 0)) {
    assert.ok(unit.rollbackKind, `deployable unit ${unit.id} needs a rollback block`);
  }
  assert.ok(withRollback.some((unit) => unit.id === "pulsar" && unit.rollbackKind === "pulsar"));
});
