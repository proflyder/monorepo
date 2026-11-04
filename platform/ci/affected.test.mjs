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
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
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
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
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
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
    affected.route(config, ["MODULE.bazel"], {
      root: REPO,
      base: "base-sha",
      graph,
    }),
  );
  const ids = result.routes.android.units.map((unit) => unit.id);
  assert.deepEqual(ids, ["app-one", "app-two", "shared-kotlin"]);
  assert.equal(result.routes.android.mode, "all");
  assert.equal(result.routes.go.mode, "all");
});

test("android fixture: maven virtual path only reaches android", () => {
  const config = affected.loadConfig(fixtureConfig);
  const graph = loadFixtureGraph();
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
    affected.route(config, [affected.MODULE_BAZEL_MAVEN], {
      root: REPO,
      base: "base-sha",
      graph,
    }),
  );
  const ids = result.routes.android.units.map((unit) => unit.id);
  assert.deepEqual(ids, ["app-one", "app-two", "shared-kotlin"]);
  assert.equal(result.routes.android.mode, "all");
  assert.deepEqual(result.routes.go.units, []);
});

test("android fixture: docs-only change runs nothing but quality", () => {
  const config = affected.loadConfig(fixtureConfig);
  const graph = loadFixtureGraph();
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
    affected.route(config, ["docs/ARCHITECTURE.md"], {
      root: REPO,
      base: "base-sha",
      graph,
    }),
  );
  assert.equal(result.docsOnly, true);
  for (const platform of ["go", "android", "bazel", "python", "js", "native"]) {
    assert.deepEqual(result.routes[platform].units, [], platform);
  }
  assert.deepEqual(result.routes.quality.units.map((unit) => unit.id), ["repository"]);
});

test("android fixture: graph failure degrades to all android units", () => {
  const config = affected.loadConfig(fixtureConfig);
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
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
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
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

function fakeDeps(_root, unit) {
  // Directory of a real Go package, as `go list -deps` reports it.
  const shared = new Set([path.join(REPO, "packages", "proto", "ricardo", "v1")]);
  const own = new Set([path.join(REPO, unit.id)]);
  // `sched` imports the shared package, nothing else does.
  if (unit.id === "sched") {
    return new Set([...own, ...shared]);
  }
  return own;
}

test("go: only importing unit is selected", () => {
  const config = affected.loadConfig(fixtureConfig);
  const result = withDeps({ goPackageDirs: fakeDeps }, () =>
    affected.route(config, ["packages/proto/ricardo/v1/scheduler.pb.go"], {
      root: REPO,
      base: "b",
    }),
  );
  assert.deepEqual(result.routes.go.units.map((unit) => unit.id), ["sched"]);
});

test("go: unit path selects its own unit", () => {
  const config = affected.loadConfig(fixtureConfig);
  const result = withDeps({ goPackageDirs: fakeDeps }, () =>
    affected.route(config, ["go/sched/cmd/sched/main.go"], { root: REPO, base: "b" }),
  );
  assert.deepEqual(result.routes.go.units.map((unit) => unit.id), ["sched"]);
});

test("go: go list failure degrades to all go units", () => {
  const config = affected.loadConfig(fixtureConfig);
  const result = withDeps(
    {
      goPackageDirs: () => {
        throw new Error("go list failed");
      },
    },
    () => affected.route(config, ["go/sched/x.go"], { root: REPO, base: "b" }),
  );
  assert.equal(result.routes.go.mode, "all");
  assert.deepEqual(result.routes.go.units.map((unit) => unit.id), ["sched"]);
});

test("go: deploy control flag follows affected units", () => {
  const config = affected.loadConfig(fixtureConfig);
  const outOfScope = withDeps({ goPackageDirs: fakeDeps }, () =>
    affected.route(config, ["apps/two/src/Two.kt"], { root: REPO, base: "b" }),
  );
  const inScope = withDeps({ goPackageDirs: fakeDeps }, () =>
    affected.route(config, ["go/sched/x.go"], { root: REPO, base: "b" }),
  );
  assert.equal(outOfScope.deployControl, false);
  assert.equal(inScope.deployControl, true);
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
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
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
  for (const platform of ["go", "android", "python", "js", "native", "quality"]) {
    assert.ok(config.byPlatform(platform).length, platform);
  }
});

test("real repo: native provider change routes to its unit", () => {
  const config = affected.loadConfig();
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
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
      publish: true,
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
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
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
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
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

test("real repo: pulsar publishes only when VERSION changed", () => {
  const config = affected.loadConfig();
  const entryFor = (changed) => {
    const result = withDeps({ goPackageDirs: () => new Set() }, () =>
      affected.route(config, changed, { root: REPO, base: "b" }),
    );
    return affected.matrixEntries(result, config).native.find((item) => item.id === "pulsar");
  };
  // A source change is PR CI only: the unit is built, but not published.
  assert.equal(entryFor(["projects/station/pulsar/satellite/crates/pulsar/src/main.rs"]).publish, false);
  assert.equal(entryFor(["projects/station/pulsar/VERSION"]).publish, true);
  // A global invalidation does not turn an unchanged VERSION into a release.
  assert.equal(entryFor(["MODULE.bazel"]).publish, false);
  // The logon provider has no publish_when: any affected change publishes it.
  const logon = affected.loadConfig();
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
    affected.route(logon, ["solutions/windows-logon-provider/crates/provider/src/worker.rs"], {
      root: REPO,
      base: "b",
    }),
  );
  assert.equal(
    affected.matrixEntries(result, logon).native.find((item) => item.id === "logon-provider")
      .publish,
    true,
  );
});

test("real repo: js worker change routes to its unit", () => {
  const config = affected.loadConfig();
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
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
  assert.deepEqual(lint.bazelSdkPackages, ["cmdline-tools;23.0"]);
});

test("real repo: android module change selects the app and the lint unit", () => {
  const config = affected.loadConfig();
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
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
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
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
    sdk_packages: "cmdline-tools;23.0",
    universe: "//packages/kotlin/... + //projects/mobile/android/...",
  });
});

test("real repo: github format is a valid JSON payload", async () => {
  const config = affected.loadConfig();
  const result = withDeps({ goPackageDirs: () => new Set() }, () =>
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
    assert.deepEqual(JSON.parse(values.go_units), []);
    assert.equal(values.docs_only, "true");
    assert.equal(values.android_distribute, "false");
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

function routeReal(changed, deps) {
  const config = affected.loadConfig();
  return withDeps(
    { goPackageDirs: deps ?? (() => new Set()) },
    () => affected.route(config, changed, { root: REPO, base: "b" }),
  );
}

test("deploy: docs-only change deploys nothing", () => {
  const config = affected.loadConfig();
  const result = routeReal(["docs/STATE.md"]);
  assert.deepEqual(affected.deployEntries(result, config), []);
});

test("deploy: scheduler change deploys only scheduler", () => {
  const config = affected.loadConfig();
  const result = routeReal(["solutions/scheduler/app/run.go"]);
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["scheduler"]);
  assert.equal(entries[0].namespace, "home-assistant");
  assert.equal(entries[0].target_root, "//projects/home/kubernetes/scheduler");
});

test("deploy: currency-bot change deploys its namespace", () => {
  const config = affected.loadConfig();
  const result = routeReal(["solutions/currency-bot/internal/server/webhook.go"]);
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["currency-bot"]);
  assert.equal(entries[0].namespace, "currency-bot");
  assert.equal(entries[0].target_root, "//solutions/currency-bot");
  assert.equal(entries[0].script, "projects/home/kubernetes/currency-bot/deploy.sh");
  assert.equal(entries[0].externalsecrets, "currency-bot cloudflared-tunnel");
});

test("deploy: currency-bot manifest change deploys it", () => {
  const config = affected.loadConfig();
  const result = routeReal([
    "projects/home/kubernetes/currency-bot/manifests/currency-bot.yaml",
  ]);
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["currency-bot"]);
});

test("deploy: ha-bridge change deploys bridge and control", () => {
  const config = affected.loadConfig();
  const result = routeReal(["solutions/ha-bridge/internal/bridge/bridge.go"]);
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["ha-bridge"]);
  assert.equal(entries[0].images, "ha-bridge ha-control");
  assert.equal(entries[0].workloads, "ha-bridge ha-control");
});

