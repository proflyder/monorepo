#!/usr/bin/env node
/**
 * Merge a pull request only when its CI result matches the tree that lands.
 *
 * A squash merge keeps the PR's tree only while `develop` does not move. If
 * another PR landed after the pull-request CI run, the merge commit has a tree
 * CI never tested and `deploy-home` fails closed for the affected units (PR
 * #218 after #217). The ruleset is not strict, so nothing on GitHub prevents
 * that race; this script closes it locally:
 *
 *   1. read the PR (`state`, `baseRefName`, `headRefOid`, `mergeable`);
 *   2. compare the current `origin/develop` with the PR head
 *      (`compare/develop...<head>`): `behind_by > 0` means CI ran on an older
 *      base. Refreshing the branch (`gh pr update-branch`) costs a full CI run,
 *      so the refresh is skipped when it is provably safe to land the PR on
 *      the newer base: `ci-ok` is green on the current head, GitHub reports the
 *      PR as `MERGEABLE`, the files changed by the PR and by the new `develop`
 *      commits are disjoint, and none of those `develop` commits touches a
 *      CI-global path (workflows, CI scripts, Bazel/toolchain config, lock
 *      files). Otherwise merge `develop` into the branch and wait for the new
 *      head. A conflict is a hard error — nothing is merged;
 *   3. wait for `ci-ok` on the current head (check-runs API, configurable
 *      timeout, 90 min by default). A non-success conclusion stops before merge;
 *   4. re-check `behind_by` right before merging; if `develop` moved again,
 *      apply the same disjointness/CI-global test, otherwise restart from
 *      step 2 (bounded number of attempts);
 *   5. merge with `--match-head-commit`, so a head that moved between the check
 *      and the merge is refused by GitHub instead of merged;
 *   6. print the merge sha and the `deploy-home` run URL for it.
 *
 * `--dry-run` performs every read and reports what it would do without
 * updating or merging. `--no-update` is a pure check: it refuses a PR whose
 * head lacks `develop` and never merges. `--always-refresh` restores the old
 * behaviour of always updating the branch when `develop` moved.
 *
 * All `gh` calls go through the injected `exec` command runner, so the decision
 * logic and the full flow are tested without a network (safe-merge.test.mjs).
 * Authentication is whatever `gh` already uses; `GH_TOKEN` is honoured by `gh`
 * and never printed.
 */

import { spawnSync } from "node:child_process";
import process from "node:process";
import { pathToFileURL } from "node:url";

/** The only base branch that may be merged into. */
export const BASE_BRANCH = "develop";
/** Aggregate required check published by `.github/workflows/ci.yml`. */
export const REQUIRED_CHECK = "ci-ok";
export const DEFAULT_TIMEOUT_MIN = 90;
// Two independent budgets. `--max-updates` bounds how often the branch is
// refreshed; the move budget only grows when `develop` really got a new commit
// (a stale compare right after `update-branch` must not consume it).
export const DEFAULT_MAX_UPDATES = 3;
export const DEFAULT_MAX_MOVES = 3;
export const DEFAULT_POLL_MS = 30_000;
export const DEFAULT_UPDATE_WAIT_MS = 120_000;
// GitHub's compare endpoint can lag a few seconds behind a branch update.
export const DEFAULT_COMPARE_WAIT_MS = 60_000;
export const DEFAULT_DEPLOY_WAIT_MS = 60_000;
export const DEFAULT_DEPLOY_POLL_MS = 5_000;
export const DEPLOY_WORKFLOW = "deploy-home.yml";
// GitHub's compare API caps the `files` array; a capped list cannot prove that
// the two changesets are disjoint, so it disables the refresh-free merge.
export const MAX_COMPARE_FILES = 300;

/**
 * Paths whose change on `develop` makes a cached `ci-ok` unreliable for a
 * squash merge: workflows, CI tooling, Bazel/toolchain configuration, the
 * mise/tool versions and dependency lock files. Glob syntax supports `*`
 * (within one segment), `**` (across segments) and `?`.
 */
export const CI_GLOBAL_PATHS = [
  ".github/**",
  "platform/ci/**",
  ".bazelrc",
  ".bazelversion",
  "MODULE.bazel*",
  "platform/android/android.bazelrc",
  "mise.toml",
  "Cargo.lock",
  "**/maven_install.json",
  "package-lock.json",
  "pnpm-lock.yaml",
  "platform/bazel/**",
];

