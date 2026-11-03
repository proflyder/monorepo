import { DispatchError } from "./errors.js";
import {
  nullableStringList,
  profilePolicyOutputSchema,
  profileProjectsOutputSchema,
} from "./schema.js";

const ACTIVE_STATUSES = new Set(["active", "starting", "pending"]);
const RUNNING_STATUSES = new Set(["active", "starting", "pending", "stopping"]);

const VOICE_ANSWER_RULE =
  "Если ждёшь ответа владельца, 🔊-строка заканчивается вопросом со знаком «?»; " +
  "если ответ не нужен — без вопросительного знака в конце.";

const SPAWN_INSTRUCTION =
  "В конце каждого ответа добавляй отдельной строкой: " +
  "🔊 <1–2 фразы для озвучивания по-русски>. " +
  VOICE_ANSWER_RULE;

const DEFAULT_COMPACT_AT = "04:30";
const DEFAULT_COMPACT_AFTER_TURNS = 30;
const DEFAULT_COMPACT_AT_CONTEXT_PERCENT = 70;
const DEFAULT_DISPATCHER_REMINDER =
  "Напоминание: ты рабочий голосовой диспетчер; отвечай коротко по-русски " +
  "и заканчивай строкой 🔊 с 1–2 фразами для озвучивания. " +
  VOICE_ANSWER_RULE;
// A requested compaction that never produces `thread/compacted` must not keep
// sends in queue mode forever.
const COMPACTION_PENDING_TIMEOUT_MS = 15 * 60 * 1000;
// Do not compact again right after a compaction: the recorded usage can still
// describe the pre-compaction context until the provider reports a new one.
const COMPACTION_COOLDOWN_MS = 10 * 60 * 1000;
// The plugin schedule fires on every server scheduler tick. Back off from
// repeated thread lookups while busy or waiting for the compaction event.
const NIGHTLY_BUSY_RETRY_MS = 60 * 1000;
const NIGHTLY_REFRESH_MS = 30 * 1000;
// `updates` scans completed dispatcher turns in a bounded loop. Skipped turns
// (owner turns, turns without a 🔊 line) only advance the cursor.
const UPDATES_POLL_INTERVAL_MS = 1000;
const UPDATES_MAX_CYCLES = 5000;

