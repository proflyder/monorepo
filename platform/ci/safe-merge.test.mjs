// Unit tests for the safe squash-merge flow.
//
// No network, no GitHub: every `gh` call is a scripted fake `exec`, and time
// is an injected counter. The tests cover the decision functions (state,
// behind, checks) and the whole flow (refresh, wait, re-check, merge, deploy).
import assert from "node:assert/strict";
import test from "node:test";

import {
  CI_GLOBAL_PATHS,
  changedFiles,
  checkPull,
  checkRunsOf,
  decideBehind,
  decideChecks,
  decideFastPath,
  findDeployRun,
  isCiGlobalPath,
  parseArgs,
  runSafeMerge,
} from "./safe-merge.mjs";

const REPO = "acme/home-lab";
const HEAD = "a".repeat(40);
const HEAD2 = "b".repeat(40);
const HEAD3 = "9".repeat(40);
const MERGE = "c".repeat(40);
const DEVELOP = "d".repeat(40);
const DEVELOP2 = "e".repeat(40);
const DEVELOP3 = "f".repeat(40);
const BASE_SHA = "5".repeat(40);

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
 * successive check-runs responses, `runs` successive `gh run list` responses,
 * `developShas` successive `develop` refs; the last value repeats.
 */
function fakeGh({
  view,
  compares = [0],
  checks = [passChecks],
  runs = [[]],
  developShas = [DEVELOP],
  nextHead = HEAD2,
  nextHeads = null,
  mergeBase = null,
  headFiles = null,
  developFiles = [],
} = {}) {
  const calls = [];
  const state = {
    view: { ...view },
    compares: [...compares],
    checks: [...checks],
    runs: [...runs],
    developShas: [...developShas],
    nextHeads: nextHeads ? [...nextHeads] : null,
  };
  const take = (list) => (list.length > 1 ? list.shift() : list[0]);
  const files = (paths) =>
    Array.isArray(paths) ? paths.map((path) => ({ filename: path })) : undefined;
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
      const head = state.nextHeads ? take(state.nextHeads) : nextHead;
      state.view = { ...state.view, headRefOid: head };
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
      if (String(action).includes(`/compare/${BASE_SHA}...develop`)) {
        return JSON.stringify({ behind_by: 0, files: files(developFiles) ?? [] });
      }
      const entry = take(state.compares);
      const compared =
        entry && typeof entry === "object" ? { ...entry } : { behind_by: entry };
      if (compared.merge_base_commit === undefined && mergeBase) {
        compared.merge_base_commit = { sha: mergeBase };
      }
      if (compared.files === undefined && headFiles) {
        compared.files = files(headFiles);
      }
      return JSON.stringify(compared);
    }
    if (group === "api" && String(action).includes("/git/ref/heads/develop")) {
      return JSON.stringify({ object: { sha: take(state.developShas) } });
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
    compareWaitMs: 5_000,
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
    alwaysRefresh: false,
    timeoutMin: 90,
    help: false,
  });
  assert.deepEqual(parseArgs(["42", "--dry-run", "--no-update", "--timeout", "5"]), {
    pr: 42,
    dryRun: true,
    noUpdate: true,
    alwaysRefresh: false,
    timeoutMin: 5,
    help: false,
  });
  assert.equal(parseArgs(["42", "--always-refresh"]).alwaysRefresh, true);
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

test("CI_GLOBAL_PATHS covers workflows, CI, Bazel/tool config and lock files", () => {
  assert.deepEqual(CI_GLOBAL_PATHS, [
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
  ]);
  for (const path of [
    ".github/workflows/ci.yml",
    "platform/ci/safe-merge.mjs",
    ".bazelrc",
    ".bazelversion",
    "MODULE.bazel",
    "MODULE.bazel.lock",
    "platform/android/android.bazelrc",
    "mise.toml",
    "Cargo.lock",
    "maven_install.json",
    "packages/android/maven_install.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "platform/bazel/zig/BUILD.bazel",
  ]) {
    assert.ok(isCiGlobalPath(path), `${path} should be CI-global`);
  }
  for (const path of [
    "docs/GIT.md",
    "platform/ci-tools/x.mjs",
    "platform/runtime/main.rs",
    "Cargo.toml",
    "platform/android/BUILD.bazel",
    "mise-tasks/x",
    "MODULE",
  ]) {
    assert.equal(isCiGlobalPath(path), false, `${path} should not be CI-global`);
  }
});

test("changedFiles keeps both sides of a rename and flags unknown or capped lists", () => {
  const parsed = changedFiles({
    files: [{ filename: "a.md" }, { filename: "b.md", previous_filename: "c.md" }],
  });
  assert.deepEqual(parsed.paths, ["a.md", "b.md", "c.md"]);
  assert.equal(parsed.known, true);
  assert.equal(parsed.truncated, false);

  // No `files` array proves nothing; it must not read as "no changed files".
  assert.deepEqual(changedFiles({}), { paths: [], known: false, truncated: false });
  assert.equal(changedFiles(undefined).known, false);

  const capped = { files: Array.from({ length: 300 }, (_, i) => ({ filename: `f${i}` })) };
  assert.equal(changedFiles(capped).truncated, true);
});

