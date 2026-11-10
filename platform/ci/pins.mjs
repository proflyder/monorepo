#!/usr/bin/env node
/**
 * Commit verified image digests to the machine branch `deploy/home-apps`.
 *
 * Flux reads that branch through `GitRepository/flux-pins` and publishes the
 * `home-app-images` ConfigMap, so a unit that owns images
 * receives its image from here instead of from a `kubectl` rollout
 * (DECISIONS 54).
 *
 * The job that runs this script is deliberately small: it has no VPN, no
 * OpenBao and no kubeconfig. It only rewrites the keys of the unit it was
 * given (`BUILD_ID` and one `<IMAGE>_IMAGE` per image), records the provenance
 * annotations (`source-commit`, `source-tree`, `verified-at`, `run`) and
 * pushes one commit. `develop` is never touched.
 *
 * The branch may move between the fetch and the push (another unit's matrix
 * job commits concurrently), so the push is retried from a fresh fetch: the
 * update is recomputed against the new file and only the same keys change.
 *
 * Pure text handling (`updatePins`) is tested without Git in
 * `pins.test.mjs`; `main` takes an injected runner so the retry path is also
 * testable.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

/** Machine branch Flux reads; never `develop`. */
export const PINS_BRANCH = "deploy/home-apps";
/** File inside the branch that holds `ConfigMap/home-app-images`. */
export const PINS_PATH = "pins/home-app-images.yaml";
/** Keys owned by this script, in canonical order. */
export const PINS_ANNOTATIONS = [
  "proflyder.dev/source-commit",
  "proflyder.dev/source-tree",
  "proflyder.dev/verified-at",
  "proflyder.dev/run",
];

const DIGEST = /^sha256:[0-9a-f]{64}$/;

/**
 * ConfigMap key for an image: `ha-bridge` -> `HA_BRIDGE_IMAGE`. The pin names
 * never reveal a secret; they are the substitution variables the app manifests
 * already use.
 */
export function imageKey(image) {
  const name = String(image || "").trim();
  if (!/^[a-z0-9][a-z0-9._-]*$/.test(name)) {
    throw new Error(`invalid image name '${name}'`);
  }
  const key = name.toUpperCase().replace(/[^A-Z0-9]+/g, "_");
  return `${key}_IMAGE`;
}

/** `sha256:…` values from `--digests image=sha256:…,image=sha256:…`. */
export function parseDigests(value) {
  const digests = new Map();
  for (const part of String(value || "").split(",")) {
    const item = part.trim();
    if (!item) {
      continue;
    }
    const index = item.indexOf("=");
    if (index <= 0) {
      throw new Error(`invalid digest entry '${item}' (expected image=sha256:…)`);
    }
    const image = item.slice(0, index).trim();
    const digest = item.slice(index + 1).trim();
    if (!DIGEST.test(digest)) {
      throw new Error(`invalid digest for '${image}': '${digest}'`);
    }
    imageKey(image);
    digests.set(image, digest);
  }
  return digests;
}

/** Full GHCR reference of a pinned image. */
export function imageReference(image, digest) {
  return `ghcr.io/proflyder/${image}@${digest}`;
}

function annotationLines(lines) {
  const start = lines.findIndex((line) => /^\s{2}annotations:\s*$/.test(line));
  if (start === -1) {
    throw new Error("pins file has no metadata.annotations block");
  }
  let end = start + 1;
  const values = new Map();
  for (; end < lines.length; end += 1) {
    const match = /^\s{4}([A-Za-z0-9._/-]+):\s*(.*?)\s*$/.exec(lines[end]);
    if (!match) {
      break;
    }
    values.set(match[1], match[2].replace(/^"(.*)"$/, "$1"));
  }
  return { start, end, values };
}

function dataLines(lines) {
  const start = lines.findIndex((line) => /^data:\s*$/.test(line));
  if (start === -1) {
    throw new Error("pins file has no top-level data: block");
  }
  let end = start + 1;
  const values = new Map();
  for (; end < lines.length; end += 1) {
    const match = /^\s{2}([A-Za-z0-9_]+):\s*(.*?)\s*$/.exec(lines[end]);
    if (!match) {
      break;
    }
    values.set(match[1], match[2].replace(/^"(.*)"$/, "$1"));
  }
  return { start, end, values };
}

