#!/usr/bin/env node
/**
 * Deploy and rollback front door behind `mise run deploy`, `deploy:status` and
 * `rollback`.
 *
 * The CI unit map (`platform/ci/units.mjs`) is the single source of truth: a
 * unit is deployable when deploy-home owns images/workloads for it, and its
 * documented rollback procedure lives in the unit's `rollback` block. This
 * module only turns that data into `gh workflow run` dispatches, status reads
 * and rollback commands. It never guesses from a directory name.
 *
 * Nothing here talks to the cluster by default: a dispatch is a `gh` call and
 * `deploy:status` is read-only `gh run list`; `--live` adds read-only
 * `kubectl get deploy`. Rollback starts a `home-deploy`/`helm` command only
 * after an explicit `--yes`.
 *
 * No npm dependencies: Node built-ins plus the unit map only.
 *
 * Usage:
 *
 *     node platform/mise-tasks/lib/ops.mjs deploy <unit...|all> [flags]
 *     node platform/mise-tasks/lib/ops.mjs deploy-status [unit] [flags]
 *     node platform/mise-tasks/lib/ops.mjs rollback <unit> [flags]
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import readline from "node:readline";
import { fileURLToPath } from "node:url";

import { loadConfig } from "../../ci/affected.mjs";

export const HERE = path.dirname(fileURLToPath(import.meta.url));
export const ROOT = path.resolve(HERE, "../../..");

export const WORKFLOW = "deploy-home.yml";
export const REF = "develop";
export const MODES = ["preserve", "execute", "shadow", "control-execute", "control-catalog"];
export const KUBECONFIG_PATH = "secrets/kubernetes-research/kubeconfig";

export function units() {
  return loadConfig().units;
}

/**
 * What a manual `deploy-home` dispatch can address: every unit deploy-home
 * owns (deploy.images/deploy.workloads), plus the Pulsar station release,
 * which `--dispatch units=pulsar` re-deploys through `deploy-station`.
 *
 * Native publication units without a station release (logon-provider) are
 * accepted by the workflow filter but a dispatch does not publish them: only a
 * push to `develop` does. They are deliberately not deploy targets here.
 */
export function isDeployTarget(unit) {
  if (unit.deployImages.length > 0) {
    return true;
  }
  return unit.platform === "native" && unit.nativeKind === "pulsar" && unit.nativePublish;
}

export function deployTargets(list = units()) {
  return list.filter(isDeployTarget);
}

/** Resolve `all`, a comma list or repeated arguments to deployable unit ids. */
export function resolveDeployUnits(list, rawArgs) {
  const targets = deployTargets(list);
  const byId = new Map(targets.map((unit) => [unit.id, unit]));
  const requested = rawArgs
    .flatMap((argument) => argument.split(","))
    .map((item) => item.trim())
    .filter(Boolean);

  if (!requested.length) {
    throw new Error(
      "deploy: name at least one unit, a comma list or `all`.\n" +
        "Run `mise run units` for the full list.",
    );
  }
  if (requested.includes("all")) {
    if (requested.length > 1) {
      throw new Error("deploy: `all` cannot be combined with other units");
    }
    return targets;
  }

  const unknown = [...new Set(requested.filter((id) => !byId.has(id)))].sort();
  if (unknown.length) {
    const known = targets.map((unit) => unit.id).sort();
    throw new Error(
      `deploy: not a deploy-home dispatch target: ${unknown.join(", ")}.\n` +
        `Deployable units: ${known.join(", ")}.\n` +
        "Native release units other than `pulsar` are published only by a push to develop.",
    );
  }

  // Keep the unit-map order so the printed plan is stable.
  return targets.filter((unit) => requested.includes(unit.id));
}

function flagValue(argv, index, name) {
  const inline = argv[index].startsWith(`${name}=`);
  if (inline) {
    return argv[index].slice(name.length + 1);
  }
  if (index + 1 >= argv.length) {
    throw new Error(`${name} needs a value`);
  }
  return argv[index + 1];
}