export function createDispatcher({
  getSdk,
  kv,
  pluginId,
  getSettings,
  log = { info() {}, warn() {}, error() {}, debug() {} },
  now = () => Date.now(),
  delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
  sendWaitMs = 3000,
}) {
  const sdk = () => {
    const value = getSdk();
    if (value === undefined || value === null) {
      throw new DispatchError(
        "server_unavailable",
        "Сервер bb ещё не готов.",
      );
    }
    return value;
  };

  // Idempotency keys must serialize even when two HTTP requests arrive
  // together: bb may deliver them concurrently, and a check-then-spawn race
  // would otherwise create two threads for one key.
  const locks = new Map();
  // In-process backoff for the minute-level schedule; durability comes from
  // the daily kv marker, not from these timestamps.
  let nextNightlyAttemptAt = 0;
  let nextNightlyRefreshAt = 0;
  async function withLock(key, run) {
    const previous = locks.get(key) ?? Promise.resolve();
    let release;
    const current = new Promise((resolve) => {
      release = resolve;
    });
    locks.set(key, previous.then(() => current));
    const tail = locks.get(key);
    await previous.catch(() => {});
    try {
      return await run();
    } finally {
      release();
      if (locks.get(key) === tail) locks.delete(key);
    }
  }

  async function readSettings() {
    const raw = (await getSettings()) ?? {};
    return {
      hmacSecret: raw.hmacSecret,
      workerLimit: clampLimit(raw.workerLimit),
      permissionMode: normalizePermissionMode(raw.permissionMode),
      ...compactionSettingsFrom(raw),
    };
  }

  // `status` must stay readable even when another setting is invalid, so it
  // reads the compaction fields without normalizing permissionMode.
  async function readCompactionSettings() {
    return compactionSettingsFrom((await getSettings()) ?? {});
  }

  // ---------------------------------------------------------- compaction --

  function compactStateKey(threadId) {
    return `compact:${threadId}`;
  }

  async function readCompactState(threadId) {
    const stored = await kv.get(compactStateKey(threadId));
    const state = isPlainObject(stored) ? stored : {};
    return {
      turns:
        Number.isSafeInteger(state.turns) && state.turns >= 0 ? state.turns : 0,
      pendingReminder: state.pendingReminder === true,
      lastCompactionAt:
        typeof state.lastCompactionAt === "number"
          ? state.lastCompactionAt
          : null,
      lastCompactionReason:
        typeof state.lastCompactionReason === "string"
          ? state.lastCompactionReason
          : null,
      lastCompactionStatus:
        typeof state.lastCompactionStatus === "string"
          ? state.lastCompactionStatus
          : null,
      pending:
        isPlainObject(state.pending) &&
        Number.isSafeInteger(state.pending.afterSeq) &&
        typeof state.pending.requestedAt === "number"
          ? {
              afterSeq: state.pending.afterSeq,
              requestedAt: state.pending.requestedAt,
              reason:
                typeof state.pending.reason === "string"
                  ? state.pending.reason
                  : "nightly",
            }
          : null,
    };
  }

  async function writeCompactState(threadId, state) {
    await kv.set(compactStateKey(threadId), state);
  }

  // bb records a completed manual compaction as `thread/compacted`. Looking it
  // up lazily avoids holding the nightly schedule open for the whole turn.
  async function refreshCompactionState(threadId, state, settings) {
    if (state.pending === null) return state;
    const pending = state.pending;
    if (now() - pending.requestedAt > COMPACTION_PENDING_TIMEOUT_MS) {
      state.pending = null;
      state.lastCompactionStatus = "failed";
      state.lastCompactionReason = pending.reason;
      await writeCompactState(threadId, state);
      return state;
    }
    try {
      const rows = await sdk().threads.events.list({
        threadId,
        afterSeq: String(pending.afterSeq),
        order: "asc",
        limit: "5",
        types: ["thread/compacted"],
      });
      if (Array.isArray(rows) && rows.length > 0) {
        state.pending = null;
        state.turns = 0;
        state.pendingReminder = settings.dispatcherReminder !== "";
        state.lastCompactionAt = now();
        state.lastCompactionStatus = "completed";
        state.lastCompactionReason = pending.reason;
        await writeCompactState(threadId, state);
      }
    } catch (cause) {
      log.warn(
        `compaction state unavailable for ${threadId}: ${messageOf(cause)}`,
      );
    }
    return state;
  }

  async function contextPercent(threadId) {
    if (typeof sdk().threads.context !== "function") return null;
    try {
      const response = await sdk().threads.context({ threadId });
      const usage = response?.usage;
      if (
        !isPlainObject(usage) ||
        !Number.isFinite(usage.usedTokens) ||
        !Number.isFinite(usage.modelContextWindow) ||
        usage.modelContextWindow <= 0
      ) {
        return null;
      }
      return (usage.usedTokens / usage.modelContextWindow) * 100;
    } catch {
      return null;
    }
  }

  async function idleForCompaction(thread) {
    if (thread.status !== "idle" && thread.status !== "error") return false;
    return !(await hasPendingInteraction(thread.id));
  }

  function reminderOnce(state, settings, compactFirst) {
    if (settings.dispatcherReminder === "") {
      // Disabling the reminder still consumes a pending one.
      return { text: "", used: state.pendingReminder };
    }
    // A compact-first send runs the compaction before delivering this very
    // message, so the reminder belongs here. Otherwise it rides the first send
    // after a nightly compaction completed.
    if (!compactFirst && !state.pendingReminder) {
      return { text: "", used: false };
    }
    return { text: settings.dispatcherReminder, used: true };
  }

  function appendReminder(text, reminder) {
    return reminder === "" ? text : `${text}\n\n${reminder}`;
  }

  // One schedule tick: run at most once per local calendar day, after
  // `compactAt`. A busy dispatcher is retried on the next tick, and a server
  // that was off overnight catches up on its first tick of the day.
  async function backgroundTick() {
    const settings = await readCompactionSettings();
    if (settings.compactAt === "" || settings.dispatcherThreadId === "") {
      return { ran: false, reason: "not_configured" };
    }
    const current = new Date(now());
    if (!nightlyDue(current, settings.compactAt)) {
      return { ran: false, reason: "not_due" };
    }
    if (now() < nextNightlyAttemptAt) {
      return { ran: false, reason: "throttled" };
    }
    const threadId = settings.dispatcherThreadId;
    const markerKey = `nightly:${localDateKey(current)}`;
    const marker = await kv.get(markerKey);
    if (isPlainObject(marker) && marker.status === "requested") {
      if (now() >= nextNightlyRefreshAt) {
        nextNightlyRefreshAt = now() + NIGHTLY_REFRESH_MS;
        const state = await readCompactState(threadId);
        await refreshCompactionState(threadId, state, settings);
      }
      return { ran: false, reason: "already_requested", threadId };
    }
    if (isPlainObject(marker) && marker.status === "failed") {
      return { ran: false, reason: "failed_today", threadId };
    }

    const state = await readCompactState(threadId);
    await refreshCompactionState(threadId, state, settings);
    const dueSince = startOfClock(current, settings.compactAt);
    if (
      state.lastCompactionAt !== null &&
      state.lastCompactionAt >= dueSince.getTime()
    ) {
      await kv.set(markerKey, { status: "ok", at: now(), reason: "recent" });
      return { ran: false, reason: "recent_compaction", threadId };
    }

    let thread;
    try {
      thread = await getThread(threadId);
    } catch {
      nextNightlyAttemptAt = now() + NIGHTLY_BUSY_RETRY_MS;
      return { ran: false, reason: "thread_not_found", threadId };
    }
    if (!(await idleForCompaction(thread))) {
      nextNightlyAttemptAt = now() + NIGHTLY_BUSY_RETRY_MS;
      return { ran: false, reason: "busy", threadId };
    }

    let afterSeq;
    try {
      afterSeq = await lastEventSeq(threadId);
      await sdk().threads.compact({ threadId });
    } catch (cause) {
      log.warn(`nightly compaction refused for ${threadId}: ${messageOf(cause)}`);
      await kv.set(markerKey, {
        status: "failed",
        at: now(),
        error: messageOf(cause),
      });
      nextNightlyAttemptAt = now() + NIGHTLY_BUSY_RETRY_MS;
      return { ran: false, reason: "compaction_failed", threadId };
    }
    state.pending = { afterSeq, requestedAt: now(), reason: "nightly" };
    state.lastCompactionReason = "nightly";
    state.lastCompactionStatus = "requested";
    await writeCompactState(threadId, state);
    await kv.set(markerKey, { status: "requested", at: now() });
    return {
      ran: true,
      threadId,
      compactAt: settings.compactAt,
      afterSeq,
    };
  }

  async function call(method, params) {
    switch (method) {
      case "targets":
        return targets();
      case "status":
        return status(params);
      case "validate_spawn":
        return validateSpawn(params);
      case "spawn":
        return spawn(params);
      case "send":
        return send(params);
      case "turn_result":
        return turnResult(params);
      case "updates":
        return updates(params);
      default:
        throw new DispatchError("unknown_method", "Неизвестная операция.", {
          httpStatus: 400,
        });
    }
  }

  // ------------------------------------------------------------- profiles --

  async function readProfileProjects() {
    try {
      const response = await sdk().plugins.callRpc({
        pluginId: "profiles",
        method: "listProjects",
        input: null,
        outputSchema: profileProjectsOutputSchema,
      });
      if (!isPlainObject(response) || !Array.isArray(response.projects)) {
        throw new Error("profiles RPC returned an unexpected shape");
      }
      return response.projects
        .filter(
          (row) =>
            isPlainObject(row) &&
            typeof row.id === "string" &&
            typeof row.name === "string" &&
            typeof row.profile === "string",
        )
        .map((row) => ({
          projectId: row.id,
          name: row.name,
          profile: row.profile,
        }));
    } catch (cause) {
      log.warn(`profiles RPC unavailable: ${messageOf(cause)}`);
      throw new DispatchError(
        "profiles_unavailable",
        "Плагин профилей недоступен, профиль проекта неизвестен.",
      );
    }
  }

  async function readProfilePolicy(profile) {
    try {
      const response = await sdk().plugins.callRpc({
        pluginId: "profiles",
        method: "profilePolicy",
        input: { profile },
        outputSchema: profilePolicyOutputSchema,
      });
      if (
        !isPlainObject(response) ||
        !nullableStringList(response.providers) ||
        !nullableStringList(response.machines)
      ) {
        throw new Error("profiles RPC returned an unexpected policy");
      }
      return {
        providers: response.providers ?? null,
        machines: response.machines ?? null,
      };
    } catch (cause) {
      log.warn(`profiles policy unavailable: ${messageOf(cause)}`);
      if (isNotFound(cause)) {
        throw new DispatchError(
          "profile_not_found",
          `Профиль «${profile}» не найден в плагине профилей.`,
        );
      }
      throw new DispatchError(
        "profiles_unavailable",
        "Плагин профилей недоступен, политика профиля неизвестна.",
      );
    }
  }

  // One profilePolicy call per profile per request, even though `targets`
  // lists several projects of the same profile.
  async function policyForProfile(profile, cache) {
    if (cache.has(profile)) return cache.get(profile);
    const policy = await readProfilePolicy(profile);
    cache.set(profile, policy);
    return policy;
  }

  async function readProjects() {
    const rows = await sdk().projects.list({ includePersonal: true });
    return Array.isArray(rows) ? rows : [];
  }

  async function readHosts() {
    const rows = await sdk().hosts.list();
    return Array.isArray(rows) ? rows : [];
  }

  function sourceOf(project) {
    const sources = Array.isArray(project.sources) ? project.sources : [];
    if (sources.length === 0) return null;
    return sources.find((source) => source.isDefault === true) ?? sources[0];
  }

  function machineOf(project, hosts) {
    const source = sourceOf(project);
    if (source === null) return null;
    const host = hosts.get(source.hostId);
    return {
      hostId: source.hostId,
      name: host?.name ?? source.hostId,
      status: host?.status ?? "disconnected",
      path: source.path,
      online: host?.status === "connected",
    };
  }

  async function resolveTarget(params) {
    const [profiles, projects, hosts] = await Promise.all([
      readProfileProjects(),
      readProjects(),
      readHosts(),
    ]);
    const profileRow = profiles.find((row) => row.projectId === params.projectId);
    if (profileRow === undefined) {
      throw new DispatchError(
        "project_not_found",
        "Проект не найден в профилях bb.",
      );
    }
    if (profileRow.profile !== params.profile) {
      throw new DispatchError(
        "profile_mismatch",
        `Проект относится к профилю «${profileRow.profile}», а не «${params.profile}».`,
      );
    }
    const project = projects.find((row) => row.id === params.projectId);
    if (project === undefined) {
      throw new DispatchError("project_not_found", "Проект не найден.");
    }
    const hostMap = new Map(hosts.map((host) => [host.id, host]));
    const machine = machineOf(project, hostMap);
    if (machine === null) {
      throw new DispatchError(
        "machine_missing",
        "У проекта нет машины с исходниками.",
      );
    }
    if (!machine.online) {
      throw new DispatchError(
        "machine_offline",
        `Машина «${machine.name}» не в сети.`,
      );
    }
    return { profiles, profileRow, project, machine, hosts: hostMap };
  }

  async function providerModelsFor({
    projectId,
    hostId,
    profile,
    providers: allowed,
    cache,
  }) {
    const key = `${projectId}\u0000${hostId}\u0000${profile}`;
    if (cache.has(key)) return cache.get(key);
    const promise = (async () => {
      const [catalog, available] = await Promise.all([
        sdk().providers.catalog(),
        sdk().providers.list({ hostId }),
      ]);
      const availableIds = new Set(
        (Array.isArray(available) ? available : [])
          .filter((row) => row?.available !== false)
          .map((row) => row.id),
      );
      const providers = (Array.isArray(catalog) ? catalog : [])
        .filter(
          (row) =>
            row?.enabled === true &&
            availableIds.has(row.id) &&
            (allowed === null || allowed.includes(row.id)),
        )
        .map((row) => ({ id: row.id, displayName: row.displayName ?? row.id }));

      const result = [];
      for (const provider of providers) {
        let models = [];
        let modelLoadError = null;
        try {
          const response = await sdk().providers.models({
            hostId,
            projectId,
            providerId: provider.id,
          });
          const providerInfo = (response?.providers ?? []).find(
            (row) => row?.id === provider.id,
          );
          const providerLevels = (providerInfo?.reasoningLevels ?? []).map(
            (level) => level.id,
          );
          models = (response?.models ?? []).map((model) => {
            const levels = (model.supportedReasoningEfforts ?? [])
              .map((effort) => effort.reasoningEffort)
              .filter((value) => typeof value === "string");
            return {
              model: model.id ?? model.model,
              displayName: model.displayName ?? model.id ?? model.model,
              defaultReasoningLevel: model.defaultReasoningEffort ?? null,
              reasoningLevels: levels.length > 0 ? levels : providerLevels,
            };
          });
          if (response?.modelLoadError) {
            modelLoadError = response.modelLoadError.code ?? "failed";
          }
        } catch (cause) {
          modelLoadError = "failed";
          log.warn(`provider models failed for ${provider.id}: ${messageOf(cause)}`);
        }
        result.push({
          providerId: provider.id,
          displayName: provider.displayName,
          models,
          ...(modelLoadError === null ? {} : { modelLoadError }),
        });
      }
      return result;
    })();
    cache.set(key, promise);
    return promise;
  }

  // -------------------------------------------------------------- methods --

  async function targets() {
    const [profiles, projects, hosts] = await Promise.all([
      readProfileProjects(),
      readProjects(),
      readHosts(),
    ]);
    const hostMap = new Map(hosts.map((host) => [host.id, host]));
    const projectMap = new Map(projects.map((project) => [project.id, project]));
    const cache = new Map();
    const policyCache = new Map();
    const rows = [];
    for (const profileProject of profiles) {
      const project = projectMap.get(profileProject.projectId);
      if (project === undefined) continue;
      const machine = machineOf(project, hostMap);
      const policy = await policyForProfile(profileProject.profile, policyCache);
      const providers =
        machine === null
          ? []
          : await providerModelsFor({
              projectId: project.id,
              hostId: machine.hostId,
              profile: profileProject.profile,
              providers: policy.providers,
              cache,
            });
      rows.push({
        projectId: project.id,
        name: profileProject.name,
        profile: profileProject.profile,
        policy,
        machine:
          machine === null
            ? null
            : {
                hostId: machine.hostId,
                name: machine.name,
                status: machine.status,
                online: machine.online,
              },
        providers,
      });
    }
    rows.sort((a, b) => a.name.localeCompare(b.name, "ru"));
    return {
      projects: rows,
      speech: `Доступно проектов: ${rows.length}.`,
    };
  }

  async function status(params) {
    const [threads, projects, hosts] = await Promise.all([
      sdk().threads.list({ includeHidden: true, limit: 200 }),
      readProjects().catch(() => []),
      readHosts().catch(() => []),
    ]);
    let profileMap = new Map();
    try {
      profileMap = new Map(
        (await readProfileProjects()).map((row) => [
          row.projectId,
          row.profile,
        ]),
      );
    } catch {
      // Status stays useful while the profiles plugin is unavailable.
    }
    const hostMap = new Map(hosts.map((host) => [host.id, host]));
    const projectMap = new Map(projects.map((project) => [project.id, project]));
    const rows = (Array.isArray(threads) ? threads : [])
      .slice()
      .sort((a, b) => {
        const rank = (thread) => (ACTIVE_STATUSES.has(thread.status) ? 0 : 1);
        return rank(a) - rank(b) || (b.updatedAt ?? 0) - (a.updatedAt ?? 0);
      })
      .slice(0, params.limit);

    const compactionSettings = await readCompactionSettings();
    const result = [];
    for (const thread of rows) {
      const project = projectMap.get(thread.projectId);
      const hostId =
        thread.environmentHostId ?? (project ? sourceOf(project)?.hostId : null);
      const host = hostId === undefined || hostId === null ? null : hostMap.get(hostId);
      let runningSeconds = null;
      if (ACTIVE_STATUSES.has(thread.status)) {
        const started = await lastTurnStartedAt(thread.id);
        runningSeconds = Math.max(
          0,
          Math.floor((now() - (started ?? thread.createdAt ?? now())) / 1000),
        );
      }
      result.push({
        threadId: thread.id,
        title: thread.title ?? thread.titleFallback ?? null,
        projectId: thread.projectId,
        projectName: project?.name ?? null,
        profile: profileMap.get(thread.projectId) ?? null,
        machine:
          hostId === undefined || hostId === null
            ? null
            : {
                hostId,
                name: host?.name ?? hostId,
                status: host?.status ?? "disconnected",
              },
        status: thread.status,
        runtimeSeconds: runningSeconds,
        updatedAt: thread.updatedAt ?? null,
        compact: compactSummary(await readCompactState(thread.id)),
      });
    }
    const running = result.filter((row) => ACTIVE_STATUSES.has(row.status)).length;
    return {
      threads: result,
      limit: params.limit,
      compaction: {
        at: compactionSettings.compactAt,
        afterTurns: compactionSettings.compactAfterTurns,
        atContextPercent: compactionSettings.compactAtContextPercent,
        dispatcherThreadId: compactionSettings.dispatcherThreadId,
      },
      speech:
        running === 0
          ? "Работающих тредов нет."
          : `Работают тредов: ${running}.`,
    };
  }

  async function lastTurnStartedAt(threadId) {
    try {
      const rows = await sdk().threads.events.list({
        threadId,
        order: "desc",
        limit: "1",
        types: ["turn/started"],
      });
      return Array.isArray(rows) && rows.length > 0 ? rows[0].createdAt : null;
    } catch {
      return null;
    }
  }

  async function validateSpawn(params) {
    const settings = await readSettings();
    const target = await resolveTarget(params);
    const policy = await readProfilePolicy(params.profile);
    if (
      policy.providers !== null &&
      !policy.providers.includes(params.providerId)
    ) {
      throw new DispatchError(
        "provider_not_allowed",
        `Провайдер «${params.providerId}» не разрешён профилю «${params.profile}».`,
      );
    }
    if (
      policy.machines !== null &&
      !policy.machines.includes(target.machine.hostId)
    ) {
      throw new DispatchError(
        "machine_not_allowed",
        `Машина «${target.machine.name}» не разрешена профилю «${params.profile}».`,
      );
    }
    const providers = await providerModelsFor({
      projectId: params.projectId,
      hostId: target.machine.hostId,
      profile: params.profile,
      providers: policy.providers,
      cache: new Map(),
    });
    const provider = providers.find(
      (row) => row.providerId === params.providerId,
    );
    if (provider === undefined) {
      throw new DispatchError(
        "provider_unavailable",
        `Провайдер «${params.providerId}» недоступен на машине.`,
      );
    }
    if (provider.modelLoadError !== undefined && provider.models.length === 0) {
      throw new DispatchError(
        "provider_unavailable",
        `Провайдер «${params.providerId}» не отдаёт список моделей.`,
      );
    }
    const model = provider.models.find((row) => row.model === params.model);
    if (model === undefined) {
      throw new DispatchError(
        "model_not_found",
        `Модель «${params.model}» недоступна у провайдера «${params.providerId}».`,
      );
    }
    if (!reasoningSupported(model, params.reasoningLevel)) {
      throw new DispatchError(
        "reasoning_not_supported",
        model.reasoningLevels.length === 0
          ? `Модель «${params.model}» не принимает уровень reasoning.`
          : `Модель «${params.model}» не поддерживает уровень «${params.reasoningLevel}».`,
      );
    }
    const active = await countActiveThreads();
    if (active >= settings.workerLimit) {
      throw new DispatchError(
        "thread_limit",
        `Достигнут лимит одновременных тредов: ${settings.workerLimit}.`,
      );
    }
    return {
      projectId: params.projectId,
      profile: params.profile,
      providerId: params.providerId,
      model: params.model,
      reasoningLevel: params.reasoningLevel,
      machine: {
        hostId: target.machine.hostId,
        name: target.machine.name,
        status: target.machine.status,
      },
      activeThreads: active,
      limit: settings.workerLimit,
      speech: "Можно запускать.",
    };
  }

  async function countActiveThreads() {
    let offset = 0;
    let active = 0;
    for (;;) {
      const rows = await sdk().threads.list({
        originPluginId: pluginId,
        includeHidden: true,
        limit: 200,
        offset,
      });
      const page = Array.isArray(rows) ? rows : [];
      active += page.filter((row) => ACTIVE_STATUSES.has(row.status)).length;
      if (page.length < 200 || offset >= 800) break;
      offset += 200;
    }
    return active;
  }

  async function spawn(params) {
    const fingerprint = fingerprintOf("spawn", params);
    return withLock(`spawn:${params.idempotencyKey}`, async () => {
      const settings = await readSettings();
      const existing = await readIdempotent(
        params.idempotencyKey,
        fingerprint,
      );
      if (existing !== null) {
        return idempotentSpawnResult(existing);
      }
      const target = await validateSpawn(params);
      let thread;
      try {
        thread = await sdk().threads.spawn({
          projectId: params.projectId,
          environment: { type: "project-default" },
          providerId: params.providerId,
          model: params.model,
          reasoningLevel: params.reasoningLevel,
          permissionMode: settings.permissionMode,
          title: params.title,
          prompt: `${params.prompt}\n\n---\n${SPAWN_INSTRUCTION}`,
          visibility: "visible",
          pluginMetadata: {
            dispatcher: pluginId,
            idempotencyKey: params.idempotencyKey,
            profile: params.profile,
            projectId: params.projectId,
          },
        });
      } catch (cause) {
        log.error(`spawn failed: ${messageOf(cause)}`);
        throw new DispatchError("spawn_failed", "Не удалось создать тред.");
      }
      const threadId = thread?.id ?? thread?.threadId;
      if (typeof threadId !== "string" || threadId.length === 0) {
        throw new DispatchError(
          "spawn_failed",
          "bb не вернул идентификатор треда.",
        );
      }
      await kv.set(`thread:${threadId}`, {
        threadId,
        title: params.title,
        projectId: params.projectId,
        profile: params.profile,
        createdAt: now(),
        idempotencyKey: params.idempotencyKey,
      });
      await writeIdempotent(params.idempotencyKey, {
        method: "spawn",
        fingerprint,
        threadId,
        projectId: params.projectId,
        profile: params.profile,
        createdAt: now(),
      });
      log.info(
        `spawn ${threadId} project=${params.projectId} profile=${params.profile}`,
      );
      return {
        threadId,
        title: params.title,
        projectId: params.projectId,
        profile: params.profile,
        machine: {
          hostId: target.machine.hostId,
          name: target.machine.name,
          status: target.machine.status,
        },
        ref: `thread:${threadId}`,
        speech: `Запустил тред «${truncate(params.title, 80)}».`,
      };
    });
  }

  async function send(params) {
    const settings = await readSettings();
    const fingerprint = fingerprintOf("send", params);
    return withLock(`send:${params.idempotencyKey}`, async () => {
      const existing = await readIdempotent(
        params.idempotencyKey,
        fingerprint,
      );
      if (existing !== null) {
        return {
          threadId: existing.threadId,
          afterSeq: existing.afterSeq ?? 0,
          requestSeq: existing.requestSeq ?? null,
          requestId: existing.requestId ?? null,
          delivery: existing.delivery ?? "sent",
          ref: `thread:${existing.threadId}`,
          idempotent: true,
          speech: "Сообщение уже отправлено.",
        };
      }
      const thread = await getThread(params.threadId);
      const owned =
        params.threadId === settings.dispatcherThreadId ||
        (await kv.get(`thread:${params.threadId}`)) !== undefined ||
        thread.originPluginId === pluginId;
      if (!owned) {
        throw new DispatchError(
          "thread_not_allowed",
          "Писать можно только в треды диспетчера.",
        );
      }
      // Compaction is decided before the send and persisted after it: a
      // failed send must not advance the counter or consume the reminder.
      const state = await readCompactState(params.threadId);
      await refreshCompactionState(params.threadId, state, settings);

      const idle = await idleForCompaction(thread);
      const recentlyCompacted =
        state.lastCompactionAt !== null &&
        now() - state.lastCompactionAt < COMPACTION_COOLDOWN_MS;
      let compactFirst = false;
      let compactTrigger = null;
      if (idle && state.pending === null && !recentlyCompacted) {
        const percent = await contextPercent(params.threadId);
        if (percent !== null) {
          if (percent >= settings.compactAtContextPercent) {
            compactFirst = true;
            compactTrigger = "context";
          }
        } else if (state.turns >= settings.compactAfterTurns) {
          compactFirst = true;
          compactTrigger = "turns";
        }
      }

      // While a nightly compaction runs, a new prompt must not steer into it.
      const mode = state.pending === null ? "auto" : "queue";
      const reminder = reminderOnce(state, settings, compactFirst);
      const text = appendReminder(params.text, reminder.text);

      const afterSeq = await lastEventSeq(params.threadId);
      let response;
      try {
        response = await sdk().threads.send({
          threadId: params.threadId,
          mode,
          // The SDK rejects `compactFirst` together with `mode: "steer"`.
          ...(compactFirst ? { compactFirst: true } : {}),
          input: [{ type: "text", text, mentions: [] }],
        });
      } catch (cause) {
        log.error(`send failed for ${params.threadId}: ${messageOf(cause)}`);
        throw new DispatchError("send_failed", "Не удалось отправить сообщение.");
      }

      state.turns += 1;
      if (reminder.used) state.pendingReminder = false;
      if (compactFirst) {
        state.turns = 0;
        state.pendingReminder = false;
        state.pending = null;
        state.lastCompactionAt = now();
        state.lastCompactionReason = compactTrigger;
        state.lastCompactionStatus = "requested";
      }
      await writeCompactState(params.threadId, state);

      const request = await waitForTurnRequest(params.threadId, afterSeq);
      const result = {
        threadId: params.threadId,
        afterSeq,
        requestSeq: request?.seq ?? null,
        requestId: request?.data?.requestId ?? null,
        delivery: response?.delivery ?? "sent",
        compactFirst,
        compactTrigger,
        ...(reminder.used ? { reminder: true } : {}),
        ref: `thread:${params.threadId}`,
        speech: "Передал агенту.",
      };
      await writeIdempotent(params.idempotencyKey, {
        method: "send",
        fingerprint,
        threadId: params.threadId,
        afterSeq: result.afterSeq,
        requestSeq: result.requestSeq,
        requestId: result.requestId,
        delivery: result.delivery,
        createdAt: now(),
      });
      log.info(
        `send ${params.threadId} seq=${result.afterSeq} request=${result.requestId ?? "pending"}`,
      );
      return result;
    });
  }

  async function waitForTurnRequest(threadId, afterSeq) {
    const deadline = now() + sendWaitMs;
    for (;;) {
      const rows = await sdk().threads.events.list({
        threadId,
        afterSeq: String(afterSeq),
        order: "asc",
        limit: "20",
        types: ["client/turn/requested"],
      });
      if (Array.isArray(rows) && rows.length > 0) return rows[0];
      if (now() >= deadline) return null;
      await delay(100);
    }
  }

  async function turnResult(params) {
    await getThread(params.threadId);
    const request = await firstEvent(params.threadId, params.afterSeq, [
      "client/turn/requested",
    ]);
    const pending = await hasPendingInteraction(params.threadId);
    const thread = await getThread(params.threadId);

    if (request === undefined) {
      if (pending) return waitingInputResult(params.threadId, "Ждёт ответа.");
      if (thread.status === "error") return failedResult(params.threadId, null);
      if (RUNNING_STATUSES.has(thread.status)) {
        return runningResult(params.threadId, "Выполняется.");
      }
      return {
        threadId: params.threadId,
        ref: `thread:${params.threadId}`,
        status: "idle",
        speech: "Ход ещё не начался.",
        spoken: null,
        text: null,
      };
    }

    // The completion is looked up by seq range instead of scanning a fixed
    // event window: a long turn can produce thousands of item events and must
    // still correlate exactly.
    const completion = await firstEvent(params.threadId, request.seq, [
      "turn/completed",
    ]);

    if (completion === undefined) {
      if (pending) return waitingInputResult(params.threadId, "Ждёт ответа.");
      if (thread.status === "error") return failedResult(params.threadId, request);
      if (RUNNING_STATUSES.has(thread.status)) {
        return runningResult(params.threadId, "Выполняется.", request);
      }
      return {
        ...runningResult(params.threadId, "Ход ещё не завершён.", request),
        status: "idle",
      };
    }

    const status = completion.data?.status;
    if (status === "failed") {
      const systemError = await lastEvent(
        params.threadId,
        request.seq,
        completion.seq,
        ["system/error"],
      );
      const message =
        completion.data?.error?.message ?? systemError?.data?.message;
      return failedResult(params.threadId, request, completion, message);
    }
    if (status === "interrupted") {
      return failedResult(
        params.threadId,
        request,
        completion,
        "Ход прерван.",
      );
    }

    const text =
      (await lastAgentMessage(
        params.threadId,
        request.seq,
        completion.seq,
      )) ?? (await safeOutput(params.threadId));
    const spoken = extractSpoken(text);
    const cleaned = cleanMarkdown(text);
    return {
      threadId: params.threadId,
      ref: `thread:${params.threadId}`,
      status: "idle",
      speech: spoken === null ? "Готово." : `Готово: ${truncate(spoken, 140)}`,
      spoken,
      text: cleaned === "" ? null : truncate(cleaned, 200),
      requestSeq: request.seq,
      completedSeq: completion.seq,
    };
  }

  // ------------------------------------------------ dispatcher updates --

  async function updates(params) {
    const settings = await readCompactionSettings();
    const threadId = settings.dispatcherThreadId;
    if (threadId === "") {
      throw new DispatchError(
        "dispatcher_not_configured",
        "Рабочий диспетчер не настроен.",
      );
    }
    // Bootstrap: report where the caller should start. Never replay history,
    // otherwise a fresh proxy would speak every old unanswered turn.
    if (params.afterSeq === undefined) {
      return {
        cursor: await lastEventSeq(threadId),
        items: [],
        speech: "Готово.",
      };
    }
    const deadline = now() + (params.waitSeconds ?? 0) * 1000;
    // The cursor is read before the scan and raised to the newest returned
    // item. A turn that completes during the scan is therefore picked up by
    // the next call instead of being skipped.
    let cursor = 0;
    for (let cycle = 0; cycle < UPDATES_MAX_CYCLES; cycle += 1) {
      cursor = await lastEventSeq(threadId);
      const items = await collectUpdates(threadId, params.afterSeq);
      if (items.length > 0) {
        for (const item of items) {
          if (item.seq > cursor) cursor = item.seq;
        }
        return { cursor, items, speech: "Готово." };
      }
      const remaining = deadline - now();
      if (remaining <= 0) break;
      await delay(Math.min(UPDATES_POLL_INTERVAL_MS, remaining));
    }
    // A skipped turn (owner turn, no 🔊 line) still advances the cursor, so
    // the next call starts after it instead of rescanning it forever.
    return { cursor, items: [], speech: "Готово." };
  }

  async function collectUpdates(threadId, afterSeq) {
    const rows = await sdk().threads.events.list({
      threadId,
      afterSeq: String(afterSeq),
      order: "asc",
      limit: "50",
      types: ["turn/completed"],
    });
    const items = [];
    for (const completion of Array.isArray(rows) ? rows : []) {
      // The request may precede afterSeq: it started before the caller's
      // cursor while the turn itself completed after it.
      const request = await lastEvent(threadId, 0, completion.seq, [
        "client/turn/requested",
      ]);
      if (request === undefined) continue;
      const initiator = request.data?.initiator;
      // Only `system` (child-completed etc.) and `agent` (another thread's
      // message) turns were not started by the owner. `user` covers both the
      // voice phrase and text typed in the UI, which must stay silent here.
      if (initiator !== "system" && initiator !== "agent") continue;
      const text = await lastAgentMessage(threadId, request.seq, completion.seq);
      const spoken = extractSpoken(text);
      if (spoken === null) continue;
      // Completion time lets the voice proxy drop answers older than its TTL
      // on a restart rescan instead of speaking them hours late.
      items.push({
        seq: completion.seq,
        spoken,
        completedAt: completion.createdAt,
      });
    }
    return items;
  }

  async function firstEvent(threadId, afterSeq, types) {
    const rows = await sdk().threads.events.list({
      threadId,
      afterSeq: String(afterSeq),
      order: "asc",
      limit: "1",
      types,
    });
    return Array.isArray(rows) ? rows[0] : undefined;
  }

  async function lastEvent(threadId, afterSeq, beforeSeq, types) {
    const rows = await sdk().threads.events.list({
      threadId,
      afterSeq: String(afterSeq),
      beforeSeq: String(beforeSeq),
      order: "desc",
      limit: "1",
      types,
    });
    return Array.isArray(rows) ? rows[0] : undefined;
  }

  async function lastAgentMessage(threadId, afterSeq, beforeSeq) {
    const rows = await sdk().threads.events.list({
      threadId,
      afterSeq: String(afterSeq),
      beforeSeq: String(beforeSeq),
      order: "desc",
      limit: "50",
      types: ["item/completed"],
    });
    for (const row of Array.isArray(rows) ? rows : []) {
      const item = row.data?.item;
      if (item?.type === "agentMessage" && typeof item.text === "string") {
        return item.text;
      }
    }
    return null;
  }

  async function getThread(threadId) {
    try {
      return await sdk().threads.get({ threadId });
    } catch (cause) {
      throw new DispatchError("thread_not_found", "Тред не найден.", {
        cause,
      });
    }
  }

  async function lastEventSeq(threadId) {
    try {
      const rows = await sdk().threads.events.list({
        threadId,
        order: "desc",
        limit: "1",
      });
      return Array.isArray(rows) && rows.length > 0 ? rows[0].seq : 0;
    } catch {
      return 0;
    }
  }

  async function hasPendingInteraction(threadId) {
    try {
      const rows = await sdk().threads.interactions.list({ threadId });
      return (
        Array.isArray(rows) &&
        rows.some((row) => row?.status === "pending")
      );
    } catch {
      return false;
    }
  }

  async function safeOutput(threadId) {
    try {
      const response = await sdk().threads.output({ threadId });
      return typeof response?.output === "string" ? response.output : "";
    } catch {
      return "";
    }
  }

  function runningResult(threadId, speech, request) {
    return {
      threadId,
      ref: `thread:${threadId}`,
      status: "running",
      speech,
      spoken: null,
      text: null,
      ...(request === undefined ? {} : { requestSeq: request.seq }),
    };
  }

  function waitingInputResult(threadId, speech) {
    return {
      threadId,
      ref: `thread:${threadId}`,
      status: "waiting_input",
      speech,
      spoken: null,
      text: null,
    };
  }

  function failedResult(threadId, request, completion, message) {
    const reason =
      message ?? completion?.data?.error?.message ?? "Ход завершился ошибкой.";
    return {
      threadId,
      ref: `thread:${threadId}`,
      status: "failed",
      speech: `Ошибка: ${truncate(reason, 140)}`,
      spoken: null,
      text: truncate(reason, 200),
      ...(request === undefined ? {} : { requestSeq: request.seq }),
      ...(completion === undefined ? {} : { completedSeq: completion.seq }),
    };
  }

  // ---------------------------------------------------------- idempotency --

  async function readIdempotent(key, fingerprint) {
    const stored = await kv.get(`idem:${key}`);
    if (stored === undefined || stored === null) return null;
    if (stored.fingerprint !== fingerprint) {
      throw new DispatchError(
        "idempotency_conflict",
        "Ключ идемпотентности уже использован с другими параметрами.",
      );
    }
    return stored;
  }

  async function writeIdempotent(key, value) {
    await kv.set(`idem:${key}`, value);
  }

  function idempotentSpawnResult(existing) {
    return {
      threadId: existing.threadId,
      projectId: existing.projectId ?? null,
      profile: existing.profile ?? null,
      ref: `thread:${existing.threadId}`,
      idempotent: true,
      speech: "Тред уже создан.",
    };
  }

  return { call, backgroundTick };
}

