// bb-plugin-profiles — isolated work/personal profiles for bb.
//
// A profile is a named set of environment variables per provider (for example
// CLAUDE_CONFIG_DIR for Claude Code). Every project is assigned to one profile
// (default: "work"); bb.providers.experimental_contributeEnv applies the
// project's profile to each provider command, so logins, memory, MCP servers
// and history never mix between profiles.
import { execFile } from "node:child_process";
import { mkdir } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";
import {
  defineRpcContract,
  experimental_PluginRpcRefusal,
  type BbPluginApi,
} from "@get-bb/plugin-sdk";
import { z } from "zod";
import {
  describePolicy,
  dispatchDecision,
  ENV_NAME,
  folderPickerAvailable,
  folderPickerError,
  KNOWN_PROVIDERS,
  parseHostHomes,
  planMachinesUpdate,
  PROFILE_NAME,
  PROFILES_DIRNAME,
  profileEnv,
  profilePolicy as resolvePolicy,
  resolveProviderEnv,
} from "./lib/policy.js";

/** Env vars per provider id; "*" applies to every provider of the profile. */
export type ProfileEnv = Record<string, Record<string, string>>;
export type Profiles = Record<string, ProfileEnv>;

const CHANGED = "profiles-changed";

const projectRow = z.object({
  id: z.string(),
  name: z.string(),
  profile: z.string(),
});
export type ProjectRow = z.infer<typeof projectRow>;

export const rpcContract = defineRpcContract({
  state: {
    input: z.null(),
    output: z.object({
      profiles: z.array(z.string()),
      defaultProfile: z.string(),
      active: z.string(),
      folderPicker: z.boolean(),
    }),
  },
  // The profile whose projects the sidebar shows; shared by every window.
  setActive: {
    input: z.object({ name: z.string() }),
    output: z.object({ active: z.string() }),
  },
  rememberThread: {
    input: z.object({ projectId: z.string(), threadId: z.string() }),
    output: z.object({ ok: z.boolean() }),
  },
  lastThread: {
    input: z.object({ profile: z.string() }),
    output: z.object({ threadId: z.string().nullable() }),
  },
  projectProfile: {
    input: z.object({ projectId: z.string() }),
    output: z.object({ profile: z.string() }),
  },
  setProjectProfile: {
    input: z.object({ projectId: z.string(), profile: z.string() }),
    output: z.object({ profile: z.string() }),
  },
  listProjects: {
    input: z.null(),
    output: z.object({ projects: z.array(projectRow) }),
  },
  createProfile: {
    input: z.object({ name: z.string() }),
    output: z.object({ profiles: z.array(z.string()) }),
  },
  addProject: {
    input: z.object({ name: z.string(), path: z.string(), profile: z.string() }),
    output: z.object({ projectId: z.string() }),
  },
  // Native Finder folder dialog on the machine the server runs on; path is
  // null when the user cancels. Non-macOS servers refuse with a clear message
  // and the app falls back to manual path entry.
  pickFolder: {
    input: z.null(),
    output: z.object({ path: z.string().nullable() }),
  },
  // Read-only policy views for other plugins (for example a dispatcher).
  // A null list means "no restriction".
  allowedProviders: {
    input: z.object({ profile: z.string() }),
    output: z.array(z.string()).nullable(),
  },
  allowedMachines: {
    input: z.object({ profile: z.string() }),
    output: z.array(z.string()).nullable(),
  },
  profilePolicy: {
    input: z.object({ profile: z.string() }),
    output: z.object({
      providers: z.array(z.string()).nullable(),
      machines: z.array(z.string()).nullable(),
    }),
  },
});

const PICK_TIMEOUT_MS = 5 * 60_000;
function pickFolder(): Promise<string | null> {
  if (!folderPickerAvailable(process.platform)) {
    return Promise.reject(new Error(folderPickerError()));
  }
  const script =
    'POSIX path of (choose folder with prompt "Choose the project folder")';
  return new Promise((resolve, reject) => {
    execFile(
      "osascript",
      ["-e", script],
      { timeout: PICK_TIMEOUT_MS },
      (error, stdout, stderr) => {
        if (error === null) {
          const path = stdout.trim().replace(/\/$/, "");
          resolve(path === "" ? null : path);
        } else if (stderr.includes("-128")) {
          resolve(null); // user cancelled
        } else {
          reject(new Error(stderr.trim() || error.message));
        }
      },
    );
  });
}

