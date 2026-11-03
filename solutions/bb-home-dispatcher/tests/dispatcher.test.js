import assert from "node:assert/strict";
import test from "node:test";

import plugin from "../server.js";
import { createDispatcher } from "../lib/dispatcher.js";
import { sign } from "../lib/hmac.js";

const SECRET = "test-secret";
const PLUGIN_ID = "home-dispatcher";

function fakeKv() {
  const map = new Map();
  return {
    map,
    async get(key) {
      return map.has(key) ? structuredClone(map.get(key)) : undefined;
    },
    async set(key, value) {
      map.set(key, structuredClone(value));
    },
    async delete(key) {
      map.delete(key);
    },
    async list(prefix = "") {
      return [...map.keys()].filter((key) => key.startsWith(prefix)).sort();
    },
  };
}

function makeWorld() {
  const projects = [
    {
      id: "proj_alpha",
      name: "Alpha",
      sources: [
        {
          hostId: "host_mac",
          path: "/work/alpha",
          isDefault: true,
          type: "local_path",
        },
      ],
    },
    {
      id: "proj_beta",
      name: "Beta",
      sources: [
        {
          hostId: "host_mac",
          path: "/work/beta",
          isDefault: true,
          type: "local_path",
        },
      ],
    },
    {
      id: "proj_gamma",
      name: "Gamma",
      sources: [
        {
          hostId: "host_off",
          path: "/work/gamma",
          isDefault: true,
          type: "local_path",
        },
      ],
    },
  ];
  const profiles = [
    { id: "proj_alpha", name: "Alpha", profile: "work" },
    { id: "proj_beta", name: "Beta", profile: "personal" },
    { id: "proj_gamma", name: "Gamma", profile: "work" },
  ];
  const policies = new Map([
    ["work", { providers: null, machines: null }],
    ["personal", { providers: null, machines: null }],
  ]);
  const hosts = [
    { id: "host_mac", name: "Mac", status: "connected" },
    { id: "host_off", name: "Offline", status: "disconnected" },
  ];
  const catalog = [
    { id: "codex", displayName: "Codex", enabled: true, available: true },
    {
      id: "opencode",
      displayName: "OpenCode",
      enabled: true,
      available: true,
    },
    {
      id: "claude-code",
      displayName: "Claude Code",
      enabled: true,
      available: true,
    },
  ];
  const models = {
    codex: {
      levels: ["low", "medium", "high"],
      rows: [
        {
          id: "gpt-5.2-codex",
          displayName: "GPT-5.2 Codex",
          defaultReasoningEffort: "medium",
          supportedReasoningEfforts: ["low", "medium", "high"],
        },
      ],
    },
    opencode: {
      levels: ["low", "medium"],
      rows: [
        {
          id: "opencode-go/deepseek-v4.1-flash",
          displayName: "DeepSeek Flash",
          defaultReasoningEffort: "low",
          supportedReasoningEfforts: ["low", "medium"],
        },
      ],
    },
    "claude-code": {
      levels: ["low", "high"],
      rows: [
        {
          id: "claude-sonnet",
          displayName: "Claude Sonnet",
          defaultReasoningEffort: "high",
          supportedReasoningEfforts: ["low", "high"],
        },
      ],
    },
  };

  const threads = new Map();
  const events = new Map();
  const pending = new Map();
  const contextUsage = new Map();
  const calls = [];
  let threadCounter = 0;
  let requestCounter = 0;
  let parseCalls = 0;

  function record(path, args) {
    calls.push({ path, args });
  }

  function pushEvent(threadId, type, data = {}) {
    const list = events.get(threadId) ?? [];
    const seq = list.length === 0 ? 1 : list[list.length - 1].seq + 1;
    const row = {
      id: `ev_${threadId}_${seq}`,
      threadId,
      seq,
      createdAt: Date.now(),
      scope: { kind: "thread" },
      type,
      data,
    };
    list.push(row);
    events.set(threadId, list);
    return row;
  }

  function setThreadStatus(threadId, status) {
    const thread = threads.get(threadId);
    if (thread) {
      thread.status = status;
      thread.updatedAt = Date.now();
    }
  }

  const sdk = {
    projects: {
      async list() {
        record("projects.list", {});
        return structuredClone(projects);
      },
    },
    hosts: {
      async list() {
        record("hosts.list", {});
        return structuredClone(hosts);
      },
    },
    providers: {
      async catalog() {
        record("providers.catalog", {});
        return structuredClone(catalog);
      },
      async list({ hostId } = {}) {
        record("providers.list", { hostId });
        const rows = hostId === "host_off" ? [] : catalog;
        return structuredClone(
          rows.map((row) => ({
            id: row.id,
            displayName: row.displayName,
            available: true,
            enabled: true,
          })),
        );
      },
      async models({ hostId, providerId }) {
        record("providers.models", { hostId, providerId });
        const entry = models[providerId];
        if (entry === undefined || hostId === "host_off") {
          return { models: [], providers: [], modelLoadError: null };
        }
        return {
          modelLoadError: null,
          permissionCeiling: "auto",
          providers: [
            {
              id: providerId,
              displayName: providerId,
              reasoningLevels: entry.levels.map((id) => ({ id, label: id })),
            },
          ],
          models: entry.rows.map((row) => ({
            ...row,
            supportedReasoningEfforts: row.supportedReasoningEfforts.map(
              (reasoningEffort) => ({
                reasoningEffort,
                description: reasoningEffort,
              }),
            ),
          })),
          selectedOnlyModels: [],
        };
      },
    },
    plugins: {
      // Strict fake: real callRpc calls outputSchema.parse before returning.
      // A Standard Schema object (only ~standard.validate) fails here, so the
      // tests reproduce the live `outputSchema.parse is not a function` bug.
      async callRpc({ pluginId, method, input, outputSchema }) {
        record("plugins.callRpc", { pluginId, method, input });
        if (typeof outputSchema?.parse !== "function") {
          throw new TypeError("outputSchema.parse is not a function");
        }
        let result;
        if (pluginId === "profiles" && method === "listProjects") {
          result = { projects: structuredClone(profiles) };
        } else if (pluginId === "profiles" && method === "profilePolicy") {
          const policy = policies.get(input?.profile);
          if (policy === undefined) {
            const error = new Error(`No profile named "${input?.profile}"`);
            error.code = "not_found";
            throw error;
          }
          result = structuredClone(policy);
        } else {
          throw new Error(`unexpected rpc ${pluginId}/${method}`);
        }
        parseCalls += 1;
        return outputSchema.parse(result);
      },
    },
    threads: {
      async spawn(args) {
        record("threads.spawn", args);
        threadCounter += 1;
        const project = projects.find((row) => row.id === args.projectId);
        const source = project.sources[0];
        const thread = {
          id: `thr_${threadCounter}`,
          title: args.title ?? null,
          titleFallback: null,
          projectId: args.projectId,
          providerId: args.providerId,
          status: "starting",
          visibility: args.visibility ?? "visible",
          originPluginId: PLUGIN_ID,
          environmentHostId: source.hostId,
          createdAt: Date.now(),
          updatedAt: Date.now(),
        };
        threads.set(thread.id, thread);
        events.set(thread.id, []);
        return structuredClone(thread);
      },
      async get({ threadId }) {
        record("threads.get", { threadId });
        const thread = threads.get(threadId);
        if (thread === undefined) throw new Error("not found");
        return structuredClone(thread);
      },
      async list(args = {}) {
        record("threads.list", args);
        let rows = [...threads.values()];
        if (Array.isArray(args.threadIds)) {
          rows = rows.filter((row) => args.threadIds.includes(row.id));
        }
        if (args.originPluginId !== undefined) {
          rows = rows.filter(
            (row) => row.originPluginId === args.originPluginId,
          );
        }
        rows.sort((a, b) => b.updatedAt - a.updatedAt);
        const offset = args.offset ?? 0;
        const limit = args.limit ?? 100;
        return structuredClone(rows.slice(offset, offset + limit));
      },
      async send(args) {
        record("threads.send", args);
        if (!threads.has(args.threadId)) throw new Error("not found");
        if (args.compactFirst === true && args.mode === "steer") {
          throw new Error("compactFirst cannot be combined with mode steer");
        }
        requestCounter += 1;
        pushEvent(args.threadId, "client/turn/requested", {
          requestId: `req_${requestCounter}`,
          target: { kind: "new-turn" },
        });
        setThreadStatus(args.threadId, "active");
        return { ok: true, delivery: "sent" };
      },
      async context({ threadId }) {
        record("threads.context", { threadId });
        return { usage: structuredClone(contextUsage.get(threadId) ?? null) };
      },
      async compact({ threadId }) {
        record("threads.compact", { threadId });
        const thread = threads.get(threadId);
        if (thread === undefined) throw new Error("not found");
        if (thread.status !== "idle" && thread.status !== "error") {
          const error = new Error(
            "Context can only be compacted while the thread is idle or errored",
          );
          error.code = "invalid_request";
          throw error;
        }
        setThreadStatus(threadId, "active");
        return { ok: true };
      },
      async output({ threadId }) {
        record("threads.output", { threadId });
        const list = events.get(threadId) ?? [];
        let text = null;
        for (const row of list) {
          if (
            row.type === "item/completed" &&
            row.data?.item?.type === "agentMessage"
          ) {
            text = row.data.item.text;
          }
        }
        return { output: text };
      },
      async eventsList(args) {
        record("threads.events.list", args);
        let rows = (events.get(args.threadId) ?? []).slice();
        if (args.afterSeq !== undefined) {
          rows = rows.filter((row) => row.seq > Number(args.afterSeq));
        }
        if (args.beforeSeq !== undefined) {
          rows = rows.filter((row) => row.seq < Number(args.beforeSeq));
        }
        if (Array.isArray(args.types)) {
          rows = rows.filter((row) => args.types.includes(row.type));
        }
        if (args.order === "desc") rows.reverse();
        if (args.limit !== undefined) rows = rows.slice(0, Number(args.limit));
        return structuredClone(rows);
      },
      events: {
        async list(args) {
          return sdk.threads.eventsList(args);
        },
      },
      interactions: {
        async list({ threadId }) {
          record("threads.interactions.list", { threadId });
          return structuredClone(pending.get(threadId) ?? []);
        },
      },
    },
  };

  return {
    sdk,
    calls,
    threads,
    events,
    pending,
    contextUsage,
    policies,
    pushEvent,
    setThreadStatus,
    parseCount() {
      return parseCalls;
    },
    countCalls(path) {
      return calls.filter((call) => call.path === path).length;
    },
    callsTo(path) {
      return calls.filter((call) => call.path === path);
    },
  };
}

