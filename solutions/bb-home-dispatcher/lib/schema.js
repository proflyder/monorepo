import { DispatchError } from "./errors.js";

// Strict, dependency-free boundary validation. The plugin runs inside the bb
// server and must not pull an npm dependency only to reject malformed JSON;
// the rules below are intentionally small and explicit.
const METHODS = new Set([
  "targets",
  "status",
  "validate_spawn",
  "spawn",
  "send",
  "turn_result",
]);

const ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/u;
const PROFILE_PATTERN = /^[a-z0-9][a-z0-9_-]{0,31}$/u;
const MODEL_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/u;
const REASONING_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/u;

export const BODY_LIMIT_BYTES = 16 * 1024;

export function parseRpcRequest(value) {
  const body = expectObject(value, "request");
  expectOnlyKeys(body, ["method", "params"], "request");
  const method = expectString(body.method, "method", 1, 64);
  if (!METHODS.has(method)) {
    throw new DispatchError("unknown_method", "Неизвестная операция.", {
      httpStatus: 400,
    });
  }
  const params =
    body.params === undefined ? {} : expectObject(body.params, "params");
  return { method, params: parseParams(method, params) };
}

function parseParams(method, params) {
  switch (method) {
    case "targets": {
      expectOnlyKeys(params, [], "params");
      return {};
    }
    case "status": {
      expectOnlyKeys(params, ["limit"], "params");
      const limit =
        params.limit === undefined
          ? 10
          : expectInteger(params.limit, "limit", 1, 50);
      return { limit };
    }
    case "validate_spawn": {
      expectOnlyKeys(
        params,
        ["profile", "projectId", "providerId", "model", "reasoningLevel"],
        "params",
      );
      return parseSpawnTarget(params);
    }
    case "spawn": {
      expectOnlyKeys(
        params,
        [
          "profile",
          "projectId",
          "providerId",
          "model",
          "reasoningLevel",
          "title",
          "prompt",
          "idempotencyKey",
        ],
        "params",
      );
      return {
        ...parseSpawnTarget(params),
        title: expectString(params.title, "title", 1, 200),
        prompt: expectString(params.prompt, "prompt", 1, 8000),
        idempotencyKey: expectString(
          params.idempotencyKey,
          "idempotencyKey",
          1,
          128,
        ),
      };
    }
    case "send": {
      expectOnlyKeys(params, ["threadId", "text", "idempotencyKey"], "params");
      return {
        threadId: expectPattern(params.threadId, "threadId", ID_PATTERN),
        text: expectString(params.text, "text", 1, 8000),
        idempotencyKey: expectString(
          params.idempotencyKey,
          "idempotencyKey",
          1,
          128,
        ),
      };
    }
    case "turn_result": {
      expectOnlyKeys(params, ["threadId", "afterSeq"], "params");
      return {
        threadId: expectPattern(params.threadId, "threadId", ID_PATTERN),
        afterSeq: expectInteger(
          params.afterSeq,
          "afterSeq",
          0,
          Number.MAX_SAFE_INTEGER,
        ),
      };
    }
    default:
      throw new DispatchError("unknown_method", "Неизвестная операция.", {
        httpStatus: 400,
      });
  }
}

function parseSpawnTarget(params) {
  return {
    profile: expectPattern(params.profile, "profile", PROFILE_PATTERN),
    projectId: expectPattern(params.projectId, "projectId", ID_PATTERN),
    providerId: expectPattern(params.providerId, "providerId", ID_PATTERN),
    model: expectPattern(params.model, "model", MODEL_PATTERN),
    reasoningLevel: expectPattern(
      params.reasoningLevel,
      "reasoningLevel",
      REASONING_PATTERN,
    ),
  };
}

function expectObject(value, name) {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw invalid(`${name} должен быть объектом.`);
  }
  return value;
}

function expectOnlyKeys(value, allowed, name) {
  for (const key of Object.keys(value)) {
    if (!allowed.includes(key)) {
      throw invalid(`Поле ${name}.${key} не поддерживается.`);
    }
  }
}

function expectString(value, name, min, max) {
  if (typeof value !== "string") {
    throw invalid(`Поле ${name} должно быть строкой.`);
  }
  const trimmed = value.trim();
  if (trimmed.length < min || trimmed.length > max) {
    throw invalid(`Поле ${name} должно быть длиной ${min}–${max}.`);
  }
  return trimmed;
}

function expectPattern(value, name, pattern) {
  const text = expectString(value, name, 1, 128);
  if (!pattern.test(text)) {
    throw invalid(`Поле ${name} содержит недопустимые символы.`);
  }
  return text;
}

function expectInteger(value, name, min, max) {
  if (!Number.isSafeInteger(value) || value < min || value > max) {
    throw invalid(`Поле ${name} должно быть целым числом ${min}–${max}.`);
  }
  return value;
}

function invalid(message) {
  return new DispatchError("invalid_request", message, { httpStatus: 400 });
}

// Standard Schema v1 validator used for cross-plugin RPC output. It checks
// nothing on purpose: the dispatcher re-reads every field it needs and treats
// the other plugin's answer as untrusted input.
export const anyOutputSchema = {
  "~standard": {
    version: 1,
    vendor: "home-dispatcher",
    validate(value) {
      return { value };
    },
  },
};

export function standardSchema(validate) {
  return {
    "~standard": {
      version: 1,
      vendor: "home-dispatcher",
      validate,
    },
  };
}
