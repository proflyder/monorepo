#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

import { normalizeConfig } from "../../ci/affected.mjs";
import {
  ROOT,
  buildCommands,
  findUnit,
  formatTable,
  isDeployable,
  runCommands,
  buildOnlyQuery,
  ciPlan,
  isTargetPattern,
  runCi,
  suggestIds,
  testCommands,
  unitHint,
} from "./units-cli.mjs";

const CONFIG = normalizeConfig({
  version: 1,
  invalidations: [],
  units: [
    {
      id: "proto",
      platform: "bazel",
      title: "packages/proto",
      paths: ["packages/proto/**"],
      deploy: { images: ["proto"], workloads: ["proto"], namespace: "x", script: "x.sh" },
      bazel: {
        roots: ["//packages/proto"],
        test_targets: ["//packages/rust/home-proto:unit_tests"],
        build_targets: ["//packages/proto/..."],
      },
    },
    {
      id: "roots-only",
      platform: "bazel",
      title: "roots without build targets",
      paths: ["solutions/voice-proxy/**"],
      bazel: {
        roots: ["//solutions/voice-proxy"],
        test_targets: ["//solutions/voice-proxy/crates/voice-proxy:unit_tests"],
      },
    },
    {
      id: "ha-voice",
      platform: "python",
      title: "projects/home-assistant/ha-voice",
      paths: ["projects/home-assistant/ha-voice/**"],
      python: { suites: ["projects/home-assistant/ha-voice"] },
    },
    {
      id: "currency",
      platform: "python",
      title: "projects/home/kubernetes/currency-bot",
      paths: ["projects/home/kubernetes/currency-bot/**"],
      python: { suites: ["projects/home/kubernetes/currency-bot"], deploy_tool: true },
    },
    {
      id: "heartbeat",
      platform: "js",
      title: "solutions/heartbeat",
      paths: ["solutions/heartbeat/**"],
      js: { tests: ["solutions/heartbeat/src"] },
    },
    {
      id: "logon-provider",
      platform: "native",
      title: "solutions/windows-logon-provider",
      paths: ["solutions/windows-logon-provider/**"],
      native: {
        workdir: "solutions/windows-logon-provider",
        kind: "script",
        test: "./run-tests.sh",
        build: "./build.sh",
        artifacts: ["out/x.dll"],
        checksums: "out/SHA256SUMS",
      },
    },
    {
      id: "repository",
      platform: "quality",
      title: "repository-wide gates",
      paths: ["platform/**"],
    },
    {
      id: "app-one",
      platform: "android",
      title: "apps/one",
      paths: ["apps/one/**"],
      bazel: {
        roots: ["//apps/one"],
        test_targets: ["//apps/one:one_test"],
        build_targets: ["//apps/one:app"],
      },
    },
  ],
});

const units = CONFIG.units;
const byId = (id) => findUnit(units, id);

test("formatTable lists id, platform, title and deployable", () => {
  const table = formatTable(units).split("\n");
  assert.match(table[0], /^id\s+platform\s+title\s+deployable$/);
  assert.match(table[1], /^proto\s+bazel\s+packages\/proto\s+yes$/);
  assert.match(table[2], /^roots-only\s+bazel\s+.*\s+no$/);
});

test("isDeployable follows deploy.images", () => {
  assert.equal(isDeployable(byId("proto")), true);
  assert.equal(isDeployable(byId("heartbeat")), false);
});

test("suggestIds finds typos and substrings", () => {
  const ids = units.map((unit) => unit.id);
  assert.deepEqual(suggestIds(ids, "protoo"), ["proto"]);
  assert.ok(suggestIds(ids, "logon").includes("logon-provider"));
  assert.deepEqual(suggestIds(ids, "zzzzzzzz"), []);
});

test("findUnit explains an unknown unit with suggestions", () => {
  assert.throws(
    () => findUnit(units, "protoo"),
    /unknown unit 'protoo'\.\nDid you mean: proto\?/,
  );
});

test("buildCommands builds every target without a unit", () => {
  assert.deepEqual(buildCommands(null), [
    { command: "bazel", args: ["build", "--config=android", "//..."], cwd: ROOT },
  ]);
});

test("buildCommands uses build_targets or falls back to bazel roots", () => {
  assert.deepEqual(buildCommands(byId("proto"))[0].args, ["build", "//packages/proto/..."]);
  assert.deepEqual(buildCommands(byId("roots-only"))[0].args, [
    "build",
    "//solutions/voice-proxy/...",
  ]);
});

test("android units build and test with --config=android", () => {
  assert.deepEqual(buildCommands(byId("app-one"))[0].args, [
    "build",
    "--config=android",
    "//apps/one:app",
  ]);
  assert.deepEqual(testCommands(byId("app-one"))[0].args, [
    "test",
    "--config=android",
    "//apps/one:one_test",
    "--test_output=errors",
  ]);
  assert.ok(!testCommands(byId("proto"))[0].args.includes("--config=android"));
});

