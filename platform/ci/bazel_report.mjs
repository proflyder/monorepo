#!/usr/bin/env node

import fs from "node:fs";
import process from "node:process";
import { fileURLToPath } from "node:url";

export const CHECK_KINDS = [
  { kind: "lint", pattern: /(^|[_:-])(android_)?lint$/ },
  { kind: "detekt", pattern: /(^|[_:-])detekt$/ },
  { kind: "clippy", pattern: /(^|[_:-])clippy$/ },
  { kind: "format", pattern: /(^|[_:-])(rustfmt|fmt|format)$/ },
];

export const KIND_ORDER = ["build", "test", "lint", "detekt", "clippy", "format"];

export const FAILED_TEST_STATUSES = new Set([
  "FAILED",
  "TIMEOUT",
  "INCOMPLETE",
  "REMOTE_FAILURE",
  "FAILED_TO_BUILD",
  "TOOL_HALTED_BEFORE_TESTING",
]);

export const LOG_TAIL_LINES = 60;

export function checkKind(label, isTest) {
  const name = String(label).split(":").pop() ?? "";
  for (const { kind, pattern } of CHECK_KINDS) {
    if (pattern.test(name)) {
      return kind;
    }
  }
  return isTest ? "test" : "build";
}

export function parseEvents(text) {
  const events = [];
  for (const line of String(text).split(/\r?\n/)) {
    if (!line.trim()) {
      continue;
    }
    try {
      events.push(JSON.parse(line));
    } catch {
      continue;
    }
  }
  return events;
}

function fileFromUri(uri) {
  if (typeof uri !== "string" || !uri.startsWith("file://")) {
    return "";
  }
  try {
    return fileURLToPath(uri);
  } catch {
    return "";
  }
}

export function collect(events) {
  const targets = new Map();
  const actions = [];
  let exitCode = null;
  let exitName = "";
  const target = (label) => {
    if (!targets.has(label)) {
      targets.set(label, {
        label,
        isTest: false,
        built: null,
        testStatus: "",
        logs: [],
        abortReason: "",
      });
    }
    return targets.get(label);
  };
  for (const event of events) {
    const id = event.id ?? {};
    if (id.targetConfigured?.label && event.configured?.testSize) {
      target(id.targetConfigured.label).isTest = true;
    }
    if (id.targetCompleted?.label) {
      const entry = target(id.targetCompleted.label);
      if (event.completed) {
        entry.built = event.completed.success === true;
        if (event.completed.testTimeoutSeconds || event.completed.testTimeout) {
          entry.isTest = true;
        }
      }
      if (event.aborted) {
        entry.built = false;
        entry.abortReason = event.aborted.description || event.aborted.reason || "aborted";
      }
    }
    if (id.testSummary?.label) {
      const entry = target(id.testSummary.label);
      entry.isTest = true;
      entry.testStatus = event.testSummary?.overallStatus ?? "";
      for (const file of event.testSummary?.failed ?? []) {
        const local = fileFromUri(file.uri);
        if (local && !entry.logs.includes(local)) {
          entry.logs.push(local);
        }
      }
    }
    if (id.testResult?.label) {
      const entry = target(id.testResult.label);
      entry.isTest = true;
      const status = event.testResult?.status ?? "";
      if (FAILED_TEST_STATUSES.has(status)) {
        for (const output of event.testResult?.testActionOutput ?? []) {
          const local = output.name === "test.log" ? fileFromUri(output.uri) : "";
          if (local && !entry.logs.includes(local)) {
            entry.logs.push(local);
          }
        }
      }
    }
    if (id.actionCompleted && event.action && event.action.success !== true) {
      actions.push({
        label: event.action.label || id.actionCompleted.label || "",
        mnemonic: event.action.type || "",
        stderr: fileFromUri(event.action.stderr?.uri),
        exitCode: event.action.exitCode ?? null,
      });
    }
    if (event.finished) {
      exitCode = event.finished.exitCode?.code ?? exitCode;
      exitName = event.finished.exitCode?.name ?? exitName;
    }
  }
  return { targets: [...targets.values()], actions, exitCode, exitName };
}

