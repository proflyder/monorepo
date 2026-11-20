#!/usr/bin/env node

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

test("check_units: one event per workflow is accepted", () => {
  const root = workflowFixture({
    "ci.yml": ["on:", "  pull_request:", "    branches: [develop]", "jobs: {}"].join("\n"),
    "push.yml": ["on: push", "jobs: {}"].join("\n"),
    "flow.yml": ["on: [schedule]", "jobs: {}"].join("\n"),
    "_shared.yml": ["on:", "  workflow_call:", "    inputs: {}", "jobs: {}"].join("\n"),
  });
  assert.deepEqual(checkUnits.workflowTriggerProblems(root), []);
});

test("check_units: two events in one workflow are rejected", () => {
  const root = workflowFixture({
    "ci.yml": [
      "on:",
      "  pull_request:",
      "    branches: [develop]",
      "  workflow_dispatch:",
      "jobs: {}",
    ].join("\n"),
  });
  const problems = checkUnits.workflowTriggerProblems(root);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /exactly one trigger/);
  assert.match(problems[0], /pull_request, workflow_dispatch/);
  assert.match(problems[0], /thin wrappers/);
});

test("check_units: a flow list with two events is rejected", () => {
  const root = workflowFixture({
    "ci.yml": ["on: [push, pull_request]", "jobs: {}"].join("\n"),
  });
  assert.match(checkUnits.workflowTriggerProblems(root)[0], /exactly one trigger/);
});

test("check_units: a reusable workflow may only be called", () => {
  const root = workflowFixture({
    "_shared.yml": ["on:", "  push:", "    branches: [develop]", "jobs: {}"].join("\n"),
  });
  const problems = checkUnits.workflowTriggerProblems(root);
  assert.equal(problems.length, 1);
  assert.match(problems[0], /must only have 'on: workflow_call'/);
});

test("check_units: a workflow_call entry point must be a _*.yml reusable", () => {
  const root = workflowFixture({
    "ci.yml": ["on:", "  workflow_call:", "jobs: {}"].join("\n"),
  });
  assert.match(checkUnits.workflowTriggerProblems(root)[0], /must be named _\*\.yml/);
});

test("check_units: a workflow without an on block is rejected", () => {
  const root = workflowFixture({ "ci.yml": "jobs: {}\n" });
  assert.match(checkUnits.workflowTriggerProblems(root)[0], /no top-level 'on:'/);
});

test("check_units: nested keys inside on: are not events", () => {
  const text = [
    "on:",
    "  workflow_dispatch:",
    "    inputs:",
    "      units:",
    "        type: string",
    "  # a comment that mentions push:",
    "jobs:",
    "  x:",
    "    on: y",
  ].join("\n");
  assert.deepEqual(checkUnits.workflowTriggers(text), ["workflow_dispatch"]);
});

test("check_units: every committed workflow has exactly one trigger", () => {
  assert.deepEqual(checkUnits.workflowTriggerProblems(), []);
});

test("check_units: a caller job granting the reusable's union is accepted", () => {
  const root = workflowFixture({
    "_release.yml": [
      "on: workflow_call",
      "permissions:",
      "  contents: read",
      "jobs:",
      "  pins:",
      "    permissions:",
      "      contents: write",
      "      packages: write",
      "      attestations: read",
      "      pull-requests: read",
      "    runs-on: ubuntu-24.04",
      "    steps:",
      "      - run: true",
      "",
    ].join("\n"),
    "deploy.yml": [
      "on: push",
      "permissions:",
      "  contents: read",
      "jobs:",
      "  release:",
      "    permissions:",
      "      contents: write",
      "      packages: write",
      "      attestations: read",
      "      pull-requests: read",
      "    uses: ./.github/workflows/_release.yml",
      "",
    ].join("\n"),
  });
  assert.deepEqual(checkUnits.workflowCallerPermissionProblems(root), []);
});

