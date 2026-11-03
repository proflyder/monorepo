#!/usr/bin/env node
/**
 * Compute the CI units affected by a change range.
 *
 * Routing is declarative for paths (`platform/ci/units.mjs`) and dependency
 * based for code:
 *
 *   go      - `go list -deps` of each unit's packages. A change in a shared
 *             package (for example `packages/proto`) selects every unit that
 *             imports it.
 *   android - reverse dependencies from the Bazel loading graph
 *             (`bazel query --output=xml`). Rule labels are mapped back to unit
 *             names by `bazel.roots`, so a change in `packages/kotlin` selects
 *             only the applications that actually depend on it. This needs no
 *             JDK and no Android SDK because it never analyses targets.
 *   MODULE.bazel - global, unless the diff only touches the
 *             rules_jvm_external Maven extension; then it becomes
 *             `MODULE.bazel#maven` and invalidates only android.
 *   python  - path rules plus optional declarative `depends_on`.
 *   js      - path rules for dependency-free edge Workers (runs `node --test`).
 *   native  - path rules for non-Bazel C/C++ repository-script builds.
 *   quality - always runs.
 *
 * Any failure to compute a dependency closure degrades to "every unit of that
 * platform" (never to "nothing"), as required for a required-check gate.
 *
 * Usage:
 *
 *     platform/ci/affected.mjs --base origin/develop --head HEAD --format text
 *     platform/ci/affected.mjs --base "$BASE_SHA" --format github
 *     platform/ci/affected.mjs --changed-file solutions/x/y.go --format json
 *
 * Exit status is 0 unless the range cannot be resolved at all.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";

import unitModule from "./units.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const DEFAULT_UNITS = path.join(HERE, "units.mjs");
export const PLATFORMS = ["go", "android", "python", "js", "native", "quality"];

// --------------------------------------------------------------------------
// Configuration
// --------------------------------------------------------------------------
export class Unit {
  constructor(init = {}) {
    Object.assign(
      this,
      {
        id: "",
        platform: "",
        title: "",
        paths: [],
        bazelRoots: [],
        testTargets: [],
        buildTargets: [],
        artifact: "",
        artifactName: "",
        // Extra Android SDK packages a unit's Bazel actions need, beyond the
        // platform/build-tools baseline installed by `_android.yml`.
        bazelSdkPackages: [],
        goModule: ".",
        goPackages: [],
        goBuild: [],
        goRace: false,
        suites: [],
        compileDirs: [],
        // Python units whose suites exec the Go deploy binary.
        pythonDeployTool: false,
        jsTests: [],
        // Native (non-Bazel) build units: a repository script that produces
        // host- or cross-built artifacts, plus the pinned toolchain it needs.
        nativeWorkdir: "",
        nativeBuild: "",
        nativeTest: "",
        nativeArtifacts: [],
        nativeChecksums: "",
        nativeToolchainName: "",
        nativeToolchainVersion: "",
        nativeToolchainUrl: "",
        nativeToolchainSha256: "",
        nativeToolchainRoot: "",
        dependsOn: [],
        // Deployable units own container images and Kubernetes workloads. An
        // empty `deployImages` means "this unit is not deployed by deploy-home".
        deployImages: [],
        deployWorkloads: [],
        deployBuilder: "bazel",
        // Bazel package that owns the `<image>_image`/`<image>_push` targets.
        deployTargetRoot: "",
        // Kubernetes namespace, deploy entrypoint and the OpenBao `secret/ci/
        // deploy-home` key/file that carries the private deploy-time env.
        deployNamespace: "",
        deployScript: "",
        deployEnvKey: "",
        deployEnvFile: "",
        // ExternalSecrets that must be Ready before the deploy touches workloads.
        deployExternalsecrets: [],
      },
      init,
    );
  }
}

export class Invalidation {
  constructor({ paths = [], platforms = [], deploy = true } = {}) {
    this.paths = paths;
    this.platforms = platforms;
    // `deploy: false` means "run these CI jobs, but do not redeploy units just
    // because this path changed". Workflow and CI-harness edits only need the
    // checks to re-run; they must not roll the home units after merge.
    this.deploy = deploy;
  }
}

export class Config {
  constructor({ units = [], invalidations = [], deployControlUnits = [], deployControlPaths = [] } = {}) {
    this.units = units;
    this.invalidations = invalidations;
    this.deployControlUnits = deployControlUnits;
    this.deployControlPaths = deployControlPaths;
  }

  byPlatform(platform) {
    return this.units.filter((unit) => unit.platform === platform);
  }

  get(unitId) {
    const unit = this.units.find((candidate) => candidate.id === unitId);
    if (!unit) {
      throw new Error(`unknown unit: ${unitId}`);
    }
    return unit;
  }
}

function list(value) {
  return Array.isArray(value) ? [...value] : [];
}

export function normalizeConfig(data, label = "units") {
  if (!data || typeof data !== "object" || data.version !== 1) {
    throw new Error(`${label}: expected a version: 1 mapping`);
  }
  const units = (data.units || []).map((raw) => {
    const bazel = raw.bazel || {};
    const go = raw.go || {};
    const python = raw.python || {};
    const js = raw.js || {};
    const native = raw.native || {};
    const nativeToolchain = native.toolchain || {};
    const deploy = raw.deploy || {};
    return new Unit({
      id: raw.id,
      platform: raw.platform,
      title: raw.title ?? raw.id,
      paths: list(raw.paths),
      bazelRoots: list(bazel.roots),
      testTargets: list(bazel.test_targets),
      buildTargets: list(bazel.build_targets),
      artifact: bazel.artifact ?? "",
      artifactName: bazel.artifact_name ?? "",
      bazelSdkPackages: list(bazel.sdk_packages),
      goModule: go.module ?? ".",
      goPackages: list(go.packages),
      goBuild: list(go.build),
      goRace: Boolean(go.race),
      suites: list(python.suites),
      compileDirs: list(python.compile),
      // The suite drives a deploy script that execs the Go deploy binary; the
      // workflow builds it once and exports HOME_DEPLOY.
      pythonDeployTool: Boolean(python.deploy_tool),
      jsTests: list(js.tests),
      nativeWorkdir: String(native.workdir ?? ""),
      nativeBuild: String(native.build ?? ""),
      nativeTest: String(native.test ?? ""),
      nativeArtifacts: list(native.artifacts),
      nativeChecksums: String(native.checksums ?? ""),
      nativeToolchainName: String(nativeToolchain.name ?? ""),
      nativeToolchainVersion: String(nativeToolchain.version ?? ""),
      nativeToolchainUrl: String(nativeToolchain.url ?? ""),
      nativeToolchainSha256: String(nativeToolchain.sha256 ?? ""),
      nativeToolchainRoot: String(nativeToolchain.root ?? ""),
      dependsOn: list(raw.depends_on),
      deployImages: list(deploy.images),
      deployWorkloads: list(deploy.workloads),
      deployBuilder: String(deploy.builder ?? "bazel"),
      deployTargetRoot: String(deploy.target_root ?? ""),
      deployNamespace: String(deploy.namespace ?? ""),
      deployScript: String(deploy.script ?? ""),
      deployEnvKey: String(deploy.env_key ?? ""),
      deployEnvFile: String(deploy.env_file ?? ""),
      deployExternalsecrets: list(deploy.externalsecrets),
    });
  });

  const invalidations = (data.invalidations || []).map((item) => {
    const deploy = item.deploy ?? true;
    if (typeof deploy !== "boolean") {
      throw new Error(
        `${label}: invalidation ${JSON.stringify(item.paths)}: deploy must be a bool`,
      );
    }
    return new Invalidation({
      paths: list(item.paths),
      platforms: list(item.platforms),
      deploy,
    });
  });

  const deployControl = data.deploy_control || {};
  return new Config({
    units,
    invalidations,
    deployControlUnits: list(deployControl.units),
    deployControlPaths: list(deployControl.paths),
  });
}

export function loadConfig(source = unitModule) {
  if (typeof source === "string") {
    throw new Error(
      "loadConfig takes a module object; use loadConfigFile(path) for a path",
    );
  }
  return normalizeConfig(source, "units");
}

export async function loadConfigFile(file) {
  const module = await import(pathToFileURL(path.resolve(file)).href);
  return normalizeConfig(module.default ?? module.config, file);
}

// --------------------------------------------------------------------------
// Path matching
// --------------------------------------------------------------------------
function globToRegex(pattern) {
  const out = [];
  let index = 0;
  while (index < pattern.length) {
    if (pattern.startsWith("**/", index)) {
      out.push("(?:.*/)?");
      index += 3;
      continue;
    }
    if (pattern.startsWith("**", index)) {
      out.push(".*");
      index += 2;
      continue;
    }
    const char = pattern[index];
    if (char === "*") {
      out.push("[^/]*");
    } else if (char === "?") {
      out.push("[^/]");
    } else {
      out.push(char.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
    }
    index += 1;
  }
  return new RegExp(`^${out.join("")}$`);
}

