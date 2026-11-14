#!/usr/bin/env node
/**
 * Firebase App Distribution release retention.
 *
 * Every PR that changes the Android app uploads a build to App Distribution
 * (`android-distribute` in `ci.yml`), and nothing removes the old ones: after a
 * week of PRs the tester list is a long scroll of dead builds. This module
 * keeps a rolling 24-hour window per Firebase app:
 *
 *   1. The newest release is never deleted, whatever its age. That is the build
 *      the job that uploaded it (or the last successful upload) just published,
 *      and it is the one a tester installs.
 *   2. A release older than `MAX_AGE_MS` (24 hours) is deleted.
 *   3. A release exactly at the boundary or younger is kept, and so is a
 *      release whose `createTime` cannot be parsed (unknown age is not a
 *      reason to destroy an artifact).
 *
 * Releases are listed with `projects.apps.releases.list` (paginated,
 * `orderBy=createTime desc`) and removed with `releases:batchDelete`, both
 * authenticated by the same service account that uploads the APK: a JWT
 * assertion signed with its private key is exchanged for an OAuth token with
 * the `cloud-platform` scope. Nothing is printed from the credential file.
 *
 * The selection is a pure function (`planReleaseCleanup`) covered by
 * `fad_cleanup.test.mjs`; listing, OAuth and deletion take an injectable
 * `fetch`, so the tests never reach Google.
 *
 * Deleting needs the **Firebase App Distribution Admin** role on the service
 * account (`roles/firebaseappdistro.admin`); uploading only needs the upload
 * permission. A 403 is reported with that hint instead of a bare status.
 */

import crypto from "node:crypto";
import fs from "node:fs";
import process from "node:process";
import { pathToFileURL } from "node:url";

export const API_BASE = "https://firebaseappdistribution.googleapis.com/v1";
export const TOKEN_URL = "https://oauth2.googleapis.com/token";
export const CLOUD_PLATFORM_SCOPE = "https://www.googleapis.com/auth/cloud-platform";
export const MAX_AGE_MS = 24 * 60 * 60 * 1000;
export const HOUR_MS = 60 * 60 * 1000;
/** Page size accepted by `projects.apps.releases.list`. */
export const PAGE_SIZE = 100;
/** `releases:batchDelete` accepts a bounded name list per call. */
export const DELETE_CHUNK = 100;
export const DEFAULT_PACKAGE = "dev.proflyder.home";
/** Hint appended to a permission failure; the exact IAM role is the fix. */
export const DELETE_PERMISSION_HINT =
  "grant the service account the Firebase App Distribution Admin role " +
  "(roles/firebaseappdistro.admin) in the Firebase project";

/** Thrown for a non-2xx App Distribution response. */
export class ApiError extends Error {
  constructor(message, { status, body = "" } = {}) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.body = body;
  }
}

function releaseTime(release) {
  const value = Date.parse(String(release?.createTime || ""));
  return Number.isFinite(value) ? value : null;
}

function hoursLabel(maxAgeMs) {
  const hours = maxAgeMs / HOUR_MS;
  return Number.isInteger(hours) ? `${hours}h` : `${hours.toFixed(1)}h`;
}

/**
 * Decide which releases fall out of the retention window.
 *
 * @param {Array<{name?: string, displayName?: string, createTime?: string}>} releases
 * @param {{now?: number, maxAgeMs?: number}} options
 */
