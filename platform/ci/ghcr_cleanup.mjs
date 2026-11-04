#!/usr/bin/env node
/**
 * GHCR cleanup policy for the deploy images (DECISIONS 36).
 *
 * A pull request publishes each built deploy image as
 * `ghcr.io/<registry>/<image>:tree-<git tree hash>`. Those tags are useful only
 * until the PR is merged or abandoned, so this workflow deletes the stale ones.
 *
 * The retention policy, per package:
 *   1. Never delete the newest version, a version tagged `develop`/`latest` or
 *      a semver release, a version whose digest is listed in
 *      `PROTECTED_DIGESTS`, or a version younger than `KEEP_DAYS` days.
 *   2. A "deploy version" carries at least one 40-hex commit-sha tag. Keep the
 *      `DEPLOY_VERSIONS_TO_KEEP` newest deploy versions; older ones beyond that
 *      window are deleted once they are also older than `KEEP_DAYS`.
 *   3. Stale `tree-*`-only and untagged versions are deleted (unless rule 1
 *      protects them).
 *   4. Referrer versions (every tag is `sha256-<64 hex>`, cosign-style) are
 *      deleted only when the image digest they point at is deleted in this run
 *      or no longer exists in the package; otherwise they are kept. They do
 *      not count toward the deploy-version window.
 *
 * Every decision is written to the job summary with a human-readable reason.
 * In particular `develop` and semver tags are never deleted: a Helm rollback
 * and the digests pinned in private values point at them.
 *
 * Packages are exactly the deploy images listed in `platform/ci/units.mjs`.
 * Other GHCR packages of the account (for example the Windows logon provider
 * artifact) are never considered.
 *
 * The selection logic is pure (`planPackageCleanup`) and covered by
 * `ghcr_cleanup.test.mjs`; the GitHub API plumbing is in `runGithubScript`.
 */

import fs from "node:fs";
import process from "node:process";
import { pathToFileURL } from "node:url";

import { config } from "./units.mjs";

export const KEEP_DAYS = 7;
export const DAY_MS = 24 * 60 * 60 * 1000;
/** How many deploy versions (commit-sha tagged) each package keeps. */
export const DEPLOY_VERSIONS_TO_KEEP = 5;
/** Tags produced by `_images.yml` on a pull request. */
export const TREE_TAG = /^tree-[0-9a-f]{40}$/;
/** A full commit-sha tag marks a deploy version (rule 2). */
export const COMMIT_TAG = /^[0-9a-f]{40}$/;
/**
 * Cosign-style referrer tags: `<algorithm>-<hex>` pointing at an image digest,
 * optionally suffixed with `.` metadata (for example the attestation type).
 */
export const REFERRER_TAG = /^sha256-([0-9a-f]{64})(?:\..*)?$/;
/** Tags whose versions must survive: deploy aliases and releases. */
export const PROTECTED_TAGS = [
  /^develop$/,
  /^latest$/,
  // The native release uses `sha-<12 hex>`; short bare commit shas stay
  // protected too. A full 40-hex tag is a deploy version and is governed by
  // the keep-N window instead.
  /^sha-[0-9a-f]{7,39}$/,
  /^[0-9a-f]{7,39}$/,
  /^v?\d+\.\d+\.\d+(?:[-+].*)?$/,
];
/**
 * Digests that must never be deleted even though the retention window would
 * otherwise select them. `voice-proxy` → sha256:238e…fca9 is the last Python
 * voice-proxy image: the Hermes health-exporter container still runs from it
 * (pinned in private runtime values) until the exporter moves to Go
 * (docs/PENDING.md, "Решение 35"). Drop this entry when that migration lands.
 */
// Digests a running workload still needs although no tag protects them, as
// { package, digest, reason }. Empty since the health exporter moved to Go.
export const PROTECTED_DIGESTS = [];
/**
 * Never touched, even if a future unit map were to list them. The Windows logon
 * provider is a public release artifact with immutable version tags. Pulsar
 * firmware is the station updater's OCI release package (`pulsar-firmware`):
 * `latest` and the version tags must survive, so the cleanup never considers it.
 */
