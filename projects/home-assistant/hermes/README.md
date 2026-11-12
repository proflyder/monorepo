# Hermes Agent — voice fallback

**Deployment:** gateway and voice proxy run in one Kubernetes
Pod; native user units are masked. [Current runbook](../../../docs/KUBERNETES-OPERATIONS.md).
State is `/srv/k3s/volumes/runtime-hermes` on the host (`/opt/data` in the Pod);
`~/.local/bin/hermes` on the K3s node is a kubectl wrapper into the Pod. Do not
start a native gateway next to the Pod: it would be a second production writer.

Hermes is the **fallback** brain for the Station Midi voice cascade, not the
fast path.  HA Assist keeps lights, volume, music transport and simple home
commands local. Time — alarms, timers, reminders — is routed to Hermes and
written only through the mcp-gateway `time` tools on the durable scheduler;
other unmatched or open-ended requests use the full Home Assistant MCP surface
and the Music Assistant MCP server.

## Layout

| Item | Value |
|---|---|
| Image | `ghcr.io/proflyder/hermes@sha256:…` (pins — `projects/home/kubernetes/versions.sh`) |
| Data | `/opt/data` in the Pod (host `/srv/k3s/volumes/runtime-hermes`) |
| Launcher | `~/.local/bin/hermes` on the K3s node (kubectl wrapper) |
| Config | `/opt/data/config.yaml`, rendered from [`config.fragment.yaml`](config.fragment.yaml) |
| Secrets | `Secret/hermes-environment` (ESO from OpenBao `secret/k8s/home-assistant/hermes-environment`) |
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
lives in [`HERMES.md`](HERMES.md); the TTS sanitizer is the
[`solutions/voice-proxy`](../../../solutions/voice-proxy/README.md) service.
Do not copy secrets into this directory.

## Delivery: Flux init container

`config.yaml`, `.hermes.md`, `SOUL.md`, `skills/` and both prompts live on the
PVC (`/srv/k3s/volumes/runtime-hermes` on lab-host, `/opt/data` in the Pod).
They are delivered automatically: this directory is baked into the reproducible
`hermes-config` image (`//projects/home-assistant/hermes:hermes-config_image`,
`ghcr.io/proflyder/hermes-config`) at `/opt/hermes-overlay`, and that image's
digest is the checksum of the reviewed inputs. CI publishes it, the `pins` job
writes `HERMES_CONFIG_IMAGE`, and `HelmRelease/runtime` runs the image twice:

- **initContainer `render-config`** before the gateway starts:
  `hermes render-config --home /opt/data --source-dir /opt/hermes-overlay
  --private-values /run/hermes-config/private.yaml`. `--private-values` is
  required on a live home and comes from `Secret/hermes-config-private`, an ESO
  Secret built from OpenBao `k8s/home-assistant/hermes-config` field
  `private.yaml` and mounted read-only at `/run/hermes-config`. The tracked
  fragment carries TEST-NET placeholders for the HA/MA endpoints; the private
  file is deep-merged **after** it and keeps the real addresses. The renderer
  refuses to write when a placeholder would replace a different live value and
  lists the offending keys, so a bad Secret fails the init container closed
  instead of writing a broken config.
- **sidecar `config-drift`** after start: `hermes drift-exporter` rebuilds the
  same plan every 300 s without writing and serves Prometheus metrics on
  `:9118`. Alerts `HermesConfigDrift`, `HermesConfigCheckFailing` and
  `HermesConfigDriftExporterDown` cover PVC divergence, a failed plan and a
  dead exporter.

Normal change: edit these files, open a PR, merge. Flux updates the image
digest and restarts Hermes; there is no manual render, copy or restart step.
Planned restarts also request a quiet drain (preStop) so the home channel does
not receive a "shutting down" message.

