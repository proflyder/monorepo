# Hermes Agent — voice fallback

**Deployment:** gateway and voice proxy run in one Kubernetes
Pod; native user units are masked. [Current runbook](../../../docs/KUBERNETES-OPERATIONS.md).
State is `/srv/k3s/volumes/runtime-hermes` on the host (`/opt/data` in the Pod);
`~/.local/bin/hermes` is a kubectl wrapper. Native Install/Service commands below
are retained for historical recovery and must not start a second production writer.

Hermes is the **fallback** brain for the Station Midi voice cascade, not the
fast path.  HA Assist keeps timers, alarms, volume, music transport and simple
home commands local; Hermes handles unmatched or open-ended requests through
the full Home Assistant MCP surface and the Music Assistant MCP server.

## Layout

| Item | Value |
|---|---|
| Install | `~/.hermes/hermes-agent` (official installer) |
| Data | `~/.hermes/` |
| Launcher | `~/.local/bin/hermes` |
| Config | `~/.hermes/config.yaml` |
| Secrets | `~/.hermes/.env` (mode `600`); in K3s, `Secret/hermes-environment` |
| MCP HA | `http://192.0.2.28:8123/api/mcp` (HA MCP Server, streamable HTTP) |
| MCP MA | `http://192.0.2.28:8095/mcp/v1` (Music Assistant FastMCP plugin; HAOS add-on) |
| MCP infra | `http://mcp-gateway.home-assistant.svc.cluster.local:8080/mcp` (cluster-internal) |
| Telegram | Long polling with the dedicated Hermes/infra bot (`TELEGRAM_BOT_TOKEN`) |
| API | `http://127.0.0.1:8642/v1` (OpenAI-compatible, loopback only) |
| Model | ChatGPT/Codex subscription via `openai-codex` |

The generated Hermes config is large; [`config.fragment.yaml`](config.fragment.yaml)
contains the reviewed overlay for the three MCP servers, model provider, web
search, platform toolsets and the Telegram channel.  The HA voice prompt lives
in `voice_prompt.txt`; the canonical Hermes identity lives
in [`SOUL.md`](SOUL.md); the infra context that becomes `/opt/data/.hermes.md`
lives in [`HERMES.md`](HERMES.md); the TTS sanitizer lives in
[`voice_proxy.py`](voice_proxy.py).  Do not copy secrets into this directory.

## Render the overlay into the live home

`config.yaml`, `.hermes.md` and `SOUL.md` live on the PVC
(`/srv/k3s/volumes/runtime-hermes` on lab-host, `/opt/data` in the Pod) and are not
watched by CI.  [`render_hermes_config.py`](render_hermes_config.py) is the
only reviewed path from this directory to that volume:

```bash
cd <repo>
# 1. see what would change (prints a redacted diff, writes nothing)
python3 projects/home-assistant/hermes/render_hermes_config.py \
  --home /srv/k3s/volumes/runtime-hermes \
  --private-values secrets/home/hermes-private.yaml --dry-run

# 2. render; a changed file keeps its previous version as <name>.prev (600)
sudo python3 projects/home-assistant/hermes/render_hermes_config.py \
  --home /srv/k3s/volumes/runtime-hermes \
  --private-values secrets/home/hermes-private.yaml

# 3. restart the gateway so it re-reads config.yaml and the context files
kubectl -n home-assistant rollout restart deployment/hermes
kubectl -n home-assistant rollout status deployment/hermes --timeout=180s
```

`--private-values` is required on a live home.  The tracked fragment carries
TEST-NET placeholders for the Home Assistant and Music Assistant endpoints;
the private file (copy [`hermes-private.example.yaml`](hermes-private.example.yaml),
git-ignored under `secrets/`) is deep-merged **after** the fragment and keeps
the real addresses.  The renderer refuses to write when a placeholder would
replace a different live value, and lists the offending keys — the only escape
hatch is `--allow-placeholders`, for throw-away test homes.