const CI_GLOBAL_MATCHERS = CI_GLOBAL_PATHS.map((glob) => globToRegExp(glob));

const COMMIT = /^[0-9a-f]{40}$/;

function globToRegExp(glob) {
  let pattern = "^";
  for (let index = 0; index < glob.length; index += 1) {
    const char = glob[index];
    if (char === "*") {
      if (glob[index + 1] === "*") {
        index += 1;
        if (glob[index + 1] === "/") {
          index += 1;
          pattern += "(?:.*/)?";
        } else {
          pattern += ".*";
        }
      } else {
        pattern += "[^/]*";
      }
    } else if (char === "?") {
      pattern += "[^/]";
    } else {
      pattern += char.replace(/[.+^${}()|[\]\\]/g, "\\$&");
    }
  }
  return new RegExp(`${pattern}$`);
}

/** True when `path` is one of the CI-global paths above. */
export function isCiGlobalPath(path) {
  return CI_GLOBAL_MATCHERS.some((matcher) => matcher.test(path));
}

/**
 * Collect the paths a compare response says are changed, including the old
 * side of a rename. `known` is false when the API returned no `files` array:
 * an absent list proves nothing and must not be read as "no changed files".
 */
export function changedFiles(compare) {
  const files = Array.isArray(compare?.files) ? compare.files : null;
  if (files === null) {
    return { paths: [], known: false, truncated: false };
  }
  const paths = new Set();
  for (const file of files) {
    if (file?.filename) {
      paths.add(file.filename);
    }
    if (file?.previous_filename) {
      paths.add(file.previous_filename);
    }
  }
  return {
    paths: [...paths].sort(),
    known: true,
    truncated: files.length >= MAX_COMPARE_FILES,
  };
}

/**
 * Decide whether `develop` may stay ahead of the PR head at merge time.
 *
 * The squash merge then produces a tree that was never tested as a whole, but
 * which differs from the tested PR tree only by `develop` commits that touch
 * other files and no CI-global path — so the green `ci-ok` still describes the
 * relevant code. `structural` reports the file/mergeability checks alone, so
 * the caller can wait for a pending `ci-ok` before deciding; `allowed` is the
 * final answer and also requires a green check.
 */
export function decideFastPath({
  behindBy,
  mergeable,
  ciGreen = false,
  prFiles = [],
  developFiles = [],
  filesKnown = true,
  filesTruncated = false,
  alwaysRefresh = false,
}) {
  if (alwaysRefresh) {
    return { allowed: false, structural: false, reason: "--always-refresh always refreshes the branch" };
  }
  if (!(behindBy > 0)) {
    return { allowed: false, structural: false, reason: `head already contains ${BASE_BRANCH}` };
  }
  if (!filesKnown) {
    return { allowed: false, structural: false, reason: "compare did not return a file list" };
  }
  if (filesTruncated) {
    return {
      allowed: false,
      structural: false,
      reason: `changed-file list is capped at ${MAX_COMPARE_FILES} entries`,
    };
  }
  if (String(mergeable || "").toUpperCase() !== "MERGEABLE") {
    return {
      allowed: false,
      structural: false,
      reason: `GitHub reports the PR as ${mergeable || "UNKNOWN"}, not MERGEABLE`,
    };
  }
  const developSet = new Set(developFiles);
  const overlap = [...new Set(prFiles)].filter((file) => developSet.has(file)).sort();
  if (overlap.length > 0) {
    return {
      allowed: false,
      structural: false,
      reason: `PR and ${BASE_BRANCH} touch the same files: ${overlap.join(", ")}`,
    };
  }
  const globals = [...developSet].filter(isCiGlobalPath).sort();
  if (globals.length > 0) {
    return {
      allowed: false,
      structural: false,
      reason: `${BASE_BRANCH} touches CI-global paths: ${globals.join(", ")}`,
    };
  }
  const reason = `${BASE_BRANCH} +${behindBy} commit(s), disjoint from PR files and no CI-global paths`;
  if (!ciGreen) {
    return {
      allowed: false,
      structural: true,
      reason: `${reason}, but ${REQUIRED_CHECK} is not green on the current head`,
    };
  }
  return { allowed: true, structural: true, reason };
}

