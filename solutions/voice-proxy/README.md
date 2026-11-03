# voice-proxy

An OpenAI-compatible passthrough that makes assistant replies safe for
Piper TTS. It replaces `projects/home-assistant/hermes/voice_proxy.py`
behaviour-for-behaviour (decision 35) and is deployed as a distroless image
next to Hermes.

```text
Home Assistant conversation agent
  → POST /v1/chat/completions        this proxy
  → POST /v1/chat/completions        Hermes (HERMES_UPSTREAM)
  ← SSE / JSON                       links, citations and Markdown stripped
```

The proxy is **not** an authentication boundary. The client `Authorization`
header is forwarded verbatim, exactly like the Python version; network policy in
front of the Pod is the boundary. The default bind is loopback.

## HTTP surface

| Method | Path | Behaviour |
|---|---|---|
| `GET` | any | Blind passthrough to Hermes; status, body and `Content-Type` relayed |
| `POST` | `/v1/chat/completions` | Cancel guard → (voice router) → `model_options.reasoning_effort` → buffered or streaming relay |
| `POST` | any other | Body is parsed and re-serialised, then proxied |
| other | — | `501`, like `http.server`, which only implements GET and POST |

* **Cancel guard.** An exact, normalised match of the last user message against
  the cancel phrases answers `" "` locally (HTTP 200, `id=chatcmpl-ricardo-cancel`)
  and never opens a Hermes turn. A whitespace-only body keeps HA's assistant
  message valid while `tts_input.strip()` still skips TTS.
* **Streaming.** Only `content` deltas and the `assistant` role are forwarded;
  the chunk fields `id`, `object`, `created`, `model` are whitelisted;
  `reasoning_content`, tool calls, tool progress and status events never reach
  HA. `[DONE]` is emitted only after every choice reported a `finish_reason`, and
  a finish frame is always written after the tail it belongs to. A mid-stream
  failure emits exactly one `{"error":…}` SSE frame and closes, never a second
  HTTP response.
* **Buffered.** `choices[].message.content` of a `200` reply is sanitised;
  everything else is relayed unchanged.
* **Timeouts.** 180 s per upstream request. Responses are close-delimited
  (`Connection: close`) like the old HTTP/1.0 server, and SSE frames are flushed
  one at a time so Piper can start on the first sentence.
* **Client disconnect** cancels the upstream request context, so a closed HA
  connection also interrupts the Hermes turn.
* **Body limit.** 10 MiB; a larger request is answered with the OpenAI-shaped
  503 error.
* **Errors.** Any upstream `>= 400` is normalised: a JSON `error` object is kept,
  a string `error` is wrapped, anything else becomes
  `{"error":{"message":"Hermes upstream returned HTTP <n>","type":"server_error","code":"upstream_error"}}`.
  HA's llama_cpp adapter calls `.get` on the error value, so it is always an
  object.

## Sanitising

`sanitize_text` removes whole citation parentheses, unwraps Markdown links,
deletes bare URLs, strips `#`/list prefixes and `*_` backtick `~` marks,
drops empty parentheses and collapses whitespace. `StreamingSanitizer` releases
complete sentences only at a boundary outside links/parentheses and followed by
whitespace, holds the tail until the stream ends, and never speaks a truncated
link or `http` fragment.

Whitespace classes are written out explicitly (`[\p{Z} …]`) instead of `\s`,
because RE2's `\s` is ASCII-only while Python's is Unicode; the parity corpus in
`testdata/parity/` pins that difference (NBSP, U+202F, NEL, ideographic space).

## Configuration

All values come from the environment. Defaults match `voice_proxy.py`; a
default applies only when the variable is **unset**, not when it is set to an
empty string.

