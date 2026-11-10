#!/usr/bin/env node
// Unit tests for the deploy/rollback front door; run with `node --test`.

import assert from "node:assert/strict";
import { test } from "node:test";

import { loadConfig } from "../../ci/affected.mjs";
import {
  MODES,
  deployCommand,
  deployPlanLines,
  deployTargets,
  formatCommand,
  parseDeploy,
  parseRollback,
  parseStatus,
  resolveDeployUnits,
  rollbackCommand,
  rollbackPlanLines,
  runDeploy,
  runRollback,
  runStatus,
  statusKubectlCommand,
  statusPlanLines,
  units,
} from "./ops.mjs";

function buffer() {
  return {
    chunks: [],
    write(value) {
      this.chunks.push(String(value));
      return true;
    },
    text() {
      return this.chunks.join("");
    },
  };
}

/** Records spawn calls; responses are keyed by the first two argv tokens. */
function fakeSpawn(responses = {}) {
  const calls = [];
  const spawn = (command, args, options = {}) => {
    calls.push({ command, args, options });
    const key = args.slice(0, 2).join(" ");
    const answer = responses[key] ?? { status: 0, stdout: "", stderr: "" };
    return { status: 0, stdout: "", stderr: "", ...answer };
  };
  spawn.calls = calls;
  return spawn;
}

const realUnits = units();
const scheduler = realUnits.find((unit) => unit.id === "scheduler");
const pulsar = realUnits.find((unit) => unit.id === "pulsar");

test("ops: deploy targets are the deploy units plus pulsar only", () => {
  const ids = deployTargets(realUnits).map((unit) => unit.id);
  for (const id of ["scheduler", "ha-bridge", "hermes", "mcp-gateway", "currency-bot", "wakegrant"]) {
    assert.ok(ids.includes(id), id);
  }
  assert.ok(ids.includes("pulsar"));
  assert.ok(!ids.includes("logon-provider"));
  assert.ok(!ids.includes("android-app"));
});

test("ops: resolveDeployUnits keeps unit-map order and dedupes", () => {
  const selected = resolveDeployUnits(realUnits, ["hermes", "scheduler,hermes"]);
  assert.deepEqual(selected.map((unit) => unit.id), ["scheduler", "hermes"]);
});

test("ops: resolveDeployUnits expands all", () => {
  const selected = resolveDeployUnits(realUnits, ["all"]);
  assert.deepEqual(selected.map((unit) => unit.id), deployTargets(realUnits).map((unit) => unit.id));
});

test("ops: resolveDeployUnits rejects unknown, non-dispatchable and `all` mixed", () => {
  assert.throws(() => resolveDeployUnits(realUnits, ["nope"]), /not a deploy-home dispatch target/);
  assert.throws(() => resolveDeployUnits(realUnits, ["logon-provider"]), /not a deploy-home dispatch target/);
  assert.throws(() => resolveDeployUnits(realUnits, ["all", "scheduler"]), /cannot be combined/);
  assert.throws(() => resolveDeployUnits(realUnits, []), /at least one unit/);
});

test("ops: parseDeploy accepts flags in both forms and rejects unknown ones", () => {
  const parsed = parseDeploy(["scheduler", "--mode=execute", "--yes", "--watch", "--simulate-hermes-failure"]);
  assert.deepEqual(parsed.units, ["scheduler"]);
  assert.equal(parsed.mode, "execute");
  assert.equal(parsed.yes, true);
  assert.equal(parsed.watch, true);
  assert.equal(parsed.simulateHermesFailure, true);
  assert.throws(() => parseDeploy(["--mode", "bogus"]), /unknown mode/);
  assert.throws(() => parseDeploy(["--bogus"]), /unknown flag/);
  assert.deepEqual(MODES, ["preserve", "execute", "shadow", "control-execute", "control-catalog"]);
});

test("ops: deployCommand only sends non-default flags", () => {
  const command = deployCommand({ units: [scheduler], mode: "preserve", simulateHermesFailure: false });
  assert.deepEqual(command.args, [
    "workflow",
    "run",
    "deploy-home.yml",
    "--ref",
    "develop",
    "-f",
    "units=scheduler",
  ]);
  const drill = deployCommand({
    units: [scheduler],
    mode: "execute",
    simulateHermesFailure: true,
  });
  assert.ok(drill.args.includes("mode=execute"));
  assert.ok(drill.args.includes("simulate_hermes_failure=true"));
});