The merge is deep: unrelated keys in the generated `config.yaml` survive
(including MCP servers the fragment does not mention), the fragment wins for
the keys it declares, private values win over both, and lists are replaced.
`agent.personalities.voice` is not in the fragment: it is set from
[`voice_prompt.txt`](voice_prompt.txt), the single source of truth for the
Ricardo persona. The renderer installs `skills/` (currently `skills/report`)
into `<HERMES_HOME>/skills` and, when the private file has
`telegram_owner_chat_id`, the Telegram channel prompt and home channel from
`telegram_prompt.txt` — see «Telegram-промпт» below. A formatting-only rewrite
by Hermes is a semantic no-op: no write and no `.prev` churn. A real change
keeps one `.prev` backup (mode 600); `config.yaml.prev` is the one-step
rollback (`cp config.yaml.prev config.yaml` + restart). Comments in a
hand-edited `config.yaml` are not preserved.

## Emergency manual render

Use this only when Flux/pins are unavailable and the PVC must be fixed by hand.
The reviewed source is still this directory; do not hand-edit the PVC.

```bash
cd <repo>
# 1. see what would change (prints a redacted diff, writes nothing)
home-deploy hermes render-config \
  --home /srv/k3s/volumes/runtime-hermes \
  --source-dir projects/home-assistant/hermes \
  --private-values secrets/home/hermes-private.yaml --dry-run

# 2. render; a changed file keeps its previous version as <name>.prev (600)
sudo home-deploy hermes render-config \
  --home /srv/k3s/volumes/runtime-hermes \
  --source-dir projects/home-assistant/hermes \
  --private-values secrets/home/hermes-private.yaml

# 3. restart the gateway so it re-reads config.yaml and the context files
mise run kubectl -- -n home-assistant rollout restart deployment/hermes
mise run kubectl -- -n home-assistant rollout status deployment/hermes --timeout=180s
```

If the binary is not installed on the host, run it from a checkout:
`bazel run //platform/deploy-rs:home-deploy -- hermes render-config …`. A
second run is a no-op, and `--check` exits `1` when a render is pending
(pre-flight use). `--allow-placeholders` is only for throw-away test homes.

Run it on lab-host against the host path, not inside the Pod: the files are the
same and the Pod is not a supported manual writer. Stop the gateway first only
when you need a consistent read; a restart after the render is enough because
the gateway reads the config once at start. Run as the user that owns
`/srv/k3s/volumes/runtime-hermes`; if you had to use `sudo`, restore ownership
of the rendered files to the Pod's uid (`mise run kubectl -- -n home-assistant
exec deploy/hermes -c gateway -- id`) and check `ls -l`, otherwise the gateway
cannot open its own config. After the emergency is over, merge the intended
change so the image and PVC converge again.

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
mise run kubectl -- -n home-assistant exec deploy/hermes -c hermes -- hermes mcp test infra
mise run kubectl -- -n home-assistant exec deploy/hermes -c hermes -- hermes mcp list
```

A healthy `mcp test infra` reports the tool count; `401` means the token is
missing or stale, `connection refused` means the gateway Service or its Pod is
not ready.

After a gateway restart Hermes reconnects and re-lists tools, but its own
registry keeps the old set unless the server announces a change (upstream
`_register_discovered_tools_if_needed` returns early when tools are already
registered).  The gateway therefore sends
`notifications/tools/list_changed` once per initialized session on the
standalone GET stream, and Hermes refreshes its registry from that
notification.  When the gateway's tool set changes, restart the gateway only —
Hermes does not need a restart.  See
`solutions/mcp-gateway/README.md` for the delivery details.

## Time tools (mcp-gateway `time`)

The same `infra` MCP server carries the `time` group: `time_now`,
`alarm_set/list/cancel/delete/update/snooze/stop`, `timer_set/list/cancel/
pause/resume` and `reminder_set/list`. Hermes is the only voice writer for
alarms, timers and reminders: it takes `time_now`, computes an absolute RFC3339
instant and calls the typed tool. The tools talk to `home.v1.SchedulerService`
with the same Connect API and token as HA/Android, so `ha-bridge` still rings the
station through `script.alarm_start`; the scheduler remains the single owner.

