#!/usr/bin/env node
/**
 * Unit tests for the CI affected-unit router and the unit-map validator.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import * as affected from "./affected.mjs";
import * as checkUnits from "./check_units.mjs";
import fixtureConfig from "./testdata/units-two-apps.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "../..");
const FIXTURE_GRAPH = path.join(HERE, "testdata", "graph-two-apps.xml");

function loadFixtureGraph() {
  return affected.BazelGraph.fromXml(fs.readFileSync(FIXTURE_GRAPH, "utf8"));
}

function withDeps(overrides, fn) {
  const saved = {};
  for (const [key, value] of Object.entries(overrides)) {
    saved[key] = affected.internals[key];
    affected.internals[key] = value;
  }
  try {
    return fn();
  } finally {
    Object.assign(affected.internals, saved);
  }
}

function withAsyncDeps(overrides, fn) {
  const saved = {};
  for (const [key, value] of Object.entries(overrides)) {
    saved[key] = affected.internals[key];
    affected.internals[key] = value;
  }
  return Promise.resolve(fn()).finally(() => {
    Object.assign(affected.internals, saved);
  });
}

function tempDir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "affected-test-"));
}

test("glob: double star matches any depth", () => {
  assert.equal(affected.pathMatches("docs/TESTING.md", ["docs/**"]), true);
  assert.equal(affected.pathMatches("docs/a/b/c.md", ["docs/**"]), true);
  assert.equal(affected.pathMatches("docsx/a.md", ["docs/**"]), false);
});

test("glob: double star prefix spans directories", () => {
  assert.equal(affected.pathMatches("a/b/tool.sh", ["**/*.sh"]), true);
  assert.equal(affected.pathMatches("tool.sh", ["**/*.sh"]), true);
});

test("glob: single star stays in one segment", () => {
  assert.equal(affected.pathMatches("platform/quality/x.py", ["platform/*/x.py"]), true);
  assert.equal(
    affected.pathMatches("platform/quality/deep/x.py", ["platform/*/x.py"]),
    false,
  );
});

test("glob: exact root file", () => {
  assert.equal(affected.pathMatches("Makefile", ["Makefile"]), true);
  assert.equal(affected.pathMatches("sub/Makefile", ["Makefile"]), false);
});

test("bazel graph: shared library change reaches only dependent app", () => {
  const graph = loadFixtureGraph();
  const touched = graph.affectedRules(["libs/shared/src/Shared.kt"]);
  assert.ok(touched.has("//libs/shared:shared"));
  assert.ok(touched.has("//libs/shared:shared_test"));
  assert.ok(touched.has("//apps/one:lib"));
  assert.ok(touched.has("//apps/one:one_test"));
  assert.ok(!touched.has("//apps/two:lib"));
  assert.ok(!touched.has("//apps/two:two_test"));
});

test("bazel graph: unknown file falls back to its package", () => {
  const graph = loadFixtureGraph();
  const touched = graph.affectedRules(["libs/shared/src/New.kt"]);
  assert.ok(touched.has("//libs/shared:shared"));
  assert.ok(touched.has("//apps/one:lib"));
});

test("bazel graph: label path conversion", () => {
  assert.equal(
    affected.BazelGraph.labelPath("//pkg/sub:src/main/A.kt"),
    "pkg/sub/src/main/A.kt",
  );
});

test("android fixture: shared library selects only its dependent application", () => {
  const config = affected.loadConfig(fixtureConfig);
  const graph = loadFixtureGraph();
  const result = withDeps({}, () =>
    affected.route(config, ["libs/shared/src/Shared.kt"], {
      root: REPO,
      base: "base-sha",
      graph,
    }),
  );
  const ids = result.routes.android.units.map((unit) => unit.id);
  assert.deepEqual(ids, ["app-one", "shared-kotlin"]);
  assert.ok(!ids.includes("app-two"));
});

test("android fixture: application change selects only that application", () => {
  const config = affected.loadConfig(fixtureConfig);
  const graph = loadFixtureGraph();
  const result = withDeps({}, () =>
    affected.route(config, ["apps/two/src/Two.kt"], {
      root: REPO,
      base: "base-sha",
      graph,
    }),
  );
  assert.deepEqual(result.routes.android.units.map((unit) => unit.id), ["app-two"]);
});

test("android fixture: module change selects every application", () => {
  const config = affected.loadConfig(fixtureConfig);
  const graph = loadFixtureGraph();
  const result = withDeps({}, () =>
    affected.route(config, ["MODULE.bazel"], {
      root: REPO,
      base: "base-sha",
      graph,
    }),
  );
  const ids = result.routes.android.units.map((unit) => unit.id);
  assert.deepEqual(ids, ["app-one", "app-two", "shared-kotlin"]);
  assert.equal(result.routes.android.mode, "all");
});

test("android fixture: maven virtual path only reaches android", () => {
  const config = affected.loadConfig(fixtureConfig);
  const graph = loadFixtureGraph();
  const result = withDeps({}, () =>
    affected.route(config, [affected.MODULE_BAZEL_MAVEN], {
      root: REPO,
      base: "base-sha",
      graph,
    }),
  );
  const ids = result.routes.android.units.map((unit) => unit.id);
  assert.deepEqual(ids, ["app-one", "app-two", "shared-kotlin"]);
  assert.equal(result.routes.android.mode, "all");
});

test("android fixture: docs-only change runs nothing but quality", () => {
  const config = affected.loadConfig(fixtureConfig);
  const graph = loadFixtureGraph();
  const result = withDeps({}, () =>
    affected.route(config, ["docs/ARCHITECTURE.md"], {
      root: REPO,
      base: "base-sha",
      graph,
    }),
  );
  assert.equal(result.docsOnly, true);
  for (const platform of ["android", "bazel", "python", "js", "native"]) {
    assert.deepEqual(result.routes[platform].units, [], platform);
  }
  assert.deepEqual(result.routes.quality.units.map((unit) => unit.id), ["repository"]);
});

test("android fixture: graph failure degrades to all android units", () => {
  const config = affected.loadConfig(fixtureConfig);
  const result = withDeps({}, () =>
    affected.route(config, ["libs/shared/src/Shared.kt"], {
      root: REPO,
      base: "base-sha",
      graph: null,
      graphLoader: () => {
        throw new Error("bazel query exploded");
      },
    }),
  );
  assert.equal(result.routes.android.mode, "all");
  assert.equal(result.routes.android.units.length, 3);
  assert.ok(result.warnings.some((warning) => warning.includes("bazel graph failed")));
});

test("android fixture: matrix entries are flat strings", () => {
  const config = affected.loadConfig(fixtureConfig);
  const graph = loadFixtureGraph();
  const result = withDeps({}, () =>
    affected.route(config, ["libs/shared/src/Shared.kt"], {
      root: REPO,
      base: "base-sha",
      graph,
    }),
  );
  const entries = affected.matrixEntries(result, config);
  assert.deepEqual(entries.android[0], {
    id: "app-one",
    test_targets: "//apps/one:one_test",
    build_targets: "//apps/one:app",
    artifact: "bazel-bin/apps/one/app.apk",
    artifact_name: "app-one-debug",
    sdk_packages: "",
    universe: "//apps/one/... + //apps/two/... + //libs/shared/...",
  });
});

test("android distribution always includes the unit that owns the APK", () => {
  // A change can route only a shared Kotlin unit (a new package the app does
  // not import yet) while still counting as an application input. The upload
  // job downloads `android-release-apk` instead of rebuilding, so the release
  // unit must be in the matrix whenever distribution is selected.
  const config = affected.loadConfig(fixtureConfig);
  const shared = config.get("shared-kotlin");
  const synthetic = (androidDistribute) => ({
    androidDistribute,
    routes: {
      android: { units: [shared] },
      bazel: { units: [] },
      python: { units: [] },
      js: { units: [] },
      native: { units: [] },
    },
  });
  assert.deepEqual(
    affected.matrixEntries(synthetic(true), config).android.map((entry) => entry.id),
    ["shared-kotlin", "app-one"],
  );
  assert.deepEqual(
    affected.matrixEntries(synthetic(false), config).android.map((entry) => entry.id),
    ["shared-kotlin"],
  );
});

function pythonUnit(overrides = {}) {
  return new affected.Unit({
    id: "py",
    platform: "python",
    title: "py",
    paths: ["py/**"],
    ...overrides,
  });
}