/** Parse deploy flags; unknown flags fail instead of being silently ignored. */
export function parseDeploy(argv) {
  const result = {
    units: [],
    mode: "preserve",
    simulateHermesFailure: false,
    dryRun: false,
    yes: false,
    watch: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--dry-run") {
      result.dryRun = true;
    } else if (arg === "--yes" || arg === "-y") {
      result.yes = true;
    } else if (arg === "--watch") {
      result.watch = true;
    } else if (arg === "--simulate-hermes-failure") {
      result.simulateHermesFailure = true;
    } else if (arg === "--mode" || arg.startsWith("--mode=")) {
      result.mode = flagValue(argv, index, "--mode");
      if (!arg.includes("=")) {
        index += 1;
      }
    } else if (arg.startsWith("-")) {
      throw new Error(`deploy: unknown flag '${arg}'`);
    } else {
      result.units.push(arg);
    }
  }
  if (!MODES.includes(result.mode)) {
    throw new Error(`deploy: unknown mode '${result.mode}' (expected ${MODES.join(", ")})`);
  }
  return result;
}

/** The `gh workflow run` command for a resolved dispatch. */
export function deployCommand(plan) {
  const args = [
    "workflow",
    "run",
    WORKFLOW,
    "--ref",
    REF,
    "-f",
    `units=${plan.units.map((unit) => unit.id).join(",")}`,
  ];
  if (plan.mode !== "preserve") {
    args.push("-f", `mode=${plan.mode}`);
  }
  if (plan.simulateHermesFailure) {
    args.push("-f", "simulate_hermes_failure=true");
  }
  return { command: "gh", args };
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

/** Human-readable plan: what deploy-home will roll and why. */
export function deployPlanLines(plan) {
  const lines = [
    `deploy-home: ${plan.units.length} unit(s) → ref ${REF}`,
  ];
  for (const unit of plan.units) {
    const where = unit.deployImages.length
      ? `namespace ${unit.deployNamespace}; workloads ${unit.deployWorkloads.join(", ")}`
      : "Pulsar station release (deploy-station)";
    lines.push(`  - ${unit.id}: ${unit.title}`);
    lines.push(`      ${where}`);
  }
  lines.push(`  mode: ${plan.mode}`);
  if (plan.simulateHermesFailure) {
    lines.push("  simulate_hermes_failure: true");
  }
  lines.push("");
  lines.push(`  ${formatCommand(deployCommand(plan))}`);
  return lines;
}

function defaultExec(command, args, options = {}) {
  return spawnSync(command, args, {
    encoding: "utf8",
    stdio: options.keepStdio ? "inherit" : "pipe",
    env: options.env ?? process.env,
    cwd: options.cwd ?? ROOT,
  });
}

/** Ask the operator before dispatching; non-interactive runs must pass --yes. */
export async function defaultConfirm(question, stdin = process.stdin, stdout = process.stdout) {
  if (!stdin.isTTY) {
    return false;
  }
  const interface_ = readline.createInterface({ input: stdin, output: stdout });
  try {
    const answer = await new Promise((resolve) => interface_.question(question, resolve));
    return /^(y|yes|д|да)$/i.test(answer.trim());
  } finally {
    interface_.close();
  }
}

/** Latest run id for the workflow, so `--watch` can follow the dispatch. */
export function latestRun(spawn, env) {
  const result = spawn(
    "gh",
    [
      "run",
      "list",
      "--workflow",
      WORKFLOW,
      "--limit",
      "1",
      "--json",
      "databaseId,status,conclusion,createdAt,displayTitle,url",
    ],
    { env },
  );
  if (result.status !== 0) {
    return null;
  }
  const runs = JSON.parse(result.stdout || "[]");
  return runs.length ? runs[0] : null;
}

/**
 * Run the dispatch. Returns a process exit code. `--dry-run` prints the command
 * and never calls `gh`. Without `--yes` the operator must confirm.
 */
export async function runDeploy(argv, options = {}) {
  const spawn = options.spawn ?? defaultExec;
  const stdout = options.stdout ?? process.stdout;
  const stderr = options.stderr ?? process.stderr;
  const env = options.env ?? process.env;
  const confirm = options.confirm ?? ((question) => defaultConfirm(question));

  let parsed;
  let selected;
  try {
    parsed = parseDeploy(argv);
    selected = resolveDeployUnits(units(), parsed.units);
  } catch (error) {
    stderr.write(`${error.message}\n`);
    return 2;
  }
  if (
    parsed.simulateHermesFailure &&
    !selected.some((unit) => unit.id === "hermes")
  ) {
    stderr.write("deploy: --simulate-hermes-failure needs the hermes unit\n");
    return 2;
  }
  // deploy-home.yml's Mode guard rejects a non-preserve mode for any unit other
  // than ha-bridge; fail here with the same rule instead of after the dispatch.
  if (
    parsed.mode !== "preserve" &&
    !(selected.length === 1 && selected[0].id === "ha-bridge")
  ) {
    stderr.write(
      `deploy: mode '${parsed.mode}' is only allowed for the ha-bridge unit ` +
        "(executor state)\n",
    );
    return 2;
  }

  const plan = {
    units: selected,
    mode: parsed.mode,
    simulateHermesFailure: parsed.simulateHermesFailure,
  };
  stdout.write(`${deployPlanLines(plan).join("\n")}\n`);

  if (parsed.dryRun) {
    stdout.write("dry-run: not dispatching\n");
    return 0;
  }
  if (!parsed.yes) {
    const approved = await confirm("Dispatch deploy-home? [y/N] ");
    if (!approved) {
      stderr.write("deploy: not confirmed (pass --yes for a non-interactive run)\n");
      return 1;
    }
  }

  const command = deployCommand(plan);
  const dispatched = spawn(command.command, command.args, { env });
  if (dispatched.status !== 0) {
    stderr.write((dispatched.stderr || `${command.command} failed\n`).trimEnd() + "\n");
    return dispatched.status ?? 1;
  }
  stdout.write("dispatched deploy-home\n");

  const run = latestRun(spawn, env);
  if (!run) {
    stderr.write("deploy: could not find the dispatched run (check `gh run list --workflow deploy-home.yml`)\n");
    return 1;
  }
  const state = run.conclusion ? ` (${run.conclusion})` : "";
  stdout.write(`run ${run.databaseId}: ${run.status ?? "unknown"}${state} ${run.url ?? ""}\n`);
  if (parsed.watch) {
    const watched = spawn("gh", ["run", "watch", String(run.databaseId), "--exit-status"], {
      env,
      keepStdio: true,
    });
    return watched.status ?? 1;
  }
  return 0;
}

function formatRun(run) {
  const state = run.conclusion ? `${run.status}/${run.conclusion}` : run.status;
  return `${String(run.databaseId).padEnd(12)} ${String(state).padEnd(20)} ${run.createdAt ?? ""}  ${run.url ?? ""}`;
}

export function parseStatus(argv) {
  const result = { unit: "", live: false, limit: 5 };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--live") {
      result.live = true;
    } else if (arg === "--limit" || arg.startsWith("--limit=")) {
      result.limit = flagValue(argv, index, "--limit");
      if (!arg.includes("=")) {
        index += 1;
      }
      if (!/^[1-9][0-9]*$/.test(result.limit)) {
        throw new Error(`deploy:status: --limit must be a positive integer`);
      }
      result.limit = Number(result.limit);
    } else if (arg.startsWith("-")) {
      throw new Error(`deploy:status: unknown flag '${arg}'`);
    } else if (result.unit) {
      throw new Error("deploy:status: at most one unit");
    } else {
      result.unit = arg;
    }
  }
  return result;
}