async function makePlugin({ settings = {}, world = makeWorld() } = {}) {
  const routes = [];
  const needsConfiguration = [];
  const schedules = [];
  const kv = fakeKv();
  let descriptors = {};
  const effective = {
    hmacSecret: SECRET,
    workerLimit: 2,
    permissionMode: "auto",
    dispatcherThreadId: "thr_dispatcher",
    ...settings,
  };
  const bb = {
    pluginId: PLUGIN_ID,
    log: { debug() {}, info() {}, warn() {}, error() {} },
    settings: {
      define(declared) {
        descriptors = declared;
        return {
          async get() {
            return { ...effective };
          },
        };
      },
    },
    storage: { kv },
    status: {
      needsConfiguration(message) {
        needsConfiguration.push(message);
      },
    },
    http: {
      route(method, path, handler, options) {
        routes.push({ method, path, handler, options });
      },
    },
    background: {
      schedule(name, cron, fn) {
        schedules.push({ name, cron, fn });
      },
    },
    sdk: world.sdk,
  };
  await plugin(bb);
  assert.equal(routes.length, 1);
  return {
    routes,
    route: routes[0],
    kv,
    world,
    needsConfiguration,
    schedules,
    descriptors,
    settings: effective,
  };
}

function makeContext(body, headers = {}) {
  const map = new Map(
    Object.entries(headers).map(([key, value]) => [
      key.toLowerCase(),
      String(value),
    ]),
  );
  return {
    req: {
      async text() {
        return body;
      },
      header(name) {
        return map.get(name.toLowerCase());
      },
    },
    json(value, status = 200) {
      return { status, body: value };
    },
  };
}

async function callRoute(
  route,
  payload,
  { secret = SECRET, timestamp, nowMs = Date.now(), headers = {}, raw } = {},
) {
  const body = raw ?? JSON.stringify(payload);
  const ts =
    timestamp === undefined ? String(Math.floor(nowMs / 1000)) : timestamp;
  const signature = secret === null ? undefined : sign(secret, ts, body);
  return route.handler(
    makeContext(body, {
      "content-type": "application/json",
      "x-bb-dispatch-timestamp": ts,
      ...(signature === undefined
        ? {}
        : { "x-bb-dispatch-signature": signature }),
      ...headers,
    }),
  );
}

