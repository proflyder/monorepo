import { createDispatcher } from "./lib/dispatcher.js";
import { normalizeError } from "./lib/errors.js";
import { verifySignature } from "./lib/hmac.js";
import {
  BODY_LIMIT_BYTES,
  parseRpcRequest,
  standardSchema,
} from "./lib/schema.js";

// Thin, deterministic executor behind the home voice path:
//
//   voice / Hermes -> mcp-gateway -> SSH forced command -> curl -> this route
//
// There is no LLM here and no arbitrary operation: the six methods below are
// the whole surface. bb itself is never exposed; only this plugin route is,
// with bb's per-plugin token plus an HMAC signature over the exact body.
export default async function plugin(bb) {
  const settings = bb.settings.define({
    hmacSecret: {
      type: "string",
      label: "HMAC secret",
      secret: true,
    },
    workerLimit: {
      type: "number",
      label: "Maximum concurrent threads created by this plugin",
      default: 2,
      experimental_schema: standardSchema((value) =>
        Number.isInteger(value) && value >= 1 && value <= 20
          ? { value }
          : { issues: [{ message: "Expected an integer from 1 to 20" }] },
      ),
    },
    permissionMode: {
      type: "select",
      label: "Permission mode for spawned threads",
      options: ["auto", "accept-edits"],
      default: "auto",
      experimental_schema: standardSchema((value) =>
        value === "auto" || value === "accept-edits"
          ? { value }
          : { issues: [{ message: "Use auto or accept-edits; full is refused" }] },
      ),
    },
    dispatcherThreadId: {
      type: "string",
      label: "Working dispatcher thread id (optional)",
      default: "",
    },
    compactAt: {
      type: "string",
      label: "Nightly dispatcher compaction time, HH:MM local (empty disables)",
      default: "04:30",
      experimental_schema: standardSchema((value) =>
        value === "" || /^([01]\d|2[0-3]):[0-5]\d$/u.test(value)
          ? { value }
          : { issues: [{ message: "Expected HH:MM in 24-hour local time" }] },
      ),
    },
    compactAfterTurns: {
      type: "number",
      label: "Compact after this many idle dispatcher sends",
      default: 30,
      experimental_schema: standardSchema((value) =>
        Number.isInteger(value) && value >= 1 && value <= 500
          ? { value }
          : { issues: [{ message: "Expected an integer from 1 to 500" }] },
      ),
    },
    compactAtContextPercent: {
      type: "number",
      label: "Compact when recorded context usage reaches this percent",
      default: 70,
      experimental_schema: standardSchema((value) =>
        Number.isInteger(value) && value >= 1 && value <= 100
          ? { value }
          : { issues: [{ message: "Expected an integer from 1 to 100" }] },
      ),
    },
    dispatcherReminder: {
      type: "string",
      label: "Role reminder appended once after each successful compaction",
      default:
        "Напоминание: ты рабочий голосовой диспетчер; отвечай коротко по-русски и заканчивай строкой 🔊 с 1–2 фразами для озвучивания.",
      experimental_schema: standardSchema((value) =>
        typeof value === "string" && value.length <= 1000
          ? { value: value.trim() }
          : {
              issues: [
                { message: "Expected a string of at most 1000 characters" },
              ],
            },
      ),
    },
  });

  const dispatcher = createDispatcher({
    getSdk: () => bb.sdk,
    kv: bb.storage.kv,
    pluginId: bb.pluginId,
    getSettings: () => settings.get(),
    log: bb.log,
    now: () => Date.now(),
  });

  if (!(await settings.get()).hmacSecret) {
    bb.status.needsConfiguration(
      `Set a secret with \`bb plugin config ${bb.pluginId} set hmacSecret <value>\`.`,
    );
  }

  // The cron fires every minute; the dispatcher decides whether the configured
  // local wall-clock time has been reached. A cron built from `compactAt`
  // would be fixed at activation and could not follow a live setting change.
  if (typeof bb.background?.schedule === "function") {
    bb.background.schedule("dispatcher-compaction", "* * * * *", async () => {
      try {
        const outcome = await dispatcher.backgroundTick();
        if (outcome?.ran) {
          bb.log.info(
            `nightly compaction requested for ${outcome.threadId} at ${outcome.compactAt}`,
          );
        } else {
          bb.log.debug(
            `nightly compaction skipped: ${outcome?.reason ?? "unknown"}`,
          );
        }
      } catch (cause) {
        bb.log.error(`nightly compaction tick failed: ${String(cause)}`);
      }
    });
  }

  bb.http.route(
    "POST",
    "/rpc",
    async (context) => {
      const contentType = context.req.header("content-type") ?? "";
      if (!contentType.toLowerCase().startsWith("application/json")) {
        return context.json(
          {
            ok: false,
            code: "unsupported_media_type",
            speech: "Ожидается JSON.",
          },
          415,
        );
      }
      const declaredLength = Number(
        context.req.header("content-length") ?? "0",
      );
      if (Number.isFinite(declaredLength) && declaredLength > BODY_LIMIT_BYTES) {
        return context.json(
          {
            ok: false,
            code: "body_too_large",
            speech: "Слишком большой запрос.",
          },
          413,
        );
      }
      const body = await context.req.text();
      if (Buffer.byteLength(body, "utf8") > BODY_LIMIT_BYTES) {
        return context.json(
          {
            ok: false,
            code: "body_too_large",
            speech: "Слишком большой запрос.",
          },
          413,
        );
      }

      const config = await settings.get();
      const verdict = verifySignature({
        secret: config.hmacSecret,
        timestamp: context.req.header("x-bb-dispatch-timestamp"),
        signature: context.req.header("x-bb-dispatch-signature"),
        body,
      });
      if (!verdict.ok) {
        if (verdict.code === "not_configured") {
          return context.json(
            {
              ok: false,
              code: "not_configured",
              speech: "Плагин не настроен.",
            },
            503,
          );
        }
        bb.log.warn(`rejected request: ${verdict.code}`);
        return context.json(
          { ok: false, code: verdict.code, speech: "Неверная подпись." },
          401,
        );
      }

      let request;
      try {
        request = parseRpcRequest(JSON.parse(body));
      } catch (cause) {
        const error = normalizeError(cause);
        if (error.code === "internal_error") {
          return context.json(
            {
              ok: false,
              code: "invalid_json",
              speech: "Некорректный JSON.",
            },
            400,
          );
        }
        return context.json(
          { ok: false, code: error.code, speech: error.speech },
          error.httpStatus,
        );
      }

      try {
        const result = await dispatcher.call(request.method, request.params);
        bb.log.info(`rpc ${request.method} ok`);
        return context.json({ ok: true, result, speech: result.speech }, 200);
      } catch (cause) {
        const error = normalizeError(cause);
        if (error.code === "internal_error") {
          bb.log.error(`rpc ${request.method} failed: ${String(cause)}`);
        } else {
          bb.log.info(`rpc ${request.method} refused: ${error.code}`);
        }
        return context.json(
          { ok: false, code: error.code, speech: error.speech },
          error.httpStatus,
        );
      }
    },
    { auth: "token" },
  );
}