The merge is deep: unrelated keys in the generated `config.yaml` survive
(including MCP servers the fragment does not mention), the fragment wins for
the keys it declares, private values win over both, and lists are replaced.
`agent.personalities.voice` is not in the fragment: it is set from
[`voice_prompt.txt`](voice_prompt.txt), the single source of truth for the
Ricardo persona.  The renderer also installs `skills/` (currently
`skills/report`) into `<HERMES_HOME>/skills` and, when the private file has
`telegram_owner_chat_id`, the Telegram channel prompt and home channel from
`telegram_prompt.txt` — see «Telegram-промпт» below.  Comments in a hand-edited `config.yaml` are not preserved —
`config.yaml.prev` is the one-step rollback (`cp config.yaml.prev config.yaml`
+ restart).  A second run is a no-op, and `--check` exits `1` when a render is
pending (pre-flight use).

Run it on lab-host against the host path, not inside the Pod: the Pod has no
Python tooling guarantee, and the files are the same.  Stop the gateway first
only when you need a consistent read; a restart after the render is enough
because the gateway reads the config once at start.  Run as the user that owns
`/srv/k3s/volumes/runtime-hermes`; if you had to use `sudo`, restore ownership of the
rendered files to the Pod's uid (`kubectl -n home-assistant exec
deploy/hermes -c hermes -- id`) and check `ls -l`, otherwise the gateway cannot
open its own config.

## Infra tools (mcp-gateway)

`mcp_servers.infra` points at the shared `mcp-gateway` Service in the same
namespace.  It exposes `infra_status`, `infra_service`, `infra_logs`,
`infra_query`, `infra_alert`, `infra_overview`, `infra_report` (read-only) and
the two-step `infra_prepare_restart` → `infra_confirm` / `infra_cancel`, plus
`infra_send_report`.  `infra_report` is the full house report (K3s, HA, MA,
station, alerts) used by `/report` and by the voice path when it only needs a
verdict and an offer to send details.  The restart confirmation is a normal tool pair because
the voice path cannot show Hermes approval prompts; `trust: full` is required
for the same reason.

The bearer token is not in this repository.  It is one OpenBao key,
`k8s/home-assistant/mcp-gateway-auth` property `token`, delivered to the Pod as
the environment variable `HERMES_INFRA_TOKEN`.  The
`ExternalSecret/hermes-environment` uses `dataFrom.extract`, so adding the key
to OpenBao is enough on the ESO side; whether the Deployment consumes the
Secret with `envFrom` or per-key `secretKeyRef` is defined in the private
`runtime_values`.  If it uses per-key references, add an entry for
`HERMES_INFRA_TOKEN` there as well, otherwise the variable stays unset and the
gateway answers `401`.

`HERMES.md` is installed as `/opt/data/.hermes.md`.  Hermes auto-loads that
file from the working directory (`/opt/data`, no git root) and injects it into
the system prompt; it must stay under 2 KB and must not contain instruction
override wording — the injection scanner blocks such a file entirely.

Verify from the Pod (the tokens are already in the Pod environment from
`Secret/hermes-environment`; the Pod also runs `voice-proxy`, hence `-c`):

```bash
kubectl -n home-assistant exec deploy/hermes -c hermes -- hermes mcp test infra
kubectl -n home-assistant exec deploy/hermes -c hermes -- hermes mcp list
```

A healthy `mcp test infra` reports the tool count; `401` means the token is
missing or stale, `connection refused` means the gateway Service or its Pod is
not ready.

## Agents (bb tools)

The same `infra` MCP server carries the `bb.read`/`bb.act` group, so no
toolset change is needed: `bb_status` (running personal threads),
`bb_prepare_spawn(project, model?, effort?, task)`, `bb_confirm()` and
`bb_cancel()`.  Both prompts and `HERMES.md` describe the flow:

1. «запусти агента в <проект>[, модель …][, усилие …]: <задача>» →
   `bb_prepare_spawn` is called straight away with the project name **verbatim**
   as heard: alias matching (and any transliteration tolerance) belongs to the
   gateway, so Hermes never re-asks or rewrites the name first. The gateway
   answers with a confirmation question and stores **one** pending spawn for
   90 s. Only a «project not found» answer is repeated to the owner, followed by
   one clarifying question;
2. Hermes repeats that question verbatim and stops;
3. «подтверди»/«да» in the next exchange → `bb_confirm` (it also switches the
   voice path into agent mode for the new thread); «отмена»/«не надо» →
   `bb_cancel`.

Hermes is the **personal** profile only.  The allowlist
(`projects/home/kubernetes/mcp-gateway/manifests/bb-targets.yaml`, rendered by
`deploy.sh` from `BB_PROJECT_STATION_LAB` / `BB_PROJECT_ENGLISH_NOTES`) lists
just the personal projects; a work project or an unknown alias produce the same
«Не нашёл такой личный проект.», so Hermes cannot enumerate work projects.
Asking «по работе» must be answered by switching the column profile
(«Рикардо, рабочий профиль»), never by guessing a project.  Models and effort
levels are only passed through when the owner names them, and the gateway still
validates them against the dispatcher's live list.

Rollout: prompts and context are installed by the renderer above (then restart
`hermes`); the target list and the gateway tool group ship with
`projects/home/kubernetes/mcp-gateway/deploy.sh`.  Verify with `hermes mcp test
infra` (the tool count includes the four bb tools) and a spoken «что делают
агенты?».

## Telegram channel

The same bot carries Alertmanager alerts and the owner's chat; Hermes consumes
its updates, so that bot must not have a webhook (`TELEGRAM_WEBHOOK_URL`
unset — polling only).  Values from `Secret/hermes-environment`:
`TELEGRAM_BOT_TOKEN` and `TELEGRAM_ALLOWED_USERS` (owner numeric id; for a
private chat it is also the chat id).  `platform_toolsets.telegram` is an
explicit list, so a chat message cannot reach terminal/file/browser tools.
Alertmanager reads the same OpenBao keys, so the token has one source of truth;
the currency bot keeps its own bot for rate alerts.  Reply to an alert message
to quote its fingerprint into the model, then ask `infra_alert` about it.


## Telegram-промпт и `/report`

Hermes applies `display.personality: voice` to every platform, so Telegram
replies used to be cut to the 180-character voice limit even when the tool
returned a full report.  The fix is a per-chat override, which Hermes reads
from `platforms.telegram.channel_overrides[<chat_id>].system_prompt`.

- `telegram_prompt.txt` is the reviewed prompt for the owner's Telegram chat
  (long structured answers, `/report`, the full MCP tool set, no `exec`).
- The private `telegram_owner_chat_id` (numeric id, same value as the owner in
  `TELEGRAM_ALLOWED_USERS`) is renderer input only.  The renderer writes the
  channel override and `platforms.telegram.home_channel`
  (`{platform: telegram, chat_id, name}`) so Hermes knows the default delivery
  destination; the key itself is removed from `config.yaml`.
- `skills/report/SKILL.md` installs as `<HERMES_HOME>/skills/report/SKILL.md`.
  A skill directory becomes a slash command on messaging platforms, so
  `/report` starts a normal agent turn that calls `infra_report` (no shell
  `exec` command).  The fragment keeps the command visible with
  `platforms.telegram.extra.command_menu` (60 slots — 100 exceeds Telegram's setMyCommands limit — `report` prepended) and
  disables it on the voice api_server surface
  (`skills.platform_disabled.api_server: [report]`).
- Only our own skill directories are installed; any other skill already on the
  PVC is left untouched, and the renderer never deletes files.

After rendering, restart the gateway; `/report` appears in the bot menu on the
next Telegram connect.  Verify with a `/report` message from the owner account
and check the reply length (it must exceed 180 characters for a real report).

## Cancel guard (proxy v6)

HA's `prefer_local_intents` is skipped for the next utterance when the previous
assistant reply ended with a question (`continue_conversation=true`,
`_intent_agent_only`). During that follow-up turn a spoken «отмена» used to
reach the Hermes model. `voice_proxy.py` now checks only the last message when
its role is `user`, normalizes NFKC/case/whitespace and strips only sentence-
final `.!?…`, then matches the exact existing `StationCancel` phrases. Quotes,
hyphens, internal commas and phrases with extra meaning are forwarded unchanged.
On a match the proxy returns an empty chat completion (JSON or SSE with
`role`, `finish_reason=stop`, `[DONE]`) and never opens the Hermes upstream.
It logs only `cancel guarded count=N stream=... upstream=skipped`.

HA's llama_cpp adapter drops an assistant message whose content is empty, so
the guard sends a single space. The pipeline strips it, skips TTS and keeps
`continue_conversation=false`; the intent response is `action_done` with
whitespace-only speech and `processed_locally=false` (the response came through
the conversation agent, not the local intent path). The proxy image is
digest-pinned in `projects/home/kubernetes/versions.sh`.

## Profile routing (proxy v7)

The proxy is the only place that decides whether a phrase belongs to Hermes or
to an agent. It is disabled unless the full path is configured, so a build
without the new deployment values behaves exactly like v6.

Per `/v1/chat/completions` request, in this order:

1. the cancel guard (unchanged, v6) — never reads the profile;
2. the HA voice profile
   (`GET {HA_URL}/api/states/input_select.ricardo_voice_profile`, Bearer HA
   token, a ≤2 s cache that is only filled by a fresh successful read);
3. `POST {VOICE_GATEWAY_URL}/v1/voice/utterance {text, profile}` with the last
   user message, using the dedicated voice token.

| Gateway `route` | Action |
|---|---|
| `hermes` | forward to Hermes exactly as before |
| `local` | answer immediately with `speech`; nothing reaches Hermes |
| `bb` with `ok:true`, `threadId`, `afterSeq` | answer immediately with `speech`, then announce the turn result later |
| anything else, `ok:false`, or a gateway error | fail-closed short phrase, nothing reaches Hermes in the work profile |

Fail-closed:

- profile unreadable, unknown value, HA unavailable → «Не могу подтвердить
  профиль.»;
- gateway unavailable in the **work** profile → «Сервер агентов недоступен.»;
- gateway unavailable in the **personal** profile → Hermes, because personal
  speech always went there before this change.

`local`/`bb` replies use the same completion shape as the cancel guard (JSON or
SSE with `role`, `content`, `finish_reason=stop`, `[DONE]`), and their speech
never ends with a question so HA does not enable continued conversation.

For `bb`, a daemon thread long-polls `POST /v1/voice/result` (25 s per call,
about 15 min overall) and, on a final answer, speaks it. The text is sanitized
with the same `sanitize_text` and limited to ~200 characters. A timeout or a
gateway error announces nothing; the thread never writes speech to the log.

- A final answer ending with `?` (or `？`) calls
  `POST {HA_URL}/api/services/assist_satellite/start_conversation` with
  `entity_id: {HERMES_VOICE_SATELLITE}`, `start_message: text`,
  `preannounce: false`. The satellite speaks the question and keeps listening.
- Otherwise, or when that call fails, it calls
  `POST {HA_URL}/api/services/script/turn_on` with
  `entity_id: script.schedule_announce` and `variables: {message, chime: false}`.

The announce log line carries `listen=true/false` so the chosen path is visible
without logging speech.

### Dispatcher updates

Turns the owner did not start by voice — `child-completed` system messages and
`[bb message from thread:…]` agent messages — are announced too, but only while
the profile is work. A daemon poller bootstraps `POST /v1/voice/updates`
without `afterSeq` (cursor only, no history), then long-polls with the returned
`cursor`. Each batch re-reads the profile from HA without the cache:

- work: join the `spoken` lines of the batch with a space, sanitize with
  `announce_text`, and speak through `announce` — a question calls
  `assist_satellite.start_conversation`, otherwise `script.schedule_announce`;
- personal or unknown: drop the batch and log only the count and profile.

Owner turns (`initiator: "user"`, including typed UI messages) are never
returned by the gateway. Errors back off 2 s → 60 s; the poller never speaks on
an error and never calls Hermes. The log line is
`voice-proxy: updates items=N announced=… listen=… profile=…` without text.

`HERMES_VOICE_UPDATES=0` disables the poller; it is otherwise on whenever
profile routing is enabled.

Environment (routing is off until all four of URL+token for HA and gateway are
present):

| Variable | Default | Purpose |
|---|---|---|
| `HERMES_HA_URL` | — | HA base URL, for the profile state and the announce service call |
| `HERMES_HA_TOKEN_FILE` / `HERMES_HA_TOKEN` | — | HA long-lived token; the existing `hermes-environment` Secret key can be reused |
| `HERMES_VOICE_GATEWAY_URL` | — | mcp-gateway base, `http://mcp-gateway.home-assistant.svc.cluster.local:8080` |
| `HERMES_VOICE_GATEWAY_TOKEN_FILE` / `HERMES_VOICE_GATEWAY_TOKEN` | — | voice token from Secret `mcp-gateway-voice` |
| `HERMES_VOICE_UPDATES` | `1` | Dispatcher updates poller; `0` disables it |
| `HERMES_VOICE_PROFILE_ENTITY` | `input_select.ricardo_voice_profile` | profile entity |
| `HERMES_VOICE_PROFILE_CACHE_MS` | `2000` | successful-read cache |
| `HERMES_HA_TIMEOUT` | `3` | HA request timeout, seconds |
| `HERMES_VOICE_GATEWAY_TIMEOUT` | `5` | gateway request timeout, seconds |
| `HERMES_VOICE_RESULT_TIMEOUT` | `900` | overall late-answer budget, seconds |
| `HERMES_VOICE_RESULT_RETRY` | `2` | pause after a failed result poll |
| `HERMES_VOICE_RESULT_WAIT` | `25` | long-poll seconds per request |
| `HERMES_VOICE_ANNOUNCE_MAX_CHARS` | `200` | announced text limit |
| `HERMES_VOICE_ANNOUNCE_SCRIPT` | `script.schedule_announce` | HA script that speaks a late statement |
| `HERMES_VOICE_SATELLITE` | `assist_satellite.iandeks_stantsiia_midi_assist_satellite` | satellite for `start_conversation` when a late answer is a question |

