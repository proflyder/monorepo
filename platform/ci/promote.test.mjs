// Unit tests for the build-once promotion decision (DECISIONS 36).
//
// No registry, no network: `planPromotion` takes the command runner as an
// argument, so every branch (missing tag, failed provenance, digest rewrite)
// is exercised with a fake `crane`/`gh`.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  formatPrebuilt,
  formatSummary,
  imageRef,
  parseDigest,
  planPromotion,
  runPromotion,
  signerIdentityPattern,
  treeTag,
  verifyArgs,
} from "./promote.mjs";

const TREE = "a".repeat(40);
const COMMIT = "b".repeat(40);
const REPO = "acme/home-lab";
const DIGEST_A = "sha256:" + "1".repeat(64);
const DIGEST_B = "sha256:" + "2".repeat(64);

function recorder(handlers) {
  const calls = [];
  const exec = (command, args) => {
    calls.push([command, ...args]);
    for (const [match, reply] of handlers) {
      if (match(command, args)) {
        if (reply instanceof Error) {
          throw reply;
        }
        return typeof reply === "function" ? reply(command, args) : reply;
      }
    }
    throw new Error(`unexpected command: ${command} ${args.join(" ")}`);
  };
  return { calls, exec };
}

function isDigestOf(ref, digest) {
  return (command, args) => command === "crane" && args[0] === "digest" && args[1] === ref;
}

const isCraneDigest = (command, args) => command === "crane" && args[0] === "digest";
const isCraneTag = (command, args) => command === "crane" && args[0] === "tag";
const isGhVerify = (command) => command === "gh";

function tagsWritten(calls) {
  return calls.filter(([command, action]) => command === "crane" && action === "tag");
}

test("treeTag requires the full git tree hash", () => {
  assert.equal(treeTag(TREE), `tree-${TREE}`);
  assert.throws(() => treeTag(""), /tree must be/);
  assert.throws(() => treeTag("abc123"), /tree must be/);
  assert.throws(() => treeTag("A".repeat(40)), /tree must be/);
});

test("imageRef only accepts an image component, never a path", () => {
  assert.equal(imageRef("ha-bridge"), "ghcr.io/proflyder/ha-bridge");
  assert.throws(() => imageRef("../evil"), /invalid image name/);
  assert.throws(() => imageRef("a/b"), /invalid image name/);
  assert.throws(() => imageRef(""), /invalid image name/);
});

test("parseDigest accepts only a full sha256 digest", () => {
  assert.equal(parseDigest(` ${DIGEST_A}\n`), DIGEST_A);
  assert.equal(parseDigest("sha256:abc"), null);
  assert.equal(parseDigest(""), null);
  assert.equal(parseDigest("sha512:" + "1".repeat(64)), null);
});

test("signer pattern is bound to the caller repository and CI workflows", () => {
  const pattern = signerIdentityPattern(REPO);
  assert.equal(
    pattern,
    "^https://github\\.com/acme/home-lab/\\.github/workflows/(ci|_images)\\.yml@",
  );
  const regex = new RegExp(pattern);
  assert.ok(regex.test("https://github.com/acme/home-lab/.github/workflows/_images.yml@refs/pull/7/merge"));
  assert.ok(regex.test("https://github.com/acme/home-lab/.github/workflows/ci.yml@refs/pull/7/merge"));
  assert.ok(!regex.test("https://github.com/acme/home-lab/.github/workflows/deploy-home.yml@refs/heads/develop"));
  assert.ok(!regex.test("https://github.com/evil/home-lab/.github/workflows/ci.yml@refs/pull/7/merge"));
  assert.throws(() => signerIdentityPattern("nope"), /invalid GitHub repository/);
});

test("verify args pin repository, signer repo and workflow identity", () => {
  const args = verifyArgs({ repository: REPO, reference: `ghcr.io/proflyder/scheduler@${DIGEST_A}` });
  assert.deepEqual(args.slice(0, 2), ["attestation", "verify"]);
  assert.equal(args[2], `oci://ghcr.io/proflyder/scheduler@${DIGEST_A}`);
  assert.deepEqual(args.slice(3, 7), ["--repo", REPO, "--signer-repo", REPO]);
  assert.equal(args[7], "--cert-identity-regex");
  assert.equal(args[8], signerIdentityPattern(REPO));
});