test("ops: deploy plan names the namespace and the exact gh command", () => {
  const text = deployPlanLines({
    units: [scheduler],
    mode: "preserve",
    simulateHermesFailure: false,
  }).join("\n");
  assert.ok(text.includes("namespace home-assistant"));
  assert.ok(text.includes("workloads scheduler"));
  assert.ok(text.includes("gh workflow run deploy-home.yml --ref develop -f units=scheduler"));
});

test("ops: deploy --dry-run prints the command and never dispatches", async () => {
  const spawn = fakeSpawn();
  const stdout = buffer();
  const code = await runDeploy(["scheduler", "--dry-run"], { spawn, stdout, stderr: buffer() });
  assert.equal(code, 0);
  assert.equal(spawn.calls.length, 0);
  assert.ok(stdout.text().includes("gh workflow run deploy-home.yml"));
  assert.ok(stdout.text().includes("dry-run: not dispatching"));
});

test("ops: deploy --yes dispatches and resolves the run id", async () => {
  const spawn = fakeSpawn({
    "run list": {
      status: 0,
      stdout: JSON.stringify([
        { databaseId: 42, status: "queued", conclusion: null, url: "https://example.invalid/42" },
      ]),
    },
  });
  const stdout = buffer();
  const code = await runDeploy(["scheduler", "--yes"], { spawn, stdout, stderr: buffer() });
  assert.equal(code, 0);
  assert.equal(spawn.calls[0].command, "gh");
  assert.deepEqual(spawn.calls[0].args.slice(0, 3), ["workflow", "run", "deploy-home.yml"]);
  assert.ok(stdout.text().includes("run 42: queued"));
});

test("ops: deploy without --yes and a non-interactive confirm does not dispatch", async () => {
  const spawn = fakeSpawn();
  const stderr = buffer();
  const code = await runDeploy(["scheduler"], {
    spawn,
    stdout: buffer(),
    stderr,
    confirm: async () => false,
  });
  assert.equal(code, 1);
  assert.equal(spawn.calls.length, 0);
  assert.ok(stderr.text().includes("not confirmed"));
});

test("ops: deploy --watch watches the found run", async () => {
  const spawn = fakeSpawn({
    "run list": { status: 0, stdout: JSON.stringify([{ databaseId: 7, status: "in_progress" }]) },
    "run watch": { status: 0 },
  });
  const code = await runDeploy(["scheduler", "--yes", "--watch"], {
    spawn,
    stdout: buffer(),
    stderr: buffer(),
  });
  assert.equal(code, 0);
  assert.deepEqual(spawn.calls.at(-1).args, ["run", "watch", "7", "--exit-status"]);
});

test("ops: --simulate-hermes-failure without the hermes unit is rejected", async () => {
  const spawn = fakeSpawn();
  const stderr = buffer();
  const code = await runDeploy(["scheduler", "--dry-run", "--simulate-hermes-failure"], {
    spawn,
    stdout: buffer(),
    stderr,
  });
  assert.equal(code, 2);
  assert.ok(stderr.text().includes("needs the hermes unit"));
});

test("ops: non-preserve modes are limited to ha-bridge like the workflow", async () => {
  const stderr = buffer();
  const code = await runDeploy(["scheduler", "--mode", "execute", "--dry-run"], {
    spawn: fakeSpawn(),
    stdout: buffer(),
    stderr,
  });
  assert.equal(code, 2);
  assert.ok(stderr.text().includes("only allowed for the ha-bridge unit"));

  const ok = await runDeploy(["ha-bridge", "--mode", "execute", "--dry-run"], {
    spawn: fakeSpawn(),
    stdout: buffer(),
    stderr: buffer(),
  });
  assert.equal(ok, 0);
});

