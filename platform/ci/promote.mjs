#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import process from "node:process";
import { pathToFileURL } from "node:url";

import { compute as computeAffected } from "./affected.mjs";
import { VERSION_ANNOTATION } from "./version.mjs";

export const REGISTRY = "ghcr.io/proflyder";
const IMAGE_NAME = /^[a-z0-9][a-z0-9._-]*$/;
const DIGEST = /^sha256:[0-9a-f]{64}$/;
const TREE = /^[0-9a-f]{40}$/;
const COMMIT = /^[0-9a-f]{7,40}$/;
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

export function parseDigest(value) {
  const text = String(value || "").trim();
  return DIGEST.test(text) ? text : null;
}

export function signerIdentityPattern(repository) {
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(String(repository || ""))) {
    throw new Error(`invalid GitHub repository '${repository}' (expected owner/repo)`);
  }
  const escaped = repository.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return `^https://github\\.com/${escaped}/\\.github/workflows/(ci|_images|_unit|_python|_attest)\\.yml@`;
}

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

export function historyEntry(entry) {
  const commit = String(entry?.commit || "");
  const tree = String(entry?.tree || "");
  if (!COMMIT.test(commit) || !TREE.test(tree)) {
    throw new Error(`invalid history entry '${commit} ${tree}'`);
  }
  return { commit, tree };
}

export function readHistory(exec, limit = 200) {
  const count = Number.isInteger(limit) && limit > 0 ? Math.min(limit, 1000) : 200;
  const output = exec("git", [
    "log",
    "--first-parent",
    `-n${count}`,
    "--format=%H %T",
    "HEAD",
  ]);
  return String(output)
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const [commit, tree] = line.split(/\s+/);
      return historyEntry({ commit, tree });
    });
}

export function pullRequestNumber(text) {
  const subject = String(text || "").split("\n")[0].trim();
  const match = /\(#([0-9]+)\)\s*$/.exec(subject);
  if (!match) {
    return null;
  }
  const number = Number(match[1]);
  return Number.isInteger(number) && number > 0 ? number : null;
}

const FULL_SHA = /^[0-9a-f]{40}$/;

function pullEntry(pull, repository) {
  const number = Number(pull?.number);
  const head = String(pull?.head?.sha || "");
  if (!Number.isInteger(number) || number <= 0 || !FULL_SHA.test(head)) {
    return null;
  }
  const headRepository = String(pull?.head?.repo?.full_name || "");
  return {
    number,
    head,
    fork: Boolean(headRepository && headRepository !== repository),
  };
}

export function pullRequestForCommit(exec, { repository, commit, message = "" }) {
  try {
    const pulls = JSON.parse(exec("gh", ["api", `repos/${repository}/commits/${commit}/pulls`]));
    for (const pull of Array.isArray(pulls) ? pulls : []) {
      const entry = pullEntry(pull, repository);
      if (entry) {
        return entry;
      }
    }
  } catch {
  }
  const number = pullRequestNumber(message);
  if (!number) {
    return null;
  }
  try {
    return pullEntry(JSON.parse(exec("gh", ["api", `repos/${repository}/pulls/${number}`])), repository);
  } catch {
    return null;
  }
}

export function githubRepositoryFor(args = {}, env = process.env) {
  const explicit = String(args.githubRepository || "").trim();
  if (explicit) {
    return explicit;
  }
  if (!args.native) {
    return String(args.repository || env.GITHUB_REPOSITORY || "").trim();
  }
  return String(env.GITHUB_REPOSITORY || "").trim();
}

export async function resolvePrHeadDelta({
  exec,
  compute,
  repository,
  commit,
  unit,
  commitMessage = "",
}) {
  try {
    signerIdentityPattern(repository);
  } catch (error) {
    return { error: message(error) };
  }
  const pull = pullRequestForCommit(exec, { repository, commit, message: commitMessage });
  if (!pull) {
    return { error: `no pull request found for ${String(commit).slice(0, 12)}` };
  }
  if (pull.fork) {
    return { error: `#${pull.number} comes from a fork; fork CI never publishes tree tags` };
  }
  try {
    exec("git", ["fetch", "--no-tags", "origin", `refs/pull/${pull.number}/head`]);
  } catch (error) {
    return { error: `cannot fetch refs/pull/${pull.number}/head: ${message(error)}` };
  }
  let head;
  try {
    head = String(exec("git", ["rev-parse", "FETCH_HEAD^{commit}"])).trim();
  } catch (error) {
    return { error: `cannot resolve refs/pull/${pull.number}/head: ${message(error)}` };
  }
  if (head !== pull.head) {
    return {
      error:
        `refs/pull/${pull.number}/head is ${String(head).slice(0, 12)}, ` +
        `but the API reports ${pull.head.slice(0, 12)}`,
    };
  }
  let headTree;
  try {
    headTree = String(exec("git", ["rev-parse", `${head}^{tree}`])).trim();
  } catch (error) {
    return { error: `cannot resolve the tree of ${head.slice(0, 12)}: ${message(error)}` };
  }
  if (!TREE.test(headTree)) {
    return { error: `cannot resolve the tree of ${head.slice(0, 12)}` };
  }
  let changed;
  try {
    changed = String(exec("git", ["diff", "--name-only", "--no-renames", head, commit]))
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean)
      .sort();
  } catch (error) {
    return {
      error: `cannot diff ${head.slice(0, 12)}..${commit.slice(0, 12)}: ${message(error)}`,
    };
  }
  const result = { number: pull.number, head, tree: headTree, changed, affectsUnit: false };
  if (!changed.length) {
    return result;
  }
  let computed;
  try {
    computed = await compute([
      "--base",
      head,
      "--head",
      commit,
      "--format",
      "json",
      ...changed.flatMap((file) => ["--changed-file", file]),
    ]);
  } catch (error) {
    return {
      error:
        `cannot route the delta ${head.slice(0, 12)}..${commit.slice(0, 12)}: ` +
        `${message(error)}`,
    };
  }
  const selected = computed?.result?.deploySelected;
  const natives = computed?.entries?.native ?? [];
  result.affectsUnit =
    Boolean(selected && typeof selected.has === "function" && selected.has(unit)) ||
    natives.some((entry) => entry.id === unit);
  result.changed = computed?.result?.changed ?? changed;
  return result;
}