const GLOB_CACHE = new Map();

export function pathMatches(filePath, patterns) {
  for (const pattern of patterns) {
    let regex = GLOB_CACHE.get(pattern);
    if (regex === undefined) {
      regex = globToRegex(pattern);
      GLOB_CACHE.set(pattern, regex);
    }
    if (regex.test(filePath)) {
      return true;
    }
  }
  return false;
}

// --------------------------------------------------------------------------
// Git
// --------------------------------------------------------------------------
function run(command, args, options = {}) {
  return spawnSync(command, args, {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    ...options,
  });
}

function git(root, ...args) {
  const result = run("git", ["-C", String(root), ...args]);
  if (result.error || result.status !== 0) {
    const stderr = (result.stderr || result.error?.message || "").trim();
    throw new Error(`git ${args.join(" ")} failed: ${stderr}`);
  }
  return result.stdout;
}

export function changedFiles(root, base, head) {
  // Files touched between base and head, renames expanded to delete+add.
  const out = git(root, "diff", "--name-only", "--no-renames", `${base}...${head}`);
  return [...new Set(out.split("\n").map((line) => line.trim()).filter(Boolean))].sort();
}

export function gitFileText(root, revision, filePath) {
  // Content of `filePath` at `revision`, or null when it does not exist there.
  const result = run("git", ["-C", String(root), "show", `${revision}:${filePath}`]);
  if (result.error || result.status !== 0) {
    return null;
  }
  return result.stdout;
}

export function resolveBase(root, requested) {
  // Pick a base revision, or null when the whole tree must run.
  if (requested) {
    if (/^0+$/.test(requested)) {
      return null;
    }
    try {
      return git(root, "rev-parse", "--verify", `${requested}^{commit}`).trim();
    } catch {
      // fall through to the develop merge-base
    }
  }
  for (const candidate of ["origin/develop", "develop"]) {
    try {
      const mergeBase = git(root, "merge-base", "HEAD", candidate).trim();
      if (mergeBase) {
        return mergeBase;
      }
    } catch {
      continue;
    }
  }
  return null;
}

// --------------------------------------------------------------------------
// MODULE.bazel scoping
// --------------------------------------------------------------------------
export const MODULE_BAZEL = "MODULE.bazel";
// Virtual changed path for a MODULE.bazel edit that only touches the
// rules_jvm_external Maven extension. `units.mjs` maps it to the android
// platform, so the Maven/APK closure is retested without invalidating Go,
// native, Python or JS units.
export const MODULE_BAZEL_MAVEN = "MODULE.bazel#maven";
export const MAVEN_EXTENSION = "@rules_jvm_external//:extensions.bzl";

const IDENT = /[\p{L}\p{N}_]/u;