test("decideFastPath needs green ci-ok, a mergeable PR and disjoint non-CI-global files", () => {
  const safe = {
    behindBy: 2,
    mergeable: "MERGEABLE",
    ciGreen: true,
    prFiles: ["platform/app/main.ts"],
    developFiles: ["docs/GIT.md", "projects/home/main.tf"],
  };
  const allowed = decideFastPath(safe);
  assert.equal(allowed.allowed, true);
  assert.equal(allowed.structural, true);
  assert.match(allowed.reason, /develop \+2 commit\(s\), disjoint from PR files and no CI-global paths/);

  const pending = decideFastPath({ ...safe, ciGreen: false });
  assert.equal(pending.allowed, false);
  assert.equal(pending.structural, true);
  assert.match(pending.reason, /ci-ok is not green/);

  assert.match(decideFastPath({ ...safe, mergeable: "UNKNOWN" }).reason, /not MERGEABLE/);
  assert.match(decideFastPath({ ...safe, mergeable: "CONFLICTING" }).reason, /not MERGEABLE/);
  assert.match(
    decideFastPath({ ...safe, developFiles: ["platform/app/main.ts"] }).reason,
    /same files: platform\/app\/main\.ts/,
  );
  assert.match(
    decideFastPath({ ...safe, developFiles: [".github/workflows/ci.yml"] }).reason,
    /CI-global paths: \.github\/workflows\/ci\.yml/,
  );
  assert.match(decideFastPath({ ...safe, filesKnown: false }).reason, /file list/);
  assert.match(decideFastPath({ ...safe, filesTruncated: true }).reason, /capped at 300/);
  assert.equal(decideFastPath({ ...safe, behindBy: 0 }).structural, false);
  assert.equal(decideFastPath({ ...safe, alwaysRefresh: true }).allowed, false);
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

test("disjoint develop commits merge on green ci-ok without refreshing the branch", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [2],
    mergeBase: BASE_SHA,
    headFiles: ["platform/app/main.ts"],
    developFiles: ["docs/GIT.md", "projects/home/main.tf"],
  });
  const { result, logs } = await runFlow(exec);
  assert.equal(result.status, "merged");
  assert.equal(result.head, HEAD);
  assert.ok(!hasCall(calls, "pr", "update-branch"));
  const merge = calls.find((call) => call[1] === "pr" && call[2] === "merge");
  assert.equal(merge[merge.length - 1], HEAD);
  // The files of the new develop commits were read from the PR's merge base.
  assert.ok(
    calls.some((call) => String(call[2]).includes(`/compare/${BASE_SHA}...develop`)),
  );
  assert.ok(
    logs.some((line) =>
      /develop \+2 commit\(s\), disjoint from PR files and no CI-global paths → merging on green ci-ok at .* without refresh/.test(
        line,
      ),
    ),
  );
});

test("overlapping files fall back to updating the branch", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [2, 0],
    mergeBase: BASE_SHA,
    headFiles: ["platform/app/main.ts"],
    developFiles: ["platform/app/main.ts"],
    nextHead: HEAD2,
  });
  const { result, logs } = await runFlow(exec);
  assert.equal(result.status, "merged");
  assert.equal(result.head, HEAD2);
  assert.ok(hasCall(calls, "pr", "update-branch"));
  assert.ok(logs.some((line) => /same files: platform\/app\/main\.ts; refreshing the branch/.test(line)));
});

test("a CI-global change on develop falls back to updating the branch", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [3, 0],
    mergeBase: BASE_SHA,
    headFiles: ["docs/GIT.md"],
    developFiles: [".github/workflows/ci.yml"],
    nextHead: HEAD2,
  });
  const { result } = await runFlow(exec);
  assert.equal(result.status, "merged");
  assert.ok(hasCall(calls, "pr", "update-branch"));
});

test("develop moving during CI merges without refresh when the changes are disjoint", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    // head starts current, then develop gets a disjoint commit while CI runs
    compares: [0, 2],
    developShas: [DEVELOP, DEVELOP2],
    mergeBase: BASE_SHA,
    headFiles: ["platform/app/main.ts"],
    developFiles: ["docs/GIT.md"],
  });
  const { result, logs } = await runFlow(exec);
  assert.equal(result.status, "merged");
  assert.ok(!hasCall(calls, "pr", "update-branch"));
  assert.ok(
    logs.some((line) =>
      /develop \+2 commit\(s\), disjoint from PR files and no CI-global paths → merging on green ci-ok at .* without refresh/.test(
        line,
      ),
    ),
  );
});

