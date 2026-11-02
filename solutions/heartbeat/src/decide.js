// Pure decision logic for the external heartbeat Worker.
//
// The Worker itself only talks to KV, Telegram and HTTP; every branch that
// decides "send an alert or stay quiet" lives here so it can be unit-tested
// with a fake clock and fake state, without a Workers runtime.

export const DEFAULT_CONFIG = Object.freeze({
  graceSeconds: 900,
  remindSeconds: 21600,
  siteName: "дом",
});

const encoder = new TextEncoder();

function positiveInt(value, fallback, allowZero = false) {
  const parsed = Number.parseInt(String(value ?? ""), 10);
  const minimum = allowZero ? 0 : 1;
  if (!Number.isFinite(parsed) || parsed < minimum) return fallback;
  return parsed;
}

export function configFromEnv(env = {}) {
  const siteName = String(env.SITE_NAME ?? DEFAULT_CONFIG.siteName).trim();
  return {
    graceSeconds: positiveInt(env.GRACE_SECONDS, DEFAULT_CONFIG.graceSeconds),
    remindSeconds: positiveInt(env.REMIND_SECONDS, DEFAULT_CONFIG.remindSeconds, true),
    siteName: siteName || DEFAULT_CONFIG.siteName,
  };
}

// KV stores integers as text. Missing, empty or malformed values become null.
export function toUnix(value) {
  if (value === null || value === undefined || value === "") return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0 ? Math.floor(parsed) : null;
}

export function normalizeState(raw = {}) {
  const lastPingUnix = toUnix(raw.lastPingUnix);
  const status = raw.status === "down" && lastPingUnix !== null ? "down" : "up";
  return {
    lastPingUnix,
    status,
    downSinceUnix: toUnix(raw.downSinceUnix),
    lastReminderUnix: toUnix(raw.lastReminderUnix),
  };
}

export function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function plural(number, one, few, many) {
  const mod10 = number % 10;
  const mod100 = number % 100;
  if (mod10 === 1 && mod100 !== 11) return one;
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return few;
  return many;
}

export function formatDuration(seconds) {
  const total = Math.max(0, Math.floor(seconds));
  if (total < 60) {
    return `${total} ${plural(total, "секунда", "секунды", "секунд")}`;
  }
  const minutes = Math.floor(total / 60);
  if (minutes < 60) {
    return `${minutes} ${plural(minutes, "минута", "минуты", "минут")}`;
  }
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  const hoursText = `${hours} ${plural(hours, "час", "часа", "часов")}`;
  return rest === 0
    ? hoursText
    : `${hoursText} ${rest} ${plural(rest, "минута", "минуты", "минут")}`;
}

function downText(siteName, silentSeconds) {
  const minutes = Math.max(1, Math.floor(silentSeconds / 60));
  const minutesText = `${minutes} ${plural(minutes, "минута", "минуты", "минут")}`;
  return (
    `⚫️ <b>${escapeHtml(siteName)}</b> молчит ${minutesText}: нет heartbeat ` +
    "от Alertmanager (питание/интернет/Proxmox/K3s/Alertmanager)."
  );
}

function reminderText(siteName, silentSeconds) {
  return (
    `⚫️ <b>${escapeHtml(siteName)}</b> всё ещё молчит: нет heartbeat ` +
    `от Alertmanager уже ${formatDuration(silentSeconds)}.`
  );
}

function recoveryText(siteName, downSeconds) {
  return (
    `🟢 Связь с <b>${escapeHtml(siteName)}</b> восстановлена. ` +
    `Молчал ${formatDuration(downSeconds)}.`
  );
}

// A ping always refreshes `last_ping_unix`. The state is written only when it
// actually changes, so the steady 5-minute ping costs exactly one KV write.
export function decidePing(state, now, cfg) {
  const current = normalizeState(state);
  const writes = { lastPingUnix: now };
  const actions = [];

  if (current.status === "down") {
    const since = current.downSinceUnix ?? current.lastPingUnix ?? now;
    actions.push({ type: "recovery", text: recoveryText(cfg.siteName, now - since) });
    writes.status = "up";
    writes.downSinceUnix = null;
    writes.lastReminderUnix = null;
  }

  return { actions, writes };
}

// Cron never alerts before the first ping: a freshly deployed Worker must not
// page anyone until Alertmanager has had a chance to send Watchdog.
export function decideCron(state, now, cfg) {
  const current = normalizeState(state);
  if (current.lastPingUnix === null) {
    return { action: null, writes: {} };
  }

  if (current.status === "down") {
    const lastReminder =
      current.lastReminderUnix ?? current.downSinceUnix ?? current.lastPingUnix;
    const due = cfg.remindSeconds > 0 && now - lastReminder >= cfg.remindSeconds;
    if (!due) return { action: null, writes: {} };
    return {
      action: { type: "reminder", text: reminderText(cfg.siteName, now - current.lastPingUnix) },
      writes: { lastReminderUnix: now },
    };
  }

  const silentFor = now - current.lastPingUnix;
  if (silentFor <= cfg.graceSeconds) {
    return { action: null, writes: {} };
  }

  return {
    action: { type: "down", text: downText(cfg.siteName, silentFor) },
    writes: {
      status: "down",
      // Silence started at the last received ping, so a recovery message can
      // report the real outage length, not just the detection delay.
      downSinceUnix: current.lastPingUnix,
      lastReminderUnix: now,
    },
  };
}

// Alertmanager sends `{"status":"firing"|"resolved", ...}`. Watchdog is routed
// with send_resolved:false, but ignore an explicit resolved payload anyway so
// a stray one cannot masquerade as proof of life.
export function parseAlertmanagerPayload(body) {
  if (!body) return { resolved: false };
  try {
    const parsed = JSON.parse(body);
    return { resolved: parsed?.status === "resolved" };
  } catch {
    return { resolved: false };
  }
}

// Length and content are compared with byte-wise XOR. This is not a substitute
// for a MAC, but it avoids leaking the matching prefix through early exits.
export function constantTimeEqual(left, right) {
  const a = encoder.encode(String(left));
  const b = encoder.encode(String(right));
  let difference = a.length ^ b.length;
  const length = Math.max(a.length, b.length);
  for (let index = 0; index < length; index += 1) {
    difference |= (a[index] ?? 0) ^ (b[index] ?? 0);
  }
  return difference === 0;
}