Logs carry the route outcome, profile name, character count and thread id —
never the spoken text. `_route_counts` keeps per-outcome counters for tests and
future metrics; the proxy has no metrics endpoint.


## Persona — Ricardo

The station assistant is **Рикардо**, the user's personal voice assistant, not
"Hermes".  Three layers must agree:

1. `~/.hermes/SOUL.md` — primary identity (`SOUL.md` in this directory is the
   reviewed source).  It explicitly says not to call itself Hermes/Гермес.
2. `agent.personalities.voice` in `~/.hermes/config.yaml` — the active
   `voice` personality, rendered from `voice_prompt.txt`
   (single source; the fragment does not repeat the text).
3. HA `llama_cpp` conversation subentry — prompt from
   `voice_prompt.txt`, title/entity `Рикардо`.

Apply on lab-host after editing the repo files.  The renderer installs
`SOUL.md` and `agent.personalities.voice` (from `voice_prompt.txt`) into the
HERMES_HOME, so no hand edit of the PVC is needed:

```bash
python3 projects/home-assistant/hermes/render_hermes_config.py \
  --home /srv/k3s/volumes/runtime-hermes \
  --private-values secrets/home/hermes-private.yaml --dry-run
sudo python3 projects/home-assistant/hermes/render_hermes_config.py \
  --home /srv/k3s/volumes/runtime-hermes \
  --private-values secrets/home/hermes-private.yaml
kubectl -n home-assistant rollout restart deployment/hermes
```