/**
 * Apply the unit's keys and the provenance annotations to the pins file.
 *
 * Only the keys in `values` and the four managed annotations are rewritten;
 * every other unit's digest is preserved verbatim. Returns the new text and
 * whether anything actually changed, so an unchanged run commits nothing.
 */
export function updatePins(text, { values = {}, annotations = {} }) {
  if (!String(text || "").trim()) {
    throw new Error("pins file is empty");
  }
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  const original = text.split(/\r?\n/);
  let lines = [...original];

  for (const [key, value] of Object.entries(annotations)) {
    if (!PINS_ANNOTATIONS.includes(key)) {
      throw new Error(`refusing to write an unmanaged annotation '${key}'`);
    }
    if (/[\r\n]/.test(String(value))) {
      throw new Error(`annotation '${key}' must be a single line`);
    }
  }
  for (const [key, value] of Object.entries(values)) {
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) {
      throw new Error(`invalid ConfigMap key '${key}'`);
    }
    if (/[\r\n"]/.test(String(value))) {
      throw new Error(`value for '${key}' must be a single line without quotes`);
    }
  }

  // Annotations: update in place, append the managed keys that are missing.
  const ann = annotationLines(lines);
  const annBlock = [...ann.values.entries()].map(([key, value]) => [
    key,
    Object.prototype.hasOwnProperty.call(annotations, key) ? annotations[key] : value,
  ]);
  for (const key of PINS_ANNOTATIONS) {
    if (Object.prototype.hasOwnProperty.call(annotations, key) &&
      !ann.values.has(key)) {
      annBlock.push([key, annotations[key]]);
    }
  }
  lines.splice(
    ann.start + 1,
    ann.end - ann.start - 1,
    ...annBlock.map(([key, value]) => `    ${key}: "${value}"`),
  );

  // Data: same treatment; the block stays sorted so diffs are minimal.
  const data = dataLines(lines);
  for (const [key, value] of Object.entries(values)) {
    data.values.set(key, value);
  }
  const dataBlock = [...data.values.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, value]) => `  ${key}: "${value}"`);
  lines.splice(
    data.start + 1,
    data.end - data.start - 1,
    ...dataBlock,
  );

  lines = lines.filter((line, index) => !(index === lines.length - 1 && line === ""));
  const next = `${lines.join(eol)}${eol}`;
  return { text: next, changed: next !== text };
}

/** Keys this unit contributes, from its image list and the deploy commit. */
export function pinUpdates({ images, digests, commit }) {
  const values = {};
  if (!/^[0-9a-f]{7,40}$/.test(String(commit || ""))) {
    throw new Error(`commit must be 7-40 hex characters, got '${commit}'`);
  }
  values.BUILD_ID = String(commit).slice(0, 12);
  for (const image of images) {
    const digest = digests.get(image);
    if (!digest) {
      throw new Error(`no verified digest for image '${image}'`);
    }
    values[imageKey(image)] = imageReference(image, digest);
  }
  return values;
}

