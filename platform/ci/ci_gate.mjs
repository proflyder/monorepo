#!/usr/bin/env node
/**
 * Decide whether the aggregate `ci-ok` job passes.
 *
 * `ci-ok` is the single required status check. It must be green when every
 * dispatched job succeeded *or was legitimately skipped* (docs-only change, or
 * no unit of that platform was affected), and red for any failure or
 * cancellation.
 *
 * Reads the `${{ toJSON(needs) }}` payload from the `NEEDS` environment
 * variable, or from the `needs` object passed by `actions/github-script`.
 */

import process from "node:process";
import { fileURLToPath } from "node:url";

export const OK_RESULTS = new Set(["success", "skipped"]);

export function evaluate(needs) {
  const results = Object.fromEntries(
    Object.entries(needs).map(([name, value]) => [name, value?.result ?? "missing"]),
  );
  const failing = Object.fromEntries(
    Object.entries(results).filter(([, result]) => !OK_RESULTS.has(result)),
  );
  return {
    ok: Object.keys(failing).length === 0 && Object.keys(results).length > 0,
    results,
    failing,
  };
}

function parseNeeds(raw) {
  if (!raw) {
    throw new Error("NEEDS is empty: ci-ok must depend on every workflow job");
  }
  try {
    return JSON.parse(raw);
  } catch (error) {
    throw new Error(`cannot parse NEEDS: ${error.message}`);
  }
}

export function gate(needs) {
  const { ok, results, failing } = evaluate(needs);
  const lines = [`job results: ${JSON.stringify(results, null, 2)}`];
  if (!ok) {
    lines.push(`failing jobs: ${JSON.stringify(failing)}`);
    return { ok, lines, results, failing };
  }
  lines.push("all jobs succeeded or were legitimately skipped");
  return { ok, lines, results, failing };
}

export function main() {
  let needs;
  try {
    needs = parseNeeds(process.env.NEEDS ?? "");
  } catch (error) {
    console.error(error.message);
    return 1;
  }
  const outcome = gate(needs);
  if (!outcome.ok) {
    console.log(`job results: ${JSON.stringify(outcome.results, null, 2)}`);
    console.error(outcome.lines.at(-1));
    return 1;
  }
  for (const line of outcome.lines) {
    console.log(line);
  }
  return 0;
}

/** Entry point for `actions/github-script`: throw on a red gate. */
export function runGithubScript({ core, needs }) {
  const outcome = gate(needs);
  for (const line of outcome.lines) {
    core.info(line);
  }
  if (!outcome.ok) {
    throw new Error(`failing jobs: ${JSON.stringify(outcome.failing)}`);
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exitCode = main();
}