function tokens(text) {
  // Small Starlark-aware tokenizer: comments, strings and punctuation.
  //
  // Only what is needed to compare MODULE.bazel files structurally. Strings
  // are kept as single tokens, so a `#`, `(` or `)` inside one cannot start a
  // comment or change the parenthesis nesting.
  const out = [];
  let index = 0;
  const length = text.length;
  while (index < length) {
    const char = text[index];
    if (char === " " || char === "\t" || char === "\r" || char === "\n") {
      index += 1;
    } else if (char === "#") {
      while (index < length && text[index] !== "\n") {
        index += 1;
      }
    } else if (text.startsWith("'''", index) || text.startsWith('"""', index)) {
      const quote = text.slice(index, index + 3);
      let end = index + 3;
      while (end < length) {
        if (text[end] === "\\") {
          end += 2;
          continue;
        }
        if (text.startsWith(quote, end)) {
          end += 3;
          break;
        }
        end += 1;
      }
      out.push(["string", text.slice(index, end)]);
      index = end;
    } else if (char === "'" || char === '"') {
      let end = index + 1;
      while (end < length) {
        if (text[end] === "\\") {
          end += 2;
          continue;
        }
        if (text[end] === char) {
          end += 1;
          break;
        }
        end += 1;
      }
      out.push(["string", text.slice(index, end)]);
      index = end;
    } else if (IDENT.test(char)) {
      let end = index;
      while (end < length && IDENT.test(text[end])) {
        end += 1;
      }
      out.push(["ident", text.slice(index, end)]);
      index = end;
    } else {
      out.push(["punct", char]);
      index += 1;
    }
  }
  return out;
}

function balanced(list) {
  const pairs = { ")": "(", "]": "[", "}": "{" };
  const stack = [];
  for (const [kind, value] of list) {
    if (kind !== "punct") {
      continue;
    }
    if (value === "(" || value === "[" || value === "{") {
      stack.push(value);
    } else if (Object.hasOwn(pairs, value)) {
      if (!stack.length || stack.pop() !== pairs[value]) {
        return false;
      }
    }
  }
  return stack.length === 0;
}

function matchParen(list, openIndex) {
  let depth = 0;
  for (let index = openIndex; index < list.length; index += 1) {
    if (list[index][0] === "punct" && list[index][1] === "(") {
      depth += 1;
    } else if (list[index][0] === "punct" && list[index][1] === ")") {
      depth -= 1;
      if (depth === 0) {
        return index;
      }
    }
  }
  return null;
}

function stringValue(token) {
  const [kind, value] = token;
  if (kind !== "string" || value.length < 2) {
    return null;
  }
  for (const quote of ["'''", '"""', "'", '"']) {
    if (value.startsWith(quote) && value.endsWith(quote)) {
      return value.slice(quote.length, value.length - quote.length);
    }
  }
  return null;
}

function mavenProxy(list) {
  // Name bound by `<name> = use_extension(MAVEN_EXTENSION, "maven")`.
  for (let index = 0; index < list.length - 3; index += 1) {
    if (list[index][0] !== "ident") {
      continue;
    }
    if (
      !(list[index + 1][0] === "punct" && list[index + 1][1] === "=") ||
      !(list[index + 2][0] === "ident" && list[index + 2][1] === "use_extension") ||
      !(list[index + 3][0] === "punct" && list[index + 3][1] === "(")
    ) {
      continue;
    }
    const end = matchParen(list, index + 3);
    if (end === null) {
      continue;
    }
    const values = new Set(
      list
        .slice(index + 4, end)
        .filter((token) => token[0] === "string")
        .map((token) => stringValue(token)),
    );
    if (values.has(MAVEN_EXTENSION) && values.has("maven")) {
      return list[index][1];
    }
  }
  return null;
}

function stripMaven(list, proxy) {
  // Drop `<proxy>.<tag>(...)` calls and `use_repo(<proxy>, ...)`.
  const rest = [];
  let index = 0;
  while (index < list.length) {
    if (
      list[index][0] === "ident" &&
      list[index][1] === proxy &&
      index + 3 < list.length &&
      list[index + 1][0] === "punct" &&
      list[index + 1][1] === "." &&
      list[index + 2][0] === "ident" &&
      list[index + 3][0] === "punct" &&
      list[index + 3][1] === "("
    ) {
      const end = matchParen(list, index + 3);
      if (end !== null) {
        index = end + 1;
        continue;
      }
    }
    if (
      list[index][0] === "ident" &&
      list[index][1] === "use_repo" &&
      index + 2 < list.length &&
      list[index + 1][0] === "punct" &&
      list[index + 1][1] === "(" &&
      list[index + 2][0] === "ident" &&
      list[index + 2][1] === proxy
    ) {
      const end = matchParen(list, index + 1);
      if (end !== null) {
        index = end + 1;
        continue;
      }
    }
    rest.push(list[index]);
    index += 1;
  }
  return rest;
}

function sameTokens(left, right) {
  if (left.length !== right.length) {
    return false;
  }
  return left.every(
    (token, index) => token[0] === right[index][0] && token[1] === right[index][1],
  );
}

export function moduleBazelIsMavenOnly(oldText, newText) {
  /**
   * Whether the only real change is inside the rules_jvm_external Maven use.
   *
   * Comments and whitespace never count, and neither does anything inside
   * `maven.install(...)`/`maven.<tag>(...)` or `use_repo(maven, ...)` where
   * `maven` is the proxy bound by `use_extension("@rules_jvm_external//:
   * extensions.bzl", "maven")`.
   *
   * Everything else is deliberately *not* scoped: a `bazel_dep`, toolchain,
   * `go_deps`, `oci` or Python change keeps the global invalidation. A missing
   * proxy binding or unbalanced input also keeps it: the fail-safe direction is
   * "run everything".
   */
  const oldTokens = tokens(oldText);
  const newTokens = tokens(newText);
  if (!balanced(oldTokens) || !balanced(newTokens)) {
    return false;
  }
  const oldProxy = mavenProxy(oldTokens);
  const newProxy = mavenProxy(newTokens);
  if (oldProxy === null || newProxy === null) {
    return false;
  }
  return sameTokens(stripMaven(oldTokens, oldProxy), stripMaven(newTokens, newProxy));
}

