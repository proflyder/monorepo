// Firebase App Distribution retention tests.
//
// `planReleaseCleanup` is pure, so these tests pin the retention contract
// exactly: a release older than 24 hours is deleted, a release at the boundary
// or younger is kept, and the newest release is never deleted even when it is
// the only thing older than the window. Listing must follow `nextPageToken`,
// and deletion must batch names without letting one refused batch hide the
// others. OAuth is exercised with a real ephemeral RSA key so the assertion
// signature is actually verified; every HTTP call is stubbed, so the suite
// never reaches Google.
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  API_BASE,
  DELETE_CHUNK,
  HOUR_MS,
  MAX_AGE_MS,
  accessToken,
  buildAssertion,
  deleteReleases,
  firebaseAppId,
  listReleases,
  main,
  planReleaseCleanup,
  projectFromAppId,
} from "./fad_cleanup.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const NOW = Date.parse("2026-11-14T12:00:00Z");
const APP_ID = "1:1234567890:android:abc123def456";

function release(id, createdMs, displayName = "") {
  return {
    name: `projects/1234567890/apps/${APP_ID}/releases/${id}`,
    displayName,
    createTime: new Date(createdMs).toISOString(),
  };
}

function byName(plan) {
  return Object.fromEntries(plan.decisions.map((item) => [item.name, item]));
}

test("an empty release list plans nothing", () => {
  const plan = planReleaseCleanup([], { now: NOW });
  assert.equal(plan.newest, null);
  assert.deepEqual(plan.decisions, []);
  assert.deepEqual(plan.delete, []);
  assert.deepEqual(plan.keep, []);
});

test("releases older than 24 hours are deleted", () => {
  const plan = planReleaseCleanup(
    [release("new", NOW - HOUR_MS), release("old", NOW - 48 * HOUR_MS)],
    { now: NOW },
  );
  assert.deepEqual(plan.delete.map((item) => item.name.split("/").pop()), ["old"]);
});

test("the boundary is strict: exactly 24 hours old is kept", () => {
  const atBoundary = release("boundary", NOW - MAX_AGE_MS);
  const boundary = planReleaseCleanup([release("new", NOW), atBoundary], { now: NOW });
  assert.equal(boundary.delete.length, 0);
  assert.equal(byName(boundary)[atBoundary.name].reason, "within the 24h window");

  const past = planReleaseCleanup(
    [release("new", NOW), release("past", NOW - MAX_AGE_MS - 1)],
    { now: NOW },
  );
  assert.deepEqual(past.delete.map((item) => item.name.split("/").pop()), ["past"]);
});

test("the newest release is never deleted, whatever its age", () => {
  const plan = planReleaseCleanup(
    [release("newest", NOW - 30 * 24 * HOUR_MS), release("older", NOW - 60 * 24 * HOUR_MS)],
    { now: NOW },
  );
  assert.equal(plan.delete.length, 1);
  assert.equal(plan.delete[0].name.split("/").pop(), "older");
  assert.equal(plan.newest, plan.keep.find((item) => item.reason === "newest release").name);
});

test("releases sharing the newest createTime are all kept", () => {
  const plan = planReleaseCleanup(
    [
      release("a", NOW - 10 * HOUR_MS),
      release("b", NOW - 10 * HOUR_MS),
      release("c", NOW - 40 * HOUR_MS),
    ],
    { now: NOW },
  );
  assert.deepEqual(plan.delete.map((item) => item.name.split("/").pop()), ["c"]);
});

test("a release without a parseable createTime is kept", () => {
  const unknown = { name: "projects/1/apps/x/releases/broken", createTime: "not a date" };
  const plan = planReleaseCleanup([release("new", NOW), unknown], { now: NOW });
  assert.equal(plan.delete.length, 0);
  assert.equal(byName(plan)[unknown.name].reason, "unknown createTime");
});

test("a custom window scales the decision", () => {
  const plan = planReleaseCleanup(
    [release("new", NOW), release("old", NOW - 2 * HOUR_MS)],
    { now: NOW, maxAgeMs: HOUR_MS },
  );
  assert.deepEqual(plan.delete.map((item) => item.name.split("/").pop()), ["old"]);
});

test("project number and app id come from the same Firebase identifiers", () => {
  assert.equal(projectFromAppId(APP_ID), "1234567890");
  assert.throws(() => projectFromAppId("not-an-app-id"), /1:<project number>:android/);

  const services = {
    client: [
      { client_info: { android_client_info: { package_name: "dev.other" }, mobilesdk_app_id: "1:9:android:other" } },
      { client_info: { android_client_info: { package_name: "dev.proflyder.home" }, mobilesdk_app_id: APP_ID } },
    ],
  };
  assert.equal(firebaseAppId(services), APP_ID);
  assert.equal(firebaseAppId(services, "dev.other"), "1:9:android:other");
  assert.throws(() => firebaseAppId(services, "dev.missing"), /no client for dev.missing/);
});

test("listReleases follows nextPageToken and asks for newest first", async () => {
  const calls = [];
  const pages = [
    { releases: [release("a", NOW), release("b", NOW - HOUR_MS)], nextPageToken: "page-2" },
    { releases: [release("c", NOW - 25 * HOUR_MS)] },
  ];
  const fetchImpl = async (url) => {
    calls.push(url);
    return {
      ok: true,
      status: 200,
      text: async () => JSON.stringify(pages[calls.length - 1]),
    };
  };
  const releases = await listReleases({
    accessToken: "token",
    appName: `projects/1234567890/apps/${APP_ID}`,
    fetchImpl,
    pageSize: 2,
  });
  assert.deepEqual(releases.map((item) => item.name.split("/").pop()), ["a", "b", "c"]);
  assert.equal(calls.length, 2);
  const first = new URL(calls[0]);
  assert.equal(first.searchParams.get("orderBy"), "createTime desc");
  assert.equal(first.searchParams.get("pageSize"), "2");
  assert.equal(first.searchParams.get("pageToken"), null);
  assert.equal(new URL(calls[1]).searchParams.get("pageToken"), "page-2");
  assert.ok(calls[0].startsWith(`${API_BASE}/projects/1234567890/apps/`));
});

