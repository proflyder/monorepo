// Cloudflare Worker: external dead-man switch for home alerting.
//
// Alertmanager routes the always-firing Watchdog alert here every 5 minutes.
// The Worker records the ping in KV; a Cron Trigger notices silence and sends
// a Telegram message through the same infra bot Alertmanager already uses.
// It sends a recovery message when pings resume.
//
// This module has no runtime dependencies beyond the Workers runtime.

import {
  configFromEnv,
  constantTimeEqual,
  decideCron,
  decidePing,
  parseAlertmanagerPayload,
  toUnix,
} from "./decide.js";

export const KV_KEYS = Object.freeze({
  lastPing: "last_ping_unix",
  status: "state",
  downSince: "down_since_unix",
  lastReminder: "last_reminder_unix",
});

// Decision objects use camelCase fields; KV uses the documented snake_case keys.
const WRITE_KEYS = Object.freeze({
  lastPingUnix: KV_KEYS.lastPing,
  status: KV_KEYS.status,
  downSinceUnix: KV_KEYS.downSince,
  lastReminderUnix: KV_KEYS.lastReminder,
});

async function readState(kv) {
  const [lastPing, status, downSince, lastReminder] = await Promise.all([
    kv.get(KV_KEYS.lastPing),
    kv.get(KV_KEYS.status),
    kv.get(KV_KEYS.downSince),
    kv.get(KV_KEYS.lastReminder),
  ]);

  return {
    lastPingUnix: toUnix(lastPing),
    status: status === "down" ? "down" : "up",
    downSinceUnix: toUnix(downSince),
    lastReminderUnix: toUnix(lastReminder),
  };
}

async function applyWrites(kv, writes) {
  await Promise.all(
    Object.entries(writes).map(([field, value]) => {
      const key = WRITE_KEYS[field];
      if (!key) throw new Error(`unknown state field: ${field}`);
      return value === null ? kv.delete(key) : kv.put(key, String(value));
    }),
  );
}

async function sendTelegram(env, text, fetchImpl) {
  const token = env.TELEGRAM_BOT_TOKEN;
  const chatId = env.TELEGRAM_CHAT_ID;
  if (!token || !chatId) {
    throw new Error("telegram is not configured (TELEGRAM_BOT_TOKEN/TELEGRAM_CHAT_ID)");
  }

  // The token is part of the Bot API URL. Never log the URL or the response body.
  const response = await fetchImpl(`https://api.telegram.org/bot${token}/sendMessage`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      chat_id: chatId,
      text,
      parse_mode: "HTML",
      disable_web_page_preview: true,
    }),
  });

  if (!response.ok) {
    throw new Error(`telegram sendMessage failed with status ${response.status}`);
  }
}

function plainText(body, status) {
  return new Response(body, {
    status,
    headers: {
      "content-type": "text/plain; charset=utf-8",
      "cache-control": "no-store",
    },
  });
}

function bearerToken(request) {
  const header = (request.headers.get("Authorization") || "").trim();
  const match = /^Bearer\s+(.+)$/i.exec(header);
  return match ? match[1] : "";
}

export async function handleFetch(request, env, deps = {}) {
  const now = deps.now ? deps.now() : Math.floor(Date.now() / 1000);
  const fetchImpl = deps.fetchImpl ?? fetch;
  const url = new URL(request.url);

  // Minimal liveness probe without auth and without exposing state.
  if (url.pathname === "/health") {
    if (request.method !== "GET") return plainText("not found", 404);
    return plainText("ok", 200);
  }

  if (url.pathname !== "/ping" || request.method !== "POST") {
    return plainText("not found", 404);
  }

  if (!env.PING_TOKEN) {
    // Fail closed on a missing secret; a misconfigured deploy must not accept
    // unauthenticated heartbeats.
    return plainText("not configured", 503);
  }
  if (!constantTimeEqual(bearerToken(request), env.PING_TOKEN)) {
    return plainText("unauthorized", 401);
  }

  const payload = parseAlertmanagerPayload(await request.text());
  if (payload.resolved) return plainText("ok", 200);

  const state = await readState(env.HEARTBEAT);
  const decision = decidePing(state, now, configFromEnv(env));

  if (decision.actions.length > 0) {
    try {
      for (const action of decision.actions) {
        await sendTelegram(env, action.text, fetchImpl);
      }
    } catch (error) {
      // Keep the liveness timestamp current so the cron does not declare a
      // second outage, then let Alertmanager retry the ping (and recovery).
      await applyWrites(env.HEARTBEAT, { lastPingUnix: now });
      console.error(
        "heartbeat: recovery delivery failed:",
        error instanceof Error ? error.message : "unknown error",
      );
      return plainText("delivery failed", 502);
    }
  }

  await applyWrites(env.HEARTBEAT, decision.writes);
  return plainText("ok", 200);
}

export async function handleScheduled(env, deps = {}) {
  const now = deps.now ? deps.now() : Math.floor(Date.now() / 1000);
  const fetchImpl = deps.fetchImpl ?? fetch;
  const state = await readState(env.HEARTBEAT);
  const decision = decideCron(state, now, configFromEnv(env));

  if (!decision.action) return;

  // Send first: if Telegram fails, the state stays unchanged and the next cron
  // tick retries. A KV failure after a successful send can duplicate one
  // message, which is preferable to silently losing an outage alert.
  await sendTelegram(env, decision.action.text, fetchImpl);
  await applyWrites(env.HEARTBEAT, decision.writes);
}

export default {
  async fetch(request, env, ctx) {
    return handleFetch(request, env);
  },
  async scheduled(event, env, ctx) {
    ctx.waitUntil(handleScheduled(env));
  },
};