function scopeModuleBazel(changed, root, base, head) {
  // Replace a Maven-only `MODULE.bazel` edit with the virtual android path.
  if (!changed.includes(MODULE_BAZEL) || !base) {
    return { changed, warning: null };
  }
  const oldText = internals.gitFileText(root, base, MODULE_BAZEL);
  const newText = internals.gitFileText(root, head, MODULE_BAZEL);
  // Added/deleted MODULE.bazel (or an unknown revision): keep it global.
  if (oldText === null || newText === null) {
    return { changed, warning: null };
  }
  if (!moduleBazelIsMavenOnly(oldText, newText)) {
    return { changed, warning: null };
  }
  const scoped = changed.map((item) => (item === MODULE_BAZEL ? MODULE_BAZEL_MAVEN : item));
  return {
    changed: scoped,
    warning:
      "MODULE.bazel changed only inside the rules_jvm_external Maven extension; " +
      "scoping it to the android platform",
  };
}

// --------------------------------------------------------------------------
// Go dependency closure
// --------------------------------------------------------------------------
export function goPackageDirs(root, unit) {
  // Absolute directories of every package the unit compiles or imports.
  const moduleDir = realpath(path.resolve(root, unit.goModule));
  const result = run("go", ["list", "-deps", "-f", "{{.Dir}}", ...unit.goPackages], {
    cwd: moduleDir,
  });
  if (result.error || result.status !== 0) {
    const stderrLines = (result.stderr || result.error?.message || "").trim().split("\n");
    const tail = stderrLines.slice(-1)[0] || "";
    throw new Error(`go list failed for unit ${unit.id}: ${tail}`);
  }
  return new Set(result.stdout.split("\n").map((line) => line.trim()).filter(Boolean));
}

// --------------------------------------------------------------------------
// Bazel loading graph
// --------------------------------------------------------------------------
export const BAZEL_GRAPH_QUERY = 'kind(".*", //...)';

function unescapeXml(value) {
  return value
    .replaceAll("&quot;", '"')
    .replaceAll("&apos;", "'")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&amp;", "&");
}

function attributes(text) {
  const out = {};
  const pattern = /([A-Za-z_][\w.-]*)\s*=\s*"([^"]*)"/g;
  let match;
  while ((match = pattern.exec(text)) !== null) {
    out[match[1]] = unescapeXml(match[2]);
  }
  return out;
}

export class BazelGraph {
  /**
   * Rule dependency graph from `bazel query --output=xml`.
   *
   * Only loading-phase information is used, so the query works without a JDK
   * or the Android SDK.
   */
  constructor({ rules = {}, inputs = {}, packageRules = {}, pathIndex = null } = {}) {
    this.rules = rules;
    this.inputs = inputs;
    this.packageRules = packageRules;
    if (pathIndex) {
      this.pathIndex = pathIndex;
    } else {
      const index = new Map();
      const add = (key, label) => {
        if (!index.has(key)) {
          index.set(key, new Set());
        }
        index.get(key).add(label);
      };
      for (const [label, deps] of Object.entries(this.inputs)) {
        add(BazelGraph.labelPath(label), label);
        for (const dep of deps) {
          add(BazelGraph.labelPath(dep), dep);
        }
      }
      this.pathIndex = index;
    }
  }

  static fromXml(text) {
    if (!/<query\b/.test(text)) {
      throw new Error("unexpected bazel query XML root");
    }
    const rules = {};
    const inputs = {};
    const packageRules = {};
    const rulePattern = /<rule\b([^>]*?)\/>|<rule\b([^>]*)>([\s\S]*?)<\/rule>/g;
    let ruleMatch;
    while ((ruleMatch = rulePattern.exec(text)) !== null) {
      const attrs = attributes(ruleMatch[1] ?? ruleMatch[2]);
      const label = attrs.name;
      if (!label) {
        continue;
      }
      rules[label] = attrs.class ?? "";
      const deps = new Set();
      const body = ruleMatch[3] ?? "";
      const inputPattern = /<rule-input\b([^>]*)\/?>/g;
      let inputMatch;
      while ((inputMatch = inputPattern.exec(body)) !== null) {
        const name = attributes(inputMatch[1]).name;
        if (name) {
          deps.add(name);
        }
      }
      inputs[label] = deps;
      const colon = label.lastIndexOf(":");
      const pkg = colon === -1 ? label : label.slice(0, colon);
      if (!packageRules[pkg]) {
        packageRules[pkg] = new Set();
      }
      packageRules[pkg].add(label);
    }
    return new BazelGraph({ rules, inputs, packageRules });
  }

  dependents() {
    const reverse = new Map();
    const add = (key, label) => {
      if (!reverse.has(key)) {
        reverse.set(key, new Set());
      }
      reverse.get(key).add(label);
    };
    for (const [label, deps] of Object.entries(this.inputs)) {
      for (const dep of deps) {
        add(dep, label);
      }
    }
    return reverse;
  }

  static labelPath(label) {
    // `//pkg:sub/file.kt` -> `pkg/sub/file.kt` (repo-relative).
    if (!label.startsWith("//")) {
      return label;
    }
    return label.slice(2).replace(":", "/");
  }

  labelsForPath(filePath) {
    return this.pathIndex.get(filePath) ?? new Set();
  }

  packageForPath(filePath) {
    // Nearest Bazel package (ancestor directory with a BUILD file).
    let pkg = filePath.slice(0, Math.max(0, filePath.lastIndexOf("/")));
    for (;;) {
      const key = pkg ? `//${pkg}` : "//";
      if (Object.hasOwn(this.packageRules, key)) {
        return key;
      }
      if (!pkg) {
        return "//";
      }
      pkg = pkg.slice(0, Math.max(0, pkg.lastIndexOf("/")));
    }
  }

