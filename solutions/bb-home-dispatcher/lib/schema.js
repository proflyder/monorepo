import { DispatchError } from "./errors.js";

// Strict, dependency-free boundary validation. The plugin runs inside the bb
// server and must not pull an npm dependency only to reject malformed JSON;
// the rules below are intentionally small and explicit.

// The exact params each method accepts, split into required and optional.
// `contract/params.json` pins the same table: the gateway's Go contract test
// and the plugin's JS test both read it, so a field added on one side alone
// fails on the other. Keep the two in sync in the same change.
export const METHOD_PARAMS = {
  targets: { required: [], optional: [] },
  status: { required: [], optional: ["limit"] },
  validate_spawn: {
    required: ["profile", "projectId", "providerId", "model", "reasoningLevel"],
    optional: [],
  },
  spawn: {
    required: [
      "profile",
      "projectId",
      "providerId",
      "model",
      "reasoningLevel",
      "title",
      "prompt",
      "idempotencyKey",
    ],
    optional: [],
  },
  send: {
    required: ["threadId", "text", "idempotencyKey"],
    optional: [],
  },
  turn_result: {
    required: ["threadId", "afterSeq"],
    optional: [],
  },
};

const METHODS = new Set(Object.keys(METHOD_PARAMS));

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
  expectParams(params, method);
  switch (method) {
    case "targets":
      return {};
    case "status": {
      const limit =
        params.limit === undefined
          ? 10
          : expectInteger(params.limit, "limit", 1, 50);
      return { limit };
    }
    case "validate_spawn":
      return parseSpawnTarget(params);
    case "spawn":
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
    case "send":
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
    case "turn_result":
      return {
        threadId: expectPattern(params.threadId, "threadId", ID_PATTERN),
        afterSeq: expectInteger(
          params.afterSeq,
          "afterSeq",
          0,
          Number.MAX_SAFE_INTEGER,
        ),
      };
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

function expectParams(value, method) {
  const { required, optional } = METHOD_PARAMS[method];
  for (const key of Object.keys(value)) {
    if (!required.includes(key) && !optional.includes(key)) {
      throw invalid(`Поле params.${key} не поддерживается.`);
    }
  }
  for (const key of required) {
    if (!Object.hasOwn(value, key)) {
      throw invalid(`Поле params.${key} обязательно.`);
    }
  }
}

// The request envelope is fixed: exactly `method` and `params`.
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

// `bb.sdk.plugins.callRpc` accepts a zod schema and calls `outputSchema.parse`
// on the other plugin's result. A Standard Schema object is not enough: it has
// only `~standard.validate`, so `callRpc` fails before the dispatcher sees any
// data. These are minimal parse-compatible schemas; like the rest of this
// module they stay dependency-free and treat cross-plugin data as untrusted.
export function outputSchema(validate) {
  return {
    parse(value) {
      if (!validate(value)) {
        throw new Error("RPC result has an unexpected shape");
      }
      return value;
    },
  };
}

export function nullableStringList(value) {
  return (
    value === null ||
    (Array.isArray(value) &&
      value.every((item) => typeof item === "string" && item !== ""))
  );
}

export const profileProjectsOutputSchema = outputSchema(
  (value) =>
    isPlainObject(value) &&
    Array.isArray(value.projects) &&
    value.projects.every(
      (row) =>
        isPlainObject(row) &&
        typeof row.id === "string" &&
        typeof row.name === "string" &&
        typeof row.profile === "string",
    ),
);

export const profilePolicyOutputSchema = outputSchema(
  (value) =>
    isPlainObject(value) &&
    nullableStringList(value.providers) &&
    nullableStringList(value.machines),
);

function isPlainObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function standardSchema(validate) {
  return {
    "~standard": {
      version: 1,
      vendor: "home-dispatcher",
      validate,
    },
  };
}
