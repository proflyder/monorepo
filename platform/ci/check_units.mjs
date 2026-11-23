#!/usr/bin/env node

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

export const MAX_UNIT_IMAGES = 4;

export const ROLLBACK_KINDS = new Set([
  "deploy-state",
  "helm",
  "hermes",
  "pulsar",
  "flux-pins",
  "forward-only",
]);

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

export function workflowTriggers(text) {
  const lines = String(text || "").split(/\r?\n/);
  const start = lines.findIndex((line) => /^on:/.test(line));
  if (start === -1) {
    return null;
  }
  const inline = /^on:\s*(.*)$/.exec(lines[start])?.[1]?.trim() ?? "";
  if (inline) {
    if (inline.startsWith("[")) {
      return inline
        .replace(/^\[/, "")
        .replace(/\]$/, "")
        .split(",")
        .map((item) => item.trim().split(/\s+/)[0])
        .filter(Boolean);
    }
    return [inline.split(/\s+/)[0]];
  }
  const events = [];
  for (let index = start + 1; index < lines.length; index += 1) {
    const line = lines[index];
    if (!line.trim() || /^\s*#/.test(line)) {
      continue;
    }
    const match = /^ {2}([A-Za-z_][A-Za-z0-9_-]*):/.exec(line);
    if (match) {
      events.push(match[1]);
      continue;
    }
    if (/^\s/.test(line)) {
      continue;
    }
    break;
  }
  return events;
}

export const ANDROID_SUITE_PACKAGE = "projects/mobile/android";
export const ANDROID_SUITE_NAME = "android_tests";

function buildFiles(directory) {
  if (!isDirectory(directory)) {
    return [];
  }
  return fs
    .readdirSync(directory, { recursive: true })
    .filter((name) => path.basename(String(name)) === "BUILD.bazel")
    .map((name) => path.join(directory, String(name)))
    .sort();
}

export function declaredTestLabels(packagePath, text) {
  const names = [...String(text).matchAll(/\bname\s*=\s*"([A-Za-z0-9_.-]+_test)"/g)];
  return names.map((match) => `//${packagePath}:${match[1]}`);
}

export function suiteTestLabels(text, suiteName) {
  const start = String(text).search(
    new RegExp(`test_suite\\(\\s*name\\s*=\\s*"${suiteName}"`),
  );
  if (start === -1) {
    return null;
  }
  const body = /tests\s*=\s*\[([^\]]*)\]/.exec(String(text).slice(start));
  return body ? [...body[1].matchAll(/"(\/\/[^"]+)"/g)].map((match) => match[1]) : [];
}

export function androidSuiteCoverageProblems(config, root = DEFAULT_ROOT) {
  const directory = path.resolve(root, ANDROID_SUITE_PACKAGE);
  const suiteFile = path.join(directory, "BUILD.bazel");
  if (!fs.existsSync(suiteFile)) {
    return [];
  }
  const suite = suiteTestLabels(fs.readFileSync(suiteFile, "utf8"), ANDROID_SUITE_NAME);
  if (!suite) {
    return [`//${ANDROID_SUITE_PACKAGE}:${ANDROID_SUITE_NAME}: test_suite not found`];
  }
  const unitTargets = config.units.flatMap((unit) => unit.testTargets ?? []);
  const covered = new Set([...suite, ...unitTargets]);
  const declared = buildFiles(directory).flatMap((file) =>
    declaredTestLabels(
      path.relative(root, path.dirname(file)).split(path.sep).join("/"),
      fs.readFileSync(file, "utf8"),
    ),
  );
  return declared
    .filter((label) => !covered.has(label))
    .map(
      (label) =>
        `${label}: not in //${ANDROID_SUITE_PACKAGE}:${ANDROID_SUITE_NAME}, CI never runs it`,
    );
}

export function workflowTriggerProblems(root = DEFAULT_ROOT) {
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
    const events = workflowTriggers(fs.readFileSync(path.join(directory, name), "utf8"));
    const reusable = name.startsWith("_");
    if (!events) {
      problems.push(`${relative}: no top-level 'on:' block`);
      continue;
    }
    if (events.length === 0) {
      problems.push(`${relative}: 'on:' declares no event`);
      continue;
    }
    if (events.length > 1) {
      problems.push(
        `${relative}: exactly one trigger is allowed, found ${events.length} ` +
          `(${events.join(", ")}); split it into thin wrappers that call a shared ` +
          "_*.yml reusable workflow",
      );
      continue;
    }
    if (reusable && events[0] !== "workflow_call") {
      problems.push(
        `${relative}: a reusable workflow must only have 'on: workflow_call', ` +
          `found '${events[0]}'`,
      );
    }
    if (!reusable && events[0] === "workflow_call") {
      problems.push(
        `${relative}: a workflow_call entry point must be named _*.yml ` +
          "(the convention that marks a file as a reusable, not a trigger)",
      );
    }
  }
  return problems;
}