  affectedRules(changed) {
    const reverse = this.dependents();
    const seeds = new Set();
    for (const filePath of changed) {
      const labels = this.labelsForPath(filePath);
      if (labels.size) {
        for (const label of labels) {
          seeds.add(label);
        }
      } else {
        // Deleted file, new file or BUILD/.bzl change: fall back to the
        // whole package, which over-approximates safely.
        for (const label of this.packageRules[this.packageForPath(filePath)] ?? []) {
          seeds.add(label);
        }
      }
    }
    const affected = new Set();
    const stack = [...seeds];
    while (stack.length) {
      const node = stack.pop();
      if (affected.has(node)) {
        continue;
      }
      affected.add(node);
      for (const dependent of reverse.get(node) ?? []) {
        stack.push(dependent);
      }
    }
    return new Set([...affected].filter((label) => Object.hasOwn(this.rules, label)));
  }
}

function which(command) {
  if (command.includes(path.sep) || command.includes("/")) {
    return fs.existsSync(command) ? command : null;
  }
  const pathEntries = (process.env.PATH || "").split(path.delimiter);
  for (const directory of pathEntries) {
    const candidate = path.join(directory, command);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return candidate;
    } catch {
      continue;
    }
  }
  return null;
}

export function loadBazelGraph(root, bazel, timeout = 300) {
  if (!which(bazel) && !fs.existsSync(bazel)) {
    throw new Error(`bazel binary not found: ${bazel}`);
  }
  const result = run(bazel, ["query", "--output=xml", "--noshow_progress", BAZEL_GRAPH_QUERY], {
    cwd: root,
    timeout: timeout * 1000,
  });
  if (result.error || result.status !== 0) {
    const tail = (result.stderr || result.error?.message || "").trim().split("\n").slice(-3).join(" ");
    throw new Error(`bazel query failed: ${tail}`);
  }
  return BazelGraph.fromXml(result.stdout);
}

// --------------------------------------------------------------------------
// Routing
// --------------------------------------------------------------------------
class PlatformRoute {
  constructor(platform, mode, units = [], warnings = []) {
    this.platform = platform;
    this.mode = mode;
    this.units = units;
    this.warnings = warnings;
  }
}

class Route {
  constructor({
    base,
    head,
    changed,
    routes,
    docsOnly,
    deployControl,
    deploySelected = new Set(),
    androidDistribute = false,
    warnings = [],
  }) {
    this.base = base;
    this.head = head;
    this.changed = changed;
    this.routes = routes;
    this.docsOnly = docsOnly;
    this.deployControl = deployControl;
    // Units selected for deploy-home, which ignores `deploy: false`
    // invalidations. CI routing (`routes`) keeps using every invalidation.
    this.deploySelected = deploySelected;
    // True only when an Android application input actually changed. A global
    // invalidation (MODULE.bazel) or a CI-harness edit still selects the android
    // units for tests, but must never publish an APK.
    this.androidDistribute = androidDistribute;
    this.warnings = warnings;
  }
}

export const GLOBAL = "*";

export function androidDistributePaths(config) {
  /**
   * Paths whose change means the Android application itself changed.
   *
   * Android unit paths cover the application and the shared Kotlin libraries it
   * builds; deploy-relevant Android invalidations cover the platform build
   * tooling (for example `platform/android/**`). A global invalidation such as
   * `MODULE.bazel`, or a CI-harness edit (`deploy: false`), still selects the
   * Android units for their checks but must never publish an APK: the checks
   * prove the change is safe, not that the app changed.
   */
  const patterns = config.byPlatform("android").flatMap((unit) => unit.paths);
  for (const rule of config.invalidations) {
    if (!rule.deploy) {
      continue;
    }
    const platforms = rule.platforms
      .join(",")
      .split(",")
      .map((part) => part.trim());
    if (platforms.includes("android")) {
      patterns.push(...rule.paths);
    }
  }
  return patterns;
}

export function androidDistribution(config, changed) {
  /**
   * Whether a changed path is an actual Android application input.
   *
   * Markdown is documentation even inside the app tree: it never reaches the
   * APK, so a README edit must not publish a build.
   */
  const inputs = changed.filter((item) => !item.toLowerCase().endsWith(".md"));
  if (!inputs.length) {
    return false;
  }
  const patterns = androidDistributePaths(config);
  return inputs.some((item) => pathMatches(item, patterns));
}

function invalidatedPlatforms(config, changed, deployOnly = false) {
  const hit = new Set();
  for (const rule of config.invalidations) {
    if (deployOnly && !rule.deploy) {
      continue;
    }
    if (changed.some((item) => pathMatches(item, rule.paths))) {
      for (const platform of rule.platforms) {
        if (platform === GLOBAL) {
          for (const name of PLATFORMS) {
            hit.add(name);
          }
        } else {
          for (const part of platform.split(",")) {
            if (part.trim()) {
              hit.add(part.trim());
            }
          }
        }
      }
    }
  }
  return hit;
}

function expandDependsOn(config, selected) {
  const byId = new Map(config.units.map((unit) => [unit.id, unit]));
  const seen = new Set();
  const stack = [...selected];
  while (stack.length) {
    const unit = stack.pop();
    if (seen.has(unit.id)) {
      continue;
    }
    seen.add(unit.id);
    for (const dep of unit.dependsOn) {
      if (byId.has(dep)) {
        stack.push(byId.get(dep));
      }
    }
  }
  return config.units.filter((unit) => seen.has(unit.id));
}

function realpath(target) {
  try {
    return fs.realpathSync(target);
  } catch {
    return path.resolve(target);
  }
}

