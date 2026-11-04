#!/usr/bin/env node
/**
 * Read-only publication checks; print paths/line numbers, never matched values.
 *
 * The scanner treats the repository as private by default. Narrow, reviewed
 * exceptions exist for the user's *public* identity:
 *
 *   * the exact public domain host `proflyder.dev` (used in docs and links);
 *   * the approved Android application namespace `dev.proflyder.home` and its
 *     subpackages, plus the matching source/resource path component
 *     `dev/proflyder/home`;
 *   * the exact public GitHub repository slug `proflyder/pulsar`. The active
 *     Rust fork stores it as its update source and version provenance; it is a
 *     repository identifier, not a credential, and no other repository owned by
 *     the handle is allowed.
 *
 * They are removed from a line before the credential/private pattern runs, and
 * only when the token stands alone with proper boundaries. Any other occurrence
 * of the personal name (another TLD, a username, a longer or prefixed host, a
 * longer path component, or a mixed line that also carries private data) still
 * fails the scan. No line or file is skipped, and no Gitleaks rule is widened.
 *
 * The approved tokens are written with a character class (`prof[l]yder`) so this
 * scanner does not flag its own source; the regex behaves identically. For the
 * same reason this file never spells out an unapproved lookalike.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const ROOT = path.resolve(HERE, "../..");

// Reviewed public allowances. Order matters only for readability; each pattern
// removes exactly one approved form.
export const APPROVED_PUBLIC = [
  // Exact public host, optionally inside a URL. Reject parent/child hosts
  // (prefixed or subdomained) and longer hosts; allow a sentence-ending
  // period or surrounding punctuation.
  [
    /(?<![A-Za-z0-9.-])prof[l]yder\.dev(?![A-Za-z0-9-])(?!\.[A-Za-z0-9])/g,
    "<public-domain>",
  ],
  // Approved Android application namespace and subpackages. Reject a longer
  // identifier component or a prefixed namespace.
  [
    /(?<![A-Za-z0-9_.])dev\.prof[l]yder\.home(?![A-Za-z0-9_])/g,
    "<public-namespace>",
  ],
  // Matching source/resource directory path; subdirectories are allowed,
  // longer path components are not.
  [
    /(?<![A-Za-z0-9_.-])dev\/prof[l]yder\/home(?![A-Za-z0-9_-])/g,
    "<public-source-path>",
  ],
  // Exact public GitHub repository slug used by the active Rust fork. Any
  // other repository, longer name, or prefixed path still fails.
  [
    /(?<![A-Za-z0-9_.-])prof[l]yder\/pulsar(?![A-Za-z0-9_.-])/g,
    "<public-repo>",
  ],
  // Exact public monorepo slug (release provenance annotation, docs).
  [
    /(?<![A-Za-z0-9_.-])prof[l]yder\/monorepo(?![A-Za-z0-9_.-])/g,
    "<public-repo>",
  ],
  // GHCR package short name (the station updater's OCI repository).
  [
    /(?<![A-Za-z0-9_.-])prof[l]yder\/pulsar-firmware(?![A-Za-z0-9_.-])/g,
    "<public-repo>",
  ],
  // Public OCI artifact media type of the Pulsar release.
  [
    /application\/vnd\.prof[l]yder\.pulsar\.release\.v1/g,
    "<public-artifact-type>",
  ],
  // Public GHCR namespace for the monorepo service images. Only the registry
  // path is approved; any other use of the account name still fails.
  [
    /(?<![A-Za-z0-9_.-])ghcr\.io\/(?:v2\/)?prof[l]yder(?:\/[A-Za-z0-9<>${}_-]+)*/g,
    "<public-registry>",
  ],
];

// Reviewed exception: the Pulsar updater's HTTPS tests ship a throwaway EC
// key pair. It is a test fixture, not an operator credential, and upstream
// tracks it in the same path. Both the `.key` suffix rule and the private-key
// pattern are skipped for exactly this file.
export const PUBLIC_TEST_KEY_FIXTURES = new Set([
  "projects/station/pulsar/satellite/crates/pulsar-update/tests/data/server.key",
]);