function kubeconfigFor(root, env) {
  if (env.KUBECONFIG) {
    return env.KUBECONFIG.split(path.delimiter).filter(Boolean)[0];
  }
  const documented = path.resolve(root, KUBECONFIG_PATH);
  return fs.existsSync(documented) ? documented : "";
}

export function statusKubectlCommand(unit, kubeconfig) {
  const args = [];
  if (kubeconfig) {
    args.push("--kubeconfig", kubeconfig);
  }
  args.push("-n", unit.deployNamespace, "get", "deploy", ...unit.deployWorkloads, "-o", "wide");
  return { command: "kubectl", args };
}

export function parseRollback(argv) {
  const result = { unit: "", yes: false, dryRun: false };
  for (const arg of argv) {
    if (arg === "--yes" || arg === "-y") {
      result.yes = true;
    } else if (arg === "--dry-run") {
      result.dryRun = true;
    } else if (arg.startsWith("-")) {
      throw new Error(`rollback: unknown flag '${arg}'`);
    } else if (result.unit) {
      throw new Error("rollback: exactly one unit");
    } else {
      result.unit = arg;
    }
  }
  return result;
}

export function findRollbackUnit(list, id) {
  if (!id) {
    throw new Error("rollback: name a unit (see `mise run units`)");
  }
  const unit = list.find((candidate) => candidate.id === id);
  if (!unit) {
    throw new Error(`rollback: unknown unit '${id}' (see \`mise run units\`)`);
  }
  return unit;
}

/**
 * The documented rollback command for a unit, or null when the kind only
 * prints an instruction. Source of truth: the unit's `rollback` block.
 */
