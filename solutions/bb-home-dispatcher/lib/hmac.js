import { createHmac, timingSafeEqual } from "node:crypto";

export const SIGNATURE_HEADER = "x-bb-dispatch-signature";
export const TIMESTAMP_HEADER = "x-bb-dispatch-timestamp";
export const CLOCK_WINDOW_SECONDS = 30;

// The signed string is "<unix-seconds>\n<exact body bytes as UTF-8>". Binding
// the timestamp into the digest prevents replaying a captured body with a
// fresher timestamp, and the caller's window is checked independently.
export function signaturePayload(timestamp, body) {
  return `${timestamp}\n${body}`;
}

export function sign(secret, timestamp, body) {
  return createHmac("sha256", secret)
    .update(signaturePayload(timestamp, body), "utf8")
    .digest("hex");
}

export function verifySignature({
  secret,
  timestamp,
  signature,
  body,
  nowMs = Date.now(),
  windowSeconds = CLOCK_WINDOW_SECONDS,
}) {
  if (typeof secret !== "string" || secret.length === 0) {
    return { ok: false, code: "not_configured" };
  }
  if (typeof timestamp !== "string" || !/^\d{1,12}$/u.test(timestamp)) {
    return { ok: false, code: "invalid_timestamp" };
  }
  const seconds = Number(timestamp);
  const nowSeconds = Math.floor(nowMs / 1000);
  if (Math.abs(nowSeconds - seconds) > windowSeconds) {
    return { ok: false, code: "stale_timestamp" };
  }
  if (typeof signature !== "string" || !/^[0-9a-f]{64}$/u.test(signature)) {
    return { ok: false, code: "invalid_signature" };
  }
  const expected = sign(secret, timestamp, body);
  const actual = Buffer.from(signature, "hex");
  const wanted = Buffer.from(expected, "hex");
  if (actual.length !== wanted.length || !timingSafeEqual(actual, wanted)) {
    return { ok: false, code: "invalid_signature" };
  }
  return { ok: true };
}