export const PERMISSION_SCOPES = [
  "actions",
  "attestations",
  "checks",
  "contents",
  "deployments",
  "discussions",
  "id-token",
  "issues",
  "models",
  "packages",
  "pages",
  "pull-requests",
  "security-events",
  "statuses",
];

export const PERMISSION_LEVELS = { none: 0, read: 1, write: 2 };
const PERMISSION_LEVEL_NAMES = ["none", "read", "write"];

function permissionLevel(value) {
  return PERMISSION_LEVELS[value] ?? PERMISSION_LEVELS.write;
}

function stripInlineComment(value) {
  return String(value).replace(/\s+#.*$/, "").trim();
}

function permissionMap(inline) {
  const permissions = new Map();
  if (!inline || inline === "{}") {
    return permissions;
  }
  if (inline === "read-all" || inline === "write-all") {
    const level = inline === "read-all" ? "read" : "write";
    for (const scope of PERMISSION_SCOPES) {
      permissions.set(scope, PERMISSION_LEVELS[level]);
    }
    return permissions;
  }
  if (inline.startsWith("{") && inline.endsWith("}")) {
    for (const entry of inline.slice(1, -1).split(",")) {
      const match = /^\s*([A-Za-z0-9_-]+)\s*:\s*([a-z-]+)\s*$/.exec(entry);
      if (match) {
        permissions.set(match[1], permissionLevel(match[2]));
      }
    }
  }
  return permissions;
}

function parsePermissions(lines, start) {
  const key = /^(\s*)permissions:(.*)$/.exec(lines[start]);
  const indent = key[1].length;
  const inline = stripInlineComment(key[2]);
  if (inline) {
    return permissionMap(inline);
  }
  const permissions = new Map();
  for (let index = start + 1; index < lines.length; index += 1) {
    const line = lines[index];
    if (!line.trim() || /^\s*#/.test(line)) {
      continue;
    }
    if (/^(\s*)/.exec(line)[1].length <= indent) {
      break;
    }
    const entry = /^\s+([A-Za-z0-9_-]+):\s*(.*)$/.exec(line);
    if (entry) {
      permissions.set(entry[1], permissionLevel(stripInlineComment(entry[2])));
    }
  }
  return permissions;
}

export function workflowPermissionModel(text) {
  const lines = String(text || "").split(/\r?\n/);
  const workflowLine = lines.findIndex((line) => /^permissions:/.test(line));
  const workflow = workflowLine === -1 ? null : parsePermissions(lines, workflowLine);
  const jobs = new Map();
  const jobsLine = lines.findIndex((line) => /^jobs:\s*$/.test(line));
  if (jobsLine === -1) {
    return { workflow, jobs };
  }
  const starts = [];
  for (let index = jobsLine + 1; index < lines.length; index += 1) {
    const match = /^ {2}([A-Za-z0-9_-]+):\s*$/.exec(lines[index]);
    if (match) {
      starts.push({ job: match[1], index });
    }
  }
  for (let position = 0; position < starts.length; position += 1) {
    const { job, index: start } = starts[position];
    const end = position + 1 < starts.length ? starts[position + 1].index : lines.length;
    let permissions = null;
    let uses = null;
    for (let index = start; index < end; index += 1) {
      if (permissions === null && /^ {4}permissions:/.test(lines[index])) {
        permissions = parsePermissions(lines, index);
      }
      const use = /^ {4}uses:\s*(.+)$/.exec(lines[index]);
      if (use && uses === null) {
        uses = stripInlineComment(use[1]);
      }
    }
    jobs.set(job, { permissions, uses });
  }
  return { workflow, jobs };
}

function localWorkflowPath(uses, root) {
  if (!uses || !uses.startsWith("./.github/workflows/")) {
    return null;
  }
  if (!/\.ya?ml$/.test(uses)) {
    return null;
  }
  return path.resolve(root, uses);
}

function relativeWorkflowPath(root, file) {
  return path.relative(root, file).split(path.sep).join("/");
}

function mergePermissions(target, source, via) {
  if (!source) {
    return;
  }
  for (const [scope, level] of source) {
    const current = target.get(scope);
    if (!current || level > current.level) {
      target.set(scope, { level, via });
    }
  }
}

function collectRequiredPermissions(file, root, seen, required) {
  const relative = relativeWorkflowPath(root, file);
  const { workflow, jobs } = workflowPermissionModel(fs.readFileSync(file, "utf8"));
  mergePermissions(required, workflow, relative);
  for (const spec of jobs.values()) {
    mergePermissions(required, spec.permissions, relative);
    const target = localWorkflowPath(spec.uses, root);
    if (target && !seen.has(target)) {
      seen.add(target);
      if (fs.existsSync(target)) {
        collectRequiredPermissions(target, root, seen, required);
      }
    }
  }
}

export function workflowCallerPermissionProblems(root = DEFAULT_ROOT) {
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
    const { workflow, jobs } = workflowPermissionModel(
      fs.readFileSync(path.join(directory, name), "utf8"),
    );
    for (const [job, spec] of jobs) {
      const target = localWorkflowPath(spec.uses, root);
      if (!target || !fs.existsSync(target)) {
        continue;
      }
      const required = new Map();
      collectRequiredPermissions(target, root, new Set([target]), required);
      const granted = spec.permissions ?? workflow ?? new Map();
      for (const [scope, requirement] of required) {
        const level = granted.get(scope) ?? PERMISSION_LEVELS.none;
        if (level >= requirement.level) {
          continue;
        }
        const callee = relativeWorkflowPath(root, target);
        const via =
          requirement.via && requirement.via !== callee
            ? ` (via ${requirement.via})`
            : "";
        problems.push(
          `${relative} job '${job}' grants ${scope}=` +
            `${PERMISSION_LEVEL_NAMES[level]} but ${spec.uses} requires ` +
            `${scope}=${PERMISSION_LEVEL_NAMES[requirement.level]}${via}; ` +
            "grant the union of the reusable workflow's permissions at job level " +
            "or GitHub fails the run with startup_failure",
        );
      }
    }
  }
  return problems;
}