export function failures({ targets, actions }) {
  const failed = [];
  const labelsWithActions = new Set(actions.map((action) => action.label).filter(Boolean));
  for (const entry of targets) {
    const testFailed = entry.isTest && FAILED_TEST_STATUSES.has(entry.testStatus);
    const buildFailed = entry.built === false;
    if (!testFailed && !buildFailed) {
      continue;
    }
    if (buildFailed && !testFailed && labelsWithActions.has(entry.label)) {
      continue;
    }
    if (buildFailed && entry.abortReason && !testFailed) {
      failed.push({
        kind: checkKind(entry.label, entry.isTest),
        label: entry.label,
        status: "NOT_BUILT",
        detail: entry.abortReason,
        logs: [],
      });
      continue;
    }
    failed.push({
      kind: checkKind(entry.label, entry.isTest),
      label: entry.label,
      status: testFailed ? entry.testStatus : "BUILD_FAILED",
      detail: "",
      logs: testFailed ? entry.logs : [],
    });
  }
  for (const action of actions) {
    failed.push({
      kind: checkKind(action.label, false),
      label: action.label || "(unlabelled action)",
      status: action.mnemonic ? `${action.mnemonic} failed` : "action failed",
      detail: "",
      logs: action.stderr ? [action.stderr] : [],
    });
  }
  const order = (kind) => {
    const index = KIND_ORDER.indexOf(kind);
    return index === -1 ? KIND_ORDER.length : index;
  };
  return failed.sort(
    (left, right) =>
      order(left.kind) - order(right.kind) || left.label.localeCompare(right.label),
  );
}

export function tail(file, lines = LOG_TAIL_LINES, read = fs.readFileSync) {
  let text;
  try {
    text = read(file, "utf8");
  } catch {
    return "";
  }
  const all = String(text).replace(/\s+$/, "").split(/\r?\n/);
  return all.slice(Math.max(0, all.length - lines)).join("\n");
}

function fence(text) {
  const longest = Math.max(2, ...[...String(text).matchAll(/`+/g)].map((match) => match[0].length));
  const marker = "`".repeat(longest + 1);
  return `${marker}\n${text}\n${marker}`;
}

export function renderSummary({ title, failed, exitCode, exitName, totals, read }) {
  const lines = [`### ${title}`, ""];
  if (exitCode === null) {
    lines.push("Bazel did not report a result (no build event file or it was cut short).", "");
  }
  if (!failed.length) {
    lines.push(
      exitCode === 0 || exitCode === null
        ? `All ${totals.tests} test targets and ${totals.built} built targets passed.`
        : `Bazel exited with ${exitName || exitCode} without a failing target in the event file.`,
    );
    return `${lines.join("\n")}\n`;
  }
  const groups = new Map();
  for (const entry of failed) {
    if (!groups.has(entry.kind)) {
      groups.set(entry.kind, []);
    }
    groups.get(entry.kind).push(entry);
  }
  lines.push("| check | failed |", "| --- | --- |");
  for (const [kind, entries] of groups) {
    lines.push(`| ${kind} | ${entries.length} |`);
  }
  lines.push("");
  for (const [kind, entries] of groups) {
    lines.push(`#### ${kind}: ${entries.length} failed`, "");
    for (const entry of entries) {
      lines.push(`- \`${entry.label}\` — ${entry.status}${entry.detail ? `: ${entry.detail}` : ""}`);
    }
    lines.push("");
    for (const entry of entries) {
      for (const log of entry.logs) {
        const excerpt = tail(log, LOG_TAIL_LINES, read);
        if (!excerpt) {
          continue;
        }
        lines.push(
          `<details><summary>${entry.label} (last ${LOG_TAIL_LINES} lines)</summary>`,
          "",
          fence(excerpt),
          "",
          "</details>",
          "",
        );
      }
    }
  }
  return `${lines.join("\n")}\n`;
}

export function annotations(failed) {
  return failed.map(
    (entry) => `::error title=${entry.kind} failed::${entry.label} — ${entry.status}`,
  );
}

export function report({ text, title, read }) {
  const collected = collect(parseEvents(text));
  const failed = failures(collected);
  const totals = {
    tests: collected.targets.filter((entry) => entry.isTest).length,
    built: collected.targets.filter((entry) => entry.built === true).length,
  };
  return {
    failed,
    exitCode: collected.exitCode,
    summary: renderSummary({
      title,
      failed,
      exitCode: collected.exitCode,
      exitName: collected.exitName,
      totals,
      read,
    }),
  };
}

function parseArgs(argv) {
  const args = { events: "", title: "Bazel", summary: process.env.GITHUB_STEP_SUMMARY ?? "" };
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
      case "--events":
        args.events = value();
        break;
      case "--title":
        args.title = value();
        break;
      case "--summary":
        args.summary = value();
        break;
      default:
        throw new Error(`unrecognized argument: ${arg}`);
    }
  }
  if (!args.events) {
    throw new Error("--events <build_event_json_file> is required");
  }
  return args;
}

export function main(argv = process.argv.slice(2)) {
  const args = parseArgs(argv);
  const text = fs.existsSync(args.events) ? fs.readFileSync(args.events, "utf8") : "";
  const outcome = report({ text, title: args.title });
  if (args.summary) {
    fs.appendFileSync(args.summary, outcome.summary, "utf8");
  } else {
    process.stdout.write(outcome.summary);
  }
  for (const line of annotations(outcome.failed)) {
    console.log(line);
  }
  if (outcome.failed.length) {
    console.log(`${outcome.failed.length} failing target(s) or action(s)`);
  }
  return outcome.failed.length ? 1 : 0;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exitCode = main();
}