function targetParams(overrides = {}) {
  return {
    profile: "work",
    projectId: "proj_alpha",
    providerId: "codex",
    model: "gpt-5.2-codex",
    reasoningLevel: "medium",
    ...overrides,
  };
}

function spawnParams(overrides = {}) {
  return {
    ...targetParams(),
    title: "Fix flaky test",
    prompt: "Find and fix the flaky test.",
    idempotencyKey: "spawn-1",
    ...overrides,
  };
}

test("route is POST /rpc with token auth", async () => {
  const { route } = await makePlugin();
  assert.equal(route.method, "POST");
  assert.equal(route.path, "/rpc");
  assert.equal(route.options.auth, "token");
});

test("HMAC: valid signature reaches the dispatcher", async () => {
  const { route } = await makePlugin();
  const response = await callRoute(route, { method: "targets", params: {} });
  assert.equal(response.status, 200);
  assert.equal(response.body.ok, true);
  assert.equal(response.body.result.projects.length, 3);
});

test("HMAC: wrong signature and stale timestamp are rejected", async () => {
  const { route } = await makePlugin();
  const payload = { method: "targets", params: {} };
  const body = JSON.stringify(payload);
  const ts = String(Math.floor(Date.now() / 1000));

  const wrong = await route.handler(
    makeContext(body, {
      "content-type": "application/json",
      "x-bb-dispatch-timestamp": ts,
      "x-bb-dispatch-signature": "0".repeat(64),
    }),
  );
  assert.equal(wrong.status, 401);
  assert.equal(wrong.body.code, "invalid_signature");

  const stale = await callRoute(route, payload, {
    nowMs: Date.now(),
    timestamp: String(Math.floor(Date.now() / 1000) - 120),
  });
  assert.equal(stale.status, 401);
  assert.equal(stale.body.code, "stale_timestamp");

  const missing = await route.handler(
    makeContext(body, { "content-type": "application/json" }),
  );
  assert.equal(missing.status, 401);
  assert.equal(missing.body.code, "invalid_timestamp");
});

test("HMAC: unconfigured secret refuses the request", async () => {
  const { route, needsConfiguration } = await makePlugin({
    settings: { hmacSecret: "" },
  });
  assert.equal(needsConfiguration.length, 1);
  const response = await callRoute(route, { method: "targets", params: {} });
  assert.equal(response.status, 503);
  assert.equal(response.body.code, "not_configured");
});

test("JSON content type is required", async () => {
  const { route } = await makePlugin();
  const body = JSON.stringify({ method: "targets", params: {} });
  const ts = String(Math.floor(Date.now() / 1000));
  const response = await route.handler(
    makeContext(body, {
      "x-bb-dispatch-timestamp": ts,
      "x-bb-dispatch-signature": sign(SECRET, ts, body),
    }),
  );
  assert.equal(response.status, 415);
  assert.equal(response.body.code, "unsupported_media_type");
});

test("body limit is enforced before parsing", async () => {
  const { route } = await makePlugin();
  const response = await callRoute(route, null, {
    raw: JSON.stringify({ method: "targets", params: {}, pad: "x".repeat(20000) }),
    headers: { "content-length": "20000" },
  });
  assert.equal(response.status, 413);
  assert.equal(response.body.code, "body_too_large");
});

test("schema: unknown method, bad JSON and extra fields are refused", async () => {
  const { route } = await makePlugin();
  const unknown = await callRoute(route, { method: "delete_everything", params: {} });
  assert.equal(unknown.status, 400);
  assert.equal(unknown.body.code, "unknown_method");

  const badJson = await callRoute(route, null, { raw: "{" });
  assert.equal(badJson.status, 400);
  assert.equal(badJson.body.code, "invalid_json");

  const extra = await callRoute(route, {
    method: "send",
    params: {
      threadId: "thr_1",
      text: "hi",
      idempotencyKey: "k1",
      terminal: "rm -rf /",
    },
  });
  assert.equal(extra.status, 400);
  assert.equal(extra.body.code, "invalid_request");
});

test("targets reports profile projects, machines and provider models", async () => {
  const { route } = await makePlugin();
  const response = await callRoute(route, { method: "targets", params: {} });
  assert.equal(response.status, 200);
  const alpha = response.body.result.projects.find(
    (row) => row.projectId === "proj_alpha",
  );
  assert.equal(alpha.profile, "work");
  assert.equal(alpha.machine.name, "Mac");
  assert.equal(alpha.machine.online, true);
  const codex = alpha.providers.find((row) => row.providerId === "codex");
  assert.deepEqual(codex.models[0].reasoningLevels, ["low", "medium", "high"]);
  const gamma = response.body.result.projects.find(
    (row) => row.projectId === "proj_gamma",
  );
  assert.equal(gamma.machine.online, false);
  assert.deepEqual(gamma.providers, []);
});

test("validate_spawn accepts a known target", async () => {
  const { route } = await makePlugin();
  const response = await callRoute(route, {
    method: "validate_spawn",
    params: targetParams(),
  });
  assert.equal(response.status, 200);
  assert.equal(response.body.ok, true);
  assert.equal(response.body.result.machine.hostId, "host_mac");
  assert.equal(response.body.result.activeThreads, 0);
});

test("validate_spawn refuses a foreign profile and a disallowed provider", async () => {
  const world = makeWorld();
  world.policies.set("work", { providers: ["codex"], machines: null });
  const { route } = await makePlugin({ world });
  const foreign = await callRoute(route, {
    method: "validate_spawn",
    params: targetParams({ projectId: "proj_beta" }),
  });
  assert.equal(foreign.body.ok, false);
  assert.equal(foreign.body.code, "profile_mismatch");

  const provider = await callRoute(route, {
    method: "validate_spawn",
    params: targetParams({
      providerId: "opencode",
      model: "opencode-go/deepseek-v4.1-flash",
      reasoningLevel: "low",
    }),
  });
  assert.equal(provider.body.ok, false);
  assert.equal(provider.body.code, "provider_not_allowed");
});