export const EXCLUDED_PACKAGES = new Set(["windows-logon-provider", "pulsar-firmware"]);

/** Package names owned by the deploy units, in a stable order. */
export function deployImagePackages(unitsConfig = config) {
  const names = [];
  for (const unit of unitsConfig.units || []) {
    for (const image of unit.deploy?.images || []) {
      if (!names.includes(image) && !EXCLUDED_PACKAGES.has(image)) {
        names.push(image);
      }
    }
  }
  return names.sort();
}

function tagList(version) {
  const tags = version?.metadata?.container?.tags;
  if (!Array.isArray(tags)) {
    return [];
  }
  return tags.filter((tag) => typeof tag === "string" && tag.length > 0);
}

function createdAt(version) {
  const value = Date.parse(String(version?.created_at || ""));
  return Number.isFinite(value) ? value : null;
}

/** Rank deploy versions by creation time, falling back to the update time. */
function updatedAt(version) {
  const value = Date.parse(String(version?.updated_at || ""));
  return Number.isFinite(value) ? value : null;
}

function versionTime(version) {
  return createdAt(version) ?? updatedAt(version) ?? -Infinity;
}

function idKey(version) {
  return String(version?.id ?? "");
}

/** The version `name` is the manifest digest (`sha256:…`). */
function digestOf(version) {
  return typeof version?.name === "string" ? version.name : "";
}

function isProtectedTag(tag) {
  return PROTECTED_TAGS.some((pattern) => pattern.test(tag));
}

function isProtectedDigest(version, protectedDigests) {
  const digest = digestOf(version);
  return digest ? protectedDigests.find((entry) => entry.digest === digest) : undefined;
}

function isDeployVersion(tags) {
  return tags.some((tag) => COMMIT_TAG.test(tag));
}

function isReferrerVersion(tags) {
  return tags.length > 0 && tags.every((tag) => REFERRER_TAG.test(tag));
}

/** `sha256-<hex>[.metadata]` → `sha256:<hex>`. */
function referrerSubject(tags) {
  for (const tag of tags) {
    const match = REFERRER_TAG.exec(tag);
    if (match) {
      return `sha256:${match[1]}`;
    }
  }
  return null;
}

/**
 * Decide what to delete for one package.
 *
 * @param {Array<{id: number|string, name?: string, created_at?: string,
 *   updated_at?: string, metadata?: {container?: {tags?: string[]}}}>} versions
 * @param {{now?: number, keepDays?: number}} options
 */