export function planReleaseCleanup(
  releases,
  { now = Date.now(), maxAgeMs = MAX_AGE_MS } = {},
) {
  const list = (Array.isArray(releases) ? releases : []).filter(Boolean);
  if (!list.length) {
    return { newest: null, decisions: [], delete: [], keep: [] };
  }

  // Every release sharing the maximum createTime is "the newest": an upload
  // race must not make the cleanup pick one of two equally fresh builds.
  let newestTime = null;
  for (const release of list) {
    const created = releaseTime(release);
    if (created !== null && (newestTime === null || created > newestTime)) {
      newestTime = created;
    }
  }

  const window = hoursLabel(maxAgeMs);
  const decisions = list.map((release) => {
    const created = releaseTime(release);
    const base = {
      name: typeof release.name === "string" ? release.name : "",
      displayName: release.displayName ?? null,
      createTime: release.createTime ?? null,
    };
    if (created === null) {
      return { ...base, action: "keep", ageMs: null, reason: "unknown createTime" };
    }
    const ageMs = now - created;
    if (created === newestTime) {
      return { ...base, action: "keep", ageMs, reason: "newest release" };
    }
    if (ageMs > maxAgeMs) {
      return { ...base, action: "delete", ageMs, reason: `older than ${window}` };
    }
    return { ...base, action: "keep", ageMs, reason: `within the ${window} window` };
  });

  // Report newest first; a release without a date goes last.
  decisions.sort((left, right) => {
    const leftTime = Date.parse(String(left.createTime || ""));
    const rightTime = Date.parse(String(right.createTime || ""));
    const leftKey = Number.isFinite(leftTime) ? leftTime : -Infinity;
    const rightKey = Number.isFinite(rightTime) ? rightTime : -Infinity;
    return rightKey - leftKey;
  });

  const newest = decisions.find((item) => item.action === "keep" && item.reason === "newest release");
  return {
    newest: newest?.name ?? null,
    decisions,
    delete: decisions.filter((item) => item.action === "delete"),
    keep: decisions.filter((item) => item.action === "keep"),
  };
}

/** `1:1234567890:android:abcdef` → `1234567890` (the project number). */
export function projectFromAppId(appId) {
  const match = /^1:(\d+):android:/.exec(String(appId || ""));
  if (!match) {
    throw new Error("Firebase app id must look like 1:<project number>:android:<id>");
  }
  return match[1];
}

/** The `mobilesdk_app_id` of `packageName` in a google-services.json. */
export function firebaseAppId(googleServices, packageName = DEFAULT_PACKAGE) {
  const clients = Array.isArray(googleServices?.client) ? googleServices.client : [];
  for (const client of clients) {
    const info = client?.client_info;
    if (info?.android_client_info?.package_name !== packageName) {
      continue;
    }
    const appId = info?.mobilesdk_app_id;
    if (appId) {
      return appId;
    }
  }
  throw new Error(`google-services.json has no client for ${packageName}`);
}

function base64url(value) {
  return Buffer.from(value).toString("base64url");
}

/**
 * A signed JWT assertion for Google's OAuth token endpoint (the
 * service-account flow; no external dependency). `private_key` is a PEM
 * string from the service-account JSON.
 */
export function buildAssertion(
  credentials,
  { now = Date.now(), lifetimeSeconds = 3600, scope = CLOUD_PLATFORM_SCOPE } = {},
) {
  const email = credentials?.client_email;
  const key = String(credentials?.private_key || "").replace(/\\n/g, "\n");
  if (!email || !key) {
    throw new Error("service account JSON needs client_email and private_key");
  }
  const issuedAt = Math.floor(now / 1000);
  const header = { alg: "RS256", typ: "JWT" };
  const claims = {
    iss: email,
    scope,
    aud: credentials.token_uri || TOKEN_URL,
    iat: issuedAt,
    exp: issuedAt + lifetimeSeconds,
  };
  const signingInput = `${base64url(JSON.stringify(header))}.${base64url(JSON.stringify(claims))}`;
  const signature = crypto.createSign("RSA-SHA256").update(signingInput).sign(key);
  return `${signingInput}.${signature.toString("base64url")}`;
}

/** Exchange a service-account assertion for an OAuth access token. */
export async function accessToken({ credentials, fetchImpl = fetch, now = Date.now() } = {}) {
  const tokenUrl = credentials?.token_uri || TOKEN_URL;
  const body = new URLSearchParams({
    grant_type: "urn:ietf:params:oauth:grant-type:jwt-bearer",
    assertion: buildAssertion(credentials, { now }),
  });
  const response = await fetchImpl(tokenUrl, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: body.toString(),
  });
  const text = await response.text();
  if (!response.ok) {
    throw new ApiError(`OAuth token request failed with ${response.status}`, {
      status: response.status,
      body: text.slice(0, 500),
    });
  }
  let payload;
  try {
    payload = JSON.parse(text);
  } catch {
    throw new ApiError("OAuth token response is not JSON", { status: response.status });
  }
  if (!payload?.access_token) {
    throw new ApiError("OAuth token response has no access_token", { status: response.status });
  }
  return payload.access_token;
}

