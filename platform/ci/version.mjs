#!/usr/bin/env node
/**
 * Deterministic, monotone release versions for native units.
 *
 * A native unit (Pulsar firmware, the Windows logon provider) used to require a
 * hand-written `VERSION` bump in a separate pull request before every release.
 * The version is now computed from the repository itself:
 *
 *   version = <base major.minor>.<patch>
 *   patch   = <base floor> + <commits touching the unit's paths>
 *
 * `VERSION` holds only the base (`v0.6`, `1.4`). The patch is the number of
 * commits reachable from `HEAD^1` that touch the unit's `paths`, so it grows by
 * one with every merged change of the unit, never repeats and never needs an
 * edit. The optional `MAJOR.MINOR.PATCH` form stays accepted as a *floor*
 * (`v0.5.1020`): it let the file seed a version above an already published tag
 * once, before the counter took over, and keeps a rollover of the minor version
 * monotone.
 *
 * Why `HEAD^1` and not all of history: a pull request builds the merge commit
 * `M = merge(D, H)`, a squash merge lands as `S` on the same `D`. Both have
 * `D` as their first parent, so both compute the same patch and the version
 * baked into the PR-built binary equals the release tag. The merge commit
 * itself is not counted, so the release that a change triggers still gets a
 * fresh number relative to the previous release.
 *
 * The published OCI artifact is the authority: `publish-oci.sh` records
 * `org.opencontainers.image.version`, and the promote path reads that
 * annotation instead of recomputing, so the release tag can never disagree with
 * the binary's `--version` (the station updater compares the two). The module
 * also reads the annotation of an already published tag, which is how a manual
 * re-deploy finds the current release version without a `VERSION` edit.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

/** `MAJOR.MINOR` with an optional `v` prefix and an optional floor patch. */
export const BASE_PATTERN = /^(v)?(\d+)\.(\d+)(?:\.(\d+))?$/;
/** The release tag shape the updater and `promote.mjs` accept. */
export const VERSION_PATTERN = /^v?\d+\.\d+\.\d+$/;
/** Annotation `publish-oci.sh` writes next to the artifact. */
export const VERSION_ANNOTATION = "org.opencontainers.image.version";

/**
 * Split a `VERSION` file into its base and floor.
 *
 * `v0.6` → `{prefix: "v", major: 0, minor: 6, floor: 0}`
 * `v0.5.1020` → `{prefix: "v", major: 0, minor: 5, floor: 1020}`
 */
export function parseBase(text) {
  const raw = String(text ?? "").trim();
  const match = BASE_PATTERN.exec(raw);
  if (!match) {
    throw new Error(
      `VERSION must be MAJOR.MINOR (or MAJOR.MINOR.PATCH as a floor), got '${raw}'`,
    );
  }
  return {
    prefix: match[1] || "",
    major: Number(match[2]),
    minor: Number(match[3]),
    floor: Number(match[4] || 0),
  };
}

/** Render the computed release version for a base and a patch counter. */
export function formatVersion(base, patch) {
  if (!Number.isInteger(patch) || patch < 0) {
    throw new Error(`patch must be a non-negative integer, got '${patch}'`);
  }
  return `${base.prefix}${base.major}.${base.minor}.${patch}`;
}

/**
 * Compute the version for a base text and a commit counter.
 *
 * The counter is added to the base floor, so a one-time `MAJOR.MINOR.PATCH`
 * seed keeps the sequence above the tag it replaced.
 */
export function computeVersion(text, count) {
  const base = parseBase(text);
  const commits = Number(count);
  if (!Number.isInteger(commits) || commits < 0) {
    throw new Error(`commit count must be a non-negative integer, got '${count}'`);
  }
  const patch = base.floor + commits;
  const version = formatVersion(base, patch);
  if (!VERSION_PATTERN.test(version)) {
    throw new Error(`computed version '${version}' is not MAJOR.MINOR.PATCH`);
  }
  return { version, base, patch, count: commits };
}

/**
 * Count the commits reachable from `rev` (buffered so the whole expression
 * fails on a git error instead of piping a fatal message through awk).
 */
