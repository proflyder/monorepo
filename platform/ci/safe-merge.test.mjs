// Unit tests for the safe squash-merge flow.
//
// No network, no GitHub: every `gh` call is a scripted fake `exec`, and time
// is an injected counter. The tests cover the decision functions (state,
// behind, checks) and the whole flow (refresh, wait, re-check, merge, deploy).
import assert from "node:assert/strict";
import test from "node:test";

import {
  checkPull,
  checkRunsOf,
  decideBehind,
  decideChecks,
  findDeployRun,
  parseArgs,
  runSafeMerge,
} from "./safe-merge.mjs";

const REPO = "acme/home-lab";
const HEAD = "a".repeat(40);
const HEAD2 = "b".repeat(40);
const MERGE = "c".repeat(40);

function clock(step = 1_000) {
  let time = 0;
  return () => {
    time += step;
    return time;
  };
}

const passChecks = {
  total_count: 1,
  check_runs: [{ id: 2, name: "ci-ok", status: "completed", conclusion: "success" }],
};

/**
 * Scripted `gh` fake. `compares` are successive compare responses, `checks`
 * successive check-runs responses, `runs` successive `gh run list` responses;
 * the last value repeats.
 */
function fakeGh({
  view,
  compares = [0],
  checks = [passChecks],
  runs = [[]],
  nextHead = HEAD2,
} = {}) {
  const calls = [];
  const state = {
    view: { ...view },
    compares: [...compares],
    checks: [...checks],
    runs: [...runs],
  };
  const take = (list) => (list.length > 1 ? list.shift() : list[0]);
  const exec = (command, args) => {
    calls.push([command, ...args]);
    assert.equal(command, "gh", `unexpected command ${command}`);
    const [group, action] = args;
    if (group === "pr" && action === "view") {
      return JSON.stringify(state.view);
    }
    if (group === "repo") {
      return `${REPO}\n`;
    }
    if (group === "pr" && action === "update-branch") {
      state.view = { ...state.view, headRefOid: nextHead };
      return "";
    }
    if (group === "pr" && action === "merge") {
      state.view = {
        ...state.view,
        state: "MERGED",
        mergeCommit: { oid: MERGE },
      };
      return "";
    }
    if (group === "api" && String(action).includes("/compare/")) {
      return JSON.stringify({ behind_by: take(state.compares) });
    }
    if (group === "api" && String(action).includes("/check-runs")) {
      return JSON.stringify(take(state.checks));
    }
    if (group === "run" && action === "list") {
      return JSON.stringify(take(state.runs));
    }
    throw new Error(`unexpected gh call: ${args.join(" ")}`);
  };
  return { calls, exec, state };
}

function pullView(overrides = {}) {
  return {
    number: 7,
    state: "OPEN",
    baseRefName: "develop",
    headRefOid: HEAD,
    mergeable: "MERGEABLE",
    url: "https://github.com/acme/home-lab/pull/7",
    title: "Add a thing",
    ...overrides,
  };
}

function runFlow(exec, overrides = {}) {
  const logs = [];
  return runSafeMerge({
    pr: 7,
    repository: REPO,
    exec,
    pollMs: 1,
    updateWaitMs: 10,
    deployWaitMs: 10,
    deployPollMs: 1,
    now: clock(),
    sleep: async () => {},
    log: (line) => logs.push(line),
    ...overrides,
  }).then((result) => ({ result, logs }));
}

function hasCall(calls, group, action) {
  return calls.some((call) => call[1] === group && call[2] === action);
}

test("parseArgs reads the PR number, flags and timeout", () => {
  assert.deepEqual(parseArgs(["42"]), {
    pr: 42,
    dryRun: false,
    noUpdate: false,
    timeoutMin: 90,
    help: false,
  });
  assert.deepEqual(parseArgs(["42", "--dry-run", "--no-update", "--timeout", "5"]), {
    pr: 42,
    dryRun: true,
    noUpdate: true,
    timeoutMin: 5,
    help: false,
  });
  assert.equal(parseArgs(["--help"]).help, true);
  assert.equal(parseArgs([]).pr, null);
  assert.equal(parseArgs(["--timeout", "5"]).timeoutMin, 5);
  assert.throws(() => parseArgs(["0"]), /positive integer/);
  assert.throws(() => parseArgs(["abc"]), /positive integer/);
  assert.throws(() => parseArgs(["1", "2"]), /extra argument/);
  assert.throws(() => parseArgs(["1", "--timeout"]), /positive number of minutes/);
  assert.throws(() => parseArgs(["1", "--timeout", "0"]), /positive number of minutes/);
  assert.throws(() => parseArgs(["1", "--wat"]), /unrecognized argument/);
});

