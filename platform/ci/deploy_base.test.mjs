import assert from "node:assert/strict";
import test from "node:test";

import { lastDeployedSha, resolveDeployBase, selectDeployBase } from "./deploy_base.mjs";

const HEAD = "a".repeat(40);
const BEFORE = "b".repeat(40);
const DEPLOYED = "c".repeat(40);
const OLDER = "d".repeat(40);
const ZERO = "0".repeat(40);

function run(sha, overrides = {}) {
  return { head_sha: sha, conclusion: "success", event: "push", head_branch: "develop", ...overrides };
}

const always = () => true;
const never = () => false;

test("the last successful deploy is the base, not event.before", () => {
  const picked = selectDeployBase({
    runs: [run(DEPLOYED), run(OLDER)],
    headSha: HEAD,
    beforeSha: BEFORE,
    isAncestor: always,
  });
  assert.deepEqual(picked, { all: false, base: DEPLOYED, source: "last-successful-deploy" });
});

test("a cancelled deploy in between keeps its changes because the base stays at the older success", () => {
  const picked = selectDeployBase({
    runs: [run(BEFORE, { conclusion: "cancelled" }), run(OLDER)],
    headSha: HEAD,
    beforeSha: BEFORE,
    isAncestor: always,
  });
  assert.equal(picked.base, OLDER);
});

test("a rerun of an already deployed head looks past its own success", () => {
  assert.equal(lastDeployedSha([run(HEAD), run(OLDER)], HEAD), OLDER);
});

test("manual dispatches and other branches never become the base", () => {
  const runs = [run(DEPLOYED, { event: "workflow_dispatch" }), run(OLDER, { head_branch: "feat/x" })];
  assert.equal(lastDeployedSha(runs, HEAD), "");
});

test("without a successful deploy it falls back to event.before", () => {
  const picked = selectDeployBase({ runs: [], headSha: HEAD, beforeSha: BEFORE, isAncestor: always });
  assert.deepEqual(picked, { all: false, base: BEFORE, source: "event-before" });
});

test("no usable base at all deploys every unit", () => {
  const picked = selectDeployBase({ runs: [], headSha: HEAD, beforeSha: ZERO, isAncestor: always });
  assert.equal(picked.all, true);
});

test("a base that is not an ancestor of head deploys every unit", () => {
  const picked = selectDeployBase({ runs: [run(DEPLOYED)], headSha: HEAD, beforeSha: BEFORE, isAncestor: never });
  assert.equal(picked.all, true);
  assert.match(picked.reason, /not an ancestor/);
});

test("an API failure warns and falls back to event.before", async () => {
  const warnings = [];
  const github = { rest: { actions: { listWorkflowRuns: async () => Promise.reject(new Error("403")) } } };
  const picked = await resolveDeployBase({
    github,
    context: { repo: { owner: "acme", repo: "home" } },
    core: { warning: (message) => warnings.push(message) },
    root: process.cwd(),
    headSha: "HEAD",
    beforeSha: "HEAD~0",
  });
  assert.equal(warnings.length, 1);
  assert.equal(picked.all, true);
});