test("an existing, attested tree image is promoted without rebuilding", () => {
  const treeRef = `ghcr.io/proflyder/scheduler:tree-${TREE}`;
  const digestRef = `ghcr.io/proflyder/scheduler@${DIGEST_A}`;
  const { calls, exec } = recorder([
    [isDigestOf(treeRef), DIGEST_A],
    [isGhVerify, ""],
    [isCraneTag, ""],
    // Every tag read-back must resolve to the promoted digest.
    [isCraneDigest, DIGEST_A],
  ]);

  const result = planPromotion({
    images: ["scheduler"],
    tree: TREE,
    commit: COMMIT,
    repository: REPO,
    exec,
  });

  assert.equal(result.mode, "prebuilt");
  assert.deepEqual(result.digests, { scheduler: DIGEST_A });
  assert.deepEqual(tagsWritten(calls), [
    ["crane", "tag", digestRef, COMMIT],
    ["crane", "tag", digestRef, "develop"],
  ]);
  // The provenance check must run before any tag is written.
  const firstTag = calls.findIndex(([command, action]) => command === "crane" && action === "tag");
  const verify = calls.findIndex(([command]) => command === "gh");
  assert.ok(verify >= 0 && verify < firstTag);
  assert.equal(result.decisions[0].status, "promoted");
  // Read-back of both tags happened after tagging.
  assert.deepEqual(
    calls.filter(([command, action]) => command === "crane" && action === "digest")
      .map((call) => call[2])
      .slice(1),
    [`ghcr.io/proflyder/scheduler:${COMMIT}`, "ghcr.io/proflyder/scheduler:develop"],
  );
});

test("a missing tree tag falls back to the build path", () => {
  const treeRef = `ghcr.io/proflyder/mcp-gateway:tree-${TREE}`;
  const { calls, exec } = recorder([
    [isDigestOf(treeRef), new Error("MANIFEST_UNKNOWN: manifest unknown")],
  ]);
  const result = planPromotion({
    images: ["mcp-gateway"],
    tree: TREE,
    commit: COMMIT,
    repository: REPO,
    exec,
  });
  assert.equal(result.mode, "build");
  assert.deepEqual(result.digests, {});
  assert.equal(result.decisions[0].status, "missing");
  assert.match(result.decisions[0].detail, /MANIFEST_UNKNOWN/);
  assert.equal(tagsWritten(calls).length, 0);
});

test("failed provenance verification falls back and tags nothing", () => {
  const treeRef = `ghcr.io/proflyder/wakegrant:tree-${TREE}`;
  const { calls, exec } = recorder([
    [isDigestOf(treeRef), DIGEST_A],
    [isGhVerify, new Error("no attestations found")],
  ]);
  const result = planPromotion({
    images: ["wakegrant"],
    tree: TREE,
    commit: COMMIT,
    repository: REPO,
    exec,
  });
  assert.equal(result.mode, "build");
  assert.equal(result.decisions[0].status, "unverified");
  assert.equal(tagsWritten(calls).length, 0);
});

test("one unusable image falls back for the whole unit before any tag", () => {
  const okRef = `ghcr.io/proflyder/scheduler:tree-${TREE}`;
  const badRef = `ghcr.io/proflyder/ha-bridge:tree-${TREE}`;
  const { calls, exec } = recorder([
    [isDigestOf(okRef), DIGEST_A],
    [isDigestOf(badRef), DIGEST_B],
    [
      isGhVerify,
      (command, args) => {
        if (String(args[2]).includes("ha-bridge")) {
          throw new Error("certificate identity mismatch");
        }
        return "";
      },
    ],
  ]);
  const result = planPromotion({
    images: ["scheduler", "ha-bridge"],
    tree: TREE,
    commit: COMMIT,
    repository: REPO,
    exec,
  });
  // The verified sibling is not tagged: the unit deploys only as a whole.
  assert.equal(result.mode, "build");
  assert.deepEqual(
    result.decisions.map((decision) => [decision.image, decision.status]),
    [["scheduler", "verified"], ["ha-bridge", "unverified"]],
  );
  assert.equal(tagsWritten(calls).length, 0);
});