The HA prompt/title are part of the live `llama_cpp` subentry and are updated
through its subentry reconfigure flow; keep `voice_prompt.txt` as the source.
Verify with a direct conversation call: «как тебя зовут?» must return
«Я Рикардо, ваш личный ассистент».  If it says Hermes, check all three layers
and restart the gateway.

## Install

```bash
# As the service user, on lab-host:
curl -fsSL https://hermes-agent.nousresearch.com/install.sh \
  -o /tmp/hermes-install.sh
bash /tmp/hermes-install.sh --non-interactive --skip-browser --skip-computer-use

# ChatGPT / Codex subscription (device-code flow; approve in a browser):
hermes auth add openai-codex --type oauth --no-browser
hermes model          # choose the Codex/ChatGPT model if needed
```

On this ChatGPT account the Codex backend accepted `gpt-5.6-luna`; it rejected
`gpt-5.4`, `gpt-5.4-mini`, `gpt-5.5`, `gpt-5.3-codex` and
`gpt-5.4-codex` with HTTP 400. The working default is stored in
`model.default`; test a new model with one short chat turn before switching.

If a slow link stalls the installer's Git clone, clone shallowly first and
point the installer at the checkout:

```bash
git clone --depth 1 --branch v0.22.5 \
  https://github.com/NousResearch/hermes-agent.git ~/.hermes/hermes-agent
bash /tmp/hermes-install.sh --non-interactive --skip-browser \
  --skip-computer-use --dir ~/.hermes/hermes-agent
```

