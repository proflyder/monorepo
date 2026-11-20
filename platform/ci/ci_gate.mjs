#!/usr/bin/env node

import fs from "node:fs";
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

export function main(argv = process.argv.slice(2)) {
  const writeOutput = argv.includes("--github-output");
  let needs;
  try {
    needs = parseNeeds(process.env.NEEDS ?? "");
  } catch (error) {
    console.error(error.message);
    if (writeOutput) {
      writeGithubOutput({ needs: {}, file: process.env.GITHUB_OUTPUT, log: () => {} });
      return 0;
    }
    return 1;
  }
  if (writeOutput) {
    writeGithubOutput({ needs, file: process.env.GITHUB_OUTPUT, log: console.log });
    return 0;
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

export function runGithubScript({ core, needs }) {
  const outcome = gate(needs);
  for (const line of outcome.lines) {
    core.info(line);
  }
  if (!outcome.ok) {
    throw new Error(`failing jobs: ${JSON.stringify(outcome.failing)}`);
  }
}

export function writeGithubOutput({ needs, file, log = console.log }) {
  const outcome = gate(needs);
  for (const line of outcome.lines) {
    log(line);
  }
  if (!file) {
    throw new Error("GITHUB_OUTPUT is not set");
  }
  const failing = Object.entries(outcome.failing)
    .map(([name, result]) => `${name}=${result}`)
    .join(" ");
  append(file, `ok=${outcome.ok ? "true" : "false"}\n`);
  append(file, `failing=${failing}\n`);
  return outcome;
}

function append(file, text) {
  fs.appendFileSync(file, text, "utf8");
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exitCode = main();
}