/** Parse CLI arguments: `<N> [--dry-run] [--timeout <min>] [--no-update] [--always-refresh]`. */
export function parseArgs(argv) {
  const args = {
    pr: null,
    dryRun: false,
    noUpdate: false,
    alwaysRefresh: false,
    timeoutMin: DEFAULT_TIMEOUT_MIN,
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    const value = argv[index + 1];
    switch (flag) {
      case "--dry-run":
        args.dryRun = true;
        break;
      case "--no-update":
        args.noUpdate = true;
        break;
      case "--always-refresh":
        args.alwaysRefresh = true;
        break;
      case "--timeout":
        if (value === undefined || !/^\d+$/.test(value) || Number(value) <= 0) {
          throw new Error("--timeout needs a positive number of minutes");
        }
        args.timeoutMin = Number(value);
        index += 1;
        break;
      case "--help":
      case "-h":
        args.help = true;
        break;
      default:
        if (flag.startsWith("-")) {
          throw new Error(`unrecognized argument: ${flag}`);
        }
        if (args.pr !== null) {
          throw new Error(`unexpected extra argument: ${flag}`);
        }
        if (!/^\d+$/.test(flag) || Number(flag) <= 0) {
          throw new Error(`pull request number must be a positive integer, got '${flag}'`);
        }
        args.pr = Number(flag);
    }
  }
  return args;
}

/**
 * Validate the PR as read from `gh pr view`.
 *
 * Only an OPEN PR into `develop` may be merged. A PR GitHub already reports as
 * `CONFLICTING` cannot be refreshed from `develop`, so it fails early with a
 * message that points at the local resolution. `mergeable: UNKNOWN` is allowed:
 * GitHub computes it lazily and the compare call decides.
 */
export function checkPull(pull) {
  if (!pull || typeof pull !== "object") {
    throw new Error("cannot read the pull request");
  }
  const state = String(pull.state || "").toUpperCase();
  if (state !== "OPEN") {
    throw new Error(`PR #${pull.number ?? "?"} is ${state || "unknown"}, not OPEN`);
  }
  if (pull.baseRefName !== BASE_BRANCH) {
    throw new Error(
      `PR #${pull.number ?? "?"} targets '${pull.baseRefName ?? "?"}', not '${BASE_BRANCH}'`,
    );
  }
  const head = String(pull.headRefOid || "");
  if (!COMMIT.test(head)) {
    throw new Error(`PR #${pull.number ?? "?"} has no full head commit`);
  }
  if (String(pull.mergeable || "").toUpperCase() === "CONFLICTING") {
    throw new Error(
      `PR #${pull.number} conflicts with ${BASE_BRANCH}; resolve the conflict locally and push`,
    );
  }
  return {
    number: pull.number,
    head,
    base: pull.baseRefName,
    mergeable: String(pull.mergeable || "").toUpperCase(),
    url: pull.url || "",
    title: pull.title || "",
  };
}

/**
 * Decide what to do with the PR head given `behind_by` from the compare API.
 *
 * `behindBy` is the number of commits on `develop` that the head does not
 * contain. Non-zero means the merge tree is unverified; `--no-update` turns
 * that into a refusal instead of an update.
 */
export function decideBehind({ behindBy, noUpdate = false }) {
  if (!Number.isInteger(behindBy) || behindBy < 0) {
    throw new Error(`compare did not return a valid behind_by (${JSON.stringify(behindBy)})`);
  }
  if (behindBy === 0) {
    return { action: "up-to-date", behindBy };
  }
  if (noUpdate) {
    return {
      action: "refuse",
      behindBy,
      message:
        `PR head is ${behindBy} commit(s) behind ${BASE_BRANCH}; ` +
        `CI ran on an older base. Update the branch (run without --no-update) ` +
        `and wait for a new ${REQUIRED_CHECK}.`,
    };
  }
  return { action: "update", behindBy };
}

/**
 * Reduce the check-runs of one commit to the state of `ci-ok`.
 *
 * A re-run leaves several check runs with the same name on the same commit;
 * the newest id is the effective one. `pass` needs a completed `success`;
 * any other completed conclusion is a failure, and an absent or in-progress
 * check keeps the caller waiting.
 */
