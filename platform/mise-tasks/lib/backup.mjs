#!/usr/bin/env node
/**
 * Backup front door behind `mise run backup:list|status|now|verify`.
 *
 * The registry (`platform/ops/backups.mjs`) is the single source of truth:
 * every command printed here is copied from the docs section named by the
 * target's `restore_doc`. Hosts are reached through the documented SSH aliases
 * (`ssh -F ~/.ssh/home-lab/config pve|k3s|haos`), never through a LAN address.
 *
 * `list` is offline. `status` runs read-only freshness/probe commands.
 * `now` and `verify` print a plan first and execute only after an explicit
 * `--yes`; `--dry-run` prints the commands and never calls anything. Restores
 * are deliberately not automated: every target prints its `restore_doc`.
 *
 * Usage:
 *
 *     node platform/mise-tasks/lib/backup.mjs list
 *     node platform/mise-tasks/lib/backup.mjs status [target...] [--dry-run]
 *     node platform/mise-tasks/lib/backup.mjs now <target> [--yes] [--dry-run]
 *     node platform/mise-tasks/lib/backup.mjs verify [target...] [--yes] [--dry-run]
 */

import { spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import readline from "node:readline";
import { fileURLToPath } from "node:url";

import {
  ROOT,
  TARGETS,
  resolveTargets,
  validateTargets,
} from "../../ops/backups.mjs";

export const SSH_CONFIG = path.join(
  process.env.HOME || os.homedir(),
  ".ssh/home-lab/config",
);

export function actionCommand(action, options = {}) {
  if (!action) {
    return null;
  }
  const root = options.root ?? ROOT;
  if (action.ssh) {
    return {
      command: "ssh",
      args: [
        "-F",
        options.sshConfig ?? SSH_CONFIG,
        action.ssh,
        action.shell,
      ],
    };
  }
  return { command: "bash", args: ["-c", action.shell], cwd: root };
}

export function parseBackupArgs(argv) {
  const result = { targets: [], yes: false, dryRun: false, help: false };
  for (const arg of argv) {
    if (arg === "--dry-run") {
      result.dryRun = true;
    } else if (arg === "--yes" || arg === "-y") {
      result.yes = true;
    } else if (arg === "--help" || arg === "-h") {
      result.help = true;
    } else if (arg.startsWith("-")) {
      throw new Error(`backup: unknown flag '${arg}'`);
    } else {
      result.targets.push(arg);
    }
  }
  return result;
}

function shellQuote(token) {
  if (/^[A-Za-z0-9_./:=@,+-]+$/.test(token)) {
    return token;
  }
  return `'${token.replace(/'/g, `'\\''`)}'`;
}

export function formatCommand({ command, args }) {
  return [command, ...args].map(shellQuote).join(" ");
}

export function listLines(list = TARGETS) {
  const header = ["id", "schedule", "where", "title"];
  const rows = list.map((target) => [
    target.id,
    target.schedule,
    target.where,
    target.title,
  ]);
  const widths = header.map((title, index) =>
    Math.max(title.length, ...rows.map((row) => row[index].length)),
  );
  const render = (row) =>
    row.map((value, index) => value.padEnd(widths[index])).join("  ").trimEnd();
  const lines = [render(header)];
  lines.push(widths.map((width) => "-".repeat(width)).join("  "));
  for (const row of rows) {
    lines.push(render(row));
  }
  return lines;
}

function lastLine(text) {
  const lines = String(text ?? "")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  if (!lines.length) {
    return "";
  }
  const value = lines[lines.length - 1];
  return value.length > 120 ? `${value.slice(0, 117)}...` : value;
}

function summaryLine(counts) {
  const parts = [`${counts.ok} ok`, `${counts.fail} fail`];
  if (counts.manual) {
    parts.push(`${counts.manual} manual`);
  }
  return `backup: ${parts.join(", ")}`;
}

function defaultExec(command, args, options = {}) {
  return spawnSync(command, args, {
    encoding: "utf8",
    stdio: options.keepStdio ? "inherit" : "pipe",
    env: options.env ?? process.env,
    cwd: options.cwd ?? ROOT,
  });
}

export function runList(argv, options = {}) {
  const stdout = options.stdout ?? process.stdout;
  const list = options.targets ?? TARGETS;
  try {
    parseBackupArgs(argv);
  } catch (error) {
    (options.stderr ?? process.stderr).write(`${error.message}\n`);
    return 2;
  }
  stdout.write(`${listLines(list).join("\n")}\n`);
  return 0;
}

/**
 * Read-only probe of every selected target. `--dry-run` prints the documented
 * commands instead of running them. A target with `status: null` is reported as
 * `manual` and does not fail the run.
 */
export function runStatus(argv, options = {}) {
  const spawn = options.spawn ?? defaultExec;
  const stdout = options.stdout ?? process.stdout;
  const stderr = options.stderr ?? process.stderr;
  const env = options.env ?? process.env;
  const list = options.targets ?? TARGETS;

  let parsed;
  let selected;
  try {
    parsed = parseBackupArgs(argv);
    selected = resolveTargets(list, parsed.targets, { defaultAll: true });
  } catch (error) {
    stderr.write(`${error.message}\n`);
    return 2;
  }

  const counts = { ok: 0, fail: 0, manual: 0 };
  for (const target of selected) {
    if (!target.status) {
      counts.manual += 1;
      stdout.write(`[manual] ${target.id}: ${target.manual_note}\n`);
      continue;
    }
    const command = actionCommand(target.status, options);
    if (parsed.dryRun) {
      stdout.write(`[dry-run] ${target.id}: ${formatCommand(command)}\n`);
      continue;
    }
    const result = spawn(command.command, command.args, {
      env,
      cwd: command.cwd,
    });
    const status = result.status ?? 1;
    const detail = lastLine(result.stdout);
    if (status === 0) {
      counts.ok += 1;
      stdout.write(`[ok]   ${target.id}${detail ? `: ${detail}` : ""}\n`);
    } else {
      counts.fail += 1;
      const reason = lastLine(result.stderr) || lastLine(result.stdout);
      stdout.write(
        `[fail] ${target.id}${reason ? `: ${reason}` : ""}\n`,
      );
    }
  }

  if (parsed.dryRun) {
    stdout.write("dry-run: not probing\n");
    return 0;
  }
  stdout.write(`${summaryLine(counts)}\n`);
  return counts.fail ? 1 : 0;
}

export function nowPlanLines(target, options = {}) {
  const lines = [`backup now ${target.id} (${target.title})`];
  lines.push(`  schedule: ${target.schedule}`);
  lines.push(`  where: ${target.where}`);
  if (!target.run) {
    lines.push("  command: none — this backup is taken through a UI");
    lines.push(`  manual: ${target.manual_note}`);
  } else {
    const command = actionCommand(target.run, options);
    lines.push(`  command: ${formatCommand(command)}`);
    if (target.run_note) {
      lines.push(`  note: ${target.run_note}`);
    }
  }
  lines.push(`  restore: ${target.restore_doc}`);
  return lines;
}

/** Plan, confirm, run. Never runs a UI-only target; prints its note instead. */
export async function runNow(argv, options = {}) {
  const spawn = options.spawn ?? defaultExec;
  const stdout = options.stdout ?? process.stdout;
  const stderr = options.stderr ?? process.stderr;
  const env = options.env ?? process.env;
  const list = options.targets ?? TARGETS;
  const confirm =
    options.confirm ??
    ((question) =>
      readlineDefaultConfirm(question, options.stdin, options.stdout));

  let parsed;
  let selected;
  try {
    parsed = parseBackupArgs(argv);
    selected = resolveTargets(list, parsed.targets);
  } catch (error) {
    stderr.write(`${error.message}\n`);
    return 2;
  }
  if (selected.length !== 1) {
    stderr.write("backup:now: exactly one target (see `mise run backup:list`)\n");
    return 2;
  }

  const target = selected[0];
  stdout.write(`${nowPlanLines(target, options).join("\n")}\n`);
  if (!target.run) {
    stdout.write("backup: UI-only target; nothing was executed\n");
    return 0;
  }
  if (parsed.dryRun) {
    stdout.write("dry-run: nothing executed\n");
    return 0;
  }
  if (!parsed.yes) {
    const approved = await confirm("Run this backup now? [y/N] ");
    if (!approved) {
      stderr.write(
        "backup: not confirmed (pass --yes for a non-interactive run)\n",
      );
      return 1;
    }
  }

  const command = actionCommand(target.run, options);
  const result = spawn(command.command, command.args, {
    env,
    cwd: command.cwd,
    keepStdio: true,
  });
  return result.status ?? 1;
}

export function verifyPlanLines(list, options = {}) {
  const lines = [`backup verify (${list.length} target(s))`];
  for (const target of list) {
    if (!target.verify) {
      lines.push(`  - ${target.id}: no automated check`);
      if (target.verify_note) {
        lines.push(`      note: ${target.verify_note}`);
      }
    } else {
      lines.push(`  - ${target.id}: ${formatCommand(actionCommand(target.verify, options))}`);
      if (target.verify_note) {
        lines.push(`      note: ${target.verify_note}`);
      }
    }
    lines.push(`      restore: ${target.restore_doc}`);
  }
  return lines;
}

/**
 * Run the documented verification for the selected targets. The checks may be
 * expensive (restic reads data from Drive), so everything is printed first and
 * one confirmation covers the batch. Restores stay manual.
 */
export async function runVerify(argv, options = {}) {
  const spawn = options.spawn ?? defaultExec;
  const stdout = options.stdout ?? process.stdout;
  const stderr = options.stderr ?? process.stderr;
  const env = options.env ?? process.env;
  const list = options.targets ?? TARGETS;
  const confirm =
    options.confirm ??
    ((question) =>
      readlineDefaultConfirm(question, options.stdin, options.stdout));

  let parsed;
  let selected;
  try {
    parsed = parseBackupArgs(argv);
    selected = resolveTargets(list, parsed.targets, { defaultAll: true });
  } catch (error) {
    stderr.write(`${error.message}\n`);
    return 2;
  }

  stdout.write(`${verifyPlanLines(selected, options).join("\n")}\n`);
  const executable = selected.filter((target) => target.verify);
  if (!executable.length) {
    stdout.write("backup: no automated verification for the selection\n");
    return 0;
  }
  if (parsed.dryRun) {
    stdout.write("dry-run: nothing executed\n");
    return 0;
  }
  if (!parsed.yes) {
    const approved = await confirm(
      `Run ${executable.length} verification command(s)? [y/N] `,
    );
    if (!approved) {
      stderr.write(
        "backup:verify: not confirmed (pass --yes for a non-interactive run)\n",
      );
      return 1;
    }
  }

  const counts = { ok: 0, fail: 0, manual: 0 };
  for (const target of selected) {
    if (!target.verify) {
      counts.manual += 1;
      continue;
    }
    const command = actionCommand(target.verify, options);
    const result = spawn(command.command, command.args, {
      env,
      cwd: command.cwd,
      keepStdio: true,
    });
    const status = result.status ?? 1;
    if (status === 0) {
      counts.ok += 1;
      stdout.write(`[ok]   ${target.id}\n`);
    } else {
      counts.fail += 1;
      stdout.write(`[fail] ${target.id}\n`);
    }
  }
  stdout.write(`${summaryLine(counts)}\n`);
  return counts.fail ? 1 : 0;
}

function readlineDefaultConfirm(question, stdin = process.stdin, stdout = process.stdout) {
  if (!stdin.isTTY) {
    return Promise.resolve(false);
  }
  const interface_ = readline.createInterface({ input: stdin, output: stdout });
  return new Promise((resolve) => {
    interface_.question(question, (answer) => {
      interface_.close();
      resolve(/^(y|yes|д|да)$/i.test(answer.trim()));
    });
  });
}

const USAGE = `Usage: backup.mjs <command>

  list
  status [target...] [--dry-run]
  now <target> [--yes] [--dry-run]
  verify [target...] [--yes] [--dry-run]
  ids
  validate
`;

export async function main(argv = process.argv.slice(2)) {
  const [command, ...rest] = argv;
  switch (command) {
    case "list":
      return runList(rest);
    case "status":
      return runStatus(rest);
    case "now":
      return runNow(rest);
    case "verify":
      return runVerify(rest);
    case "ids":
      process.stdout.write(`${TARGETS.map((target) => target.id).join("\n")}\n`);
      return 0;
    case "validate": {
      const problems = validateTargets();
      if (problems.length) {
        process.stderr.write(`${problems.join("\n")}\n`);
        return 1;
      }
      process.stdout.write("backup registry: ok\n");
      return 0;
    }
    default:
      process.stderr.write(USAGE);
      return command === undefined ? 0 : 2;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().then(
    (code) => {
      process.exitCode = code;
    },
    (error) => {
      process.stderr.write(`${error?.stack || String(error)}\n`);
      process.exitCode = 1;
    },
  );
}