test("validate_spawn enforces the profile machine allowlist", async () => {
  const world = makeWorld();
  world.policies.set("work", {
    providers: null,
    machines: ["host_other"],
  });
  const { route } = await makePlugin({ world });

  const refused = await callRoute(route, {
    method: "validate_spawn",
    params: targetParams(),
  });
  assert.equal(refused.body.ok, false);
  assert.equal(refused.body.code, "machine_not_allowed");
  assert.match(refused.body.speech, /не разрешена профилю/u);

  world.policies.set("work", { providers: null, machines: ["host_mac"] });
  const allowed = await callRoute(route, {
    method: "validate_spawn",
    params: targetParams(),
  });
  assert.equal(allowed.body.ok, true);
  assert.equal(allowed.body.result.machine.hostId, "host_mac");

  world.policies.set("work", { providers: null, machines: null });
  const unrestricted = await callRoute(route, {
    method: "validate_spawn",
    params: targetParams({ projectId: "proj_gamma" }),
  });
  // The machine policy no longer blocks it; `proj_gamma` is offline instead.
  assert.equal(unrestricted.body.code, "machine_offline");
});

test("a profile missing from the profiles plugin is refused", async () => {
  const world = makeWorld();
  world.policies.delete("work");
  const { route } = await makePlugin({ world });

  const response = await callRoute(route, {
    method: "validate_spawn",
    params: targetParams(),
  });
  assert.equal(response.body.ok, false);
  assert.equal(response.body.code, "profile_not_found");
  assert.match(response.body.speech, /Профиль «work» не найден/u);
});

test("targets reports the profile policy and uses it for providers", async () => {
  const world = makeWorld();
  world.policies.set("work", {
    providers: ["codex"],
    machines: ["host_mac"],
  });
  const { route, world: used } = await makePlugin({ world });

  const response = await callRoute(route, { method: "targets", params: {} });
  assert.equal(response.status, 200);
  const alpha = response.body.result.projects.find(
    (row) => row.projectId === "proj_alpha",
  );
  assert.deepEqual(alpha.policy, {
    providers: ["codex"],
    machines: ["host_mac"],
  });
  assert.deepEqual(
    alpha.providers.map((row) => row.providerId),
    ["codex"],
  );
  const beta = response.body.result.projects.find(
    (row) => row.projectId === "proj_beta",
  );
  assert.deepEqual(beta.policy, { providers: null, machines: null });
  assert.equal(response.body.result.profileProvidersConfigured, undefined);
  // One strict outputSchema.parse per RPC (listProjects + one per profile).
  assert.equal(used.parseCount(), 3);
});

test("validate_spawn refuses unknown model, unsupported reasoning and offline machine", async () => {
  const { route } = await makePlugin();
  const model = await callRoute(route, {
    method: "validate_spawn",
    params: targetParams({ model: "nope" }),
  });
  assert.equal(model.body.code, "model_not_found");

  const reasoning = await callRoute(route, {
    method: "validate_spawn",
    params: targetParams({ reasoningLevel: "xhigh" }),
  });
  assert.equal(reasoning.body.code, "reasoning_not_supported");

  const offline = await callRoute(route, {
    method: "validate_spawn",
    params: targetParams({ projectId: "proj_gamma" }),
  });
  assert.equal(offline.body.code, "machine_offline");
});

test("spawn creates a visible attributed thread and is idempotent", async () => {
  const { route, world } = await makePlugin();
  const first = await callRoute(route, { method: "spawn", params: spawnParams() });
  assert.equal(first.status, 200);
  assert.equal(first.body.ok, true);
  assert.equal(first.body.result.threadId, "thr_1");
  assert.equal(world.countCalls("threads.spawn"), 1);

  const call = world.callsTo("threads.spawn")[0].args;
  assert.equal(call.title, "Fix flaky test");
  assert.equal(call.visibility, "visible");
  assert.equal(call.permissionMode, "auto");
  assert.match(call.prompt, /🔊 <1–2 фразы для озвучивания по-русски>/u);
  assert.match(
    call.prompt,
    /Если ждёшь ответа владельца, 🔊-строка заканчивается вопросом со знаком «\?»/u,
  );
  assert.deepEqual(call.environment, { type: "project-default" });
  assert.equal(call.pluginMetadata.idempotencyKey, "spawn-1");
  assert.equal(call.pluginMetadata.profile, "work");

  const repeat = await callRoute(route, {
    method: "spawn",
    params: spawnParams(),
  });
  assert.equal(repeat.body.result.threadId, "thr_1");
  assert.equal(repeat.body.result.idempotent, true);
  assert.equal(world.countCalls("threads.spawn"), 1);

  const conflict = await callRoute(route, {
    method: "spawn",
    params: spawnParams({ title: "Another title" }),
  });
  assert.equal(conflict.body.ok, false);
  assert.equal(conflict.body.code, "idempotency_conflict");
});

test("permissionMode is explicit, defaults to auto and refuses full", async () => {
  const { route, world, descriptors } = await makePlugin();
  await callRoute(route, { method: "spawn", params: spawnParams() });
  assert.equal(
    world.callsTo("threads.spawn")[0].args.permissionMode,
    "auto",
  );

  const validate =
    descriptors.permissionMode.experimental_schema["~standard"].validate;
  assert.deepEqual(validate("auto"), { value: "auto" });
  assert.deepEqual(validate("accept-edits"), { value: "accept-edits" });
  assert.equal(validate("full").issues.length, 1);
  assert.deepEqual(descriptors.permissionMode.options, [
    "auto",
    "accept-edits",
  ]);

  const custom = await makePlugin({
    settings: { permissionMode: "accept-edits" },
  });
  await callRoute(custom.route, {
    method: "spawn",
    params: spawnParams(),
  });
  assert.equal(
    custom.world.callsTo("threads.spawn")[0].args.permissionMode,
    "accept-edits",
  );

  const full = await makePlugin({ settings: { permissionMode: "full" } });
  const refused = await callRoute(full.route, {
    method: "spawn",
    params: spawnParams(),
  });
  assert.equal(refused.body.ok, false);
  assert.equal(refused.body.code, "bad_settings");
  assert.equal(full.world.countCalls("threads.spawn"), 0);
});

test("concurrent spawn with one key creates exactly one thread", async () => {
  const { route, world } = await makePlugin();
  const spawn = world.sdk.threads.spawn;
  world.sdk.threads.spawn = async (args) => {
    await new Promise((resolve) => setTimeout(resolve, 20));
    return spawn(args);
  };
  const [first, second] = await Promise.all([
    callRoute(route, { method: "spawn", params: spawnParams() }),
    callRoute(route, { method: "spawn", params: spawnParams() }),
  ]);
  assert.equal(first.body.ok, true);
  assert.equal(second.body.ok, true);
  assert.equal(first.body.result.threadId, second.body.result.threadId);
  assert.equal(world.countCalls("threads.spawn"), 1);
});

