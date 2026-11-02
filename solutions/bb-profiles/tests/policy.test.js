import assert from "node:assert/strict";
import test from "node:test";
import {
  describePolicy,
  dispatchDecision,
  expandHome,
  folderPickerAvailable,
  folderPickerError,
  parseHostHomes,
  parseMachineList,
  planMachinesUpdate,
  profileEnv,
  profilePolicy,
  resolveProviderEnv,
  validateMachineIds,
} from "../lib/policy.js";

const HOME = "/home/bb";

test("profileEnv stores paths relative to the provider machine home", () => {
  assert.deepEqual(profileEnv("personal"), {
    "claude-code": { CLAUDE_CONFIG_DIR: "~/.bb-profiles/personal/claude" },
    codex: { CODEX_HOME: "~/.bb-profiles/personal/codex" },
    "*": { GH_CONFIG_DIR: "~/.bb-profiles/personal/gh" },
  });
  assert.equal(
    profileEnv("work", HOME)["claude-code"].CLAUDE_CONFIG_DIR,
    "/home/bb/.bb-profiles/work/claude",
  );
});

test("expandHome expands only a leading tilde", () => {
  assert.equal(expandHome("~/x", HOME), "/home/bb/x");
  assert.equal(expandHome("~", HOME), "/home/bb");
  assert.equal(expandHome("/Users/example/.bb-profiles/x", HOME), "/Users/example/.bb-profiles/x");
  assert.equal(expandHome("~x/y", HOME), "~x/y");
  assert.equal(expandHome("~/x", undefined), "~/x");
});

test("resolveProviderEnv merges '*' with the provider and drops invalid names", () => {
  const profile = {
    "*": { GH_CONFIG_DIR: "~/gh", lower: "invalid" },
    codex: { CODEX_HOME: "~/codex", NUMERIC: 1 },
  };
  assert.deepEqual(resolveProviderEnv(profile, "codex", HOME), {
    GH_CONFIG_DIR: "/home/bb/gh",
    CODEX_HOME: "/home/bb/codex",
  });
  assert.deepEqual(resolveProviderEnv(undefined, "claude-code", HOME), {});
});

test("parseHostHomes accepts an absolute per-host map and ignores junk", () => {
  assert.deepEqual(
    parseHostHomes('{"host_a":"/Users/example/","host_b":"relative","bad id":"/x"}'),
    { host_a: "/Users/example" },
  );
  assert.deepEqual(parseHostHomes("not json"), {});
  assert.deepEqual(parseHostHomes("[]"), {});
  assert.deepEqual(parseHostHomes(undefined), {});
});

test("profilePolicy returns null for an unrestricted profile", () => {
  assert.deepEqual(profilePolicy({ work: ["claude-code"] }, {}, "work"), {
    providers: ["claude-code"],
    machines: null,
  });
  assert.deepEqual(profilePolicy({ work: null }, { work: ["host_a"] }, "work"), {
    providers: null,
    machines: ["host_a"],
  });
  assert.deepEqual(profilePolicy({}, {}, "personal"), {
    providers: null,
    machines: null,
  });
});

test("parseMachineList: all, dedupe, empty and mixed forms", () => {
  assert.deepEqual(parseMachineList("all"), { ok: true, value: null });
  assert.deepEqual(parseMachineList("host_a, host_b,host_a"), {
    ok: true,
    value: ["host_a", "host_b"],
  });
  assert.equal(parseMachineList("").ok, false);
  assert.equal(parseMachineList(" , ").ok, false);
  assert.equal(parseMachineList("all,host_a").ok, false);
  assert.equal(parseMachineList("host a").ok, false);
});

test("validateMachineIds checks against the SDK host list", () => {
  assert.equal(validateMachineIds(["host_a"], ["host_a", "host_b"]), null);
  assert.match(validateMachineIds(["host_c"], ["host_a"]), /Unknown machine: host_c/);
  assert.match(validateMachineIds(["host_c", "host_d"], []), /bb knows no machines/);
  assert.equal(validateMachineIds(["host_c"], null), null);
});

