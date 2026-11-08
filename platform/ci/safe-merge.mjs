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
 *      base, so merge `develop` into the branch (`gh pr update-branch`) and
 *      wait for the new head. A conflict is a hard error — nothing is merged;
 *   3. wait for `ci-ok` on the current head (check-runs API, configurable
 *      timeout, 90 min by default). A non-success conclusion stops before merge;
 *   4. re-check `behind_by` right before merging and restart from step 2 if
 *      `develop` moved again (bounded number of attempts);
 *   5. merge with `--match-head-commit`, so a head that moved between the check
 *      and the merge is refused by GitHub instead of merged;
 *   6. print the merge sha and the `deploy-home` run URL for it.
 *
 * `--dry-run` performs every read and reports what it would do without
 * updating or merging. `--no-update` is a pure check: it refuses a PR whose
 * head lacks `develop` and never merges.
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
export const DEFAULT_MAX_ATTEMPTS = 3;
export const DEFAULT_POLL_MS = 30_000;
export const DEFAULT_UPDATE_WAIT_MS = 120_000;
export const DEFAULT_DEPLOY_WAIT_MS = 60_000;
export const DEFAULT_DEPLOY_POLL_MS = 5_000;
export const DEPLOY_WORKFLOW = "deploy-home.yml";

const COMMIT = /^[0-9a-f]{40}$/;

/** Parse CLI arguments: `<N> [--dry-run] [--timeout <min>] [--no-update]`. */
export function parseArgs(argv) {
  const args = {
    pr: null,
    dryRun: false,
    noUpdate: false,
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
  const compared = ghJson(
    exec,
    ["api", `repos/${repository}/compare/${BASE_BRANCH}...${head}`],
    `compare ${BASE_BRANCH}...${head}`,
  );
  return compared?.behind_by;
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
    timeoutMin = DEFAULT_TIMEOUT_MIN,
    maxAttempts = DEFAULT_MAX_ATTEMPTS,
    pollMs = DEFAULT_POLL_MS,
    updateWaitMs = DEFAULT_UPDATE_WAIT_MS,
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

  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    const decision = decideBehind({
      behindBy: behindBy(exec, repository, pull.head),
      noUpdate,
    });
    if (decision.action === "refuse") {
      throw new Error(decision.message);
    }
    if (decision.action === "update") {
      if (dryRun) {
        log(
          `[dry-run] PR head ${pull.head.slice(0, 12)} is ${decision.behindBy} commit(s) ` +
            `behind ${BASE_BRANCH}; would run: gh pr update-branch ${pr}`,
        );
        return { status: "dry-run", wouldUpdate: true, head: pull.head };
      }
      log(
        `PR head ${pull.head.slice(0, 12)} is ${decision.behindBy} commit(s) behind ` +
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
      continue;
    }

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

    const recheck = decideBehind({ behindBy: behindBy(exec, repository, pull.head) });
    if (recheck.action === "update") {
      log(
        `${BASE_BRANCH} moved while CI was running (${recheck.behindBy} commit(s)); refreshing`,
      );
      if (attempt === maxAttempts) {
        break;
      }
      continue;
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
  throw new Error(
    `${BASE_BRANCH} kept moving after ${maxAttempts} refresh attempt(s); nothing was merged — retry later`,
  );
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
      "usage: safe-merge.mjs <pr> [--dry-run] [--no-update] [--timeout <min>]\n" +
        "  merge a PR into develop only when ci-ok ran on the tree that lands",
    );
    return 0;
  }
  if (args.pr === null) {
    throw new Error("usage: safe-merge.mjs <pr> [--dry-run] [--no-update] [--timeout <min>]");
  }
  await runSafeMerge({
    pr: args.pr,
    repository: repositoryName(spawnExec, env),
    exec: spawnExec,
    dryRun: args.dryRun,
    noUpdate: args.noUpdate,
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
