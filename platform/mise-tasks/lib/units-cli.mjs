#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { loadConfig } from "../../ci/affected.mjs";

export const HERE = path.dirname(fileURLToPath(import.meta.url));
export const ROOT = path.resolve(HERE, "../../..");

export function units() {
  return loadConfig().units;
}

export function isDeployable(unit) {
  return unit.deployImages.length > 0;
}

export function formatTable(list) {
  const rows = [["id", "platform", "title", "deployable"]];
  for (const unit of list) {
    rows.push([unit.id, unit.platform, unit.title, isDeployable(unit) ? "yes" : "no"]);
  }
  const widths = rows[0].map((_, column) =>
    Math.max(...rows.map((row) => row[column].length)),
  );
  return rows
    .map((row) =>
      row
        .map((cell, column) =>
          column === row.length - 1 ? cell : cell.padEnd(widths[column]),
        )
        .join("  ")
        .trimEnd(),
    )
    .join("\n");
}

function editDistance(left, right) {
  const previous = Array.from({ length: right.length + 1 }, (_, index) => index);
  for (let row = 1; row <= left.length; row += 1) {
    let diagonal = previous[0];
    previous[0] = row;
    for (let column = 1; column <= right.length; column += 1) {
      const above = previous[column];
      previous[column] = Math.min(
        previous[column] + 1,
        previous[column - 1] + 1,
        diagonal + (left[row - 1] === right[column - 1] ? 0 : 1),
      );
      diagonal = above;
    }
  }
  return previous[right.length];
}

export function suggestIds(ids, query, limit = 5) {
  const needle = query.toLowerCase();
  return ids
    .map((id) => {
      const candidate = id.toLowerCase();
      const distance = editDistance(candidate, needle);
      const contained =
        candidate.includes(needle) || (needle.length > 2 && needle.includes(candidate));
      return { id, score: contained ? Math.min(distance, 1) : distance };
    })
    .filter(({ score }) => score <= 3)
    .sort((left, right) => left.score - right.score || left.id.localeCompare(right.id))
    .slice(0, limit)
    .map(({ id }) => id);
}

export function findUnit(list, id) {
  const unit = list.find((candidate) => candidate.id === id);
  if (unit) {
    return unit;
  }
  const suggestions = suggestIds(
    list.map((candidate) => candidate.id),
    id,
  );
  const hint = suggestions.length
    ? `\nDid you mean: ${suggestions.join(", ")}?`
    : "";
  throw new Error(
    `unknown unit '${id}'.${hint}\nRun \`mise run units\` for the full list.`,
  );
}