test("spawn enforces the concurrent thread limit", async () => {
  const { route } = await makePlugin();
  await callRoute(route, {
    method: "spawn",
    params: spawnParams({ idempotencyKey: "spawn-1", title: "One" }),
  });
  await callRoute(route, {
    method: "spawn",
    params: spawnParams({ idempotencyKey: "spawn-2", title: "Two" }),
  });
  const third = await callRoute(route, {
    method: "spawn",
    params: spawnParams({ idempotencyKey: "spawn-3", title: "Three" }),
  });
  assert.equal(third.body.ok, false);
  assert.equal(third.body.code, "thread_limit");
});

test("send writes only to plugin threads or the configured dispatcher", async () => {
  const { route, world } = await makePlugin();
  world.threads.set("thr_foreign", {
    id: "thr_foreign",
    title: "Foreign",
    projectId: "proj_alpha",
    status: "idle",
    originPluginId: null,
    environmentHostId: "host_mac",
    createdAt: Date.now(),
    updatedAt: Date.now(),
  });
  const foreign = await callRoute(route, {
    method: "send",
    params: { threadId: "thr_foreign", text: "hi", idempotencyKey: "s0" },
  });
  assert.equal(foreign.body.ok, false);
  assert.equal(foreign.body.code, "thread_not_allowed");

  const dispatcher = await callRoute(route, {
    method: "send",
    params: { threadId: "thr_dispatcher", text: "hi", idempotencyKey: "s1" },
  });
  // thr_dispatcher must exist in bb; add it for the happy path below.
  assert.equal(dispatcher.body.code, "thread_not_found");

  world.threads.set("thr_dispatcher", {
    id: "thr_dispatcher",
    title: "Dispatcher",
    projectId: "proj_alpha",
    status: "idle",
    originPluginId: null,
    environmentHostId: "host_mac",
    createdAt: Date.now(),
    updatedAt: Date.now(),
  });
  const ok = await callRoute(route, {
    method: "send",
    params: { threadId: "thr_dispatcher", text: "hi", idempotencyKey: "s2" },
  });
  assert.equal(ok.body.ok, true);
  assert.equal(ok.body.result.threadId, "thr_dispatcher");
  assert.equal(typeof ok.body.result.afterSeq, "number");
  assert.equal(typeof ok.body.result.requestSeq, "number");
  assert.equal(world.countCalls("threads.send"), 1);

  const repeat = await callRoute(route, {
    method: "send",
    params: { threadId: "thr_dispatcher", text: "hi", idempotencyKey: "s2" },
  });
  assert.equal(repeat.body.result.idempotent, true);
  assert.equal(world.countCalls("threads.send"), 1);
});

test("turn_result correlates the request after afterSeq, not the previous answer", async () => {
  const { route, world } = await makePlugin();
  const spawned = await callRoute(route, {
    method: "spawn",
    params: spawnParams(),
  });
  const threadId = spawned.body.result.threadId;

  const first = await callRoute(route, {
    method: "send",
    params: { threadId, text: "first", idempotencyKey: "send-1" },
  });
  const afterFirst = first.body.result.afterSeq;
  const requestOne = first.body.result.requestSeq;
  world.pushEvent(threadId, "item/completed", {
    item: { type: "agentMessage", text: "Первый ответ.\n🔊 Первый." },
  });
  const completeOne = world.pushEvent(threadId, "turn/completed", {
    status: "completed",
  });
  world.setThreadStatus(threadId, "idle");

  const firstResult = await callRoute(route, {
    method: "turn_result",
    params: { threadId, afterSeq: afterFirst },
  });
  assert.equal(firstResult.body.ok, true);
  assert.equal(firstResult.body.result.status, "idle");
  assert.equal(firstResult.body.result.spoken, "Первый.");
  assert.equal(firstResult.body.result.requestSeq, requestOne);
  assert.equal(firstResult.body.result.completedSeq, completeOne.seq);

  const second = await callRoute(route, {
    method: "send",
    params: { threadId, text: "second", idempotencyKey: "send-2" },
  });
  const afterSecond = completeOne.seq;
  const requestTwo = second.body.result.requestSeq;
  world.pushEvent(threadId, "item/completed", {
    item: { type: "agentMessage", text: "Второй ответ.\n🔊 Второй." },
  });
  const completeTwo = world.pushEvent(threadId, "turn/completed", {
    status: "completed",
  });
  world.setThreadStatus(threadId, "idle");

  const secondResult = await callRoute(route, {
    method: "turn_result",
    params: { threadId, afterSeq: afterSecond },
  });
  assert.equal(secondResult.body.result.spoken, "Второй.");
  assert.equal(secondResult.body.result.requestSeq, requestTwo);
  assert.equal(secondResult.body.result.completedSeq, completeTwo.seq);
});

test("turn_result reports running, waiting_input and failed", async () => {
  const { route, world } = await makePlugin();
  const spawned = await callRoute(route, {
    method: "spawn",
    params: spawnParams(),
  });
  const threadId = spawned.body.result.threadId;
  const sent = await callRoute(route, {
    method: "send",
    params: { threadId, text: "work", idempotencyKey: "send-1" },
  });
  const afterSeq = sent.body.result.afterSeq;

  const running = await callRoute(route, {
    method: "turn_result",
    params: { threadId, afterSeq },
  });
  assert.equal(running.body.result.status, "running");

  world.pending.set(threadId, [{ id: "int_1", status: "pending" }]);
  const waiting = await callRoute(route, {
    method: "turn_result",
    params: { threadId, afterSeq },
  });
  assert.equal(waiting.body.result.status, "waiting_input");

  world.pending.delete(threadId);
  world.pushEvent(threadId, "turn/completed", {
    status: "failed",
    error: { message: "provider exploded" },
  });
  world.setThreadStatus(threadId, "error");
  const failed = await callRoute(route, {
    method: "turn_result",
    params: { threadId, afterSeq },
  });
  assert.equal(failed.body.result.status, "failed");
  assert.match(failed.body.result.speech, /provider exploded/u);
});

