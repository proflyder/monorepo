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
 * Native units (Pulsar firmware, the Windows logon provider) follow the same
 * rule with one artifact instead of an image set: their PR build publishes the
 * release-shaped OCI artifact under `tree-<hash>`, and `planNativePromotion`
 * verifies it and attaches the unit's release tags (`{version}`, `latest`,
 * `sha-{commit12}`) to the same digest.
 *
 * Immutable tags (`{version}`, `sha-{commit12}`) are immutable by CONTENT, not
 * by manifest digest: two pushes of identical layers differ only in manifest
 * annotations (`created`, `revision`), so a digest comparison would report a
 * false conflict. An existing tag whose artifact type and ordered
 * `(title, digest, mediaType)` layers equal the candidate's is left where it
 * is (no retag) and the promotion succeeds. Different content is a hard
 * failure, never an overwrite. `latest` moves to the version tag's digest, so
 * `latest` and `<version>` always agree even when the version tag was kept.
 *
 * Pure decision logic (`planPromotion`/`planNativePromotion`) takes an injected
 * `exec`, so it is tested without a registry. The CLI writes the action outputs:
 *   prebuilt_digests   `<image>=sha256:…,<image>=sha256:…` (empty on fallback)
 *   mode               `prebuilt` or `build` (native: `promoted` or `build`)
 *   digest             the promoted native artifact digest
 *   IMAGE_PROMOTION / NATIVE_PROMOTION   same mode, for the deploy job summary
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

/**
 * Signer of a native artifact's provenance: the same reusable-workflow rule as
 * images, limited to `_native.yml` (the job that pushes and attests it).
 */
export function nativeSignerIdentityPattern(repository) {
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(String(repository || ""))) {
    throw new Error(`invalid GitHub repository '${repository}' (expected owner/repo)`);
  }
  const escaped = repository.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return `^https://github\\.com/${escaped}/\\.github/workflows/(ci|_native)\\.yml@`;
}