export function unitHint(unit) {
  switch (unit.platform) {
    case "python": {
      const parts = [
        ...unit.suites.map(
          (suite) => `python3 -m unittest discover -s ${suite} -p 'test_*.py'`,
        ),
        ...unit.compileDirs.map((directory) => `python3 -m compileall -q ${directory}`),
      ];
      return parts.length
        ? `python unit: ${parts.join("; ")}`
        : "python unit: no suites or compile directories declared";
    }
    case "js":
      return `js unit: node --test ${unit.jsTests.map((d) => `${d}/*.test.js`).join(" ")}`;
    case "native": {
      const command = unit.nativeTest || unit.nativeBuild || "(no command declared)";
      return `native unit: cd ${unit.nativeWorkdir} && ${command}`;
    }
    case "quality":
      return "quality unit: mise run check";
    default:
      return `platform '${unit.platform}' has no Bazel build targets`;
  }
}

function bazelTargetOverride() {
  const raw = process.env.BAZEL_TARGETS;
  return raw ? raw.trim().split(/\s+/).filter(Boolean) : [];
}

function bazelExtraArgs() {
  const raw = process.env.BAZEL_EXTRA_ARGS;
  return raw ? raw.trim().split(/\s+/).filter(Boolean) : [];
}

export const ANDROID_CONFIG = "--config=android";

export function needsAndroidConfig(unit) {
  return unit === null || unit.platform === "android";
}

export function bazelTargets(unit, kind) {
  const override = bazelTargetOverride();
  if (override.length) {
    return override;
  }
  if (kind === "test") {
    return [...unit.testTargets];
  }
  return unit.buildTargets.length
    ? [...unit.buildTargets]
    : unit.bazelRoots.map((bazelRoot) => `${bazelRoot}/...`);
}

export function buildCommands(unit, root = ROOT) {
  if (unit === null) {
    return [{
      command: "bazel",
      args: ["build", ANDROID_CONFIG, ...bazelExtraArgs(), "//..."],
      cwd: root,
    }];
  }
  if (!["bazel", "android"].includes(unit.platform)) {
    throw new Error(
      `unit '${unit.id}' is platform '${unit.platform}' and has no Bazel build targets.\n` +
        unitHint(unit),
    );
  }
  const targets = bazelTargets(unit, "build");
  if (!targets.length) {
    throw new Error(
      `unit '${unit.id}' declares no build targets.\n${unitHint(unit)}`,
    );
  }
  const config = needsAndroidConfig(unit) ? [ANDROID_CONFIG] : [];
  return [{
    command: "bazel",
    args: ["build", ...config, ...bazelExtraArgs(), ...targets],
    cwd: root,
  }];
}

function testFiles(root, directories, suffix) {
  const files = [];
  for (const directory of directories) {
    const absolute = path.resolve(root, directory);
    for (const name of fs.readdirSync(absolute).filter((entry) => entry.endsWith(suffix)).sort()) {
      files.push(path.relative(root, path.join(absolute, name)));
    }
  }
  return files;
}

export function testCommands(unit, root = ROOT) {
  if (unit === null) {
    return [
      {
        command: path.join(root, "platform/quality/run-tests.sh"),
        args: [],
        cwd: root,
      },
    ];
  }
  switch (unit.platform) {
    case "bazel":
    case "android": {
      const targets = bazelTargets(unit, "test");
      if (!targets.length) {
        throw new Error(
          `unit '${unit.id}' declares no Bazel test targets.\n${unitHint(unit)}`,
        );
      }
      const config = needsAndroidConfig(unit) ? [ANDROID_CONFIG] : [];
      return [
        {
          command: "bazel",
          args: [
            "test",
            ...config,
            ...bazelExtraArgs(),
            ...targets,
            "--test_output=errors",
          ],
          cwd: root,
        },
      ];
    }
    case "python": {
      const commands = [];
      let env;
      if (unit.pythonDeployTool) {
        const binary = path.join(root, "bazel-bin/platform/deploy-rs/home-deploy");
        commands.push({
          command: "bazel",
          args: ["build", "//platform/deploy-rs:home-deploy"],
          cwd: root,
          prerequisite: true,
        });
        env = { HOME_DEPLOY: binary };
      }
      for (const directory of unit.compileDirs) {
        commands.push({
          command: "python3",
          args: ["-m", "compileall", "-q", directory],
          cwd: root,
          env,
        });
      }
      for (const suite of unit.suites) {
        commands.push({
          command: "python3",
          args: ["-m", "compileall", "-q", suite],
          cwd: root,
          env,
        });
        commands.push({
          command: "python3",
          args: ["-m", "unittest", "discover", "-s", suite, "-p", "test_*.py", "-v"],
          cwd: root,
          env,
        });
      }
      if (!commands.length) {
        throw new Error(
          `unit '${unit.id}' declares no Python suites or compile directories.\n` +
            unitHint(unit),
        );
      }
      return commands;
    }
    case "js": {
      const files = testFiles(root, unit.jsTests, ".test.js");
      if (!files.length) {
        throw new Error(
          `unit '${unit.id}' has no *.test.js files.\n${unitHint(unit)}`,
        );
      }
      return [{ command: "node", args: ["--test", ...files], cwd: root }];
    }
    case "native": {
      if (!unit.nativeTest) {
        throw new Error(
          `unit '${unit.id}' declares no native test command.\n${unitHint(unit)}`,
        );
      }
      return [
        {
          command: unit.nativeTest,
          shell: true,
          cwd: path.resolve(root, unit.nativeWorkdir),
        },
      ];
    }
    case "quality": {
      const nodeTests = [
        ...testFiles(root, ["platform/ci"], ".test.mjs"),
        ...testFiles(root, ["platform/quality"], ".test.mjs"),
        ...testFiles(root, ["platform/mise-tasks/lib"], ".test.mjs"),
      ];
      return [
        { command: "node", args: ["platform/quality/audit_repository.mjs"], cwd: root },
        {
          command: "python3",
          args: ["-m", "unittest", "discover", "-s", "platform/quality", "-p", "test_static.py", "-v"],
          cwd: root,
        },
        { command: "node", args: ["platform/ci/check_units.mjs"], cwd: root },
        { command: "node", args: ["--test", ...nodeTests], cwd: root },
        {
          command: "shellcheck",
          args: [
            "-S",
            "error",
            ...[
              "platform/mise-tasks/*.sh",
              "platform/mise-tasks/*/*.sh",
              "platform/quality/*.sh",
              "platform/ci/*.sh",
              "platform/deploy/*.sh",
              "projects/home/kubernetes/scheduler/*.sh",
              "projects/home/kubernetes/scheduler/nft/*.sh",
              "projects/home/kubernetes/currency-bot/nft/*.sh",
              "projects/station/scripts/*.sh",
              "solutions/windows-logon-provider/*.sh",
              "platform/workstation/env.sh",
              "platform/workstation/run.sh",
              "platform/workstation/bin/bazel",
            ],
          ],
          shell: true,
          cwd: root,
        },
        {
          command: "actionlint",
          args: ["-color", ".github/workflows/*.yml"],
          shell: true,
          cwd: root,
        },
        {
          command: "gitleaks",
          args: ["dir", ".", "--redact=100", "--no-banner", "--config=.gitleaks.toml"],
          cwd: root,
        },
      ];
    }
    default:
      throw new Error(`unit '${unit.id}' has no local test command (platform '${unit.platform}')`);
  }
}

