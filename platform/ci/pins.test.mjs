// Unit tests for the pins writer (FLUX-APPS-PLAN 6.1).
//
// No network and no registry: the text update is pure, and the retry path is
// exercised through an injected `once`.
import assert from "node:assert/strict";
import test from "node:test";

import {
  PINS_ANNOTATIONS,
  commitMessage,
  imageKey,
  imageReference,
  parseDigests,
  pinUpdates,
  syncPins,
  updatePins,
} from "./pins.mjs";

const COMMIT = "b".repeat(40);
const TREE = "c".repeat(40);
const DIGEST_A = `sha256:${"1".repeat(64)}`;
const DIGEST_B = `sha256:${"2".repeat(64)}`;

const PINS = [
  "apiVersion: v1",
  "kind: ConfigMap",
  "metadata:",
  "  name: home-app-images",
  "  namespace: flux-system",
  "  annotations:",
  `    proflyder.dev/source-commit: "${"a".repeat(40)}"`,
  `    proflyder.dev/source-tree: "${"d".repeat(40)}"`,
  '    proflyder.dev/verified-at: "2026-11-10T12:00:00Z"',
  '    proflyder.dev/run: "https://github.com/proflyder/monorepo/actions/runs/1"',
  "data:",
  `  BUILD_ID: "${"a".repeat(12)}"`,
  `  INTERCOM_IMAGE: "ghcr.io/proflyder/intercom@${DIGEST_B}"`,
  `  WAKEGRANT_IMAGE: "ghcr.io/proflyder/wakegrant@${DIGEST_A}"`,
  "",
].join("\n");

const ANNOTATIONS = {
  "proflyder.dev/source-commit": COMMIT,
  "proflyder.dev/source-tree": TREE,
  "proflyder.dev/verified-at": "2026-11-11T00:00:00Z",
  "proflyder.dev/run": "https://github.com/proflyder/monorepo/actions/runs/2",
};

test("imageKey maps an image to its ConfigMap key", () => {
  assert.equal(imageKey("wakegrant"), "WAKEGRANT_IMAGE");
  assert.equal(imageKey("ha-bridge"), "HA_BRIDGE_IMAGE");
  assert.throws(() => imageKey("../evil"), /invalid image name/);
  assert.throws(() => imageKey(""), /invalid image name/);
});

test("imageReference pins the digest, never a tag", () => {
  assert.equal(
    imageReference("wakegrant", DIGEST_A),
    `ghcr.io/proflyder/wakegrant@${DIGEST_A}`,
  );
});

test("parseDigests accepts only full sha256 digests", () => {
  const digests = parseDigests(`wakegrant=${DIGEST_A},intercom=${DIGEST_B}`);
  assert.equal(digests.get("wakegrant"), DIGEST_A);
  assert.equal(digests.get("intercom"), DIGEST_B);
  assert.throws(() => parseDigests("wakegrant=sha256:abc"), /invalid digest/);
  assert.throws(() => parseDigests("wakegrant"), /invalid digest entry/);
});

test("pinUpdates needs a verified digest for every image", () => {
  const digests = parseDigests(`wakegrant=${DIGEST_A}`);
  const values = pinUpdates({ images: ["wakegrant"], digests, commit: COMMIT });
  assert.deepEqual(values, {
    BUILD_ID: "b".repeat(12),
    WAKEGRANT_IMAGE: `ghcr.io/proflyder/wakegrant@${DIGEST_A}`,
  });
  assert.throws(
    () => pinUpdates({ images: ["intercom"], digests, commit: COMMIT }),
    /no verified digest for image 'intercom'/,
  );
});

test("updatePins rewrites only this unit's keys and the annotations", () => {
  const values = pinUpdates({
    images: ["wakegrant"],
    digests: parseDigests(`wakegrant=${DIGEST_B}`),
    commit: COMMIT,
  });
  const { text, changed } = updatePins(PINS, { values, annotations: ANNOTATIONS });
  assert.ok(changed);
  assert.match(text, new RegExp(`WAKEGRANT_IMAGE: "ghcr.io/proflyder/wakegrant@${DIGEST_B}"`));
  // Another unit's digest is preserved verbatim.
  assert.match(text, new RegExp(`INTERCOM_IMAGE: "ghcr.io/proflyder/intercom@${DIGEST_B}"`));
  assert.match(text, new RegExp(`BUILD_ID: "${"b".repeat(12)}"`));
  for (const key of PINS_ANNOTATIONS) {
    assert.match(text, new RegExp(`${key}: "`));
  }
  assert.match(text, new RegExp(`proflyder.dev/source-commit: "${COMMIT}"`));
  assert.match(text, new RegExp(`proflyder.dev/verified-at: "2026-11-11T00:00:00Z"`));
});

test("updatePins is idempotent", () => {
  const values = pinUpdates({
    images: ["wakegrant"],
    digests: parseDigests(`wakegrant=${DIGEST_A}`),
    commit: "a".repeat(40),
  });
  const annotations = {
    "proflyder.dev/source-commit": "a".repeat(40),
    "proflyder.dev/source-tree": "d".repeat(40),
    "proflyder.dev/verified-at": "2026-11-10T12:00:00Z",
    "proflyder.dev/run": "https://github.com/proflyder/monorepo/actions/runs/1",
  };
  const first = updatePins(PINS, { values, annotations });
  assert.equal(first.changed, false);
  assert.equal(first.text, PINS);
});

test("updatePins adds a new unit key in sorted order", () => {
  const values = pinUpdates({
    images: ["scheduler"],
    digests: parseDigests(`scheduler=${DIGEST_A}`),
    commit: COMMIT,
  });
  const { text } = updatePins(PINS, { values, annotations: ANNOTATIONS });
  const dataKeys = text
    .split("\n")
    .filter((line) => /^ {2}[A-Z0-9_]+:/.test(line))
    .map((line) => line.trim().split(":")[0]);
  assert.deepEqual(dataKeys, ["BUILD_ID", "INTERCOM_IMAGE", "SCHEDULER_IMAGE", "WAKEGRANT_IMAGE"]);
});

test("updatePins refuses unmanaged annotations", () => {
  assert.throws(
    () => updatePins(PINS, { values: {}, annotations: { "proflyder.dev/owner": "x" } }),
    /unmanaged annotation/,
  );
});

test("commitMessage names the unit and the commit", () => {
  assert.equal(commitMessage("wakegrant", COMMIT), `chore(pins): wakegrant ${"b".repeat(12)}`);
});

test("syncPins retries a raced push and reports the first success", () => {
  const attempts = [];
  const result = syncPins(
    { unit: "wakegrant", attempts: 3 },
    {
      log: () => {},
      once: () => {
        attempts.push(1);
        if (attempts.length < 3) {
          throw new Error("non-fast-forward");
        }
        return { changed: true, commit: "f".repeat(40) };
      },
    },
  );
  assert.equal(attempts.length, 3);
  assert.deepEqual(result, { changed: true, commit: "f".repeat(40) });
});

test("syncPins fails after the last attempt", () => {
  assert.throws(
    () =>
      syncPins(
        { unit: "wakegrant", attempts: 2 },
        { log: () => {}, once: () => { throw new Error("push rejected"); } },
      ),
    /push rejected/,
  );
});
