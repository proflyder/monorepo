#!/usr/bin/env node
/**
 * Android target plan shared by `ci.yml`'s `plan` job and the matrix entries.
 *
 * target-determinator needs the analysed loading graph of the base revision,
 * which costs ~2 minutes plus a JDK and the Android SDK. Running it once per
 * matrix entry paid that price three times for the same answer; `plan` now runs
 * it once for the whole Android universe and every entry only intersects its
 * own declared targets with the result.
 *
 * Usage:
 *
 *     node platform/ci/plan-targets.mjs metadata >> "$GITHUB_OUTPUT"
 *     node platform/ci/plan-targets.mjs narrow \
 *       --affected "$AFFECTED_TARGETS" \
 *       --tests "$TEST_CANDIDATES" \
 *       --builds "$BUILD_CANDIDATES" >> "$GITHUB_OUTPUT"
 *
 * `metadata` prints `candidates=` (every Android test and build target of the
 * unit map, space separated) and `universe=` (the Bazel query expression that
 * covers them).
 *
 * `narrow` prints `test_targets=` and `build_targets=`. An empty affected set —
 * `plan` did not run, or target-determinator failed and degraded to "every
 * candidate" — keeps every declared target. Narrowing may only ever run fewer
 * targets, never hide a failed plan behind a green job.
 */

import process from "node:process";

import { androidUniverse, loadConfig } from "./affected.mjs";

const SPACE = /\s+/;

/** Split a space separated Bazel label list, dropping empty entries. */
export function splitLabels(value) {
  return String(value ?? "")
    .trim()
    .split(SPACE)
    .filter(Boolean);
}

/** Join labels back into the workflow's space separated form. */
export function joinLabels(labels) {
  return labels.join(" ");
}

/**
 * Intersect a unit's candidate targets with the affected set.
 *
 * Matches exactly, like `bazel-targets.sh`: a pattern (`//a/...`) is never
 * treated as a target, and an unaffected candidate is dropped even when a
 * sibling is affected.
 */
export function narrow(candidates, affected) {
  const affectedSet = new Set(splitLabels(affected));
  if (affectedSet.size === 0) {
    return [...splitLabels(candidates)];
  }
  return splitLabels(candidates).filter((target) => affectedSet.has(target));
}

/** Every Android test and build target the unit map declares. */
export function androidCandidates(config = loadConfig()) {
  const seen = [];
  for (const unit of config.byPlatform("android")) {
    for (const target of [...unit.testTargets, ...unit.buildTargets]) {
      if (!seen.includes(target)) {
        seen.push(target);
      }
    }
  }
  return seen.sort();
}

/** GitHub-output lines for the metadata command. */
export function metadataLines(config = loadConfig()) {
  return [
    `candidates=${joinLabels(androidCandidates(config))}`,
    `universe=${androidUniverse(config)}`,
  ];
}

/** GitHub-output lines for the narrow command. */
export function narrowLines({ affected, tests, builds }) {
  return [
    `test_targets=${joinLabels(narrow(tests, affected))}`,
    `build_targets=${joinLabels(narrow(builds, affected))}`,
  ];
}

function parseArgs(argv) {
  const values = { affected: "", tests: "", builds: "" };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    const name = flag.startsWith("--") ? flag.slice(2) : "";
    if (!(name in values) || index + 1 >= argv.length) {
      throw new Error(`unknown or incomplete argument: ${flag}`);
    }
    values[name] = argv[index + 1];
    index += 1;
  }
  return values;
}

export function main(argv = process.argv.slice(2)) {
  const [command, ...rest] = argv;
  switch (command) {
    case "metadata":
      console.log(metadataLines().join("\n"));
      return 0;
    case "narrow":
      console.log(narrowLines(parseArgs(rest)).join("\n"));
      return 0;
    default:
      console.error(
        "usage: plan-targets.mjs metadata | narrow [--affected L] [--tests L] [--builds L]",
      );
      return 2;
  }
}

if (process.argv[1] && process.argv[1].endsWith("plan-targets.mjs")) {
  process.exitCode = main();
}