test("checkPull accepts only an OPEN PR into develop with a full head", () => {
  const pull = checkPull(pullView());
  assert.equal(pull.head, HEAD);
  assert.equal(pull.number, 7);
  assert.throws(() => checkPull(pullView({ state: "MERGED" })), /not OPEN/);
  assert.throws(() => checkPull(pullView({ state: "CLOSED" })), /not OPEN/);
  assert.throws(() => checkPull(pullView({ baseRefName: "main" })), /not 'develop'/);
  assert.throws(() => checkPull(pullView({ headRefOid: "abc" })), /full head commit/);
  assert.throws(() => checkPull(pullView({ mergeable: "CONFLICTING" })), /conflicts with develop/);
  // UNKNOWN is fine: GitHub computes mergeability lazily.
  assert.equal(checkPull(pullView({ mergeable: "UNKNOWN" })).head, HEAD);
});

test("decideBehind treats any behind_by > 0 as an unverified base", () => {
  assert.equal(decideBehind({ behindBy: 0 }).action, "up-to-date");
  assert.equal(decideBehind({ behindBy: 2 }).action, "update");
  const refusal = decideBehind({ behindBy: 2, noUpdate: true });
  assert.equal(refusal.action, "refuse");
  assert.match(refusal.message, /2 commit\(s\) behind develop/);
  assert.throws(() => decideBehind({ behindBy: -1 }), /valid behind_by/);
  assert.throws(() => decideBehind({ behindBy: "2" }), /valid behind_by/);
});

test("decideChecks needs a completed success and keeps pending waiting", () => {
  assert.equal(decideChecks([]).state, "pending");
  assert.equal(
    decideChecks([{ id: 1, name: "ci-ok", status: "in_progress", conclusion: null }]).state,
    "pending",
  );
  assert.equal(decideChecks(passChecks.check_runs).state, "pass");
  const failed = decideChecks([
    { id: 1, name: "ci-ok", status: "completed", conclusion: "failure", html_url: "https://x" },
  ]);
  assert.equal(failed.state, "fail");
  assert.equal(failed.url, "https://x");
  // Other checks are ignored: ci-ok is the aggregate.
  assert.equal(
    decideChecks([{ id: 1, name: "android", status: "completed", conclusion: "failure" }]).state,
    "pending",
  );
  // A re-run is a newer check run with the same name.
  assert.equal(
    decideChecks([
      { id: 1, name: "ci-ok", status: "completed", conclusion: "failure" },
      { id: 2, name: "ci-ok", status: "completed", conclusion: "success" },
    ]).state,
    "pass",
  );
  assert.equal(decideChecks(checkRunsOf({ total_count: 1, check_runs: passChecks.check_runs })).state, "pass");
  assert.throws(() => checkRunsOf({}), /no check_runs/);
});

test("runs the happy path: up to date, ci green, squash merge by head", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [0],
    checks: [passChecks],
    runs: [[{ databaseId: 5, status: "in_progress", url: "https://github.com/acme/home-lab/actions/runs/5" }]],
  });
  const { result } = await runFlow(exec);
  assert.equal(result.status, "merged");
  assert.equal(result.sha, MERGE);
  assert.equal(result.deployRun.url, "https://github.com/acme/home-lab/actions/runs/5");
  const merge = calls.find((call) => call[1] === "pr" && call[2] === "merge");
  assert.deepEqual(merge.slice(4), [
    "--squash",
    "--admin",
    "--delete-branch",
    "--match-head-commit",
    HEAD,
  ]);
  const compare = calls.find((call) => String(call[2]).includes("/compare/"));
  assert.match(compare[2], new RegExp(`/compare/develop\\.\\.\\.${HEAD}$`));
  assert.ok(!hasCall(calls, "pr", "update-branch"));
});

test("updates a branch behind develop and waits for the new head", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [2, 0],
    checks: [passChecks],
    nextHead: HEAD2,
  });
  const { result } = await runFlow(exec);
  assert.equal(result.status, "merged");
  assert.equal(result.head, HEAD2);
  assert.ok(hasCall(calls, "pr", "update-branch"));
  const merge = calls.find((call) => call[1] === "pr" && call[2] === "merge");
  assert.equal(merge[merge.length - 1], HEAD2);
  // compare before the update, after it and the pre-merge re-check
  const compares = calls.filter((call) => String(call[2]).includes("/compare/"));
  assert.equal(compares.length, 3);
  assert.match(compares[1][2], new RegExp(`${HEAD2}$`));
  assert.match(compares[2][2], new RegExp(`${HEAD2}$`));
});

