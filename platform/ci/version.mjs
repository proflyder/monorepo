#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

export const BASE_PATTERN = /^(v)?(\d+)\.(\d+)(?:\.(\d+))?$/;
export const VERSION_PATTERN = /^v?\d+\.\d+\.\d+$/;
export const VERSION_ANNOTATION = "org.opencontainers.image.version";

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

export function formatVersion(base, patch) {
  if (!Number.isInteger(patch) || patch < 0) {
    throw new Error(`patch must be a non-negative integer, got '${patch}'`);
  }
  return `${base.prefix}${base.major}.${base.minor}.${patch}`;
}

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

export function readCommitCount(exec, { rev, paths }) {
  const targets = [rev, "--", ...paths];
  const output = exec("git", ["rev-list", "--count", ...targets]).trim();
  const count = Number(output);
  if (!Number.isInteger(count) || count < 0) {
    throw new Error(`git rev-list printed '${output}', expected a commit count`);
  }
  return count;
}

export function counterRevision(exec) {
  try {
    exec("git", ["rev-parse", "--verify", "--quiet", "HEAD^1^{commit}"]);
    return "HEAD^1";
  } catch {
    throw new Error(
      "HEAD has no parent: the version counter needs the first-parent history " +
        "(fetch-depth: 0 in actions/checkout)",
    );
  }
}

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
    emit(`version=${version}`, args);
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
    emit(
      [
        `version=${result.version}`,
        `base=${result.baseText}`,
        `patch=${result.patch}`,
        `count=${result.count}`,
      ].join("\n"),
      args,
    );
  } else {
    process.stdout.write(`${result.version}\n`);
  }
  return 0;
}

export function emit(lines, args) {
  const text = String(lines);
  if (!args.githubOutput) {
    process.stdout.write(`${text}\n`);
    return;
  }
  const file = process.env.GITHUB_OUTPUT;
  if (!file) {
    throw new Error("--github-output needs GITHUB_OUTPUT");
  }
  fs.appendFileSync(file, `${text}\n`);
  process.stdout.write(`${text}\n`);
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
