import assert from "node:assert/strict";
import test from "node:test";

import { handleFetch, handleScheduled, KV_KEYS } from "./worker.js";

class FakeKV {
  constructor(initial = {}) {
    this.data = new Map(Object.entries(initial));
    this.puts = [];
    this.deletes = [];
  }

  async get(key) {
    return this.data.has(key) ? this.data.get(key) : null;
  }

  async put(key, value) {
    this.puts.push([key, value]);
    this.data.set(key, value);
  }

  async delete(key) {
    this.deletes.push(key);
    this.data.delete(key);
  }
}

function makeEnv(kv, overrides = {}) {
  return {
    HEARTBEAT: kv,
    PING_TOKEN: "ping-token",
    TELEGRAM_BOT_TOKEN: "bot-token",
    TELEGRAM_CHAT_ID: "42",
    SITE_NAME: "дом",
    GRACE_SECONDS: "900",
    REMIND_SECONDS: "21600",
    ...overrides,
  };
}

function jsonResponse(status = 200) {
  return new Response(JSON.stringify({ ok: status < 400 }), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function fetchMock(calls, status = 200) {
  return async (url, init) => {
    calls.push({ url, init });
    return jsonResponse(status);
  };
}

function pingRequest(body = '{"status":"firing"}', token = "ping-token") {
  const headers = {};
  if (token !== null) headers.Authorization = `Bearer ${token}`;
  return new Request("https://heartbeat.example.com/ping", {
    method: "POST",
    headers,
    body,
  });
}

test("GET /health is public, minimal and does not reveal state", async () => {
  const kv = new FakeKV({ [KV_KEYS.status]: "down" });
  const response = await handleFetch(
    new Request("https://heartbeat.example.com/health"),
    makeEnv(kv),
  );
  assert.equal(response.status, 200);
  assert.equal(await response.text(), "ok");
  assert.deepEqual(kv.puts, []);
});

test("unknown paths and methods are 404", async () => {
  const env = makeEnv(new FakeKV());
  for (const request of [
    new Request("https://heartbeat.example.com/health", { method: "POST" }),
    new Request("https://heartbeat.example.com/other"),
    new Request("https://heartbeat.example.com/ping"),
  ]) {
    const response = await handleFetch(request, env);
    assert.equal(response.status, 404);
  }
});

test("ping requires the bearer token", async () => {
  const env = makeEnv(new FakeKV());
  for (const token of [null, "wrong"]) {
    const response = await handleFetch(pingRequest(undefined, token), env);
    assert.equal(response.status, 401);
    assert.equal(await response.text(), "unauthorized");
  }
});

test("ping fails closed when the PING_TOKEN secret is missing", async () => {
  const response = await handleFetch(
    pingRequest(),
    makeEnv(new FakeKV(), { PING_TOKEN: "" }),
  );
  assert.equal(response.status, 503);
});

test("a firing ping records last_ping_unix", async () => {
  const kv = new FakeKV({ [KV_KEYS.lastPing]: "1000", [KV_KEYS.status]: "up" });
  const calls = [];
  const response = await handleFetch(pingRequest(), makeEnv(kv), {
    now: () => 1300,
    fetchImpl: fetchMock(calls),
  });
  assert.equal(response.status, 200);
  assert.equal(await response.text(), "ok");
  assert.deepEqual(kv.puts, [[KV_KEYS.lastPing, "1300"]]);
  assert.equal(calls.length, 0);
});

test("a resolved ping is acknowledged but does not count as proof of life", async () => {
  const kv = new FakeKV();
  const response = await handleFetch(pingRequest('{"status":"resolved"}'), makeEnv(kv));
  assert.equal(response.status, 200);
  assert.deepEqual(kv.puts, []);
});

test("a ping while down sends recovery and transitions to up", async () => {
  const kv = new FakeKV({
    [KV_KEYS.lastPing]: "1000",
    [KV_KEYS.status]: "down",
    [KV_KEYS.downSince]: "1000",
    [KV_KEYS.lastReminder]: "1200",
  });
  const calls = [];
  const response = await handleFetch(pingRequest(), makeEnv(kv), {
    now: () => 1600,
    fetchImpl: fetchMock(calls),
  });

  assert.equal(response.status, 200);
  assert.equal(calls.length, 1);
  assert.match(calls[0].url, /^https:\/\/api\.telegram\.org\/bot/);
  const body = JSON.parse(calls[0].init.body);
  assert.equal(body.parse_mode, "HTML");
  assert.equal(body.chat_id, "42");
  assert.match(body.text, /восстановлена/);
  assert.match(body.text, /Молчал 10 минут/);
  assert.equal(kv.data.get(KV_KEYS.status), "up");
  assert.equal(kv.data.get(KV_KEYS.lastPing), "1600");
  assert.equal(kv.data.has(KV_KEYS.downSince), false);
  assert.equal(kv.data.has(KV_KEYS.lastReminder), false);
});

test("cron does nothing before the first ping", async () => {
  const kv = new FakeKV();
  const calls = [];
  await handleScheduled(makeEnv(kv), { now: () => 10_000, fetchImpl: fetchMock(calls) });
  assert.equal(calls.length, 0);
  assert.deepEqual(kv.puts, []);
});

test("cron alerts after the grace window and stores the down state", async () => {
  const kv = new FakeKV({ [KV_KEYS.lastPing]: "1000", [KV_KEYS.status]: "up" });
  const calls = [];
  await handleScheduled(makeEnv(kv), { now: () => 1901, fetchImpl: fetchMock(calls) });

  assert.equal(calls.length, 1);
  const body = JSON.parse(calls[0].init.body);
  assert.match(body.text, /молчит 15 минут/);
  assert.equal(kv.data.get(KV_KEYS.status), "down");
  assert.equal(kv.data.get(KV_KEYS.downSince), "1000");
  assert.equal(kv.data.get(KV_KEYS.lastReminder), "1901");
});

test("cron stays quiet inside the grace window", async () => {
  const kv = new FakeKV({ [KV_KEYS.lastPing]: "1000", [KV_KEYS.status]: "up" });
  const calls = [];
  await handleScheduled(makeEnv(kv), { now: () => 1900, fetchImpl: fetchMock(calls) });
  assert.equal(calls.length, 0);
  assert.deepEqual(kv.puts, []);
});

test("cron retries a failed Telegram send without changing state", async () => {
  const kv = new FakeKV({ [KV_KEYS.lastPing]: "1000", [KV_KEYS.status]: "up" });
  const calls = [];
  await assert.rejects(
    handleScheduled(makeEnv(kv), { now: () => 1901, fetchImpl: fetchMock(calls, 500) }),
    /telegram sendMessage failed with status 500/,
  );
  assert.equal(calls.length, 1);
  assert.equal(kv.data.get(KV_KEYS.status), "up");
});