/** Commit subject for the machine branch. */
export function commitMessage(unit, commit) {
  return `chore(pins): ${unit} ${String(commit).slice(0, 12)}`;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    ...options,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} failed: ${(result.stderr || "").trim()}`,
    );
  }
  return result.stdout;
}

function parseArgs(argv) {
  const args = {
    branch: PINS_BRANCH,
    path: PINS_PATH,
    attempts: 3,
    verifiedAt: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = () => {
      index += 1;
      if (index >= argv.length) {
        throw new Error(`${arg} requires a value`);
      }
      return argv[index];
    };
    switch (arg) {
      case "--unit":
        args.unit = next();
        break;
      case "--images":
        args.images = next().split(/[\s,]+/).filter(Boolean);
        break;
      case "--digests":
        args.digests = next();
        break;
      case "--commit":
        args.commit = next();
        break;
      case "--tree":
        args.tree = next();
        break;
      case "--run":
        args.run = next();
        break;
      case "--verified-at":
        args.verifiedAt = next();
        break;
      case "--branch":
        args.branch = next();
        break;
      case "--path":
        args.path = next();
        break;
      case "--attempts":
        args.attempts = Number(next());
        break;
      case "--repo":
        args.repo = next();
        break;
      default:
        throw new Error(`unrecognized argument: ${arg}`);
    }
  }
  for (const required of ["unit", "images", "digests", "commit", "tree"]) {
    if (!args[required] || (Array.isArray(args[required]) && !args[required].length)) {
      throw new Error(`--${required.replace(/_/g, "-")} is required`);
    }
  }
  return args;
}

/**
 * Fetch the branch into a detached worktree, apply the update and push once.
 * Returns `{ changed, commit }`.
 */
export function syncOnce(args, options = {}) {
  const exec = options.exec ?? run;
  const repo = args.repo ?? process.cwd();
  const root = fs.mkdtempSync(path.join(options.tmpdir ?? os.tmpdir(), "home-pins-"));
  // `git worktree add` needs a path it can create itself.
  const dir = path.join(root, "wt");
  let added = false;
  try {
    exec("git", ["-C", repo, "fetch", "--no-tags", "origin", args.branch]);
    exec("git", [
      "-C", repo, "worktree", "add", "--force", "--detach", dir, "FETCH_HEAD",
    ]);
    added = true;
    const file = path.join(dir, args.path);
    const current = fs.existsSync(file) ? fs.readFileSync(file, "utf8") : "";
    if (!current) {
      throw new Error(`${args.path} is missing on ${args.branch}`);
    }
    const digests = parseDigests(args.digests);
    const values = pinUpdates({ images: args.images, digests, commit: args.commit });
    const update = updatePins(current, {
      values,
      annotations: {
        "proflyder.dev/source-commit": args.commit,
        "proflyder.dev/source-tree": args.tree,
        "proflyder.dev/verified-at": args.verifiedAt,
        ...(args.run ? { "proflyder.dev/run": args.run } : {}),
      },
    });
    if (!update.changed) {
      return { changed: false, commit: null };
    }
    fs.writeFileSync(file, update.text);
    exec("git", ["-C", dir, "add", args.path]);
    exec("git", [
      "-C", dir,
      "-c", "user.name=github-actions[bot]",
      "-c", "user.email=41898282+github-actions[bot]@users.noreply.github.com",
      "commit", "--quiet", "-m", commitMessage(args.unit, args.commit),
    ]);
    exec("git", [
      "-C", dir, "push", "--quiet", "origin",
      `HEAD:refs/heads/${args.branch}`,
    ]);
    return {
      changed: true,
      commit: exec("git", ["-C", dir, "rev-parse", "HEAD"]).trim(),
    };
  } finally {
    if (added) {
      try {
        exec("git", ["-C", repo, "worktree", "remove", "--force", dir]);
      } catch {
        // A failed cleanup must not mask the real error or the push result.
      }
    }
    fs.rmSync(root, { recursive: true, force: true });
  }
}

/** Retry `syncOnce` on a race with another unit's pins commit. */
export function syncPins(args, options = {}) {
  const log = options.log ?? console.log;
  const once = options.once ?? syncOnce;
  let lastError;
  for (let attempt = 1; attempt <= args.attempts; attempt += 1) {
    try {
      return once(args, options);
    } catch (error) {
      lastError = error;
      if (attempt === args.attempts) {
        break;
      }
      log(
        `pins: attempt ${attempt}/${args.attempts} failed (${error.message}); ` +
          "refetching the branch",
      );
    }
  }
  throw lastError;
}

export async function main(argv = process.argv.slice(2)) {
  const args = parseArgs(argv);
  const result = syncPins(args);
  if (!result.changed) {
    console.log(`pins: ${args.unit} is already pinned at ${args.commit.slice(0, 12)}`);
    return 0;
  }
  console.log(`pins: committed ${result.commit} to ${args.branch}`);
  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = await main();
  } catch (error) {
    console.error(error?.stack || String(error));
    process.exitCode = 1;
  }
}