test("planMachinesUpdate validates usage, profile and host ids", async () => {
  const machines = { work: null };
  const listHostIds = async () => ["host_a", "host_b"];

  assert.deepEqual(
    await planMachinesUpdate({ args: ["work"], profileNames: ["work"], machines, listHostIds }),
    { ok: false, error: "Usage: bb profiles machines <profile> <hostId,...|all>" },
  );
  assert.deepEqual(
    await planMachinesUpdate({
      args: ["missing", "host_a"],
      profileNames: ["work"],
      machines,
      listHostIds,
    }),
    { ok: false, error: 'No profile "missing"' },
  );
  const planned = await planMachinesUpdate({
    args: ["work", "host_a,host_b"],
    profileNames: ["work"],
    machines,
    listHostIds,
  });
  assert.equal(planned.ok, true);
  assert.deepEqual(planned.machines, { work: ["host_a", "host_b"] });
  assert.deepEqual(planned.value, ["host_a", "host_b"]);
  assert.equal(planned.warning, null);

  const unknown = await planMachinesUpdate({
    args: ["work", "host_c"],
    profileNames: ["work"],
    machines,
    listHostIds,
  });
  assert.equal(unknown.ok, false);
  assert.match(unknown.error, /Unknown machine: host_c/);

  const cleared = await planMachinesUpdate({
    args: ["work", "all"],
    profileNames: ["work"],
    machines: { work: ["host_a"] },
    listHostIds,
  });
  assert.equal(cleared.ok, true);
  assert.deepEqual(cleared.machines, { work: null });

  const unverified = await planMachinesUpdate({
    args: ["work", "host_z"],
    profileNames: ["work"],
    machines,
    listHostIds: async () => {
      throw new Error("hosts unavailable");
    },
  });
  assert.equal(unverified.ok, true);
  assert.match(unverified.warning, /not verified/);
});

test("dispatchDecision rejects a provider outside the profile allowlist", () => {
  assert.deepEqual(
    dispatchDecision({
      profile: "work",
      providerId: "codex",
      hostId: "host_a",
      allowedProviders: ["claude-code"],
      allowedMachines: null,
    }),
    {
      action: "reject",
      message:
        'Provider "codex" is not allowed in profile "work" (allowed: claude-code). ' +
        "Pick another provider, or move the project to another profile.",
    },
  );
  assert.deepEqual(
    dispatchDecision({
      profile: "work",
      providerId: "claude-code",
      hostId: "host_a",
      allowedProviders: ["claude-code"],
      allowedMachines: null,
    }),
    { action: "proceed" },
  );
});

test("dispatchDecision rejects a machine outside the profile list", () => {
  assert.deepEqual(
    dispatchDecision({
      profile: "work",
      providerId: "codex",
      hostId: "host_b",
      allowedProviders: null,
      allowedMachines: ["host_a"],
    }),
    {
      action: "reject",
      message:
        'Machine "host_b" is not allowed in profile "work" (allowed: host_a). ' +
        "Start the thread on an allowed machine, or update the profile with `bb profiles machines`.",
    },
  );
  assert.deepEqual(
    dispatchDecision({
      profile: "work",
      providerId: "codex",
      hostId: null,
      allowedProviders: null,
      allowedMachines: ["host_a"],
    }),
    {
      action: "reject",
      message:
        'Profile "work" only runs on selected machines (host_a), but this thread has no target machine yet. ' +
        "Start it on an allowed machine, or update the profile with `bb profiles machines`.",
    },
  );
});

test("null lists allow every provider and machine", () => {
  assert.deepEqual(
    dispatchDecision({
      profile: "personal",
      providerId: "opencode",
      hostId: null,
      allowedProviders: null,
      allowedMachines: null,
    }),
    { action: "proceed" },
  );
});

test("folder picker availability follows the server platform", () => {
  assert.equal(folderPickerAvailable("darwin"), true);
  assert.equal(folderPickerAvailable("linux"), false);
  assert.match(folderPickerError(), /Enter the absolute path .* manually/);
});

test("describePolicy renders all and restricted values", () => {
  assert.equal(
    describePolicy({ providers: null, machines: null }),
    "providers: all providers\nmachines: all machines",
  );
  assert.equal(
    describePolicy({ providers: ["claude-code"], machines: ["host_a"] }),
    "providers: claude-code\nmachines: host_a",
  );
});