export function planPackageCleanup(
  versions,
  { now = Date.now(), keepDays = KEEP_DAYS, protectedDigests = PROTECTED_DIGESTS } = {},
) {
  const list = Array.isArray(versions) ? versions.filter(Boolean) : [];
  if (!list.length) {
    return { newest: null, decisions: [], delete: [], keep: [] };
  }

  // Newest by creation time, with the id as the tie-breaker (GitHub ids grow
  // monotonically). A missing date never makes a version "newest".
  let newest = null;
  for (const version of list) {
    if (newest === null) {
      newest = version;
      continue;
    }
    const current = versionTime(version);
    const best = versionTime(newest);
    if (current > best || (current === best && Number(idKey(version)) > Number(idKey(newest)))) {
      newest = version;
    }
  }

  const existingDigests = new Set(list.map(digestOf).filter(Boolean));

  // The newest deploy versions form the per-package retention window. Referrers
  // never count toward it.
  const deployVersions = list.filter((version) => {
    const tags = tagList(version);
    return !isReferrerVersion(tags) && isDeployVersion(tags);
  });
  deployVersions.sort((left, right) => {
    const delta = versionTime(right) - versionTime(left);
    return delta !== 0 ? delta : Number(idKey(right)) - Number(idKey(left));
  });
  const retainedDeployIds = new Set(
    deployVersions.slice(0, DEPLOY_VERSIONS_TO_KEEP).map(idKey),
  );

  const decisions = new Array(list.length);
  const deletedDigests = new Set();

  // PASS 1: ordinary versions. Their deletions decide which referrers follow.
  list.forEach((version, index) => {
    const tags = tagList(version);
    if (isReferrerVersion(tags)) {
      return;
    }
    const created = createdAt(version);
    const base = { id: version.id, tags, created_at: version.created_at ?? null };
    const decision = (action, reason) => ({ ...base, action, reason });

    if (idKey(version) === idKey(newest)) {
      decisions[index] = decision("keep", "newest version of the package");
      return;
    }
    if (created === null) {
      decisions[index] = decision("keep", "unknown creation date");
      return;
    }
    const ageDays = (now - created) / DAY_MS;
    if (ageDays < keepDays) {
      decisions[index] = decision("keep", `younger than ${keepDays} days (${ageDays.toFixed(1)}d)`);
      return;
    }
    const protectedDigest = isProtectedDigest(version, protectedDigests);
    if (protectedDigest) {
      decisions[index] = decision("keep", `protected digest (${protectedDigest.reason})`);
      return;
    }
    const protectedTag = tags.find(isProtectedTag);
    if (protectedTag) {
      decisions[index] = decision("keep", `protected tag '${protectedTag}'`);
      return;
    }
    if (isDeployVersion(tags)) {
      if (retainedDeployIds.has(idKey(version))) {
        decisions[index] = decision(
          "keep",
          `one of the ${DEPLOY_VERSIONS_TO_KEEP} newest deploy versions`,
        );
      } else {
        decisions[index] = decision(
          "delete",
          `deploy version older than ${keepDays} days beyond the newest ` +
            `${DEPLOY_VERSIONS_TO_KEEP} (${ageDays.toFixed(1)}d)`,
        );
      }
      return;
    }
    if (!tags.length) {
      decisions[index] = decision("delete", "untagged and older than the window");
      return;
    }
    const foreignTag = tags.find((tag) => !TREE_TAG.test(tag));
    if (foreignTag) {
      decisions[index] = decision("keep", `tag '${foreignTag}' is not a tree tag`);
      return;
    }
    decisions[index] = decision("delete", `only tree tags, ${ageDays.toFixed(1)}d old`);
  });

  for (let index = 0; index < list.length; index += 1) {
    if (decisions[index]?.action === "delete") {
      const digest = digestOf(list[index]);
      if (digest) {
        deletedDigests.add(digest);
      }
    }
  }

  // PASS 2: referrers follow the fate of the digest they point at.
  list.forEach((version, index) => {
    const tags = tagList(version);
    if (!isReferrerVersion(tags)) {
      return;
    }
    const created = createdAt(version);
    const base = { id: version.id, tags, created_at: version.created_at ?? null };
    const decision = (action, reason) => ({ ...base, action, reason });

    if (idKey(version) === idKey(newest)) {
      decisions[index] = decision("keep", "newest version of the package");
      return;
    }
    if (created === null) {
      decisions[index] = decision("keep", "unknown creation date");
      return;
    }
    const ageDays = (now - created) / DAY_MS;
    if (ageDays < keepDays) {
      decisions[index] = decision("keep", `younger than ${keepDays} days (${ageDays.toFixed(1)}d)`);
      return;
    }
    const protectedDigest = isProtectedDigest(version, protectedDigests);
    if (protectedDigest) {
      decisions[index] = decision("keep", `protected digest (${protectedDigest.reason})`);
      return;
    }
    const subject = referrerSubject(tags);
    const protectedSubject = protectedDigests.find((entry) => entry.digest === subject);
    if (protectedSubject) {
      decisions[index] = decision(
        "keep",
        `referrer of protected digest ${subject} (${protectedSubject.reason})`,
      );
      return;
    }
    if (subject && deletedDigests.has(subject)) {
      decisions[index] = decision("delete", `subject digest ${subject} is deleted in this run`);
      return;
    }
    if (subject && !existingDigests.has(subject)) {
      decisions[index] = decision("delete", `subject digest ${subject} no longer exists in the package`);
      return;
    }
    decisions[index] = decision("keep", `subject digest ${subject} is kept`);
  });

  return {
    newest: newest?.id ?? null,
    decisions,
    delete: decisions.filter((item) => item.action === "delete"),
    keep: decisions.filter((item) => item.action === "keep"),
  };
}