test("a failed branch update is a clear error and merges nothing", async () => {
  const calls = [];
  const exec = (command, args) => {
    calls.push([command, ...args]);
    const [group, action] = args;
    if (group === "pr" && action === "view") {
      return JSON.stringify(pullView());
    }
    if (group === "api" && String(action).includes("/compare/")) {
      return JSON.stringify({ behind_by: 1 });
    }
    if (group === "pr" && action === "update-branch") {
      throw new Error("merge conflict");
    }
    throw new Error(`unexpected gh call: ${args.join(" ")}`);
  };
  await assert.rejects(runFlow(exec), /cannot update PR #7 from develop.*merge conflict/);
  assert.ok(!hasCall(calls, "pr", "merge"));
});

test("--no-update refuses a stale head and never merges", async () => {
  const { calls, exec } = fakeGh({ view: pullView(), compares: [3] });
  await assert.rejects(runFlow(exec, { noUpdate: true }), /3 commit\(s\) behind develop/);
  assert.ok(!hasCall(calls, "pr", "update-branch"));
  assert.ok(!hasCall(calls, "pr", "merge"));
});

test("--no-update with a current head only verifies and stops", async () => {
  const { calls, exec } = fakeGh({ view: pullView(), compares: [0] });
  const { result } = await runFlow(exec, { noUpdate: true });
  assert.equal(result.status, "checked");
  assert.ok(!hasCall(calls, "pr", "merge"));
});

test("red ci-ok stops before merge", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [0],
    checks: [
      {
        total_count: 1,
        check_runs: [
          { id: 1, name: "ci-ok", status: "completed", conclusion: "failure", html_url: "https://ci" },
        ],
      },
    ],
  });
  await assert.rejects(runFlow(exec), /ci-ok concluded 'failure'.*https:\/\/ci/);
  assert.ok(!hasCall(calls, "pr", "merge"));
});

test("pending ci-ok times out without merging", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [0],
    checks: [{ total_count: 0, check_runs: [] }],
  });
  await assert.rejects(
    runFlow(exec, { timeoutMin: 1, now: clock(35_000) }),
    /timed out after 1 min waiting for ci-ok/,
  );
  assert.ok(!hasCall(calls, "pr", "merge"));
});

test("develop moving during CI triggers another refresh", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [0, 2, 2, 0],
    checks: [passChecks],
    nextHead: HEAD2,
  });
  const { result } = await runFlow(exec);
  assert.equal(result.status, "merged");
  assert.ok(hasCall(calls, "pr", "update-branch"));
  assert.equal(calls.filter((call) => call[1] === "pr" && call[2] === "merge").length, 1);
});

test("an endlessly moving develop gives up after the attempt limit", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [0, 1, 1],
    checks: [passChecks],
  });
  await assert.rejects(runFlow(exec, { maxAttempts: 2 }), /kept moving after 2 refresh/);
  assert.ok(!hasCall(calls, "pr", "merge"));
});

test("--dry-run reports a needed update and changes nothing", async () => {
  const { calls, exec } = fakeGh({ view: pullView(), compares: [4] });
  const { result } = await runFlow(exec, { dryRun: true });
  assert.equal(result.status, "dry-run");
  assert.equal(result.wouldUpdate, true);
  assert.ok(!hasCall(calls, "pr", "update-branch"));
  assert.ok(!hasCall(calls, "pr", "merge"));
});

test("--dry-run on a current head waits for ci and reports the merge", async () => {
  const { calls, exec } = fakeGh({ view: pullView(), compares: [0] });
  const { result } = await runFlow(exec, { dryRun: true });
  assert.equal(result.status, "dry-run");
  assert.equal(result.wouldMerge, true);
  assert.ok(!hasCall(calls, "pr", "merge"));
});

test("a non-OPEN PR never reaches compare or merge", async () => {
  const { calls, exec } = fakeGh({ view: pullView({ state: "MERGED" }) });
  await assert.rejects(runFlow(exec), /not OPEN/);
  assert.ok(!calls.some((call) => String(call[2]).includes("/compare/")));
  assert.ok(!hasCall(calls, "pr", "merge"));
});

test("findDeployRun returns null when no run appears in time", async () => {
  const { exec } = fakeGh({ view: pullView(), runs: [[]] });
  const run = await findDeployRun({
    exec,
    repository: REPO,
    sha: MERGE,
    waitMs: 5,
    pollMs: 1,
    now: clock(),
    sleep: async () => {},
  });
  assert.equal(run, null);
});