function compactSummary(state) {
  return {
    turnsSinceCompaction: state.turns,
    lastCompactionAt: state.lastCompactionAt,
    lastCompactionReason: state.lastCompactionReason,
    lastCompactionStatus: state.lastCompactionStatus,
    pendingReminder: state.pendingReminder,
    compactionPending: state.pending !== null,
  };
}

function compactionSettingsFrom(raw) {
  return {
    compactAt: normalizeClock(raw.compactAt),
    compactAfterTurns: clampInteger(
      raw.compactAfterTurns,
      1,
      500,
      DEFAULT_COMPACT_AFTER_TURNS,
    ),
    compactAtContextPercent: clampInteger(
      raw.compactAtContextPercent,
      1,
      100,
      DEFAULT_COMPACT_AT_CONTEXT_PERCENT,
    ),
    dispatcherReminder:
      typeof raw.dispatcherReminder === "string"
        ? raw.dispatcherReminder.trim()
        : DEFAULT_DISPATCHER_REMINDER,
    dispatcherThreadId:
      typeof raw.dispatcherThreadId === "string"
        ? raw.dispatcherThreadId.trim()
        : "",
  };
}

function normalizeClock(value) {
  if (value === undefined || value === null) return DEFAULT_COMPACT_AT;
  if (typeof value !== "string") return DEFAULT_COMPACT_AT;
  const text = value.trim();
  if (text === "") return "";
  return /^([01]\d|2[0-3]):[0-5]\d$/u.test(text) ? text : DEFAULT_COMPACT_AT;
}