test("turn_result falls back to plain text when no 🔊 line exists", async () => {
  const { route, world } = await makePlugin();
  const spawned = await callRoute(route, {
    method: "spawn",
    params: spawnParams(),
  });
  const threadId = spawned.body.result.threadId;
  const sent = await callRoute(route, {
    method: "send",
    params: { threadId, text: "work", idempotencyKey: "send-1" },
  });
  world.pushEvent(threadId, "item/completed", {
    item: {
      type: "agentMessage",
      text: "## Итог\n\n**Готово**: тесты зелёные.",
    },
  });
  world.pushEvent(threadId, "turn/completed", { status: "completed" });
  world.setThreadStatus(threadId, "idle");

  const result = await callRoute(route, {
    method: "turn_result",
    params: { threadId, afterSeq: sent.body.result.afterSeq },
  });
  assert.equal(result.body.result.spoken, null);
  assert.equal(result.body.result.text, "Итог Готово: тесты зелёные.");
  assert.equal(result.body.result.speech, "Готово.");
});

test("send returns requestSeq null when the turn has not started yet", async () => {
  const world = makeWorld();
  const kv = fakeKv();
  const settings = {
    hmacSecret: SECRET,
    workerLimit: 2,
    dispatcherThreadId: "thr_dispatcher",
  };
  const dispatcher = createDispatcher({
    getSdk: () => world.sdk,
    kv,
    pluginId: PLUGIN_ID,
    getSettings: async () => settings,
    sendWaitMs: 0,
  });
  world.threads.set("thr_dispatcher", {
    id: "thr_dispatcher",
    title: "Dispatcher",
    projectId: "proj_alpha",
    status: "idle",
    originPluginId: null,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  });
  world.sdk.threads.send = async (args) => {
    world.calls.push({ path: "threads.send", args });
    return { ok: true, delivery: "queued" };
  };
  const result = await dispatcher.call("send", {
    threadId: "thr_dispatcher",
    text: "queued",
    idempotencyKey: "q1",
  });
  assert.equal(result.requestSeq, null);
  assert.equal(result.requestId, null);
  assert.equal(result.delivery, "queued");
});

test("status includes running threads with profiles and machine", async () => {
  const { route, world } = await makePlugin();
  const spawned = await callRoute(route, {
    method: "spawn",
    params: spawnParams(),
  });
  const threadId = spawned.body.result.threadId;
  world.pushEvent(threadId, "turn/started", {});
  const response = await callRoute(route, {
    method: "status",
    params: { limit: 5 },
  });
  assert.equal(response.status, 200);
  const row = response.body.result.threads.find(
    (item) => item.threadId === threadId,
  );
  assert.equal(row.profile, "work");
  assert.equal(row.machine.name, "Mac");
  assert.equal(row.projectName, "Alpha");
  assert.equal(row.status, "starting");
  assert.equal(typeof row.runtimeSeconds, "number");
});

// ------------------------------------------------------------ compaction --

function addDispatcherThread(world, status = "idle") {
  world.threads.set("thr_dispatcher", {
    id: "thr_dispatcher",
    title: "Dispatcher",
    projectId: "proj_alpha",
    status,
    originPluginId: null,
    environmentHostId: "host_mac",
    createdAt: Date.now(),
    updatedAt: Date.now(),
  });
}

function makeCompactionDispatcher({ world, settings = {}, nowRef, delay }) {
  const kv = fakeKv();
  const dispatcher = createDispatcher({
    getSdk: () => world.sdk,
    kv,
    pluginId: PLUGIN_ID,
    getSettings: async () => ({
      hmacSecret: SECRET,
      workerLimit: 2,
      permissionMode: "auto",
      dispatcherThreadId: "thr_dispatcher",
      ...settings,
    }),
    now: () => nowRef.value,
    ...(delay === undefined ? {} : { delay }),
    sendWaitMs: 0,
  });
  return { dispatcher, kv };
}

async function idleSend(dispatcher, world, text, key) {
  world.setThreadStatus("thr_dispatcher", "idle");
  return dispatcher.call("send", {
    threadId: "thr_dispatcher",
    text,
    idempotencyKey: key,
  });
}

test("plugin registers the nightly compaction schedule", async () => {
  const { schedules } = await makePlugin();
  assert.equal(schedules.length, 1);
  assert.equal(schedules[0].name, "dispatcher-compaction");
  assert.equal(schedules[0].cron, "* * * * *");
  assert.equal(typeof schedules[0].fn, "function");
});

test("nightly compaction waits for compactAt, is daily and catches up", async () => {
  const world = makeWorld();
  addDispatcherThread(world, "idle");
  const nowRef = { value: new Date(2026, 10, 3, 4, 29, 0).getTime() };
  const { dispatcher, kv } = makeCompactionDispatcher({
    world,
    settings: { compactAt: "04:30" },
    nowRef,
  });

  const early = await dispatcher.backgroundTick();
  assert.equal(early.ran, false);
  assert.equal(early.reason, "not_due");
  assert.equal(world.countCalls("threads.compact"), 0);

  nowRef.value = new Date(2026, 10, 3, 4, 30, 0).getTime();
  const due = await dispatcher.backgroundTick();
  assert.equal(due.ran, true);
  assert.equal(due.threadId, "thr_dispatcher");
  assert.equal(world.countCalls("threads.compact"), 1);
  const state = await kv.get("compact:thr_dispatcher");
  assert.equal(state.lastCompactionStatus, "requested");
  assert.equal(state.lastCompactionReason, "nightly");
  assert.equal(state.pending.reason, "nightly");

  const repeat = await dispatcher.backgroundTick();
  assert.equal(repeat.ran, false);
  assert.equal(repeat.reason, "already_requested");
  assert.equal(world.countCalls("threads.compact"), 1);
});

test("nightly compaction skips a busy thread and retries on the next tick", async () => {
  const world = makeWorld();
  addDispatcherThread(world, "active");
  const nowRef = { value: new Date(2026, 10, 3, 4, 30, 0).getTime() };
  const { dispatcher } = makeCompactionDispatcher({
    world,
    settings: { compactAt: "04:30" },
    nowRef,
  });

  const busy = await dispatcher.backgroundTick();
  assert.equal(busy.ran, false);
  assert.equal(busy.reason, "busy");
  assert.equal(world.countCalls("threads.compact"), 0);

  // A busy retry is throttled so the minute-level schedule does not look the
  // thread up on every server tick.
  world.setThreadStatus("thr_dispatcher", "idle");
  const throttled = await dispatcher.backgroundTick();
  assert.equal(throttled.reason, "throttled");

  nowRef.value += 60_000;
  const retry = await dispatcher.backgroundTick();
  assert.equal(retry.ran, true);
  assert.equal(world.countCalls("threads.compact"), 1);
});

