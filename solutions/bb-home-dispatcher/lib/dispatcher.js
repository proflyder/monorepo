import { DispatchError } from "./errors.js";
import { anyOutputSchema } from "./schema.js";

const ACTIVE_STATUSES = new Set(["active", "starting", "pending"]);
const RUNNING_STATUSES = new Set(["active", "starting", "pending", "stopping"]);

const SPAWN_INSTRUCTION =
  "В конце каждого ответа добавляй отдельной строкой: " +
  "🔊 <1–2 фразы для озвучивания по-русски>.";

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
      dispatcherThreadId:
        typeof raw.dispatcherThreadId === "string"
          ? raw.dispatcherThreadId.trim()
          : "",
      profileProviders: parseProfileProviders(raw.profileProviders),
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
        outputSchema: anyOutputSchema,
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

  function allowedProviderIds(settings, profile) {
    if (settings.profileProviders === null) return null;
    const map = settings.profileProviders;
    if (Object.prototype.hasOwnProperty.call(map, profile)) {
      return map[profile];
    }
    if (Object.prototype.hasOwnProperty.call(map, "*")) {
      return map["*"];
    }
    return null;
  }

  async function providerModelsFor({
    projectId,
    hostId,
    profile,
    settings,
    cache,
  }) {
    const key = `${projectId}\u0000${hostId}\u0000${profile}`;
    if (cache.has(key)) return cache.get(key);
    const promise = (async () => {
      const allowed = allowedProviderIds(settings, profile);
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
    const settings = await readSettings();
    const [profiles, projects, hosts] = await Promise.all([
      readProfileProjects(),
      readProjects(),
      readHosts(),
    ]);
    const hostMap = new Map(hosts.map((host) => [host.id, host]));
    const projectMap = new Map(projects.map((project) => [project.id, project]));
    const cache = new Map();
    const rows = [];
    for (const profileProject of profiles) {
      const project = projectMap.get(profileProject.projectId);
      if (project === undefined) continue;
      const machine = machineOf(project, hostMap);
      const providers =
        machine === null
          ? []
          : await providerModelsFor({
              projectId: project.id,
              hostId: machine.hostId,
              profile: profileProject.profile,
              settings,
              cache,
            });
      rows.push({
        projectId: project.id,
        name: profileProject.name,
        profile: profileProject.profile,
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
      profileProvidersConfigured: settings.profileProviders !== null,
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
      });
    }
    const running = result.filter((row) => ACTIVE_STATUSES.has(row.status)).length;
    return {
      threads: result,
      limit: params.limit,
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
    const allowed = allowedProviderIds(settings, params.profile);
    if (allowed !== null && !allowed.includes(params.providerId)) {
      throw new DispatchError(
        "provider_not_allowed",
        `Провайдер «${params.providerId}» не разрешён профилю «${params.profile}».`,
      );
    }
    const providers = await providerModelsFor({
      projectId: params.projectId,
      hostId: target.machine.hostId,
      profile: params.profile,
      settings,
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
      const afterSeq = await lastEventSeq(params.threadId);
      let response;
      try {
        response = await sdk().threads.send({
          threadId: params.threadId,
          mode: "auto",
          input: [{ type: "text", text: params.text, mentions: [] }],
        });
      } catch (cause) {
        log.error(`send failed for ${params.threadId}: ${messageOf(cause)}`);
        throw new DispatchError("send_failed", "Не удалось отправить сообщение.");
      }
      const request = await waitForTurnRequest(params.threadId, afterSeq);
      const result = {
        threadId: params.threadId,
        afterSeq,
        requestSeq: request?.seq ?? null,
        requestId: request?.data?.requestId ?? null,
        delivery: response?.delivery ?? "sent",
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

  return { call };
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

function parseProfileProviders(raw) {
  if (raw === undefined || raw === null || String(raw).trim() === "") {
    return null;
  }
  let value;
  try {
    value = JSON.parse(String(raw));
  } catch {
    throw new DispatchError(
      "bad_settings",
      "Настройка profileProviders должна быть JSON-объектом.",
    );
  }
  if (!isPlainObject(value)) {
    throw new DispatchError(
      "bad_settings",
      "Настройка profileProviders должна быть JSON-объектом.",
    );
  }
  for (const [profile, list] of Object.entries(value)) {
    if (list === null) continue;
    if (
      !Array.isArray(list) ||
      list.some((item) => typeof item !== "string" || item === "")
    ) {
      throw new DispatchError(
        "bad_settings",
        `Настройка profileProviders для «${profile}» должна быть списком id или null.`,
      );
    }
  }
  return value;
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
