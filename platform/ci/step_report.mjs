#!/usr/bin/env node

import fs from "node:fs";
import process from "node:process";
import { fileURLToPath } from "node:url";

export const PASSING_OUTCOMES = new Set(["success", "skipped"]);

export const OUTCOME_ICONS = {
  success: "✅",
  skipped: "⏭️",
  failure: "❌",
  cancelled: "🚫",
  missing: "❔",
};

export function parseCheck(spec) {
  const index = String(spec).indexOf("=");
  if (index <= 0) {
    throw new Error(`--check expects <step-id>[?]=<title>, got '${spec}'`);
  }
  const key = spec.slice(0, index);
  const optional = key.endsWith("?");
  const id = optional ? key.slice(0, -1) : key;
  if (!id) {
    throw new Error(`--check expects <step-id>[?]=<title>, got '${spec}'`);
  }
  return { id, title: spec.slice(index + 1) || id, optional };
}

export function evaluate(steps, checks) {
  const rows = checks.map(({ id, title, optional = false }) => {
    const outcome = steps?.[id]?.outcome ?? (optional ? "skipped" : "missing");
    return { id, title, outcome, ok: PASSING_OUTCOMES.has(outcome) };
  });
  return { rows, failed: rows.filter((row) => !row.ok) };
}

export function renderSummary(title, rows) {
  const lines = [`### ${title}`, "", "| check | result |", "| --- | --- |"];
  for (const row of rows) {
    lines.push(`| ${row.title} | ${OUTCOME_ICONS[row.outcome] ?? ""} ${row.outcome} |`);
  }
  return `${lines.join("\n")}\n\n`;
}

function parseArgs(argv) {
  const args = { title: "Checks", checks: [], summary: process.env.GITHUB_STEP_SUMMARY ?? "" };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const value = () => {
      index += 1;
      if (index >= argv.length) {
        throw new Error(`${arg} needs a value`);
      }
      return argv[index];
    };
    switch (arg) {
      case "--title":
        args.title = value();
        break;
      case "--check":
        args.checks.push(parseCheck(value()));
        break;
      case "--summary":
        args.summary = value();
        break;
      default:
        throw new Error(`unrecognized argument: ${arg}`);
    }
  }
  if (!args.checks.length) {
    throw new Error("at least one --check <step-id>=<title> is required");
  }
  return args;
}

export function main(argv = process.argv.slice(2), env = process.env) {
  const args = parseArgs(argv);
  let steps;
  try {
    steps = JSON.parse(env.STEPS || "{}");
  } catch (error) {
    console.error(`cannot parse STEPS: ${error.message}`);
    return 1;
  }
  const { rows, failed } = evaluate(steps, args.checks);
  const summary = renderSummary(args.title, rows);
  if (args.summary) {
    fs.appendFileSync(args.summary, summary, "utf8");
  }
  for (const row of rows) {
    console.log(`${row.ok ? "ok  " : "FAIL"} ${row.title} (${row.outcome})`);
  }
  if (failed.length) {
    for (const row of failed) {
      console.log(`::error title=${args.title}::${row.title}: ${row.outcome}`);
    }
    console.log(`${failed.length} of ${rows.length} checks failed: ${failed.map((row) => row.title).join(", ")}`);
    return 1;
  }
  return 0;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exitCode = main();
}