export function verifyArgs({ repository, reference, signerPattern = null }) {
  return [
    "attestation",
    "verify",
    `oci://${reference}`,
    "--repo",
    repository,
    // gh refuses --signer-repo together with --cert-identity-regex; the regex
    // already pins both the repository and the signing workflow.
    "--cert-identity-regex",
    signerPattern || signerIdentityPattern(repository),
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

// --------------------------------------------------------------------------
// Native artifact promotion (DECISIONS 36, native units)
// --------------------------------------------------------------------------

/** Full GHCR reference of a native publish repository. */
const NATIVE_REPOSITORY = /^ghcr\.io\/[a-z0-9.-]+\/[a-z0-9._-]+$/;
/** Destination tag characters accepted from units.mjs / CLI. */
const TAG = /^[A-Za-z0-9_][A-Za-z0-9._-]{0,127}$/;
const VERSION = /^v?[0-9]+\.[0-9]+\.[0-9]+$/;

export function nativeRepository(repository) {
  const text = String(repository || "").trim();
  if (!NATIVE_REPOSITORY.test(text)) {
    throw new Error(`invalid native repository '${text}' (expected ghcr.io/owner/name)`);
  }
  return text;
}

/** Expand `{version}` and `{commit12}` in the unit's publish tag templates. */
export function expandTags(templates, { version, commit }) {
  if (!VERSION.test(String(version || ""))) {
    throw new Error(`version must be MAJOR.MINOR.PATCH or vMAJOR.MINOR.PATCH, got '${version}'`);
  }
  if (!COMMIT.test(String(commit || ""))) {
    throw new Error(`commit must be a git sha, got '${commit}'`);
  }
  const expanded = [];
  for (const raw of templates || []) {
    const tag = String(raw || "")
      .replaceAll("{version}", version)
      .replaceAll("{commit12}", commit.slice(0, 12));
    if (!TAG.test(tag)) {
      throw new Error(`invalid publish tag '${tag}' (from '${raw}')`);
    }
    if (!expanded.includes(tag)) {
      expanded.push(tag);
    }
  }
  if (!expanded.length) {
    throw new Error("no publish tags to attach");
  }
  return expanded;
}

/** Layer/descriptor title annotation used by `oras push` for a file name. */
const TITLE_ANNOTATION = "org.opencontainers.image.title";

/**
 * Reduce an OCI manifest/index to the fields that define artifact identity:
 * its artifact type and the ordered `(title, digest, mediaType)` descriptors.
 * Manifest-level annotations (`created`, `revision`) are deliberately ignored:
 * a rebuild of the same layers must compare equal, as the publish scripts do
 * with `SHA256SUMS` content.
 */
export function manifestContent(text) {
  let parsed;
  try {
    parsed = JSON.parse(String(text || ""));
  } catch (error) {
    throw new Error(`manifest is not valid JSON: ${message(error)}`);
  }
  if (!parsed || typeof parsed !== "object") {
    throw new Error("manifest is not a JSON object");
  }
  const artifactType = String(parsed.artifactType || parsed.mediaType || "");
  const descriptors = Array.isArray(parsed.layers)
    ? parsed.layers
    : Array.isArray(parsed.manifests)
      ? parsed.manifests
      : null;
  if (!descriptors) {
    throw new Error("manifest has neither layers nor manifests");
  }
  return {
    artifactType,
    layers: descriptors.map((layer) => ({
      title: String(layer?.annotations?.[TITLE_ANNOTATION] || ""),
      digest: String(layer?.digest || "").trim(),
      mediaType: String(layer?.mediaType || ""),
    })),
  };
}

/** True when two manifests describe the same artifact content. */
export function sameManifestContent(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

/** Read and normalise a manifest; a missing tag is reported by the caller. */
function readManifest(exec, reference) {
  return manifestContent(exec("crane", ["manifest", reference]));
}

/**
 * Promote a PR-built native artifact from `tree-<hash>` to the unit's release
 * tags. Releases are immutable by content: an existing `<version>` or
 * `sha-<commit>` tag with the same artifact type and layers is kept as-is
 * (no retag) even when its manifest digest differs, while different content is
 * a hard failure. `latest` is the only tag that moves; it follows the version
 * tag's digest so the station installs exactly that release.
 */
export function planNativePromotion({
  repository,
  tree,
  commit,
  version,
  tags,
  githubRepository = process.env.GITHUB_REPOSITORY,
  exec,
}) {
  const tag = treeTag(tree);
  const ref = nativeRepository(repository);
  const destinations = expandTags(tags, { version, commit });
  const signer = nativeSignerIdentityPattern(githubRepository);
  const decisions = [];

  let digest;
  try {
    digest = parseDigest(exec("crane", ["digest", `${ref}:${tag}`]));
  } catch (error) {
    return {
      mode: "build",
      tag,
      repository: ref,
      digest: null,
      tags: destinations,
      decisions: [
        { tag, status: "missing", detail: `${tag} not found: ${message(error)}` },
      ],
      reason: `${tag} is missing`,
    };
  }
  if (!digest) {
    return {
      mode: "build",
      tag,
      repository: ref,
      digest: null,
      tags: destinations,
      decisions: [{ tag, status: "missing", detail: `${tag} did not resolve to a sha256 digest` }],
      reason: `${tag} did not resolve`,
    };
  }

  try {
    exec(
      "gh",
      verifyArgs({ repository: githubRepository, reference: `${ref}@${digest}`, signerPattern: signer }),
    );
  } catch (error) {
    return {
      mode: "build",
      tag,
      repository: ref,
      digest,
      tags: destinations,
      decisions: [
        { tag, status: "unverified", digest, detail: `provenance check failed: ${message(error)}` },
      ],
      reason: "provenance did not verify",
    };
  }
  decisions.push({ tag, status: "verified", digest });

  // Content identity of the candidate, read once. Immutable tags are compared
  // against this instead of their manifest digest: a rebuild of the same files
  // produces different manifest annotations but the same layers.
  const candidate = readManifest(exec, `${ref}@${digest}`);

  // The tag whose digest `latest` must follow: the version tag, whether it was
  // created now or kept from an earlier push. Without a version tag, `latest`
  // tracks the candidate itself.
  const versionTag = expandTags(["{version}"], { version, commit })[0];
  let releaseDigest = null;

  /** Keep/tag one immutable release tag and return the digest it now points to. */
  const applyImmutable = (destination) => {
    let existing = null;
    try {
      existing = parseDigest(exec("crane", ["digest", `${ref}:${destination}`]));
    } catch {
      existing = null;
    }
    if (existing === digest) {
      decisions.push({ tag: destination, status: "unchanged", digest });
      return digest;
    }
    if (existing) {
      const existingContent = readManifest(exec, `${ref}:${destination}`);
      if (sameManifestContent(existingContent, candidate)) {
        decisions.push({
          tag: destination,
          status: "unchanged",
          digest: existing,
          detail: "already published with identical content",
        });
        return existing;
      }
      throw new Error(
        `${ref}:${destination} already exists with ${existing}, not ${digest}; ` +
          "release tags are immutable by content — bump the version",
      );
    }
    exec("crane", ["tag", `${ref}@${digest}`, destination]);
    const actual = parseDigest(exec("crane", ["digest", `${ref}:${destination}`]));
    if (actual !== digest) {
      throw new Error(
        `${ref}:${destination} resolves to ${actual || "an unknown digest"}, not ${digest}`,
      );
    }
    decisions.push({ tag: destination, status: "tagged", digest });
    return digest;
  };

  // Immutable tags first, so `latest` follows the version digest even when the
  // version tag already existed with identical content but another digest.
  for (const destination of destinations) {
    if (destination === "latest") {
      continue;
    }
    const effective = applyImmutable(destination);
    if (destination === versionTag) {
      releaseDigest = effective;
    }
  }

  if (destinations.includes("latest")) {
    const target = releaseDigest || digest;
    let existingLatest = null;
    try {
      existingLatest = parseDigest(exec("crane", ["digest", `${ref}:latest`]));
    } catch {
      existingLatest = null;
    }
    if (existingLatest === target) {
      decisions.push({ tag: "latest", status: "unchanged", digest: target });
    } else {
      exec("crane", ["tag", `${ref}@${target}`, "latest"]);
      const actual = parseDigest(exec("crane", ["digest", `${ref}:latest`]));
      if (actual !== target) {
        throw new Error(`${ref}:latest resolves to ${actual || "an unknown digest"}, not ${target}`);
      }
      decisions.push({ tag: "latest", status: existingLatest ? "moved" : "tagged", digest: target });
    }
  }

  return {
    mode: "promoted",
    tag,
    repository: ref,
    digest: releaseDigest || digest,
    tags: destinations,
    decisions,
  };
}

export function formatNativeSummary(result) {
  const lines = [`### native promotion (${result.tag})`, ""];
  lines.push(`- repository: \`${result.repository}\``);
  for (const decision of result.decisions) {
    const digest = decision.digest ? ` \`${decision.digest}\`` : "";
    const detail = decision.detail ? ` — ${decision.detail}` : "";
    lines.push(`- \`${decision.tag}\`: ${decision.status}${digest}${detail}`);
  }
  lines.push("", `- path: \`${result.mode}\``);
  if (result.mode !== "promoted") {
    lines.push(`- fallback build: ${result.reason}; building and publishing as before`);
  }
  return `${lines.join("\n")}\n`;
}

export function runNativePromotion({ args, exec, env = process.env, log = console.log }) {
  const repository = nativeRepository(args.repository);
  log(`promoting native ${repository} for tree ${args.tree}`);
  const result = planNativePromotion({
    repository,
    tree: args.tree,
    commit: args.commit,
    version: args.version,
    tags: args.tags,
    githubRepository: args.githubRepository || env.GITHUB_REPOSITORY,
    exec,
  });
  log(formatNativeSummary(result));
  if (env.GITHUB_OUTPUT) {
    appendFile(env.GITHUB_OUTPUT, `mode=${result.mode}\n`);
    appendFile(env.GITHUB_OUTPUT, `digest=${result.digest || ""}\n`);
  }
  if (env.GITHUB_ENV) {
    appendFile(env.GITHUB_ENV, `NATIVE_PROMOTION=${result.mode}\n`);
  }
  if (env.GITHUB_STEP_SUMMARY) {
    appendFile(env.GITHUB_STEP_SUMMARY, formatNativeSummary(result));
  }
  return result;
}

function parseArgs(argv) {
  const args = {
    images: [],
    tree: "",
    commit: "",
    repository: "",
    native: false,
    version: "",
    tags: [],
    githubRepository: "",
  };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    const value = argv[index + 1];
    switch (flag) {
      case "--native":
        args.native = true;
        break;
      case "--version":
        args.version = value;
        index += 1;
        break;
      case "--tags":
        args.tags = String(value || "").split(/[\s,]+/).filter(Boolean);
        index += 1;
        break;
      case "--github-repository":
        args.githubRepository = value;
        index += 1;
        break;
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
        "[--repository owner/repo]\n" +
        "   or: promote.mjs --native --repository ghcr.io/owner/name --tree <40-hex> " +
        "--commit <sha> --version <vX.Y.Z> --tags '{version} latest'",
    );
    return 0;
  }
  if (args.native) {
    runNativePromotion({ args, exec: spawnExec });
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