test("nightly compaction catches up after a late server start", async () => {
  const world = makeWorld();
  addDispatcherThread(world, "idle");
  const nowRef = { value: new Date(2026, 10, 3, 9, 15, 0).getTime() };
  const { dispatcher } = makeCompactionDispatcher({
    world,
    settings: { compactAt: "04:30" },
    nowRef,
  });

  const outcome = await dispatcher.backgroundTick();
  assert.equal(outcome.ran, true);
  assert.equal(world.countCalls("threads.compact"), 1);
});

test("nightly compaction is disabled by an empty compactAt", async () => {
  const world = makeWorld();
  addDispatcherThread(world, "idle");
  const nowRef = { value: new Date(2026, 10, 3, 12, 0, 0).getTime() };
  const { dispatcher } = makeCompactionDispatcher({
    world,
    settings: { compactAt: "" },
    nowRef,
  });

  const outcome = await dispatcher.backgroundTick();
  assert.equal(outcome.ran, false);
  assert.equal(outcome.reason, "not_configured");
  assert.equal(world.countCalls("threads.compact"), 0);
});

test("after a completed nightly compaction the reminder rides the next send once", async () => {
  const world = makeWorld();
  addDispatcherThread(world, "idle");
  const nowRef = { value: new Date(2026, 10, 3, 4, 30, 0).getTime() };
  const { dispatcher, kv } = makeCompactionDispatcher({
    world,
    settings: { compactAt: "04:30", dispatcherReminder: "ROLE REMINDER" },
    nowRef,
  });

  await dispatcher.backgroundTick();

  // While the compaction turn runs, a prompt waits in the queue instead of
  // steering into the compaction.
  const during = await dispatcher.call("send", {
    threadId: "thr_dispatcher",
    text: "during compaction",
    idempotencyKey: "during-1",
  });
  const duringCall = world.callsTo("threads.send")[0].args;
  assert.equal(duringCall.mode, "queue");
  assert.equal(duringCall.input[0].text, "during compaction");
  assert.equal(duringCall.compactFirst, undefined);

  world.pushEvent("thr_dispatcher", "thread/compacted", {});
  world.setThreadStatus("thr_dispatcher", "idle");

  const first = await idleSend(
    dispatcher,
    world,
    "after compaction",
    "after-1",
  );
  assert.equal(first.compactFirst, false);
  assert.equal(first.reminder, true);
  assert.match(
    world.callsTo("threads.send")[1].args.input[0].text,
    /after compaction\n\nROLE REMINDER$/u,
  );
  const state = await kv.get("compact:thr_dispatcher");
  assert.equal(state.turns, 1);
  assert.equal(state.pendingReminder, false);
  assert.equal(state.lastCompactionStatus, "completed");
  assert.equal(state.turns, 1);

  const second = await idleSend(dispatcher, world, "next message", "after-2");
  assert.equal(second.reminder, undefined);
  assert.equal(
    world.callsTo("threads.send")[2].args.input[0].text,
    "next message",
  );
});

test("send compacts after compactAfterTurns idle sends, never with steer", async () => {
  const world = makeWorld();
  addDispatcherThread(world, "idle");
  const nowRef = { value: new Date(2026, 10, 3, 12, 0, 0).getTime() };
  const { dispatcher, kv } = makeCompactionDispatcher({
    world,
    settings: {
      compactAt: "",
      compactAfterTurns: 2,
      dispatcherReminder: "ROLE REMINDER",
    },
    nowRef,
  });

  const first = await idleSend(dispatcher, world, "one", "t-1");
  const second = await idleSend(dispatcher, world, "two", "t-2");
  assert.equal(first.compactFirst, false);
  assert.equal(second.compactFirst, false);
  const before = await kv.get("compact:thr_dispatcher");
  assert.equal(before.turns, 2);

  const third = await idleSend(dispatcher, world, "three", "t-3");
  assert.equal(third.compactFirst, true);
  assert.equal(third.compactTrigger, "turns");
  const compactCall = world.callsTo("threads.send")[2].args;
  assert.equal(compactCall.compactFirst, true);
  assert.notEqual(compactCall.mode, "steer");
  assert.equal(compactCall.mode, "auto");
  // The compaction runs before this message, so the reminder belongs here.
  assert.match(compactCall.input[0].text, /three\n\nROLE REMINDER$/u);

  const after = await kv.get("compact:thr_dispatcher");
  assert.equal(after.turns, 0);
  assert.equal(after.lastCompactionStatus, "requested");
  assert.equal(after.lastCompactionReason, "turns");

  for (const call of world.callsTo("threads.send")) {
    assert.notEqual(call.args.mode, "steer");
  }
});

test("a busy thread is sent without compaction and keeps the turn count", async () => {
  const world = makeWorld();
  addDispatcherThread(world, "active");
  const nowRef = { value: new Date(2026, 10, 3, 12, 0, 0).getTime() };
  const { dispatcher, kv } = makeCompactionDispatcher({
    world,
    settings: { compactAt: "", compactAfterTurns: 1 },
    nowRef,
  });

  const busy = await dispatcher.call("send", {
    threadId: "thr_dispatcher",
    text: "busy",
    idempotencyKey: "b-1",
  });
  assert.equal(busy.compactFirst, false);
  assert.equal(world.callsTo("threads.send")[0].args.compactFirst, undefined);
  const busyState = await kv.get("compact:thr_dispatcher");
  assert.equal(busyState.turns, 1);

  const next = await idleSend(dispatcher, world, "idle again", "b-2");
  assert.equal(next.compactFirst, true);
  assert.equal(world.callsTo("threads.send")[1].args.compactFirst, true);
});

test("context usage takes priority over the turn counter", async () => {
  const world = makeWorld();
  addDispatcherThread(world, "idle");
  world.contextUsage.set("thr_dispatcher", {
    usedTokens: 75000,
    modelContextWindow: 100000,
    estimated: false,
  });
  const nowRef = { value: new Date(2026, 10, 3, 12, 0, 0).getTime() };
  const { dispatcher } = makeCompactionDispatcher({
    world,
    settings: {
      compactAt: "",
      compactAfterTurns: 30,
      compactAtContextPercent: 70,
      dispatcherReminder: "",
    },
    nowRef,
  });

  const first = await idleSend(dispatcher, world, "high usage", "c-1");
  assert.equal(first.compactFirst, true);
  assert.equal(first.compactTrigger, "context");
  assert.equal(world.callsTo("threads.send")[0].args.compactFirst, true);
});

