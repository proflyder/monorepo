#!/usr/bin/env node
/**
 * Promote the images a pull request already built (DECISIONS 36).
 *
 * `ci.yml` builds every affected deploy image on a pull request and publishes it
 * as `ghcr.io/<registry>/<image>:tree-<git tree hash>` with build provenance.
 * The squash merge of that PR keeps the tree, so `deploy-home` can deploy the
 * exact bytes that passed CI instead of rebuilding them:
 *
 *   1. resolve `<image>:tree-<tree>` to a digest (`crane digest`);
 *   2. verify its provenance (`gh attestation verify oci://…`) with the signer
 *      restricted to this repository and the CI workflows that produce it;
 *   3. add the usual `<commit sha>` and `develop` tags to the SAME digest
 *      (`crane tag`) and prove the tag resolves to it.
 *
 * If any image of the unit is missing, unverified or cannot be tagged, the whole
 * unit falls back to the normal build path: the workflow passes an empty
 * `PREBUILT_DIGESTS`, and the deploy script builds and pushes exactly as before.
 * A partially promoted unit is never deployed.
 *
 * The rule that matters: the tag itself is only a claim. Only a digest whose
 * provenance says "this repository's CI built this commit's tree" may be
 * deployed, so a wrong-tree image can never reach the cluster.
 *
 * Pure decision logic (`planPromotion`) takes an injected `exec`, so it is
 * tested without a registry. The CLI writes the action outputs:
 *   prebuilt_digests   `<image>=sha256:…,<image>=sha256:…` (empty on fallback)
 *   mode               `prebuilt` or `build`
 *   IMAGE_PROMOTION    same mode, for the deploy job summary
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import process from "node:process";
import { pathToFileURL } from "node:url";

/** Public GHCR namespace of the monorepo service images. */
export const REGISTRY = "ghcr.io/proflyder";
/** Image name characters accepted from a unit map / CLI argument. */
const IMAGE_NAME = /^[a-z0-9][a-z0-9._-]*$/;
const DIGEST = /^sha256:[0-9a-f]{64}$/;
const TREE = /^[0-9a-f]{40}$/;
const COMMIT = /^[0-9a-f]{7,40}$/;
/** Tags that must never be recreated from a promoted digest here. */
export const PROMOTED_TAGS = ["develop"];

export function treeTag(tree) {
  if (!TREE.test(String(tree || ""))) {
    throw new Error(`tree must be a full 40-hex git tree hash, got '${tree}'`);
  }
  return `tree-${tree}`;
}

export function imageRef(image) {
  if (!IMAGE_NAME.test(String(image || ""))) {
    throw new Error(`invalid image name '${image}'`);
  }
  return `${REGISTRY}/${image}`;
}

/** Normalise a digest string, or return null when it is not a sha256 digest. */
export function parseDigest(value) {
  const text = String(value || "").trim();
  return DIGEST.test(text) ? text : null;
}

/**
 * Certificate identity of the workflows allowed to sign image provenance.
 *
 * The attestation is created inside the reusable `_images.yml`, so its SAN is
 * `<caller ref>` of the reusable workflow file. Restricting both the workflow
 * file and the repository keeps a manual push, a fork run or an unrelated
 * workflow from satisfying verification.
 */
export function signerIdentityPattern(repository) {
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(String(repository || ""))) {
    throw new Error(`invalid GitHub repository '${repository}' (expected owner/repo)`);
  }
  const escaped = repository.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return `^https://github\\.com/${escaped}/\\.github/workflows/(ci|_images)\\.yml@`;
}

export function verifyArgs({ repository, reference }) {
  return [
    "attestation",
    "verify",
    `oci://${reference}`,
    "--repo",
    repository,
    "--signer-repo",
    repository,
    "--cert-identity-regex",
    signerIdentityPattern(repository),
  ];
}

function message(error) {
  const text = String(error?.message || error || "");
  return text.split("\n").find((line) => line.trim()) || "unknown error";
}

function uniqueImages(images) {
  const seen = new Set();
  const result = [];
  for (const raw of images) {
    const image = String(raw || "").trim();
    if (!image) {
      continue;
    }
    if (!IMAGE_NAME.test(image)) {
      throw new Error(`invalid image name '${image}'`);
    }
    if (!seen.has(image)) {
      seen.add(image);
      result.push(image);
    }
  }
  if (!result.length) {
    throw new Error("no images to promote");
  }
  return result;
}

/**
 * Decide and perform the promotion. `exec(command, args)` returns stdout and
 * throws on a non-zero exit; it is injected so tests never touch a registry.
 */
