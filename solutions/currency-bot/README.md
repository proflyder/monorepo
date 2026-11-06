# currency-bot

One static Go binary scrapes
[kurs.kz](https://kurs.kz), stores USD/KZT and RUB/KZT snapshots in SQLite,
alerts a Telegram chat when a threshold is crossed, and serves a small internal
JSON API.

## Behaviour

- **Hourly scrape.** At `HOURLY_MINUTE` (default `0`) the bot fetches
  `https://kurs.kz`, extracts the embedded `var punkts = [...]` array, removes
  IQR outliers per field and averages the best five quotes:
  - `sell` = lowest five (cheapest rate when you buy currency),
  - `buy` = highest five (best rate when you sell currency).
  The `buy`/`sell` labels in the Telegram message are **deliberately inverted**
  relative to the model: "Покупка" prints `sell`,
  "Продажа" prints `buy`.
  A non-2xx response or a body larger than 10 MiB fails the job instead of
  being parsed. A non-positive or non-finite rate is skipped by the threshold
  check, so a broken historical row cannot produce a `+Inf` alert.
- **Alerts only.** The scheduled job does **not** broadcast rates. It sends a
  message only when at least one threshold fires. `/help` still claims an hourly
  broadcast; the text is kept verbatim for compatibility.
- **Thresholds** are evaluated in this order: HOUR, DAY, WEEK, MONTH. Each
  period compares against the newest stored record that is **at least** that
  old (not the record closest to the cutoff).

  | Period | Lookback | Warning | Critical | Re-alert band |
  |---|---|---|---|---|
  | HOUR | 1h | 0.5% | 1.0% | 0.25% |
  | DAY | 24h | 1.0% | 2.0% | 0.50% |
  | WEEK | 168h | 2.0% | 4.0% | 1.00% |
  | MONTH | 720h (30 days) | 3.0% | 5.0% | 1.50% |

  Per period the four checks run in this order: USD sell, USD buy, RUB sell,
  RUB buy. Thresholds are inclusive.
- **Deduplication** uses `sent_alerts` keyed by `(pair, period, rate_type)`:
  1. no previous alert → send;
  2. stored WARNING, candidate CRITICAL → send;
  3. direction changed → send;
  4. rate moved at least the re-alert band since the last alert → send;
  5. otherwise suppress.
  When a threshold no longer fires, the corresponding row is cleared.
- **Retention.** A weekly job at `CLEANUP_WEEKDAY` `CLEANUP_HOUR` (default
  Sunday 00:00) deletes history older than 35 days.
- **Time zone.** Schedules run in `TZ` (default `UTC`, any IANA zone).
- **Manual trigger asymmetry (kept intentionally).** `/trigger` and
  `POST /api/trigger` skip dedup reads and always send, but still write
  `sent_alerts` and never save history. This can suppress the next scheduled
  alert until the rate leaves the re-alert band. The spec identifies this as a
  quirk, not a bug to fix.

## Configuration

All settings come from the environment. The process fails fast when a required
variable is missing.

| Variable | Required | Default | Meaning |
|---|---|---|---|
| `BOT_TOKEN` | yes | — | Telegram bot token. Never logged. |
| `CHAT_ID` | yes | — | Allowlist of chat ids, comma separated. The first is the default scheduled/trigger chat. Messages from other chats are ignored. |
| `WEBHOOK_SECRET` | yes | — | Expected `X-Telegram-Bot-Api-Secret-Token`. Never logged. |
| `API_TOKEN` | yes | — | Bearer token for the internal API. Never logged. |
| `WEBHOOK_URL` | no | — | When set, calls `setWebhook` at startup with `allowed_updates=["message"]`. |
| `DB_PATH` | no | `/data/currency-bot.db` | SQLite file (WAL enabled). |
| `HTTP_ADDR` | no | `:8080` | Internal API listener. |
| `WEBHOOK_ADDR` | no | `:8081` | Telegram webhook listener (only `/telegram/webhook`). |
| `TZ` | no | `UTC` | IANA zone for both schedules. |
| `KURS_URL` | no | `https://kurs.kz` | Rates page (tests/overrides). |
| `TELEGRAM_API_URL` | no | `https://api.telegram.org` | Bot API base URL (tests). |
| `HOURLY_MINUTE` | no | `0` | Minute of every hour for the scrape job. |
| `CLEANUP_WEEKDAY` | no | `sunday` | `0..6` or a weekday name. |
| `CLEANUP_HOUR` | no | `0` | Hour of the weekly cleanup. |
| `LOG_LEVEL` | no | `info` | `debug`, `info`, `warn`, `error`. |

There is no `.env` loader and no secret has a default.

## HTTP

### Main listener (`HTTP_ADDR`, default `:8080`)

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/` | none | `Hello World!` compatibility probe. |
| `GET` | `/healthz` | none | Liveness. |
| `GET` | `/readyz` | none | Readiness; pings SQLite and, when `WEBHOOK_URL` is set, the Telegram webhook registration. |
| `GET` | `/metrics` | none | Prometheus text exposition. |
| `GET` | `/api/history` | Bearer | Full history or a filtered range. |
| `GET` | `/api/latest` | Bearer | Newest stored snapshot (`404` when empty). |
| `DELETE` | `/api/history` | Bearer | Deletes all history (not `sent_alerts`). |
| `POST` | `/api/trigger` | Bearer | Runs the pipeline in-process, always sends. |

Auth is `Authorization: Bearer <API_TOKEN>` with a constant-time comparison.

`/api/history` filters:

- `?date=YYYY-MM-DD` → `[00:00Z, next midnight)`, so a record at exactly
  `00:00:00.000` belongs only to that day;
- `?from=RFC3339&to=RFC3339` (both required) → inclusive range;
- otherwise the full history, newest first.

An unparseable `date`, `from` or `to` returns `400 Bad Request` with the usual
error envelope and `errorCode=BAD_REQUEST`.

Response shape:

```json
{"records":[{"timestamp":"2026-10-31T12:00:00Z",
  "rates":{"usdToKzt":{"buy":485.5,"sell":487.2},
           "rubToKzt":{"buy":4.85,"sell":4.92}}}],"totalCount":1}
```

```json
{"timestamp":"2026-10-31T12:00:00Z",
 "rates":{"usdToKzt":{"buy":485.5,"sell":487.2},
          "rubToKzt":{"buy":4.85,"sell":4.92}}}
```

```json
{"message":"Currency rates updated and sent to Telegram successfully","executionTimeMs":123}
```

Errors use the compatibility envelope (`timestamp`, `status`, `error`,
`message`, `path`, `errorCode`, `service`, optional `details`). Trigger failures
return `502 EXTERNAL_SERVICE_ERROR` with
`Failed to update and send currency rates: <cause>`.

### Webhook listener (`WEBHOOK_ADDR`, default `:8081`)

Only `POST /telegram/webhook` is served. Requests must carry
`X-Telegram-Bot-Api-Secret-Token: <WEBHOOK_SECRET>` (constant-time compare);
otherwise `401`. A request with a valid secret always gets `200 OK`, even when
the JSON is malformed or handling fails: Telegram retries every non-2xx
response, which would re-run `/trigger`. `/trigger` is acknowledged first and
runs in the background, and a seen `update_id` is ignored, so a retried update
cannot fetch and send twice. Failures are logged only. The webhook is intended
to be exposed through a Cloudflare Tunnel.

Commands are matched by prefix:
`/trigger`, `/start`, `/help`. Replies go to the chat that sent the command, and
only when that chat is in `CHAT_ID`. Other chats are logged by id only (never
with message text) and ignored. `/trigger` sends no acknowledgement on success;
the rates message itself is the response. On failure the sender gets
`❌ Не удалось обновить курсы: <error>`.

## Metrics

| Metric | Type | Labels |
|---|---|---|
| `currency_bot_rate` | gauge | `pair` (`USD_TO_KZT`, `RUB_TO_KZT`), `type` (`BUY`, `SELL`) |
| `currency_bot_job_runs_total` | counter | `job` (`hourly`, `trigger`, `cleanup`) |
| `currency_bot_job_failures_total` | counter | `job` |
| `currency_bot_parse_duration_seconds` | histogram | `job` |
| `currency_bot_telegram_send_failures_total` | counter | — |
| `currency_bot_http_requests_total` | counter | `route`, `status` |

## Logs

JSON via `log/slog` to stdout (no files, no Promtail). Every line has `level`
and `msg`; business and HTTP lines add `event`:

- `event=currency_rate_updated` with `currency_from`, `currency_to`,
  `rate_buy`, `rate_sell`, `timestamp` (epoch ms);
- `event=http_request` with `http_method`, `http_path`, `http_status`,
  `duration_ms`;
- `event=job_started`, `job_failed`, `cleanup_completed`.

`BOT_TOKEN`, `API_TOKEN`, `WEBHOOK_SECRET` and message bodies are never logged.

## Storage

SQLite (`modernc.org/sqlite`, WAL) with the H2 tables translated 1:1:

- `currency_history(id, timestamp, usd_buy, usd_sell, rub_buy, rub_sell)`,
  index on `timestamp`; `timestamp` is Unix milliseconds UTC.
- `sent_alerts(id, pair, period, rate_type, level, direction, rate_at_alert,
  change_percent)` with a unique index on `(pair, period, rate_type)`.

## H2 import

`cmd/currency-bot-import` imports CSV exports of the former H2 tables
(`currency_history`, `sent_alerts`) into SQLite; it is idempotent and skips
existing timestamps. The home deployment and its history live in
`projects/home/kubernetes/currency-bot/README.md`.

## Build and test

```bash
go vet ./solutions/currency-bot/...
go test -race ./solutions/currency-bot/...
bazel build //solutions/currency-bot/...   # includes :currency-bot_image
bazel test  //solutions/currency-bot/...
```

Tests are hermetic: `httptest` fakes cover Telegram and kurs.kz, SQLite uses a
temporary file, and the parser fixture is a captured kurs.kz response with names,
addresses, phones and coordinates stripped.

`testdata/` is the language-neutral contract used when the service is ported to
Rust:

- `testdata/contract/*.json` — HTTP goldens in the `packages/go/contracttest`
  format (exact paths, statuses, significant headers, bodies);
- `testdata/vectors/*.json` — domain vectors for parsing/IQR/best-five, alert
  thresholds and dedup, exact Telegram HTML (including `.x5` rounding),
  command texts and schedules (UTC, Asia/Almaty and DST);
- `testdata/store/schema_v1.sql` — schema v1 with sample rows; the store test
  loads it, reads the rows and inserts again.

Regenerate with `go test ./solutions/currency-bot/internal/<pkg> -update`; the
Rust port must replay the committed bytes unchanged.

The OCI image follows the scheduler pattern: one static linux/amd64 binary on
the digest-pinned distroless base, built without a Docker daemon. Targets:
`//solutions/currency-bot:currency-bot_image`, `:currency-bot_load`,
`:currency-bot_push` (repository `ghcr.io/proflyder/currency-bot`).