/** `[{"id":…,"created_at":…,"tags":[…]}]` -> the GitHub version shape. */
export function normalizeApiVersion(version) {
  return {
    id: version.id,
    name: version.name,
    created_at: version.created_at,
    updated_at: version.updated_at,
    metadata: { container: { tags: Array.isArray(version.tags) ? version.tags : [] } },
  };
}

export function formatPackageSummary(name, plan, { dryRun }) {
  const mode = dryRun ? "dry run" : "delete";
  const lines = [`### GHCR cleanup / ${name} (${mode})`, ""];
  if (!plan.decisions.length) {
    lines.push("- no versions found");
    return `${lines.join("\n")}\n`;
  }
  for (const item of plan.decisions) {
    const tags = item.tags.length ? item.tags.map((tag) => `\`${tag}\``).join(", ") : "untagged";
    lines.push(`- \`${item.id}\` ${tags} — **${item.action}**: ${item.reason}`);
  }
  lines.push(
    "",
    `- delete: ${plan.delete.length}, keep: ${plan.keep.length}`,
  );
  return `${lines.join("\n")}\n`;
}

/** Read every version of a user- or org-owned container package. */
export async function fetchVersions(github, { owner, ownerType, packageName }) {
  if (ownerType === "Organization") {
    return github.paginate(github.rest.packages.getAllPackageVersionsForPackageOwnedByOrg, {
      package_type: "container",
      package_name: packageName,
      org: owner,
      per_page: 100,
    });
  }
  return github.paginate(github.rest.packages.getAllPackageVersionsForPackageOwnedByUser, {
    package_type: "container",
    package_name: packageName,
    username: owner,
    per_page: 100,
  });
}

export async function deleteVersion(github, { owner, ownerType, packageName, versionId }) {
  if (ownerType === "Organization") {
    return github.rest.packages.deletePackageVersionForOrg({
      package_type: "container",
      package_name: packageName,
      org: owner,
      package_version_id: versionId,
    });
  }
  return github.rest.packages.deletePackageVersionForUser({
    package_type: "container",
    package_name: packageName,
    username: owner,
    package_version_id: versionId,
  });
}

function isNotFound(error) {
  return Number(error?.status) === 404;
}

/**
 * Entry point for `actions/github-script` in `.github/workflows/ghcr-cleanup.yml`.
 * `env`: `DRY_RUN` ("true"/"false") and `TOKEN_SOURCE` (informational).
 */