async function requestJson({ fetchImpl, url, accessToken: token, method = "GET", body }) {
  const headers = { authorization: `Bearer ${token}` };
  if (body !== undefined) {
    headers["content-type"] = "application/json";
  }
  const response = await fetchImpl(url, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  if (!response.ok) {
    throw new ApiError(
      `${method} ${url.split("?")[0]} failed with ${response.status}` +
        (response.status === 403 ? `; ${DELETE_PERMISSION_HINT}` : ""),
      { status: response.status, body: text.slice(0, 500) },
    );
  }
  if (!text) {
    return {};
  }
  try {
    return JSON.parse(text);
  } catch {
    throw new ApiError(`${method} ${url.split("?")[0]} returned invalid JSON`, {
      status: response.status,
    });
  }
}

/**
 * Every release of one app, newest first, following `nextPageToken` until the
 * API stops returning one.
 */
export async function listReleases({
  accessToken: token,
  appName,
  fetchImpl = fetch,
  pageSize = PAGE_SIZE,
  baseUrl = API_BASE,
}) {
  const releases = [];
  let pageToken = "";
  do {
    const url = new URL(`${baseUrl}/${appName}/releases`);
    url.searchParams.set("pageSize", String(pageSize));
    url.searchParams.set("orderBy", "createTime desc");
    if (pageToken) {
      url.searchParams.set("pageToken", pageToken);
    }
    const page = await requestJson({ fetchImpl, url: url.toString(), accessToken: token });
    for (const release of page.releases || []) {
      if (release) {
        releases.push(release);
      }
    }
    pageToken = page.nextPageToken || "";
  } while (pageToken);
  return releases;
}

/**
 * Delete release names in batches. A failed batch is reported, not thrown, so
 * one refused batch does not abandon the rest of the cleanup.
 */
export async function deleteReleases({
  accessToken: token,
  appName,
  names,
  fetchImpl = fetch,
  baseUrl = API_BASE,
  chunkSize = DELETE_CHUNK,
}) {
  const pending = Array.isArray(names) ? names.filter(Boolean) : [];
  const deleted = [];
  const failures = [];
  for (let start = 0; start < pending.length; start += chunkSize) {
    const chunk = pending.slice(start, start + chunkSize);
    try {
      await requestJson({
        fetchImpl,
        url: `${baseUrl}/${appName}/releases:batchDelete`,
        accessToken: token,
        method: "POST",
        body: { names: chunk },
      });
      deleted.push(...chunk);
    } catch (error) {
      failures.push({
        names: chunk,
        status: error?.status ?? null,
        message: error?.message || String(error),
      });
    }
  }
  return { deleted, failures };
}

function describe(item) {
  const label = item.displayName ? `${item.name} (${item.displayName})` : item.name;
  return label;
}

/**
 * One cleanup run: plan, report, and (unless `dryRun`) delete. Returns the plan
 * and the deletion result so a caller can set an exit code.
 */
export async function runCleanup({
  credentials,
  appId,
  now = Date.now(),
  maxAgeMs = MAX_AGE_MS,
  dryRun = true,
  fetchImpl = fetch,
  baseUrl = API_BASE,
  log = console.log,
  token = null,
}) {
  const project = projectFromAppId(appId);
  const appName = `projects/${project}/apps/${appId}`;
  const accessTokenValue = token || (await accessToken({ credentials, fetchImpl, now }));
  log(`Firebase app ${appId} (project ${project}), ${dryRun ? "dry run" : "delete"}`);

  const releases = await listReleases({
    accessToken: accessTokenValue,
    appName,
    fetchImpl,
    baseUrl,
  });
  const plan = planReleaseCleanup(releases, { now, maxAgeMs });
  log(`${releases.length} release(s), ${plan.delete.length} older than ${hoursLabel(maxAgeMs)}`);
  for (const item of plan.decisions) {
    log(`  ${describe(item)} — ${item.action}: ${item.reason}`);
  }
  if (dryRun) {
    log(`dry run: ${plan.delete.length} release(s) would be deleted`);
    return { plan, releases, deleted: [], failures: [] };
  }
  const result = await deleteReleases({
    accessToken: accessTokenValue,
    appName,
    names: plan.delete.map((item) => item.name),
    fetchImpl,
    baseUrl,
  });
  for (const name of result.deleted) {
    log(`deleted ${name}`);
  }
  for (const failure of result.failures) {
    log(`warning: ${failure.names.length} release(s) not deleted: ${failure.message}`);
  }
  return { plan, releases, ...result };
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function parseArgs(argv) {
  const args = {
    credentials: "",
    googleServices: "",
    appId: "",
    package: DEFAULT_PACKAGE,
    fixture: "",
    now: Date.now(),
    maxAgeHours: MAX_AGE_MS / HOUR_MS,
    dryRun: true,
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    const value = () => {
      index += 1;
      if (index >= argv.length) {
        throw new Error(`${flag} needs a value`);
      }
      return argv[index];
    };
    switch (flag) {
      case "--credentials":
        args.credentials = value();
        break;
      case "--google-services":
        args.googleServices = value();
        break;
      case "--app-id":
        args.appId = value();
        break;
      case "--package":
        args.package = value();
        break;
      case "--fixture":
        args.fixture = value();
        break;
      case "--now":
        args.now = Date.parse(value());
        break;
      case "--max-age-hours":
        args.maxAgeHours = Number(value());
        break;
      case "--dry-run":
        args.dryRun = true;
        break;
      case "--apply":
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
 * CLI used by `.github/workflows/cleanup.yml`:
 *
 *   node platform/ci/fad_cleanup.mjs \
 *     --credentials "$FIREBASE_SERVICE_ACCOUNT" \
 *     --google-services "$GOOGLE_SERVICES_JSON" [--apply]
 *
 * `--fixture <json>` ({releases: [...]}) replays the selection locally without
 * credentials or network access; the local CLI never deletes.
 */
export async function main(argv = process.argv.slice(2), log = console.log) {
  const args = parseArgs(argv);
  if (args.help) {
    log(
      "usage: fad_cleanup.mjs --google-services <json> --credentials <json> " +
        "[--package <id>] [--dry-run|--apply] [--max-age-hours N] [--now <iso>]\n" +
        "       fad_cleanup.mjs --fixture <json> [--now <iso>] [--max-age-hours N]",
    );
    return 0;
  }
  if (!Number.isFinite(args.now)) {
    throw new Error("--now must be an ISO date");
  }
  if (!Number.isFinite(args.maxAgeHours) || args.maxAgeHours <= 0) {
    throw new Error("--max-age-hours must be a positive number");
  }
  const maxAgeMs = args.maxAgeHours * HOUR_MS;

  if (args.fixture) {
    if (!args.dryRun) {
      throw new Error("the local fixture CLI is a dry run only");
    }
    const fixture = readJson(args.fixture);
    const plan = planReleaseCleanup(fixture.releases, { now: args.now, maxAgeMs });
    for (const item of plan.decisions) {
      log(`  ${describe(item)} — ${item.action}: ${item.reason}`);
    }
    log(`dry run: ${plan.delete.length} release(s) would be deleted`);
    return 0;
  }

  if (!args.credentials) {
    throw new Error("--credentials is required (or use --fixture)");
  }
  if (!args.appId) {
    if (!args.googleServices) {
      throw new Error("--google-services or --app-id is required");
    }
    args.appId = firebaseAppId(readJson(args.googleServices), args.package);
  }
  const credentials = readJson(args.credentials);
  const result = await runCleanup({
    credentials,
    appId: args.appId,
    now: args.now,
    maxAgeMs,
    dryRun: args.dryRun,
    log,
  });
  if (result.failures.length) {
    throw new Error(
      `${result.failures.length} batch(es) could not be deleted; ${DELETE_PERMISSION_HINT}. ` +
        `Example: ${result.failures[0].message}`,
    );
  }
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    process.exitCode = await main();
  } catch (error) {
    console.error(error?.message || String(error));
    process.exitCode = 1;
  }
}