export function decideChecks(checkRuns, name = REQUIRED_CHECK) {
  const runs = (Array.isArray(checkRuns) ? checkRuns : []).filter(
    (run) => run && run.name === name,
  );
  if (runs.length === 0) {
    return { state: "pending", reason: `${name} has not been reported yet` };
  }
  const latest = runs.reduce((best, run) => {
    const bestId = Number(best?.id ?? 0);
    const runId = Number(run?.id ?? 0);
    return runId >= bestId ? run : best;
  });
  const status = String(latest.status || "").toLowerCase();
  const conclusion = String(latest.conclusion || "").toLowerCase();
  if (status !== "completed") {
    return { state: "pending", reason: `${name} is ${status || "pending"}` };
  }
  if (conclusion !== "success") {
    return {
      state: "fail",
      reason: `${name} concluded '${conclusion || "unknown"}'`,
      url: latest.html_url || latest.details_url || "",
    };
  }
  return { state: "pass", reason: `${name} is green`, url: latest.html_url || "" };
}

/**
 * Turn a check-runs API response into the bare array the decision works on.
 */
export function checkRunsOf(response) {
  if (Array.isArray(response)) {
    return response;
  }
  if (response && Array.isArray(response.check_runs)) {
    return response.check_runs;
  }
  throw new Error("check-runs response has no check_runs array");
}

function defaultSleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function ghJson(exec, args, what) {
  const output = exec("gh", args);
  try {
    return JSON.parse(String(output));
  } catch {
    throw new Error(`cannot parse ${what} from gh`);
  }
}

function prView(exec, pr) {
  return ghJson(
    exec,
    [
      "pr",
      "view",
      String(pr),
      "--json",
      "number,state,baseRefName,headRefOid,mergeable,mergeCommit,url,title",
    ],
    `pull request #${pr}`,
  );
}

function behindBy(exec, repository, head) {
  return compareWithDevelop(exec, repository, head)?.behind_by;
}

/** Full compare response for `develop...<head>`, including the merge base. */
function compareWithDevelop(exec, repository, head) {
  return ghJson(
    exec,
    ["api", `repos/${repository}/compare/${BASE_BRANCH}...${head}`],
    `compare ${BASE_BRANCH}...${head}`,
  );
}

/** Files changed between two refs, as reported by the compare API. */
function compareFiles(exec, repository, base, head) {
  return changedFiles(
    ghJson(
      exec,
      ["api", `repos/${repository}/compare/${base}...${head}`],
      `compare ${base}...${head}`,
    ),
  );
}

function readChecks(exec, repository, head) {
  return decideChecks(
    checkRunsOf(
      ghJson(
        exec,
        ["api", `repos/${repository}/commits/${head}/check-runs?per_page=100`],
        `check runs of ${head}`,
      ),
    ),
  );
}

/** Current commit of `develop`, used to tell a real move from a stale compare. */
function readDevelopSha(exec, repository) {
  const ref = ghJson(
    exec,
    ["api", `repos/${repository}/git/ref/heads/${BASE_BRANCH}`],
    `ref ${BASE_BRANCH}`,
  );
  const sha = ref?.object?.sha;
  if (!COMMIT.test(String(sha || ""))) {
    throw new Error(`cannot read the ${BASE_BRANCH} commit from the API`);
  }
  return String(sha);
}

/**
 * Wait for a compare that lagged a branch update to catch up.
 *
 * `update-branch` changes the head immediately, but `compare` can keep
 * reporting the old `behind_by` for a few seconds. Poll until the head contains
 * `develop` (`behind_by === 0`) or `develop` really moves away from
 * `baselineSha`; never treat the stale value as a reason to update again.
 * Returns the last observation, with `timedOut` when neither happened.
 */
export async function waitForCompareSettled({
  exec,
  repository,
  head,
  baselineSha,
  timeoutMs,
  pollMs,
  now,
  sleep,
  log = () => {},
}) {
  const deadline = now() + timeoutMs;
  for (;;) {
    const behind = behindBy(exec, repository, head);
    const developSha = readDevelopSha(exec, repository);
    if (behind === 0) {
      return { behindBy: 0, developSha, timedOut: false };
    }
    if (developSha !== baselineSha) {
      return { behindBy: behind, developSha, timedOut: false };
    }
    if (now() >= deadline) {
      return { behindBy: behind, developSha: baselineSha, timedOut: true };
    }
    log(
      `compare for ${head.slice(0, 12)} is ${behind} commit(s) behind ${BASE_BRANCH}; ` +
        `waiting for it to catch up`,
    );
    await sleep(pollMs);
  }
}