function clampInteger(value, min, max, fallback) {
  const number = Number(value);
  if (!Number.isSafeInteger(number)) return fallback;
  return Math.min(max, Math.max(min, number));
}

function clockMinutes(value) {
  const [hours, minutes] = value.split(":").map((part) => Number(part));
  return hours * 60 + minutes;
}

function minutesOfDay(date) {
  return date.getHours() * 60 + date.getMinutes();
}

function nightlyDue(date, compactAt) {
  return minutesOfDay(date) >= clockMinutes(compactAt);
}

function startOfClock(date, compactAt) {
  const [hours, minutes] = compactAt.split(":").map((part) => Number(part));
  const start = new Date(date);
  start.setHours(hours, minutes, 0, 0);
  return start;
}

function localDateKey(date) {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

function reasoningSupported(model, level) {
  if (model.reasoningLevels.length === 0) {
    return level === model.defaultReasoningLevel;
  }
  return model.reasoningLevels.includes(level);
}

export function extractSpoken(text) {
  if (typeof text !== "string") return null;
  const lines = text.split(/\r?\n/u);
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    const match = lines[index].match(/🔊\s*(.+)$/u);
    if (match) {
      const value = match[1].replace(/^[:—-]\s*/u, "").trim();
      if (value !== "") return truncate(value, 280);
    }
  }
  return null;
}