The HA intent scripts `StationAlarm*`, `StationTimer*`, `StationReminder*`,
`StationTimeRoute*` and the `script.schedule_*` / `script.alarm_*` executor
scripts are **excluded** from the HA MCP server in `config.fragment.yaml`. The
voice prompt and `SOUL.md` say the same thing: time only through `time` tools,
always repeat the final local time, one clarifying question when the time is
unclear. `StationTimeRoute*` in HA is only a forwarder to this agent, never an
alarm parser.

`MCP_GROUPS` on the gateway must include `time`, and the gateway needs
`MCP_SCHEDULER_URL` + `MCP_SCHEDULER_TOKEN_FILE`
(`k8s/home-assistant/scheduler-auth`, key `token`). Both are part of the
Flux-rendered mcp-gateway manifest set, not the Hermes PVC.

`config.fragment.yaml`, `voice_prompt.txt`, `SOUL.md` and `HERMES.md` are
delivered by Flux through the `hermes-config` image: the `render-config`
initContainer merges them into the PVC before the gateway starts, and a merge
into `develop` (plus the image pins) is enough. Do not hand-edit the PVC; the
manual render in «Emergency manual render» is only the fallback.

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
`hermes`); the target list and the gateway tool group ship with the
mcp-gateway manifest set (`manifests/bb-targets.yaml`, applied by Flux).
Verify with `hermes mcp test
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

The initContainer installs the override on every Pod start, so merging
`telegram_prompt.txt` (or the private chat id) is enough; the rollout restarts
the gateway and `/report` appears in the bot menu on the next Telegram connect.
Verify with a `/report` message from the owner account and check the reply
length (it must exceed 180 characters for a real report).

## Voice proxy (Rust)

The TTS sanitizer, cancel guard, profile routing, late answers, dispatcher
updates and durable announcement queue are now the reusable
[`solutions/voice-proxy`](../../../solutions/voice-proxy/README.md) service
(Rust, `rules_oci`/distroless image). That README is the single contract for every
`HERMES_*` variable and for `-healthcheck` / `-status`.

Hermes-specific notes:

- HA calls the proxy at `http://192.0.2.27:8643/v1`; the proxy forwards to
  `127.0.0.1:8642`. The nftables rule in
  `projects/home/kubernetes/voice-firewall/` still limits clients to HAOS
  `.28`, the K3s node `.27` and loopback.
- The proxy sets `model_options.reasoning_effort=low` (or
  `HERMES_VOICE_REASONING_EFFORT`) only for voice requests.
- The container is distroless: diagnostics use
  `mise run kubectl -- -n home-assistant exec deploy/hermes -c voice-proxy -- /usr/local/bin/voice-proxy -status`
  (or `-healthcheck`); there is no shell or `curl`.

Deployment values and the readiness probe are described in
[`deploy/README.md`](../deploy/README.md).

## Persona — Ricardo

The station assistant is **Рикардо**, the user's personal voice assistant, not
"Hermes".  Three layers must agree:

1. `/opt/data/SOUL.md` — primary identity (`SOUL.md` in this directory is the
   reviewed source).  It explicitly says not to call itself Hermes/Гермес.
2. `agent.personalities.voice` in `/opt/data/config.yaml` — the active
   `voice` personality, rendered from `voice_prompt.txt`
   (single source; the fragment does not repeat the text).
3. HA `llama_cpp` conversation subentry — prompt from
   `voice_prompt.txt`, title/entity `Рикардо`.

Commit the reviewed files and merge; the `render-config` initContainer installs
`SOUL.md` and `agent.personalities.voice` (from `voice_prompt.txt`) into the
HERMES_HOME on the next rollout, so no hand edit of the PVC is needed. If Flux
is unavailable, use the manual render in «Emergency manual render».

The HA prompt/title are part of the live `llama_cpp` subentry and are updated
through its subentry reconfigure flow; keep `voice_prompt.txt` as the source.
Verify with a direct conversation call: «как тебя зовут?» must return
«Я Рикардо, ваш личный ассистент».  If it says Hermes, check all three layers
and restart the gateway.

