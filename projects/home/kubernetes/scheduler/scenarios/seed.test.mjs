import assert from "node:assert/strict";
import test from "node:test";

import {
  loadDefinitions,
  normalizeLabel,
  planSeed,
  run,
  seedKey,
} from "./seed.mjs";

function fakeScheduler(t, scenarios) {
  const calls = [];
  t.mock.method(globalThis, "fetch", async (url, init) => {
    const method = String(url).split("/").pop();
    calls.push({ method, body: JSON.parse(init.body) });
    if (method === "ListScenarios") {
      return new Response(JSON.stringify({ scenarios }), { status: 200 });
    }
    return new Response(JSON.stringify({ scenario: { id: "scn-1" } }), { status: 200 });
  });
  return calls;
}

test("labels are compared trimmed and case-insensitively", () => {
  assert.equal(normalizeLabel("  Блэкаут "), "блэкаут");
  assert.equal(normalizeLabel(undefined), "");
});

test("an existing label is never overwritten", () => {
  const definitions = [
    { slug: "blackout", label: "Блэкаут" },
    { slug: "work_mode", label: "Сел работать" },
  ];
  const existing = [{ label: "  БЛЭКАУТ " }];
  const planned = planSeed(definitions, existing);
  assert.deepEqual(planned.map((definition) => definition.slug), ["work_mode"]);
});

test("a missing label is planned even when another scenario exists", () => {
  const definitions = [
    { slug: "blackout", label: "Блэкаут" },
    { slug: "work_mode", label: "Сел работать" },
  ];
  const planned = planSeed(definitions, [{ label: "Что-то другое" }]);
  assert.deepEqual(planned.map((definition) => definition.slug), ["blackout", "work_mode"]);
});

test("a fully seeded list plans nothing", () => {
  const definitions = [{ slug: "blackout", label: "Блэкаут" }];
  assert.deepEqual(planSeed(definitions, [{ label: "Блэкаут" }]), []);
});

test("an empty or missing existing list plans every definition", () => {
  const definitions = [{ slug: "blackout", label: "Блэкаут" }];
  assert.deepEqual(planSeed(definitions, []), definitions);
  assert.deepEqual(planSeed(definitions, undefined), definitions);
});

test("a duplicate label inside the definitions is planned once", () => {
  const definitions = [
    { slug: "one", label: "Блэкаут" },
    { slug: "two", label: "блэкаут" },
  ];
  assert.deepEqual(planSeed(definitions, []).map((definition) => definition.slug), ["one"]);
});

test("a definition without a label is rejected", () => {
  assert.throws(() => planSeed([{ slug: "x", label: " " }], []), /without a label/);
});

test("the idempotency key is stable per slug", () => {
  assert.equal(seedKey({ slug: "work_mode", label: "Сел работать" }), "seed-work_mode");
  assert.throws(() => seedKey({ label: "no slug" }), /no slug/);
});

test("run creates only missing labels and never overwrites", async (t) => {
  const calls = fakeScheduler(t, [{ label: "блэкаут" }]);
  const created = await run({
    base: "http://scheduler.test",
    token: "test-token",
    definitions: [
      { slug: "blackout", label: "Блэкаут", steps: [] },
      { slug: "work_mode", label: "Сел работать", steps: [] },
    ],
  });
  assert.equal(created, 1);
  assert.deepEqual(calls.map((call) => call.method), ["ListScenarios", "CreateScenario"]);
  const body = calls[1].body;
  assert.equal(body.label, "Сел работать");
  assert.equal(body.idempotencyKey, "seed-work_mode");
  assert.equal(body.enabled, true);
  assert.deepEqual(body.triggers, [{ manual: {} }]);
});

test("dry-run lists but never calls CreateScenario", async (t) => {
  const calls = fakeScheduler(t, []);
  const created = await run({
    dryRun: true,
    base: "http://scheduler.test",
    token: "test-token",
    definitions: [{ slug: "blackout", label: "Блэкаут", steps: [] }],
  });
  assert.equal(created, 1);
  assert.deepEqual(calls.map((call) => call.method), ["ListScenarios"]);
});

test("a Connect error is reported without the response body", async (t) => {
  t.mock.method(globalThis, "fetch", async () =>
    new Response(JSON.stringify({ code: "unauthenticated", message: "denied" }), {
      status: 401,
    }),
  );
  await assert.rejects(
    run({ base: "http://scheduler.test", token: "bad", definitions: [] }),
    (error) => error.message === "ListScenarios: unauthenticated" && !error.message.includes("bad"),
  );
});

test("the checked-in definitions are valid and unique", async () => {
  const definitions = await loadDefinitions();
  assert.deepEqual(
    definitions.map((definition) => definition.slug).sort(),
    ["blackout", "work_mode"],
  );
  for (const definition of definitions) {
    assert.ok(definition.label.length > 0);
    assert.ok(definition.steps.length > 0);
    assert.deepEqual(definition.triggers, [{ manual: {} }]);
    assert.equal(definition.continueOnError, true);
    for (const step of definition.steps) {
      assert.equal(typeof step.catalog?.entryId, "string");
      assert.equal(typeof step.catalog?.action, "string");
      assert.doesNotThrow(() => JSON.parse(step.catalog.paramsJson));
    }
  }
});

test("blackout turns off exactly the three bedroom lights", async () => {
  const definitions = await loadDefinitions();
  const blackout = definitions.find((definition) => definition.slug === "blackout");
  assert.deepEqual(
    blackout.steps.map((step) => [step.catalog.entryId, step.catalog.action]),
    [
      ["light.spalnia_lenta", "turn_off"],
      ["light.yeelink_cn_827036414_bslamp2_s_2_light", "turn_off"],
      ["light.yeelink_cn_919937191_lamp22_s_2", "turn_off"],
    ],
  );
});

test("work mode restores the captured work lighting settings", async () => {
  const definitions = await loadDefinitions();
  const workMode = definitions.find((definition) => definition.slug === "work_mode");
  const applied = Object.fromEntries(
    workMode.steps.map((step) => [
      step.catalog.entryId,
      JSON.parse(step.catalog.paramsJson),
    ]),
  );
  assert.deepEqual(applied, {
    "light.spalnia_lenta": { brightness: 100, kelvin: 2816 },
    "light.yeelink_cn_827036414_bslamp2_s_2_light": {
      brightness: 100,
      kelvin: 2800,
      effect: "Day",
    },
    "light.yeelink_cn_919937191_lamp22_s_2": { brightness: 100, kelvin: 2800 },
  });
});