export const TARGET_PATTERN = /(\.\.\.|:all(-targets)?|:\*)$/;

function listFromEnv(name) {
  const raw = process.env[name];
  return raw === undefined ? null : raw.trim().split(/\s+/).filter(Boolean);
}

export function isTargetPattern(label) {
  return TARGET_PATTERN.test(label);
}

export function foreignTestsQuery(patterns, tests) {
  const under = `tests(set(${patterns.join(" ")}))`;
  return tests.length ? `${under} except set(${tests.join(" ")})` : under;
}

export function ciPlan(unit) {
  if (!["bazel", "android"].includes(unit.platform)) {
    throw new Error(
      `unit '${unit.id}' is platform '${unit.platform}'; ci runs Bazel units only.\n` +
        unitHint(unit),
    );
  }
  const tests = listFromEnv("BAZEL_TEST_TARGETS") ?? [...unit.testTargets];
  const builds = listFromEnv("BAZEL_BUILD_TARGETS") ?? [...unit.buildTargets];
  if (!tests.length && !builds.length) {
    throw new Error(`unit '${unit.id}' declares no Bazel targets.\n${unitHint(unit)}`);
  }
  return {
    command: tests.length ? "test" : "build",
    config: needsAndroidConfig(unit) ? [ANDROID_CONFIG] : [],
    tests,
    explicitBuilds: builds.filter((label) => !isTargetPattern(label)),
    patternBuilds: builds.filter(isTargetPattern),
  };
}

export function ciCommand(plan, foreignTests, patternFile) {
  const args = [plan.command, ...plan.config, ...bazelExtraArgs(), "--keep_going"];
  if (plan.command === "test") {
    args.push("--test_output=errors");
  }
  const patterns = [
    ...new Set([...plan.tests, ...plan.explicitBuilds, ...plan.patternBuilds]),
    ...[...new Set(foreignTests)].map((label) => `-${label}`),
  ];
  fs.writeFileSync(patternFile, `${patterns.join("\n")}\n`, "utf8");
  args.push(`--target_pattern_file=${patternFile}`);
  return { command: "bazel", args, cwd: ROOT, patterns };
}