test("check_units: a caller job inheriting read-only contents is rejected", () => {
  const root = workflowFixture({
    "_release.yml": [
      "on: workflow_call",
      "jobs:",
      "  pins:",
      "    permissions:",
      "      contents: write",
      "      packages: write",
      "    runs-on: ubuntu-24.04",
      "    steps:",
      "      - run: true",
      "",
    ].join("\n"),
    "deploy-home.yml": [
      "on: push",
      "permissions:",
      "  contents: read",
      "jobs:",
      "  release:",
      "    uses: ./.github/workflows/_release.yml",
      "",
    ].join("\n"),
  });
  const problems = checkUnits.workflowCallerPermissionProblems(root);
  assert.equal(problems.length, 2);
  assert.ok(problems.every((problem) => problem.includes("deploy-home.yml job 'release'")));
  assert.ok(problems.some((problem) => problem.includes("contents=read")));
  assert.ok(problems.some((problem) => problem.includes("packages=none")));
  assert.ok(problems.some((problem) => problem.includes("startup_failure")));
});

test("check_units: requirements of nested reusables are checked recursively", () => {
  const root = workflowFixture({
    "_inner.yml": [
      "on: workflow_call",
      "jobs:",
      "  publish:",
      "    permissions:",
      "      id-token: write",
      "      packages: write",
      "      checks: write",
      "    runs-on: ubuntu-24.04",
      "    steps:",
      "      - run: true",
      "",
    ].join("\n"),
    "_outer.yml": [
      "on: workflow_call",
      "permissions:",
      "  contents: read",
      "jobs:",
      "  inner:",
      "    permissions:",
      "      contents: read",
      "      packages: write",
      "      id-token: write",
      "    uses: ./.github/workflows/_inner.yml",
      "",
    ].join("\n"),
    "entry.yml": [
      "on: workflow_dispatch",
      "permissions:",
      "  contents: read",
      "jobs:",
      "  call:",
      "    uses: ./.github/workflows/_outer.yml",
      "",
    ].join("\n"),
  });
  const problems = checkUnits.workflowCallerPermissionProblems(root);
  const fromEntry = problems.filter((problem) => problem.includes("entry.yml job 'call'"));
  assert.equal(fromEntry.length, 3);
  assert.ok(fromEntry.some((problem) => problem.includes("id-token=none")));
  assert.ok(fromEntry.some((problem) => problem.includes("packages=none")));
  assert.ok(fromEntry.some((problem) => problem.includes("checks=none")));
  assert.ok(
    fromEntry.some((problem) =>
      problem.includes("checks=none but ./.github/workflows/_outer.yml requires checks=write (via .github/workflows/_inner.yml)"),
    ),
  );
  assert.ok(problems.some((problem) => problem.includes("_outer.yml job 'inner'")));
});

test("check_units: job-level permissions replace the workflow level", () => {
  const root = workflowFixture({
    "_shared.yml": [
      "on: workflow_call",
      "jobs:",
      "  write:",
      "    permissions:",
      "      contents: write",
      "    runs-on: ubuntu-24.04",
      "    steps:",
      "      - run: true",
      "",
    ].join("\n"),
    "entry.yml": [
      "on: push",
      "permissions:",
      "  contents: write",
      "jobs:",
      "  call:",
      "    permissions:",
      "      contents: read",
      "    uses: ./.github/workflows/_shared.yml",
      "",
    ].join("\n"),
  });
  const problems = checkUnits.workflowCallerPermissionProblems(root);
  assert.equal(problems.length, 1);
  assert.ok(problems[0].includes("contents=read"));
  assert.ok(problems[0].includes("contents=write"));
});

test("check_units: read-all cannot satisfy a write requirement", () => {
  const root = workflowFixture({
    "_shared.yml": [
      "on: workflow_call",
      "permissions:",
      "  packages: write",
      "jobs: {}",
      "",
    ].join("\n"),
    "entry.yml": [
      "on: push",
      "permissions: read-all",
      "jobs:",
      "  call:",
      "    uses: ./.github/workflows/_shared.yml",
      "",
    ].join("\n"),
  });
  const problems = checkUnits.workflowCallerPermissionProblems(root);
  assert.equal(problems.length, 1);
  assert.ok(problems[0].includes("packages=read"));
  assert.ok(problems[0].includes("packages=write"));
});

test("check_units: every committed caller grants its reusable workflows' permissions", () => {
  assert.deepEqual(checkUnits.workflowCallerPermissionProblems(), []);
});