## MCP tokens

Create a dedicated long-lived HA token for Hermes (owner/admin is required by
the HA MCP Server's default admin policy) and a separate long-lived Music
Assistant token. Store both only in `~/.hermes/.env` (mode `600`):

```text
HERMES_HA_TOKEN=<token>
HERMES_MA_TOKEN=<token>
API_SERVER_ENABLED=true
API_SERVER_HOST=127.0.0.1
API_SERVER_PORT=8642
API_SERVER_KEY=<random bearer key>
```

The HA MCP Server accepts the HA long-lived token as a normal `Authorization:
Bearer` header. For MA, enable the **MCP Server** plugin in Music Assistant
(`Settings → Providers → Add provider → MCP Server`), then create a token
labelled `MCP — Hermes` in Profile → Long-lived access tokens. In this
deployment the MA plugin is configured with:

- all query, control, edit, delete, resource and prompt capabilities;
- debug and config namespaces enabled;
- `require_auth=true`, `require_confirmation=false`;
- mount path `/mcp/v1`.

The MA MCP token is a secret: never print it or commit `.env`. Keep the MCP
binds on loopback and do not expose `8642`, `/api/mcp` or `/mcp/v1` to the
internet. The MA MCP plugin is marked experimental upstream; rollback is to
remove the `music-assistant` entry from Hermes and/or disable the plugin.

## Web search and general questions

The fallback is not music-only.  `platform_toolsets.api_server` and
`platform_toolsets.telegram` contain `mcp-homeassistant`,
`mcp-music-assistant`, `web` and `mcp-infra`.  General knowledge is
answered by the model;
current facts (news, weather, prices, links) use the `web_search` tool.  The
search backend is `openai-native`, which uses the same ChatGPT/Codex
subscription as inference and needs no extra API key.  `web_extract` falls back
to the keyless provider ring; add a keyed extract backend only if page
extraction becomes a real workload.

Voice uses this only after the local HA intent path fails.  A question such as
"что такое фотосинтез" goes to Hermes; "какой будильник" stays in HA.

## Voice proxy (TTS sanitizer)

HA does not call the Hermes API directly.  It calls the K3s
`hermes-voice-proxy` on the node address:

```text
HA llama_cpp -> http://192.0.2.27:8643/v1 -> Hermes 127.0.0.1:8642
```

The proxy binds `0.0.0.0:8643` in the hostNetwork Hermes Pod. An nftables
rule on the K3s VM allows only HAOS `.28`, the K3s node `.27` and loopback;
other LAN clients are dropped.

The proxy forwards the OpenAI-compatible request using the same bearer key and
sanitizes assistant text before Piper TTS:

- removes `[text](https://...)` and bare URLs;
- removes `([domain](https://...))` source citations;
- strips Markdown headings, emphasis, code ticks and list markers;
- collapses whitespace.

It is a presentation layer, not an authentication boundary; the K3s
nftables rule limits it to HAOS/K3s/loopback.
The HA entry supports streaming through the proxy. Streaming requests receive
cleaned complete sentences as SSE; links split across chunks stay buffered.
Paragraph separators let Piper synthesize each sentence without waiting for
the next one. Reasoning and Hermes tool/status events are never forwarded as
speech. Non-streaming clients still receive sanitized JSON.

The proxy sets `model_options.reasoning_effort=low` for voice requests only
(default controlled by `HERMES_VOICE_REASONING_EFFORT`). Other Hermes clients
keep their configured reasoning. Current runtime facts: [`../../../docs/STATE.md`](../../../docs/STATE.md).
The service is
`~/.config/systemd/user/hermes-voice-proxy.service`:

```bash
systemctl --user enable --now hermes-voice-proxy.service
systemctl --user status hermes-voice-proxy
journalctl --user -u hermes-voice-proxy -n 50 --no-pager
```

Voice answers therefore do not read links aloud.  If the user explicitly asks
for a URL, the proxy still removes it by design; the source name can remain as
plain text.

## MCP contract

**Full access is intentional.** All MCP servers are configured without a
`tools.include`/`tools.exclude` allowlist, with MCP resources and prompts
enabled:

- **Home Assistant** exposes all discovered HA MCP tools (57 at the time of
  deployment), including all intent scripts, lights, media players, timers,
  todos, broadcasts and live context.
- **Music Assistant** exposes all discovered MA MCP tools (86 at deployment),
  including library search, queue/playback/player control, playlists, favorites,
  volume, announcements, debug and config namespaces.
- **infra** (mcp-gateway) exposes the read-only diagnostics, the two-step
  restart pair and `infra_send_report`; every restart target is allow-listed by
  the gateway catalog, and the gateway itself performs only a rollout restart.

Non-HA platform toolsets remain disabled: terminal, shell, file, browser,
computer-use and similar. Only the MCP servers and `web` are enabled, on both
`api_server` and `telegram`. The HA voice prompt still tells Hermes that local
intents own timers/alarms; the MCP surface is broader than the voice policy.

This is a deliberate tradeoff: Hermes can now do anything the HA/MA MCP servers
expose. MA config/debug tools and destructive library/queue operations are
reachable. The MA MCP plugin is experimental. Keep the tokens private and both
endpoints loopback-only.

## Verify

```bash
set -a; . ~/.hermes/.env; set +a
hermes mcp list
hermes mcp test homeassistant
hermes mcp test music-assistant
hermes tools list        # interactive; run in a real terminal
hermes doctor
```

A healthy MCP test reports the server, a discovery count and the selected
tools. A `401` means the corresponding token is stale; a `404` means the HA
MCP Server integration or the MA MCP plugin is not configured/loaded. `mcp
list` should show `homeassistant → all` and `music-assistant → all`.

## Service

Use the official gateway service after the model auth succeeds:

```bash
hermes gateway install
hermes gateway start
hermes gateway status
```

The API server listens on `127.0.0.1:8642` only.  HA connects to it from the
same host with the `API_SERVER_KEY`; do not bind it to `0.0.0.0`.

## Voice wiring

The HA Assist pipeline remains the front door.  The deployed `Рикардо`
pipeline (`01m5t7gvhp2c3bmvn3bw5emje7`) uses:

- `conversation_engine: conversation.hermes_agent`
- `conversation_language: ru`
- `prefer_local_intents: true`
- STT `stt.onnx_asr` (GigaAM v3 CTC int8, primary); Whisper `medium-int8`
  remains the fallback
- Piper `tts.piper`; текущий voice и язык — [`../../../docs/STATE.md`](../../../docs/STATE.md), установка/откат — [`../tts/puriso/README.md`](../tts/puriso/README.md)
- voice prompt: Рикардо, 1–2 short sentences, max 180 characters, no URL/Markdown
- identity: `SOUL.md` + `agent.personalities.voice` + HA prompt; tested answer
  «как тебя зовут» → «Я Рикардо, ваш личный ассистент»
- `assist_satellite_responding_watchdog`: 25 s in `responding` → reload the
  ESPHome config entry, only when the alarm is not ringing

Flow:

1. Local intents and custom sentences run first (`Prefer handling commands
   locally`).
2. Unmatched text goes to the Hermes conversation agent.
3. Hermes uses the MCP music tools and `web_search`; HA owns the player,
   source, alarms and timers.

Verified without Hermes running: `какой будильник` still returns
"Будильник выключен" while the gateway is stopped.  That is the required
failure mode: local alarms/timers/volume must not depend on the fallback.

If Hermes is down, local volume/timer/alarm/music commands must still work.
Do not put alarms, timers or the alarm audio endpoint behind Hermes.


## HA MCP tool errors

`StationLightOff`/`StationLightOn` are exposed by HA MCP as intent-script tools
with a generic `name`/`area`/`floor`/`domain` schema, not the internal
`light_target` slot. If Hermes calls them with only `name`, the intent script
must resolve the target itself; otherwise HA returns
`Error handling StationLightOff`.

The intent scripts now resolve `name`/`area` and map aliases such as
`лента`/`лету`, `лампа`/`ночник`, `светильник`/`монитор`. The lights also have
matching HA aliases and are exposed to Assist, so `GetLiveContext` and
`HassTurnOff` can find them.

If Hermes reports an MCP circuit-breaker pause after three rejected calls,
restart just `hermes-gateway.service` (or wait for the pause) after fixing
arguments; do not repeatedly retry the same failed call. A healthy check:

```bash
export HERMES_HOME=/home/labuser/.hermes
/home/labuser/.hermes/hermes-agent/.hermes/bin/hermes mcp test homeassistant
```

Title-generation warnings about a missing Codex OAuth token are separate from
the main chat/tool path; main `openai-codex` inference still works through the
credential pool.