test("a list failure surfaces the permission hint", async () => {
  const fetchImpl = async () => ({ ok: false, status: 403, text: async () => "{}" });
  await assert.rejects(
    listReleases({
      accessToken: "token",
      appName: `projects/1234567890/apps/${APP_ID}`,
      fetchImpl,
    }),
    /Firebase App Distribution Admin/,
  );
});

function fakeFetch({ failOn = [] } = {}) {
  const calls = [];
  const fetchImpl = async (url, options) => {
    const names = options?.body ? JSON.parse(options.body).names : [];
    calls.push({ url, names });
    if (failOn.some((prefix) => names[0]?.includes(prefix))) {
      return { ok: false, status: 403, text: async () => '{"error":"forbidden"}' };
    }
    return { ok: true, status: 200, text: async () => "{}" };
  };
  return { fetchImpl, calls };
}

test("deleteReleases batches names in bounded chunks", async () => {
  const { fetchImpl, calls } = fakeFetch();
  const names = Array.from({ length: DELETE_CHUNK + 5 }, (_, index) => `releases/${index}`);
  const result = await deleteReleases({
    accessToken: "token",
    appName: "projects/1/apps/app",
    names,
    fetchImpl,
  });
  assert.equal(result.deleted.length, names.length);
  assert.deepEqual(result.failures, []);
  assert.equal(calls.length, 2);
  assert.equal(calls[0].names.length, DELETE_CHUNK);
  assert.equal(calls[0].url, `${API_BASE}/projects/1/apps/app/releases:batchDelete`);
});

test("a refused batch is reported without abandoning the others", async () => {
  const { fetchImpl } = fakeFetch({ failOn: ["releases/0"] });
  const result = await deleteReleases({
    accessToken: "token",
    appName: "projects/1/apps/app",
    names: ["releases/0", "releases/1"],
    fetchImpl,
    chunkSize: 1,
  });
  assert.deepEqual(result.deleted, ["releases/1"]);
  assert.equal(result.failures.length, 1);
  assert.equal(result.failures[0].status, 403);
});

test("the service-account assertion is signed and carries the cloud-platform scope", () => {
  const { privateKey, publicKey } = crypto.generateKeyPairSync("rsa", { modulusLength: 2048 });
  const credentials = {
    client_email: "ci@example.iam.gserviceaccount.com",
    private_key: privateKey.export({ type: "pkcs8", format: "pem" }),
  };
  const assertion = buildAssertion(credentials, { now: NOW });
  const [header, claims, signature] = assertion.split(".");
  assert.deepEqual(JSON.parse(Buffer.from(header, "base64url")), { alg: "RS256", typ: "JWT" });
  const payload = JSON.parse(Buffer.from(claims, "base64url"));
  assert.equal(payload.iss, "ci@example.iam.gserviceaccount.com");
  assert.equal(payload.scope, "https://www.googleapis.com/auth/cloud-platform");
  assert.equal(payload.iat, Math.floor(NOW / 1000));
  assert.equal(payload.exp, Math.floor(NOW / 1000) + 3600);
  assert.ok(
    crypto.verify(
      "RSA-SHA256",
      Buffer.from(`${header}.${claims}`),
      publicKey,
      Buffer.from(signature, "base64url"),
    ),
  );
});

test("accessToken exchanges the assertion without exposing it", async () => {
  const { privateKey } = crypto.generateKeyPairSync("rsa", { modulusLength: 2048 });
  const calls = [];
  const fetchImpl = async (url, options) => {
    calls.push({ url, options });
    return { ok: true, status: 200, text: async () => '{"access_token":"secret-token"}' };
  };
  const token = await accessToken({
    credentials: {
      client_email: "ci@example.iam.gserviceaccount.com",
      private_key: privateKey.export({ type: "pkcs8", format: "pem" }),
    },
    fetchImpl,
    now: NOW,
  });
  assert.equal(token, "secret-token");
  assert.equal(calls.length, 1);
  const body = new URLSearchParams(calls[0].options.body);
  assert.equal(body.get("grant_type"), "urn:ietf:params:oauth:grant-type:jwt-bearer");
  assert.match(body.get("assertion"), /^[\w-]+\.[\w-]+\.[\w-]+$/);
});

test("the local fixture CLI is a dry run and never deletes", async () => {
  const lines = [];
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "fad-cleanup-"));
  const fixture = path.join(directory, "releases.json");
  fs.writeFileSync(
    fixture,
    JSON.stringify({
      releases: [release("new", NOW - HOUR_MS), release("old", NOW - 30 * HOUR_MS)],
    }),
  );
  const code = await main(
    ["--fixture", fixture, "--now", new Date(NOW).toISOString()],
    (line) => lines.push(line),
  );
  assert.equal(code, 0);
  assert.match(lines.join("\n"), /dry run: 1 release\(s\) would be deleted/);
  await assert.rejects(
    main(["--fixture", fixture, "--apply"], () => {}),
    /fixture CLI is a dry run only/,
  );
});

test("the checked-in fixture used in docs stays valid", () => {
  const fixture = JSON.parse(
    fs.readFileSync(path.join(HERE, "testdata", "fad-releases.json"), "utf8"),
  );
  assert.ok(Array.isArray(fixture.releases) && fixture.releases.length > 1);
});