test("deploy: shared go package deploys every importer", () => {
  const config = affected.loadConfig();
  const shared = new Set([path.join(REPO, "packages", "proto", "ricardo", "v1")]);
  const deps = (_root, unit) => {
    const own = new Set([path.join(REPO, unit.goModule, unit.goPackages[0])]);
    if (unit.id === "scheduler" || unit.id === "ha-bridge") {
      return new Set([...own, ...shared]);
    }
    return own;
  };
  const result = routeReal(["packages/proto/ricardo/v1/scheduler.pb.go"], deps);
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

test("deploy: voice-proxy solution change runs its go unit and deploys hermes", () => {
  const config = affected.loadConfig();
  const result = routeReal(["solutions/voice-proxy/app/run.go"]);
  assert.deepEqual(result.routes.go.units.map((unit) => unit.id), ["voice-proxy"]);
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["hermes"]);
});

test("deploy: shared go package change reaches the hermes deploy unit", () => {
  const config = affected.loadConfig();
  const result = routeReal(["packages/go/ha/ha.go"]);
  const entries = affected.deployEntries(result, config);
  assert.deepEqual(entries.map((entry) => entry.id), ["hermes"]);
});

test("deploy: ci workflow change runs checks but deploys nothing", () => {
  const config = affected.loadConfig();
  const result = routeReal([".github/workflows/ci.yml"]);
  for (const platform of ["go", "android", "python", "js"]) {
    assert.ok(result.routes[platform].units.length, platform);
  }
  assert.deepEqual(affected.deployEntries(result, config), []);
});

test("deploy: workflow and service change deploys only the service", () => {
  const config = affected.loadConfig();
  const result = routeReal([".github/workflows/ci.yml", "solutions/currency-bot/x.go"]);
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

test("workflow invalidation: go workflow runs every go unit", () => {
  const result = routeReal([".github/workflows/_go.yml"]);
  assert.equal(result.routes.go.mode, "all");
  assert.ok(result.routes.go.units.length);
  assert.deepEqual(result.routes.android.units, []);
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
  for (const platform of ["go", "android", "bazel", "python", "js", "native"]) {
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

go_deps = use_extension("@gazelle//:extensions.bzl", "go_deps")
go_deps.from_file(go_mod = "//:go.mod")
use_repo(go_deps, "com_github_example")

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

go_deps = use_extension("@gazelle//:extensions.bzl", "go_deps")
go_deps.from_file(go_mod = "//:go.mod")
use_repo(go_deps, "com_github_example")

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
    'use_repo(go_deps, "com_github_example")',
    'use_repo(go_deps, "com_github_example", "com_github_other")',
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
      goPackageDirs: () => new Set(),
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
  for (const platform of ["go", "python", "js", "native"]) {
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
  assert.equal(result.routes.go.mode, "all");
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
  assert.equal(result.routes.go.mode, "all");
});

test("module routing: missing revision keeps the global invalidation", () => {
  const config = affected.loadConfig();
  const result = withDeps(
    {
      goPackageDirs: () => new Set(),
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
  assert.equal(result.routes.go.mode, "all");
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
      goPackageDirs: () => new Set(),
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
  for (const platform of ["go", "android", "bazel", "python", "js", "native"]) {
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
      goPackageDirs: () => new Set(),
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
  assert.equal(result.routes.go.mode, "all");
});

test("bazelrc routing: a deleted file keeps the global invalidation", () => {
  const config = affected.loadConfig();
  const result = withDeps(
    {
      goPackageDirs: () => new Set(),
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
  assert.equal(result.routes.go.mode, "all");
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
      goPackageDirs: () => new Set(),
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

test("module routing: native-only edit reaches native and pulsar-bazel only", () => {
  const config = affected.loadConfig();
  const result = routeModuleFiles(
    [affected.MODULE_BAZEL],
    MODULE_BAZEL_NATIVE_OLD,
    MODULE_BAZEL_NATIVE_NEW,
  );
  assert.deepEqual(result.changed, [affected.MODULE_BAZEL_NATIVE]);
  for (const platform of ["go", "android", "python", "js"]) {
    assert.deepEqual(result.routes[platform].units, [], platform);
  }
  assert.deepEqual(result.routes.native.units.map((unit) => unit.id), [
    "logon-provider",
    "pulsar",
  ]);
  assert.deepEqual(result.routes.bazel.units.map((unit) => unit.id), ["pulsar-bazel"]);
  assert.deepEqual(result.routes.quality.units.map((unit) => unit.id), ["repository"]);
  assert.deepEqual(affected.deployEntries(result, config), []);
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
  assert.deepEqual(result.routes.go.units, []);
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
  assert.equal(result.routes.go.mode, "paths");
  assert.deepEqual(result.routes.go.units, []);
});

test("module routing: the lock alone keeps the global invalidation", () => {
  const result = routeModuleFiles(
    [affected.MODULE_BAZEL_LOCK],
    MODULE_BAZEL_NATIVE_OLD,
    MODULE_BAZEL_NATIVE_NEW,
  );
  assert.deepEqual(result.changed, [affected.MODULE_BAZEL_LOCK]);
  assert.equal(result.routes.go.mode, "all");
  assert.equal(result.routes.android.mode, "all");
});

// --------------------------------------------------------------------------
// platform/bazel toolchain scoping
// --------------------------------------------------------------------------
test("platform bazel routing: zig toolchain change reaches only native", () => {
  const result = routeReal(["platform/bazel/zig/toolchain.bzl"]);
  assert.deepEqual(result.changed, [affected.BAZEL_CONFIG_NATIVE]);
  for (const platform of ["go", "android", "bazel", "python", "js"]) {
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
  assert.deepEqual(result.routes.go.units, []);
  assert.deepEqual(result.routes.android.units, []);
  assert.deepEqual(result.routes.native.units.map((unit) => unit.id), [
    "logon-provider",
    "pulsar",
  ]);
});

test("platform bazel routing: other files under platform/bazel stay global", () => {
  const result = routeReal(["platform/bazel/README.md"]);
  assert.deepEqual(result.changed, ["platform/bazel/README.md"]);
  assert.equal(result.routes.go.mode, "all");
  assert.equal(result.routes.android.mode, "all");
});

test("platform bazel routing: a native toolchain and a go change select both", () => {
  const result = routeReal(["platform/bazel/zig/repositories.bzl", "solutions/scheduler/app/run.go"]);
  assert.deepEqual(result.changed, [
    "platform/bazel#native",
    "solutions/scheduler/app/run.go",
  ]);
  assert.deepEqual(result.routes.native.units.map((unit) => unit.id), [
    "logon-provider",
    "pulsar",
  ]);
  assert.deepEqual(result.routes.go.units.map((unit) => unit.id), ["scheduler"]);
  assert.deepEqual(result.routes.android.units, []);
});

