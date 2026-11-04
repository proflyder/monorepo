#!/usr/bin/env node
/**
 * Validate the CI unit map and prove no source/test file falls out of CI.
 *
 * Checks:
 * 1. schema: unique ids, known platforms, non-empty paths, required sections;
 * 2. android units declare Bazel roots and every test/build target lives under
 *    one of those roots;
 * 3. `depends_on` edges point at existing units;
 * 4. every tracked test/source/build file is owned by at least one unit path or
 *    an explicit invalidation rule, so a new test cannot silently stop running.
 *
 * Run locally with `node platform/ci/check_units.mjs`; CI runs it in the
 * quality job and `make test` runs it through the unit tests.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import {
  DEFAULT_UNITS,
  loadConfig,
  loadConfigFile,
  pathMatches,
} from "./affected.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(HERE, "../..");
export const KNOWN_PLATFORMS = new Set(["go", "android", "python", "js", "native", "quality"]);

// Suffixes/names that must be routed somewhere: compiled sources, tests, build
// files and shell entry points.
export const OWNED_SUFFIXES = [
  ".go",
  ".py",
  ".kt",
  ".kts",
  ".sh",
  ".bzl",
  ".js",
  ".mjs",
  ".cpp",
  ".h",
  ".hpp",
];
export const OWNED_NAMES = ["BUILD.bazel"];

export function trackedFiles(root) {
  const result = spawnSync("git", ["-C", String(root), "ls-files", "-z"], {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.error || result.status !== 0) {
    throw new Error(`git ls-files failed: ${(result.stderr || result.error?.message || "").trim()}`);
  }
  return result.stdout.split("\0").filter(Boolean);
}

export function needsOwner(filePath) {
  const name = filePath.split("/").pop();
  if (OWNED_NAMES.includes(name)) {
    return true;
  }
  return OWNED_SUFFIXES.some((suffix) => name.endsWith(suffix));
}

function hasGlob(directory, pattern) {
  const regex = new RegExp(`^${pattern.replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace("\\*", ".*")}$`);
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (entry.isFile() && regex.test(entry.name)) {
      return true;
    }
  }
  return false;
}

function isDirectory(target) {
  try {
    return fs.statSync(target).isDirectory();
  } catch {
    return false;
  }
}

export function validateSchema(config, root = DEFAULT_ROOT) {
  const problems = [];
  const ids = config.units.map((unit) => unit.id);
  const duplicates = [...new Set(ids.filter((item) => ids.filter((value) => value === item).length > 1))].sort();
  if (duplicates.length) {
    problems.push(`duplicate unit ids: ${duplicates.join(",")}`);
  }

  for (const rule of config.invalidations) {
    const prefix = `invalidation ${JSON.stringify(rule.paths)}`;
    if (!rule.paths.length) {
      problems.push(`${prefix}: no paths declared`);
    }
    if (!rule.platforms.length) {
      problems.push(`${prefix}: no platforms declared`);
    }
    for (const platform of rule.platforms) {
      if (platform !== "*" && !KNOWN_PLATFORMS.has(platform)) {
        problems.push(`${prefix}: unknown platform '${platform}'`);
      }
    }
    if (typeof rule.deploy !== "boolean") {
      problems.push(`${prefix}: deploy must be a bool`);
    }
  }

  for (const unit of config.units) {
    const prefix = `unit ${unit.id}`;
    if (!KNOWN_PLATFORMS.has(unit.platform)) {
      problems.push(`${prefix}: unknown platform '${unit.platform}'`);
    }
    if (!unit.paths.length) {
      problems.push(`${prefix}: no paths declared`);
    }
    for (const pattern of unit.paths) {
      if (pattern === "*" || pattern === "**" || pattern === "**/*") {
        problems.push(`${prefix}: path pattern '${pattern}' is too broad`);
      }
    }
    for (const dep of unit.dependsOn) {
      if (!ids.includes(dep)) {
        problems.push(`${prefix}: depends_on unknown unit '${dep}'`);
      }
    }
    if (unit.deployImages.length && !unit.deployWorkloads.length) {
      problems.push(`${prefix}: deploy.images without deploy.workloads`);
    }
    if (unit.deployWorkloads.length && !unit.deployImages.length) {
      problems.push(`${prefix}: deploy.workloads without deploy.images`);
    }
    if (unit.deployImages.length && !["bazel", "docker", "mixed"].includes(unit.deployBuilder)) {
      problems.push(`${prefix}: unknown deploy.builder '${unit.deployBuilder}'`);
    }
    if (unit.deployImages.length) {
      if (!unit.deployNamespace) {
        problems.push(`${prefix}: deploy.namespace is required`);
      }
      if (!unit.deployScript) {
        problems.push(`${prefix}: deploy.script is required`);
      } else if (!fs.existsSync(path.resolve(root, unit.deployScript)) ||
        !fs.statSync(path.resolve(root, unit.deployScript)).isFile()) {
        problems.push(`${prefix}: deploy.script does not exist: ${unit.deployScript}`);
      }
      if (!unit.deployEnvKey || !unit.deployEnvFile) {
        problems.push(
          `${prefix}: deploy.env_key and deploy.env_file are required ` +
            "(OpenBao secret/ci/deploy-home key and local filename)",
        );
      }
      if (!unit.deployExternalsecrets.length) {
        problems.push(`${prefix}: deploy.externalsecrets is required`);
      }
      if (unit.deployBuilder === "bazel" && !unit.deployTargetRoot.startsWith("//")) {
        problems.push(`${prefix}: deploy.target_root must be a Bazel package (//pkg)`);
      }
      const bazelImages = Object.entries(unit.deployBazelImages || {});
      if (unit.deployBuilder === "mixed") {
        if (!bazelImages.length) {
          problems.push(
            `${prefix}: a mixed unit needs deploy.bazel_images (image -> Bazel package)`,
          );
        } else if (unit.deployTargetRoot) {
          problems.push(
            `${prefix}: deploy.target_root is ambiguous for a mixed unit; use deploy.bazel_images`,
          );
        }
      }
      for (const [image, target] of bazelImages) {
        if (!unit.deployImages.includes(image)) {
          problems.push(`${prefix}: deploy.bazel_images.${image} is not in deploy.images`);
        }
        if (!String(target).startsWith("//")) {
          problems.push(
            `${prefix}: deploy.bazel_images.${image} must be a Bazel package (//pkg)`,
          );
        }
      }
    }
    if (unit.platform === "go") {
      if (!unit.goPackages.length) {
        problems.push(`${prefix}: go unit without packages`);
      }
    } else if (unit.platform === "android") {
      if (!unit.bazelRoots.length) {
        problems.push(`${prefix}: android unit without bazel.roots`);
      }
      for (const target of [...unit.testTargets, ...unit.buildTargets]) {
        if (
          !unit.bazelRoots.some(
            (bazelRoot) =>
              target.startsWith(`${bazelRoot}:`) || target.startsWith(`${bazelRoot}/`),
          )
        ) {
          problems.push(`${prefix}: target ${target} is outside bazel.roots`);
        }
      }
      if (!unit.testTargets.length) {
        problems.push(`${prefix}: android unit without test_targets`);
      }
    } else if (unit.platform === "python") {
      if (!unit.suites.length && !unit.compileDirs.length) {
        problems.push(`${prefix}: python unit without suites or compile dirs`);
      }
      for (const suite of unit.suites) {
        if (!isDirectory(path.resolve(root, suite))) {
          problems.push(`${prefix}: suite directory missing: ${suite}`);
        } else if (!hasGlob(path.resolve(root, suite), "test_*.py")) {
          problems.push(
            `${prefix}: suite ${suite} has no test_*.py; use python.compile for ` +
              "syntax-only directories",
          );
        }
      }
      for (const directory of unit.compileDirs) {
        if (!isDirectory(path.resolve(root, directory))) {
          problems.push(`${prefix}: compile directory missing: ${directory}`);
        }
      }
    } else if (unit.platform === "js") {
      if (!unit.jsTests.length) {
        problems.push(`${prefix}: js unit without test directories`);
      }
      for (const directory of unit.jsTests) {
        if (!isDirectory(path.resolve(root, directory))) {
          problems.push(`${prefix}: js test directory missing: ${directory}`);
        } else if (!hasGlob(path.resolve(root, directory), "*.test.js")) {
          problems.push(`${prefix}: js test directory ${directory} has no *.test.js`);
        }
      }
    } else if (unit.platform === "native") {
      if (!unit.nativeWorkdir) {
        problems.push(`${prefix}: native unit without workdir`);
      } else if (!isDirectory(path.resolve(root, unit.nativeWorkdir))) {
        problems.push(`${prefix}: native workdir missing: ${unit.nativeWorkdir}`);
      }
      if (!unit.nativeBuild) {
        problems.push(`${prefix}: native unit without a build command`);
      }
      if (!unit.nativeArtifacts.length) {
        problems.push(`${prefix}: native unit without artifacts`);
      }
      for (const artifact of [
        ...unit.nativeArtifacts,
        ...(unit.nativeChecksums ? [unit.nativeChecksums] : []),
      ]) {
        if (artifact.startsWith("/") || artifact.split(/[\\/]/).includes("..")) {
          problems.push(`${prefix}: artifact must be relative to workdir: ${artifact}`);
        }
      }
      if (!unit.nativeChecksums) {
        problems.push(`${prefix}: native unit without a checksums file`);
      }
      const toolchain = [
        unit.nativeToolchainName,
        unit.nativeToolchainVersion,
        unit.nativeToolchainUrl,
        unit.nativeToolchainSha256,
        unit.nativeToolchainRoot,
      ];
      if (toolchain.some(Boolean) && !toolchain.every(Boolean)) {
        problems.push(
          `${prefix}: native toolchain needs name, version, url, ` +
            "sha256 and root",
        );
      }
      if (unit.nativeToolchainSha256 && !/^[0-9a-f]{64}$/.test(unit.nativeToolchainSha256)) {
        problems.push(`${prefix}: native toolchain sha256 must be 64 lowercase hex chars`);
      }
      if (unit.nativeToolchainUrl && !unit.nativeToolchainUrl.startsWith("https://")) {
        problems.push(`${prefix}: native toolchain url must be https`);
      }
    }
  }
  return problems;
}

