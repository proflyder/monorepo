#!/usr/bin/env node
/**
 * GHCR cleanup policy for the deploy images (DECISIONS 36).
 *
 * A pull request publishes each built deploy image as
 * `ghcr.io/<registry>/<image>:tree-<git tree hash>`. Those tags are useful only
 * until the PR is merged or abandoned, so this workflow deletes the stale ones.
 *
 * A package version is deleted only when ALL of the following hold:
 *   * it is older than `KEEP_DAYS` days;
 *   * every tag it has matches `^tree-` (or it has no tags at all);
 *   * it is not the newest version of the package.
 *
 * Everything else is kept, and every decision is written to the job summary.
 * In particular the deploy tags (`develop`, a commit sha, a semver release) are
 * never deleted: a Helm rollback and the digests pinned in private values point
 * at them, so removing one would break an existing rollback path.
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
/** Tags produced by `_images.yml` on a pull request. */
export const TREE_TAG = /^tree-[0-9a-f]{40}$/;
/** Tags whose versions must survive: deploy tags, commit tags, releases. */
export const PROTECTED_TAGS = [
  /^develop$/,
  /^latest$/,
  // deploy-home tags promoted/built images with the full commit sha, and the
  // native release uses `sha-<12 hex>`.
  /^sha-?[0-9a-f]{7,40}$/,
  /^[0-9a-f]{7,40}$/,
  /^v?\d+\.\d+\.\d+(?:[-+.].*)?$/,
];
/**
 * Never touched, even if a future unit map were to list them. The Windows logon
 * provider is a public release artifact with immutable version tags.
 */
export const EXCLUDED_PACKAGES = new Set(["windows-logon-provider"]);

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

function idKey(version) {
  return String(version?.id ?? "");
}

function isProtectedTag(tag) {
  return PROTECTED_TAGS.some((pattern) => pattern.test(tag));
}

/**
 * Decide what to delete for one package.
 *
 * @param {Array<{id: number|string, created_at?: string,
 *   metadata?: {container?: {tags?: string[]}}}>} versions
 * @param {{now?: number, keepDays?: number}} options
 */
export function planPackageCleanup(versions, { now = Date.now(), keepDays = KEEP_DAYS } = {}) {
  const list = Array.isArray(versions) ? versions.filter(Boolean) : [];
  const decisions = [];
  if (!list.length) {
    return { newest: null, decisions, delete: [], keep: [] };
  }

  // Newest by creation time, with the id as the tie-breaker (GitHub ids grow
  // monotonically). A missing date never makes a version "newest".
  let newest = null;
  for (const version of list) {
    if (newest === null) {
      newest = version;
      continue;
    }
    const current = createdAt(version) ?? -Infinity;
    const best = createdAt(newest) ?? -Infinity;
    if (current > best || (current === best && Number(idKey(version)) > Number(idKey(newest)))) {
      newest = version;
    }
  }

  for (const version of list) {
    const tags = tagList(version);
    const created = createdAt(version);
    const base = { id: version.id, tags, created_at: version.created_at ?? null };
    if (idKey(version) === idKey(newest)) {
      decisions.push({ ...base, action: "keep", reason: "newest version of the package" });
      continue;
    }
    if (created === null) {
      decisions.push({ ...base, action: "keep", reason: "unknown creation date" });
      continue;
    }
    const ageDays = (now - created) / DAY_MS;
    if (ageDays < keepDays) {
      decisions.push({
        ...base,
        action: "keep",
        reason: `younger than ${keepDays} days (${ageDays.toFixed(1)}d)`,
      });
      continue;
    }
    if (!tags.length) {
      decisions.push({ ...base, action: "delete", reason: "untagged and older than the window" });
      continue;
    }
    const protectedTag = tags.find(isProtectedTag);
    if (protectedTag) {
      decisions.push({ ...base, action: "keep", reason: `protected tag '${protectedTag}'` });
      continue;
    }
    const foreignTag = tags.find((tag) => !TREE_TAG.test(tag));
    if (foreignTag) {
      decisions.push({ ...base, action: "keep", reason: `tag '${foreignTag}' is not a tree tag` });
      continue;
    }
    decisions.push({
      ...base,
      action: "delete",
      reason: `only tree tags, ${ageDays.toFixed(1)}d old`,
    });
  }

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
    created_at: version.created_at,
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
