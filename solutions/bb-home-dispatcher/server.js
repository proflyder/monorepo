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
    profileProviders: {
      type: "string",
      label: "Allowed providers per profile (JSON object, empty = all)",
      experimental_multiline: true,
      default: "",
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