export function runCi(unit, { spawn = spawnSync, log = process.stderr } = {}) {
  const plan = ciPlan(unit);
  let foreignTests = [];
  if (plan.patternBuilds.length) {
    const query = spawn(
      "bazel",
      ["query", "--output=label", foreignTestsQuery(plan.patternBuilds, plan.tests)],
      {
        cwd: ROOT,
        encoding: "utf8",
        maxBuffer: 64 * 1024 * 1024,
        stdio: ["ignore", "pipe", "inherit"],
      },
    );
    if (query.error || query.status !== 0) {
      log.write(`bazel query for the tests under the build targets of '${unit.id}' failed\n`);
      return query.status || 1;
    }
    foreignTests = String(query.stdout).split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  }
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "units-ci-"));
  try {
    const command = ciCommand(plan, foreignTests, path.join(directory, "targets.txt"));
    log.write(
      `bazel ${plan.command} ${unit.id}: tests ${plan.tests.join(" ") || "(none)"}; ` +
        `build ${[...plan.explicitBuilds, ...plan.patternBuilds].join(" ") || "(none)"}; ` +
        `${foreignTests.length} test(s) under the build patterns are built, not run\n`,
    );
    return runCommands([command], { spawn, log });
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

export function describeCommand({ command, args = [] }) {
  return [command, ...args].join(" ");
}

export function runCommands(commands, { keepGoing = false, spawn = spawnSync, log = process.stderr } = {}) {
  const failed = [];
  for (const entry of commands) {
    const { command, args, cwd, shell = false, env, prerequisite = false } = entry;
    const result = spawn(command, args, {
      cwd,
      shell,
      stdio: "inherit",
      env: env ? { ...process.env, ...env } : process.env,
    });
    if (result.error) {
      log.write(`${result.error.message}\n`);
    }
    const status = result.error ? 1 : result.status ?? 1;
    if (status === 0) {
      continue;
    }
    failed.push({ entry, status });
    if (!keepGoing || prerequisite) {
      break;
    }
  }
  if (!failed.length) {
    return 0;
  }
  if (keepGoing && commands.length > 1) {
    log.write(`${failed.length} of ${commands.length} commands failed:\n`);
    for (const { entry, status } of failed) {
      log.write(`  exit ${status}: ${describeCommand(entry)}\n`);
    }
  }
  return failed[0].status;
}

const USAGE = `Usage: units-cli.mjs <command>

  units          list units: id, platform, title, deployable
  ids            print unit ids, one per line (completion)
  build [unit]   bazel build //..., or one unit's Bazel targets
  test [unit]    hermetic suite or one unit's CI test commands
  ci <unit>      one keep-going Bazel call: the unit's tests plus its build
                 targets (tests under build patterns are built, not run)

Android units (and the whole-workspace build) pass --config=android, which
activates platform/android/android.bazelrc.
`;

export function main(argv = process.argv.slice(2)) {
  const [command, ...rest] = argv;
  try {
    switch (command) {
      case "units":
        process.stdout.write(`${formatTable(units())}\n`);
        return 0;
      case "ids":
        process.stdout.write(
          `${units()
            .map((unit) => unit.id)
            .sort()
            .join("\n")}\n`,
        );
        return 0;
      case "build": {
        const unit = rest.length ? findUnit(units(), rest[0]) : null;
        return runCommands(buildCommands(unit));
      }
      case "test": {
        const unit = rest.length ? findUnit(units(), rest[0]) : null;
        return runCommands(testCommands(unit), { keepGoing: true });
      }
      case "ci": {
        if (!rest.length) {
          throw new Error("ci needs a unit id");
        }
        return runCi(findUnit(units(), rest[0]));
      }
      default:
        process.stderr.write(USAGE);
        return command === undefined ? 0 : 2;
    }
  } catch (error) {
    process.stderr.write(`${error?.message || String(error)}\n`);
    return 1;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = main();
}