export function rollbackCommand(unit) {
  const stateDir = unit.rollbackStateDir;
  switch (unit.rollbackKind) {
    case "deploy-state": {
      const script =
        `set -euo pipefail; source "${path.join(ROOT, "platform/deploy/resolve.sh")}"; ` +
        `home_deploy state rollback --state-dir "${stateDir}" ` +
        `--namespace "${unit.rollbackNamespace}"`;
      return { command: "bash", args: ["-c", script] };
    }
    case "helm":
      return {
        command: "helm",
        args: ["--namespace", unit.rollbackNamespace, "rollback", unit.rollbackRelease],
      };
    case "hermes":
      return {
        command: "bash",
        args: [
          "-c",
          `set -euo pipefail; source "${path.join(ROOT, "platform/deploy/resolve.sh")}"; ` +
            `home_deploy hermes rollout --rollback-only --state-dir "${stateDir}"`,
        ],
      };
    default:
      return null;
  }
}

export function rollbackPlanLines(unit) {
  const lines = [`rollback ${unit.id} (${unit.title})`];
  if (!unit.rollbackKind) {
    lines.push("  kind: none");
    lines.push("  no documented rollback procedure for this unit.");
    return lines;
  }
  lines.push(`  kind: ${unit.rollbackKind}`);
  lines.push(`  source: ${unit.rollbackSource || "platform/ci/units.mjs"}`);
  if (unit.rollbackKind === "deploy-state") {
    lines.push(`  restores: Deployments ${unit.deployWorkloads.join(", ")} in namespace ${unit.deployNamespace}`);
    lines.push(`  state: ${unit.rollbackStateDir}`);
  }
  if (unit.rollbackKind === "hermes") {
    lines.push(`  restores: Helm release ${unit.rollbackRelease} in namespace ${unit.rollbackNamespace}`);
    lines.push(`  state: ${unit.rollbackStateDir} (recorded previous revision)`);
  }
  if (unit.rollbackKind === "helm") {
    lines.push(`  restores: Helm release ${unit.rollbackRelease} in namespace ${unit.rollbackNamespace}`);
  }
  if (unit.rollbackNote) {
    lines.push(`  note: ${unit.rollbackNote}`);
  }
  if (unit.rollbackKind === "forward-only") {
    lines.push("  action: none — fix forward with the previous commit.");
    return lines;
  }
  if (unit.rollbackKind === "pulsar") {
    lines.push("  action: on-device only; run this on the station (never automatically):");
    lines.push(`    ${unit.rollbackInstruction}`);
    return lines;
  }
  const command = rollbackCommand(unit);
  if (command) {
    lines.push(`  command: ${formatCommand(command)}`);
    if (unit.rollbackKind === "hermes") {
      lines.push(
        "  fallback: helm --namespace " +
          `${unit.rollbackNamespace} rollback ${unit.rollbackRelease} <revision>`,
      );
    }
  }
  return lines;
}

/**
 * Print the plan, then execute the documented command only with `--yes`.
 * `forward-only` exits non-zero; `pulsar` prints the device instruction and
 * never executes it.
 */
export async function runRollback(argv, options = {}) {
  const spawn = options.spawn ?? defaultExec;
  const stdout = options.stdout ?? process.stdout;
  const stderr = options.stderr ?? process.stderr;
  const env = options.env ?? process.env;

  let parsed;
  let unit;
  try {
    parsed = parseRollback(argv);
    unit = findRollbackUnit(units(), parsed.unit);
  } catch (error) {
    stderr.write(`${error.message}\n`);
    return 2;
  }

  stdout.write(`${rollbackPlanLines(unit).join("\n")}\n`);

  if (!unit.rollbackKind) {
    stderr.write(`rollback: ${unit.id} has no documented rollback; fix forward\n`);
    return 1;
  }
  if (unit.rollbackKind === "forward-only") {
    stderr.write(`rollback: ${unit.id} is forward-only\n`);
    return 1;
  }
  if (unit.rollbackKind === "pulsar") {
    stdout.write("rollback: station rollback is manual; nothing was executed\n");
    return 0;
  }
  if (parsed.dryRun) {
    stdout.write("dry-run: nothing executed\n");
    return 0;
  }
  if (!parsed.yes) {
    stderr.write("rollback: pass --yes to execute the command above\n");
    return 1;
  }

  const command = rollbackCommand(unit);
  const result = spawn(command.command, command.args, { env, keepStdio: true });
  return result.status ?? 1;
}

