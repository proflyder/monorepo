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
  expandTags,
  formatNativeSummary,
  formatPrebuilt,
  formatSummary,
  imageRef,
  manifestContent,
  nativeSignerIdentityPattern,
  parseDigest,
  planNativePromotion,
  planPromotion,
  runPromotion,
  sameManifestContent,
  signerIdentityPattern,
  treeTag,
  verifyArgs,
} from "./promote.mjs";

const TREE = "a".repeat(40);
const COMMIT = "b".repeat(40);
const REPO = "acme/home-lab";
const DIGEST_A = "sha256:" + "1".repeat(64);
const DIGEST_B = "sha256:" + "2".repeat(64);
const DIGEST_C = "sha256:" + "3".repeat(64);

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

test("verify args pin repository and the signing workflow identity", () => {
  const args = verifyArgs({ repository: REPO, reference: `ghcr.io/proflyder/scheduler@${DIGEST_A}` });
  assert.deepEqual(args.slice(0, 2), ["attestation", "verify"]);
  assert.equal(args[2], `oci://ghcr.io/proflyder/scheduler@${DIGEST_A}`);
  assert.deepEqual(args.slice(3, 5), ["--repo", REPO]);
  assert.equal(args[5], "--cert-identity-regex");
  assert.equal(args[6], signerIdentityPattern(REPO));
  // gh rejects --signer-repo combined with --cert-identity-regex.
  assert.ok(!args.includes("--signer-repo"));
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

test("native signer pattern is bound to ci/_native and the repository", () => {
  const pattern = nativeSignerIdentityPattern(REPO);
  const regex = new RegExp(pattern);
  assert.ok(regex.test("https://github.com/acme/home-lab/.github/workflows/_native.yml@refs/pull/7/merge"));
  assert.ok(regex.test("https://github.com/acme/home-lab/.github/workflows/ci.yml@refs/pull/7/merge"));
  assert.ok(!regex.test("https://github.com/acme/home-lab/.github/workflows/deploy-home.yml@refs/heads/develop"));
  assert.ok(!regex.test("https://github.com/evil/home-lab/.github/workflows/_native.yml@refs/pull/7/merge"));
});

test("native tag templates expand version and commit", () => {
  assert.deepEqual(
    expandTags(["{version}", "sha-{commit12}", "latest"], { version: "v0.5.1008", commit: COMMIT }),
    ["v0.5.1008", "sha-bbbbbbbbbbbb", "latest"],
  );
  assert.deepEqual(expandTags(["latest"], { version: "1.2.3", commit: COMMIT }), ["latest"]);
  assert.throws(() => expandTags([], { version: "v1.2.3", commit: COMMIT }), /no publish tags/);
  assert.throws(() => expandTags(["latest"], { version: "nope", commit: COMMIT }), /version must be/);
  assert.throws(() => expandTags(["bad tag"], { version: "v1.2.3", commit: COMMIT }), /invalid publish tag/);
});

function nativeHandlers({ ref, tree, digest, existing = {}, manifests = {}, candidate = null }) {
  const treeRef = `${ref}:tree-${tree}`;
  const tagged = new Map();
  const candidateManifest = candidate || artifactManifest();
  // `crane digest <ref>:<tag>` -> args[1]; `crane tag <ref>@<digest> <tag>` -> args[2].
  const tagOf = (args) => String(args[args.length - 1]).slice(
    String(args[args.length - 1]).lastIndexOf(":") + 1,
  );
  return [
    [
      (command, args) => command === "crane" && args[0] === "digest" && args[1] === treeRef,
      digest,
    ],
    [isGhVerify, ""],
    [
      isCraneTag,
      (command, args) => {
        const source = String(args[1]).slice(String(args[1]).lastIndexOf("@") + 1);
        tagged.set(tagOf(args), parseDigest(source) || digest);
        return "";
      },
    ],
    [
      (command, args) => command === "crane" && args[0] === "digest",
      (command, args) => {
        const tag = tagOf(args);
        if (tagged.has(tag)) {
          return tagged.get(tag);
        }
        if (Object.hasOwn(existing, tag)) {
          const value = existing[tag];
          if (value instanceof Error) throw value;
          return value;
        }
        throw new Error("MANIFEST_UNKNOWN: manifest unknown");
      },
    ],
    [
      (command, args) => command === "crane" && args[0] === "manifest",
      (command, args) => {
        const target = String(args[1]);
        if (target === `${ref}@${digest}`) {
          return candidateManifest;
        }
        const tag = target.slice(target.lastIndexOf(":") + 1);
        if (Object.hasOwn(manifests, tag)) {
          const value = manifests[tag];
          if (value instanceof Error) throw value;
          return value;
        }
        throw new Error("MANIFEST_UNKNOWN: manifest unknown");
      },
    ],
  ];
}

const LAYER_DIGESTS = {
  "RicardoKompLogon.dll": `sha256:${"d".repeat(64)}`,
  "cp-selftest.exe": `sha256:${"e".repeat(64)}`,
  SHA256SUMS: `sha256:${"a".repeat(64)}`,
};

/** Release-shaped manifest; only the annotations differ between rebuilds. */
function artifactManifest({
  artifactType = "application/vnd.example.windows-logon-provider.v1",
  layerDigests = LAYER_DIGESTS,
  annotations = { "org.opencontainers.image.created": "2026-01-01T00:00:00Z" },
} = {}) {
  return JSON.stringify({
    schemaVersion: 2,
    mediaType: "application/vnd.oci.image.manifest.v1+json",
    artifactType,
    config: {
      mediaType: "application/vnd.oci.empty.v1+json",
      digest: `sha256:${"0".repeat(64)}`,
      size: 2,
    },
    layers: Object.entries(layerDigests).map(([title, layerDigest]) => ({
      mediaType:
        title.endsWith(".dll") || title.endsWith(".exe")
          ? "application/vnd.microsoft.portable-executable"
          : "text/plain",
      digest: layerDigest,
      size: 123,
      annotations: { "org.opencontainers.image.title": title },
    })),
    annotations,
  });
}

test("native artifact is promoted to version and latest without rebuilding", () => {
  const ref = "ghcr.io/proflyder/pulsar-firmware";
  const { calls, exec } = recorder(
    nativeHandlers({ ref, tree: TREE, digest: DIGEST_A, existing: { latest: DIGEST_B } }),
  );
  const result = planNativePromotion({
    repository: ref,
    tree: TREE,
    commit: COMMIT,
    version: "v0.5.1008",
    tags: ["{version}", "latest"],
    githubRepository: REPO,
    exec,
  });
  assert.equal(result.mode, "promoted");
  assert.equal(result.digest, DIGEST_A);
  assert.deepEqual(tagsWritten(calls), [
    ["crane", "tag", `${ref}@${DIGEST_A}`, "v0.5.1008"],
    ["crane", "tag", `${ref}@${DIGEST_A}`, "latest"],
  ]);
  assert.equal(result.decisions.find((item) => item.tag === "latest").status, "moved");
  assert.equal(result.decisions.find((item) => item.tag === "v0.5.1008").status, "tagged");
});

test("a missing native tree tag falls back to a build", () => {
  const ref = "ghcr.io/proflyder/windows-logon-provider";
  const { calls, exec } = recorder([
    [
      (command, args) => command === "crane" && args[0] === "digest",
      new Error("MANIFEST_UNKNOWN: manifest unknown"),
    ],
  ]);
  const result = planNativePromotion({
    repository: ref,
    tree: TREE,
    commit: COMMIT,
    version: "1.2.3",
    tags: ["{version}", "sha-{commit12}"],
    githubRepository: REPO,
    exec,
  });
  assert.equal(result.mode, "build");
  assert.equal(result.decisions[0].status, "missing");
  assert.equal(tagsWritten(calls).length, 0);
  assert.match(formatNativeSummary(result), /fallback build/);
});

test("unverified native provenance falls back and tags nothing", () => {
  const ref = "ghcr.io/proflyder/pulsar-firmware";
  const { calls, exec } = recorder([
    [
      (command, args) => command === "crane" && args[0] === "digest",
      DIGEST_A,
    ],
    [isGhVerify, new Error("no attestations found")],
  ]);
  const result = planNativePromotion({
    repository: ref,
    tree: TREE,
    commit: COMMIT,
    version: "v0.5.1008",
    tags: ["{version}", "latest"],
    githubRepository: REPO,
    exec,
  });
  assert.equal(result.mode, "build");
  assert.equal(result.decisions[0].status, "unverified");
  assert.equal(tagsWritten(calls).length, 0);
});

test("an existing version tag with different layer content is refused", () => {
  const ref = "ghcr.io/proflyder/pulsar-firmware";
  const changed = artifactManifest({
    layerDigests: { ...LAYER_DIGESTS, "pulsar-aarch64.bin": `sha256:${"9".repeat(64)}` },
  });
  const { calls, exec } = recorder(
    nativeHandlers({
      ref,
      tree: TREE,
      digest: DIGEST_A,
      existing: { "v0.5.1008": DIGEST_B },
      manifests: { "v0.5.1008": changed },
    }),
  );
  assert.throws(
    () =>
      planNativePromotion({
        repository: ref,
        tree: TREE,
        commit: COMMIT,
        version: "v0.5.1008",
        tags: ["{version}", "latest"],
        githubRepository: REPO,
        exec,
      }),
    /already exists with .*immutable by content/,
  );
  // The conflicting immutable tag was not overwritten, and `latest` was never
  // reached because the version is checked first.
  assert.equal(tagsWritten(calls).length, 0);
});

test("identical content with different annotations is a no-op", () => {
  const ref = "ghcr.io/proflyder/windows-logon-provider";
  // Two builds of the same files: same artifact type and layers, but different
  // manifest digests because only `created`/`revision` annotations changed.
  const candidate = artifactManifest({
    annotations: {
      "org.opencontainers.image.created": "2026-02-02T00:00:00Z",
      "org.opencontainers.image.revision": COMMIT,
    },
  });
  const existingManifest = artifactManifest({
    annotations: {
      "org.opencontainers.image.created": "2026-01-01T00:00:00Z",
      "org.opencontainers.image.revision": "c".repeat(40),
    },
  });
  const { calls, exec } = recorder(
    nativeHandlers({
      ref,
      tree: TREE,
      digest: DIGEST_A,
      existing: { "1.2.3": DIGEST_B, "sha-bbbbbbbbbbbb": DIGEST_B },
      manifests: { "1.2.3": existingManifest, "sha-bbbbbbbbbbbb": existingManifest },
      candidate,
    }),
  );
  const result = planNativePromotion({
    repository: ref,
    tree: TREE,
    commit: COMMIT,
    version: "1.2.3",
    tags: ["{version}", "sha-{commit12}"],
    githubRepository: REPO,
    exec,
  });
  assert.equal(result.mode, "promoted");
  assert.equal(result.digest, DIGEST_B);
  assert.equal(tagsWritten(calls).length, 0);
  const kept = result.decisions.filter((item) => item.status === "unchanged");
  assert.deepEqual(kept.map((item) => item.tag), ["1.2.3", "sha-bbbbbbbbbbbb"]);
  for (const item of kept) {
    assert.equal(item.digest, DIGEST_B);
    assert.match(item.detail, /already published with identical content/);
  }
  // The digest of the kept version tag is what the summary reports.
  assert.match(formatNativeSummary(result), /already published with identical content/);
});

test("latest follows the kept version tag digest, not the candidate", () => {
  const ref = "ghcr.io/proflyder/pulsar-firmware";
  const candidate = artifactManifest({
    annotations: { "org.opencontainers.image.created": "2026-02-02T00:00:00Z" },
  });
  const existingManifest = artifactManifest({
    annotations: { "org.opencontainers.image.created": "2026-01-01T00:00:00Z" },
  });
  const { calls, exec } = recorder(
    nativeHandlers({
      ref,
      tree: TREE,
      digest: DIGEST_A,
      existing: { "v0.5.1008": DIGEST_B, latest: DIGEST_C },
      manifests: { "v0.5.1008": existingManifest },
      candidate,
    }),
  );
  const result = planNativePromotion({
    repository: ref,
    tree: TREE,
    commit: COMMIT,
    version: "v0.5.1008",
    tags: ["{version}", "latest"],
    githubRepository: REPO,
    exec,
  });
  assert.equal(result.mode, "promoted");
  assert.equal(result.digest, DIGEST_B);
  // The version tag was kept; `latest` moves to its digest, not the candidate.
  assert.deepEqual(tagsWritten(calls), [["crane", "tag", `${ref}@${DIGEST_B}`, "latest"]]);
  assert.equal(result.decisions.find((item) => item.tag === "latest").status, "moved");
  assert.equal(result.decisions.find((item) => item.tag === "latest").digest, DIGEST_B);
});

test("latest is unchanged when it already points at the kept version digest", () => {
  const ref = "ghcr.io/proflyder/pulsar-firmware";
  const existingManifest = artifactManifest();
  const { calls, exec } = recorder(
    nativeHandlers({
      ref,
      tree: TREE,
      digest: DIGEST_A,
      existing: { "v0.5.1008": DIGEST_B, latest: DIGEST_B },
      manifests: { "v0.5.1008": existingManifest },
    }),
  );
  const result = planNativePromotion({
    repository: ref,
    tree: TREE,
    commit: COMMIT,
    version: "v0.5.1008",
    tags: ["{version}", "latest"],
    githubRepository: REPO,
    exec,
  });
  assert.equal(tagsWritten(calls).length, 0);
  assert.equal(result.decisions.find((item) => item.tag === "latest").status, "unchanged");
});

test("a sha tag follows the same content identity rule", () => {
  const ref = "ghcr.io/proflyder/windows-logon-provider";
  const changed = artifactManifest({
    layerDigests: { ...LAYER_DIGESTS, SHA256SUMS: `sha256:${"8".repeat(64)}` },
  });
  // Same content under a different digest is kept.
  {
    const { calls, exec } = recorder(
      nativeHandlers({
        ref,
        tree: TREE,
        digest: DIGEST_A,
        existing: { "sha-bbbbbbbbbbbb": DIGEST_B },
        manifests: { "sha-bbbbbbbbbbbb": artifactManifest() },
      }),
    );
    const result = planNativePromotion({
      repository: ref,
      tree: TREE,
      commit: COMMIT,
      version: "1.2.3",
      tags: ["sha-{commit12}"],
      githubRepository: REPO,
      exec,
    });
    assert.equal(result.mode, "promoted");
    assert.equal(tagsWritten(calls).length, 0);
    assert.equal(result.decisions.find((item) => item.tag === "sha-bbbbbbbbbbbb").status, "unchanged");
  }
  // Different content under the same sha tag is a hard failure.
  {
    const { exec } = recorder(
      nativeHandlers({
        ref,
        tree: TREE,
        digest: DIGEST_A,
        existing: { "sha-bbbbbbbbbbbb": DIGEST_B },
        manifests: { "sha-bbbbbbbbbbbb": changed },
      }),
    );
    assert.throws(
      () =>
        planNativePromotion({
          repository: ref,
          tree: TREE,
          commit: COMMIT,
          version: "1.2.3",
          tags: ["sha-{commit12}"],
          githubRepository: REPO,
          exec,
        }),
      /already exists with .*immutable by content/,
    );
  }
});

test("manifestContent compares ordered layer identity and artifact type", () => {
  const base = manifestContent(artifactManifest());
  assert.equal(base.artifactType, "application/vnd.example.windows-logon-provider.v1");
  assert.deepEqual(base.layers.map((layer) => layer.title), [
    "RicardoKompLogon.dll",
    "cp-selftest.exe",
    "SHA256SUMS",
  ]);
  // Manifest-level annotations are not content identity.
  const annotated = manifestContent(
    artifactManifest({ annotations: { "org.opencontainers.image.created": "later" } }),
  );
  assert.equal(sameManifestContent(base, annotated), true);
  // Layer order matters.
  const reordered = manifestContent(
    artifactManifest({
      layerDigests: {
        "cp-selftest.exe": LAYER_DIGESTS["cp-selftest.exe"],
        "RicardoKompLogon.dll": LAYER_DIGESTS["RicardoKompLogon.dll"],
        SHA256SUMS: LAYER_DIGESTS.SHA256SUMS,
      },
    }),
  );
  assert.equal(sameManifestContent(base, reordered), false);
  // Artifact type matters.
  const otherType = manifestContent(artifactManifest({ artifactType: "application/vnd.other" }));
  assert.equal(sameManifestContent(base, otherType), false);
  assert.throws(() => manifestContent("not json"), /not valid JSON/);
  assert.throws(() => manifestContent("{}"), /neither layers nor manifests/);
});

test("an identical existing tag is a no-op", () => {
  const ref = "ghcr.io/proflyder/windows-logon-provider";
  const { calls, exec } = recorder(
    nativeHandlers({
      ref,
      tree: TREE,
      digest: DIGEST_A,
      existing: { "1.2.3": DIGEST_A, "sha-bbbbbbbbbbbb": DIGEST_A },
    }),
  );
  const result = planNativePromotion({
    repository: ref,
    tree: TREE,
    commit: COMMIT,
    version: "1.2.3",
    tags: ["{version}", "sha-{commit12}"],
    githubRepository: REPO,
    exec,
  });
  assert.equal(result.mode, "promoted");
  assert.equal(tagsWritten(calls).length, 0);
  assert.deepEqual(
    result.decisions.filter((item) => item.status === "unchanged").map((item) => item.tag),
    ["1.2.3", "sha-bbbbbbbbbbbb"],
  );
});

test("native invalid input fails the step instead of falling back", () => {
  const exec = () => {
    throw new Error("no command may run");
  };
  assert.throws(
    () =>
      planNativePromotion({
        repository: "https://example.test/x",
        tree: TREE,
        commit: COMMIT,
        version: "v1.2.3",
        tags: ["latest"],
        githubRepository: REPO,
        exec,
      }),
    /invalid native repository/,
  );
  assert.throws(
    () =>
      planNativePromotion({
        repository: "ghcr.io/proflyder/pulsar-firmware",
        tree: "short",
        commit: COMMIT,
        version: "v1.2.3",
        tags: ["latest"],
        githubRepository: REPO,
        exec,
      }),
    /tree must be/,
  );
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