export function readCommitCount(exec, { rev, paths }) {
  const targets = [rev, "--", ...paths];
  const output = exec("git", ["rev-list", "--count", ...targets]).trim();
  const count = Number(output);
  if (!Number.isInteger(count) || count < 0) {
    throw new Error(`git rev-list printed '${output}', expected a commit count`);
  }
  return count;
}

/** First-parent revision of HEAD, falling back to HEAD for a root commit. */
export function counterRevision(exec) {
  for (const rev of ["HEAD^1", "HEAD"]) {
    try {
      exec("git", ["rev-parse", "--verify", "--quiet", `${rev}^{commit}`]);
      return rev;
    } catch {
      // try the next candidate
    }
  }
  throw new Error("cannot resolve HEAD^1 or HEAD");
}

/** Resolve the version of a native unit from the working tree. */
export function resolveVersion({ unit, exec, cwd = process.cwd() }) {
  const baseFile = path.join(unit.workdir, "VERSION");
  const absolute = path.join(cwd, baseFile);
  if (!fs.existsSync(absolute)) {
    throw new Error(`unit ${unit.id} has no ${baseFile}`);
  }
  const baseText = fs.readFileSync(absolute, "utf8");
  const count = readCommitCount(exec, {
    rev: counterRevision(exec),
    paths: unit.paths,
  });
  return { ...computeVersion(baseText, count), baseText: baseText.trim(), baseFile };
}

/** Read the version annotation of an already published OCI artifact. */
export function publishedVersion(exec, reference) {
  const text = exec("crane", ["manifest", String(reference || "")]);
  let manifest;
  try {
    manifest = JSON.parse(text);
  } catch (error) {
    throw new Error(`cannot parse the manifest of ${reference}: ${error.message}`);
  }
  const value = String(manifest?.annotations?.[VERSION_ANNOTATION] || "").trim();
  if (!VERSION_PATTERN.test(value)) {
    throw new Error(
      `${reference} has no usable ${VERSION_ANNOTATION} annotation (got '${value}')`,
    );
  }
  return value;
}

function realExec(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd,
    encoding: "utf8",
    env: process.env,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    const detail = (result.stderr || result.stdout || "").trim();
    throw new Error(`${command} ${args.join(" ")} failed${detail ? `: ${detail}` : ""}`);
  }
  return result.stdout;
}

/** Native units from the unit map, the one source of paths and workdir. */
async function nativeUnit(id) {
  const { config } = await import("./units.mjs");
  const unit = config.units.find((entry) => entry.platform === "native" && entry.id === id);
  if (!unit) {
    throw new Error(`unknown native unit '${id}'`);
  }
  return { id: unit.id, workdir: unit.native.workdir, paths: unit.paths };
}

function parseArgs(argv) {
  const args = { unit: "", published: "", json: false, githubOutput: false };
  for (let index = 0; index < argv.length; index += 1) {
    switch (argv[index]) {
      case "--unit":
        args.unit = argv[index + 1];
        index += 1;
        break;
      case "--published":
        args.published = argv[index + 1];
        index += 1;
        break;
      case "--json":
        args.json = true;
        break;
      case "--github-output":
        args.githubOutput = true;
        break;
      default:
        throw new Error(`unknown argument '${argv[index]}'`);
    }
  }
  if (!args.unit && !args.published) {
    throw new Error("--unit <id> or --published <oci reference> is required");
  }
  return args;
}

export async function main(argv = process.argv.slice(2)) {
  const args = parseArgs(argv);
  const exec = realExec;
  if (args.published) {
    const version = publishedVersion(exec, args.published);
    if (args.githubOutput) {
      process.stdout.write(`version=${version}\n`);
    } else {
      process.stdout.write(`${version}\n`);
    }
    return 0;
  }
  const unit = await nativeUnit(args.unit);
  const result = resolveVersion({ unit, exec });
  if (args.json) {
    process.stdout.write(
      `${JSON.stringify({
        unit: unit.id,
        version: result.version,
        base: result.baseText,
        patch: result.patch,
        count: result.count,
        base_file: result.baseFile,
      })}\n`,
    );
  } else if (args.githubOutput) {
    process.stdout.write(
      `version=${result.version}\nbase=${result.baseText}\npatch=${result.patch}\ncount=${result.count}\n`,
    );
  } else {
    process.stdout.write(`${result.version}\n`);
  }
  return 0;
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