export async function preparePrHead({ args, exec, compute = computeAffected, unit = "", env = process.env }) {
  try {
    const repository = githubRepositoryFor(args, env);
    signerIdentityPattern(repository);
    const commitMessage = exec("git", ["log", "-1", "--format=%B", args.commit]);
    return await resolvePrHeadDelta({
      exec,
      compute,
      repository,
      commit: args.commit,
      unit: args.unit || unit,
      commitMessage,
    });
  } catch (error) {
    return { error: message(error) };
  }
}

export function planPromotion({
  images,
  optionalImages = [],
  tree,
  commit,
  repository,
  exec,
  history = [],
  allowHistory = false,
  allowPrHead = false,
  prHead = null,
  unit = "",
}) {
  const tag = treeTag(tree);
  if (!COMMIT.test(String(commit || ""))) {
    throw new Error(`commit must be a git sha, got '${commit}'`);
  }
  signerIdentityPattern(repository);
  const required = uniqueImages(images);
  const optional = (optionalImages.length ? uniqueImages(optionalImages) : []).filter(
    (image) => !required.includes(image),
  );
  const lineage = history.map(historyEntry);
  const decisions = [];
  const verified = [];
  const tagSets = new Map();

  const knownTags = (ref) => {
    if (!tagSets.has(ref)) {
      try {
        tagSets.set(
          ref,
          new Set(
            String(exec("crane", ["ls", ref]))
              .split("\n")
              .map((line) => line.trim())
              .filter(Boolean),
          ),
        );
      } catch {
        tagSets.set(ref, null);
      }
    }
    return tagSets.get(ref);
  };

  const resolveTag = (ref, candidateTag) => {
    let digest;
    try {
      digest = parseDigest(exec("crane", ["digest", `${ref}:${candidateTag}`]));
    } catch (error) {
      return { error: "missing", detail: `${candidateTag} not found: ${message(error)}` };
    }
    if (!digest) {
      return { error: "missing", detail: `${candidateTag} did not resolve to a sha256 digest` };
    }
    try {
      exec("gh", verifyArgs({ repository, reference: `${ref}@${digest}` }));
    } catch (error) {
      return {
        error: "unverified",
        digest,
        detail: `provenance check failed for ${candidateTag}: ${message(error)}`,
      };
    }
    return { digest };
  };

  const resolveFromHistory = (ref) => {
    const tags = knownTags(ref);
    for (const entry of lineage) {
      if (entry.tree === tree) {
        continue;
      }
      const candidateTag = `tree-${entry.tree}`;
      if (tags && !tags.has(candidateTag)) {
        continue;
      }
      const resolved = resolveTag(ref, candidateTag);
      if (!resolved.error) {
        return { digest: resolved.digest, source: entry.commit, sourceTree: entry.tree };
      }
    }
    return null;
  };

  const usablePrHead = () =>
    prHead && !prHead.error && !prHead.affectsUnit ? prHead : null;

  const deltaDetail = (head) =>
    `from PR head ${String(head).slice(0, 12)}, delta to ${String(commit).slice(0, 12)} ` +
    `does not affect ${unit || "the unit"}`;

  const resolveFromPrHead = (ref) => {
    const state = usablePrHead();
    if (!state) {
      return null;
    }
    const candidateTag = `tree-${state.tree}`;
    const resolved = resolveTag(ref, candidateTag);
    if (resolved.error) {
      return {
        error:
          `${candidateTag} (PR #${state.number} head ${state.head.slice(0, 12)}): ` +
          resolved.detail,
      };
    }
    return {
      digest: resolved.digest,
      source: state.head,
      sourceTree: state.tree,
      detail: deltaDetail(state.head),
    };
  };

  for (const [image, isOptional] of [
    ...required.map((image) => [image, false]),
    ...optional.map((image) => [image, true]),
  ]) {
    const ref = imageRef(image);
    const fallback = isOptional || allowHistory;
    const current = resolveTag(ref, tag);
    let resolved = null;
    let detail = current.detail;
    if (!current.error) {
      resolved = { digest: current.digest, source: commit, sourceTree: tree };
    } else {
      const steps = isOptional ? ["history", "pr-head"] : ["pr-head", "history"];
      for (const step of steps) {
        if (step === "pr-head" && allowPrHead) {
          const fromPrHead = resolveFromPrHead(ref);
          if (fromPrHead && fromPrHead.digest) {
            resolved = fromPrHead;
            break;
          }
          if (fromPrHead && fromPrHead.error) {
            detail = fromPrHead.error;
          }
        }
        if (step === "history" && fallback) {
          const previous = resolveFromHistory(ref);
          if (previous) {
            resolved = previous;
            break;
          }
          detail = `${tag} is not available and no verified tree-<hash> exists in the last ${lineage.length} first-parent commits`;
        }
      }
      if (!resolved && allowPrHead && prHead) {
        if (prHead.error) {
          detail = `${tag} is not available; the PR head fallback failed: ${prHead.error}`;
        } else if (prHead.affectsUnit) {
          detail =
            `${tag} is not available; the delta from PR head ${prHead.head.slice(0, 12)} to ` +
            `${String(commit).slice(0, 12)} changes ${unit || "this unit"} — bring develop ` +
            "into the PR, wait for CI and merge again, or workflow_dispatch after the next merge";
        }
      }
    }
    if (!resolved) {
      decisions.push({
        image,
        optional: isOptional,
        status: current.error || "missing",
        digest: current.digest || null,
        detail,
        source: null,
      });
      continue;
    }
    verified.push({ image, ref, digest: resolved.digest, source: resolved.source, optional: isOptional });
    decisions.push({
      image,
      optional: isOptional,
      status: "verified",
      digest: resolved.digest,
      source: resolved.source,
      detail:
        resolved.detail ??
        (resolved.source === commit
          ? "current tree"
          : `newest verified build in history (tree-${resolved.sourceTree})`),
    });
  }

  if (verified.length !== required.length + optional.length) {
    let reason = "not every image is available and verified";
    if (allowPrHead && prHead && !prHead.error && prHead.affectsUnit) {
      reason =
        `the delta from PR #${prHead.number} head ${prHead.head.slice(0, 12)} to ` +
        `${String(commit).slice(0, 12)} changes ${unit || "this unit"} — bring develop into ` +
        "the PR, wait for CI and merge again (or workflow_dispatch after the next merge)";
    } else if (allowPrHead && prHead?.error) {
      reason = `this tree has no verified build and the PR head fallback failed: ${prHead.error}`;
    }
    return {
      mode: "build",
      tag,
      digests: {},
      decisions,
      reason,
    };
  }

  for (const item of verified.filter((entry) => !entry.optional)) {
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
  for (const item of verified.filter((entry) => entry.optional)) {
    decisions.find((entry) => entry.image === item.image).status = "reused";
  }

  return {
    mode: "prebuilt",
    tag,
    digests: Object.fromEntries(verified.map((item) => [item.image, item.digest])),
    decisions,
  };
}

export function formatPrebuilt(digests) {
  return Object.entries(digests)
    .map(([image, digest]) => `${image}=${digest}`)
    .join(",");
}

export function formatSummary(result) {
  const lines = [`### image promotion (${result.tag})`, ""];
  for (const decision of result.decisions) {
    const digest = decision.digest ? ` \`${decision.digest}\`` : "";
    const source =
      decision.status === "verified" || decision.status === "promoted" || decision.status === "reused"
        ? decision.source
          ? ` (from ${String(decision.source).slice(0, 12)})`
          : ""
        : "";
    const optional = decision.optional ? " [sibling]" : "";
    const detail = decision.detail ? ` — ${decision.detail}` : "";
    lines.push(`- \`${decision.image}\`${optional}: ${decision.status}${digest}${source}${detail}`);
  }
  lines.push("", `- path: \`${result.mode}\``);
  if (result.mode !== "prebuilt") {
    lines.push(
      `- not deployable: ${result.reason || "promotion unavailable"}; ` +
        "deploy-home does not build (DECISIONS 36)",
    );
  }
  return `${lines.join("\n")}\n`;
}

const NATIVE_REPOSITORY = /^ghcr\.io\/[a-z0-9.-]+\/[a-z0-9._-]+$/;
const TAG = /^[A-Za-z0-9_][A-Za-z0-9._-]{0,127}$/;
const VERSION = /^v?[0-9]+\.[0-9]+\.[0-9]+$/;

export function nativeRepository(repository) {
  const text = String(repository || "").trim();
  if (!NATIVE_REPOSITORY.test(text)) {
    throw new Error(`invalid native repository '${text}' (expected ghcr.io/owner/name)`);
  }
  return text;
}

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

const TITLE_ANNOTATION = "org.opencontainers.image.title";
export { VERSION_ANNOTATION };

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

export function sameManifestContent(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function readManifest(exec, reference) {
  return manifestContent(exec("crane", ["manifest", reference]));
}

export function manifestAnnotations(text) {
  let parsed;
  try {
    parsed = JSON.parse(String(text || ""));
  } catch (error) {
    throw new Error(`manifest is not valid JSON: ${message(error)}`);
  }
  const annotations = parsed?.annotations;
  return annotations && typeof annotations === "object" ? annotations : {};
}

function readManifestWithAnnotations(exec, reference) {
  const text = exec("crane", ["manifest", reference]);
  return { content: manifestContent(text), annotations: manifestAnnotations(text) };
}

export function planNativePromotion({
  repository,
  tree,
  commit,
  version,
  tags,
  githubRepository = process.env.GITHUB_REPOSITORY,
  exec,
  allowPrHead = false,
  prHead = null,
  unit = "",
  allowRetag = false,
} = {}) {
  const tag = treeTag(tree);
  const ref = nativeRepository(repository);
  const expand = (value) =>
    VERSION.test(String(value || "")) ? expandTags(tags, { version: value, commit }) : [];
  let destinations = expand(version);
  const signer = nativeSignerIdentityPattern(githubRepository);
  const decisions = [];

  let digest = null;
  let artifactTag = tag;
  let source = commit;
  let deltaNote = "";
  let failure = null;
  try {
    digest = parseDigest(exec("crane", ["digest", `${ref}:${tag}`]));
    if (!digest) {
      failure = `${tag} did not resolve to a sha256 digest`;
    }
  } catch (error) {
    failure = `${tag} not found: ${message(error)}`;
  }
  if (failure && allowPrHead && prHead) {
    if (prHead.error) {
      failure = `${tag} is missing and the PR head fallback failed: ${prHead.error}`;
    } else if (prHead.affectsUnit) {
      failure =
        `${tag} is missing and the delta from PR head ${prHead.head.slice(0, 12)} to ` +
        `${String(commit).slice(0, 12)} changes ${unit || "this unit"}; bring develop into ` +
        "the PR, wait for CI and merge again (or workflow_dispatch after the next merge)";
    } else {
      artifactTag = treeTag(prHead.tree);
      source = prHead.head;
      deltaNote =
        `from PR head ${prHead.head.slice(0, 12)}, delta to ${String(commit).slice(0, 12)} ` +
        `does not affect ${unit || "the unit"}`;
      try {
        digest = parseDigest(exec("crane", ["digest", `${ref}:${artifactTag}`]));
        failure = digest
          ? null
          : `${artifactTag} did not resolve to a sha256 digest`;
      } catch (error) {
        failure = `${artifactTag} (PR #${prHead.number} head ${prHead.head.slice(0, 12)}) not found: ${message(error)}`;
      }
    }
  }
  if (failure || !digest) {
    return {
      mode: "build",
      tag,
      repository: ref,
      digest: null,
      tags: destinations,
      decisions: [{ tag, status: "missing", detail: failure || `${tag} is missing` }],
      reason: failure || `${tag} is missing`,
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
        {
          tag: artifactTag,
          status: "unverified",
          digest,
          detail:
            `provenance check failed: ${message(error)}` +
            (deltaNote ? ` (${deltaNote})` : ""),
        },
      ],
      reason: "provenance did not verify",
    };
  }
  decisions.push({ tag: artifactTag, status: "verified", digest, ...(deltaNote ? { detail: deltaNote } : {}) });

  const candidateRead = readManifestWithAnnotations(exec, `${ref}@${digest}`);
  const candidate = candidateRead.content;

  const annotated = String(candidateRead.annotations[VERSION_ANNOTATION] || "").trim();
  const explicit = String(version || "").trim();
  if (annotated && explicit && annotated !== explicit) {
    throw new Error(
      `${ref}@${digest} is annotated ${annotated} but the caller asked for ${explicit}; ` +
        "the release tag must match the version the binary prints",
    );
  }
  const effectiveVersion = annotated || explicit;
  if (!VERSION.test(effectiveVersion)) {
    throw new Error(
      `${ref}@${digest} has no usable ${VERSION_ANNOTATION} annotation (got '${annotated}') ` +
        "and no --version was given",
    );
  }
  destinations = expand(effectiveVersion);
  if (!destinations.length) {
    throw new Error("no publish tags to attach");
  }

  const versionTag = expandTags(["{version}"], { version: effectiveVersion, commit })[0];
  let releaseDigest = null;

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
      if (allowRetag) {
        exec("crane", ["tag", `${ref}@${digest}`, destination]);
        const moved = parseDigest(exec("crane", ["digest", `${ref}:${destination}`]));
        if (moved !== digest) {
          throw new Error(
            `${ref}:${destination} resolves to ${moved || "an unknown digest"}, not ${digest}`,
          );
        }
        decisions.push({
          tag: destination,
          status: "retagged",
          digest,
          detail: `rebuilt the same tree; moved from ${existing}`,
        });
        return digest;
      }
      throw new Error(
        `${ref}:${destination} already exists with ${existing}, not ${digest}; ` +
          "release tags are immutable by content — a merged change computes a new version, " +
          "and re-pointing one is only the explicit deploy-services rebuild path",
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
    version: effectiveVersion,
    source,
    decisions,
  };
}

export function verifyReleaseVersion({ repository, tree, exec }) {
  const ref = nativeRepository(repository);
  const tag = treeTag(tree);
  let digest;
  try {
    digest = parseDigest(exec("crane", ["digest", `${ref}:${tag}`]));
  } catch (error) {
    throw new Error(`${tag} is not published: ${message(error)}`);
  }
  if (!digest) {
    throw new Error(`${tag} did not resolve to a sha256 digest`);
  }
  const candidate = readManifestWithAnnotations(exec, `${ref}@${digest}`);
  const version = String(candidate.annotations[VERSION_ANNOTATION] || "").trim();
  if (!VERSION.test(version)) {
    throw new Error(
      `${tag} has no usable ${VERSION_ANNOTATION} annotation (got '${version}'); ` +
        "the PR build must annotate the release version it prints",
    );
  }
  let existing = null;
  try {
    existing = parseDigest(exec("crane", ["digest", `${ref}:${version}`]));
  } catch {
    existing = null;
  }
  if (!existing) {
    return { repository: ref, tag, version, digest, status: "available" };
  }
  if (existing === digest) {
    return { repository: ref, tag, version, digest, status: "unchanged" };
  }
  const published = readManifest(exec, `${ref}:${version}`);
  if (sameManifestContent(published, candidate.content)) {
    return { repository: ref, tag, version, digest: existing, status: "unchanged" };
  }
  throw new Error(
    `${ref}:${version} is already published with different content; the computed version ` +
      "did not increase for this tree. A merged change that alters the artifact must also " +
      "increase the version: add a commit under the unit's paths or raise its VERSION floor " +
      "before merge, or repair an already-published release with the explicit rebuild path",
  );
}

export function verifyNativeReleases({ units, tree, exec, log = console.log }) {
  const failures = [];
  let checked = 0;
  for (const unit of units || []) {
    if (!unit || unit.publish === false || !unit.publish_repository) {
      continue;
    }
    checked += 1;
    try {
      const result = verifyReleaseVersion({ repository: unit.publish_repository, tree, exec });
      log(`${unit.id}: ${result.version} ${result.status}`);
    } catch (error) {
      failures.push(`${unit.id}: ${message(error)}`);
    }
  }
  if (failures.length) {
    throw new Error(
      `the release version guard refused ${failures.length} native unit(s):\n- ${failures.join("\n- ")}`,
    );
  }
  return { checked, failures };
}

export function formatNativeSummary(result) {
  const lines = [`### native promotion (${result.tag})`, ""];
  lines.push(`- repository: \`${result.repository}\``);
  if (result.version) {
    lines.push(`- version: \`${result.version}\``);
  }
  for (const decision of result.decisions) {
    const digest = decision.digest ? ` \`${decision.digest}\`` : "";
    const detail = decision.detail ? ` — ${decision.detail}` : "";
    lines.push(`- \`${decision.tag}\`: ${decision.status}${digest}${detail}`);
  }
  lines.push("", `- path: \`${result.mode}\``);
  if (result.mode !== "promoted") {
    lines.push(
      `- not deployable: ${result.reason || "promotion unavailable"}; ` +
        "deploy-home does not build (DECISIONS 36)",
    );
  }
  return `${lines.join("\n")}\n`;
}

export function runNativePromotion({
  args,
  exec,
  env = process.env,
  log = console.log,
  prHead = null,
}) {
  const repository = nativeRepository(args.repository);
  log(`promoting native ${repository} for tree ${args.tree}`);
  const result = planNativePromotion({
    repository,
    tree: args.tree,
    commit: args.commit,
    version: args.version,
    tags: args.tags,
    githubRepository: githubRepositoryFor({ ...args, native: true }, env),
    exec,
    allowPrHead: Boolean(args.allowPrHead),
    prHead,
    unit: args.unit || "",
    allowRetag: Boolean(args.retag),
  });
  log(formatNativeSummary(result));
  if (env.GITHUB_OUTPUT) {
    appendFile(env.GITHUB_OUTPUT, `mode=${result.mode}\n`);
    appendFile(env.GITHUB_OUTPUT, `digest=${result.digest || ""}\n`);
    appendFile(env.GITHUB_OUTPUT, `version=${result.version || ""}\n`);
  }
  if (env.GITHUB_ENV) {
    appendFile(env.GITHUB_ENV, `NATIVE_PROMOTION=${result.mode}\n`);
  }
  if (env.GITHUB_STEP_SUMMARY) {
    appendFile(env.GITHUB_STEP_SUMMARY, formatNativeSummary(result));
  }
  if (result.mode !== "promoted") {
    throw new Error(
      `native artifact for tree-${args.tree} is not published and verified (${result.reason}); ` +
        "the pull request must publish tree-<hash> — re-run PR CI for the merged commit " +
        "and re-merge (DECISIONS 36). If develop moved after the PR CI run, the PR head " +
        "delta was refused: bring develop into the PR, wait for CI and merge again",
    );
  }
  return result;
}

function parseArgs(argv) {
  const args = {
    images: [],
    optionalImages: [],
    allowHistory: false,
    allowPrHead: false,
    retag: false,
    historyLimit: 200,
    unit: "",
    tree: "",
    commit: "",
    repository: "",
    native: false,
    version: "",
    tags: [],
    githubRepository: "",
    verifyNativeReleases: false,
    nativeUnits: "",
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
      case "--verify-native-releases":
        args.verifyNativeReleases = true;
        break;
      case "--native-units":
        args.nativeUnits = value;
        index += 1;
        break;
      case "--images":
        args.images = String(value || "").split(/[\s,]+/).filter(Boolean);
        index += 1;
        break;
      case "--optional-images":
        args.optionalImages = String(value || "").split(/[\s,]+/).filter(Boolean);
        index += 1;
        break;
      case "--allow-history":
        args.allowHistory = true;
        break;
      case "--allow-pr-head":
        args.allowPrHead = true;
        break;
      case "--retag":
        args.retag = true;
        break;
      case "--unit":
        args.unit = String(value || "");
        index += 1;
        break;
      case "--history-limit":
        args.historyLimit = Number(value);
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

export function runPromotion({
  args,
  exec,
  env = process.env,
  log = console.log,
  prHead = null,
}) {
  const repository = args.repository || env.GITHUB_REPOSITORY || "";
  const optionalImages = args.optionalImages || [];
  log(
    `promoting ${uniqueImages(args.images).join(", ")}` +
      `${optionalImages.length ? ` (+ siblings ${optionalImages.join(", ")})` : ""}` +
      ` for tree ${args.tree} (${repository})`,
  );
  const wantsHistory = Boolean(args.allowHistory || optionalImages.length);
  const history = wantsHistory
    ? args.history || readHistory(exec, args.historyLimit)
    : [];
  const result = planPromotion({
    images: args.images,
    optionalImages,
    tree: args.tree,
    commit: args.commit,
    repository,
    exec,
    history,
    allowHistory: Boolean(args.allowHistory),
    allowPrHead: Boolean(args.allowPrHead),
    prHead,
    unit: args.unit || "",
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

export async function main(argv = process.argv.slice(2)) {
  const args = parseArgs(argv);
  if (args.help) {
    console.log(
      "usage: promote.mjs --images 'a b' [--optional-images 'c d'] " +
        "[--allow-history] [--allow-pr-head --unit <id>] [--history-limit N] " +
        "--tree <40-hex> --commit <sha> [--repository owner/repo] [--github-repository owner/repo]\n" +
        "   or: promote.mjs --native --repository ghcr.io/owner/name --tree <40-hex> " +
        "--commit <sha> --version <vX.Y.Z> --tags '{version} latest' " +
        "[--allow-pr-head --unit <id>] --github-repository owner/repo\n" +
        "   or: promote.mjs --verify-native-releases --tree <40-hex> --native-units '<json>'\n" +
        "\n" +
        "--repository is where the artifact lives (images: GitHub owner/repo; native: the " +
        "GHCR publish repository). PR lookups always use --github-repository or " +
        "GITHUB_REPOSITORY, never a native publish repository.",
    );
    return 0;
  }
  if (args.verifyNativeReleases) {
    const units = JSON.parse(args.nativeUnits || "[]");
    if (!Array.isArray(units)) {
      throw new Error("--native-units must be a JSON array of routed native units");
    }
    if (!args.tree) {
      throw new Error("--verify-native-releases needs --tree");
    }
    verifyNativeReleases({ units, tree: args.tree, exec: spawnExec });
    return 0;
  }
  const prHead = args.allowPrHead
    ? await preparePrHead({ args, exec: spawnExec, unit: args.unit })
    : null;
  if (args.native) {
    runNativePromotion({ args, exec: spawnExec, prHead });
    return 0;
  }
  runPromotion({ args, exec: spawnExec, prHead });
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    process.exitCode = await main();
  } catch (error) {
    console.error(error?.stack || String(error));
    process.exitCode = 1;
  }
}
