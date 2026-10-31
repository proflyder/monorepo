# Hermes Agent — voice fallback

**Deployment:** gateway and voice proxy run in one Kubernetes
Pod; native user units are masked. [Current runbook](../../../docs/KUBERNETES-OPERATIONS.md).
State is `/srv/k3s/volumes/hermes` on the host (`/opt/data` in the Pod);
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
| Secrets | `~/.hermes/.env` (mode `600`) |
| MCP HA | `http://192.0.2.28:8123/api/mcp` (HA MCP Server, streamable HTTP) |
| MCP MA | `http://192.0.2.28:8095/mcp/v1` (Music Assistant FastMCP plugin; HAOS add-on) |
| API | `http://127.0.0.1:8642/v1` (OpenAI-compatible, loopback only) |
| Model | ChatGPT/Codex subscription via `openai-codex` |

The generated Hermes config is large; [`config.fragment.yaml`](config.fragment.yaml)
contains the reviewed overlay for the two MCP servers, model provider, web
search and `api_server` toolset.  The HA voice prompt lives in
[`voice_prompt.txt`](voice_prompt.txt); the canonical Hermes identity lives in
[`SOUL.md`](SOUL.md); the TTS sanitizer lives in
[`voice_proxy.py`](voice_proxy.py).  Do not copy secrets into this directory.

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
`docker.io/ricardo/voice-proxy:v6` in `projects/home/kubernetes/versions.sh`.

## Persona — Ricardo

The station assistant is **Рикардо**, the user's personal voice assistant, not
"Hermes".  Three layers must agree:

1. `~/.hermes/SOUL.md` — primary identity (`SOUL.md` in this directory is the
   reviewed source).  It explicitly says not to call itself Hermes/Гермес.
2. `agent.personalities.voice` in `~/.hermes/config.yaml` — the active
   `voice` personality; the overlay in `config.fragment.yaml` contains the
   reviewed text.
3. HA `llama_cpp` conversation subentry — prompt from
   [`voice_prompt.txt`](voice_prompt.txt), title/entity `Рикардо`.

Apply on lab-host after editing the repo files:

```bash
cp projects/home-assistant/hermes/SOUL.md ~/.hermes/SOUL.md
~/.local/bin/hermes config set --force agent.personalities.voice \
  "$(cat projects/home-assistant/hermes/voice_prompt.txt)"
~/.local/bin/hermes gateway restart
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

The fallback is not music-only.  `platform_toolsets.api_server` contains
`mcp-homeassistant`, `mcp-music-assistant` and `web`.  General knowledge is
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

**Full access is intentional.** Both MCP servers are configured without a
`tools.include`/`tools.exclude` allowlist, with MCP resources and prompts
enabled:

- **Home Assistant** exposes all discovered HA MCP tools (57 at the time of
  deployment), including all intent scripts, lights, media players, timers,
  todos, broadcasts and live context.
- **Music Assistant** exposes all discovered MA MCP tools (86 at deployment),
  including library search, queue/playback/player control, playlists, favorites,
  volume, announcements, debug and config namespaces.

Non-HA platform toolsets remain disabled: terminal, shell, file, browser,
computer-use and similar. Only the two MCP servers and `web` are enabled for
`api_server`. The HA voice prompt still tells Hermes that local intents own
timers/alarms; the MCP surface is broader than the voice policy.

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