export function statusPlanLines(list, parsed, env = process.env) {
  const lines = [`deploy-home status (last ${parsed.limit} run(s))`];
  if (parsed.unit) {
    const unit = list.find((candidate) => candidate.id === parsed.unit);
    if (!unit) {
      throw new Error(`deploy:status: unknown unit '${parsed.unit}' (see \`mise run units\`)`);
    }
    lines.push("");
    lines.push(`${unit.id} (${unit.title})`);
    if (unit.deployImages.length) {
      lines.push(`  namespace: ${unit.deployNamespace}`);
      lines.push(`  workloads: ${unit.deployWorkloads.join(", ")}`);
      lines.push(`  deploy: ${unit.deployScript}`);
      const kubeconfig = kubeconfigFor(ROOT, env);
      if (kubeconfig) {
        lines.push(`  check: ${formatCommand(statusKubectlCommand(unit, kubeconfig))}`);
      } else {
        lines.push(
          "  check: kubectl command unavailable — restore the admin kubeconfig with " +
            `\`mise run secrets:file ${KUBECONFIG_PATH.replace(/^secrets\//, "")}\``,
        );
        lines.push(
          `         then: ${formatCommand(statusKubectlCommand(unit, `<${KUBECONFIG_PATH}>`))}`,
        );
      }
    } else if (unit.platform === "native" && unit.nativeKind === "pulsar") {
      lines.push("  station release: deploy-station (Environment Station) after an approved deploy-home run");
    } else {
      lines.push("  not deployed by deploy-home");
    }
  }
  return lines;
}

export async function runStatus(argv, options = {}) {
  const spawn = options.spawn ?? defaultExec;
  const stdout = options.stdout ?? process.stdout;
  const stderr = options.stderr ?? process.stderr;
  const env = options.env ?? process.env;
  const list = options.units ?? units();

  let parsed;
  let lines;
  try {
    parsed = parseStatus(argv);
    lines = statusPlanLines(list, parsed, env);
  } catch (error) {
    stderr.write(`${error.message}\n`);
    return 2;
  }
  stdout.write(`${lines.join("\n")}\n`);

  const result = spawn(
    "gh",
    [
      "run",
      "list",
      "--workflow",
      WORKFLOW,
      "--limit",
      String(parsed.limit),
      "--json",
      "databaseId,status,conclusion,createdAt,displayTitle,url",
    ],
    { env },
  );
  if (result.status !== 0) {
    stderr.write((result.stderr || "gh run list failed\n").trimEnd() + "\n");
    return result.status ?? 1;
  }
  let runs = [];
  try {
    runs = JSON.parse(result.stdout || "[]");
  } catch (error) {
    stderr.write(`deploy:status: cannot parse gh output: ${error.message}\n`);
    return 1;
  }
  if (!runs.length) {
    stdout.write("no deploy-home runs\n");
  } else {
    stdout.write("\n");
    for (const run of runs) {
      stdout.write(`${formatRun(run)}\n`);
    }
  }

  if (!parsed.live) {
    return 0;
  }
  const unit = parsed.unit ? list.find((candidate) => candidate.id === parsed.unit) : null;
  if (!unit || !unit.deployImages.length) {
    stderr.write("deploy:status: --live needs a deployed unit\n");
    return 2;
  }
  const kubeconfig = kubeconfigFor(ROOT, env);
  if (!kubeconfig) {
    stderr.write(
      `deploy:status: no kubeconfig for --live; restore ${KUBECONFIG_PATH} with ` +
        `\`mise run secrets:file ${KUBECONFIG_PATH.replace(/^secrets\//, "")}\`\n`,
    );
    return 1;
  }
  const command = statusKubectlCommand(unit, kubeconfig);
  stdout.write(`\n${formatCommand(command)}\n`);
  const live = spawn(command.command, command.args, { env, keepStdio: true });
  return live.status ?? 1;
}

const USAGE = `Usage: ops.mjs <command>

  deploy <unit...|all> [--mode M] [--yes] [--dry-run] [--watch]
                         [--simulate-hermes-failure]
  deploy-status [unit] [--live] [--limit N]
  rollback <unit> [--yes] [--dry-run]
`;

export async function main(argv = process.argv.slice(2)) {
  const [command, ...rest] = argv;
  switch (command) {
    case "deploy":
      return runDeploy(rest);
    case "deploy-status":
      return runStatus(rest);
    case "rollback":
      return runRollback(rest);
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
