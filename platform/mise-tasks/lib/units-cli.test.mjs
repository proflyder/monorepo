#!/usr/bin/env node
// Unit tests for the mise unit dispatcher; run with `node --test`.

import assert from "node:assert/strict";
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
  // `//...` includes the Android targets, so the Android config must be active
  // exactly as it was when these flags lived in the root `.bazelrc`.
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
  // Non-Android Bazel units must not get the Android-only flags.
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
  // `_android.yml` sets the release versionCode/versionName this way; the same
  // defines must reach both commands or Bazel builds two different APKs.
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