## MCP tokens

Create a dedicated long-lived HA token for Hermes (owner/admin is required by
the HA MCP Server's default admin policy) and a separate long-lived Music
Assistant token. Both live in OpenBao `secret/k8s/home-assistant/hermes-environment`
(rendered into `Secret/hermes-environment`):

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

The MA MCP token is a secret: never print it or commit it. Keep the MCP
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
"что такое фотосинтез" goes to Hermes; "какой будильник" is routed by
`StationTimeRoute*` to Hermes and answered with `alarm_list`.

## Voice proxy (TTS sanitizer)

HA does not call the Hermes API directly. It calls the K3s `hermes-voice-proxy`
on the node address:

```text
HA llama_cpp -> http://192.0.2.27:8643/v1 -> Hermes 127.0.0.1:8642
```

The proxy binds `0.0.0.0:8643` in the hostNetwork Hermes Pod. An nftables rule
on the K3s VM allows only HAOS `.28`, the K3s node `.27` and loopback; other
LAN clients are dropped. Sanitising, routing, announcements and diagnostics
are documented in [`solutions/voice-proxy/README.md`](../../../solutions/voice-proxy/README.md).
It is a presentation layer, not an authentication boundary.

## MCP contract

**Full access is intentional.** All MCP servers are configured without a
`tools.include`/`tools.exclude` allowlist, with MCP resources and prompts
enabled:

- **Home Assistant** exposes all discovered HA MCP tools (57 at the time of
  deployment), including all intent scripts, lights, media players, timers,
  todos, broadcasts and live context. The time-writing intent/scripts
  (`StationAlarm*`, `StationTimer*`, `StationReminder*`, `StationTimeRoute*`,
  `script.schedule_*`, `script.alarm_*`) are excluded so Hermes uses the typed
  `time` tools instead.
- **Music Assistant** exposes all discovered MA MCP tools (86 at deployment),
  including library search, queue/playback/player control, playlists, favorites,
  volume, announcements, debug and config namespaces.
- **infra** (mcp-gateway) exposes the read-only diagnostics, the two-step
  restart pair, `infra_send_report`, the `bb.*` tools and the `time` group;
  every restart target is allow-listed by the gateway catalog, and the gateway
  itself performs only a rollout restart.

Non-HA platform toolsets remain disabled: terminal, shell, file, browser,
computer-use and similar. Only the MCP servers and `web` are enabled, on both
`api_server` and `telegram`. Time writes are restricted to the mcp-gateway
`time` tools by the HA MCP exclusions and the prompt; the rest of the MCP
surface stays intentionally broad.

This is a deliberate tradeoff: Hermes can now do anything the HA/MA MCP servers
expose. MA config/debug tools and destructive library/queue operations are
reachable. The MA MCP plugin is experimental. Keep the tokens private and both
endpoints loopback-only.

## Verify

```bash
# on the K3s node; `hermes` execs into the Pod
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
   locally`); the `StationTimeRoute*` sentences forward time phrases to Hermes.
2. Everything else unmatched goes to the Hermes conversation agent.
3. Hermes uses the MCP music tools, the MCP `time` tools and `web_search`; HA
   owns the player, source and the alarm audio executor.

Verified without Hermes running: `какой будильник` now intentionally goes to
Hermes, while the local ring commands (`стоп`, `отложи`, `хватит`,
`выключи звонок`), lights, music and volume still work. Keep the alarm audio
path (`script.alarm_start` / `alarm_stop`) independent of Hermes: the scheduler
and ha-bridge own it.


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
restart the Pod (`mise run kubectl -- -n home-assistant rollout restart deploy/hermes`) or
wait for the pause after fixing arguments; do not repeatedly retry the same
failed call. A healthy check: `hermes mcp test homeassistant`.

Title-generation warnings about a missing Codex OAuth token are separate from
the main chat/tool path; main `openai-codex` inference still works through the
credential pool.