function routePlatforms(config, changed, root, invalidated, graph, graphLoader, warnings) {
  // Select units per platform for the given invalidation set.
  const changedDirs = new Set(
    changed.filter(Boolean).map((item) => realpath(path.resolve(root, path.dirname(item)))),
  );
  const routes = {};

  // --- go: dependency closure from the Go toolchain ---------------------
  const goRoute = new PlatformRoute("go", "go-list");
  const goUnits = config.byPlatform("go");
  const goPatterns = goUnits.flatMap((unit) => unit.paths);
  const goDirty =
    invalidated.has(GLOBAL) ||
    invalidated.has("go") ||
    changed.some(
      (item) =>
        item.endsWith(".go") ||
        item.endsWith("go.mod") ||
        item.endsWith("go.sum") ||
        item.endsWith("go.work") ||
        pathMatches(item, goPatterns),
    );
  if (!goDirty) {
    goRoute.mode = "paths";
  } else if (invalidated.has(GLOBAL) || invalidated.has("go")) {
    goRoute.mode = "all";
    goRoute.units = [...goUnits];
  } else {
    for (const unit of goUnits) {
      const matchedPaths = changed.some((item) => pathMatches(item, unit.paths));
      let deps;
      try {
        deps = internals.goPackageDirs(root, unit);
      } catch (error) {
        goRoute.mode = "all";
        goRoute.units = [...config.byPlatform("go")];
        goRoute.warnings.push(`go list failed (${error.message}); running every go unit`);
        break;
      }
      if (matchedPaths || [...changedDirs].some((directory) => deps.has(directory))) {
        goRoute.units.push(unit);
      }
    }
  }
  goRoute.units = expandDependsOn(config, goRoute.units);
  routes.go = goRoute;

  // --- android: Bazel reverse dependencies ------------------------------
  const androidRoute = new PlatformRoute("android", "bazel-graph");
  const androidUnits = config.byPlatform("android");
  if (invalidated.has(GLOBAL) || invalidated.has("android")) {
    androidRoute.mode = "all";
    androidRoute.units = [...androidUnits];
  } else {
    const touched = changed.filter((item) =>
      androidUnits.some((unit) => pathMatches(item, unit.paths)),
    );
    if (!touched.length) {
      androidRoute.mode = "paths";
    } else {
      try {
        if (graph === null || graph === undefined) {
          if (!graphLoader) {
            throw new Error("no graph loader configured");
          }
          graph = graphLoader();
        }
        const affected = graph.affectedRules(changed);
        for (const unit of androidUnits) {
          if (
            changed.some((item) => pathMatches(item, unit.paths)) ||
            [...affected].some((label) =>
              unit.bazelRoots.some(
                (rootLabel) =>
                  label.startsWith(`${rootLabel}:`) || label.startsWith(`${rootLabel}/`),
              ),
            )
          ) {
            androidRoute.units.push(unit);
          }
        }
      } catch (error) {
        androidRoute.mode = "all";
        androidRoute.units = [...androidUnits];
        androidRoute.warnings.push(
          `bazel graph failed (${error.message}); running every android unit`,
        );
      }
    }
  }
  androidRoute.units = expandDependsOn(config, androidRoute.units);
  routes.android = androidRoute;

  // --- python: declarative path rules ----------------------------------
  const pythonRoute = new PlatformRoute("python", "paths");
  for (const unit of config.byPlatform("python")) {
    if (invalidated.has(GLOBAL) || invalidated.has("python")) {
      pythonRoute.units.push(unit);
      pythonRoute.mode = "all";
    } else if (changed.some((item) => pathMatches(item, unit.paths))) {
      pythonRoute.units.push(unit);
    }
  }
  pythonRoute.units = expandDependsOn(config, pythonRoute.units);
  routes.python = pythonRoute;

  // --- js: declarative path rules for dependency-free edge Workers ------
  const jsRoute = new PlatformRoute("js", "paths");
  for (const unit of config.byPlatform("js")) {
    if (invalidated.has(GLOBAL) || invalidated.has("js")) {
      jsRoute.units.push(unit);
      jsRoute.mode = "all";
    } else if (changed.some((item) => pathMatches(item, unit.paths))) {
      jsRoute.units.push(unit);
    }
  }
  jsRoute.units = expandDependsOn(config, jsRoute.units);
  routes.js = jsRoute;

  // --- native: declarative path rules for non-Bazel C/C++ builds --------
  const nativeRoute = new PlatformRoute("native", "paths");
  for (const unit of config.byPlatform("native")) {
    if (invalidated.has(GLOBAL) || invalidated.has("native")) {
      nativeRoute.units.push(unit);
      nativeRoute.mode = "all";
    } else if (changed.some((item) => pathMatches(item, unit.paths))) {
      nativeRoute.units.push(unit);
    }
  }
  nativeRoute.units = expandDependsOn(config, nativeRoute.units);
  routes.native = nativeRoute;

  // --- quality: always --------------------------------------------------
  routes.quality = new PlatformRoute("quality", "always", [...config.byPlatform("quality")]);

  return routes;
}

