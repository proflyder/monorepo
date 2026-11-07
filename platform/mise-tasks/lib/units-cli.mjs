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
    case "go": {
      const packages = unit.goPackages.length ? unit.goPackages.join(" ") : "./...";
      return `go unit (${unit.goModule}): cd ${unit.goModule} && go test ${packages}`;
    }
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
 * Commands that build a unit. Only Bazel platforms have a build entry point
 * here; other platforms answer with their own command instead.
 */
export function buildCommands(unit, root = ROOT) {
  if (unit === null) {
    return [{ command: "bazel", args: ["build", "//..."], cwd: root }];
  }
  if (!["bazel", "android"].includes(unit.platform)) {
    throw new Error(
      `unit '${unit.id}' is platform '${unit.platform}' and has no Bazel build targets.\n` +
        unitHint(unit),
    );
  }
  const targets = unit.buildTargets.length
    ? unit.buildTargets
    : unit.bazelRoots.map((bazelRoot) => `${bazelRoot}/...`);
  if (!targets.length) {
    throw new Error(
      `unit '${unit.id}' declares no build targets.\n${unitHint(unit)}`,
    );
  }
  return [{ command: "bazel", args: ["build", ...targets], cwd: root }];
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
 * `bazel test`, `go test [-race]`, Python compile + unittest discovery,
 * `node --test`, or the native repository script.
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
      if (!unit.testTargets.length) {
        throw new Error(
          `unit '${unit.id}' declares no Bazel test targets.\n${unitHint(unit)}`,
        );
      }
      return [{ command: "bazel", args: ["test", ...unit.testTargets], cwd: root }];
    }
    case "go": {
      const args = ["test"];
      if (unit.goRace) {
        args.push("-race");
      }
      args.push(...(unit.goPackages.length ? unit.goPackages : ["./..."]));
      return [{ command: "go", args, cwd: path.resolve(root, unit.goModule) }];
    }
    case "python": {
      const commands = [];
      for (const suite of unit.suites) {
        commands.push({
          command: "python3",
          args: ["-m", "compileall", "-q", suite],
          cwd: root,
        });
        commands.push({
          command: "python3",
          args: ["-m", "unittest", "discover", "-s", suite, "-p", "test_*.py", "-v"],
          cwd: root,
        });
      }
      for (const directory of unit.compileDirs) {
        commands.push({
          command: "python3",
          args: ["-m", "compileall", "-q", directory],
          cwd: root,
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
      ];
      return [
        { command: "node", args: ["platform/quality/audit_repository.mjs"], cwd: root },
        {
          command: "python3",
          args: ["-m", "unittest", "discover", "-s", "platform/quality", "-p", "test_static.py", "-v"],
          cwd: root,
        },
        { command: "node", args: ["--test", ...nodeTests], cwd: root },
        { command: "actionlint", args: [".github/workflows/*.yml"], shell: true, cwd: root },
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
  for (const { command, args, cwd, shell = false } of commands) {
    const result = spawnSync(command, args, { cwd, shell, stdio: "inherit" });
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
  build [unit]   bazel build //... or one unit's build targets
  test [unit]    hermetic suite or one unit's CI test commands
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