/**
 * Poll `ci-ok` on one commit until it passes, fails or the timeout expires.
 */
async function waitForCi({ exec, repository, head, timeoutMin, pollMs, now, sleep, log }) {
  const deadline = now() + timeoutMin * 60_000;
  for (;;) {
    const response = ghJson(
      exec,
      ["api", `repos/${repository}/commits/${head}/check-runs?per_page=100`],
      `check runs of ${head}`,
    );
    const decision = decideChecks(checkRunsOf(response));
    if (decision.state === "pass") {
      log(`${decision.reason} on ${head.slice(0, 12)}`);
      return decision;
    }
    if (decision.state === "fail") {
      throw new Error(`${decision.reason} on ${head.slice(0, 12)}${decision.url ? ` — ${decision.url}` : ""}`);
    }
    if (now() >= deadline) {
      throw new Error(
        `timed out after ${timeoutMin} min waiting for ${REQUIRED_CHECK} on ${head.slice(0, 12)}`,
      );
    }
    log(`waiting for ${REQUIRED_CHECK} on ${head.slice(0, 12)}: ${decision.reason}`);
    await sleep(pollMs);
  }
}

async function waitForNewHead({ exec, pr, oldHead, waitMs, pollMs, now, sleep, log }) {
  const deadline = now() + waitMs;
  for (;;) {
    const view = prView(exec, pr);
    if (view.headRefOid !== oldHead) {
      log(`branch updated: ${oldHead.slice(0, 12)} -> ${String(view.headRefOid).slice(0, 12)}`);
      return view.headRefOid;
    }
    if (now() >= deadline) {
      throw new Error(`timed out waiting for a new head of PR #${pr} after updating the branch`);
    }
    await sleep(pollMs);
  }
}

async function waitForMergeSha({ exec, pr, waitMs, pollMs, now, sleep }) {
  const deadline = now() + waitMs;
  for (;;) {
    const view = prView(exec, pr);
    const sha = view.mergeCommit?.oid;
    if (String(view.state || "").toUpperCase() === "MERGED" && COMMIT.test(String(sha || ""))) {
      return sha;
    }
    if (now() >= deadline) {
      throw new Error(`PR #${pr} is not reported as MERGED with a merge commit yet`);
    }
    await sleep(pollMs);
  }
}

/** Find the `deploy-home` run for the merge commit, or return null. */
export async function findDeployRun({
  exec,
  repository,
  sha,
  waitMs,
  pollMs,
  now,
  sleep,
  log = () => {},
}) {
  const deadline = now() + waitMs;
  for (;;) {
    let runs = [];
    try {
      runs = ghJson(
        exec,
        ["run", "list", "--workflow", DEPLOY_WORKFLOW, "--commit", sha, "--limit", "1", "--json", "databaseId,status,url"],
        `deploy-home runs for ${sha}`,
      );
    } catch (error) {
      // A missing run is not fatal; the hint below is still useful.
      log(`cannot list deploy-home runs yet: ${error.message}`);
    }
    if (Array.isArray(runs) && runs.length > 0 && runs[0].url) {
      return runs[0];
    }
    if (now() >= deadline) {
      return null;
    }
    await sleep(pollMs);
  }
}

/**
 * Full flow: validate, refresh from `develop` when needed, wait for `ci-ok`,
 * re-check the base, merge by exact head, then report the deploy run.
 */