export function route(
  config,
  changed,
  {
    root,
    base,
    head = "HEAD",
    bazel = "bazel",
    graph = null,
    graphLoader = null,
  } = {},
) {
  // Route changed paths onto units. Exposed for tests.
  const scoped = scopeModuleBazel(changed, root, base, head);
  changed = scoped.changed;
  const invalidated = invalidatedPlatforms(config, changed);
  const warnings = scoped.warning ? [scoped.warning] : [];
  const routes = routePlatforms(
    config,
    changed,
    root,
    invalidated,
    graph,
    graphLoader,
    warnings,
  );

  // Deploy selection ignores `deploy: false` invalidations. CI keeps running
  // the invalidated jobs (routes above), but a workflow/harness-only change
  // must not roll every deployable unit after merge. When a deploy-relevant
  // invalidation covers the same platforms, the CI selection is already the
  // deploy selection and no second pass is needed.
  const deployInvalidated = invalidatedPlatforms(config, changed, true);
  let deploySelected;
  const samePlatforms =
    deployInvalidated.size === invalidated.size &&
    [...deployInvalidated].every((platform) => invalidated.has(platform));
  if (samePlatforms) {
    deploySelected = new Set(
      Object.values(routes).flatMap((platformRoute) =>
        platformRoute.units.map((unit) => unit.id),
      ),
    );
  } else {
    const deployRoutes = routePlatforms(
      config,
      changed,
      root,
      deployInvalidated,
      graph,
      graphLoader,
      warnings,
    );
    deploySelected = new Set(
      Object.values(deployRoutes).flatMap((platformRoute) =>
        platformRoute.units.map((unit) => unit.id),
      ),
    );
  }

  const docsOnly =
    Boolean(changed.length) &&
    changed.every((item) => item.endsWith(".md") || item.endsWith(".mdx") || item.startsWith("docs/"));
  const affectedGo = new Set(routes.go.units.map((unit) => unit.id));
  const deployControl =
    config.deployControlUnits.some((unit) => affectedGo.has(unit)) ||
    changed.some((item) => pathMatches(item, config.deployControlPaths));

  for (const platformRoute of Object.values(routes)) {
    warnings.push(...platformRoute.warnings);
  }
  return new Route({
    base,
    head,
    changed,
    routes,
    docsOnly,
    deployControl,
    deploySelected,
    androidDistribute: androidDistribution(config, changed),
    warnings,
  });
}

function space(items) {
  return items.join(" ");
}

// Python's json.dumps default: `", "` and `": "` separators. The GitHub outputs
// used to be produced by Python, so keep the exact byte format.
export function jsonDumps(value) {
  if (value === null) {
    return "null";
  }
  if (Array.isArray(value)) {
    return `[${value.map((item) => jsonDumps(item)).join(", ")}]`;
  }
  if (typeof value === "object") {
    return `{${Object.entries(value)
      .map(([key, item]) => `${JSON.stringify(key)}: ${jsonDumps(item)}`)
      .join(", ")}}`;
  }
  return JSON.stringify(value);
}

export function matrixEntries(route_, config) {
  // Matrix payloads for the reusable platform workflows.
  const go = route_.routes.go.units.map((unit) => ({
    id: unit.id,
    workdir: unit.goModule,
    packages: space(unit.goPackages),
    race: unit.goRace,
    build_packages: space(unit.goBuild),
  }));
  // Every android/kotlin target so the precise target step can see shared
  // libraries that belong to another unit than the application it builds.
  const androidRoots = [
    ...new Set(config.byPlatform("android").flatMap((unit) => unit.bazelRoots)),
  ].sort();
  const universe = androidRoots.map((root) => `${root}/...`).join(" + ");
  const android = route_.routes.android.units.map((unit) => ({
    id: unit.id,
    test_targets: space(unit.testTargets),
    build_targets: space(unit.buildTargets),
    artifact: unit.artifact,
    artifact_name: unit.artifactName,
    sdk_packages: space(unit.bazelSdkPackages),
    universe,
  }));
  const python = route_.routes.python.units.map((unit) => ({
    id: unit.id,
    suites: space(unit.suites),
    compile: space(unit.compileDirs),
    deploy_tool: unit.pythonDeployTool,
  }));
  const js = route_.routes.js.units.map((unit) => ({
    id: unit.id,
    tests: space(unit.jsTests),
  }));
  const native = route_.routes.native.units.map((unit) => ({
    id: unit.id,
    workdir: unit.nativeWorkdir,
    build: unit.nativeBuild,
    test: unit.nativeTest,
    artifacts: space(unit.nativeArtifacts),
    checksums: unit.nativeChecksums,
    toolchain_name: unit.nativeToolchainName,
    toolchain_version: unit.nativeToolchainVersion,
    toolchain_url: unit.nativeToolchainUrl,
    toolchain_sha256: unit.nativeToolchainSha256,
    toolchain_root: unit.nativeToolchainRoot,
  }));
  return { go, android, python, js, native };
}

export function deployEntries(route_, config) {
  /**
   * Deployable units selected by the same routing as CI.
   *
   * A shared Go package change reaches every importer, so it also deploys every
   * importer that declares a `deploy` block. Units without one (documentation,
   * infrastructure, observability) never appear here, so `deploy-home` cannot
   * roll them by accident.
   */
  const selected = route_.deploySelected;
  const entries = [];
  for (const unit of config.units) {
    if (!unit.deployImages.length || !selected.has(unit.id)) {
      continue;
    }
    entries.push({
      id: unit.id,
      images: space(unit.deployImages),
      workloads: space(unit.deployWorkloads),
      builder: unit.deployBuilder,
      target_root: unit.deployTargetRoot,
      namespace: unit.deployNamespace,
      script: unit.deployScript,
      env_key: unit.deployEnvKey,
      env_file: unit.deployEnvFile,
      externalsecrets: space(unit.deployExternalsecrets),
    });
  }
  return entries;
}