test("python units: suite without tests is rejected", () => {
  const tmp = tempDir();
  try {
    fs.mkdirSync(path.join(tmp, "py"));
    fs.writeFileSync(path.join(tmp, "py", "helper.py"), "X = 1\n");
    const config = new affected.Config({
      units: [pythonUnit({ suites: ["py"] })],
      invalidations: [],
    });
    const problems = checkUnits.validateSchema(config, tmp);
    assert.ok(
      problems.some((problem) => problem.includes("no test_*.py")),
      problems.join("\n"),
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test("python units: compile-only unit is accepted", () => {
  const tmp = tempDir();
  try {
    fs.mkdirSync(path.join(tmp, "py"));
    fs.writeFileSync(path.join(tmp, "py", "live_script.py"), "X = 1\n");
    const config = new affected.Config({
      units: [pythonUnit({ compileDirs: ["py"] })],
      invalidations: [],
    });
    assert.deepEqual(checkUnits.validateSchema(config, tmp), []);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test("python units: matrix entry carries compile dirs", () => {
  const config = affected.loadConfig(fixtureConfig);
  const result = withDeps({}, () =>
    affected.route(config, ["apps/two/src/Two.kt"], { root: REPO, base: "b" }),
  );
  const entries = affected.matrixEntries(result, config);
  assert.deepEqual(entries.python, []);
});

function nativeUnit(overrides = {}) {
  return new affected.Unit({
    id: "native",
    platform: "native",
    title: "native",
    paths: ["native/**"],
    nativeWorkdir: "native",
    nativeBuild: "./build.sh",
    nativeArtifacts: ["out/lib.dll"],
    nativeChecksums: "out/SHA256SUMS",
    nativeToolchainName: "cross-toolchain",
    nativeToolchainVersion: "1",
    nativeToolchainUrl: "https://example.test/toolchain.tar.xz",
    nativeToolchainSha256: "a".repeat(64),
    nativeToolchainRoot: "toolchain-1",
    ...overrides,
  });
}

function validateNative(unit) {
  const tmp = tempDir();
  try {
    fs.mkdirSync(path.join(tmp, "native"));
    const config = new affected.Config({ units: [unit], invalidations: [] });
    return checkUnits.validateSchema(config, tmp);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

test("native units: valid native unit is accepted", () => {
  assert.deepEqual(validateNative(nativeUnit()), []);
});

test("native units: missing artifacts or checksums is rejected", () => {
  const problems = validateNative(
    nativeUnit({ nativeArtifacts: [], nativeChecksums: "" }),
  );
  assert.ok(problems.some((problem) => problem.includes("without artifacts")), problems);
  assert.ok(
    problems.some((problem) => problem.includes("without a checksums file")),
    problems,
  );
});

test("native units: artifact paths must stay inside workdir", () => {
  const problems = validateNative(nativeUnit({ nativeArtifacts: ["/etc/passwd"] }));
  assert.ok(problems.some((problem) => problem.includes("relative to workdir")), problems);
});

test("native units: toolchain digest must be sha256", () => {
  const problems = validateNative(nativeUnit({ nativeToolchainSha256: "deadbeef" }));
  assert.ok(problems.some((problem) => problem.includes("sha256")), problems);
});

test("native units: partial toolchain is rejected", () => {
  const problems = validateNative(nativeUnit({ nativeToolchainRoot: "" }));
  assert.ok(problems.some((problem) => problem.includes("toolchain needs")), problems);
});

test("github output: multiline summary is written as heredoc", () => {
  const tmp = tempDir();
  try {
    const out = path.join(tmp, "output");
    fs.writeFileSync(out, "");
    affected.writeGithubOutput(out, { a: "1", summary: "line1\nline2" });
    const text = fs.readFileSync(out, "utf8");
    assert.ok(text.includes("a=1\n"));
    assert.ok(text.includes("summary<<EOF\nline1\nline2\nEOF\n"));
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test("real repo: schema has no problems", () => {
  const config = affected.loadConfig();
  assert.deepEqual(checkUnits.validateSchema(config), []);
});

test("real repo: every tracked source and test file is owned", () => {
  const config = affected.loadConfig();
  const files = checkUnits.trackedFiles(REPO);
  assert.ok(files.length > 100);
  assert.deepEqual(checkUnits.coverageProblems(config, files), []);
});

test("real repo: every known platform has a unit", () => {
  const config = affected.loadConfig();
  for (const platform of ["android", "python", "js", "native", "quality"]) {
    assert.ok(config.byPlatform(platform).length, platform);
  }
});

test("real repo: native provider change routes to its unit without publishing", () => {
  const config = affected.loadConfig();
  const result = withDeps({}, () =>
    affected.route(config, ["solutions/windows-logon-provider/crates/provider/src/worker.rs"], {
      root: REPO,
      base: "b",
    }),
  );
  assert.deepEqual(
    result.routes.native.units.map((unit) => unit.id),
    ["logon-provider"],
  );
  const entries = affected.matrixEntries(result, config);
  assert.deepEqual(entries.native, [
    {
      id: "logon-provider",
      kind: "script",
      publish: false,
      publish_script: "solutions/windows-logon-provider/publish-oci.sh",
      publish_attest_glob: "RicardoKompLogon.dll",
      publish_repository: "ghcr.io/proflyder/windows-logon-provider",
      publish_tags: "{version} sha-{commit12}",
      workdir: "solutions/windows-logon-provider",
      build: "./build.sh",
      test: "./run-tests.sh",
      artifacts: "out/RicardoKompLogon.dll out/cp-selftest.exe",
      checksums: "out/SHA256SUMS",
      toolchain_name: "",
      toolchain_version: "",
      toolchain_url: "",
      toolchain_sha256: "",
      toolchain_root: "",
    },
  ]);
});

test("real repo: native workflow change runs native units", () => {
  const config = affected.loadConfig();
  const result = withDeps({}, () =>
    affected.route(config, [".github/workflows/_native.yml"], { root: REPO, base: "b" }),
  );
  assert.equal(result.routes.native.mode, "all");
  assert.deepEqual(result.routes.native.units.map((unit) => unit.id), [
    "logon-provider",
    "pulsar",
  ]);
});

test("real repo: pulsar change routes to the native pulsar unit", () => {
  const config = affected.loadConfig();
  const result = withDeps({}, () =>
    affected.route(config, ["projects/station/pulsar/satellite/crates/pulsar/src/main.rs"], {
      root: REPO,
      base: "b",
    }),
  );
  assert.deepEqual(result.routes.native.units.map((unit) => unit.id), ["pulsar"]);
  const entries = affected.matrixEntries(result, config);
  assert.deepEqual(entries.native, [
    {
      id: "pulsar",
      kind: "pulsar",
      publish: false,
      publish_script: "projects/station/pulsar/publish-oci.sh",
      publish_attest_glob: "*",
      publish_repository: "ghcr.io/proflyder/pulsar-firmware",
      publish_tags: "{version} latest",
      workdir: "projects/station/pulsar",
      build: "",
      test: "",
      artifacts: "",
      checksums: "",
      toolchain_name: "",
      toolchain_version: "",
      toolchain_url: "",
      toolchain_sha256: "",
      toolchain_root: "",
    },
  ]);
});

test("real repo: pulsar and the logon provider publish only when VERSION changed", () => {
  const config = affected.loadConfig();
  const entryFor = (changed, id) => {
    const result = withDeps({}, () =>
      affected.route(config, changed, { root: REPO, base: "b" }),
    );
    return affected.matrixEntries(result, config).native.find((item) => item.id === id);
  };
  // A source change is PR CI only: the unit is built, but not published.
  assert.equal(entryFor(["projects/station/pulsar/satellite/crates/pulsar/src/main.rs"], "pulsar").publish, false);
  assert.equal(entryFor(["projects/station/pulsar/VERSION"], "pulsar").publish, true);
  // A global invalidation does not turn an unchanged VERSION into a release.
  assert.equal(entryFor(["MODULE.bazel"], "pulsar").publish, false);

  // The logon provider follows the same release rule: source changes are
  // verified in the PR, only a VERSION bump publishes.
  assert.equal(
    entryFor(["solutions/windows-logon-provider/crates/provider/src/worker.rs"], "logon-provider")
      .publish,
    false,
  );
  assert.equal(
    entryFor(["solutions/windows-logon-provider/VERSION"], "logon-provider").publish,
    true,
  );
  assert.equal(entryFor(["MODULE.bazel"], "logon-provider").publish, false);
});

test("real repo: js worker change routes to its unit", () => {
  const config = affected.loadConfig();
  const result = withDeps({}, () =>
    affected.route(config, ["solutions/heartbeat/src/decide.js"], {
      root: REPO,
      base: "b",
    }),
  );
  assert.deepEqual(result.routes.js.units.map((unit) => unit.id), ["heartbeat"]);
  const entries = affected.matrixEntries(result, config);
  assert.deepEqual(entries.js, [{ id: "heartbeat", tests: "solutions/heartbeat/src" }]);
});

test("real repo: graph fixture exists and android roots cover the app", () => {
  const config = affected.loadConfig();
  const graphFile = path.join(HERE, "testdata", "graph-two-apps.xml");
  assert.ok(fs.existsSync(graphFile));
  const app = config.get("android-app");
  assert.ok(app.bazelRoots.some((root) => root.startsWith("//projects/mobile/android")));
});

test("real repo: android lint unit is a separate job with the same paths", () => {
  const config = affected.loadConfig();
  const app = config.get("android-app");
  const lint = config.get("android-lint");
  assert.deepEqual(lint.paths, app.paths);
  assert.deepEqual(lint.bazelRoots, app.bazelRoots);
  assert.deepEqual(lint.testTargets, ["//projects/mobile/android:android_lint"]);
  assert.deepEqual(lint.buildTargets, []);
  assert.deepEqual(lint.bazelSdkPackages, [
    "cmdline-tools;23.0",
    "ndk;28.2.13676358",
  ]);
});

test("real repo: android module change selects the app and the lint unit", () => {
  const config = affected.loadConfig();
  const result = withDeps({}, () =>
    affected.route(config, ["projects/mobile/android/features/alarms/impl/AlarmsFragment.kt"], {
      root: REPO,
      base: "b",
      graph: { affectedRules: () => new Set() },
    }),
  );
  assert.deepEqual(
    result.routes.android.units.map((unit) => unit.id),
    ["android-app", "android-lint"],
  );
});

test("real repo: shared kotlin change selects the lint unit through the graph", () => {
  const config = affected.loadConfig();
  const result = withDeps({}, () =>
    affected.route(config, ["packages/kotlin/core/src/main/A.kt"], {
      root: REPO,
      base: "b",
      graph: {
        affectedRules: () => new Set(["//projects/mobile/android/app:app_lib"]),
      },
    }),
  );
  assert.deepEqual(
    result.routes.android.units.map((unit) => unit.id),
    ["android-app", "android-lint", "kotlin-shared"],
  );
});

test("real repo: the Android-only bazelrc selects only android units", () => {
  // Android-specific flags live in platform/android/android.bazelrc, activated
  // with --config=android. A change there must not invalidate every unit the
  // way a root `.bazelrc` change does.
  const result = routeReal(["platform/android/android.bazelrc"]);
  assert.equal(result.routes.android.mode, "all");
  assert.deepEqual(
    result.routes.android.units.map((unit) => unit.id),
    ["android-app", "android-lint", "kotlin-shared"],
  );
  for (const platform of ["bazel", "python", "js", "native"]) {
    assert.deepEqual(result.routes[platform].units, [], platform);
  }
});

test("real repo: lint rule change runs every android unit including lint", () => {
  const config = affected.loadConfig();
  const result = routeReal(["platform/android/lint.bzl"]);
  assert.equal(result.routes.android.mode, "all");
  assert.deepEqual(
    result.routes.android.units.map((unit) => unit.id),
    ["android-app", "android-lint", "kotlin-shared"],
  );
  const entry = affected
    .matrixEntries(result, config)
    .android.find((item) => item.id === "android-lint");
  assert.deepEqual(entry, {
    id: "android-lint",
    test_targets: "//projects/mobile/android:android_lint",
    build_targets: "",
    artifact: "",
    artifact_name: "",
    sdk_packages: "cmdline-tools;23.0 ndk;28.2.13676358",
    universe: "//packages/kotlin/... + //projects/mobile/android/...",
  });
});

test("real repo: github format is a valid JSON payload", async () => {
  const config = affected.loadConfig();
  const result = withDeps({}, () =>
    affected.route(config, ["docs/TESTING.md"], { root: REPO, base: "b", graph: null }),
  );
  const tmp = tempDir();
  try {
    const out = path.join(tmp, "output");
    fs.writeFileSync(out, "");
    const savedEnv = process.env.GITHUB_OUTPUT;
    process.env.GITHUB_OUTPUT = out;
    try {
      await withAsyncDeps(
        {
          resolveBase: () => "base-sha",
          changedFiles: () => ["docs/TESTING.md"],
          route: () => result,
        },
        () => affected.main(["--format", "github"]),
      );
    } finally {
      if (savedEnv === undefined) {
        delete process.env.GITHUB_OUTPUT;
      } else {
        process.env.GITHUB_OUTPUT = savedEnv;
      }
    }
    const values = {};
    for (const line of fs.readFileSync(out, "utf8").split("\n")) {
      if (line.includes("=") && !line.includes("<<")) {
        const index = line.indexOf("=");
        values[line.slice(0, index)] = line.slice(index + 1);
      }
    }
    assert.equal(values.docs_only, "true");
    assert.equal(values.android_distribute, "false");
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

function routeReal(changed, deps, graph = null) {
  const config = affected.loadConfig();
  return withDeps(
    {},
    () => affected.route(config, changed, { root: REPO, base: "b", graph }),
  );
}

test("deploy: docs-only change deploys nothing", () => {
  const config = affected.loadConfig();
  const result = routeReal(["docs/STATE.md"]);
  assert.deepEqual(affected.deployEntries(result, config), []);
});

test("deploy: platform/deploy-rs change runs the Rust unit and routes deploy units", () => {
  const config = affected.loadConfig();
  const result = routeReal(["platform/deploy-rs/src/render.rs"], undefined, {
    affectedRules: () => new Set(),
  });
  assert.deepEqual(result.routes.bazel.units.map((unit) => unit.id), [
    "platform-deploy",
    "scheduler",
    "wakegrant",
    "intercom",
    "currency-bot",
    "mcp-gateway",
    "ha-bridge",
  ]);
  // A tooling change reaches every unit that runs a deploy script.
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id).sort(), [
    "currency-bot",
    "ha-bridge",
    "hermes",
    "intercom",
    "mcp-gateway",
    "scheduler",
    "wakegrant",
  ]);
});

test("deploy: scheduler change deploys only scheduler", () => {
  const config = affected.loadConfig();
  const result = routeReal(
    ["solutions/scheduler/crates/scheduler/src/api/service.rs"],
    undefined,
    {
      affectedRules: () =>
        new Set(["//solutions/scheduler/crates/scheduler:scheduler_lib"]),
    },
  );
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["scheduler"]);
  assert.equal(entries[0].namespace, "home-assistant");
  assert.equal(entries[0].target_root, "//projects/home/kubernetes/scheduler");
});

test("deploy: scheduler Rust crate change deploys scheduler", () => {
  const config = affected.loadConfig();
  const result = routeReal(
    ["solutions/scheduler/crates/scheduler/src/app.rs"],
    undefined,
    {
      affectedRules: () =>
        new Set(["//solutions/scheduler/crates/scheduler:scheduler"]),
    },
  );
  assert.deepEqual(result.routes.bazel.units.map((unit) => unit.id), ["scheduler"]);
  assert.equal(result.deployControl, true);
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["scheduler"]);
});

test("deploy: currency-bot change deploys its namespace", () => {
  const config = affected.loadConfig();
  const result = routeReal(
    ["solutions/currency-bot/crates/currency-bot/src/api/webhook.rs"],
    undefined,
    {
      affectedRules: () =>
        new Set(["//solutions/currency-bot/crates/currency-bot:currency_bot_lib"]),
    },
  );
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["currency-bot"]);
  assert.equal(entries[0].namespace, "currency-bot");
  assert.equal(entries[0].target_root, "//solutions/currency-bot");
  assert.equal(entries[0].script, "projects/home/kubernetes/currency-bot/deploy.sh");
  assert.equal(entries[0].externalsecrets, "currency-bot cloudflared-tunnel");
});

test("deploy: currency-bot manifest change deploys it", () => {
  const config = affected.loadConfig();
  const result = routeReal(
    ["projects/home/kubernetes/currency-bot/manifests/currency-bot.yaml"],
    undefined,
    {
      affectedRules: () =>
        new Set(["//projects/home/kubernetes/currency-bot:deploy_templates"]),
    },
  );
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["currency-bot"]);
});

test("deploy: ha-bridge change deploys bridge and control", () => {
  const config = affected.loadConfig();
  const result = routeReal(
    ["solutions/ha-bridge/crates/ha-bridge/src/bridge.rs"],
    undefined,
    {
      affectedRules: () =>
        new Set(["//solutions/ha-bridge/crates/ha-bridge:ha-bridge"]),
    },
  );
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["ha-bridge"]);
  assert.equal(entries[0].images, "ha-bridge ha-control ha-scenario");
  assert.equal(entries[0].workloads, "ha-bridge ha-control ha-scenario");
});

test("deploy: Rust ha-control crate change deploys both services", () => {
  const config = affected.loadConfig();
  const result = routeReal(
    ["solutions/ha-bridge/crates/ha-bridge/src/control.rs"],
    undefined,
    {
      affectedRules: () =>
        new Set(["//solutions/ha-bridge/crates/ha-bridge:ha-control"]),
    },
  );
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["ha-bridge"]);
  assert.equal(entries[0].images, "ha-bridge ha-control ha-scenario");
});

test("deploy: shared proto package deploys scheduler and ha-bridge", () => {
  // Both Rust services are generated from the shared home.v1 schema, so both
  // units declare the proto path explicitly.
  const config = affected.loadConfig();
  const result = routeReal(
    ["packages/proto/home/v1/scheduler.proto"],
    undefined,
    { affectedRules: () => new Set() },
  );
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["scheduler", "ha-bridge"]);
});

test("deploy: hermes change selects its images", () => {
  const config = affected.loadConfig();
  const result = routeReal(["projects/home-assistant/hermes/README.md"]);
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["hermes"]);
  assert.equal(entries[0].images, "hermes voice-proxy health-exporter home-deploy");
  assert.equal(entries[0].builder, "mixed");
  assert.equal(entries[0].target_root, "");
  assert.deepEqual(JSON.parse(entries[0].bazel_images), {
    "voice-proxy": "//solutions/voice-proxy",
    "health-exporter": "//projects/home-assistant/deploy/health-exporter",
    "home-deploy": "//projects/home/kubernetes/scheduler",
  });
});

test("deploy: hermes Dockerfile change deploys the hermes unit", () => {
  const config = affected.loadConfig();
  const result = routeReal(["projects/home-assistant/deploy/images/hermes/Dockerfile"]);
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["hermes"]);
});

test("deploy: hermes upstream revision bump deploys the hermes unit", () => {
  const config = affected.loadConfig();
  const result = routeReal(["projects/home-assistant/deploy/images/hermes/revision"]);
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["hermes"]);
});

test("deploy: voice-proxy solution change runs its bazel unit and deploys hermes", () => {
  const config = affected.loadConfig();
  const result = affected.route(
    config,
    ["solutions/voice-proxy/crates/voice-proxy/src/api.rs"],
    { root: REPO, base: "b", graph: { affectedRules: () => new Set() } },
  );
  assert.deepEqual(result.routes.bazel.units.map((unit) => unit.id), ["voice-proxy"]);
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["hermes"]);
});

test("deploy: a shared rust crate change reaches the hermes deploy unit", () => {
  const config = affected.loadConfig();
  const result = affected.route(
    config,
    ["packages/rust/home-ha/src/lib.rs"],
    {
      root: REPO,
      base: "b",
      // A real reverse-dependency closure reaches voice-proxy, which is one of
      // hermes' Bazel images; home_ha alone is not enough to select a deploy.
      graph: {
        affectedRules: () =>
          new Set([
            "//packages/rust/home-ha:home_ha",
            "//solutions/voice-proxy/crates/voice-proxy:voice_proxy",
          ]),
      },
    },
  );
  const entries = affected.deployEntries(result, config).map((entry) => entry.id);
  assert.ok(entries.includes("hermes"), entries.join(","));
});

test("rust cutover: a crate change runs the bazel unit and deploys it", () => {
  const config = affected.loadConfig();
  const result = routeReal(["solutions/currency-bot/crates/currency-bot/src/lib.rs"]);
  const bazelIds = result.routes.bazel.units.map((unit) => unit.id);
  assert.ok(bazelIds.includes("currency-bot"), bazelIds.join(","));
  assert.ok(!bazelIds.includes("currency-bot-rust"), bazelIds.join(","));
  const unit = config.units.find((item) => item.id === "currency-bot");
  assert.ok(unit, "currency-bot unit exists");
  assert.deepEqual(unit.deployImages, ["currency-bot"]);
  assert.deepEqual(unit.deployWorkloads, ["currency-bot"]);
});

test("rust cutover: a shared crate change also runs the bazel unit", () => {
  const result = routeReal(["packages/rust/home-telegram/src/lib.rs"]);
  const bazelIds = result.routes.bazel.units.map((unit) => unit.id);
  assert.ok(bazelIds.includes("currency-bot"), bazelIds.join(","));
});

// --------------------------------------------------------------------------
// Precise Rust routing (shared crates, Cargo.lock, crate_universe)
// --------------------------------------------------------------------------
function routeRust(changed, graph, deps = {}) {
  const config = affected.loadConfig();
  return withDeps(deps, () =>
    affected.route(config, changed, { root: REPO, base: "b", graph }),
  );
}

test("rust routing: home-ha change selects its consumers, not pulsar/provider", () => {
  const result = routeRust(["packages/rust/home-ha/src/lib.rs"], {
    affectedRules: () =>
      new Set([
        "//packages/rust/home-ha:home_ha",
        "//solutions/ha-bridge/crates/ha-bridge:ha_bridge_lib",
        "//solutions/voice-proxy/crates/voice-proxy:voice_proxy",
      ]),
  });
  const bazelIds = result.routes.bazel.units.map((unit) => unit.id);
  assert.deepEqual(bazelIds, ["voice-proxy", "ha-bridge"]);
  assert.ok(!bazelIds.includes("scheduler"), bazelIds.join(","));
  const nativeIds = result.routes.native.units.map((unit) => unit.id);
  assert.deepEqual(nativeIds, []);
  assert.ok(!nativeIds.includes("pulsar"));
  assert.ok(!nativeIds.includes("logon-provider"));
  // The voice-proxy image is deployed through `hermes`.
  assert.deepEqual(result.routes.python.units.map((unit) => unit.id), ["hermes"]);
});

test("rust routing: a shared TLS crate selects only its consumers", () => {
  const result = routeRust(["packages/rust/home-tls/src/lib.rs"], {
    affectedRules: () =>
      new Set([
        "//packages/rust/home-tls:home_tls",
        "//projects/home-assistant/deploy/health-exporter/crates/health-exporter:health_exporter",
        "//solutions/currency-bot/crates/currency-bot:currency_bot_lib",
        "//solutions/mcp-gateway/crates/mcp-gateway:mcp_gateway_lib",
      ]),
  });
  assert.deepEqual(result.routes.bazel.units.map((unit) => unit.id), [
    "health-exporter",
    "currency-bot",
    "mcp-gateway",
  ]);
  assert.deepEqual(result.routes.native.units.map((unit) => unit.id), []);
  assert.deepEqual(result.routes.python.units.map((unit) => unit.id), ["hermes"]);
});

test("rust routing: wakegrant-protocol reaches the native logon provider", () => {
  const result = routeRust(["packages/rust/wakegrant-protocol/src/lib.rs"], {
    affectedRules: () =>
      new Set([
        "//packages/rust/wakegrant-protocol:wakegrant_protocol",
        "//solutions/windows-logon-provider/crates/provider:provider",
      ]),
  });
  assert.deepEqual(result.routes.native.units.map((unit) => unit.id), ["logon-provider"]);
  assert.ok(!result.routes.native.units.some((unit) => unit.id === "pulsar"));
});

const CARGO_LOCK_BASE = `version = 4

[[package]]
name = "ha-bridge"
version = "0.1.0"
dependencies = [
 "home-ha",
]

[[package]]
name = "home-ha"
version = "0.1.0"
dependencies = [
 "serde",
]

[[package]]
name = "serde"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaa"
`;

const CARGO_LOCK_HEAD = `version = 4

[[package]]
name = "ha-bridge"
version = "0.1.0"
dependencies = [
 "home-ha",
]

[[package]]
name = "home-ha"
version = "0.1.0"
dependencies = [
 "serde",
 "tokio-tungstenite",
]

[[package]]
name = "serde"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaa"

[[package]]
name = "tokio-tungstenite"
version = "0.28.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "bbb"
`;

function universeLock(withNewCrate) {
  const crates = {
    "ha-bridge 0.1.0": {
      name: "ha-bridge",
      version: "0.1.0",
      common_attrs: { deps: [{ id: "home-ha 0.1.0" }] },
    },
    "home-ha 0.1.0": {
      name: "home-ha",
      version: "0.1.0",
      common_attrs: {
        deps: withNewCrate
          ? [{ id: "serde 1.0.0" }, { id: "tokio-tungstenite 0.28.0" }]
          : [{ id: "serde 1.0.0" }],
      },
    },
    "serde 1.0.0": {
      name: "serde",
      version: "1.0.0",
      common_attrs: { deps: [] },
    },
  };
  if (withNewCrate) {
    crates["tokio-tungstenite 0.28.0"] = {
      name: "tokio-tungstenite",
      version: "0.28.0",
      common_attrs: { deps: [{ id: "serde 1.0.0" }] },
    };
  }
  return JSON.stringify(
    {
      crates,
      direct_deps: withNewCrate
        ? ["serde 1.0.0", "tokio-tungstenite 0.28.0"]
        : ["serde 1.0.0"],
      workspace_members: {
        "ha-bridge 0.1.0": "solutions/ha-bridge/crates/ha-bridge",
        "home-ha 0.1.0": "packages/rust/home-ha",
      },
    },
    null,
    1,
  );
}

const CARGO_TOML_BASE = `[workspace]
members = ["packages/rust/*"]

[workspace.dependencies]
serde = "=1.0.0"
`;

const CARGO_TOML_HEAD = `[workspace]
members = ["packages/rust/*"]

[workspace.dependencies]
serde = "=1.0.0"
tokio-tungstenite = "=0.28.0"
`;

function lockFixtureDeps() {
  return {
    gitFileText: (_root, revision, filePath) => {
      const base = revision === "base-sha";
      if (filePath === "Cargo.toml") {
        return base ? CARGO_TOML_BASE : CARGO_TOML_HEAD;
      }
      if (filePath === "Cargo.lock") {
        return base ? CARGO_LOCK_BASE : CARGO_LOCK_HEAD;
      }
      if (filePath === "crate-universe.lock") {
        return universeLock(!base);
      }
      return null;
    },
  };
}

test("rust routing: lock dependency spelling is resolved before diffing", () => {
  const base = `version = 4

[[package]]
name = "ureq"
version = "3.5.1"
dependencies = [
 "webpki-roots",
]

[[package]]
name = "webpki-roots"
version = "1.0.11"
checksum = "one"
`;
  const head = `version = 4

[[package]]
name = "ureq"
version = "3.5.1"
dependencies = [
 "webpki-roots 1.0.11",
]

[[package]]
name = "webpki-roots"
version = "0.26.11"
checksum = "old"
dependencies = [
 "webpki-roots 1.0.11",
]

[[package]]
name = "webpki-roots"
version = "1.0.11"
checksum = "one"
`;
  const changes = affected.cargoLockChanges(base, head);
  // ureq resolves to the same webpki-roots 1.0.11 in both locks.
  assert.ok(!changes.has("ureq 3.5.1"), [...changes].join(","));
  assert.ok(changes.has("webpki-roots 0.26.11"));
});

test("rust routing: a lock diff routes only the new crate's consumers", () => {
  const config = affected.loadConfig();
  let seenLabels = null;
  const result = withDeps(lockFixtureDeps(), () =>
    affected.route(
      config,
      ["Cargo.toml", "Cargo.lock", "crate-universe.lock"],
      {
        root: REPO,
        base: "base-sha",
        head: "head-sha",
        graph: {
          affectedRules: (changed, labels) => {
            seenLabels = [...labels];
            const out = new Set();
            if (labels.includes("@crates//:tokio-tungstenite")) {
              out.add("//solutions/ha-bridge/crates/ha-bridge:ha_bridge_lib");
              out.add("//solutions/voice-proxy/crates/voice-proxy:voice_proxy");
            }
            if (changed.some((item) => item.startsWith("packages/rust/home-ha"))) {
              out.add("//solutions/ha-bridge/crates/ha-bridge:ha_bridge_lib");
            }
            return out;
          },
        },
      },
    ),
  );
  assert.deepEqual(seenLabels, ["@crates//:tokio-tungstenite"]);
  assert.deepEqual(result.routes.bazel.units.map((unit) => unit.id), [
    "voice-proxy",
    "ha-bridge",
  ]);
  assert.deepEqual(result.routes.native.units.map((unit) => unit.id), []);
  assert.ok(result.warnings.some((warning) => warning.includes("lockfile diff")));
});

test("rust routing: test-only consumers run CI but do not deploy", () => {
  const config = affected.loadConfig();
  const graph = new affected.BazelGraph({
    rules: {
      "//solutions/ha-bridge/crates/ha-bridge:ha_bridge_lib": "rust_library",
      "//solutions/mcp-gateway/crates/mcp-gateway:voice_tests": "rust_test",
    },
    inputs: {
      "//solutions/ha-bridge/crates/ha-bridge:ha_bridge_lib": new Set([
        "@crates//:tokio-tungstenite",
      ]),
      "//solutions/mcp-gateway/crates/mcp-gateway:voice_tests": new Set([
        "@crates//:tokio-tungstenite",
      ]),
    },
  });
  const result = withDeps(lockFixtureDeps(), () =>
    affected.route(config, ["Cargo.lock", "crate-universe.lock"], {
      root: REPO,
      base: "base-sha",
      head: "head-sha",
      graph,
    }),
  );
  assert.deepEqual(result.routes.bazel.units.map((unit) => unit.id), [
    "mcp-gateway",
    "ha-bridge",
  ]);
  assert.deepEqual(
    affected.deployEntries(result, config).map((entry) => entry.id),
    ["ha-bridge"],
  );
});

test("rust routing: a changed package with no alias or workspace path is conservative", () => {
  const config = affected.loadConfig();
  const orphanLock = `${CARGO_LOCK_BASE}
[[package]]
name = "orphan"
version = "0.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "ccc"
`;
  const result = withDeps(
    {
      gitFileText: (_root, revision, filePath) => {
        const base = revision === "base-sha";
        if (filePath === "Cargo.lock") {
          return base ? CARGO_LOCK_BASE : orphanLock;
        }
        if (filePath === "crate-universe.lock") {
          return universeLock(false);
        }
        return null;
      },
    },
    () =>
      affected.route(config, ["Cargo.lock", "crate-universe.lock"], {
        root: REPO,
        base: "base-sha",
        head: "head-sha",
      }),
  );
  assert.equal(result.routes.bazel.mode, "all");
  assert.equal(result.routes.native.mode, "all");
  assert.ok(
    result.warnings.some(
      (warning) => warning.includes("orphan") && warning.includes("did not map"),
    ),
    result.warnings.join("\n"),
  );
});

test("rust routing: workspace-dependency edit without a lock diff is conservative", () => {
  const config = affected.loadConfig();
  const result = withDeps(
    {
      gitFileText: (_root, revision, filePath) =>
        filePath === "Cargo.toml"
          ? revision === "base-sha"
            ? CARGO_TOML_BASE
            : CARGO_TOML_HEAD
          : null,
    },
    () =>
      affected.route(config, ["Cargo.toml"], {
        root: REPO,
        base: "base-sha",
        head: "head-sha",
      }),
  );
  assert.equal(result.routes.bazel.mode, "all");
  assert.equal(result.routes.native.mode, "all");
  assert.ok(
    result.warnings.some((warning) => warning.includes("no lockfile changed")),
    result.warnings.join("\n"),
  );
});

test("rust routing: changing [profile] in Cargo.toml runs every Rust unit", () => {
  const config = affected.loadConfig();
  const changedManifest = CARGO_TOML_BASE.replace(
    "[workspace.dependencies]",
    "[profile.release]\nlto = true\n\n[workspace.dependencies]",
  );
  const result = withDeps(
    {
      gitFileText: (_root, revision, filePath) =>
        filePath === "Cargo.toml"
          ? revision === "base-sha"
            ? CARGO_TOML_BASE
            : changedManifest
          : null,
    },
    () =>
      affected.route(config, ["Cargo.toml"], {
        root: REPO,
        base: "base-sha",
        head: "head-sha",
      }),
  );
  assert.equal(result.routes.bazel.mode, "all");
  assert.equal(result.routes.native.mode, "all");
});

test("rust routing: an unreadable Cargo.lock falls back to every Rust unit", () => {
  const config = affected.loadConfig();
  const result = withDeps(
    {
      gitFileText: (_root, revision, filePath) => {
        if (filePath === "Cargo.lock") {
          return revision === "base-sha" ? CARGO_LOCK_BASE : "not a lockfile";
        }
        if (filePath === "crate-universe.lock") {
          return universeLock(revision !== "base-sha");
        }
        return null;
      },
    },
    () =>
      affected.route(config, ["Cargo.lock"], {
        root: REPO,
        base: "base-sha",
        head: "head-sha",
      }),
  );
  assert.equal(result.routes.bazel.mode, "all");
  assert.equal(result.routes.native.mode, "all");
  assert.ok(result.warnings.some((warning) => warning.includes("fell back")));
});

test("rust routing: rust-toolchain.toml still runs every Rust unit", () => {
  const result = routeReal(["rust-toolchain.toml"]);
  assert.equal(result.routes.bazel.mode, "all");
  assert.equal(result.routes.native.mode, "all");
});

test("rust routing: a bazel query failure falls back to every Rust unit", () => {
  const config = affected.loadConfig();
  const result = withDeps({}, () =>
    affected.route(config, ["packages/rust/home-ha/src/lib.rs"], {
      root: REPO,
      base: "b",
      graph: null,
      graphLoader: () => {
        throw new Error("query boom");
      },
    }),
  );
  assert.equal(result.routes.bazel.mode, "all");
  assert.equal(result.routes.native.mode, "all");
  assert.ok(
    result.warnings.some((warning) => warning.includes("bazel graph failed")),
  );
});


test("deploy: ci workflow change runs checks but deploys nothing", () => {
  const config = affected.loadConfig();
  const result = routeReal([".github/workflows/ci.yml"]);
  for (const platform of ["android", "python", "js"]) {
    assert.ok(result.routes[platform].units.length, platform);
  }
  assert.deepEqual(affected.deployEntries(result, config), []);
});

test("deploy: workflow and service change deploys only the service", () => {
  const config = affected.loadConfig();
  const result = routeReal(
    [".github/workflows/ci.yml", "solutions/currency-bot/crates/currency-bot/src/lib.rs"],
    undefined,
    {
      affectedRules: () =>
        new Set(["//solutions/currency-bot/crates/currency-bot:currency_bot_lib"]),
    },
  );
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["currency-bot"]);
});

test("deploy: module change still deploys every deployable unit", () => {
  const config = affected.loadConfig();
  const result = routeReal(["MODULE.bazel"]);
  const entries = affected.deployEntries(result, config);
  const expected = config.units.filter((unit) => unit.deployImages.length).map((unit) => unit.id);
  assert.deepEqual(entries.map((entry) => entry.id), expected);
  assert.equal(entries.length, expected.length);
});

test("android distribution: module change runs tests without distributing", () => {
  const config = affected.loadConfig();
  const result = routeReal(["MODULE.bazel"]);
  const ids = result.routes.android.units.map((unit) => unit.id);
  assert.ok(ids.includes("android-app"), "android tests must still run");
  assert.equal(result.routes.android.mode, "all");
  assert.equal(result.androidDistribute, false);
});

test("android distribution: application change distributes", () => {
  assert.equal(
    routeReal(["projects/mobile/android/app/BUILD.bazel"]).androidDistribute,
    true,
  );
});

test("android distribution: shared kotlin change distributes", () => {
  assert.equal(
    routeReal(["packages/kotlin/core/src/main/Kotlin.kt"]).androidDistribute,
    true,
  );
});

test("android distribution: platform android change distributes", () => {
  assert.equal(routeReal(["platform/android/defs.bzl"]).androidDistribute, true);
});

test("android distribution: workflow and docs changes do not distribute", () => {
  for (const changed of [
    ".github/workflows/ci.yml",
    ".github/workflows/_android.yml",
    "docs/STATE.md",
  ]) {
    assert.equal(routeReal([changed]).androidDistribute, false, changed);
  }
});

test("android distribution: markdown inside android tree does not distribute", () => {
  for (const changed of [
    "projects/mobile/android/README.md",
    "packages/kotlin/core/README.md",
  ]) {
    assert.equal(routeReal([changed]).androidDistribute, false, changed);
  }
  assert.equal(
    routeReal([
      "projects/mobile/android/README.md",
      "projects/mobile/android/app/BUILD.bazel",
    ]).androidDistribute,
    true,
  );
});

test("android distribution: paths cover android units and platform tooling", () => {
  const config = affected.loadConfig();
  const paths = affected.androidDistributePaths(config);
  for (const pattern of [
    "projects/mobile/android/**",
    "packages/kotlin/**",
    "platform/android/**",
  ]) {
    assert.ok(paths.includes(pattern), pattern);
  }
});

test("workflow invalidation: deploy-control workflow runs every platform", () => {
  const result = routeReal([".github/workflows/_deploy-control.yml"]);
  assert.equal(result.routes.android.mode, "all");
  assert.ok(result.routes.android.units.length);
});

test("workflow invalidation: android workflow runs android", () => {
  const result = routeReal([".github/workflows/_android.yml"]);
  assert.equal(result.routes.android.mode, "all");
  assert.ok(result.routes.android.units.length);
});

test("workflow invalidation: native workflow runs native", () => {
  const result = routeReal([".github/workflows/_native.yml"]);
  assert.equal(result.routes.native.mode, "all");
  assert.ok(result.routes.native.units.length);
});

test("workflow invalidation: ci entrypoint runs everything", () => {
  const result = routeReal([".github/workflows/ci.yml"]);
  for (const platform of ["android", "bazel", "python", "js", "native"]) {
    assert.ok(result.routes[platform].units.length, platform);
  }
});

// Modelled on the real 49937f5..54a0fe3 diff (PR #90): two Firebase artifacts
// and a comment were added inside `maven.install(...)`; everything else stayed.
const MODULE_BAZEL_MAVEN_OLD = `module(name = "home")

bazel_dep(name = "rules_jvm_external", version = "6.6")

maven = use_extension("@rules_jvm_external//:extensions.bzl", "maven")
maven.install(
    name = "maven",
    artifacts = [
        "com.google.firebase:firebase-common:21.0.0",
        "com.google.firebase:firebase-config-interop:16.0.1",
        "com.google.firebase:firebase-crashlytics:19.4.4",
    ],
    repositories = ["https://maven.google.com"],
)
use_repo(maven, "maven")

other = use_extension("@example//:extensions.bzl", "other")
use_repo(other, "com_example")

# A \`#\` and parentheses in a string must not confuse the tokenizer.
MARKER = "value (with # parens)"
`;

const MODULE_BAZEL_MAVEN_NEW = `module(name = "home")

bazel_dep(name = "rules_jvm_external", version = "6.6")

maven = use_extension("@rules_jvm_external//:extensions.bzl", "maven")
maven.install(
    name = "maven",
    artifacts = [
        "com.google.firebase:firebase-common:21.0.0",
        "com.google.firebase:firebase-config-interop:16.0.1",
        # Remote Config (endpoints/timeouts): abt is its A/B-testing runtime
        # dependency (from the firebase-config POM).
        "com.google.firebase:firebase-abt:21.1.1",
        "com.google.firebase:firebase-config:22.1.2",
        "com.google.firebase:firebase-crashlytics:19.4.4",
    ],
    repositories = ["https://maven.google.com"],
)
use_repo(maven, "maven")

other = use_extension("@example//:extensions.bzl", "other")
use_repo(other, "com_example")

# A \`#\` and parentheses in a string must not confuse the tokenizer.
MARKER = "value (with # parens)"
`;

test("module bazel: maven-only edit is scoped", () => {
  assert.equal(
    affected.moduleBazelIsMavenOnly(MODULE_BAZEL_MAVEN_OLD, MODULE_BAZEL_MAVEN_NEW),
    true,
  );
});

test("module bazel: use_repo maven edit is scoped", () => {
  const updated = MODULE_BAZEL_MAVEN_OLD.replace(
    'use_repo(maven, "maven")',
    'use_repo(maven, "maven", "maven_extra")',
  );
  assert.equal(affected.moduleBazelIsMavenOnly(MODULE_BAZEL_MAVEN_OLD, updated), true);
});

test("module bazel: bazel_dep edit is global", () => {
  const updated = MODULE_BAZEL_MAVEN_OLD.replace('version = "6.6"', 'version = "7.0"');
  assert.equal(affected.moduleBazelIsMavenOnly(MODULE_BAZEL_MAVEN_OLD, updated), false);
});

test("module bazel: mixed maven and bazel_dep edit is global", () => {
  const updated = MODULE_BAZEL_MAVEN_NEW.replace('version = "6.6"', 'version = "7.0"');
  assert.equal(affected.moduleBazelIsMavenOnly(MODULE_BAZEL_MAVEN_OLD, updated), false);
});

test("module bazel: other extension use_repo edit is global", () => {
  const updated = MODULE_BAZEL_MAVEN_OLD.replace(
    'use_repo(other, "com_example")',
    'use_repo(other, "com_example", "com_example_other")',
  );
  assert.equal(affected.moduleBazelIsMavenOnly(MODULE_BAZEL_MAVEN_OLD, updated), false);
});

test("module bazel: string change outside maven is global", () => {
  const updated = MODULE_BAZEL_MAVEN_OLD.replace(
    'MARKER = "value (with # parens)"',
    'MARKER = "value (with # parens)!"',
  );
  assert.equal(affected.moduleBazelIsMavenOnly(MODULE_BAZEL_MAVEN_OLD, updated), false);
});

test("module bazel: comment change outside maven is scoped", () => {
  // Comments never reach the build, so a comment-only edit is Maven-safe.
  const updated = MODULE_BAZEL_MAVEN_OLD.replace(
    "# A `#` and parentheses in a string",
    "# A comment mentioning maven.install( outside the call",
  );
  assert.equal(affected.moduleBazelIsMavenOnly(MODULE_BAZEL_MAVEN_OLD, updated), true);
});

test("module bazel: missing maven proxy is global", () => {
  const updated = MODULE_BAZEL_MAVEN_NEW.replace(
    'maven = use_extension("@rules_jvm_external//:extensions.bzl", "maven")',
    "maven = object()",
  );
  assert.equal(affected.moduleBazelIsMavenOnly(MODULE_BAZEL_MAVEN_OLD, updated), false);
});

test("module bazel: unbalanced input is global", () => {
  assert.equal(
    affected.moduleBazelIsMavenOnly(`${MODULE_BAZEL_MAVEN_OLD}(\n`, MODULE_BAZEL_MAVEN_NEW),
    false,
  );
});

function routeModule(oldText, newText) {
  const config = affected.loadConfig();
  return withDeps(
    {
      gitFileText: (_root, revision) =>
        revision === "base-sha" ? oldText : newText,
    },
    () =>
      affected.route(config, ["MODULE.bazel"], {
        root: REPO,
        base: "base-sha",
        head: "head-sha",
      }),
  );
}

test("module routing: maven-only edit runs android checks and no other platform", () => {
  const config = affected.loadConfig();
  const androidIds = config.byPlatform("android").map((unit) => unit.id);
  const result = routeModule(MODULE_BAZEL_MAVEN_OLD, MODULE_BAZEL_MAVEN_NEW);
  assert.deepEqual(result.changed, [affected.MODULE_BAZEL_MAVEN]);
  assert.equal(result.routes.android.mode, "all");
  assert.deepEqual(result.routes.android.units.map((unit) => unit.id), androidIds);
  for (const platform of ["python", "js", "native"]) {
    assert.deepEqual(result.routes[platform].units, [], platform);
  }
  assert.equal(result.routes.quality.mode, "always");
  assert.equal(result.docsOnly, false);
  assert.equal(result.deployControl, false);
});

test("module routing: maven-only edit distributes the apk but deploys no service", () => {
  const config = affected.loadConfig();
  const androidIds = config.byPlatform("android").map((unit) => unit.id);
  const result = routeModule(MODULE_BAZEL_MAVEN_OLD, MODULE_BAZEL_MAVEN_NEW);
  assert.equal(result.androidDistribute, true);
  assert.deepEqual(
    [...result.deploySelected].sort(),
    [...new Set([...androidIds, "repository"])].sort(),
  );
  assert.deepEqual(affected.deployEntries(result, config), []);
});

test("module routing: bazel_dep change keeps the global invalidation", () => {
  const config = affected.loadConfig();
  const updated = MODULE_BAZEL_MAVEN_OLD.replace('version = "6.6"', 'version = "7.0"');
  const result = routeModule(MODULE_BAZEL_MAVEN_OLD, updated);
  assert.deepEqual(result.changed, ["MODULE.bazel"]);
  assert.equal(result.routes.python.mode, "all");
  const expected = config.units.filter((unit) => unit.deployImages.length).map((unit) => unit.id);
  assert.deepEqual(
    affected.deployEntries(result, config).map((entry) => entry.id),
    expected,
  );
});

test("module routing: mixed edit keeps the global invalidation", () => {
  const updated = MODULE_BAZEL_MAVEN_NEW.replace('version = "6.6"', 'version = "7.0"');
  const result = routeModule(MODULE_BAZEL_MAVEN_OLD, updated);
});

test("module routing: missing revision keeps the global invalidation", () => {
  const config = affected.loadConfig();
  const result = withDeps(
    {
      gitFileText: (_root, revision) =>
        revision === "base-sha" ? null : MODULE_BAZEL_MAVEN_NEW,
    },
    () =>
      affected.route(config, ["MODULE.bazel"], {
        root: REPO,
        base: "base-sha",
        head: "head-sha",
      }),
  );
  assert.deepEqual(result.changed, ["MODULE.bazel"]);
});

// --------------------------------------------------------------------------
// .bazelrc scoping
// --------------------------------------------------------------------------
const BAZELRC_BASE = `common --enable_bzlmod

build --announce_rc
build --color=yes
test --test_output=errors

build --define=home_version_code=1
build --define=home_version_name=0.1.0

common --disk_cache=~/.cache/bazel-disk
`;

function withCacheFlags(text) {
  return text.replace(
    "common --disk_cache=~/.cache/bazel-disk",
    "common --disk_cache=~/.cache/bazel-disk\n" +
      "common --experimental_disk_cache_gc_max_size=15G\n" +
      "common --experimental_disk_cache_gc_max_age=14d",
  );
}

test("bazelrc: cache-flag diff is scoped", () => {
  assert.equal(affected.bazelrcIsCacheOnly(BAZELRC_BASE, withCacheFlags(BAZELRC_BASE)), true);
});

test("bazelrc: comment-only diff is neutral", () => {
  const updated = BAZELRC_BASE.replace(
    "common --enable_bzlmod",
    "# Bazel modules are on by default\ncommon --enable_bzlmod",
  );
  assert.equal(affected.bazelrcIsCacheOnly(BAZELRC_BASE, updated), true);
});

test("bazelrc: a real build flag is global", () => {
  const updated = BAZELRC_BASE.replace(
    "build --define=home_version_code=1",
    "build --define=home_version_code=2",
  );
  assert.equal(affected.bazelrcIsCacheOnly(BAZELRC_BASE, updated), false);
});

test("bazelrc: adding a real build flag is global", () => {
  const updated = `${BAZELRC_BASE}build --stamp\n`;
  assert.equal(affected.bazelrcIsCacheOnly(BAZELRC_BASE, updated), false);
});

test("bazelrc: remote_cache is not in the cache whitelist", () => {
  const updated = `${BAZELRC_BASE}build --remote_cache=grpc://cache:9092\n`;
  assert.equal(affected.bazelrcIsCacheOnly(BAZELRC_BASE, updated), false);
});

test("bazelrc: an unknown flag is global", () => {
  const updated = `${BAZELRC_BASE}common --unrecognized_thing=1\n`;
  assert.equal(affected.bazelrcIsCacheOnly(BAZELRC_BASE, updated), false);
});

test("bazelrc: import lines are global", () => {
  const updated = `import %workspace%/.bazelrc.local\n${BAZELRC_BASE}`;
  assert.equal(affected.bazelrcIsCacheOnly(BAZELRC_BASE, updated), false);
  const tryImport = `try-import %workspace%/.bazelrc.local\n${BAZELRC_BASE}`;
  assert.equal(affected.bazelrcIsCacheOnly(BAZELRC_BASE, tryImport), false);
});

test("bazelrc: a line mixing cache and real flags is global when edited", () => {
  const base = `${BAZELRC_BASE}common --disk_cache=~/.cache/bazel-disk --config=strict\n`;
  const updated = base.replace("--config=strict", "--config=other");
  assert.equal(affected.bazelrcIsCacheOnly(base, updated), false);
  // Unchanged mixed lines do not block a cache-only edit elsewhere.
  assert.equal(affected.bazelrcIsCacheOnly(base, withCacheFlags(base)), true);
});

test("bazelrc routing: cache-only edit runs no platform and deploys nothing", () => {
  const config = affected.loadConfig();
  const result = withDeps(
    {
      gitFileText: (_root, revision, filePath) => {
        if (filePath !== affected.BAZELRC) {
          return null;
        }
        return revision === "base-sha" ? BAZELRC_BASE : withCacheFlags(BAZELRC_BASE);
      },
    },
    () =>
      affected.route(config, [affected.BAZELRC], {
        root: REPO,
        base: "base-sha",
        head: "head-sha",
      }),
  );
  assert.deepEqual(result.changed, [affected.BAZELRC_CACHE]);
  for (const platform of ["android", "bazel", "python", "js", "native"]) {
    assert.deepEqual(result.routes[platform].units, [], platform);
  }
  assert.deepEqual(result.routes.quality.units.map((unit) => unit.id), ["repository"]);
  assert.deepEqual(affected.deployEntries(result, config), []);
  assert.equal(result.androidDistribute, false);
});

test("bazelrc routing: a real flag edit keeps the global invalidation", () => {
  const updated = BAZELRC_BASE.replace("--define=home_version_code=1", "--define=home_version_code=2");
  const config = affected.loadConfig();
  const result = withDeps(
    {
      gitFileText: (_root, revision, filePath) => {
        if (filePath !== affected.BAZELRC) {
          return null;
        }
        return revision === "base-sha" ? BAZELRC_BASE : updated;
      },
    },
    () =>
      affected.route(config, [affected.BAZELRC], {
        root: REPO,
        base: "base-sha",
        head: "head-sha",
      }),
  );
  assert.deepEqual(result.changed, [affected.BAZELRC]);
});

test("bazelrc routing: a deleted file keeps the global invalidation", () => {
  const config = affected.loadConfig();
  const result = withDeps(
    {
      gitFileText: (_root, revision) => (revision === "base-sha" ? BAZELRC_BASE : null),
    },
    () =>
      affected.route(config, [affected.BAZELRC], {
        root: REPO,
        base: "base-sha",
        head: "head-sha",
      }),
  );
  assert.deepEqual(result.changed, [affected.BAZELRC]);
});

// --------------------------------------------------------------------------
// MODULE.bazel native scope
// --------------------------------------------------------------------------
const MODULE_BAZEL_NATIVE_OLD = `module(name = "home")

bazel_dep(name = "rules_rust", version = "0.75.0")

rust = use_extension("@rules_rust//rust:extensions.bzl", "rust")
rust.toolchain(
    edition = "2024",
    extra_target_triples = [
        "aarch64-unknown-linux-gnu",
        "armv7-unknown-linux-gnueabihf",
    ],
    versions = ["1.99.1"],
)
use_repo(rust, "rust_toolchains")
register_toolchains("@rust_toolchains//:all")

zig_sdk_repository = use_repo_rule("//platform/bazel/zig:repositories.bzl", "zig_sdk_repository")
zig_sdk_repository(name = "zig_0_14_1")
register_toolchains(
    "@zig_0_14_1//:aarch64_toolchain",
    "@zig_0_14_1//:armhf_toolchain",
)

crate = use_extension("@rules_rust//crate_universe:extensions.bzl", "crate")
crate.from_cargo(
    name = "pulsar_crates",
    manifests = ["//projects/station/pulsar/satellite:Cargo.toml"],
)
use_repo(crate, "pulsar_crates")

http_archive = use_repo_rule("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")
_SENDSPIN_BAZEL = "//projects/station/pulsar/satellite/third_party/bazel"
http_archive(
    name = "sendspin_cpp",
    build_file = _SENDSPIN_BAZEL + ":sendspin_cpp.BUILD.bazel",
    sha256 = "a",
    url = "https://example.test/sendspin.tar.gz",
)

maven = use_extension("@rules_jvm_external//:extensions.bzl", "maven")
maven.install(
    name = "maven",
    artifacts = ["junit:junit:4.13.2"],
)
use_repo(maven, "maven")
`;

const MODULE_BAZEL_NATIVE_NEW = `module(name = "home")

bazel_dep(name = "rules_rust", version = "0.75.0")

single_version_override(
    module_name = "rules_rust",
    patch_strip = 1,
    patches = ["//platform/bazel/windows:rules_rust_windows_gnu_triples.patch"],
)

rust = use_extension("@rules_rust//rust:extensions.bzl", "rust")
rust.toolchain(
    edition = "2024",
    extra_target_triples = [
        "aarch64-unknown-linux-gnu",
        "armv7-unknown-linux-gnueabihf",
        "x86_64-pc-windows-gnu",
    ],
    versions = ["1.99.1"],
)
use_repo(rust, "rust_toolchains")
register_toolchains("@rust_toolchains//:all")

zig_sdk_repository = use_repo_rule("//platform/bazel/zig:repositories.bzl", "zig_sdk_repository")
zig_sdk_repository(name = "zig_0_14_1")
register_toolchains(
    "@zig_0_14_1//:aarch64_toolchain",
    "@zig_0_14_1//:armhf_toolchain",
    "@zig_0_14_1//:windows_x86_64_toolchain",
)

crate = use_extension("@rules_rust//crate_universe:extensions.bzl", "crate")
crate.from_cargo(
    name = "pulsar_crates",
    manifests = ["//projects/station/pulsar/satellite:Cargo.toml"],
)
use_repo(crate, "pulsar_crates")
crate.from_cargo(
    name = "logon_crates",
    manifests = ["//solutions/windows-logon-provider:Cargo.toml"],
)
use_repo(crate, "logon_crates")

http_archive = use_repo_rule("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")
_SENDSPIN_BAZEL = "//projects/station/pulsar/satellite/third_party/bazel"
http_archive(
    name = "sendspin_cpp",
    build_file = _SENDSPIN_BAZEL + ":sendspin_cpp.BUILD.bazel",
    sha256 = "b",
    url = "https://example.test/sendspin.tar.gz",
)

maven = use_extension("@rules_jvm_external//:extensions.bzl", "maven")
maven.install(
    name = "maven",
    artifacts = ["junit:junit:4.13.2"],
)
use_repo(maven, "maven")
`;

function moduleScope(oldText, newText) {
  return affected.moduleBazelScope(oldText, newText);
}

test("module bazel: native toolchain edit is scoped to native", () => {
  assert.equal(moduleScope(MODULE_BAZEL_NATIVE_OLD, MODULE_BAZEL_NATIVE_NEW), "native");
});

test("module bazel: a bazel_dep edit inside a native diff is global", () => {
  const updated = MODULE_BAZEL_NATIVE_NEW.replace('version = "0.75.0"', 'version = "0.76.0"');
  assert.equal(moduleScope(MODULE_BAZEL_NATIVE_OLD, updated), null);
});

test("module bazel: a maven artifact edit is android-scoped even with native inputs", () => {
  const updated = MODULE_BAZEL_NATIVE_NEW.replace(
    'artifacts = ["junit:junit:4.13.2"]',
    'artifacts = ["junit:junit:4.13.2", "org.slf4j:slf4j-api:2.0.0"]',
  );
  assert.equal(moduleScope(MODULE_BAZEL_NATIVE_NEW, updated), "maven");
});

test("module bazel: mixed maven and native edit is the union of both", () => {
  const updated = MODULE_BAZEL_NATIVE_NEW.replace(
    'artifacts = ["junit:junit:4.13.2"]',
    'artifacts = ["junit:junit:4.13.2", "org.slf4j:slf4j-api:2.0.0"]',
  );
  assert.equal(moduleScope(MODULE_BAZEL_NATIVE_OLD, updated), "maven+native");
});

test("module bazel: changing a non-zig register_toolchains stays global", () => {
  const updated = MODULE_BAZEL_NATIVE_OLD.replace(
    'register_toolchains("@rust_toolchains//:all")',
    'register_toolchains("@rust_toolchains//:all")\nregister_toolchains("@other//:all")',
  );
  assert.equal(moduleScope(MODULE_BAZEL_NATIVE_OLD, updated), null);
});

test("module bazel: an http_archive without the station overlay is global", () => {
  const updated = `${MODULE_BAZEL_NATIVE_OLD}\nhttp_archive(\n    name = "other",\n    sha256 = "c",\n    url = "https://example.test/x.tar.gz",\n)\n`;
  assert.equal(moduleScope(MODULE_BAZEL_NATIVE_OLD, updated), null);
});

function routeModuleFiles(changed, oldText, newText) {
  const config = affected.loadConfig();
  return withDeps(
    {
      gitFileText: (_root, revision, filePath) =>
        filePath === affected.MODULE_BAZEL
          ? revision === "base-sha"
            ? oldText
            : newText
          : null,
    },
    () =>
      affected.route(config, changed, {
        root: REPO,
        base: "base-sha",
        head: "head-sha",
      }),
  );
}

test("module routing: native-only edit reaches native and every Rust Bazel unit", () => {
  const config = affected.loadConfig();
  const result = routeModuleFiles(
    [affected.MODULE_BAZEL],
    MODULE_BAZEL_NATIVE_OLD,
    MODULE_BAZEL_NATIVE_NEW,
  );
  assert.deepEqual(result.changed, [affected.MODULE_BAZEL_NATIVE]);
  for (const platform of ["android", "python", "js"]) {
    assert.deepEqual(result.routes[platform].units, [], platform);
  }
  assert.deepEqual(result.routes.native.units.map((unit) => unit.id), [
    "logon-provider",
    "pulsar",
  ]);
  assert.deepEqual(result.routes.bazel.units.map((unit) => unit.id), [
    "platform-deploy",
    "voice-proxy",
    "health-exporter",
    "scheduler",
    "wakegrant",
    "intercom",
    "currency-bot",
    "mcp-gateway",
    "ha-bridge",
    "ble-gateway",
    "proto",
    "pulsar-bazel",
  ]);
  assert.deepEqual(result.routes.quality.units.map((unit) => unit.id), ["repository"]);
  // scheduler, wakegrant, intercom, currency-bot and ha-bridge are Rust Bazel
  // deploy units now, so a Rust toolchain pin change legitimately rebuilds and
  // rolls them; the native-only units are not deployed.
  assert.deepEqual(
    affected.deployEntries(result, config).map((entry) => entry.id),
    ["scheduler", "wakegrant", "intercom", "currency-bot", "mcp-gateway", "ha-bridge"],
  );
  assert.equal(result.androidDistribute, false);
});

test("module routing: mixed maven+native edit unions android, native and bazel", () => {
  const updated = MODULE_BAZEL_NATIVE_NEW.replace(
    'artifacts = ["junit:junit:4.13.2"]',
    'artifacts = ["junit:junit:4.13.2", "org.slf4j:slf4j-api:2.0.0"]',
  );
  const result = routeModuleFiles([affected.MODULE_BAZEL], MODULE_BAZEL_NATIVE_OLD, updated);
  assert.deepEqual(result.changed, [
    affected.MODULE_BAZEL_MAVEN,
    affected.MODULE_BAZEL_NATIVE,
  ]);
  assert.equal(result.routes.android.mode, "all");
  assert.equal(result.routes.native.mode, "all");
  assert.equal(result.routes.bazel.mode, "all");
  assert.deepEqual(result.routes.python.units, []);
  assert.deepEqual(result.routes.js.units, []);
  assert.equal(result.androidDistribute, true);
});

test("module routing: a native edit with the lock follows the same scope", () => {
  const result = routeModuleFiles(
    [affected.MODULE_BAZEL, affected.MODULE_BAZEL_LOCK],
    MODULE_BAZEL_NATIVE_OLD,
    MODULE_BAZEL_NATIVE_NEW,
  );
  assert.deepEqual(result.changed, [affected.MODULE_BAZEL_NATIVE]);
});

test("module routing: the lock alone keeps the global invalidation", () => {
  const result = routeModuleFiles(
    [affected.MODULE_BAZEL_LOCK],
    MODULE_BAZEL_NATIVE_OLD,
    MODULE_BAZEL_NATIVE_NEW,
  );
  assert.deepEqual(result.changed, [affected.MODULE_BAZEL_LOCK]);
  assert.equal(result.routes.android.mode, "all");
});

// --------------------------------------------------------------------------
// platform/bazel toolchain scoping
// --------------------------------------------------------------------------
test("platform bazel routing: zig toolchain change reaches only native", () => {
  const result = routeReal(["platform/bazel/zig/toolchain.bzl"]);
  assert.deepEqual(result.changed, [affected.BAZEL_CONFIG_NATIVE]);
  for (const platform of ["android", "bazel", "python", "js"]) {
    assert.deepEqual(result.routes[platform].units, [], platform);
  }
  assert.deepEqual(result.routes.native.units.map((unit) => unit.id), [
    "logon-provider",
    "pulsar",
  ]);
  assert.equal(result.routes.quality.mode, "always");
});

test("platform bazel routing: windows patches reach only native", () => {
  const result = routeReal(["platform/bazel/windows/BUILD.bazel"]);
  assert.deepEqual(result.changed, [affected.BAZEL_CONFIG_NATIVE]);
  assert.deepEqual(result.routes.android.units, []);
  assert.deepEqual(result.routes.native.units.map((unit) => unit.id), [
    "logon-provider",
    "pulsar",
  ]);
});

test("platform bazel routing: other files under platform/bazel stay global", () => {
  const result = routeReal(["platform/bazel/README.md"]);
  assert.deepEqual(result.changed, ["platform/bazel/README.md"]);
  assert.equal(result.routes.android.mode, "all");
});

test("platform bazel routing: a native toolchain and a scheduler change select both", () => {
  const config = affected.loadConfig();
  const result = withDeps({}, () =>
    affected.route(
      config,
      ["platform/bazel/zig/repositories.bzl", "solutions/scheduler/crates/scheduler/src/api/service.rs"],
      {
        root: REPO,
        base: "b",
        graph: { affectedRules: () => new Set(["//solutions/scheduler/crates/scheduler:scheduler_lib"]) },
      },
    ),
  );
  assert.deepEqual(result.changed, [
    "platform/bazel#native",
    "solutions/scheduler/crates/scheduler/src/api/service.rs",
  ]);
  assert.deepEqual(result.routes.native.units.map((unit) => unit.id), [
    "logon-provider",
    "pulsar",
  ]);
  assert.deepEqual(result.routes.bazel.units.map((unit) => unit.id), ["scheduler"]);
  assert.deepEqual(result.routes.android.units, []);
});


test("MODULE.bazel: rules_android/NDK edits scope to android (#maven)", () => {
  const base = [
    'bazel_dep(name = "rules_android", version = "0.7.3")',
    'maven = use_extension("@rules_jvm_external//:extensions.bzl", "maven")',
    'maven.install(artifacts = ["a:b:1"])',
    'use_repo(maven, "maven")',
    'bazel_dep(name = "rules_rust", version = "0.60.0")',
    "",
  ].join("\n");
  const withNdk = base + [
    'bazel_dep(name = "rules_android_ndk", version = "0.1.5")',
    'ndk = use_extension("@rules_android_ndk//:extension.bzl", "android_ndk_repository_extension")',
    "ndk.configure(api_level = 26)",
    'use_repo(ndk, "androidndk")',
    'register_toolchains("@androidndk//:all")',
    "",
  ].join("\n");
  assert.equal(affected.moduleBazelScope(base, withNdk), "maven");
  assert.equal(
    affected.moduleBazelScope(base, base.replace('"0.7.3"', '"0.7.4"')),
    "maven",
  );
  // Non-Android edits keep the global rule.
  assert.equal(
    affected.moduleBazelScope(base, base + 'bazel_dep(name = "rules_oci", version = "2.0.0")\n'),
    null,
  );
  assert.equal(
    affected.moduleBazelScope(base, base + 'register_toolchains("@other//:all")\n'),
    null,
  );
  assert.equal(
    affected.moduleBazelScope(base, base.replace('"0.60.0"', '"0.61.0"')),
    null,
  );
});