export function coverageProblems(config, files) {
  const ownedPatterns = config.units.flatMap((unit) => unit.paths);
  ownedPatterns.push(...config.invalidations.flatMap((rule) => rule.paths));
  const uncovered = files.filter(
    (filePath) => needsOwner(filePath) && !pathMatches(filePath, ownedPatterns),
  );
  if (!uncovered.length) {
    return [];
  }
  const lines = [
    `${uncovered.length} tracked file(s) are not owned by any unit in platform/ci/units.mjs:`,
  ];
  lines.push(...uncovered.slice(0, 40).map((filePath) => `  - ${filePath}`));
  if (uncovered.length > 40) {
    lines.push(`  ... and ${uncovered.length - 40} more`);
  }
  lines.push("Add the path to a unit (or an invalidation rule) so CI keeps running its tests.");
  return lines;
}

function parseArgs(argv) {
  const args = { root: DEFAULT_ROOT, units: DEFAULT_UNITS, quiet: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    switch (arg) {
      case "--root":
        index += 1;
        args.root = argv[index];
        break;
      case "--units":
        index += 1;
        args.units = argv[index];
        break;
      case "--quiet":
        args.quiet = true;
        break;
      default:
        throw new Error(`unrecognized argument: ${arg}`);
    }
  }
  return args;
}