export function planPromotion({ images, tree, commit, repository, exec }) {
  const tag = treeTag(tree);
  if (!COMMIT.test(String(commit || ""))) {
    throw new Error(`commit must be a git sha, got '${commit}'`);
  }
  signerIdentityPattern(repository); // validate before any registry call
  const names = uniqueImages(images);
  const decisions = [];
  const verified = [];

  // Phase 1: resolve and verify every image before tagging anything, so a
  // missing/unverified image cannot leave a half-promoted unit behind.
  for (const image of names) {
    const ref = imageRef(image);
    let digest;
    try {
      digest = parseDigest(exec("crane", ["digest", `${ref}:${tag}`]));
    } catch (error) {
      decisions.push({ image, status: "missing", detail: `${tag} not found: ${message(error)}` });
      continue;
    }
    if (!digest) {
      decisions.push({ image, status: "missing", detail: `${tag} did not resolve to a sha256 digest` });
      continue;
    }
    try {
      exec("gh", verifyArgs({ repository, reference: `${ref}@${digest}` }));
    } catch (error) {
      decisions.push({
        image,
        status: "unverified",
        digest,
        detail: `provenance check failed: ${message(error)}`,
      });
      continue;
    }
    verified.push({ image, ref, digest });
    decisions.push({ image, status: "verified", digest });
  }

  if (verified.length !== names.length) {
    return {
      mode: "build",
      tag,
      digests: {},
      decisions,
      reason: "not every image is available and verified",
    };
  }

  // Phase 2: attach the usual tags to the verified digests and prove each tag
  // still resolves to the same digest (a rewritten manifest would differ).
  for (const item of verified) {
    for (const destination of [String(commit), ...PROMOTED_TAGS]) {
      try {
        exec("crane", ["tag", `${item.ref}@${item.digest}`, destination]);
        const actual = parseDigest(exec("crane", ["digest", `${item.ref}:${destination}`]));
        if (actual !== item.digest) {
          throw new Error(`${destination} resolves to ${actual || "an unknown digest"}`);
        }
      } catch (error) {
        decisions.find((entry) => entry.image === item.image).status = "tag-failed";
        decisions.find((entry) => entry.image === item.image).detail =
          `could not tag ${destination}: ${message(error)}`;
        return {
          mode: "build",
          tag,
          digests: {},
          decisions,
          reason: `tagging failed for ${item.image}`,
        };
      }
    }
    decisions.find((entry) => entry.image === item.image).status = "promoted";
  }

  return {
    mode: "prebuilt",
    tag,
    digests: Object.fromEntries(verified.map((item) => [item.image, item.digest])),
    decisions,
  };
}

/** `name=sha256:…,name=sha256:…` (empty on fallback). */
export function formatPrebuilt(digests) {
  return Object.entries(digests)
    .map(([image, digest]) => `${image}=${digest}`)
    .join(",");
}

export function formatSummary(result) {
  const lines = [`### image promotion (${result.tag})`, ""];
  for (const decision of result.decisions) {
    const digest = decision.digest ? ` \`${decision.digest}\`` : "";
    const detail = decision.detail ? ` — ${decision.detail}` : "";
    lines.push(`- \`${decision.image}\`: ${decision.status}${digest}${detail}`);
  }
  lines.push("", `- path: \`${result.mode}\``);
  if (result.mode !== "prebuilt") {
    lines.push(`- fallback: ${result.reason || "promotion unavailable"}; building images`);
  }
  return `${lines.join("\n")}\n`;
}

function parseArgs(argv) {
  const args = { images: [], tree: "", commit: "", repository: "" };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    const value = argv[index + 1];
    switch (flag) {
      case "--images":
        args.images = String(value || "").split(/[\s,]+/).filter(Boolean);
        index += 1;
        break;
      case "--tree":
        args.tree = value;
        index += 1;
        break;
      case "--commit":
        args.commit = value;
        index += 1;
        break;
      case "--repository":
        args.repository = value;
        index += 1;
        break;
      case "--help":
        args.help = true;
        break;
      default:
        throw new Error(`unrecognized argument: ${flag}`);
    }
  }
  return args;
}

function appendFile(path, text) {
  if (!path) {
    return;
  }
  fs.appendFileSync(path, text, "utf8");
}

export function runPromotion({ args, exec, env = process.env, log = console.log }) {
  const repository = args.repository || env.GITHUB_REPOSITORY || "";
  log(`promoting ${args.images.join(", ")} for tree ${args.tree} (${repository})`);
  // Invalid input throws and fails the step: the fallback below is only for
  // missing or unverifiable images, never for a malformed tree/commit.
  const result = planPromotion({
    images: args.images,
    tree: args.tree,
    commit: args.commit,
    repository,
    exec,
  });
  log(formatSummary(result));
  if (env.GITHUB_OUTPUT) {
    appendFile(env.GITHUB_OUTPUT, `prebuilt_digests=${formatPrebuilt(result.digests)}\n`);
    appendFile(env.GITHUB_OUTPUT, `mode=${result.mode}\n`);
  }
  if (env.GITHUB_ENV) {
    appendFile(env.GITHUB_ENV, `IMAGE_PROMOTION=${result.mode}\n`);
  }
  if (env.GITHUB_STEP_SUMMARY) {
    appendFile(env.GITHUB_STEP_SUMMARY, formatSummary(result));
  }
  return result;
}

export function spawnExec(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    env: process.env,
    ...options,
  });
  if (result.error) {
    throw new Error(`${command}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    const stderr = String(result.stderr || "").trim();
    throw new Error(`${command} ${args.join(" ")}: ${stderr || `exit ${result.status}`}`);
  }
  return String(result.stdout || "");
}

export function main(argv = process.argv.slice(2)) {
  const args = parseArgs(argv);
  if (args.help) {
    console.log(
      "usage: promote.mjs --images 'a b' --tree <40-hex> --commit <sha> " +
        "[--repository owner/repo]",
    );
    return 0;
  }
  runPromotion({ args, exec: spawnExec });
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    process.exitCode = main();
  } catch (error) {
    console.error(error?.stack || String(error));
    process.exitCode = 1;
  }
}
