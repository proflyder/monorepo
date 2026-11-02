import assert from "node:assert/strict";
import test from "node:test";

import {
  configFromEnv,
  constantTimeEqual,
  decideCron,
  decidePing,
  DEFAULT_CONFIG,
  escapeHtml,
  formatDuration,
  normalizeState,
  parseAlertmanagerPayload,
} from "./decide.js";

const CFG = { graceSeconds: 900, remindSeconds: 21600, siteName: "дом" };

test("configFromEnv applies defaults and rejects nonsense", () => {
  assert.deepEqual(configFromEnv({}), DEFAULT_CONFIG);
  assert.deepEqual(
    configFromEnv({ GRACE_SECONDS: "60", REMIND_SECONDS: "0", SITE_NAME: "  дача  " }),
    { graceSeconds: 60, remindSeconds: 0, siteName: "дача" },
  );
  assert.deepEqual(
    configFromEnv({ GRACE_SECONDS: "-1", REMIND_SECONDS: "nope", SITE_NAME: "   " }),
    DEFAULT_CONFIG,
  );
});

test("normalizeState tolerates missing and malformed KV values", () => {
  assert.deepEqual(normalizeState({}), {
    lastPingUnix: null,
    status: "up",
    downSinceUnix: null,
    lastReminderUnix: null,
  });
  assert.deepEqual(
    normalizeState({
      lastPingUnix: "100",
      status: "down",
      downSinceUnix: "90",
      lastReminderUnix: "95",
    }),
    { lastPingUnix: 100, status: "down", downSinceUnix: 90, lastReminderUnix: 95 },
  );
  assert.equal(normalizeState({ lastPingUnix: "x", status: "down" }).status, "up");
});

test("a ping while up only refreshes last_ping", () => {
  const decision = decidePing({ lastPingUnix: 1000, status: "up" }, 1300, CFG);
  assert.deepEqual(decision.actions, []);
  assert.deepEqual(decision.writes, { lastPingUnix: 1300 });
});

test("a ping while down sends recovery and returns to up", () => {
  const decision = decidePing(
    { lastPingUnix: 1000, status: "down", downSinceUnix: 400, lastReminderUnix: 1100 },
    2200,
    CFG,
  );
  assert.equal(decision.actions.length, 1);
  assert.equal(decision.actions[0].type, "recovery");
  assert.match(decision.actions[0].text, /Связь с <b>дом<\/b> восстановлена/);
  assert.match(decision.actions[0].text, /Молчал 30 минут/);
  assert.deepEqual(decision.writes, {
    lastPingUnix: 2200,
    status: "up",
    downSinceUnix: null,
    lastReminderUnix: null,
  });
});

test("cron is silent before the first ping", () => {
  const decision = decideCron({}, 10_000, CFG);
  assert.equal(decision.action, null);
  assert.deepEqual(decision.writes, {});
});

test("cron is silent inside the grace window", () => {
  const decision = decideCron({ lastPingUnix: 1000, status: "up" }, 1900, CFG);
  assert.equal(decision.action, null);
});

test("cron sends one down alert after the grace window", () => {
  const decision = decideCron({ lastPingUnix: 1000, status: "up" }, 1901, CFG);
  assert.equal(decision.action.type, "down");
  assert.match(decision.action.text, /<b>дом<\/b> молчит 15 минут/);
  assert.match(decision.action.text, /нет heartbeat от Alertmanager/);
  assert.deepEqual(decision.writes, {
    status: "down",
    downSinceUnix: 1000,
    lastReminderUnix: 1901,
  });
});

test("cron does not repeat the down alert before the reminder interval", () => {
  const state = {
    lastPingUnix: 1000,
    status: "down",
    downSinceUnix: 1000,
    lastReminderUnix: 1901,
  };
  assert.equal(decideCron(state, 1901 + 100, CFG).action, null);
});

test("cron sends an optional reminder while still down", () => {
  const state = {
    lastPingUnix: 1000,
    status: "down",
    downSinceUnix: 1000,
    lastReminderUnix: 1901,
  };
  const decision = decideCron(state, 1901 + CFG.remindSeconds, CFG);
  assert.equal(decision.action.type, "reminder");
  assert.match(decision.action.text, /всё ещё молчит/);
  assert.deepEqual(decision.writes, { lastReminderUnix: 1901 + CFG.remindSeconds });
});

test("REMIND_SECONDS=0 disables reminders", () => {
  const state = { lastPingUnix: 1000, status: "down", downSinceUnix: 1000 };
  assert.equal(decideCron(state, 1_000_000, { ...CFG, remindSeconds: 0 }).action, null);
});

test("resolved Alertmanager payloads are ignored", () => {
  assert.deepEqual(parseAlertmanagerPayload('{"status":"resolved"}'), { resolved: true });
  assert.deepEqual(parseAlertmanagerPayload('{"status":"firing"}'), { resolved: false });
  assert.deepEqual(parseAlertmanagerPayload("not json"), { resolved: false });
});

test("token comparison is exact", () => {
  assert.equal(constantTimeEqual("secret", "secret"), true);
  assert.equal(constantTimeEqual("secret", "secreT"), false);
  assert.equal(constantTimeEqual("secret", "secret "), false);
  assert.equal(constantTimeEqual("", ""), true);
});

test("HTML in the site name cannot break Telegram markup", () => {
  assert.equal(escapeHtml("<дом & дача>"), "&lt;дом &amp; дача&gt;");
  assert.equal(formatDuration(59), "59 секунд");
  assert.equal(formatDuration(60), "1 минута");
  assert.equal(formatDuration(125), "2 минуты");
  assert.equal(formatDuration(3600), "1 час");
  assert.equal(formatDuration(3900), "1 час 5 минут");
});
