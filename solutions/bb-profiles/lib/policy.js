// Pure profile policy: no bb SDK, no npm dependencies.
//
// The server entry imports these helpers and the CI tests import the same
// functions directly with `node --test`, so policy changes stay covered
// without installing node_modules or starting a bb server.

/** Profile names: lowercase, digits, '-' and '_', at most 32 characters. */
export const PROFILE_NAME = /^[a-z0-9][a-z0-9_-]{0,31}$/;
/** Environment variable names a profile may set. */
export const ENV_NAME = /^[A-Z_][A-Z0-9_]*$/;
/** Providers that exist without any profile declaring them. */
export const KNOWN_PROVIDERS = [
  "claude-code",
  "codex",
  "pi",
  "acp-cursor",
  "opencode",
];
/** Directory under a machine's home that holds per-profile provider state. */
export const PROFILES_DIRNAME = ".bb-profiles";
/**
 * Machine ids are opaque bb host ids. They cross a comma-separated CLI list,
 * so the only hard rules are "not empty, no comma, no whitespace".
 */
const HOST_ID = /^[^\s,]+$/;

function trimSlash(value) {
  return value.length > 1 && value.endsWith("/") ? value.slice(0, -1) : value;
}

/**
 * Default env for a new profile, expressed relative to the home directory of
 * the machine that runs the provider. A `~` prefix is deliberate: the server
 * may run on another machine, so it must not bake its own home directory into
 * newly created profiles. `expandHome` resolves it when the provider starts.
 *
 * @param {string} name
 * @param {string} [home] absolute home or "~" for a relative template
 * @returns {Record<string, Record<string, string>>}
 */
export function profileEnv(name, home = "~") {
  const root = `${trimSlash(home)}/${PROFILES_DIRNAME}/${name}`;
  return {
    "claude-code": { CLAUDE_CONFIG_DIR: `${root}/claude` },
    codex: { CODEX_HOME: `${root}/codex` },
    "*": { GH_CONFIG_DIR: `${root}/gh` },
  };
}

/**
 * Expand a leading `~` using the target machine's home. Absolute paths (the
 * values stored before the split-server change) are returned unchanged.
 *
 * @param {string} value
 * @param {string | null | undefined} home
 * @returns {string}
 */
export function expandHome(value, home) {
  if (home == null || home === "") return value;
  const root = trimSlash(home);
  if (value === "~") return root;
  if (value.startsWith("~/")) return `${root}${value.slice(1)}`;
  return value;
}

/**
 * Merge a profile's `*` entries with one provider's entries, expand `~`, and
 * keep only valid env names. Returns a plain object so the server can create
 * missing directories before it turns them into env entries.
 *
 * @param {Record<string, Record<string, string>> | undefined} profile
 * @param {string} providerId
 * @param {string | null | undefined} home
 * @returns {Record<string, string>}
 */
export function resolveProviderEnv(profile, providerId, home) {
  const merged = { ...(profile?.["*"] ?? {}), ...(profile?.[providerId] ?? {}) };
  /** @type {Record<string, string>} */
  const result = {};
  for (const [key, value] of Object.entries(merged)) {
    if (ENV_NAME.test(key) && typeof value === "string") {
      result[key] = expandHome(value, home);
    }
  }
  return result;
}

/**
 * Parse the `hostHomes` setting: `{"<hostId>": "/absolute/home"}`.
 * Invalid JSON or entries are ignored, so a typo cannot break provider starts.
 *
 * @param {unknown} raw
 * @returns {Record<string, string>}
 */
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
  /** @type {Record<string, string>} */
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

/**
 * Effective policy for one profile. A missing entry (or an explicit null)
 * means "everything is allowed"; the read RPCs mirror this shape.
 *
 * @param {Record<string, string[] | null>} allowed
 * @param {Record<string, string[] | null>} machines
 * @param {string} profile
 * @returns {{ providers: string[] | null, machines: string[] | null }}
 */
export function profilePolicy(allowed, machines, profile) {
  return {
    providers: allowed[profile] ?? null,
    machines: machines[profile] ?? null,
  };
}

/**
 * Parse `bb profiles machines <profile> <hostId,...|all>`.
 * `all` means no restriction and is stored as null.
 *
 * @param {string} input
 * @returns {{ ok: true, value: string[] | null } | { ok: false, error: string }}
 */
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

/**
 * Check host ids against the machines bb knows about. `knownHosts === null`
 * means the SDK could not list machines: the update proceeds unverified.
 *
 * @param {readonly string[]} ids
 * @param {readonly string[] | null} knownHosts
 * @returns {string | null} error message, or null when valid
 */
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

/**
 * Plan a `bb profiles machines` update without writing kv. The server calls
 * this with the real kv and host list; tests call it with fakes.
 *
 * @param {{
 *   args: readonly string[],
 *   profileNames: readonly string[],
 *   machines: Record<string, string[] | null>,
 *   listHostIds: () => Promise<readonly string[] | null>,
 * }} input
 * @returns {Promise<
 *   | { ok: true, machines: Record<string, string[] | null>, value: string[] | null, warning: string | null }
 *   | { ok: false, error: string }
 * >}
 */
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

/**
 * The `message.dispatch` verdict for provider and machine allowlists.
 * A restricted machine list rejects a dispatch without a resolved host,
 * because there is nothing to verify against.
 *
 * @param {{
 *   profile: string,
 *   providerId: string,
 *   hostId: string | null,
 *   allowedProviders: readonly string[] | null,
 *   allowedMachines: readonly string[] | null,
 * }} input
 * @returns {{ action: "proceed" } | { action: "reject", message: string }}
 */
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

/** Whether this server can show the native macOS folder dialog. */
export function folderPickerAvailable(platform) {
  return platform === "darwin";
}

/** Clear refusal text for servers without a native folder dialog. */
export function folderPickerError() {
  return (
    "The folder picker is only available when the bb server runs on macOS " +
    "with a desktop session. Enter the absolute path on the provider machine manually."
  );
}

/**
 * Human-readable policy block for `bb profiles show`.
 *
 * @param {{ providers: readonly string[] | null, machines: readonly string[] | null }} policy
 * @returns {string}
 */
export function describePolicy(policy) {
  const providers = policy.providers === null ? "all providers" : policy.providers.join(", ");
  const machines = policy.machines === null ? "all machines" : policy.machines.join(", ");
  return `providers: ${providers}\nmachines: ${machines}`;
}