function defaultProfiles(): Profiles {
  // "work" keeps the existing ~/.claude, ~/.codex, ...: no overrides.
  // "personal" shares ~/.codex (existing Codex login, history, memory):
  // Codex is only allowed there, so there is nothing to isolate it from.
  const { codex: _codex, ...personal } = profileEnv("personal");
  return { work: {}, personal };
}

export default async function plugin(bb: BbPluginApi) {
  const settings = bb.settings.define({
    defaultProfile: {
      type: "string",
      label: "Default profile",
      default: "work",
    },
    hostHomes: {
      type: "string",
      label: "Provider machine homes (JSON: {\"<hostId>\":\"/home/user\"})",
      description:
        "Home directory of each machine that runs providers. New profiles store " +
        "~/.bb-profiles paths; this map expands them for remote hosts. Empty uses " +
        "the bb server's own home.",
      default: "",
    },
  });

  async function readProfiles(): Promise<Profiles> {
    return (await bb.storage.kv.get<Profiles>("profiles")) ?? defaultProfiles();
  }
  // Providers a profile may run; a profile without an entry allows all.
  async function readAllowed(): Promise<Record<string, string[] | null>> {
    return (
      (await bb.storage.kv.get<Record<string, string[] | null>>("allowed")) ?? {
        work: ["claude-code"],
      }
    );
  }
  // Machines a profile may run on; a profile without an entry allows all.
  async function readMachines(): Promise<Record<string, string[] | null>> {
    return (await bb.storage.kv.get<Record<string, string[] | null>>("machines")) ?? {};
  }
  async function readAssignments(): Promise<Record<string, string>> {
    return (await bb.storage.kv.get<Record<string, string>>("assignments")) ?? {};
  }
  // The provider runs on `hostId`, which may be another machine. Only an
  // explicit hostHomes entry knows its home; the server's home is the safe
  // default for same-machine installs. Existing absolute profile values never
  // consult this.
  async function profileHome(hostId: string): Promise<string> {
    const { hostHomes } = await settings.get();
    return parseHostHomes(hostHomes)[hostId] ?? homedir();
  }
  async function policyFor(profile: string) {
    if (!(profile in (await readProfiles()))) {
      refuse("not_found", `No profile named "${profile}"`);
    }
    return resolvePolicy(await readAllowed(), await readMachines(), profile);
  }
  async function defaultProfile(): Promise<string> {
    const { defaultProfile: name } = await settings.get();
    return name;
  }
  async function profileFor(projectId: string | null): Promise<string> {
    const profiles = await readProfiles();
    const fallback = await defaultProfile();
    if (projectId === null) return fallback;
    const assigned = (await readAssignments())[projectId];
    return assigned !== undefined && assigned in profiles ? assigned : fallback;
  }
  function refuse(code: "not_found" | "invalid", message: string): never {
    throw new experimental_PluginRpcRefusal({ code, message });
  }

  async function activeProfile(): Promise<string> {
    const stored = await bb.storage.kv.get<string>("active");
    const profiles = await readProfiles();
    return stored !== undefined && stored in profiles
      ? stored
      : await defaultProfile();
  }

  // The built-in thread list shows every project, so projects of the other
  // profiles are hidden through its `hiddenGroups` preference. Only the groups
  // this plugin hid earlier are replaced; the user's own hidden groups stay.
  async function applyHiddenProjects(): Promise<void> {
    const active = await activeProfile();
    const projects = await bb.sdk.projects.list({ includePersonal: true });
    const hidden: string[] = [];
    for (const project of projects) {
      if ((await profileFor(project.id)) !== active) hidden.push(`project:${project.id}`);
    }
    const managed = new Set(
      (await bb.storage.kv.get<string[]>("hiddenManaged")) ?? [],
    );
    // The built-in thread list keeps its own preferences; `bb settings ui` is
    // only a one-time import for it, so write through its rpc.
    const { preferences } = await bb.sdk.plugins.callRpc({
      pluginId: "thread-list",
      method: "listPreferences",
      outputSchema: z.object({ preferences: z.record(z.string(), z.unknown()) }),
    });
    const current = Array.isArray(preferences.hiddenGroups)
      ? preferences.hiddenGroups.filter((key): key is string => typeof key === "string")
      : [];
    const kept = current.filter((key) => !managed.has(key));
    await bb.sdk.plugins.callRpc({
      pluginId: "thread-list",
      method: "setPreference",
      input: { key: "hiddenGroups", value: [...kept, ...hidden] },
      outputSchema: z.unknown(),
    });
    await bb.storage.kv.set("hiddenManaged", hidden);
  }
  async function safeApplyHidden(): Promise<void> {
    try {
      await applyHiddenProjects();
    } catch (cause) {
      console.warn("profiles: could not hide other profiles' projects", cause);
    }
  }

  async function assign(projectId: string, profile: string): Promise<string> {
    if (!(profile in (await readProfiles()))) {
      refuse("not_found", `No profile named "${profile}"`);
    }
    const assignments = await readAssignments();
    assignments[projectId] = profile;
    await bb.storage.kv.set("assignments", assignments);
    await safeApplyHidden();
    bb.realtime.publish(CHANGED, { projectId, profile });
    return profile;
  }
  async function createProfile(name: string): Promise<string[]> {
    if (!PROFILE_NAME.test(name)) {
      refuse("invalid", "Profile names use a-z, 0-9, '-' and '_' (max 32)");
    }
    const profiles = await readProfiles();
    if (name in profiles) refuse("invalid", `Profile "${name}" already exists`);
    profiles[name] = profileEnv(name);
    await bb.storage.kv.set("profiles", profiles);
    bb.realtime.publish(CHANGED, { profile: name });
    return Object.keys(profiles);
  }
  // The new project lives on the host the existing local projects use.
  async function addProject(
    name: string,
    path: string,
    profile: string,
  ): Promise<string> {
    if (name.trim() === "" || path.trim() === "") {
      refuse("invalid", "Name and folder are required");
    }
    if (!(profile in (await readProfiles()))) {
      refuse("not_found", `No profile named "${profile}"`);
    }
    const existing = await bb.sdk.projects.list({ includePersonal: true });
    const hostId = existing
      .flatMap((project) => project.sources ?? [])
      .find((source) => source.type === "local_path")?.hostId;
    if (hostId === undefined) {
      refuse("invalid", "No local host found; add a project with `bb project create` first");
    }
    const project = await bb.sdk.projects.create({
      name: name.trim(),
      source: { type: "local_path", hostId, path: path.trim() },
    });
    await assign(project.id, profile);
    return project.id;
  }
  async function projectRows(): Promise<ProjectRow[]> {
    const projects = await bb.sdk.projects.list({ includePersonal: true });
    const rows: ProjectRow[] = [];
    for (const project of projects) {
      rows.push({
        id: project.id,
        name: project.name,
        profile: await profileFor(project.id),
      });
    }
    return rows;
  }

  // Apply the project's profile to every provider command. Values are only
  // paths (they show up in diagnostics), never secrets. `context.hostId`
  // names the machine that runs the provider; its home comes from the
  // hostHomes setting (or the server's own home on an all-in-one install).
  const profilesRoot = join(homedir(), PROFILES_DIRNAME);
  const providerIds = new Set([
    ...KNOWN_PROVIDERS,
    ...Object.values(await readProfiles()).flatMap((env) => Object.keys(env)),
  ]);
  providerIds.delete("*");
  for (const providerId of providerIds) {
    bb.providers.experimental_contributeEnv(providerId, async (context) => {
      const name = await profileFor(context.projectId);
      const profile = (await readProfiles())[name] ?? {};
      const home = await profileHome(context.hostId);
      const merged = resolveProviderEnv(profile, providerId, home);
      const entries = Object.entries(merged).slice(0, 32);
      for (const [, value] of entries) {
        // Config directories must exist before the provider starts. Only a
        // path on this server can be created here; a remote machine's home
        // is created by its own provider or by the operator.
        if (value.startsWith(profilesRoot)) {
          await mkdir(value, { recursive: true });
        }
      }
      return entries.map(([key, value]) => ({
        name: key,
        value,
        reason: `Profile "${name}" isolates ${providerId}`,
      }));
    });
  }

  // Hard isolation: the provider picker cannot be filtered by plugins, so a
  // message to a provider or machine the project's profile does not allow is
  // rejected. `ctx.host` is resolved from the thread's environment, or from
  // its start intent before the environment exists.
  bb.experimental_hooks.on("message.dispatch", async (ctx) => {
    const name = await profileFor(ctx.project.id);
    // Resolve the raw maps: a hook must not throw an RPC refusal, and a
    // profile missing from kv simply means "no restriction".
    const policy = resolvePolicy(await readAllowed(), await readMachines(), name);
    return dispatchDecision({
      profile: name,
      providerId: ctx.thread.providerId,
      hostId: ctx.host?.id ?? null,
      allowedProviders: policy.providers,
      allowedMachines: policy.machines,
    });
  });

  bb.rpc.register(rpcContract, {
    state: async () => ({
      profiles: Object.keys(await readProfiles()),
      defaultProfile: await defaultProfile(),
      active: await activeProfile(),
      folderPicker: folderPickerAvailable(process.platform),
    }),
    setActive: async ({ name }) => {
      if (!(name in (await readProfiles()))) {
        refuse("not_found", `No profile named "${name}"`);
      }
      await bb.storage.kv.set("active", name);
      await applyHiddenProjects();
      bb.realtime.publish(CHANGED, { active: name });
      return { active: name };
    },
    rememberThread: async ({ projectId, threadId }) => {
      const last =
        (await bb.storage.kv.get<Record<string, string>>("lastThreads")) ?? {};
      const profile = await profileFor(projectId);
      if (last[profile] !== threadId) {
        last[profile] = threadId;
        await bb.storage.kv.set("lastThreads", last);
      }
      return { ok: true };
    },
    lastThread: async ({ profile }) => ({
      threadId:
        ((await bb.storage.kv.get<Record<string, string>>("lastThreads")) ?? {})[
          profile
        ] ?? null,
    }),
    projectProfile: async ({ projectId }) => ({
      profile: await profileFor(projectId),
    }),
    setProjectProfile: async ({ projectId, profile }) => ({
      profile: await assign(projectId, profile),
    }),
    listProjects: async () => ({ projects: await projectRows() }),
    createProfile: async ({ name }) => ({ profiles: await createProfile(name) }),
    addProject: async ({ name, path, profile }) => ({
      projectId: await addProject(name, path, profile),
    }),
    pickFolder: async () => {
      try {
        return { path: await pickFolder() };
      } catch (cause) {
        const message = cause instanceof Error ? cause.message : String(cause);
        refuse("invalid", message.startsWith("The folder picker") ? message : `Folder dialog failed: ${message}`);
      }
    },
    allowedProviders: async ({ profile }) => (await policyFor(profile)).providers,
    allowedMachines: async ({ profile }) => (await policyFor(profile)).machines,
    profilePolicy: async ({ profile }) => policyFor(profile),
  });

  void safeApplyHidden();

  const usage = [
    "Usage:",
    "  bb profiles list",
    "  bb profiles create <name>",
    "  bb profiles use <project-id> <profile>",
    "  bb profiles env <profile> <provider|*> <NAME> <value>",
    "  bb profiles unset <profile> <provider|*> <NAME>",
    "  bb profiles show [project-id]",
    "  bb profiles providers <profile> <id,id,...|all>",
    "  bb profiles machines <profile> <hostId,...|all>",
  ].join("\n");
  bb.cli.register({
    name: "profiles",
    summary: "Isolated work/personal profiles for provider config and logins",
    commands: [
      { name: "list", summary: "Projects and their profiles", usage: "bb profiles list" },
      { name: "create", summary: "Create a profile", usage: "bb profiles create <name>" },
      {
        name: "use",
        summary: "Assign a project to a profile",
        usage: "bb profiles use <project-id> <profile>",
      },
      {
        name: "env",
        summary: "Set an env var of a profile for one provider (or *)",
        usage: "bb profiles env <profile> <provider|*> <NAME> <value>",
      },
      {
        name: "unset",
        summary: "Remove an env var of a profile, so the provider uses its default",
        usage: "bb profiles unset <profile> <provider|*> <NAME>",
      },
      {
        name: "providers",
        summary: "Restrict which providers a profile may run",
        usage: "bb profiles providers <profile> <id,id,...|all>",
      },
      {
        name: "machines",
        summary: "Restrict which machines a profile may run on",
        usage: "bb profiles machines <profile> <hostId,...|all>",
      },
      {
        name: "show",
        summary: "Show a profile's policy and env, or the profile of a project",
        usage: "bb profiles show [project-id]",
      },
    ],
    async run(argv, ctx) {
      const [command, ...args] = argv;
      try {
        switch (command) {
          case "list": {
            const rows = await projectRows();
            return {
              exitCode: 0,
              stdout: rows
                .map((r) => `${r.id}  ${r.profile.padEnd(10)} ${r.name}`)
                .join("\n"),
            };
          }
          case "create": {
            if (args.length !== 1) break;
            const profiles = await createProfile(args[0]!);
            return { exitCode: 0, stdout: `Profiles: ${profiles.join(", ")}` };
          }
          case "use": {
            if (args.length !== 2) break;
            await assign(args[0]!, args[1]!);
            return { exitCode: 0, stdout: `${args[0]} -> ${args[1]}` };
          }
          case "env": {
            if (args.length !== 4) break;
            const [name, providerId, key, value] = args as [
              string,
              string,
              string,
              string,
            ];
            if (!ENV_NAME.test(key)) {
              return { exitCode: 1, stderr: "Invalid env var name" };
            }
            const profiles = await readProfiles();
            const profile = profiles[name];
            if (profile === undefined) {
              return { exitCode: 1, stderr: `No profile "${name}"` };
            }
            profile[providerId] = { ...profile[providerId], [key]: value };
            await bb.storage.kv.set("profiles", profiles);
            const note =
              providerId === "*" || providerIds.has(providerId)
                ? ""
                : "\nNew provider id: run `bb plugin reload profiles` to apply it.";
            return {
              exitCode: 0,
              stdout: `${name}/${providerId}: ${key} set${note}`,
            };
          }
          case "unset": {
            if (args.length !== 3) break;
            const [name, providerId, key] = args as [string, string, string];
            const profiles = await readProfiles();
            const vars = profiles[name]?.[providerId];
            if (vars === undefined || !(key in vars)) {
              return { exitCode: 1, stderr: `${name}/${providerId}: ${key} is not set` };
            }
            const { [key]: _removed, ...rest } = vars;
            profiles[name]![providerId] = rest;
            await bb.storage.kv.set("profiles", profiles);
            return { exitCode: 0, stdout: `${name}/${providerId}: ${key} removed` };
          }
          case "providers": {
            if (args.length !== 2) break;
            const [name, list] = args as [string, string];
            if (!(name in (await readProfiles()))) {
              return { exitCode: 1, stderr: `No profile "${name}"` };
            }
            const allowed = await readAllowed();
            allowed[name] =
              list === "all" ? null : list.split(",").map((id) => id.trim());
            await bb.storage.kv.set("allowed", allowed);
            return {
              exitCode: 0,
              stdout: `${name}: ${allowed[name]?.join(", ") ?? "all providers"}`,
            };
          }
          case "machines": {
            if (args.length !== 2) break;
            const plan = await planMachinesUpdate({
              args,
              profileNames: Object.keys(await readProfiles()),
              machines: await readMachines(),
              listHostIds: async () => {
                const hosts = await bb.sdk.hosts.list();
                return hosts.map((host) => host.id);
              },
            });
            if (!plan.ok) {
              return { exitCode: 1, stderr: plan.error };
            }
            await bb.storage.kv.set("machines", plan.machines);
            bb.realtime.publish(CHANGED, { profile: args[0], machines: plan.value });
            return {
              exitCode: 0,
              stdout:
                `${args[0]}: ${plan.value?.join(", ") ?? "all machines"}` +
                (plan.warning === null ? "" : `\n${plan.warning}`),
            };
          }
          case "show": {
            const projectId = args[0] ?? ctx.projectId ?? null;
            const profiles = await readProfiles();
            const name = await profileFor(projectId);
            const policy = await policyFor(name);
            return {
              exitCode: 0,
              stdout:
                `${projectId ?? "(default)"} -> ${name}\n` +
                `${describePolicy(policy)}\n` +
                `env:\n${JSON.stringify(profiles[name] ?? {}, null, 2)}`,
            };
          }
        }
      } catch (cause) {
        return {
          exitCode: 1,
          stderr: cause instanceof Error ? cause.message : String(cause),
        };
      }
      return { exitCode: 1, stderr: usage };
    },
  });
}
