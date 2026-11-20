#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const ROOT = path.resolve(HERE, "../..");

export const APPROVED_PUBLIC = [
  [
    /(?<![A-Za-z0-9.-])prof[l]yder\.dev(?![A-Za-z0-9-])(?!\.[A-Za-z0-9])/g,
    "<public-domain>",
  ],
  [
    /(?<![A-Za-z0-9_.])dev\.prof[l]yder\.home(?![A-Za-z0-9_])/g,
    "<public-namespace>",
  ],
  [
    /(?<![A-Za-z0-9_.-])dev\/prof[l]yder\/home(?![A-Za-z0-9_-])/g,
    "<public-source-path>",
  ],
  [
    /(?<![A-Za-z0-9_.-])prof[l]yder\/pulsar(?![A-Za-z0-9_.-])/g,
    "<public-repo>",
  ],
  [
    /(?<![A-Za-z0-9_.-])prof[l]yder\/monorepo(?![A-Za-z0-9_.-])/g,
    "<public-repo>",
  ],
  [
    /(?<![A-Za-z0-9_.-])prof[l]yder\/pulsar-firmware(?![A-Za-z0-9_.-])/g,
    "<public-repo>",
  ],
  [
    /application\/vnd\.prof[l]yder\.pulsar\.release\.v1/g,
    "<public-artifact-type>",
  ],
  [
    /(?<![A-Za-z0-9_.-])ghcr\.io\/(?:v2\/)?prof[l]yder(?:\/[A-Za-z0-9<>${}_-]+)*/g,
    "<public-registry>",
  ],
];

export const PUBLIC_TEST_KEY_FIXTURES = new Set([
  "projects/station/pulsar/satellite/crates/pulsar-update/tests/data/server.key",
  "packages/rust/home-tls/testdata/server-key.pem",
  "solutions/phone-push/crates/phone-push/testdata/service-account.pem",
]);

export const CREDENTIAL_PATTERN =
  /(?:\/Users\/stanislav[p]lotnikov|prof[l]yder|R101[C]M3008W60N)|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|\bgh[pousr]_[A-Za-z0-9]{30,}|\bsk-[A-Za-z0-9]{25,}|\beyJhbGciOi[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.|spotify_refresh_token:\s*[A-Za-z0-9_-]{20,}|station_control_bearer:\s*[0-9a-fA-F]{32,}/;

const LINE_BREAK = /\r\n|[\n\r\v\f\x1c\x1d\x1e\x85\u2028\u2029]/;

export function sanitizeApprovedTokens(line) {
  for (const [pattern, placeholder] of APPROVED_PUBLIC) {
    line = line.replace(pattern, placeholder);
  }
  return line;
}

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
