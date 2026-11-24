#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";

import unitModule from "./units.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const DEFAULT_UNITS = path.join(HERE, "units.mjs");
export const PLATFORMS = ["android", "bazel", "python", "js", "native", "quality"];

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
        routingRoots: [],
        testTargets: [],
        buildTargets: [],
        artifact: "",
        artifactName: "",
        androidRelease: false,
        bazelSdkPackages: [],
        suites: [],
        compileDirs: [],
        pythonDeployTool: false,
        jsTests: [],
        nativeWorkdir: "",
        nativeKind: "script",
        nativePublish: true,
        nativePublishWhen: [],
        nativePublishScript: "",
        nativePublishAttestGlob: "",
        nativePublishRepository: "",
        nativePublishTags: [],
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
        deployImages: [],
        deployOptionalImages: [],
        deployWorkloads: [],
        deployBuilder: "bazel",
        deployTargetRoot: "",
        deployBazelImages: {},
        deployNamespace: "",
        rollbackKind: "",
        rollbackStateDir: "",
        rollbackNamespace: "",
        rollbackRelease: "",
        rollbackInstruction: "",
        rollbackNote: "",
        rollbackSource: "",
      },
      init,
    );
  }
}

export class Invalidation {
  constructor({ paths = [], platforms = [], deploy = true } = {}) {
    this.paths = paths;
    this.platforms = platforms;
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

function normalizeImageMap(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }
  const result = {};
  for (const [image, target] of Object.entries(value)) {
    result[String(image)] = String(target ?? "");
  }
  return result;
}

export function normalizeConfig(data, label = "units") {
  if (!data || typeof data !== "object" || data.version !== 1) {
    throw new Error(`${label}: expected a version: 1 mapping`);
  }
  const units = (data.units || []).map((raw) => {
    const bazel = raw.bazel || {};
    const python = raw.python || {};
    const js = raw.js || {};
    const native = raw.native || {};
    const nativeToolchain = native.toolchain || {};
    const deploy = raw.deploy || {};
    const rollback = raw.rollback || {};
    const bazelRoots = list(bazel.roots);
    const testTargets = list(bazel.test_targets);
    const routingRoots = list(bazel.routing_roots);
    if (!routingRoots.length) {
      const derived = bazelRoots.filter((root) => root !== "//packages/rust");
      for (const target of testTargets) {
        const match = /^\/\/(packages\/rust\/[^/:]+):([^/]+)$/.exec(target);
        if (match && !["clippy", "rustfmt"].includes(match[2])) {
          derived.push(`//${match[1]}`);
        }
      }
      routingRoots.push(...derived);
    }
    return new Unit({
      id: raw.id,
      platform: raw.platform,
      title: raw.title ?? raw.id,
      paths: list(raw.paths),
      bazelRoots,
      routingRoots: [...new Set(routingRoots)],
      testTargets,
      buildTargets: list(bazel.build_targets),
      artifact: bazel.artifact ?? "",
      artifactName: bazel.artifact_name ?? "",
      androidRelease: Boolean(bazel.release),
      bazelSdkPackages: list(bazel.sdk_packages),
      suites: list(python.suites),
      compileDirs: list(python.compile),
      pythonDeployTool: Boolean(python.deploy_tool),
      jsTests: list(js.tests),
      nativeWorkdir: String(native.workdir ?? ""),
      nativeKind: String(native.kind ?? "script"),
      nativePublish: native.publish !== false,
      nativePublishWhen: list(native.publish_when),
      nativePublishScript: String(native.publish_script ?? ""),
      nativePublishAttestGlob: String(native.publish_attest_glob ?? ""),
      nativePublishRepository: String(native.publish_repository ?? ""),
      nativePublishTags: list(native.publish_tags).map((tag) => String(tag)),
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
      deployOptionalImages: list(deploy.optional_images),
      deployWorkloads: list(deploy.workloads),
      deployBuilder: String(deploy.builder ?? "bazel"),
      deployTargetRoot: String(deploy.target_root ?? ""),
      deployBazelImages: normalizeImageMap(deploy.bazel_images),
      deployNamespace: String(deploy.namespace ?? ""),
      rollbackKind: String(rollback.kind ?? ""),
      rollbackStateDir: String(rollback.state_dir ?? ""),
      rollbackNamespace: String(rollback.namespace ?? ""),
      rollbackRelease: String(rollback.release ?? ""),
      rollbackInstruction: String(rollback.instruction ?? ""),
      rollbackNote: String(rollback.note ?? ""),
      rollbackSource: String(rollback.source ?? ""),
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
  const out = git(root, "diff", "--name-only", "--no-renames", `${base}...${head}`);
  return [...new Set(out.split("\n").map((line) => line.trim()).filter(Boolean))].sort();
}

export function gitFileText(root, revision, filePath) {
  const result = run("git", ["-C", String(root), "show", `${revision}:${filePath}`]);
  if (result.error || result.status !== 0) {
    return null;
  }
  return result.stdout;
}

export function resolveBase(root, requested) {
  if (requested) {
    if (/^0+$/.test(requested)) {
      return null;
    }
    try {
      return git(root, "rev-parse", "--verify", `${requested}^{commit}`).trim();
    } catch {
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

export const MODULE_BAZEL = "MODULE.bazel";
export const MODULE_BAZEL_LOCK = "MODULE.bazel.lock";
export const MODULE_BAZEL_MAVEN = "MODULE.bazel#maven";
export const MODULE_BAZEL_NATIVE = "MODULE.bazel#native";
export const MAVEN_EXTENSION = "@rules_jvm_external//:extensions.bzl";
export const RUST_EXTENSION = "@rules_rust//rust:extensions.bzl";
export const CRATE_EXTENSION = "@rules_rust//crate_universe:extensions.bzl";
export const ZIG_TOOLCHAIN_PREFIX = "@zig_0_14_1//";
export const SENDSPIN_BAZEL_MARKER = "satellite/third_party/bazel";

export const BAZELRC = ".bazelrc";
export const BAZELRC_CACHE = ".bazelrc#cache";

export const BAZEL_CONFIG_NATIVE = "platform/bazel#native";
const NATIVE_BAZEL_CONFIG_DIRS = ["platform/bazel/zig/", "platform/bazel/windows/"];

const IDENT = /[\p{L}\p{N}_]/u;

function tokens(text) {
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

function extensionProxy(list, extension, tag) {
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
    if (values.has(extension) && values.has(tag)) {
      return list[index][1];
    }
  }
  return null;
}

function stripProxyUse(list, proxies) {
  const rest = [];
  let index = 0;
  while (index < list.length) {
    if (
      list[index][0] === "ident" &&
      proxies.has(list[index][1]) &&
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
      proxies.has(list[index + 2][1])
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

const ANDROID_MODULE = /^rules_android(?:_ndk)?$/;
const ANDROID_EXTENSION_PREFIX = /^@rules_android(?:_ndk)?\/\//;
const ANDROID_TOOLCHAIN_PREFIX = "@androidndk//";

function stringArgNamed(args, key) {
  for (let index = 0; index + 2 < args.length; index += 1) {
    if (
      args[index][0] === "ident" &&
      args[index][1] === key &&
      args[index + 1][0] === "punct" &&
      args[index + 1][1] === "=" &&
      args[index + 2][0] === "string"
    ) {
      return stringValue(args[index + 2]);
    }
  }
  return null;
}

function firstString(args) {
  const token = args.find((item) => item[0] === "string");
  return token ? stringValue(token) : null;
}

function stripAndroid(list) {
  const proxies = new Set();
  const rest = [];
  let index = 0;
  while (index < list.length) {
    const token = list[index];
    if (
      token[0] === "ident" &&
      list[index + 1]?.[0] === "punct" &&
      list[index + 1][1] === "=" &&
      list[index + 2]?.[0] === "ident" &&
      list[index + 2][1] === "use_extension" &&
      list[index + 3]?.[0] === "punct" &&
      list[index + 3][1] === "("
    ) {
      const call = callArguments(list, index + 3);
      const label = call ? firstString(call.args) : null;
      if (call && label !== null && ANDROID_EXTENSION_PREFIX.test(label)) {
        proxies.add(token[1]);
        index = call.end + 1;
        continue;
      }
    }
    if (token[0] === "ident" && list[index + 1]?.[0] === "punct" && list[index + 1][1] === "(") {
      const call = callArguments(list, index + 1);
      if (call !== null) {
        const name = token[1];
        const moduleName = stringArgNamed(call.args, name === "bazel_dep" ? "name" : "module_name");
        const scoped =
          ((name === "bazel_dep" || name.endsWith("_override")) &&
            moduleName !== null &&
            ANDROID_MODULE.test(moduleName)) ||
          (name === "register_toolchains" &&
            onlyStrings(call.args, (value) => value.startsWith(ANDROID_TOOLCHAIN_PREFIX)));
        if (scoped) {
          index = call.end + 1;
          continue;
        }
      }
    }
    rest.push(token);
    index += 1;
  }
  return stripProxyUse(rest, proxies);
}

function stripMaven(list, proxy) {
  return stripAndroid(stripProxyUse(list, new Set([proxy])));
}

function callArguments(list, openIndex) {
  const end = matchParen(list, openIndex);
  if (end === null) {
    return null;
  }
  return { args: list.slice(openIndex + 1, end), end };
}

function onlyStrings(args, predicate) {
  let seen = false;
  for (const token of args) {
    if (token[0] === "punct" && token[1] === ",") {
      continue;
    }
    if (token[0] !== "string") {
      return false;
    }
    const value = stringValue(token);
    if (value === null || !predicate(value)) {
      return false;
    }
    seen = true;
  }
  return seen;
}

function isZigRegisterToolchains(args) {
  return onlyStrings(args, (value) => value.startsWith(ZIG_TOOLCHAIN_PREFIX));
}

function isRulesRustOverride(args) {
  for (let index = 0; index + 2 < args.length; index += 1) {
    if (
      args[index][0] === "ident" &&
      args[index][1] === "module_name" &&
      args[index + 1][0] === "punct" &&
      args[index + 1][1] === "=" &&
      args[index + 2][0] === "string"
    ) {
      if (stringValue(args[index + 2]) === "rules_rust") {
        return true;
      }
    }
  }
  return false;
}

function isSendspinArchive(args) {
  return args.some(
    (token) =>
      (token[0] === "ident" && token[1] === "_SENDSPIN_BAZEL") ||
      (token[0] === "string" && String(stringValue(token)).includes(SENDSPIN_BAZEL_MARKER)),
  );
}

function stripNative(list) {
  const proxies = new Set(
    [
      extensionProxy(list, RUST_EXTENSION, "rust"),
      extensionProxy(list, CRATE_EXTENSION, "crate"),
    ].filter((proxy) => proxy !== null),
  );
  const rest = [];
  let index = 0;
  while (index < list.length) {
    const token = list[index];
    if (token[0] === "ident" && list[index + 1]?.[0] === "punct" && list[index + 1][1] === "(") {
      const call = callArguments(list, index + 1);
      if (call !== null) {
        const name = token[1];
        const scoped =
          name === "zig_sdk_repository" ||
          (name === "register_toolchains" && isZigRegisterToolchains(call.args)) ||
          (name === "single_version_override" && isRulesRustOverride(call.args)) ||
          (name === "http_archive" && isSendspinArchive(call.args));
        if (scoped) {
          index = call.end + 1;
          continue;
        }
      }
    }
    rest.push(token);
    index += 1;
  }
  return stripProxyUse(rest, proxies);
}

function sameTokens(left, right) {
  if (left.length !== right.length) {
    return false;
  }
  return left.every(
    (token, index) => token[0] === right[index][0] && token[1] === right[index][1],
  );
}

export function moduleBazelScope(oldText, newText) {
  const oldTokens = tokens(oldText);
  const newTokens = tokens(newText);
  if (!balanced(oldTokens) || !balanced(newTokens)) {
    return null;
  }
  const oldMavenProxy = extensionProxy(oldTokens, MAVEN_EXTENSION, "maven");
  const newMavenProxy = extensionProxy(newTokens, MAVEN_EXTENSION, "maven");
  const mavenScoped =
    oldMavenProxy !== null &&
    newMavenProxy !== null &&
    sameTokens(stripMaven(oldTokens, oldMavenProxy), stripMaven(newTokens, newMavenProxy));
  if (mavenScoped) {
    return "maven";
  }
  const oldNoNative = stripNative(oldTokens);
  const newNoNative = stripNative(newTokens);
  if (sameTokens(oldNoNative, newNoNative)) {
    return "native";
  }
  if (
    oldMavenProxy !== null &&
    newMavenProxy !== null &&
    sameTokens(
      stripNative(stripMaven(oldTokens, oldMavenProxy)),
      stripNative(stripMaven(newTokens, newMavenProxy)),
    )
  ) {
    return "maven+native";
  }
  return null;
}

export function moduleBazelIsMavenOnly(oldText, newText) {
  return moduleBazelScope(oldText, newText) === "maven";
}

const BAZELRC_CACHE_FLAGS = new Set([
  "disk_cache",
  "experimental_disk_cache_gc_max_size",
  "experimental_disk_cache_gc_max_age",
  "experimental_disk_cache_gc_idle_delay",
  "repository_cache",
  "announce_rc",
  "color",
  "curses",
  "test_output",
  "test_summary",
  "verbose_failures",
]);
const BAZELRC_CACHE_FLAG_PREFIXES = ["repo_contents_cache", "show_progress"];

function isCacheFlagName(name) {
  return (
    BAZELRC_CACHE_FLAGS.has(name) ||
    BAZELRC_CACHE_FLAG_PREFIXES.some((prefix) => name.startsWith(prefix))
  );
}

function bazelrcStructuralLines(text) {
  const structural = [];
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) {
      continue;
    }
    if (/^(?:try-import|import)\b/.test(line)) {
      return null;
    }
    const parts = line.split(/\s+/);
    const command = parts[0];
    if (!/^[a-z][a-z0-9_-]*$/.test(command)) {
      return null;
    }
    const flags = parts.slice(1);
    if (!flags.length) {
      return null;
    }
    let cacheOnly = true;
    for (const flag of flags) {
      if (!flag.startsWith("--")) {
        return null;
      }
      const name = flag.slice(2).split("=")[0];
      if (!name) {
        return null;
      }
      if (!isCacheFlagName(name)) {
        cacheOnly = false;
      }
    }
    if (!cacheOnly) {
      structural.push(line);
    }
  }
  return structural;
}

export function bazelrcIsCacheOnly(oldText, newText) {
  const oldLines = bazelrcStructuralLines(oldText);
  const newLines = bazelrcStructuralLines(newText);
  if (oldLines === null || newLines === null) {
    return false;
  }
  return sameTokens(
    oldLines.map((line) => ["line", line]),
    newLines.map((line) => ["line", line]),
  );
}

function isNativeBazelConfigPath(item) {
  return NATIVE_BAZEL_CONFIG_DIRS.some((prefix) => item.startsWith(prefix));
}

function scopeChanges(changed, root, base, head) {
  const warnings = [];
  const scoped = [];
  const push = (item) => {
    if (!scoped.includes(item)) {
      scoped.push(item);
    }
  };

  let moduleVirtual = null;
  let moduleScope = null;
  if (changed.includes(MODULE_BAZEL) && base) {
    const oldText = internals.gitFileText(root, base, MODULE_BAZEL);
    const newText = internals.gitFileText(root, head, MODULE_BAZEL);
    if (oldText !== null && newText !== null) {
      moduleScope = moduleBazelScope(oldText, newText);
      if (moduleScope !== null) {
        moduleVirtual =
          moduleScope === "maven"
            ? [MODULE_BAZEL_MAVEN]
            : moduleScope === "native"
              ? [MODULE_BAZEL_NATIVE]
              : [MODULE_BAZEL_MAVEN, MODULE_BAZEL_NATIVE];
      }
    }
  }
  const lockFollows = moduleVirtual !== null && changed.includes(MODULE_BAZEL_LOCK);

  let cacheOnly = false;
  let sawNativeBazel = false;
  for (const item of changed) {
    if (item === MODULE_BAZEL && moduleVirtual) {
      for (const path of moduleVirtual) {
        push(path);
      }
      continue;
    }
    if (item === MODULE_BAZEL_LOCK && lockFollows) {
      for (const path of moduleVirtual) {
        push(path);
      }
      continue;
    }
    if (item === BAZELRC && base) {
      const oldText = internals.gitFileText(root, base, BAZELRC);
      const newText = internals.gitFileText(root, head, BAZELRC);
      if (oldText !== null && newText !== null && bazelrcIsCacheOnly(oldText, newText)) {
        push(BAZELRC_CACHE);
        cacheOnly = true;
        continue;
      }
    }
    if (isNativeBazelConfigPath(item)) {
      push(BAZEL_CONFIG_NATIVE);
      sawNativeBazel = true;
      continue;
    }
    push(item);
  }

  if (moduleVirtual !== null) {
    warnings.push(
      `MODULE.bazel changed only in the ${moduleScope} inputs; ` +
        `scoping it to ${moduleVirtual.join(", ")}`,
    );
    if (lockFollows) {
      warnings.push("MODULE.bazel.lock follows the scoped MODULE.bazel change");
    }
  }
  if (cacheOnly) {
    warnings.push(
      ".bazelrc changed only in cache/output flags; scoping it out of every " +
        "build platform",
    );
  }
  if (sawNativeBazel) {
    warnings.push(
      "platform/bazel/zig and platform/bazel/windows only affect the native " +
        "station/provider toolchains; scoping them to the native platform",
    );
  }
  return { changed: scoped, warnings };
}


export const CARGO_MANIFEST = "Cargo.toml";
export const CARGO_LOCK = "Cargo.lock";
export const CRATE_UNIVERSE_LOCK = "crate-universe.lock";

export function isRustGraphPath(filePath) {
  return (
    filePath === CARGO_MANIFEST ||
    filePath === CARGO_LOCK ||
    filePath === CRATE_UNIVERSE_LOCK ||
    pathMatches(filePath, ["packages/rust/**"])
  );
}

function isCargoInputPath(filePath) {
  return (
    filePath === CARGO_MANIFEST ||
    filePath === CARGO_LOCK ||
    filePath === CRATE_UNIVERSE_LOCK
  );
}

function unquoteToml(value) {
  const text = String(value).trim();
  const double = /^"((?:[^"\\]|\\.)*)"$/.exec(text);
  if (double) {
    return double[1].replace(/\\"/g, '"').replace(/\\\\/g, "\\");
  }
  const single = /^'([^']*)'$/.exec(text);
  if (single) {
    return single[1];
  }
  throw new Error(`unexpected TOML string: ${text.slice(0, 40)}`);
}

export function parseCargoLock(text) {
  if (typeof text !== "string" || !text.includes("[[package]]")) {
    throw new Error("Cargo.lock: no [[package]] records");
  }
  const packages = new Map();
  let current = null;
  let inPackage = false;
  let collecting = null;
  const finish = () => {
    if (!current) {
      return;
    }
    if (!current.name || !current.version) {
      throw new Error("Cargo.lock: a [[package]] record lacks name/version");
    }
    const key = `${current.name} ${current.version}`;
    packages.set(key, {
      name: current.name,
      version: current.version,
      source: current.source ?? null,
      checksum: current.checksum ?? null,
      dependencies: current.dependencies ?? [],
    });
    current = null;
  };
  for (const raw of String(text).split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) {
      continue;
    }
    if (line === "[[package]]") {
      finish();
      current = { dependencies: [] };
      inPackage = true;
      collecting = null;
      continue;
    }
    if (/^\[[^\]]+\]$/.test(line)) {
      finish();
      inPackage = false;
      collecting = null;
      continue;
    }
    if (!inPackage || !current) {
      continue;
    }
    if (collecting) {
      collecting.push(line);
      if (line.includes("]")) {
        const value = collecting.join(" ");
        current.dependencies = [...value.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((match) =>
          match[1].replace(/\\"/g, '"').replace(/\\\\/g, "\\"),
        );
        collecting = null;
      }
      continue;
    }
    const eq = line.indexOf("=");
    if (eq === -1) {
      throw new Error(`Cargo.lock: unparseable line: ${line.slice(0, 60)}`);
    }
    const key = line.slice(0, eq).trim();
    const value = line.slice(eq + 1).trim();
    if (key === "dependencies") {
      if (value.startsWith("[")) {
        if (value.includes("]")) {
          current.dependencies = [...value.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((match) =>
            match[1].replace(/\\"/g, '"').replace(/\\\\/g, "\\"),
          );
        } else {
          collecting = [value];
        }
      }
      continue;
    }
    if (["name", "version", "source", "checksum"].includes(key)) {
      current[key] = unquoteToml(value);
    }
  }
  finish();
  if (!packages.size) {
    throw new Error("Cargo.lock: no packages parsed");
  }
  return packages;
}

function collectCrateUniverseDeps(value, out) {
  if (!value) {
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) {
      if (item && typeof item === "object" && item.id) {
        out.add(String(item.id));
      }
    }
    return;
  }
  if (typeof value === "object") {
    for (const child of Object.values(value)) {
      collectCrateUniverseDeps(child, out);
    }
  }
}

export function parseCrateUniverse(text) {
  let data;
  try {
    data = JSON.parse(text);
  } catch (error) {
    throw new Error(`crate-universe.lock: ${error.message}`);
  }
  if (!data || typeof data !== "object" || !data.crates || typeof data.crates !== "object") {
    throw new Error("crate-universe.lock: missing crates map");
  }
  const entries = new Map();
  for (const [key, value] of Object.entries(data.crates)) {
    if (!value || typeof value !== "object") {
      throw new Error(`crate-universe.lock: invalid crate record ${key}`);
    }
    const deps = new Set();
    collectCrateUniverseDeps(value.common_attrs?.deps, deps);
    collectCrateUniverseDeps(value.common_attrs?.proc_macro_deps, deps);
    collectCrateUniverseDeps(value.common_attrs?.build_deps, deps);
    entries.set(key, {
      name: String(value.name ?? key.split(" ")[0]),
      version: String(value.version ?? key.split(" ")[1] ?? ""),
      checksum: JSON.stringify(value.repository ?? null),
      dependencies: [...deps],
    });
  }
  const direct = new Set(
    list(data.direct_deps).map((id) => String(id).split(/\s+/)[0]).filter(Boolean),
  );
  const workspaceMembers = new Map();
  for (const [key, value] of Object.entries(data.workspace_members ?? {})) {
    workspaceMembers.set(key, String(value));
  }
  return { entries, direct, workspaceMembers };
}

function sameSet(left, right) {
  if (left.length !== right.length) {
    return false;
  }
  const a = [...left].sort();
  const b = [...right].sort();
  return a.every((item, index) => item === b[index]);
}

function lockEntryChanged(left, right) {
  return (
    left.source !== right.source ||
    left.checksum !== right.checksum ||
    !sameSet(left.dependencies, right.dependencies)
  );
}

function cargoLockEntryChanged(left, right, leftEntries, rightEntries) {
  const leftDeps = new Set();
  const rightDeps = new Set();
  for (const dependency of left.dependencies) {
    for (const target of dependencyTargets(dependency, leftEntries)) {
      leftDeps.add(target);
    }
  }
  for (const dependency of right.dependencies) {
    for (const target of dependencyTargets(dependency, rightEntries)) {
      rightDeps.add(target);
    }
  }
  return (
    left.source !== right.source ||
    left.checksum !== right.checksum ||
    !sameSet([...leftDeps], [...rightDeps])
  );
}

export function cargoLockChanges(baseText, headText) {
  const base = parseCargoLock(baseText);
  const head = parseCargoLock(headText);
  const changed = new Set();
  for (const key of new Set([...base.keys(), ...head.keys()])) {
    const before = base.get(key);
    const after = head.get(key);
    if (!before || !after || cargoLockEntryChanged(before, after, base, head)) {
      changed.add(key);
    }
  }
  return changed;
}

export function crateUniverseChanges(base, head) {
  const changed = new Set();
  for (const key of new Set([...base.entries.keys(), ...head.entries.keys()])) {
    const before = base.entries.get(key);
    const after = head.entries.get(key);
    if (!before || !after || lockEntryChanged(before, after)) {
      changed.add(key);
    }
  }
  return changed;
}

function dependencyTargets(dependency, entries) {
  if (entries.has(dependency)) {
    return [dependency];
  }
  const name = dependency.split(/\s+/)[0];
  return [...entries.keys()].filter((key) => entries.get(key).name === name);
}

function lockAncestors(keys, entries) {
  const reverse = new Map();
  for (const [key, entry] of entries) {
    for (const dependency of entry.dependencies) {
      for (const target of dependencyTargets(dependency, entries)) {
        if (!reverse.has(target)) {
          reverse.set(target, new Set());
        }
        reverse.get(target).add(key);
      }
    }
  }
  const seen = new Set();
  const stack = [...keys];
  while (stack.length) {
    const key = stack.pop();
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    for (const parent of reverse.get(key) ?? []) {
      stack.push(parent);
    }
  }
  return seen;
}

function resolveLockSeeds(changedKeys, baseEntries, headEntries, baseUniverse, headUniverse) {
  const entryFor = (key) =>
    headEntries?.get(key) ??
    baseEntries?.get(key) ??
    headUniverse?.entries.get(key) ??
    baseUniverse?.entries.get(key) ??
    null;
  const direct = new Set([
    ...(baseUniverse?.direct ?? []),
    ...(headUniverse?.direct ?? []),
  ]);
  const labels = new Set();
  const paths = new Set();
  const unresolved = [];

  const ancestorsFor = (key) => {
    const ancestors = new Set();
    for (const entries of [baseEntries, headEntries]) {
      if (!entries?.has(key)) {
        continue;
      }
      for (const item of lockAncestors([key], entries)) {
        ancestors.add(item);
      }
    }
    for (const universe of [baseUniverse, headUniverse]) {
      if (!universe?.entries.has(key)) {
        continue;
      }
      for (const item of lockAncestors([key], universe.entries)) {
        ancestors.add(item);
      }
    }
    return ancestors;
  };

  for (const key of changedKeys) {
    const names = new Set();
    for (const ancestor of ancestorsFor(key)) {
      const entry = entryFor(ancestor);
      if (entry) {
        names.add(entry.name);
      }
    }
    const own = entryFor(key);
    if (own) {
      names.add(own.name);
    }
    const keyLabels = [...names]
      .filter((name) => direct.has(name))
      .map((name) => `@crates//:${name}`);
    const keyPaths = [];
    for (const universe of [headUniverse, baseUniverse]) {
      const memberPath = universe?.workspaceMembers.get(key);
      if (memberPath) {
        keyPaths.push(`${memberPath}/Cargo.toml`);
      }
    }
    for (const label of keyLabels) {
      labels.add(label);
    }
    for (const path of keyPaths) {
      paths.add(path);
    }
    if (!keyLabels.length && !keyPaths.length) {
      unresolved.push(own ? `${own.name} ${own.version}` : key);
    }
  }

  return {
    labels: [...labels].sort(),
    paths: [...paths].sort(),
    unresolved: unresolved.sort(),
  };
}

function stripTomlComment(line) {
  let quote = null;
  let escaped = false;
  for (let index = 0; index < line.length; index += 1) {
    const char = line[index];
    if (quote === '"') {
      if (escaped) {
        escaped = false;
      } else if (char === "\\") {
        escaped = true;
      } else if (char === '"') {
        quote = null;
      }
      continue;
    }
    if (quote === "'") {
      if (char === "'") {
        quote = null;
      }
      continue;
    }
    if (char === '"' || char === "'") {
      quote = char;
    } else if (char === "#") {
      return line.slice(0, index);
    }
  }
  return line;
}

function tomlSections(text) {
  const sections = new Map();
  const push = (name) => {
    if (!sections.has(name)) {
      sections.set(name, []);
    }
    return sections.get(name);
  };
  let current = "";
  for (const raw of String(text).split(/\r?\n/)) {
    const line = stripTomlComment(raw).trim();
    if (!line) {
      continue;
    }
    if (line.startsWith("[") && line.endsWith("]")) {
      current = line.replace(/\s+/g, "");
      push(current);
      continue;
    }
    push(current).push(line);
  }
  return sections;
}

export function cargoTomlChangedSections(baseText, headText) {
  const base = tomlSections(baseText);
  const head = tomlSections(headText);
  const changed = new Set();
  for (const name of new Set([...base.keys(), ...head.keys()])) {
    const before = (base.get(name) ?? []).join("\n");
    const after = (head.get(name) ?? []).join("\n");
    if (before !== after) {
      changed.add(name || "(top level)");
    }
  }
  return [...changed].sort();
}

function cargoScopeFailure(reason) {
  return {
    mode: "all",
    labels: [],
    paths: [],
    warnings: [`Cargo routing fell back to every Rust unit: ${reason}`],
  };
}

function analyzeCargo(root, base, head, changed) {
  const manifestChanged = changed.includes(CARGO_MANIFEST);
  const lockChanged = changed.includes(CARGO_LOCK);
  const universeChanged = changed.includes(CRATE_UNIVERSE_LOCK);
  if (!manifestChanged && !lockChanged && !universeChanged) {
    return { mode: "none", labels: [], paths: [], warnings: [] };
  }
  if (!base) {
    return cargoScopeFailure("no base revision");
  }

  const read = (revision, filePath) => internals.gitFileText(root, revision, filePath);
  let baseCargo = null;
  let headCargo = null;
  let baseUniverseText = null;
  let headUniverseText = null;
  let manifestSections = [];

  if (manifestChanged) {
    baseCargo = read(base, CARGO_MANIFEST);
    headCargo = read(head, CARGO_MANIFEST);
    if (baseCargo === null || headCargo === null) {
      return cargoScopeFailure("Cargo.toml was added or deleted");
    }
    manifestSections = cargoTomlChangedSections(baseCargo, headCargo);
    if (manifestSections.some((section) => section !== "[workspace.dependencies]")) {
      return cargoScopeFailure(
        `Cargo.toml changed outside [workspace.dependencies]: ${manifestSections.join(", ")}`,
      );
    }
  }

  if (manifestChanged && !lockChanged && !universeChanged) {
    if (manifestSections.length) {
      return cargoScopeFailure(
        "Cargo.toml changed only in [workspace.dependencies] but no lockfile changed",
      );
    }
    return {
      mode: "precise",
      labels: [],
      paths: [],
      warnings: ["Cargo.toml comment-only change; no Rust unit recompiles"],
    };
  }

  let baseLockText = null;
  let headLockText = null;
  if (lockChanged) {
    baseLockText = read(base, CARGO_LOCK);
    headLockText = read(head, CARGO_LOCK);
    if (baseLockText === null || headLockText === null) {
      return cargoScopeFailure("Cargo.lock was added or deleted");
    }
  }
  if (universeChanged || lockChanged || manifestChanged) {
    baseUniverseText = read(base, CRATE_UNIVERSE_LOCK);
    headUniverseText = read(head, CRATE_UNIVERSE_LOCK);
    if (headUniverseText === null) {
      return cargoScopeFailure("crate-universe.lock is missing at head");
    }
  }

  try {
    const baseUniverse = baseUniverseText ? parseCrateUniverse(baseUniverseText) : null;
    const headUniverse = parseCrateUniverse(headUniverseText);
    let changedKeys;
    let baseEntries = null;
    let headEntries = null;
    if (lockChanged) {
      baseEntries = parseCargoLock(baseLockText);
      headEntries = parseCargoLock(headLockText);
      changedKeys = cargoLockChanges(baseLockText, headLockText);
    } else if (universeChanged) {
      if (!baseUniverse) {
        return cargoScopeFailure("crate-universe.lock was added");
      }
      changedKeys = crateUniverseChanges(baseUniverse, headUniverse);
    } else {
      return cargoScopeFailure("Cargo input changed without a lockfile diff");
    }

    const seeds = resolveLockSeeds(
      changedKeys,
      baseEntries,
      headEntries,
      baseUniverse,
      headUniverse,
    );
    if (!changedKeys.size) {
      return {
        mode: "precise",
        labels: [],
        paths: [],
        warnings: ["lockfiles changed without a package change; no Rust unit recompiles"],
      };
    }
    const summary = [...changedKeys].sort().slice(0, 8).join(", ");
    const more = changedKeys.size > 8 ? `, +${changedKeys.size - 8} more` : "";
    if (seeds.unresolved.length) {
      const unresolved = seeds.unresolved.slice(0, 8).join(", ");
      const extra = seeds.unresolved.length > 8 ? `, +${seeds.unresolved.length - 8} more` : "";
      return cargoScopeFailure(
        `changed package(s) did not map to a crate alias or workspace path: ${unresolved}${extra}`,
      );
    }
    if (!seeds.labels.length && !seeds.paths.length) {
      return cargoScopeFailure(
        `changed package(s) produced no Bazel seed: ${summary}${more}`,
      );
    }
    return {
      mode: "precise",
      labels: seeds.labels,
      paths: seeds.paths,
      warnings: [
        `lockfile diff (${changedKeys.size} package(s): ${summary}${more}); ` +
          `routing to ${seeds.labels.length} crate alias(es)`,
      ],
    };
  } catch (error) {
    return cargoScopeFailure(error.message);
  }
}



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
    if (!label.startsWith("//")) {
      return label;
    }
    return label.slice(2).replace(":", "/");
  }

  labelsForPath(filePath) {
    return this.pathIndex.get(filePath) ?? new Set();
  }

  packageForPath(filePath) {
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

  static isTestClass(ruleClass) {
    return ruleClass === "test_suite" || /(^|_)tests?$/.test(ruleClass);
  }

  affectedRules(changed, extraSeeds = [], { includeTestRules = true } = {}) {
    const reverse = this.dependents();
    const seeds = new Set();
    for (const label of extraSeeds) {
      seeds.add(label);
    }
    for (const filePath of changed) {
      const labels = this.labelsForPath(filePath);
      if (labels.size) {
        for (const label of labels) {
          seeds.add(label);
        }
      } else {
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
    return new Set(
      [...affected].filter(
        (label) =>
          Object.hasOwn(this.rules, label) &&
          (includeTestRules || !BazelGraph.isTestClass(this.rules[label])),
      ),
    );
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
    this.deploySelected = deploySelected;
    this.androidDistribute = androidDistribute;
    this.warnings = warnings;
  }
}

export const GLOBAL = "*";

export function androidDistributePaths(config) {
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

function labelUnderRoot(label, rootLabel) {
  return (
    label === rootLabel ||
    label.startsWith(`${rootLabel}:`) ||
    label.startsWith(`${rootLabel}/`)
  );
}

function routePlatforms(
  config,
  changed,
  root,
  invalidated,
  graph,
  graphLoader,
  warnings,
  cargo = null,
  includeTestTargets = true,
) {
  const routes = {};
  const rustGraphRequested = changed.some(isRustGraphPath);
  const cargoLabels = cargo?.mode === "precise" ? cargo.labels : [];
  const cargoPaths = cargo?.mode === "precise" ? cargo.paths : [];
  const graphChanged = changed.filter((item) => !isCargoInputPath(item));
  let graphAffected = null;
  let graphFailure = null;

  const ensureGraph = () => {
    if (graphAffected) {
      return graphAffected;
    }
    if (graphFailure) {
      throw graphFailure;
    }
    try {
      if (graph === null || graph === undefined) {
        if (!graphLoader) {
          throw new Error("no graph loader configured");
        }
        graph = graphLoader();
      }
      graphAffected = graph.affectedRules(
        [...graphChanged, ...cargoPaths],
        cargoLabels,
        { includeTestRules: includeTestTargets },
      );
      return graphAffected;
    } catch (error) {
      graphFailure = error;
      throw error;
    }
  };

  const matchesRoutingRoots = (unit, labels) =>
    [...labels].some((label) =>
      unit.routingRoots.some((rootLabel) => labelUnderRoot(label, rootLabel)),
    );

  for (const platform of ["android", "bazel"]) {
    const bazelRoute = new PlatformRoute(platform, "bazel-graph");
    const units = config.byPlatform(platform);
    if (invalidated.has(GLOBAL) || invalidated.has(platform)) {
      bazelRoute.mode = "all";
      bazelRoute.units = [...units];
    } else {
      const touched = changed.some((item) =>
        units.some((unit) => pathMatches(item, unit.paths)),
      );
      const needsGraph = touched || (platform === "bazel" && rustGraphRequested);
      if (!needsGraph) {
        bazelRoute.mode = "paths";
      } else {
        try {
          const affected = ensureGraph();
          for (const unit of units) {
            if (
              changed.some((item) => pathMatches(item, unit.paths)) ||
              matchesRoutingRoots(unit, affected)
            ) {
              bazelRoute.units.push(unit);
            }
          }
        } catch (error) {
          bazelRoute.mode = "all";
          bazelRoute.units = [...units];
          bazelRoute.warnings.push(
            `bazel graph failed (${error.message}); running every ${platform} unit`,
          );
        }
      }
    }
    bazelRoute.units = expandDependsOn(config, bazelRoute.units);
    routes[platform] = bazelRoute;
  }

  const pythonRoute = new PlatformRoute("python", "paths");
  const pythonUnits = config.byPlatform("python");
  if (invalidated.has(GLOBAL) || invalidated.has("python")) {
    pythonRoute.mode = "all";
    pythonRoute.units = [...pythonUnits];
  } else {
    let pythonAffected = null;
    let pythonFailed = false;
    if (rustGraphRequested) {
      try {
        pythonAffected = ensureGraph();
      } catch {
        pythonFailed = true;
      }
    }
    for (const unit of pythonUnits) {
      if (changed.some((item) => pathMatches(item, unit.paths))) {
        pythonRoute.units.push(unit);
      } else if (rustGraphRequested && unit.routingRoots.length) {
        if (pythonFailed || matchesRoutingRoots(unit, pythonAffected)) {
          pythonRoute.units.push(unit);
        }
      }
    }
    if (pythonFailed) {
      pythonRoute.warnings.push(
        `bazel graph failed (${graphFailure.message}); running every graph-aware python unit`,
      );
    }
  }
  pythonRoute.units = expandDependsOn(config, pythonRoute.units);
  routes.python = pythonRoute;

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

  const nativeRoute = new PlatformRoute("native", "paths");
  const nativeUnits = config.byPlatform("native");
  if (invalidated.has(GLOBAL) || invalidated.has("native")) {
    nativeRoute.mode = "all";
    nativeRoute.units = [...nativeUnits];
  } else {
    let nativeAffected = null;
    let nativeFailed = false;
    if (rustGraphRequested) {
      try {
        nativeAffected = ensureGraph();
      } catch {
        nativeFailed = true;
      }
    }
    for (const unit of nativeUnits) {
      if (changed.some((item) => pathMatches(item, unit.paths))) {
        nativeRoute.units.push(unit);
      } else if (rustGraphRequested) {
        if (!unit.routingRoots.length) {
          nativeRoute.units.push(unit);
        } else if (nativeFailed || matchesRoutingRoots(unit, nativeAffected)) {
          nativeRoute.units.push(unit);
        }
      }
    }
    if (nativeFailed) {
      nativeRoute.mode = "all";
      nativeRoute.warnings.push(
        `bazel graph failed (${graphFailure.message}); running every native unit`,
      );
    }
  }
  nativeRoute.units = expandDependsOn(config, nativeRoute.units);
  routes.native = nativeRoute;

  routes.quality = new PlatformRoute("quality", "always", [...config.byPlatform("quality")]);

  return routes;
}

export function isMarkdownPath(filePath) {
  const lower = filePath.toLowerCase();
  return lower.endsWith(".md") || lower.endsWith(".mdx");
}

export function explicitMarkdownPatterns(config) {
  return [
    ...config.units.flatMap((unit) => unit.paths),
    ...config.invalidations.flatMap((rule) => rule.paths),
  ].filter(isMarkdownPath);
}

export function routableChanges(config, changed) {
  const explicit = explicitMarkdownPatterns(config);
  return changed.filter((item) => !isMarkdownPath(item) || pathMatches(item, explicit));
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
  const scoped = scopeChanges(changed, root, base, head);
  const allChanged = scoped.changed;
  changed = routableChanges(config, allChanged);
  const cargo = analyzeCargo(root, base, head, changed);
  const invalidated = invalidatedPlatforms(config, changed);
  if (cargo.mode === "all") {
    for (const platform of ["bazel", "native", "python"]) {
      invalidated.add(platform);
    }
  }
  const warnings = [...scoped.warnings, ...cargo.warnings];
  const routes = routePlatforms(
    config,
    changed,
    root,
    invalidated,
    graph,
    graphLoader,
    warnings,
    cargo,
  );

  const deployInvalidated = invalidatedPlatforms(config, changed, true);
  if (cargo.mode === "all") {
    for (const platform of ["bazel", "native", "python"]) {
      deployInvalidated.add(platform);
    }
  }
  let deploySelected;
  const samePlatforms =
    deployInvalidated.size === invalidated.size &&
    [...deployInvalidated].every((platform) => invalidated.has(platform));
  const deployNeedsPrecisePass = changed.some(isRustGraphPath) && cargo.mode !== "all";
  if (samePlatforms && !deployNeedsPrecisePass) {
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
      cargo,
      false,
    );
    deploySelected = new Set(
      Object.values(deployRoutes).flatMap((platformRoute) =>
        platformRoute.units.map((unit) => unit.id),
      ),
    );
  }

  const docsOnly =
    Boolean(allChanged.length) &&
    allChanged.every((item) => isMarkdownPath(item) || item.startsWith("docs/"));
  const affectedIds = new Set(
    Object.values(routes).flatMap((platformRoute) =>
      platformRoute.units.map((unit) => unit.id),
    ),
  );
  const deployControl =
    config.deployControlUnits.some((unit) => affectedIds.has(unit)) ||
    changed.some((item) => pathMatches(item, config.deployControlPaths));

  for (const platformRoute of Object.values(routes)) {
    warnings.push(...platformRoute.warnings);
  }
  return new Route({
    base,
    head,
    changed: allChanged,
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

export function nativePublishSelected(unit, changed) {
  if (!unit.nativePublish) {
    return false;
  }
  const releasePaths = unit.nativePublishWhen.length ? unit.nativePublishWhen : unit.paths;
  return changed.some((item) => pathMatches(item, releasePaths));
}

export function androidUniverse(config) {
  const androidRoots = [
    ...new Set(config.byPlatform("android").flatMap((unit) => unit.bazelRoots)),
  ].sort();
  return androidRoots.map((root) => `${root}/...`).join(" + ");
}

export function matrixEntries(route_, config) {
  const universe = androidUniverse(config);
  const android = route_.routes.android.units.map((unit) => ({
    id: unit.id,
    test_targets: space(unit.testTargets),
    build_targets: space(unit.buildTargets),
    artifact: unit.artifact,
    artifact_name: unit.artifactName,
    sdk_packages: space(unit.bazelSdkPackages),
    universe,
  }));
  if (route_.androidDistribute) {
    const release = config.units.find(
      (unit) => unit.platform === "android" && unit.androidRelease,
    );
    if (release && !android.some((entry) => entry.id === release.id)) {
      android.push({
        id: release.id,
        test_targets: space(release.testTargets),
        build_targets: space(release.buildTargets),
        artifact: release.artifact,
        artifact_name: release.artifactName,
        sdk_packages: space(release.bazelSdkPackages),
        universe,
      });
    }
  }
  const deployFields = (unit) =>
    route_.deploySelected.has(unit.id)
      ? deployImageFields(unit)
      : {
          images: "",
          image_list: "[]",
          image_count: 0,
          builder: unit.deployBuilder,
          target_root: "",
          bazel_images: "{}",
        };
  const bazel = route_.routes.bazel.units.map((unit) => ({
    id: unit.id,
    kind: "bazel",
    test_targets: space(unit.testTargets),
    build_targets: space(unit.buildTargets),
    artifact: unit.artifact,
    artifact_name: unit.artifactName,
    suites: "",
    compile: "",
    deploy_tool: false,
    ...deployFields(unit),
  }));
  const pythonWithImages = route_.routes.python.units.filter(
    (unit) => unit.deployImages.length && route_.deploySelected.has(unit.id),
  );
  const python = route_.routes.python.units
    .filter((unit) => !pythonWithImages.includes(unit))
    .map((unit) => ({
      id: unit.id,
      suites: space(unit.suites),
      compile: space(unit.compileDirs),
      deploy_tool: unit.pythonDeployTool,
    }));
  const unit_ = [
    ...bazel,
    ...pythonWithImages.map((unit) => ({
      id: unit.id,
      kind: "python",
      test_targets: "",
      build_targets: "",
      artifact: "",
      artifact_name: "",
      suites: space(unit.suites),
      compile: space(unit.compileDirs),
      deploy_tool: unit.pythonDeployTool,
      ...deployFields(unit),
    })),
  ];
  const js = route_.routes.js.units.map((unit) => ({
    id: unit.id,
    tests: space(unit.jsTests),
  }));
  const native = route_.routes.native.units.map((unit) => ({
    id: unit.id,
    kind: unit.nativeKind,
    publish: nativePublishSelected(unit, route_.changed),
    publish_script: unit.nativePublishScript,
    publish_attest_glob: unit.nativePublishAttestGlob,
    publish_repository: unit.nativePublishRepository,
    publish_tags: space(unit.nativePublishTags),
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
  return { android, bazel, unit: unit_, python, js, native };
}

export function deployImageFields(unit) {
  if (!unit.deployImages.length) {
    return {
      images: "",
      image_list: "[]",
      image_count: 0,
      builder: unit.deployBuilder,
      target_root: "",
      bazel_images: "{}",
    };
  }
  const imageList = [...unit.deployImages];
  if (
    (unit.deployBuilder === "bazel" || unit.deployBuilder === "mixed") &&
    !imageList.includes("home-deploy")
  ) {
    imageList.push("home-deploy");
  }
  return {
    images: space(unit.deployImages),
    image_list: JSON.stringify(imageList),
    image_count: imageList.length,
    builder: unit.deployBuilder,
    target_root: unit.deployTargetRoot,
    bazel_images: JSON.stringify(unit.deployBazelImages),
  };
}

export function deployEntries(route_, config) {
  const selected = route_.deploySelected;
  const entries = [];
  for (const unit of config.units) {
    if (!unit.deployImages.length || !selected.has(unit.id)) {
      continue;
    }
    entries.push({
      id: unit.id,
      images: space(unit.deployImages),
      optional_images: space(unit.deployOptionalImages),
      workloads: space(unit.deployWorkloads),
      builder: unit.deployBuilder,
      target_root: unit.deployTargetRoot,
      bazel_images: JSON.stringify(unit.deployBazelImages),
      namespace: unit.deployNamespace,
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
    android_units: jsonDumps(entries.android),
    unit_units: jsonDumps(entries.unit),
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

export const internals = {
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