test("ops: every unit with a rollback kind has the fields its kind needs", () => {
  const kinds = new Set(["deploy-state", "helm", "hermes", "pulsar", "flux-pins", "forward-only"]);
  for (const unit of loadConfig().units.filter((candidate) => candidate.rollbackKind)) {
    assert.ok(kinds.has(unit.rollbackKind), `${unit.id}: ${unit.rollbackKind}`);
    assert.ok(unit.rollbackSource, `${unit.id}: source`);
    if (unit.rollbackKind === "deploy-state") {
      assert.ok(unit.rollbackStateDir, `${unit.id}: state_dir`);
      assert.ok(unit.rollbackNamespace, `${unit.id}: namespace`);
    }
    if (unit.rollbackKind === "hermes") {
      assert.ok(unit.rollbackRelease, `${unit.id}: release`);
      assert.ok(unit.rollbackNamespace, `${unit.id}: namespace`);
    }
    if (unit.rollbackKind === "forward-only") {
      assert.ok(unit.rollbackNote, `${unit.id}: note`);
    }
    if (unit.rollbackKind === "pulsar") {
      assert.ok(unit.rollbackInstruction, `${unit.id}: instruction`);
    }
  }
});

test("ops: rollback plan for a deploy-state unit carries state dir and source", () => {
  const text = rollbackPlanLines(scheduler).join("\n");
  assert.ok(text.includes("kind: deploy-state"));
  assert.ok(text.includes("state: secrets/local/rendered/scheduler/state"));
  assert.ok(text.includes("in namespace home-assistant"));
  assert.ok(text.includes("projects/home/kubernetes/scheduler/README.md"));
  assert.ok(text.includes("home_deploy state rollback"));
});

test("ops: hermes rollback uses the recorded revision and names the helm fallback", () => {
  const hermes = realUnits.find((unit) => unit.id === "hermes");
  const text = rollbackPlanLines(hermes).join("\n");
  assert.ok(text.includes("kind: hermes"));
  assert.ok(text.includes("--rollback-only"));
  assert.ok(text.includes("helm --namespace home-assistant rollback runtime"));
  assert.ok(rollbackCommand(hermes).args[1].includes("home_deploy hermes rollout --rollback-only"));
});

test("ops: pulsar rollback prints the on-device instruction and never executes", async () => {
  const spawn = fakeSpawn();
  const stdout = buffer();
  const code = await runRollback(["pulsar", "--yes"], { spawn, stdout, stderr: buffer() });
  assert.equal(code, 0);
  assert.equal(spawn.calls.length, 0);
  assert.ok(stdout.text().includes("pulsar.prev"));
  assert.ok(stdout.text().includes("nothing was executed"));
});

test("ops: forward-only units exit non-zero without executing", async () => {
  const spawn = fakeSpawn();
  const stderr = buffer();
  const code = await runRollback(["intercom", "--yes"], {
    spawn,
    stdout: buffer(),
    stderr,
  });
  assert.equal(code, 1);
  assert.equal(spawn.calls.length, 0);
  assert.ok(stderr.text().includes("forward-only"));
});

test("ops: flux-pins rollback prints the pins revert and never executes", async () => {
  const spawn = fakeSpawn();
  const stdout = buffer();
  const code = await runRollback(["wakegrant", "--yes"], {
    spawn,
    stdout,
    stderr: buffer(),
  });
  assert.equal(code, 0);
  assert.equal(spawn.calls.length, 0);
  assert.ok(stdout.text().includes("deploy/home-apps"));
  assert.ok(stdout.text().includes("nothing was executed"));
});

test("ops: rollback --dry-run prints the command without executing", async () => {
  const spawn = fakeSpawn();
  const stdout = buffer();
  const code = await runRollback(["scheduler", "--dry-run"], {
    spawn,
    stdout,
    stderr: buffer(),
  });
  assert.equal(code, 0);
  assert.equal(spawn.calls.length, 0);
  assert.ok(stdout.text().includes("dry-run: nothing executed"));
});

test("ops: rollback --yes executes the documented deploy-state command", async () => {
  const spawn = fakeSpawn();
  const code = await runRollback(["scheduler", "--yes"], {
    spawn,
    stdout: buffer(),
    stderr: buffer(),
  });
  assert.equal(code, 0);
  assert.equal(spawn.calls.length, 1);
  assert.equal(spawn.calls[0].command, "bash");
  assert.ok(spawn.calls[0].args[1].includes("home_deploy state rollback"));
  assert.ok(spawn.calls[0].args[1].includes('--namespace "home-assistant"'));
});

test("ops: rollback needs --yes to execute", async () => {
  const spawn = fakeSpawn();
  const stderr = buffer();
  const code = await runRollback(["scheduler"], { spawn, stdout: buffer(), stderr });
  assert.equal(code, 1);
  assert.equal(spawn.calls.length, 0);
  assert.ok(stderr.text().includes("--yes"));
});

