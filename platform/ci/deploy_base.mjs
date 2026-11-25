import { spawnSync } from "node:child_process";

export const DEPLOY_WORKFLOW = "deploy-home.yml";
export const DEPLOY_BRANCH = "develop";
export const RUN_PAGE_SIZE = 30;
const SHA_PATTERN = /^[0-9a-f]{40}$/;
const NULL_SHA = /^0+$/;

function usableSha(value) {
  const sha = String(value || "").toLowerCase();
  return SHA_PATTERN.test(sha) && !NULL_SHA.test(sha) ? sha : "";
}

export function lastDeployedSha(runs, headSha) {
  const head = usableSha(headSha);
  const deployed = (runs || [])
    .filter((run) => run?.conclusion === "success" && run?.event === "push")
    .filter((run) => run?.head_branch === DEPLOY_BRANCH)
    .map((run) => usableSha(run.head_sha))
    .find((sha) => sha && sha !== head);
  return deployed || "";
}

export function selectDeployBase({ runs, headSha, beforeSha, isAncestor }) {
  const deployed = lastDeployedSha(runs, headSha);
  const candidate = deployed || usableSha(beforeSha);
  const source = deployed ? "last-successful-deploy" : "event-before";
  if (!candidate) {
    return { all: true, reason: "no deploy base found" };
  }
  if (!isAncestor(candidate, headSha)) {
    return { all: true, reason: `${source} ${candidate} is not an ancestor of ${headSha}` };
  }
  return { all: false, base: candidate, source };
}

export function gitIsAncestor(root) {
  return (ancestor, descendant) =>
    spawnSync("git", ["merge-base", "--is-ancestor", ancestor, descendant], {
      cwd: root,
      stdio: "ignore",
    }).status === 0;
}

export async function successfulDeployRuns({ github, owner, repo }) {
  const response = await github.rest.actions.listWorkflowRuns({
    owner,
    repo,
    workflow_id: DEPLOY_WORKFLOW,
    branch: DEPLOY_BRANCH,
    event: "push",
    status: "success",
    per_page: RUN_PAGE_SIZE,
  });
  return response?.data?.workflow_runs || [];
}

export async function resolveDeployBase({ github, context, core, root, headSha, beforeSha }) {
  const runs = await successfulDeployRuns({ github, owner: context.repo.owner, repo: context.repo.repo }).then(
    (value) => value,
    (error) => {
      core.warning(`listing ${DEPLOY_WORKFLOW} runs failed; using event.before: ${error.message}`);
      return [];
    },
  );
  return selectDeployBase({ runs, headSha, beforeSha, isAncestor: gitIsAncestor(root) });
}