export function summaryMarkdown(route_, config) {
  const lines = [
    `Base: \`${route_.base || "(none - full run)"}\` -> head \`${route_.head}\``,
    `Changed files: ${route_.changed.length}`,
    "",
    "| Platform | Mode | Units |",
    "|---|---|---|",
  ];
  for (const platform of PLATFORMS) {
    const platformRoute = route_.routes[platform];
    const ids = platformRoute.units.map((unit) => `\`${unit.id}\``).join(", ") || "-";
    lines.push(`| ${platform} | ${platformRoute.mode} | ${ids} |`);
  }
  lines.push("");
  lines.push(`Docs only: \`${route_.docsOnly}\``);
  lines.push(`Deploy control regression: \`${route_.deployControl}\``);
  lines.push(`Android distribute: \`${route_.androidDistribute}\``);
  const deployed = config.units
    .filter((unit) => unit.deployImages.length && route_.deploySelected.has(unit.id))
    .map((unit) => unit.id)
    .sort();
  lines.push(`Deploy units: ${deployed.map((unit) => `\`${unit}\``).join(", ") || "-"}`);
  if (route_.warnings.length) {
    lines.push("");
    lines.push("Warnings:");
    lines.push(...route_.warnings.map((warning) => `- ${warning}`));
  }
  return lines.join("\n");
}

export function writeGithubOutput(filePath, values) {
  const handle = fs.openSync(filePath, "a");
  try {
    for (const [key, value] of Object.entries(values)) {
      const text = String(value);
      if (text.includes("\n")) {
        fs.writeSync(handle, `${key}<<EOF\n${text}\nEOF\n`);
      } else {
        fs.writeSync(handle, `${key}=${text}\n`);
      }
    }
  } finally {
    fs.closeSync(handle);
  }
}

function parseArgs(argv) {
  const args = {
    root: ".",
    units: DEFAULT_UNITS,
    base: null,
    head: "HEAD",
    bazel: process.env.BAZEL || "bazel",
    graphFile: null,
    changedFile: [],
    format: "text",
    out: null,
    all: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const take = () => {
      index += 1;
      if (index >= argv.length) {
        throw new Error(`missing value for ${arg}`);
      }
      return argv[index];
    };
    switch (arg) {
      case "--root":
        args.root = take();
        break;
      case "--units":
        args.units = take();
        break;
      case "--base":
        args.base = take();
        break;
      case "--head":
        args.head = take();
        break;
      case "--bazel":
        args.bazel = take();
        break;
      case "--graph-file":
        args.graphFile = take();
        break;
      case "--changed-file":
        args.changedFile.push(take());
        break;
      case "--format":
        args.format = take();
        break;
      case "--out":
        args.out = take();
        break;
      case "--all":
        args.all = true;
        break;
      case "-h":
      case "--help":
        args.help = true;
        break;
      default:
        throw new Error(`unrecognized argument: ${arg}`);
    }
  }
  if (!["text", "json", "github"].includes(args.format)) {
    throw new Error(`invalid --format: ${args.format}`);
  }
  return args;
}

export async function compute(argv) {
  const args = parseArgs(argv);
  const root = path.resolve(args.root);
  const config =
    args.units === DEFAULT_UNITS ? loadConfig() : await loadConfigFile(args.units);
  const base = args.all ? null : internals.resolveBase(root, args.base);

  let changed;
  if (args.changedFile.length) {
    changed = [...args.changedFile].sort();
  } else if (base) {
    changed = internals.changedFiles(root, base, args.head);
  } else {
    changed = [];
  }

  let graph = null;
  let loader = null;
  if (args.graphFile) {
    graph = BazelGraph.fromXml(fs.readFileSync(args.graphFile, "utf8"));
  } else {
    loader = () => loadBazelGraph(root, args.bazel);
  }

  const result = internals.route(config, changed, {
    root,
    base,
    head: args.head,
    bazel: args.bazel,
    graph,
    graphLoader: loader,
  });

  // No base revision (workflow_dispatch, unknown before-SHA): run everything.
  if ((base === null && !args.changedFile.length) || args.all) {
    for (const platform of PLATFORMS) {
      const platformRoute = result.routes[platform];
      if (platform === "quality") {
        continue;
      }
      platformRoute.mode = "all";
      platformRoute.units = [...config.byPlatform(platform)];
    }
    result.deploySelected = new Set(config.units.map((unit) => unit.id));
    result.deployControl = true;
    result.changed = [];
    result.warnings.push("no base revision resolved; running every unit");
  }

  const entries = matrixEntries(result, config);
  return { args, config, result, entries };
}

export function githubOutputs(computed) {
  const { result, entries, config } = computed;
  return {
    base_sha: result.base || "",
    go_units: jsonDumps(entries.go),
    android_units: jsonDumps(entries.android),
    python_units: jsonDumps(entries.python),
    js_units: jsonDumps(entries.js),
    native_units: jsonDumps(entries.native),
    quality: "true",
    docs_only: String(result.docsOnly),
    deploy_control: String(result.deployControl),
    android_distribute: String(result.androidDistribute),
    deploy_units: jsonDumps(deployEntries(result, config)),
    summary: summaryMarkdown(result, config),
  };
}

export async function main(argv = process.argv.slice(2)) {
  const computed = await compute(argv);
  const { args, config, result, entries } = computed;

  if (args.format === "json") {
    console.log(
      JSON.stringify(
        {
          base: result.base,
          head: result.head,
          changed_files: result.changed,
          docs_only: result.docsOnly,
          deploy_control: result.deployControl,
          android_distribute: result.androidDistribute,
          deploy_units: deployEntries(result, config),
          warnings: result.warnings,
          modes: Object.fromEntries(
            Object.entries(result.routes).map(([name, platformRoute]) => [
              name,
              platformRoute.mode,
            ]),
          ),
          units: entries,
        },
        null,
        2,
      ),
    );
    return 0;
  }

  if (args.format === "github") {
    const outputs = githubOutputs(computed);
    const out = args.out || process.env.GITHUB_OUTPUT || "/dev/null";
    writeGithubOutput(out, outputs);
    console.log(outputs.summary);
    return 0;
  }

  console.log(summaryMarkdown(result, config));
  return 0;
}

/**
 * Entry point for `actions/github-script`: set step outputs with core.setOutput
 * and write the routing table with core.summary. No file or stdout plumbing.
 */
export async function runGithubScript({ core, argv = [] }) {
  const computed = await compute(argv);
  const outputs = githubOutputs(computed);
  for (const [key, value] of Object.entries(outputs)) {
    if (key === "summary") {
      continue;
    }
    core.setOutput(key, value);
  }
  await core.summary.addRaw(outputs.summary).write();
  return computed;
}

// Test seam: mutate these to simulate Git/toolchain failures without touching
// module bindings (ES modules are immutable from the outside).
export const internals = {
  goPackageDirs,
  gitFileText,
  resolveBase,
  changedFiles,
  route,
};

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