test("usage below the context threshold falls back to the turn counter", async () => {
  const world = makeWorld();
  addDispatcherThread(world, "idle");
  world.contextUsage.set("thr_dispatcher", {
    usedTokens: 20000,
    modelContextWindow: 100000,
    estimated: false,
  });
  const nowRef = { value: new Date(2026, 10, 3, 12, 0, 0).getTime() };
  const { dispatcher } = makeCompactionDispatcher({
    world,
    settings: {
      compactAt: "",
      compactAfterTurns: 1,
      compactAtContextPercent: 70,
      dispatcherReminder: "",
    },
    nowRef,
  });

  const below = await idleSend(dispatcher, world, "below usage", "c-2");
  assert.equal(below.compactFirst, false);

  world.contextUsage.delete("thr_dispatcher");
  const fallback = await idleSend(dispatcher, world, "no usage", "c-3");
  assert.equal(fallback.compactFirst, true);
  assert.equal(fallback.compactTrigger, "turns");
});

test("status reports compaction state and settings", async () => {
  const { route, world } = await makePlugin({
    settings: {
      compactAt: "05:15",
      compactAfterTurns: 12,
      compactAtContextPercent: 65,
    },
  });
  addDispatcherThread(world, "idle");
  const sent = await callRoute(route, {
    method: "send",
    params: { threadId: "thr_dispatcher", text: "hi", idempotencyKey: "st-1" },
  });
  assert.equal(sent.body.ok, true);

  const response = await callRoute(route, {
    method: "status",
    params: { limit: 5 },
  });
  assert.equal(response.status, 200);
  assert.deepEqual(response.body.result.compaction, {
    at: "05:15",
    afterTurns: 12,
    atContextPercent: 65,
    dispatcherThreadId: "thr_dispatcher",
  });
  const row = response.body.result.threads.find(
    (item) => item.threadId === "thr_dispatcher",
  );
  assert.equal(row.compact.turnsSinceCompaction, 1);
  assert.equal(row.compact.lastCompactionAt, null);
  assert.equal(row.compact.lastCompactionStatus, null);
  assert.equal(row.compact.pendingReminder, false);
  assert.equal(row.compact.compactionPending, false);
});

// -------------------------------------------------------------- updates --

function pushTurn(world, threadId, initiator, spoken, text) {
  let counter = (pushTurn.counters ??= new Map());
  const next = (counter.get(threadId) ?? 0) + 1;
  counter.set(threadId, next);
  const request = world.pushEvent(threadId, "client/turn/requested", {
    requestId: `req_updates_${next}`,
    target: { kind: "new-turn" },
    initiator,
  });
  if (text !== undefined) {
    world.pushEvent(threadId, "item/completed", {
      item: { type: "agentMessage", text },
    });
  } else if (spoken !== null) {
    world.pushEvent(threadId, "item/completed", {
      item: { type: "agentMessage", text: `Ответ.\n🔊 ${spoken}` },
    });
  } else {
    world.pushEvent(threadId, "item/completed", {
      item: { type: "agentMessage", text: "Ответ без озвучки." },
    });
  }
  const completion = world.pushEvent(threadId, "turn/completed", {
    status: "completed",
  });
  return { request, completion };
}

test("updates returns system and agent turns, never owner turns", async () => {
  const { route, world } = await makePlugin();
  const owner = pushTurn(world, "thr_dispatcher", "user", "Владелец.");
  const system = pushTurn(world, "thr_dispatcher", "system", "Система.");
  const agent = pushTurn(world, "thr_dispatcher", "agent", "Агент.");
  const ownerSilent = pushTurn(world, "thr_dispatcher", "user", null);
  const systemSilent = pushTurn(world, "thr_dispatcher", "system", null);

  const response = await callRoute(route, {
    method: "updates",
    params: { afterSeq: 0, waitSeconds: 5 },
  });
  assert.equal(response.status, 200);
  assert.equal(response.body.ok, true);
  const items = response.body.result.items;
  assert.deepEqual(
    items.map((item) => item.seq),
    [system.completion.seq, agent.completion.seq],
  );
  assert.deepEqual(
    items.map((item) => item.spoken),
    ["Система.", "Агент."],
  );
  assert.ok(items.every((item) => item.seq !== owner.completion.seq));
  assert.ok(items.every((item) => item.seq !== ownerSilent.completion.seq));
  assert.ok(
    items.every(
      (item) =>
        typeof item.completedAt === "number" && item.completedAt > 0,
    ),
    "every updates item must carry its completion time for the voice TTL",
  );
  assert.equal(response.body.result.cursor, systemSilent.completion.seq);
});

test("updates bootstrap returns the cursor and no history", async () => {
  const { route, world } = await makePlugin();
  pushTurn(world, "thr_dispatcher", "system", "Старое обновление.");
  const last = world.pushEvent("thr_dispatcher", "turn/started", {});

  const response = await callRoute(route, { method: "updates", params: {} });
  assert.equal(response.status, 200);
  assert.deepEqual(response.body.result.items, []);
  assert.equal(response.body.result.cursor, last.seq);
});

test("updates long-polls, advances the cursor and skips a silent turn", async () => {
  const world = makeWorld();
  addDispatcherThread(world, "idle");
  pushTurn(world, "thr_dispatcher", "user", null);
  const clock = { value: 1000 };
  const delays = [];
  const { dispatcher } = makeCompactionDispatcher({
    world,
    nowRef: clock,
    delay: async (ms) => {
      delays.push(ms);
      clock.value += ms;
    },
  });

  const result = await dispatcher.call("updates", {
    afterSeq: 0,
    waitSeconds: 4,
  });
  assert.deepEqual(result.items, []);
  assert.deepEqual(delays, [1000, 1000, 1000, 1000]);
  const cursor = result.cursor;
  assert.ok(cursor > 0);

  // The skipped turn is before the cursor, so a follow-up call returns at
  // once instead of rescanning it for the whole wait window.
  delays.length = 0;
  const repeat = await dispatcher.call("updates", {
    afterSeq: cursor,
    waitSeconds: 4,
  });
  assert.deepEqual(repeat.items, []);
  assert.equal(repeat.cursor, cursor);
  assert.deepEqual(delays, [1000, 1000, 1000, 1000]);
});

test("updates refuses a plugin without a configured dispatcher", async () => {
  const { route } = await makePlugin({ settings: { dispatcherThreadId: "" } });
  const response = await callRoute(route, { method: "updates", params: {} });
  assert.equal(response.status, 200);
  assert.equal(response.body.ok, false);
  assert.equal(response.body.code, "dispatcher_not_configured");
});