export function workflowMatrixOutputProblems(root = DEFAULT_ROOT) {
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
    const starts = [];
    for (let index = jobsLine + 1; index < lines.length; index += 1) {
      const match = /^ {2}([A-Za-z0-9_-]+):\s*$/.exec(lines[index]);
      if (match) {
        starts.push({ job: match[1], index });
      }
    }
    const matrixJobs = new Set();
    for (let position = 0; position < starts.length; position += 1) {
      const end = position + 1 < starts.length ? starts[position + 1].index : lines.length;
      const body = lines.slice(starts[position].index, end);
      if (body.some((line) => /^ {6}matrix:/.test(line))) {
        matrixJobs.add(starts[position].job);
      }
    }
    lines.forEach((line, index) => {
      for (const match of line.matchAll(/needs\.([A-Za-z0-9_-]+)\.outputs\./g)) {
        if (matrixJobs.has(match[1])) {
          problems.push(
            `${relative}:${index + 1} reads needs.${match[1]}.outputs of a matrix job; ` +
              "GitHub keeps only the output of the leg that finished last, so another " +
              "matrix entry can win (deploy station got the logon-provider version); " +
              "read the value from a non-matrix job instead",
          );
        }
      }
    });
  }
  return problems;
}

export const MAX_BACKGROUND_STEPS = 10;

export function stepBlocks(text) {
  const lines = String(text).split(/\r?\n/);
  const blocks = [];
  for (let index = 0; index < lines.length; index += 1) {
    const header = /^(\s*)steps:\s*$/.exec(lines[index]);
    if (!header) {
      continue;
    }
    const base = header[1].length;
    const steps = [];
    let itemIndent = -1;
    let current = null;
    let cursor = index + 1;
    for (; cursor < lines.length; cursor += 1) {
      const line = lines[cursor];
      if (!line.trim() || /^\s*#/.test(line)) {
        continue;
      }
      const indent = line.length - line.trimStart().length;
      if (indent <= base && !(indent === base && line.trimStart().startsWith("- "))) {
        break;
      }
      const item = /^(\s*)-\s+(.*)$/.exec(line);
      if (item && (itemIndent === -1 || item[1].length === itemIndent)) {
        itemIndent = item[1].length;
        current = { line: cursor + 1, keys: {} };
        steps.push(current);
        const key = /^([A-Za-z0-9_-]+):\s*(.*)$/.exec(item[2]);
        if (key) {
          current.keys[key[1]] = key[2].trim();
        }
        continue;
      }
      if (current && indent === itemIndent + 2) {
        const key = /^\s*([A-Za-z0-9_-]+):\s*(.*)$/.exec(line);
        if (key) {
          current.keys[key[1]] = key[2].trim();
        }
      }
    }
    blocks.push({ line: index + 1, steps });
    index = cursor - 1;
  }
  return blocks;
}