test("a pending ci-ok on a safely stale head is awaited instead of refreshing", async () => {
  const pending = {
    total_count: 1,
    check_runs: [{ id: 1, name: "ci-ok", status: "in_progress", conclusion: null }],
  };
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [2],
    checks: [pending, passChecks],
    mergeBase: BASE_SHA,
    headFiles: ["platform/app/main.ts"],
    developFiles: ["docs/GIT.md"],
  });
  const { result, logs } = await runFlow(exec);
  assert.equal(result.status, "merged");
  assert.ok(!hasCall(calls, "pr", "update-branch"));
  assert.ok(
    logs.some((line) => /waiting for ci-ok on the current head instead of refreshing/.test(line)),
  );
});

test("--always-refresh updates a disjoint branch anyway", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [2, 0],
    mergeBase: BASE_SHA,
    headFiles: ["platform/app/main.ts"],
    developFiles: ["docs/GIT.md"],
    nextHead: HEAD2,
  });
  const { result } = await runFlow(exec, { alwaysRefresh: true });
  assert.equal(result.status, "merged");
  assert.ok(hasCall(calls, "pr", "update-branch"));
});

test("--dry-run reports a refresh-free merge for a safe stale head", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [2],
    mergeBase: BASE_SHA,
    headFiles: ["platform/app/main.ts"],
    developFiles: ["docs/GIT.md"],
  });
  const { result, logs } = await runFlow(exec, { dryRun: true });
  assert.equal(result.status, "dry-run");
  assert.equal(result.wouldMergeWithoutRefresh, true);
  assert.ok(!hasCall(calls, "pr", "update-branch"));
  assert.ok(logs.some((line) => /\[dry-run\].*without refresh/.test(line)));
});

test("a stale head without a merge base keeps refreshing", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [2, 0],
    nextHead: HEAD2,
  });
  const { result, logs } = await runFlow(exec);
  assert.equal(result.status, "merged");
  assert.ok(hasCall(calls, "pr", "update-branch"));
  assert.ok(logs.some((line) => /compare did not report a merge base; refreshing/.test(line)));
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
    if (group === "api" && String(action).includes("/git/ref/heads/develop")) {
      return JSON.stringify({ object: { sha: DEVELOP } });
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
    // head is current, ci runs, develop gets D2, then the refreshed head is current
    compares: [0, 2, 2, 2, 0, 0],
    developShas: [DEVELOP, DEVELOP2],
    checks: [passChecks],
    nextHead: HEAD2,
  });
  const { result } = await runFlow(exec);
  assert.equal(result.status, "merged");
  assert.equal(calls.filter((call) => call[1] === "pr" && call[2] === "update-branch").length, 1);
  assert.equal(calls.filter((call) => call[1] === "pr" && call[2] === "merge").length, 1);
});

test("a stale compare after update-branch does not refresh again", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    // behind -> update; the new head still reports behind for one poll while
    // develop itself is unchanged, then compare catches up
    compares: [1, 1, 1, 0, 0],
    developShas: [DEVELOP],
    checks: [passChecks],
    nextHead: HEAD2,
  });
  const { result } = await runFlow(exec, { maxUpdates: 1 });
  assert.equal(result.status, "merged");
  assert.equal(calls.filter((call) => call[1] === "pr" && call[2] === "update-branch").length, 1);
});

test("a real develop move after update-branch spends a move, not a refresh", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    // update, then compare reports behind while develop moves to D2 -> refresh again
    compares: [1, 1, 1, 1, 1, 0, 0],
    developShas: [DEVELOP, DEVELOP, DEVELOP2, DEVELOP2],
    checks: [passChecks],
    nextHeads: [HEAD2, HEAD3],
  });
  const { result } = await runFlow(exec, { maxMoves: 2 });
  assert.equal(result.status, "merged");
  assert.equal(calls.filter((call) => call[1] === "pr" && call[2] === "update-branch").length, 2);
});

test("a stale compare that never settles stops without a second refresh", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [1, 1],
    developShas: [DEVELOP],
    checks: [passChecks],
  });
  await assert.rejects(
    runFlow(exec, { maxUpdates: 1 }),
    /still reports 1 commit\(s\) behind develop after 5s while develop did not move/,
  );
  assert.equal(calls.filter((call) => call[1] === "pr" && call[2] === "update-branch").length, 1);
  assert.ok(!hasCall(calls, "pr", "merge"));
});

test("real develop moves exhaust the move budget, never the refresh budget", async () => {
  const { calls, exec } = fakeGh({
    view: pullView(),
    compares: [0, 1, 1, 0, 1, 1],
    developShas: [DEVELOP, DEVELOP2, DEVELOP3],
    checks: [passChecks],
  });
  await assert.rejects(runFlow(exec, { maxMoves: 1 }), /moved 2 time\(s\) during the run/);
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