test("buildCommands rejects platforms without a build entry point", () => {
  assert.throws(() => buildCommands(byId("ha-voice")), /platform 'python'/);
  assert.throws(() => buildCommands(byId("repository")), /mise run check/);
});

test("testCommands mirrors the Bazel workflow, including --test_output=errors", () => {
  assert.deepEqual(testCommands(byId("proto"))[0].args, [
    "test",
    "//packages/rust/home-proto:unit_tests",
    "--test_output=errors",
  ]);
});

test("testCommands honours BAZEL_TARGETS narrowed by CI", () => {
  process.env.BAZEL_TARGETS = "//packages/proto:a //packages/proto:b";
  try {
    assert.deepEqual(testCommands(byId("proto"))[0].args, [
      "test",
      "//packages/proto:a",
      "//packages/proto:b",
      "--test_output=errors",
    ]);
    assert.deepEqual(buildCommands(byId("proto"))[0].args, [
      "build",
      "//packages/proto:a",
      "//packages/proto:b",
    ]);
  } finally {
    delete process.env.BAZEL_TARGETS;
  }
});

test("build and test append BAZEL_EXTRA_ARGS so the tested APK is the signed one", () => {
  process.env.BAZEL_EXTRA_ARGS =
    "--define home_version_code=1700000000 --define home_version_name=0.1.0-abc1234";
  try {
    assert.deepEqual(buildCommands(byId("app-one"))[0].args, [
      "build",
      "--config=android",
      "--define",
      "home_version_code=1700000000",
      "--define",
      "home_version_name=0.1.0-abc1234",
      "//apps/one:app",
    ]);
    assert.deepEqual(testCommands(byId("app-one"))[0].args, [
      "test",
      "--config=android",
      "--define",
      "home_version_code=1700000000",
      "--define",
      "home_version_name=0.1.0-abc1234",
      "//apps/one:one_test",
      "--test_output=errors",
    ]);
  } finally {
    delete process.env.BAZEL_EXTRA_ARGS;
  }
});

test("testCommands compiles and discovers Python suites", () => {
  const commands = testCommands(byId("ha-voice"));
  assert.equal(commands.length, 2);
  assert.deepEqual(commands[0].args, ["-m", "compileall", "-q", "projects/home-assistant/ha-voice"]);
  assert.deepEqual(commands[1].args.slice(0, 4), ["-m", "unittest", "discover", "-s"]);
});

test("testCommands builds the Rust deploy tool for deploy_tool units", () => {
  const commands = testCommands(byId("currency"));
  const [build, compile, unittest] = commands;
  assert.equal(build.command, "bazel");
  assert.deepEqual(build.args, ["build", "//platform/deploy-rs:home-deploy"]);
  assert.equal(build.prerequisite, true);
  assert.match(compile.env.HOME_DEPLOY, /bazel-bin\/platform\/deploy-rs\/home-deploy$/);
  assert.equal(unittest.env.HOME_DEPLOY, compile.env.HOME_DEPLOY);
});

test("testCommands collects *.test.js for JS units", () => {
  const [command] = testCommands(byId("heartbeat"));
  assert.equal(command.command, "node");
  assert.deepEqual(command.args.slice(0, 1), ["--test"]);
  assert.ok(command.args.some((arg) => arg === "solutions/heartbeat/src/decide.test.js"));
});

test("testCommands runs the native repository script in its workdir", () => {
  const [command] = testCommands(byId("logon-provider"));
  assert.equal(command.command, "./run-tests.sh");
  assert.equal(command.shell, true);
  assert.equal(command.cwd, path.join(ROOT, "solutions/windows-logon-provider"));
});

test("testCommands without a unit runs the hermetic suite", () => {
  const [command] = testCommands(null);
  assert.equal(command.command, path.join(ROOT, "platform/quality/run-tests.sh"));
});

test("testCommands mirrors the quality workflow gates", () => {
  const commands = testCommands(byId("repository"));
  const commandsSeen = commands.map(({ command, args }) => [command, args[0]]);
  assert.deepEqual(commandsSeen, [
    ["node", "platform/quality/audit_repository.mjs"],
    ["python3", "-m"],
    ["node", "platform/ci/check_units.mjs"],
    ["node", "--test"],
    ["shellcheck", "-S"],
    ["actionlint", "-color"],
    ["gitleaks", "dir"],
  ]);
  assert.ok(commands[4].args.includes("platform/workstation/run.sh"));
  assert.equal(commands[6].args.at(-1), "--config=.gitleaks.toml");
});

test("unitHint names the platform command", () => {
  assert.match(unitHint(byId("ha-voice")), /unittest discover/);
  assert.match(unitHint(byId("heartbeat")), /node --test/);
});

function fakeSpawn(statuses) {
  const calls = [];
  return {
    calls,
    spawn: (command, args) => {
      calls.push([command, ...args].join(" "));
      return { status: statuses.shift() ?? 0 };
    },
  };
}

function sink() {
  const lines = [];
  return { lines, write: (text) => lines.push(text) };
}