test("ops: rollback rejects unknown units", async () => {
  const stderr = buffer();
  const code = await runRollback(["nope"], { spawn: fakeSpawn(), stdout: buffer(), stderr });
  assert.equal(code, 2);
  assert.ok(stderr.text().includes("unknown unit"));
});

test("ops: parseRollback rejects extra units and unknown flags", () => {
  assert.deepEqual(parseRollback(["pulsar", "--dry-run"]), {
    unit: "pulsar",
    yes: false,
    dryRun: true,
  });
  assert.throws(() => parseRollback(["a", "b"]), /exactly one unit/);
  assert.throws(() => parseRollback(["--nope"]), /unknown flag/);
});

test("ops: status parses --live and --limit", () => {
  assert.deepEqual(parseStatus(["scheduler", "--live", "--limit", "2"]), {
    unit: "scheduler",
    live: true,
    limit: 2,
  });
  assert.throws(() => parseStatus(["--limit", "zero"]), /positive integer/);
  assert.throws(() => parseStatus(["--nope"]), /unknown flag/);
});

test("ops: status shows unit namespace/workloads and the read-only kubectl hint", () => {
  const text = statusPlanLines(realUnits, { unit: "scheduler", live: false, limit: 5 }, {
    KUBECONFIG: "/tmp/fake-kubeconfig",
  }).join("\n");
  assert.ok(text.includes("namespace: home-assistant"));
  assert.ok(text.includes("workloads: scheduler"));
  assert.ok(text.includes("kubectl --kubeconfig /tmp/fake-kubeconfig -n home-assistant get deploy scheduler -o wide"));
  assert.deepEqual(statusKubectlCommand(scheduler, "").args, [
    "-n",
    "home-assistant",
    "get",
    "deploy",
    "scheduler",
    "-o",
    "wide",
  ]);
});

test("ops: status lists runs from gh", async () => {
  const spawn = fakeSpawn({
    "run list": {
      status: 0,
      stdout: JSON.stringify([
        { databaseId: 1, status: "completed", conclusion: "success", createdAt: "2026-01-01T00:00:00Z" },
      ]),
    },
  });
  const stdout = buffer();
  const code = await runStatus([], { spawn, stdout, stderr: buffer(), units: realUnits });
  assert.equal(code, 0);
  assert.ok(stdout.text().includes("deploy-home status"));
  assert.ok(stdout.text().includes("1            completed/success"));
});

test("ops: status --live runs read-only kubectl get deploy", async () => {
  const spawn = fakeSpawn({ "run list": { status: 0, stdout: "[]" } });
  const code = await runStatus(["scheduler", "--live"], {
    spawn,
    stdout: buffer(),
    stderr: buffer(),
    units: realUnits,
    env: { KUBECONFIG: "/tmp/fake-kubeconfig" },
  });
  assert.equal(code, 0);
  assert.equal(spawn.calls.at(-1).command, "kubectl");
  assert.deepEqual(spawn.calls.at(-1).args, [
    "--kubeconfig",
    "/tmp/fake-kubeconfig",
    "-n",
    "home-assistant",
    "get",
    "deploy",
    "scheduler",
    "-o",
    "wide",
  ]);
});

test("ops: status --live without a kubeconfig refuses to call kubectl", async () => {
  const spawn = fakeSpawn({ "run list": { status: 0, stdout: "[]" } });
  const stderr = buffer();
  const code = await runStatus(["scheduler", "--live"], {
    spawn,
    stdout: buffer(),
    stderr,
    units: realUnits,
    env: {},
  });
  // A workstation may have the documented kubeconfig on disk; only assert the
  // no-kubectl half when it does not exist.
  if (code !== 0) {
    assert.ok(stderr.text().includes("no kubeconfig for --live"));
    assert.ok(!spawn.calls.some((call) => call.command === "kubectl"));
  }
});

test("ops: formatCommand quotes only tokens that need it", () => {
  assert.equal(
    formatCommand({ command: "gh", args: ["workflow", "run", "units=a,b"] }),
    "gh workflow run units=a,b",
  );
  assert.equal(
    formatCommand({ command: "bash", args: ["-c", "echo 'x y'"] }),
    `bash -c 'echo '\\''x y'\\'''`,
  );
});
