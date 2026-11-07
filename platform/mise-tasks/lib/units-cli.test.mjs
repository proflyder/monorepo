#!/usr/bin/env node
// Unit tests for the mise unit dispatcher; run with `node --test`.

import assert from "node:assert/strict";
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
      id: "platform-deploy",
      platform: "go",
      title: "platform/deploy",
      paths: ["platform/deploy/**"],
      go: {
        module: ".",
        packages: ["./platform/deploy/..."],
        race: true,
        build: ["./platform/deploy/cmd/home-deploy"],
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
  assert.ok(suggestIds(ids, "deploy").includes("platform-deploy"));
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
    { command: "bazel", args: ["build", "//..."], cwd: ROOT },
  ]);
});

test("buildCommands uses build_targets or falls back to bazel roots", () => {
  assert.deepEqual(buildCommands(byId("proto"))[0].args, ["build", "//packages/proto/..."]);
  assert.deepEqual(buildCommands(byId("roots-only"))[0].args, [
    "build",
    "//solutions/voice-proxy/...",
  ]);
});

test("buildCommands rejects non-Bazel units with their own command", () => {
  assert.throws(
    () => buildCommands(byId("platform-deploy")),
    /platform 'go'.*\n.*go test \.\/platform\/deploy\/\.\.\./s,
  );
  assert.throws(() => buildCommands(byId("repository")), /mise run check/);
});

test("testCommands mirrors the Bazel workflow", () => {
  assert.deepEqual(testCommands(byId("proto"))[0].args, [
    "test",
    "//packages/rust/home-proto:unit_tests",
  ]);
});

test("testCommands runs go tests with race and the unit module", () => {
  const [command] = testCommands(byId("platform-deploy"));
  assert.equal(command.command, "go");
  assert.deepEqual(command.args, ["test", "-race", "./platform/deploy/..."]);
  assert.equal(command.cwd, path.join(ROOT, "."));
});

test("testCommands compiles and discovers Python suites", () => {
  const commands = testCommands(byId("ha-voice"));
  assert.equal(commands.length, 2);
  assert.deepEqual(commands[0].args, ["-m", "compileall", "-q", "projects/home-assistant/ha-voice"]);
  assert.deepEqual(commands[1].args.slice(0, 4), ["-m", "unittest", "discover", "-s"]);
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

test("unitHint names the platform command", () => {
  assert.match(unitHint(byId("platform-deploy")), /go unit/);
  assert.match(unitHint(byId("heartbeat")), /node --test/);
});
