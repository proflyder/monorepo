import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { METHOD_PARAMS, parseRpcRequest } from "../lib/schema.js";

// The gateway (solutions/mcp-gateway) serializes requests for the same six
// methods. `contract/params.json` is the shared hand-written source of truth:
// this test pins `schema.js` to it, and the Go contract test in
// `internal/bb/contract_test.go` pins the gateway's payloads to it. A field
// added on one side alone fails one of the two suites.
const CONTRACT = JSON.parse(
  readFileSync(new URL("../contract/params.json", import.meta.url), "utf8"),
);

const SAMPLE_VALUES = {
  profile: "work",
  projectId: "proj_alpha",
  providerId: "codex",
  model: "gpt-5.2-codex",
  reasoningLevel: "medium",
  title: "Fix the flaky test",
  prompt: "Find and fix the flaky test.",
  idempotencyKey: "key-1",
  threadId: "thr_1",
  text: "hello",
  limit: 5,
  afterSeq: 0,
  waitSeconds: 5,
};

function sorted(values) {
  return [...values].sort();
}

test("contract: schema.js method table matches contract/params.json", () => {
  assert.deepEqual(Object.keys(CONTRACT.methods).sort(), Object.keys(METHOD_PARAMS).sort());
  for (const [method, declared] of Object.entries(CONTRACT.methods)) {
    const actual = METHOD_PARAMS[method];
    assert.ok(actual, `schema.js has no entry for ${method}`);
    assert.deepEqual(
      sorted(actual.required),
      sorted(declared.required),
      `required fields differ for ${method}`,
    );
    assert.deepEqual(
      sorted(actual.optional),
      sorted(declared.optional),
      `optional fields differ for ${method}`,
    );
    assert.deepEqual(
      Object.keys(declared).sort(),
      ["optional", "required"],
      `unexpected keys in the contract for ${method}`,
    );
  }
});

test("contract: every declared field is a sample value the schema accepts", () => {
  for (const [method, declared] of Object.entries(CONTRACT.methods)) {
    for (const field of [...declared.required, ...declared.optional]) {
      assert.ok(
        Object.hasOwn(SAMPLE_VALUES, field),
        `add a sample value for the new field ${field} (${method})`,
      );
    }
  }
});

test("contract: the schema accepts exactly the declared fields", () => {
  for (const [method, declared] of Object.entries(CONTRACT.methods)) {
    const params = {};
    for (const field of [...declared.required, ...declared.optional]) {
      params[field] = SAMPLE_VALUES[field];
    }
    assert.doesNotThrow(
      () => parseRpcRequest({ method, params }),
      `schema rejects a declared field for ${method}`,
    );

    const extra = parseRpcRequestExtra(method, params);
    assert.throws(
      () => parseRpcRequest({ method, params: extra }),
      /не поддерживается/u,
      `schema accepts an undeclared field for ${method}`,
    );
  }
});

test("contract: required fields are enforced", () => {
  for (const [method, declared] of Object.entries(CONTRACT.methods)) {
    for (const field of declared.required) {
      const params = {};
      for (const other of [...declared.required, ...declared.optional]) {
        if (other !== field) params[other] = SAMPLE_VALUES[other];
      }
      assert.throws(
        () => parseRpcRequest({ method, params }),
        new RegExp(`Поле params\\.${field} обязательно\\.`, "u"),
        `${method} accepted a request without ${field}`,
      );
    }
  }
});

function parseRpcRequestExtra(method, params) {
  // "extra" is not a field of any method, so it is refused everywhere and
  // would immediately resurface the live validate_spawn/title bug if a
  // caller or the schema ever widened the contract by accident.
  assert.ok(!("extra" in SAMPLE_VALUES));
  return { ...params, extra: SAMPLE_VALUES.text };
}