export async function runGithubScript({ core, github, context, env = process.env }) {
  const dryRun = String(env.DRY_RUN || "false") === "true";
  const owner = context.repo.owner;
  const ownerType = context.payload?.repository?.owner?.type || "User";
  const packages = deployImagePackages();
  const summary = [];
  const failures = [];
  let deleted = 0;
  let selected = 0;

  core.info(
    `GHCR cleanup: ${packages.length} deploy package(s), owner ${owner} (${ownerType}), ` +
      `mode ${dryRun ? "dry run" : "delete"}, token ${env.TOKEN_SOURCE || "github.token"}`,
  );
  summary.push(
    `## GHCR cleanup (${dryRun ? "dry run" : "delete"})`,
    "",
    `- owner: \`${owner}\` (${ownerType})`,
    `- packages: ${packages.map((name) => `\`${name}\``).join(", ")}`,
    "",
  );

  for (const packageName of packages) {
    let versions;
    try {
      versions = await fetchVersions(github, { owner, ownerType, packageName });
    } catch (error) {
      if (isNotFound(error)) {
        core.info(`${packageName}: package not found, skipping`);
        summary.push(`### GHCR cleanup / ${packageName}`, "", "- package not found; skipped", "");
        continue;
      }
      throw error;
    }
    const plan = planPackageCleanup(versions);
    core.info(
      `${packageName}: ${versions.length} version(s), ${plan.delete.length} to delete, ` +
        `${plan.keep.length} to keep`,
    );
    for (const item of plan.decisions) {
      core.info(
        `${packageName} ${item.id} [${item.tags.join(",") || "untagged"}] ${item.action}: ${item.reason}`,
      );
    }
    summary.push(formatPackageSummary(packageName, plan, { dryRun }));

    selected += plan.delete.length;
    if (dryRun) {
      continue;
    }
    for (const item of plan.delete) {
      try {
        await deleteVersion(github, { owner, ownerType, packageName, versionId: item.id });
        deleted += 1;
        core.info(`${packageName}: deleted version ${item.id}`);
      } catch (error) {
        const message = `${packageName} version ${item.id}: ${error?.message || error}`;
        failures.push(message);
        core.warning(`${message} (status ${error?.status ?? "unknown"})`);
        summary.push(`- **failed** \`${item.id}\`: ${error?.message || error}`);
      }
    }
  }

  summary.push(
    "",
    `- selected: ${selected}`,
    `- deleted: ${dryRun ? 0 : deleted}`,
    `- failures: ${failures.length}`,
    "",
  );
  await core.summary.addRaw(summary.join("\n")).write();

  if (failures.length) {
    const hint = String(env.TOKEN_SOURCE || "") === "GHCR_CLEANUP_TOKEN"
      ? "the GHCR_CLEANUP_TOKEN secret is set but was refused"
      : "GITHUB_TOKEN was refused; provision a GHCR_CLEANUP_TOKEN secret with package " +
        "delete permission and link it to this repository";
    throw new Error(
      `${failures.length} package version(s) could not be deleted; ${hint}. ` +
        "Example: " + failures[0],
    );
  }
  return { dryRun, packages, selected, deleted, failures };
}

function parseArgs(argv) {
  const args = { fixture: "", now: Date.now(), dryRun: true, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    switch (flag) {
      case "--fixture":
        args.fixture = argv[index + 1];
        index += 1;
        break;
      case "--now":
        args.now = Date.parse(argv[index + 1]);
        index += 1;
        break;
      case "--dry-run":
        args.dryRun = true;
        break;
      case "--delete":
        args.dryRun = false;
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

/**
 * Local dry run against a fixture (never calls GitHub):
 *
 *   node platform/ci/ghcr_cleanup.mjs --fixture platform/ci/testdata/ghcr-versions.json
 *
 * `--delete` is refused: deletion only happens in the workflow.
 */
export function main(argv = process.argv.slice(2), log = console.log) {
  const args = parseArgs(argv);
  if (args.help) {
    log("usage: ghcr_cleanup.mjs --fixture <json> [--now <iso date>] [--dry-run]");
    return 0;
  }
  if (!args.fixture) {
    throw new Error("--fixture is required; the CLI never talks to GitHub");
  }
  if (!args.dryRun) {
    throw new Error("the local CLI is a dry run only; use the workflow to delete");
  }
  if (!Number.isFinite(args.now)) {
    throw new Error("--now must be an ISO date");
  }
  const fixture = JSON.parse(fs.readFileSync(args.fixture, "utf8"));
  const packages = fixture.packages || {};
  let selected = 0;
  for (const [name, versions] of Object.entries(packages)) {
    const plan = planPackageCleanup(versions, { now: args.now });
    selected += plan.delete.length;
    log(formatPackageSummary(name, plan, { dryRun: true }));
  }
  log(`dry run: ${selected} version(s) would be deleted`);
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
