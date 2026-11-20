export const PROFILE_NAME = /^[a-z0-9][a-z0-9_-]{0,31}$/;
export const ENV_NAME = /^[A-Z_][A-Z0-9_]*$/;
export const KNOWN_PROVIDERS = [
  "claude-code",
  "codex",
  "pi",
  "acp-cursor",
  "opencode",
];
export const PROFILES_DIRNAME = ".bb-profiles";
const HOST_ID = /^[^\s,]+$/;

function trimSlash(value) {
  return value.length > 1 && value.endsWith("/") ? value.slice(0, -1) : value;
}

export function profileEnv(name, home = "~") {
  const root = `${trimSlash(home)}/${PROFILES_DIRNAME}/${name}`;
  return {
    "claude-code": { CLAUDE_CONFIG_DIR: `${root}/claude` },
    codex: { CODEX_HOME: `${root}/codex` },
    "*": { GH_CONFIG_DIR: `${root}/gh` },
  };
}

export function expandHome(value, home) {
  if (home == null || home === "") return value;
  const root = trimSlash(home);
  if (value === "~") return root;
  if (value.startsWith("~/")) return `${root}${value.slice(1)}`;
  return value;
}

export function resolveProviderEnv(profile, providerId, home) {
  const merged = { ...(profile?.["*"] ?? {}), ...(profile?.[providerId] ?? {}) };
  const result = {};
  for (const [key, value] of Object.entries(merged)) {
    if (ENV_NAME.test(key) && typeof value === "string") {
      result[key] = expandHome(value, home);
    }
  }
  return result;
}

export function parseHostHomes(raw) {
  if (typeof raw !== "string" || raw.trim() === "") return {};
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return {};
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    return {};
  }
  const result = {};
  for (const [hostId, home] of Object.entries(parsed)) {
    if (
      HOST_ID.test(hostId) &&
      typeof home === "string" &&
      home.trim().startsWith("/")
    ) {
      result[hostId] = trimSlash(home.trim());
    }
  }
  return result;
}

export function profilePolicy(allowed, machines, profile) {
  return {
    providers: allowed[profile] ?? null,
    machines: machines[profile] ?? null,
  };
}

export function parseMachineList(input) {
  const trimmed = input.trim();
  if (trimmed === "all") return { ok: true, value: null };
  const ids = [...new Set(trimmed.split(",").map((id) => id.trim()).filter(Boolean))];
  if (ids.length === 0) {
    return { ok: false, error: "Give at least one host id, or `all` to remove the limit" };
  }
  if (ids.includes("all")) {
    return { ok: false, error: "Use either `all` or a host id list, not both" };
  }
  for (const id of ids) {
    if (!HOST_ID.test(id)) {
      return { ok: false, error: `Invalid host id "${id}"` };
    }
  }
  return { ok: true, value: ids };
}

export function validateMachineIds(ids, knownHosts) {
  if (knownHosts === null) return null;
  const known = new Set(knownHosts);
  const unknown = ids.filter((id) => !known.has(id));
  if (unknown.length === 0) return null;
  const available = knownHosts.length === 0
    ? "bb knows no machines"
    : `known machines: ${knownHosts.join(", ")}`;
  return `Unknown machine${unknown.length === 1 ? "" : "s"}: ${unknown.join(", ")} (${available})`;
}

export async function planMachinesUpdate({ args, profileNames, machines, listHostIds }) {
  if (args.length !== 2) {
    return { ok: false, error: "Usage: bb profiles machines <profile> <hostId,...|all>" };
  }
  const [name, list] = args;
  if (!profileNames.includes(name)) {
    return { ok: false, error: `No profile "${name}"` };
  }
  const parsed = parseMachineList(list);
  if (!parsed.ok) return parsed;

  let warning = null;
  if (parsed.value !== null) {
    let knownHosts = null;
    try {
      knownHosts = await listHostIds();
    } catch {
      warning = "could not list machines; host ids were not verified";
    }
    const error = validateMachineIds(parsed.value, knownHosts);
    if (error !== null) return { ok: false, error };
  }
  return {
    ok: true,
    machines: { ...machines, [name]: parsed.value },
    value: parsed.value,
    warning,
  };
}

export function dispatchDecision({
  profile,
  providerId,
  hostId,
  allowedProviders,
  allowedMachines,
}) {
  if (allowedProviders !== null && !allowedProviders.includes(providerId)) {
    return {
      action: "reject",
      message:
        `Provider "${providerId}" is not allowed in profile "${profile}" ` +
        `(allowed: ${allowedProviders.join(", ")}). ` +
        "Pick another provider, or move the project to another profile.",
    };
  }
  if (allowedMachines !== null) {
    if (hostId === null) {
      return {
        action: "reject",
        message:
          `Profile "${profile}" only runs on selected machines ` +
          `(${allowedMachines.join(", ")}), but this thread has no target machine yet. ` +
          "Start it on an allowed machine, or update the profile with `bb profiles machines`.",
      };
    }
    if (!allowedMachines.includes(hostId)) {
      return {
        action: "reject",
        message:
          `Machine "${hostId}" is not allowed in profile "${profile}" ` +
          `(allowed: ${allowedMachines.join(", ")}). ` +
          "Start the thread on an allowed machine, or update the profile with `bb profiles machines`.",
      };
    }
  }
  return { action: "proceed" };
}

export function folderPickerAvailable(platform) {
  return platform === "darwin";
}

export function folderPickerError() {
  return (
    "The folder picker is only available when the bb server runs on macOS " +
    "with a desktop session. Enter the absolute path on the provider machine manually."
  );
}

export function describePolicy(policy) {
  const providers = policy.providers === null ? "all providers" : policy.providers.join(", ");
  const machines = policy.machines === null ? "all machines" : policy.machines.join(", ");
  return `providers: ${providers}\nmachines: ${machines}`;
}