| Variable | Default | Meaning |
|---|---|---|
| `HERMES_UPSTREAM` | `http://127.0.0.1:8642` | OpenAI-compatible upstream |
| `HERMES_PROXY_BIND` | `127.0.0.1` | Listen address |
| `HERMES_PROXY_PORT` | `8643` | Listen port |
| `HERMES_VOICE_REASONING_EFFORT` | `low` | Injected as `model_options.reasoning_effort` |
| `HERMES_HA_URL` | — | Home Assistant base URL (routing) |
| `HERMES_HA_TOKEN_FILE` | — | HA token file; wins over the inline value |
| `HERMES_HA_TOKEN` | — | HA token, inline fallback |
| `HERMES_VOICE_GATEWAY_URL` | — | mcp-gateway voice base URL (routing) |
| `HERMES_VOICE_GATEWAY_TOKEN_FILE` | — | Gateway token file |
| `HERMES_VOICE_GATEWAY_TOKEN` | — | Gateway token, inline fallback |
| `HERMES_VOICE_PROFILE_ENTITY` | `input_select.ricardo_voice_profile` | Profile `input_select` |
| `HERMES_VOICE_PROFILE_CACHE_MS` | `2000` | Successful-read cache TTL |
| `HERMES_HA_TIMEOUT` | `3` | HA request timeout, seconds |
| `HERMES_VOICE_GATEWAY_TIMEOUT` | `5` | Gateway request timeout, seconds |
| `HERMES_VOICE_RESULT_TIMEOUT` | `900` | Result long-poll budget, seconds |
| `HERMES_VOICE_RESULT_RETRY` | `2` | Result retry pause, seconds |
| `HERMES_VOICE_RESULT_WAIT` | `25` | Result long-poll wait, seconds |
| `HERMES_VOICE_UPDATES` | `1` | Dispatcher updates poller; `0/false/no/off` disables |
| `HERMES_VOICE_UPDATES_RETRY` | `2` | Updates retry minimum, seconds |
| `HERMES_VOICE_UPDATES_RESCAN` | `300` | Bootstrap rescan window with the queue on |
| `HERMES_VOICE_ANNOUNCE_MAX_CHARS` | `200` | Announcement truncation limit |
| `HERMES_VOICE_ANNOUNCE_SCRIPT` | `script.schedule_announce` | Statement announcement script |
| `HERMES_VOICE_SATELLITE` | `assist_satellite.iandeks_stantsiia_midi_assist_satellite` | Satellite entity |
| `HERMES_VOICE_MEDIA_PLAYER` | `media_player.iandeks_stantsiia_midi_media_player` | Media player idle gate |
| `HERMES_SCHEDULER_URL` | — | Scheduler base URL (durable queue) |
| `HERMES_SCHEDULER_TOKEN_FILE` | — | Scheduler token file |
| `HERMES_SCHEDULER_TOKEN` | — | Scheduler token, inline fallback |
| `HERMES_VOICE_QUEUE` | `1` | Durable announcement queue; `0/false/no/off` disables |
| `HERMES_VOICE_CONSUMER` | `voice-proxy-home` | Scheduler consumer id |
| `HERMES_VOICE_ANNOUNCE_TTL` | `10800` | Announcement TTL, seconds |

A `*_FILE` variable wins over its inline counterpart; a configured but
unreadable file yields an **empty** secret and does not fall back. Routing is
enabled only when `HERMES_HA_URL`+token **and** `HERMES_VOICE_GATEWAY_URL`+token
are present, so a partially configured deployment keeps the plain sanitiser
behaviour. The queue additionally needs `HERMES_SCHEDULER_URL`+token.

## Diagnostics

The container has no shell, so the two former `kubectl exec … sh -c 'curl …'`
checks are subcommands:

```text
voice-proxy -healthcheck   # load config, then GET <upstream>/v1/models
voice-proxy -status        # redacted JSON summary; never a token value
```

Logs go to stderr as `voice-proxy: <message>`, with an uppercase `ERROR` level
for real failures so the rollout log scan keeps working. They carry outcome,
profile, counts and timings only — never recognised speech.

## Tests

```text
go test -race -count=1 ./solutions/voice-proxy/...
bazel build //solutions/voice-proxy/...
```

`testdata/parity/*.json` is a golden corpus generated by calling the Python
implementation. The Go tests assert byte-identical output for `sanitize_text`,
`ensure_not_question`, `announce_text` and for the streaming sanitizer fed
char-by-char and at every split index. Python is not needed at test time; the
throwaway generator is deliberately not committed.

## Status

This is the core service: config, sanitising, SSE, upstream passthrough and the
chat pipeline. Profile routing, the result announcer, the dispatcher updates
poller and the durable queue consumer are added in the following steps; the
`proxy.Router` option and the worker hooks in `app.Run` are their extension
points. Until the cutover step the Hermes deployment still runs the Python
proxy.
