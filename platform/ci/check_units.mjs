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
 * quality job and `mise run test` runs it through the unit tests.
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
export const KNOWN_PLATFORMS = new Set([
  "android",
  "bazel",
  "python",
  "js",
  "native",
  "quality",
]);

// `_unit.yml` attests each pushed image with one static step per index; the
// unit map must not exceed that bound (today the maximum is Hermes' four).
export const MAX_UNIT_IMAGES = 4;

// Where a deployable unit is applied: `deploy-home` (`ci`) or a Flux
// Kustomization that reads the `deploy/home-apps` pins branch (`flux`).
export const DEPLOY_RUNTIMES = new Set(["ci", "flux"]);

// Rollback kinds backed by `mise run rollback`. `forward-only` is an explicit
// "no documented procedure"; `pulsar` only prints the on-device instruction.
export const ROLLBACK_KINDS = new Set([
  "deploy-state",
  "helm",
  "hermes",
  "pulsar",
  "forward-only",
]);

// Suffixes/names that must be routed somewhere: compiled sources, tests, build
// files and shell entry points.
export const OWNED_SUFFIXES = [
  ".rs",
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
    if (unit.deployImages.length && !DEPLOY_RUNTIMES.has(unit.deployRuntime)) {
      problems.push(
        `${prefix}: unknown deploy.runtime '${unit.deployRuntime}' ` +
          `(expected ${[...DEPLOY_RUNTIMES].join(", ")})`,
      );
    }
    // `_unit.yml` has one static attestation step per index; include the
    // automatically published home-deploy image in the bound.
    const attestImages = [...unit.deployImages];
    if (
      (unit.deployBuilder === "bazel" || unit.deployBuilder === "mixed") &&
      !attestImages.includes("home-deploy")
    ) {
      attestImages.push("home-deploy");
    }
    if (attestImages.length > MAX_UNIT_IMAGES) {
      problems.push(
        `${prefix}: ${attestImages.length} attested images (including ` +
          `home-deploy) exceed the ${MAX_UNIT_IMAGES} attestation steps in _unit.yml`,
      );
    }
    if (unit.rollbackKind) {
      if (!ROLLBACK_KINDS.has(unit.rollbackKind)) {
        problems.push(
          `${prefix}: unknown rollback.kind '${unit.rollbackKind}' ` +
            `(expected ${[...ROLLBACK_KINDS].join(", ")})`,
        );
      }
      if (unit.rollbackKind === "forward-only" && !unit.rollbackNote) {
        problems.push(`${prefix}: rollback.kind forward-only needs a note explaining the fix`);
      }
      if (unit.rollbackKind === "deploy-state") {
        if (!unit.rollbackStateDir) {
          problems.push(`${prefix}: rollback.kind deploy-state needs state_dir`);
        }
        if (!unit.rollbackNamespace) {
          problems.push(`${prefix}: rollback.kind deploy-state needs namespace`);
        }
      }
      if (unit.rollbackKind === "helm" || unit.rollbackKind === "hermes") {
        if (!unit.rollbackRelease) {
          problems.push(`${prefix}: rollback.kind ${unit.rollbackKind} needs release`);
        }
        if (!unit.rollbackNamespace) {
          problems.push(`${prefix}: rollback.kind ${unit.rollbackKind} needs namespace`);
        }
      }
      if (unit.rollbackKind === "pulsar" && !unit.rollbackInstruction) {
        problems.push(`${prefix}: rollback.kind pulsar needs the documented instruction`);
      }
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
    if (unit.platform === "android" || unit.platform === "bazel") {
      if (!unit.bazelRoots.length) {
        problems.push(`${prefix}: ${unit.platform} unit without bazel.roots`);
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
        problems.push(`${prefix}: ${unit.platform} unit without test_targets`);
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
      if (!["script", "pulsar"].includes(unit.nativeKind)) {
        problems.push(`${prefix}: unknown native.kind '${unit.nativeKind}'`);
        continue;
      }
      if (unit.nativeKind === "script" && !unit.nativeBuild) {
        problems.push(`${prefix}: native unit without a build command`);
      }
      if (unit.nativeKind === "script" && !unit.nativeArtifacts.length) {
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
      if (unit.nativeKind === "script" && !unit.nativeChecksums) {
        problems.push(`${prefix}: native unit without a checksums file`);
      }
      if (unit.nativePublishScript) {
        const script = path.resolve(root, unit.nativePublishScript);
        if (!fs.existsSync(script) || !fs.statSync(script).isFile()) {
          problems.push(
            `${prefix}: publish_script does not exist: ${unit.nativePublishScript}`,
          );
        }
        if (!unit.nativePublishRepository) {
          problems.push(`${prefix}: publish_script requires publish_repository`);
        } else if (
          !/^ghcr\.io\/[a-z0-9.-]+\/[a-z0-9._-]+$/.test(unit.nativePublishRepository)
        ) {
          problems.push(
            `${prefix}: publish_repository must be ghcr.io/<owner>/<name>: ` +
              `'${unit.nativePublishRepository}'`,
          );
        }
        if (!unit.nativePublishTags.length) {
          problems.push(`${prefix}: publish_script requires publish_tags`);
        }
      }
      for (const pattern of unit.nativePublishWhen) {
        if (pattern === "*" || pattern === "**" || pattern === "**/*") {
          problems.push(`${prefix}: publish_when pattern '${pattern}' is too broad`);
        }
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

/**
 * Registry writes in a workflow job need credentials even when the package is
 * publicly readable: anonymous reads succeed and only the manifest PUT fails
 * with UNAUTHORIZED. Match the commands that write, including
 * `platform/ci/promote.mjs`, which re-tags with crane internally and cannot be
 * found by looking for literal `crane tag` text.
 */
export const REGISTRY_WRITE_PATTERNS = [
  /\bcrane\s+(?:tag|push)\b/,
  /\boras\s+(?:tag|push|manifest\s+push)\b/,
  /\bpromote\.mjs\b/,
  /^\s*push-to-registry:\s*true\s*$/,
];

export const REGISTRY_LOGIN_PATTERNS = [
  /uses:\s+docker\/login-action@/,
  /\bcrane\s+auth\s+login\b/,
  /\boras\s+login\b/,
  /\bdocker\s+login\b/,
];

function isRegistryWrite(line) {
  return REGISTRY_WRITE_PATTERNS.some((pattern) => pattern.test(line));
}

function isRegistryLogin(line) {
  return REGISTRY_LOGIN_PATTERNS.some((pattern) => pattern.test(line));
}

/**
 * Every job that writes to a registry must log in before the first write.
 * Jobs are delimited by their two-space keys under the top-level `jobs:` map;
 * a nested `steps:` list never uses that indentation, so a line scan is enough
 * and needs no YAML dependency.
 */
export function workflowRegistryProblems(root = DEFAULT_ROOT) {
  const problems = [];
  const directory = path.resolve(root, ".github/workflows");
  if (!isDirectory(directory)) {
    return problems;
  }
  const files = fs
    .readdirSync(directory)
    .filter((name) => name.endsWith(".yml") || name.endsWith(".yaml"))
    .sort();
  for (const name of files) {
    const relative = `.github/workflows/${name}`;
    const lines = fs.readFileSync(path.join(directory, name), "utf8").split(/\r?\n/);
    const jobsLine = lines.findIndex((line) => /^jobs:\s*$/.test(line));
    if (jobsLine === -1) {
      continue;
    }
    const jobStarts = [];
    for (let index = jobsLine + 1; index < lines.length; index += 1) {
      const match = /^  ([A-Za-z0-9_-]+):\s*$/.exec(lines[index]);
      if (match) {
        jobStarts.push({ job: match[1], index });
      }
    }
    for (let position = 0; position < jobStarts.length; position += 1) {
      const { job, index: start } = jobStarts[position];
      const end = position + 1 < jobStarts.length ? jobStarts[position + 1].index : lines.length;
      let firstWrite = -1;
      let firstLogin = -1;
      for (let index = start; index < end; index += 1) {
        // A comment that mentions `crane tag` is prose, not a write.
        if (/^\s*#/.test(lines[index])) {
          continue;
        }
        if (firstLogin === -1 && isRegistryLogin(lines[index])) {
          firstLogin = index;
        }
        if (firstWrite === -1 && isRegistryWrite(lines[index])) {
          firstWrite = index;
        }
      }
      if (firstWrite === -1) {
        continue;
      }
      if (firstLogin === -1) {
        problems.push(
          `${relative} job '${job}' writes to a registry without a login step ` +
            "(public packages still need credentials for the manifest PUT)",
        );
      } else if (firstLogin > firstWrite) {
        problems.push(
          `${relative} job '${job}' logs in after its first registry write; ` +
            "the login must come first",
        );
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
      ...workflowRegistryProblems(root),
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