export async function check(argv) {
  const args = parseArgs(argv);
  const root = path.resolve(args.root);
  const config = args.units === DEFAULT_UNITS ? loadConfig() : await loadConfigFile(args.units);
  return {
    root,
    config,
    problems: [
      ...validateSchema(config, root),
      ...coverageProblems(config, trackedFiles(root)),
    ],
  };
}

export function formatSuccess(config) {
  const counts = [...KNOWN_PLATFORMS]
    .sort()
    .map((platform) => `${platform}=${config.byPlatform(platform).length}`)
    .join(", ");
  return `units OK (${counts}); every tracked source/test file is routed`;
}

export async function main(argv = process.argv.slice(2)) {
  const args = parseArgs(argv);
  const { config, problems } = await check(argv);
  if (problems.length) {
    for (const problem of problems) {
      console.error(problem);
    }
    return 1;
  }
  if (!args.quiet) {
    console.log(formatSuccess(config));
  }
  return 0;
}

/** Entry point for `actions/github-script`: fail the step on any problem. */
export async function runGithubScript({ core, argv = [] }) {
  const args = parseArgs(argv);
  const { config, problems } = await check(argv);
  if (problems.length) {
    for (const problem of problems) {
      core.error(problem);
    }
    throw new Error(`${problems.length} unit-map problem(s)`);
  }
  if (!args.quiet) {
    core.info(formatSuccess(config));
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().then(
    (code) => {
      process.exitCode = code;
    },
    (error) => {
      console.error(error?.stack || String(error));
      process.exitCode = 1;
    },
  );
}