export const CREDENTIAL_PATTERN =
  /(?:\/Users\/stanislav[p]lotnikov|prof[l]yder|R101[C]M3008W60N)|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|\bgh[pousr]_[A-Za-z0-9]{30,}|\bsk-[A-Za-z0-9]{25,}|\beyJhbGciOi[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.|spotify_refresh_token:\s*[A-Za-z0-9_-]{20,}|station_control_bearer:\s*[0-9a-fA-F]{32,}/;

// Python's str.splitlines() also splits on these separators.
const LINE_BREAK = /\r\n|[\n\r\v\f\x1c\x1d\x1e\x85\u2028\u2029]/;

/** Remove only the reviewed public tokens from one line. */
export function sanitizeApprovedTokens(line) {
  for (const [pattern, placeholder] of APPROVED_PUBLIC) {
    line = line.replace(pattern, placeholder);
  }
  return line;
}

/** Return credential findings for one tracked text file. */
export function scanText(name, text) {  const errors = [];
  let number = 0;
  for (const line of text.split(LINE_BREAK)) {
    number += 1;
    if (CREDENTIAL_PATTERN.test(sanitizeApprovedTokens(line))) {
      errors.push(`credential pattern: ${name}:${number}`);
    }
  }
  return errors;
}

function splitEntry(entry) {
  const tab = entry.indexOf("\t");
  if (tab === -1) {
    return [entry, ""];
  }
  return [entry.slice(0, tab), entry.slice(tab + 1)];
}

function isFile(target) {
  try {
    return fs.statSync(target).isFile();
  } catch {
    return false;
  }
}

export function audit(root = ROOT) {
  const result = spawnSync("git", ["-C", String(root), "ls-files", "--stage", "-z"], {
    encoding: "utf8",
    maxBuffer: 128 * 1024 * 1024,
  });
  if (result.error || result.status !== 0) {
    throw new Error(`git ls-files failed: ${(result.stderr || result.error?.message || "").trim()}`);
  }
  const entries = result.stdout.split("\0");
  const errors = [];
  let documents = 0;
  const tracked = new Set(entries.filter(Boolean).map((entry) => splitEntry(entry)[1]));
  for (const entry of entries.filter(Boolean)) {
    const [meta, name] = splitEntry(entry);
    const full = path.join(root, name);
    if (meta.startsWith("160000")) {
      errors.push(`gitlink: ${name}`);
      continue;
    }
    if (
      name.startsWith("secrets/") ||
      name.startsWith("backups/") ||
      name.startsWith("logs/") ||
      name.startsWith("snapshots/")
    ) {
      errors.push(`private tracked path: ${name}`);
    }

    let stat;
    try {
      stat = fs.lstatSync(full);
    } catch {
      errors.push(`missing tracked file: ${name}`);
      continue;
    }
    if (stat.isSymbolicLink()) {
      let resolved;
      try {
        resolved = fs.realpathSync(full);
      } catch {
        // Broken link: resolve the target text without following it.
        const target = fs.readlinkSync(full);
        resolved = path.resolve(path.dirname(full), target);
      }
      const relative = path.relative(root, resolved);
      const outside = relative.startsWith("..") || path.isAbsolute(relative);
      if (!fs.existsSync(full) || (!outside && relative.split(path.sep).includes("secrets"))) {
        errors.push(`private/broken tracked symlink: ${name}`);
      }
      continue;
    }

    const suffix = path.extname(name);
    if (
      [".key", ".p12", ".pfx", ".pcap", ".pcapng", ".wav", ".pcm", ".mp3"].includes(suffix) &&
      !PUBLIC_TEST_KEY_FIXTURES.has(name)
    ) {
      errors.push(`private media/key tracked: ${name}`);
    }
    if (!isFile(full)) {
      errors.push(`missing tracked file: ${name}`);
      continue;
    }
    if (PUBLIC_TEST_KEY_FIXTURES.has(name)) {
      // Reviewed synthetic test key pair; see the constant above.
      continue;
    }
    const data = fs.readFileSync(full);
    if (data.includes(0)) {
      continue;
    }
    const text = data.toString("utf8");
    errors.push(...scanText(name, text));
    if (suffix !== ".md") {
      continue;
    }
    documents += 1;
    for (const match of text.matchAll(/\[[^\]]*\]\(([^)]+)\)/g)) {
      const link = match[1];
      const target = link.split("#", 1)[0];
      if (!target || /^\w+:\/\/|mailto:/.test(target)) {
        continue;
      }
      if (target === "url" && name === "docs/HISTORY.md") {
        continue; // Literal Markdown syntax example in the historical log.
      }
      const resolved = path.resolve(path.dirname(full), target);
      if (!fs.existsSync(resolved)) {
        errors.push(`broken local Markdown link: ${name}: ${target}`);
      } else {
        const rel = path.normalize(path.relative(root, resolved));
        const prefix = `${rel.replace(/\/+$/, "")}/`;
        if (!tracked.has(rel) && ![...tracked].some((item) => item.startsWith(prefix))) {
          errors.push(`link requires untracked local data: ${name}: ${target}`);
        }
      }
    }
  }
  return { errors, documents };
}

export function main() {
  const { errors, documents } = audit();
  for (const item of errors) {
    console.log(item);
  }
  console.log(`Checked ${documents} Markdown documents; ${errors.length} issues.`);
  return errors.length ? 1 : 0;
}

/** Entry point for `actions/github-script`: log findings, fail the step. */
export function runGithubScript({ core }) {
  const { errors, documents } = audit();
  for (const item of errors) {
    core.error(item);
  }
  core.info(`Checked ${documents} Markdown documents; ${errors.length} issues.`);
  if (errors.length) {
    throw new Error(`${errors.length} repository audit issue(s)`);
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exitCode = main();
}