export function cleanMarkdown(text) {
  if (typeof text !== "string") return "";
  return text
    .replace(/```[\s\S]*?```/gu, " ")
    .replace(/`([^`]*)`/gu, "$1")
    .replace(/!\[[^\]]*\]\([^)]*\)/gu, " ")
    .replace(/\[([^\]]*)\]\([^)]*\)/gu, "$1")
    .replace(/^#{1,6}\s+/gmu, "")
    .replace(/^>\s?/gmu, "")
    .replace(/^\s*[-*+]\s+/gmu, "")
    .replace(/(\*\*|__)(.*?)\1/gu, "$2")
    .replace(/(\*|_)(.*?)\1/gu, "$2")
    .replace(/\s+/gu, " ")
    .trim();
}

function truncate(text, max) {
  const value = String(text ?? "").trim();
  if (value.length <= max) return value;
  return `${value.slice(0, max - 1).trimEnd()}…`;
}

function clampLimit(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) return 2;
  return Math.min(20, Math.max(1, Math.trunc(number)));
}

function normalizePermissionMode(value) {
  if (value === undefined || value === null || value === "") return "auto";
  if (value === "auto" || value === "accept-edits") return value;
  throw new DispatchError(
    "bad_settings",
    "permissionMode допускает только auto или accept-edits.",
  );
}

function fingerprintOf(method, params) {
  return `${method}:${JSON.stringify(params)}`;
}

function isPlainObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function messageOf(cause) {
  return cause instanceof Error ? cause.message : String(cause);
}

// bb turns a plugin RPC refusal into an error carrying the plugin's code.
function isNotFound(cause) {
  return (
    isPlainObject(cause) &&
    (cause.code === "not_found" || cause.code === "profile_not_found")
  );
}
