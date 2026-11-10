#!/usr/bin/env node
/**
 * Unit dispatcher behind the mise tasks (`build`, `test`, `units`).
 *
 * The CI unit map (`platform/ci/units.mjs`) is the single source of truth for
 * what a unit runs; this script only turns that map into local commands, so
 * `mise run build <unit>` / `mise run test <unit>` reproduce the matching CI
 * job instead of guessing from a directory name.
 *
 * It has no npm dependencies: Node built-ins plus the unit map only.
 *
 * Usage:
 *
 *     node platform/mise-tasks/lib/units-cli.mjs units
 *     node platform/mise-tasks/lib/units-cli.mjs ids
 *     node platform/mise-tasks/lib/units-cli.mjs build [unit]
 *     node platform/mise-tasks/lib/units-cli.mjs test [unit]
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { loadConfig } from "../../ci/affected.mjs";

export const HERE = path.dirname(fileURLToPath(import.meta.url));
export const ROOT = path.resolve(HERE, "../../..");

/** All units from the CI map, in declaration order. */
export function units() {
  return loadConfig().units;
}

/** A unit is deployable when deploy-home owns images and workloads for it. */
export function isDeployable(unit) {
  return unit.deployImages.length > 0;
}

/** Aligned table: id, platform, title, deployable. */
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

/**
 * Nearest unit ids for a typo: distance plus substring containment, so both
 * `mise run test sched` and `mise run test schedular` are useful.
 */
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

/** Resolve a unit id, or throw with the closest ids and the list command. */
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

/** How to exercise a unit that has no Bazel build targets. */
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

/**
 * Optional target override for Bazel units (`_android.yml` narrows the list
 * before calling the task). Space separated, like the
 * workflow inputs.
 */
function bazelTargetOverride() {
  const raw = process.env.BAZEL_TARGETS;
  return raw ? raw.trim().split(/\s+/).filter(Boolean) : [];
}

/**
 * Extra Bazel options appended to build and test commands (`BAZEL_EXTRA_ARGS`,
 * space separated). `_android.yml` uses it to build and test the distributable
 * APK once with the release `home_version_code`/`home_version_name` defines, so
 * the APK the tests exercise is exactly the APK that gets signed. Local tasks
 * leave it unset and keep the defaults from `platform/android/android.bazelrc`.
 */
function bazelExtraArgs() {
  const raw = process.env.BAZEL_EXTRA_ARGS;
  return raw ? raw.trim().split(/\s+/).filter(Boolean) : [];
}

/**
 * Android-only flags live in `platform/android/android.bazelrc` and are selected
 * with this config, so a change there routes to the Android units only instead
 * of invalidating every unit through the root `.bazelrc`. Passed first, so a
 * caller's own `--define` (distribute.sh version codes) still wins.
 */
export const ANDROID_CONFIG = "--config=android";

/** Whether a command builds Android targets and needs `--config=android`. */
export function needsAndroidConfig(unit) {
  // `null` means the whole workspace (`bazel build //...`), which includes the
  // Android targets and must behave exactly like a plain `bazel build` did
  // while these flags still lived in the root `.bazelrc`.
  return unit === null || unit.platform === "android";
}

/** Build/test targets of a Bazel unit, honouring BAZEL_TARGETS when set. */
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

/**
 * Commands that build a unit: its Bazel targets.
 */
export function buildCommands(unit, root = ROOT) {
  if (unit === null) {
    // `//...` includes the Android targets, so keep the Android config active.
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

/**
 * Commands that test a unit, mirroring the reusable platform workflow:
 * `bazel test`, Python compile + unittest discovery (plus the Rust deploy
 * tool for deploy_tool
 * units), `node --test`, or the native repository script.
 */
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
        // The suites drive a deploy script that execs the Rust deploy CLI
        // through HOME_DEPLOY; same Bazel target as the home-deploy image.
        const binary = path.join(root, "bazel-bin/platform/deploy-rs/home-deploy");
        commands.push({
          command: "bazel",
          args: ["build", "//platform/deploy-rs:home-deploy"],
          cwd: root,
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
      // Same order as `.github/workflows/_quality.yml`.
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

/** Run commands in order; stop at the first failure and return its status. */
export function runCommands(commands) {
  for (const { command, args, cwd, shell = false, env } of commands) {
    const result = spawnSync(command, args, {
      cwd,
      shell,
      stdio: "inherit",
      env: env ? { ...process.env, ...env } : process.env,
    });
    if (result.error) {
      process.stderr.write(`${result.error.message}\n`);
      return 1;
    }
    if (result.status !== 0) {
      return result.status ?? 1;
    }
  }
  return 0;
}

const USAGE = `Usage: units-cli.mjs <command>

  units          list units: id, platform, title, deployable
  ids            print unit ids, one per line (completion)
  build [unit]   bazel build //..., or one unit's Bazel targets
  test [unit]    hermetic suite or one unit's CI test commands

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
        return runCommands(testCommands(unit));
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