export async function runSafeMerge(options) {
  const {
    pr,
    repository,
    exec,
    dryRun = false,
    noUpdate = false,
    alwaysRefresh = false,
    timeoutMin = DEFAULT_TIMEOUT_MIN,
    maxUpdates = DEFAULT_MAX_UPDATES,
    maxMoves = DEFAULT_MAX_MOVES,
    pollMs = DEFAULT_POLL_MS,
    updateWaitMs = DEFAULT_UPDATE_WAIT_MS,
    compareWaitMs = DEFAULT_COMPARE_WAIT_MS,
    deployWaitMs = DEFAULT_DEPLOY_WAIT_MS,
    deployPollMs = DEFAULT_DEPLOY_POLL_MS,
    now = Date.now,
    sleep = defaultSleep,
    log = console.log,
  } = options;
  if (!Number.isInteger(pr) || pr <= 0) {
    throw new Error("a pull request number is required");
  }
  if (typeof exec !== "function") {
    throw new Error("exec is required");
  }

  const pull = checkPull(prView(exec, pr));
  log(`PR #${pr} (${pull.url || "no url"}): ${pull.title}`);

  let developSha = readDevelopSha(exec, repository);
  let updates = 0;
  let moves = 0;
  // True right after our own update-branch: until compare catches up, a
  // non-zero behind_by is not a reason to update the branch again.
  let refreshed = false;

  const staleCompareError = (behind) =>
    new Error(
      `compare for ${pull.head.slice(0, 12)} still reports ` +
        `${behind} commit(s) behind ${BASE_BRANCH} ` +
        `after ${Math.round(compareWaitMs / 1000)}s while ${BASE_BRANCH} did not move; ` +
        `nothing was merged — retry later`,
    );

  const logPath = (message) => log(dryRun ? `[dry-run] ${message}` : message);

  /**
   * Check the refresh-free merge conditions against the current API state:
   * mergeability, the files of the PR and of the new `develop` commits, and
   * `ci-ok` on the current head. `structural` ignores the check state, so a
   * pending run can be awaited instead of being thrown away.
   */
  async function evaluateFastPath(behind, headCompare) {
    if (alwaysRefresh) {
      return {
        allowed: false,
        structural: false,
        ciState: "unknown",
        reason: "--always-refresh always refreshes the branch",
      };
    }
    const view = prView(exec, pr);
    const headNow = String(view.headRefOid || "");
    if (headNow !== pull.head) {
      return {
        allowed: false,
        structural: false,
        ciState: "unknown",
        reason: `PR head moved to ${headNow.slice(0, 12)}`,
      };
    }
    const mergeBase = String(headCompare?.merge_base_commit?.sha || "");
    if (!COMMIT.test(mergeBase)) {
      return {
        allowed: false,
        structural: false,
        ciState: "unknown",
        reason: "compare did not report a merge base",
      };
    }
    const prChanged = changedFiles(headCompare);
    const developChanged = compareFiles(exec, repository, mergeBase, BASE_BRANCH);
    const checks = readChecks(exec, repository, pull.head);
    return {
      ...decideFastPath({
        behindBy: behind,
        mergeable: view.mergeable,
        ciGreen: checks.state === "pass",
        prFiles: prChanged.paths,
        developFiles: developChanged.paths,
        filesKnown: prChanged.known && developChanged.known,
        filesTruncated: prChanged.truncated || developChanged.truncated,
        alwaysRefresh,
      }),
      ciState: checks.state,
    };
  }

  for (;;) {
    let compared = compareWithDevelop(exec, repository, pull.head);
    let behind = compared?.behind_by;

    if (behind > 0) {
      const decision = decideBehind({ behindBy: behind, noUpdate });
      if (decision.action === "refuse") {
        throw new Error(decision.message);
      }

      if (refreshed) {
        // The head changed, but GitHub's compare can lag. Wait for it to catch
        // up before deciding whether another refresh is actually needed.
        const settled = await waitForCompareSettled({
          exec,
          repository,
          head: pull.head,
          baselineSha: developSha,
          timeoutMs: compareWaitMs,
          pollMs,
          now,
          sleep,
          log,
        });
        if (settled.developSha !== developSha) {
          developSha = settled.developSha;
          moves += 1;
          refreshed = false;
          if (moves > maxMoves) {
            throw new Error(
              `${BASE_BRANCH} moved ${moves} time(s) while refreshing the branch; ` +
                `nothing was merged — retry later`,
            );
          }
          log(`${BASE_BRANCH} moved to ${developSha.slice(0, 12)}; re-evaluating`);
          continue;
        }
        if (settled.behindBy !== 0) {
          throw staleCompareError(settled.behindBy);
        }
        behind = 0;
        refreshed = false;
      }
    }

    let merging = false;

    if (behind > 0) {
      if (!alwaysRefresh) {
        let candidate = await evaluateFastPath(behind, compared);
        if (candidate.structural && candidate.ciState === "pending" && !dryRun) {
          // The current CI run already covers the PR tree; wait for it instead
          // of discarding it with an update-branch.
          log(
            `${candidate.reason}; waiting for ${REQUIRED_CHECK} on the current head ` +
              `instead of refreshing the branch`,
          );
          await waitForCi({
            exec,
            repository,
            head: pull.head,
            timeoutMin,
            pollMs,
            now,
            sleep,
            log,
          });
          compared = compareWithDevelop(exec, repository, pull.head);
          behind = compared?.behind_by;
          candidate =
            behind > 0
              ? await evaluateFastPath(behind, compared)
              : {
                  allowed: false,
                  structural: false,
                  ciState: "pass",
                  reason: `head now contains ${BASE_BRANCH}`,
                };
        }

        if (dryRun && candidate.structural) {
          logPath(
            `${candidate.reason}; would merge at ${pull.head.slice(0, 12)} without refresh ` +
              `once ${REQUIRED_CHECK} is green`,
          );
          return { status: "dry-run", wouldMergeWithoutRefresh: true, head: pull.head };
        }
        if (candidate.allowed) {
          logPath(
            `${candidate.reason} → merging on green ${REQUIRED_CHECK} at ` +
              `${pull.head.slice(0, 12)} without refresh`,
          );
          merging = true;
        } else if (behind > 0) {
          log(`${candidate.reason}; refreshing the branch`);
        }
      }

      if (!merging && behind > 0) {
        if (dryRun) {
          log(
            `[dry-run] PR head ${pull.head.slice(0, 12)} is ${behind} commit(s) ` +
              `behind ${BASE_BRANCH}; would run: gh pr update-branch ${pr}`,
          );
          return { status: "dry-run", wouldUpdate: true, head: pull.head };
        }
        if (updates >= maxUpdates) {
          throw new Error(
            `refreshed the branch ${updates} time(s) without a stable base; ` +
              `nothing was merged — retry later`,
          );
        }
        log(
          `PR head ${pull.head.slice(0, 12)} is ${behind} commit(s) behind ` +
            `${BASE_BRANCH}; merging ${BASE_BRANCH} into the branch`,
        );
        const oldHead = pull.head;
        try {
          exec("gh", ["pr", "update-branch", String(pr)]);
        } catch (error) {
          throw new Error(
            `cannot update PR #${pr} from ${BASE_BRANCH} (resolve the conflict locally and push): ` +
              error.message,
          );
        }
        updates += 1;
        pull.head = await waitForNewHead({
          exec,
          pr,
          oldHead,
          waitMs: updateWaitMs,
          pollMs,
          now,
          sleep,
          log,
        });
        refreshed = true;
        continue;
      }
    }

    if (!merging) {
      log(`PR head ${pull.head.slice(0, 12)} contains ${BASE_BRANCH}`);

      await waitForCi({
        exec,
        repository,
        head: pull.head,
        timeoutMin,
        pollMs,
        now,
        sleep,
        log,
      });
    }

    if (noUpdate) {
      log(`ready to merge PR #${pr} at ${pull.head}; --no-update stops here`);
      return { status: "checked", head: pull.head };
    }
    if (dryRun) {
      log(
        `[dry-run] would run: gh pr merge ${pr} --squash --admin --delete-branch ` +
          `--match-head-commit ${pull.head}`,
      );
      return { status: "dry-run", wouldMerge: true, head: pull.head };
    }

    let recheckCompare = compareWithDevelop(exec, repository, pull.head);
    let recheck = recheckCompare?.behind_by;
    if (recheck !== 0) {
      const shaNow = readDevelopSha(exec, repository);
      if (shaNow === developSha && !merging) {
        // `develop` did not move and the head was up to date before this
        // check, so a non-zero value is a lagging compare. Give it the same
        // grace period instead of merging on it. (A fast-path merge expects
        // the head to stay behind, so it is not treated as lag.)
        const settled = await waitForCompareSettled({
          exec,
          repository,
          head: pull.head,
          baselineSha: developSha,
          timeoutMs: compareWaitMs,
          pollMs,
          now,
          sleep,
          log,
        });
        if (settled.developSha === developSha) {
          if (settled.behindBy !== 0) {
            throw staleCompareError(settled.behindBy);
          }
          recheck = 0;
        } else {
          developSha = settled.developSha;
          recheck = settled.behindBy;
        }
      } else if (shaNow !== developSha) {
        developSha = shaNow;
      }
      if (recheck !== 0) {
        // The compare may have been read before `develop` moved; read it again
        // right before deciding, so the file sets describe the real base.
        recheckCompare = compareWithDevelop(exec, repository, pull.head);
        recheck = recheckCompare?.behind_by;
      }
      if (recheck !== 0) {
        let mergeWithoutRefresh = false;
        if (!alwaysRefresh) {
          const candidate = await evaluateFastPath(recheck, recheckCompare);
          if (candidate.allowed) {
            log(
              `${candidate.reason} → merging on green ${REQUIRED_CHECK} at ` +
                `${pull.head.slice(0, 12)} without refresh`,
            );
            mergeWithoutRefresh = true;
          } else {
            log(
              `${BASE_BRANCH} moved while CI was running (${candidate.reason}); ` +
                `refreshing the branch`,
            );
          }
        } else {
          log(`${BASE_BRANCH} moved while CI was running; refreshing the branch`);
        }
        if (!mergeWithoutRefresh) {
          moves += 1;
          if (moves > maxMoves) {
            throw new Error(
              `${BASE_BRANCH} moved ${moves} time(s) during the run; ` +
                `nothing was merged — retry later`,
            );
          }
          continue;
        }
      }
    }

    log(`merging PR #${pr} (squash) at ${pull.head.slice(0, 12)}`);
    try {
      exec("gh", [
        "pr",
        "merge",
        String(pr),
        "--squash",
        "--admin",
        "--delete-branch",
        "--match-head-commit",
        pull.head,
      ]);
    } catch (error) {
      throw new Error(
        `gh pr merge failed (the head may have moved; re-run): ${error.message}`,
      );
    }
    const sha = await waitForMergeSha({
      exec,
      pr,
      waitMs: updateWaitMs,
      pollMs,
      now,
      sleep,
    });
    log(`merged as ${sha}`);
    const deployRun = await findDeployRun({
      exec,
      repository,
      sha,
      waitMs: deployWaitMs,
      pollMs: deployPollMs,
      now,
      sleep,
      log,
    });
    if (deployRun) {
      log(`deploy-home: ${deployRun.url}`);
    } else {
      log(
        `deploy-home has not started yet; check Actions → ${DEPLOY_WORKFLOW} ` +
          `for commit ${sha} (push to ${BASE_BRANCH} triggers it)`,
      );
    }
    return { status: "merged", head: pull.head, sha, deployRun };
  }
}

