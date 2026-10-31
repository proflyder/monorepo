# currency-bot

Go port of the Kotlin `tg-bot-currency` service. One static binary scrapes
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
  relative to the model, exactly as before: "Покупка" prints `sell`,
  "Продажа" prints `buy`.
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
- **Time zone.** The Kotlin service used the JVM default zone; the production
  container had no `TZ`, so schedules effectively ran in **UTC**. This port
  defaults `TZ=UTC` and accepts any IANA zone.
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
| `GET` | `/readyz` | none | Readiness; pings SQLite. |
| `GET` | `/metrics` | none | Prometheus text exposition. |
| `GET` | `/api/history` | Bearer | Full history or a filtered range. |
| `GET` | `/api/latest` | Bearer | Newest stored snapshot (`404` when empty). |
| `DELETE` | `/api/history` | Bearer | Deletes all history (not `sent_alerts`). |
| `POST` | `/api/trigger` | Bearer | Runs the pipeline in-process, always sends. |

Auth is `Authorization: Bearer <API_TOKEN>` with a constant-time comparison.
This replaces Unkey; there is no external verification call and no Swagger UI.

`/api/history` filters:

- `?date=YYYY-MM-DD` → `[00:00Z, next midnight]`, inclusive on both ends;
- `?from=RFC3339&to=RFC3339` (both required) → inclusive range;
- otherwise the full history, newest first.

An unparseable `date`, `from` or `to` returns `400 Bad Request` with the usual
error envelope and `errorCode=BAD_REQUEST`.

Response shapes match the Kotlin DTOs:

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
response, which would re-run `/trigger`. Those failures are logged only. The
webhook is intended to be exposed through a Cloudflare Tunnel.

Commands are matched by prefix, like the Kotlin `startsWith` dispatch:
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

## Importing the H2 data

Stop the old bot first, then export both tables with H2 Shell from the old
image. The fat jar already contains the H2 classes, so `/app/app.jar` is the
classpath. The compose service is `currency-bot` and its data volume is
`currency-bot-data`.

```bash
# 1. stop the old bot (H2 must not be open by two processes)
docker compose stop currency-bot

# 2. export both tables to ./export on the host
mkdir -p export
docker compose run --rm -v "$PWD/export:/export" --entrypoint sh currency-bot -c '
java -cp /app/app.jar org.h2.tools.Shell \
  -url "jdbc:h2:file:/app/data/currency-history;IFEXISTS=TRUE" \
  -user sa -password "" \
  -sql "CALL CSVWRITE('"'"'/export/currency_history.csv'"'"', '"'"'SELECT * FROM CURRENCY_HISTORY'"'"');
        CALL CSVWRITE('"'"'/export/sent_alerts.csv'"'"', '"'"'SELECT * FROM SENT_ALERTS'"'"');"
'
```

If the deployment is not the compose one, run the same `java -cp …` command in
a one-off container with the old database volume mounted at `/app/data`.
H2 quoting (`"` around fields, `""` for an embedded quote) is handled by the
importer; both `,` and `;` separators are auto-detected.

```bash
# 3. import into SQLite (idempotent: re-running skips existing timestamps)
go run ./solutions/currency-bot/cmd/currency-bot-import \
  -db /data/currency-bot.db \
  -history ./export/currency_history.csv \
  -alerts ./export/sent_alerts.csv
```

Migrate at least 35 days of history (the alert lookbacks need it) and all
`sent_alerts` rows, otherwise the first run after migration re-sends alerts that
were already delivered.

## Build and test

```bash
go vet ./solutions/currency-bot/...
go test -race ./solutions/currency-bot/...
bazel build //solutions/currency-bot/...   # includes :currency-bot_image
bazel test  //solutions/currency-bot/...
```

Tests are hermetic: `httptest` fakes cover Telegram and kurs.kz, SQLite uses a
temporary file, and the parser fixture in `internal/rates/testdata` is a captured
kurs.kz response with names, addresses, phones and coordinates stripped.

The OCI image follows the scheduler pattern: one static linux/amd64 binary on
the digest-pinned distroless base, built without a Docker daemon. Targets:
`//solutions/currency-bot:currency-bot_image`, `:currency-bot_load`,
`:currency-bot_push` (repository `ghcr.io/proflyder/currency-bot`).