test("a tag that resolves to a different digest falls back", () => {
  const treeRef = `ghcr.io/proflyder/currency-bot:tree-${TREE}`;
  const repoRef = "ghcr.io/proflyder/currency-bot";
  const { calls, exec } = recorder([
    [isDigestOf(treeRef), DIGEST_A],
    [isGhVerify, ""],
    [isCraneTag, ""],
    // The tag read-back disagrees with the promoted digest.
    [isCraneDigest, DIGEST_B],
  ]);
  const result = planPromotion({
    images: ["currency-bot"],
    tree: TREE,
    commit: COMMIT,
    repository: REPO,
    exec,
  });
  assert.equal(result.mode, "build");
  assert.equal(result.decisions[0].status, "tag-failed");
  assert.match(result.decisions[0].detail, /resolves to/);
  assert.ok(tagsWritten(calls).some((call) => call[2] === `${repoRef}@${DIGEST_A}`));
});

test("invalid input fails the step instead of falling back", () => {
  const exec = () => {
    throw new Error("no command may run");
  };
  assert.throws(
    () => planPromotion({ images: ["scheduler"], tree: "short", commit: COMMIT, repository: REPO, exec }),
    /tree must be/,
  );
  assert.throws(
    () => planPromotion({ images: [], tree: TREE, commit: COMMIT, repository: REPO, exec }),
    /no images to promote/,
  );
  assert.throws(
    () => planPromotion({ images: ["scheduler"], tree: TREE, commit: "HEAD", repository: REPO, exec }),
    /commit must be/,
  );
  assert.throws(
    () => planPromotion({ images: ["scheduler"], tree: TREE, commit: COMMIT, repository: "", exec }),
    /invalid GitHub repository/,
  );
});

test("the workflow contract: promoted digests land in GITHUB_OUTPUT/ENV", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "promote-"));
  const output = path.join(dir, "output");
  const envFile = path.join(dir, "env");
  const summaryFile = path.join(dir, "summary");
  const treeRef = `ghcr.io/proflyder/scheduler:tree-${TREE}`;
  const { exec } = recorder([
    [isDigestOf(treeRef), DIGEST_A],
    [isGhVerify, ""],
    [isCraneTag, ""],
    [isCraneDigest, DIGEST_A],
  ]);
  try {
    const result = runPromotion({
      args: { images: ["scheduler"], tree: TREE, commit: COMMIT, repository: REPO },
      exec,
      env: {
        GITHUB_REPOSITORY: REPO,
        GITHUB_OUTPUT: output,
        GITHUB_ENV: envFile,
        GITHUB_STEP_SUMMARY: summaryFile,
      },
      log: () => {},
    });
    assert.equal(result.mode, "prebuilt");
    assert.equal(
      fs.readFileSync(output, "utf8"),
      `prebuilt_digests=scheduler=${DIGEST_A}\nmode=prebuilt\n`,
    );
    assert.equal(fs.readFileSync(envFile, "utf8"), "IMAGE_PROMOTION=prebuilt\n");
    assert.match(fs.readFileSync(summaryFile, "utf8"), /path: `prebuilt`/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("the fallback emits an empty digest list so the deploy builds", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "promote-"));
  const output = path.join(dir, "output");
  const envFile = path.join(dir, "env");
  const { exec } = recorder([
    [(command, args) => command === "crane" && args[0] === "digest",
      new Error("MANIFEST_UNKNOWN")],
  ]);
  try {
    const result = runPromotion({
      args: { images: ["scheduler"], tree: TREE, commit: COMMIT, repository: REPO },
      exec,
      env: { GITHUB_OUTPUT: output, GITHUB_ENV: envFile },
      log: () => {},
    });
    assert.equal(result.mode, "build");
    assert.equal(fs.readFileSync(output, "utf8"), "prebuilt_digests=\nmode=build\n");
    assert.equal(fs.readFileSync(envFile, "utf8"), "IMAGE_PROMOTION=build\n");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("output formats carry the digest list and the chosen path", () => {
  assert.equal(formatPrebuilt({ scheduler: DIGEST_A, "ha-bridge": DIGEST_B }), `scheduler=${DIGEST_A},ha-bridge=${DIGEST_B}`);
  assert.equal(formatPrebuilt({}), "");
  const summary = formatSummary({
    mode: "build",
    tag: `tree-${TREE}`,
    decisions: [{ image: "scheduler", status: "missing", detail: "not found" }],
    reason: "not every image is available and verified",
  });
  assert.match(summary, /image promotion \(tree-/);
  assert.match(summary, /`scheduler`: missing — not found/);
  assert.match(summary, /path: `build`/);
  assert.match(summary, /fallback: not every image/);
});