export function spawnExec(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    env: process.env,
    ...options,
  });
  if (result.error) {
    throw new Error(`${command}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    const stderr = String(result.stderr || "").trim();
    throw new Error(`${command} ${args.join(" ")}: ${stderr || `exit ${result.status}`}`);
  }
  return String(result.stdout || "");
}

function repositoryName(exec, env) {
  if (env.GITHUB_REPOSITORY) {
    return env.GITHUB_REPOSITORY;
  }
  return String(
    exec("gh", ["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"]),
  ).trim();
}

export async function main(argv = process.argv.slice(2), env = process.env) {
  const args = parseArgs(argv);
  if (args.help) {
    console.log(
      "usage: safe-merge.mjs <pr> [--dry-run] [--no-update] [--always-refresh] [--timeout <min>]\n" +
        "  merge a PR into develop only when ci-ok ran on the tree that lands;\n" +
        "  a disjoint PR with green ci-ok merges without an update-branch refresh",
    );
    return 0;
  }
  if (args.pr === null) {
    throw new Error(
      "usage: safe-merge.mjs <pr> [--dry-run] [--no-update] [--always-refresh] [--timeout <min>]",
    );
  }
  await runSafeMerge({
    pr: args.pr,
    repository: repositoryName(spawnExec, env),
    exec: spawnExec,
    dryRun: args.dryRun,
    noUpdate: args.noUpdate,
    alwaysRefresh: args.alwaysRefresh,
    timeoutMin: args.timeoutMin,
  });
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