test("runCommands keeps going and lists every failed test command", () => {
  const { calls, spawn } = fakeSpawn([1, 0, 2]);
  const log = sink();
  const status = runCommands(
    [
      { command: "a", args: [] },
      { command: "b", args: [] },
      { command: "c", args: ["x"] },
    ],
    { keepGoing: true, spawn, log },
  );
  assert.equal(status, 1);
  assert.deepEqual(calls, ["a", "b", "c x"]);
  assert.match(log.lines.join(""), /2 of 3 commands failed:\n  exit 1: a\n  exit 2: c x/);
});

test("runCommands stops at a failed prerequisite even when keeping going", () => {
  const { calls, spawn } = fakeSpawn([1]);
  const status = runCommands(
    [
      { command: "bazel", args: ["build"], prerequisite: true },
      { command: "python3", args: [] },
    ],
    { keepGoing: true, spawn, log: sink() },
  );
  assert.equal(status, 1);
  assert.deepEqual(calls, ["bazel build"]);
});

test("runCommands stops at the first failure without keepGoing", () => {
  const { calls, spawn } = fakeSpawn([3, 0]);
  const status = runCommands(
    [
      { command: "a", args: [] },
      { command: "b", args: [] },
    ],
    { spawn, log: sink() },
  );
  assert.equal(status, 3);
  assert.deepEqual(calls, ["a"]);
});

test("isTargetPattern tells patterns from labels", () => {
  assert.equal(isTargetPattern("//packages/rust/..."), true);
  assert.equal(isTargetPattern("//a:all"), true);
  assert.equal(isTargetPattern("//a:*"), true);
  assert.equal(isTargetPattern("//apps/one:app"), false);
});

test("buildOnlyQuery keeps the tests under a build pattern out of the run", () => {
  assert.equal(
    buildOnlyQuery(["//a/...", "//b/..."]),
    "set(//a/... //b/...) except tests(set(//a/... //b/...))",
  );
});

test("ciPlan splits tests, explicit build labels and build patterns", () => {
  const plan = ciPlan(byId("proto"));
  assert.equal(plan.command, "test");
  assert.deepEqual(plan.config, []);
  assert.deepEqual(plan.tests, ["//packages/rust/home-proto:unit_tests"]);
  assert.deepEqual(plan.explicitBuilds, []);
  assert.deepEqual(plan.patternBuilds, ["//packages/proto/..."]);
  const android = ciPlan(byId("app-one"));
  assert.deepEqual(android.config, ["--config=android"]);
  assert.deepEqual(android.explicitBuilds, ["//apps/one:app"]);
  assert.throws(() => ciPlan(byId("ha-voice")), /Bazel units only/);
});

test("runCi runs one keep-going bazel test over tests and build-only labels", () => {
  const calls = [];
  let targets = "";
  const spawn = (command, args) => {
    calls.push([command, ...args]);
    if (args[0] === "query") {
      return { status: 0, stdout: "//packages/proto:home_proto\n//packages/proto:lib\n" };
    }
    const file = args.find((arg) => arg.startsWith("--target_pattern_file=")).split("=")[1];
    targets = fs.readFileSync(file, "utf8");
    return { status: 0 };
  };
  const saved = process.env.BAZEL_EXTRA_ARGS;
  process.env.BAZEL_EXTRA_ARGS = "--build_event_json_file=/tmp/x.json";
  try {
    const status = runCi(byId("proto"), { spawn, log: { write: () => {} } });
    assert.equal(status, 0);
  } finally {
    if (saved === undefined) {
      delete process.env.BAZEL_EXTRA_ARGS;
    } else {
      process.env.BAZEL_EXTRA_ARGS = saved;
    }
  }
  assert.deepEqual(calls[0], [
    "bazel",
    "query",
    "--output=label",
    "set(//packages/proto/...) except tests(set(//packages/proto/...))",
  ]);
  assert.deepEqual(calls[1].slice(0, 4), [
    "bazel",
    "test",
    "--build_event_json_file=/tmp/x.json",
    "--keep_going",
  ]);
  assert.equal(
    targets,
    "//packages/rust/home-proto:unit_tests\n//packages/proto:home_proto\n//packages/proto:lib\n",
  );
});

test("runCi honours the CI target lists and fails when the query fails", () => {
  const saved = { tests: process.env.BAZEL_TEST_TARGETS, builds: process.env.BAZEL_BUILD_TARGETS };
  process.env.BAZEL_TEST_TARGETS = "";
  process.env.BAZEL_BUILD_TARGETS = "//x/...";
  try {
    const status = runCi(byId("proto"), {
      spawn: () => ({ status: 7 }),
      log: { write: () => {} },
    });
    assert.equal(status, 7);
    assert.equal(ciPlan(byId("proto")).command, "build");
  } finally {
    for (const [key, name] of [["tests", "BAZEL_TEST_TARGETS"], ["builds", "BAZEL_BUILD_TARGETS"]]) {
      if (saved[key] === undefined) {
        delete process.env[name];
      } else {
        process.env[name] = saved[key];
      }
    }
  }
});
