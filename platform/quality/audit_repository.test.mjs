#!/usr/bin/env node
/**
 * Boundary tests for the reviewed public-name allowances in the audit scanner.
 *
 * Sample strings are built by concatenation so this file never contains the
 * personal name as a literal and cannot flag itself.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { scanText, sanitizeApprovedTokens } from "./audit_repository.mjs";

const NAME = "prof" + "lyder";
const DOMAIN = `${NAME}.dev`;
const PACKAGE = `dev.${NAME}.home`;
const SOURCE_PATH = `dev/${NAME}/home`;
const REPO = `${NAME}/pulsar`;
const LEGACY_HOME = "/Users/stanislav" + "plotnikov";

test("audit: exact public domain is accepted", () => {
  for (const text of [
    `Сайт: https://${DOMAIN}`,
    `Домен ${DOMAIN} используется для приложения.`,
    `link http://${DOMAIN}/path?q=1`,
  ]) {
    assert.deepEqual(scanText("docs/example.md", text), [], text);
  }
});

test("audit: android namespace and subpackages are accepted", () => {
  for (const text of [
    `applicationId = "${PACKAGE}"`,
    `package ${PACKAGE}.ui`,
    `import ${PACKAGE}.config.AppConfig`,
  ]) {
    assert.deepEqual(
      scanText("projects/mobile/android/app/build.gradle", text),
      [],
      text,
    );
  }
});

test("audit: source and resource paths are accepted", () => {
  for (const text of [
    `src/main/java/${SOURCE_PATH}/MainActivity.kt`,
    `res/${SOURCE_PATH}/values/strings.xml`,
  ]) {
    assert.deepEqual(scanText("projects/mobile/android/README.md", text), [], text);
  }
});

test("audit: sanitizer replaces only approved tokens", () => {
  const line = `${PACKAGE} ${DOMAIN} ${SOURCE_PATH} ${REPO}`;
  const cleaned = sanitizeApprovedTokens(line);
  assert.ok(!cleaned.includes(NAME));
  assert.ok(cleaned.includes("<public-namespace>"));
  assert.ok(cleaned.includes("<public-domain>"));
  assert.ok(cleaned.includes("<public-source-path>"));
  assert.ok(cleaned.includes("<public-repo>"));
});

test("audit: exact public repo slug is accepted", () => {
  for (const text of [
    `\`UPDATE_REPO=${REPO}\`,`,
    `const UPDATE_REPO: &str = "${REPO}";`,
    `https://github.com/${REPO}`,
  ]) {
    assert.deepEqual(
      scanText("projects/station/pulsar/RICARDO.md", text),
      [],
      text,
    );
  }
});

function assertFlagged(text) {
  const errors = scanText("docs/example.md", text);
  assert.ok(errors.length, `expected a finding for ${JSON.stringify(text)}`);
}

test("audit: bare personal name is rejected", () => {
  assertFlagged(`Автор ${NAME} пишет код.`);
});

test("audit: unrelated com domain is rejected", () => {
  assertFlagged(`https://${NAME}.com`);
});

test("audit: longer host is rejected", () => {
  assertFlagged(`https://${DOMAIN}.example`);
});

test("audit: prefixed host is rejected", () => {
  assertFlagged(`https://evil${NAME}.dev`);
});

test("audit: subdomain is rejected", () => {
  assertFlagged(`https://sub.${DOMAIN}`);
});

test("audit: longer identifier is rejected", () => {
  assertFlagged(`package dev.${NAME}.homepage`);
});

test("audit: prefixed namespace is rejected", () => {
  assertFlagged(`package foo.dev.${NAME}.home`);
});

test("audit: longer source directory is rejected", () => {
  assertFlagged(`path ${SOURCE_PATH}page/Main.kt`);
});

test("audit: prefixed source directory is rejected", () => {
  assertFlagged(`path x${SOURCE_PATH}/Main.kt`);
});

test("audit: legacy private path is rejected", () => {
  assertFlagged(`${LEGACY_HOME}/work`);
});

test("audit: other repository of handle is rejected", () => {
  assertFlagged(`${NAME}/other`);
});

test("audit: longer repository name is rejected", () => {
  assertFlagged(`${REPO}-private`);
  assertFlagged(`${REPO}2`);
  assertFlagged(`${REPO}.private`);
  assertFlagged(`${REPO}.secret`);
  assertFlagged(`${REPO}.git`);
});

test("audit: prefixed repository path is rejected", () => {
  assertFlagged(`https://github.com/evil${REPO}`);
});

test("audit: mixed public repo and token is rejected", () => {
  const token = `ghp_${"c".repeat(36)}`;
  assertFlagged(`${REPO} token=${token}`);
});

test("audit: mixed public repo and private key is rejected", () => {
  const keyHeader = "-----BEGIN " + "PRIVATE KEY-----";
  assertFlagged(`${REPO} ${keyHeader}`);
});

test("audit: mixed approved namespace and private marker is rejected", () => {
  assertFlagged(`${PACKAGE} ${LEGACY_HOME}`);
});

test("audit: mixed approved domain and token is rejected", () => {
  const token = `ghp_${"a".repeat(36)}`;
  assertFlagged(`${DOMAIN} token=${token}`);
});

test("audit: mixed approved path and private key is rejected", () => {
  const keyHeader = "-----BEGIN " + "PRIVATE KEY-----";
  assertFlagged(`${SOURCE_PATH}/key ${keyHeader}`);
});

test("audit: private key and api key patterns still fire", () => {
  const keyHeader = "-----BEGIN " + "OPENSSH PRIVATE KEY-----";
  assert.ok(scanText("x", keyHeader).length);
  assert.ok(scanText("x", "export KEY=sk-" + "b".repeat(30)).length);
});

test("audit: line numbers are reported", () => {
  const text = `${PACKAGE}\n${NAME}\n`;
  assert.deepEqual(scanText("docs/example.md", text), [
    "credential pattern: docs/example.md:2",
  ]);
});

test("audit: scanner source does not flag itself", () => {
  const here = path.dirname(fileURLToPath(import.meta.url));
  const source = fs.readFileSync(path.join(here, "audit_repository.mjs"), "utf8");
  assert.deepEqual(scanText("platform/quality/audit_repository.mjs", source), []);
});