function unquote(value) {
  return String(value ?? "").replace(/^["']|["']$/g, "");
}

export function waitTargets(value) {
  const raw = String(value ?? "").trim();
  const list = /^\[(.*)\]$/.exec(raw);
  return (list ? list[1].split(",") : [raw]).map((item) => unquote(item.trim())).filter(Boolean);
}

export function parallelStepProblems(relative, text, { composite = false } = {}) {
  const problems = [];
  for (const block of stepBlocks(text)) {
    const background = new Map();
    const pending = new Set();
    for (const step of block.steps) {
      const where = `${relative}:${step.line}`;
      const keys = step.keys;
      if ("parallel" in keys) {
        problems.push(
          `${where}: use 'background: true' steps and 'wait'/'wait-all' instead of 'parallel:' ` +
            "(actionlint does not check steps nested under 'parallel')",
        );
      }
      const isBackground = unquote(keys.background) === "true";
      const isWait = "wait" in keys || "wait-all" in keys || "cancel" in keys;
      if (composite && (isBackground || isWait || "background" in keys)) {
        problems.push(`${where}: composite actions cannot declare background, wait or cancel steps`);
        continue;
      }
      if (isBackground) {
        const id = unquote(keys.id);
        if (!id) {
          problems.push(`${where}: a background step needs an 'id' so a wait and the report can name it`);
          continue;
        }
        background.set(id, step.line);
        pending.add(id);
        if (pending.size > MAX_BACKGROUND_STEPS) {
          problems.push(
            `${where}: more than ${MAX_BACKGROUND_STEPS} background steps run at once; ` +
              "GitHub queues the rest, add a wait before starting more",
          );
        }
        continue;
      }
      if ("wait-all" in keys) {
        if (keys["wait-all"]) {
          problems.push(`${where}: 'wait-all' takes no value`);
        }
        if (!pending.size) {
          problems.push(`${where}: 'wait-all' with no background step started before it`);
        }
        pending.clear();
      }
      if ("wait" in keys || "cancel" in keys) {
        const targets = "wait" in keys ? waitTargets(keys.wait) : waitTargets(keys.cancel);
        if (!targets.length) {
          problems.push(`${where}: '${"wait" in keys ? "wait" : "cancel"}' names no step`);
        }
        for (const target of targets) {
          if (!background.has(target)) {
            problems.push(`${where}: '${target}' is not a background step started earlier in this job`);
          }
          pending.delete(target);
        }
      }
      const actions = ["run", "uses", "wait", "wait-all", "cancel", "parallel"].filter((key) => key in keys);
      if (actions.length !== 1) {
        problems.push(
          `${where}: a step needs exactly one of run, uses, wait, wait-all or cancel` +
            (actions.length ? ` (found ${actions.join(", ")})` : ""),
        );
      }
      if (isWait && "if" in keys) {
        problems.push(`${where}: wait, wait-all and cancel always run and do not support 'if'`);
      }
    }
    for (const id of pending) {
      problems.push(
        `${relative}:${background.get(id)}: background step '${id}' is never waited for; ` +
          "add 'wait' or 'wait-all' so its failure is reported before the job ends",
      );
    }
  }
  return problems;
}

export function workflowParallelStepProblems(root = DEFAULT_ROOT) {
  const problems = [];
  const workflows = path.resolve(root, ".github/workflows");
  if (isDirectory(workflows)) {
    for (const name of fs.readdirSync(workflows).sort()) {
      if (name.endsWith(".yml") || name.endsWith(".yaml")) {
        problems.push(
          ...parallelStepProblems(
            `.github/workflows/${name}`,
            fs.readFileSync(path.join(workflows, name), "utf8"),
          ),
        );
      }
    }
  }
  const actions = path.resolve(root, ".github/actions");
  if (isDirectory(actions)) {
    for (const name of fs.readdirSync(actions).sort()) {
      for (const file of ["action.yml", "action.yaml"]) {
        const target = path.join(actions, name, file);
        if (fs.existsSync(target)) {
          problems.push(
            ...parallelStepProblems(`.github/actions/${name}/${file}`, fs.readFileSync(target, "utf8"), {
              composite: true,
            }),
          );
        }
      }
    }
  }
  return problems;
}

function isRegistryLogin(line) {
  return REGISTRY_LOGIN_PATTERNS.some((pattern) => pattern.test(line));
}

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
      ...workflowTriggerProblems(root),
      ...androidSuiteCoverageProblems(config, root),
      ...workflowCallerPermissionProblems(root),
      ...workflowMatrixOutputProblems(root),
      ...workflowParallelStepProblems(root),
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
