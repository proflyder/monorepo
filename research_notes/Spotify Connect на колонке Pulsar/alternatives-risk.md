# Spotify Connect alternatives for Yandex Station Midi / Pulsar

**Date:** 2026-10-26  
**Scope:** making the Station Midi (Pulsar v0.4.1) usable as a Spotify target, without changing the station or credentials.  
**Method:** primary docs from Music Assistant, Spotify/Soloist, go-librespot, plus the local lab docs and open MA issue reports.  
**Confidence tags:** `[CONFIRMED]` = stated by a primary source or directly observed in the local project; `[INFERRED]` = reasoned from confirmed facts, not directly tested; `[GAP]` = no reliable source or no local test.

> **Bottom line:** the only option that meets the “station is a Spotify Connect target” requirement with acceptable risk is **A — Music Assistant’s Spotify Connect plugin on the nettop, using Spotify Soloist, targeting the existing Pulsar/Sendspin player**. It keeps Spotify WAN traffic and the audio server on the nettop, requires no station changes, and preserves the station’s egress quarantine. Its two gating risks are (1) the open MA 2.10.8 Spotify regression and (2) Connect→Sendspin sync/buffering on Pulsar. A native receiver on the station (B) is the worst option: the station has no public WAN egress and Pulsar’s audio path is not an input. A standalone receiver on the nettop with a custom bridge to Pulsar (C) is technically possible but loses metadata/voice integration and adds a second moving part. Keeping only the MA music provider (D) is the safest fallback but does not make the station appear in the Spotify app. The practical coexistence pattern is **A + D on one account with explicit source arbitration**, or **A + D on two accounts** if simultaneous phone and voice use matters (E).

---

## Decision matrix

### 1. Baseline: what is already true

| Fact | Status | Source |
|---|---|---|
| Station Midi `cucumber`, firmware `.160`; kernel aarch64, **userspace armhf**; rootfs `ro`; Pulsar v0.4.1 installed; `maind`/native Alice stopped | `[CONFIRMED]` | [local: STATE.md](../../docs/STATE.md) |
| Pulsar exposes ESPHome API `:6053` and Sendspin `:8928`; MA sees “Яндекс Станция Миди” as a Sendspin player | `[CONFIRMED]` | [local: STATE.md](../../docs/STATE.md) |
| Pulsar audio path: firmware GStreamer for HTTP streams + Sendspin → **ALSA dmix**; volume via ALSA Master | `[CONFIRMED]` | local: Pulsar README (`projects/station/upstream/pulsar/README.md`, только локально) |
| Pulsar local voice commands (“стоп/пауза/продолжи/дальше”) run on-device; “дальше/назад” explicitly target the **Sendspin** player | `[CONFIRMED]` | local: Pulsar README (`projects/station/upstream/pulsar/README.md`, только локально) |
| Station egress guard allows only private LAN, loopback, DHCP, multicast, Google NTP; all other WAN is dropped | `[CONFIRMED]` | [local: STATE.md](../../docs/STATE.md), [local: ARCHITECTURE.md](../../docs/ARCHITECTURE.md), [local: SECURITY.md](../../docs/SECURITY.md) |
| Nettop `lab-host`: x86_64 Ubuntu 26.04, HA 2026.10.4, MA 2.10.8, MA container on **host network**; Soloist 1.3.10.13 installed; Spotify playback already tested in the MA web player | `[CONFIRMED]` | [local: STATE.md](../../docs/STATE.md), [local: stack/docker-compose.yml](../../projects/home-assistant/stack/docker-compose.yml) |
| Current Spotify integration is the MA **music provider** (library/voice), not a Connect target; Spotify Web API returned 429 with `Retry-After` ~18.5 h; Liked Songs not synced | `[CONFIRMED]` local observation | [local: STATE.md](../../docs/STATE.md), [local: PENDING.md](../../docs/PENDING.md) |
| MA has two separate things: a **Spotify music provider** and a **Spotify Connect plugin**. The plugin makes MA players appear as Connect devices; the music provider supplies library/search/voice playback | `[CONFIRMED]` | [MA Spotify provider](https://www.music-assistant.io/music-providers/spotify), [MA Spotify Connect plugin](https://www.music-assistant.io/plugins/spotify-connect) |
| MA Connect plugin supports any MA player/group, one device per selected player, optional Spotify volume sync, and “stopping playback in MA releases the device; moving playback to another device in the Spotify app stops the MA player” | `[CONFIRMED]` | [MA Spotify Connect plugin](https://www.music-assistant.io/plugins/spotify-connect) |
| Spotify Soloist is Spotify’s official headless Connect client; Linux ARMv7/ARM64/x86_64; **requires PipeWire or PulseAudio**; builds expire after **90 days**; API key needs Premium, but Free accounts can connect after setup; lossless up to 24-bit/44.1 kHz | `[CONFIRMED]` | [Soloist announcement](https://developer.spotify.com/blog/2026-08-13-introducing-spotify-soloist), [Soloist GitHub](https://github.com/spotify/soloist), [Soloist getting started](https://developer.spotify.com/documentation/soloist/tutorials/getting-started) |
| go-librespot is a Go Connect client with ALSA/pipe/PulseAudio backends, REST/WebSocket, MPRIS; Premium required | `[CONFIRMED]` | [go-librespot](https://github.com/devgianlu/go-librespot) |
| MA says librespot is “intended for Premium accounts created before December 2024” and may stop working; newer accounts generally need Soloist | `[CONFIRMED]` MA statement | [MA Spotify provider](https://www.music-assistant.io/music-providers/spotify) |

### 2. The options

| ID | Option | Where the Spotify receiver runs | How audio reaches Pulsar |
|---|---|---|---|
| **A** | MA **Spotify Connect plugin** targeting an existing MA player (Pulsar/Sendspin) | Nettop, inside MA (Soloist or go-librespot engine) | MA’s existing Sendspin stream to Pulsar |
| **B** | Native Connect receiver on the station itself (Soloist / go-librespot / librespot / spotiya) | Station, armhf userspace | Direct ALSA and/or a new Pulsar integration |
| **C** | Standalone Connect receiver on the nettop, audio forwarded to Pulsar | Nettop, outside MA | Custom bridge: Sendspin source, HTTP/Icecast, or another stream |
| **D** | Keep only the current MA Spotify **music provider** + voice | Nettop, inside MA | MA Sendspin stream to Pulsar |
| **E** | Hybrid/coexistence: Connect plugin for app playback + music provider for voice, with source/account policy | Nettop, inside MA | MA Sendspin stream to Pulsar |

### 3. Functional matrix

Ratings are comparative, not lab measurements. `5` = best fit, `1` = worst.

| Criterion | A: MA Connect plugin | B: receiver on station | C: receiver on nettop + bridge | D: music provider only | E: hybrid |
|---|---|---|---|---|---|
| Station appears in Spotify app | **Yes**, home LAN; one device per selected MA player | **Only if** receiver runs and can reach Spotify; network gate below | **Yes**, home LAN, if the receiver advertises correctly | **No** | **Yes** (via A) |
| Works from other networks / cellular | **No reliable discovery**; MA docs require phone on same network as MA; Spotify’s “different Wi-Fi” feature is explicitly unsupported | Same LAN discovery; off-network visibility unverified | Same LAN discovery; off-network unverified | N/A | Same as A |
| Phone → speaker | **Yes**; select the device | **Yes** if running | **Yes** at receiver; bridge adds latency | **No** via Spotify app; only MA app/HA | **Yes** |
| Speaker → phone | **Yes**; selecting phone stops the MA player | **Yes** native Connect | **Yes** at receiver; Pulsar/MA state may be stale | **No** | **Yes** |
| Metadata in Spotify app | **Yes**, native Connect metadata | **Yes** | **Yes** | **No** Spotify app; metadata in MA/HA | **Yes** |
| Metadata in MA/HA | **Yes** as external source; HA favorite works if artist/title present | **No** unless custom bridge | **No** unless custom bridge | **Yes**, native | **Yes** for Connect source; native for MA queue |
| Position sync | **Yes**, but app progress can run ahead of buffered audio; Sendspin low-latency risk | **Yes** at receiver | **Yes** at receiver; bridge may add drift | **Yes** in MA/HA | **Yes** with A’s caveats |
| Volume sync | Default **player volume only** (audio untouched); optional Spotify slider ↔ player volume | Receiver can sync ALSA mixer, but Pulsar also controls ALSA Master → conflict | Receiver can sync its own mixer; Pulsar volume separate | MA/HA volume; Spotify app not involved | Same as A/D |
| Voice/Alice control | **Partial**: HA `media_pause/play/stop` on the MA player likely works; voice `play_media` takes over and ends the Connect source; local Pulsar “next” may not map to Spotify next | **No** HA integration; `maind` stopped, so spotiya’s Alice IPC is unavailable; Pulsar TTS/voice will overlap | **No** unless custom bridge | **Yes**, native MA/HA voice path (once 429 clears) | **Partial**, same as A for Connect; D for MA queue |
| Spotify app can take over | **Yes**, by design; selecting another device stops the MA player | **Yes** | **Yes** | **Partial**: app sees `Music Assistant Playback` and can pause/skip, but cannot select the station as a target | **Yes** |
| Quality ceiling | Soloist lossless up to 24/44.1; MA sends PCM; Pulsar Sendspin path may resample/limit to 16-bit | Receiver can be lossless; but ALSA mixing/Pulsar overlap degrades experience | Receiver can be lossless; bridge may transcode | MA queue quality; no Connect | A/D depending source |
| Latency | First track/seek slower with Soloist; player buffering can make app ahead of audio | Low at receiver; but no ducking/coordination | Receiver low; bridge adds buffering | MA stream latency | A/D |

### 4. Risk/operations matrix

| Criterion | A: MA Connect plugin | B: receiver on station | C: receiver on nettop + bridge | D: music provider only | E: hybrid |
|---|---|---|---|---|---|
| Hardware feasibility | **High**: nettop x86_64, host network, Soloist already installed | **Low**: armhf userspace, Buildroot/ro rootfs, no evidence of PulseAudio/PipeWire; glibc/ABI unknown; Soloist needs an audio server | **Medium-high**: nettop is fine; the weak link is the custom audio bridge to Pulsar | **High**: already working | **High** |
| Network feasibility | **High**: MA on nettop has WAN; mDNS on flat LAN | **Very low without changes**: station egress guard drops public WAN; Spotify auth/CDN unreachable unless WAN is opened or proxied via LAN | **High**: WAN stays on nettop; audio goes over LAN | **High** | **High** |
| Setup complexity | **Low–medium**: add plugin, choose engine, API key, pair, select players (≈15–30 min) | **Very high**: cross-build/install, audio server, init script, WAN exception, ALSA, voice bridge (days) | **High**: receiver + bridge + HA/Pulsar integration (days) | **None**: already done | **Medium**: A + provider policy + tests (hours) |
| Maintainability | **Medium**: MA releases and plugin changes; Soloist 90-day builds auto-downloaded by MA; pin versions | **Low**: manual 90-day binary refresh, no package manager, rootfs/OTA risk | **Low–medium**: two services, manual updates | **High**: existing stack | **Medium** |
| Observability | **Good**: MA UI/player state, `music_assistant.spotify_connect` logs, HA entity, Pulsar ESPHome logs | **Poor**: station process/ALSA logs only; no MA/HA state | **Medium**: container logs + MA/Pulsar logs, split across services | **Good**: MA/HA logs | **Good but split** |
| Rollback | **Easy**: disable plugin/unselect player; no station changes; revert MA image if needed | **Hard**: remove service/init script, restore rootfs, revert WAN rules | **Medium**: stop container/remove bridge | **Trivial** | **Easy** (disable plugin; keep provider) |
| Spotify Premium/Free | Soloist setup needs Premium; afterwards Free can connect; MA music provider needs Premium | Soloist setup needs Premium; Free can connect afterwards; go-librespot needs Premium | Same as B | **Premium required**; Free does not work | Same as A/D |
| 429/Web API dependence | Connect playback does **not** need Web API; library/search/voice still does; 429 blocks setup/library, not Connect playback (inference) | Connect protocol does not use Web API; no 429 for playback | Same as B | **Fully dependent** on Web API for library/search; 429 blocks voice favorites | Mixed; Connect path bypasses, voice path does not |
| Legal/ToS risk | **Medium**: MA downloads/runs Soloist on your behalf; MA itself warns Spotify’s terms “do not clearly allow” this | **Medium–high**: native Soloist is closer to intended use, but binary redistribution prohibited; opening station WAN weakens quarantine; unsupported platform | **Medium**: same Soloist terms; if go-librespot, reverse-engineered client risk | **Low–medium**: official Web API, developer terms | **Medium** |
| Implementation risk | **Medium** | **Very high** | **High** | **Low** (but does not meet goal) | **Medium–high** |

### 5. Weighted decision matrix

Weights reflect the stated objective: app target and control/sync are hard requirements; voice coexistence is equally important; conflict safety and feasibility follow.

| Criterion | Weight | A | B | C | D | E |
|---|---:|---:|---:|---:|---:|---:|
| Spotify app presence + network scope | 20% | 5 | 2 | 5 | 1 | 5 |
| Phone↔speaker switching + metadata/position/volume | 20% | 4 | 3 | 4 | 1 | 4 |
| Voice/Alice + HA coexistence | 20% | 3 | 1 | 2 | 5 | 3 |
| Conflict/account/queue safety (inverse risk) | 15% | 3 | 2 | 2 | 5 | 2 |
| Hardware/network feasibility | 10% | 5 | 1 | 4 | 5 | 5 |
| Setup/maintenance/observability/rollback | 10% | 4 | 1 | 2 | 5 | 3 |
| Legal/ToS + 429 resilience | 5% | 3 | 2 | 3 | 3 | 3 |
| **Weighted total** | **100%** | **3.90** | **1.80** | **3.25** | **3.30** | **3.65** |

**Interpretation:**

- **A wins narrowly** because it satisfies the hard app-target requirement without station changes. Its score is held down by voice/source conflicts and the 2.10.8 regression.
- **E is close** and becomes preferable if you want an explicit two-mode policy (app vs voice) or separate accounts.
- **D is the best “quality of voice” option** but fails the app-target requirement. If the user’s real priority is reliable Alice control and app presence is optional, D should win.
- **B is not viable** in the current architecture: the station’s egress guard and Pulsar’s audio path are fundamental blockers, not just installation details.
- **C is dominated by A** for this stack: it adds a bridge and loses MA/HA integration for no gain, unless A’s plugin is unusable.

**Sensitivity:** if voice is weighted 30% and app presence 10%, D overtakes A. If the app target is a hard gate, B/C/D are eliminated regardless of score. If the 2.10.8 regression is treated as blocking for all Spotify playback, A/E drop by ~0.15–0.30 and D remains the only stable option until MA is pinned back. If E uses **two separate Spotify accounts** (E2), its conflict score rises from 2 to 4 and its total becomes ≈3.95 — effectively tied with A. That is the strongest argument for E2 when simultaneous phone and voice playback matters.

**Decision rule:**

1. If the station **must** appear in the Spotify app → **A** by default; **E2** if phone and voice must work simultaneously.
2. If **Alice/voice reliability** is the priority and app presence is optional → **D**.
3. If **A fails** the G1/G2 gates → **C** only as a lab fallback; **D** as the stable production fallback.
4. **B** is eliminated by the station’s egress guard and audio architecture, not by preference.

### 6. Per-option assessment

#### A. Music Assistant Spotify Connect plugin → existing MA player (Pulsar/Sendspin)

**What it is.** Add the MA **Spotify Connect** plugin, choose the **Spotify Soloist** engine, complete the API-key/pairing flow, then select the Pulsar Sendspin player under “Connected players.” MA advertises that player as a separate Spotify Connect device, named after the player (default `Player name | Music Assistant`). Audio is decoded by Soloist inside MA and sent to Pulsar over the existing Sendspin path. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)

**Confirmed behavior.**

- Any MA player, including groups, can be exposed; each selected player is its own device. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)
- Stopping playback in MA releases the device in the Spotify app; moving playback to another Spotify device stops the MA player. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)
- Volume can be “Player volume only” (default; audio untouched) or synced with the Spotify app slider. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)
- The phone/tablet/computer must be on the same network as the MA server; MA explicitly says Spotify’s “different Wi-Fi networks” feature is not supported. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)
- Soloist cannot be used on hosts with Docker bridge networks; the local MA is on host network, so it passes this gate. [MA Spotify provider](https://www.music-assistant.io/music-providers/spotify), [local: stack/docker-compose.yml](../../projects/home-assistant/stack/docker-compose.yml)
- MA downloads Soloist from Spotify’s servers on your behalf after consent and updates it automatically; MA warns Spotify’s terms do not clearly allow this. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)

**Assessment.**

- **App presence:** strong. The station should appear as a normal Connect device on the home LAN. `[CONFIRMED]` for the plugin’s design; `[INFERRED]` for Pulsar specifically because no Pulsar-specific test was found.
- **Switching/sync:** phone→speaker and speaker→phone should work because this is native Connect. Metadata and position are Spotify-driven; MA warns the app’s progress can run slightly ahead of audio due to player buffering. Spotify notes that a pause longer than 10 minutes may require reconnecting. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect), [Spotify Connect support](https://support.spotify.com/us/article/spotify-connect/)
- **Metadata in MA/HA:** MA exposes the external source to HA; the HA Music Assistant integration says the favorite button works with remote content such as Spotify Connect as long as the external source provides artist/title (and optionally album). [HA Music Assistant integration](https://www.home-assistant.io/integrations/music_assistant)
- **Voice/Alice:** the current HA scripts call `media_player.media_pause/play/stop` on the MA Pulsar entity, and `music_assistant.play_media` for Spotify favorites. If the Connect plugin is the active source, pause/stop likely reaches the external source, but voice `play_media` will start an MA queue and replace the Connect session. Local Pulsar “next” targets Sendspin, not necessarily the Spotify session. `[INFERRED]` — needs the G3 test.
- **Conflicts:** one Spotify account/stream; MA queue vs external source; app can always take over; MA “powered on” state can cause MA to take control even when playback was started elsewhere. [MA troubleshooting](https://www.music-assistant.io/faq/troubleshooting)
- **Hardware:** best of the five. The nettop is x86_64, has WAN, runs MA on host network, and already has Soloist 1.3.10.13. [local: STATE.md](../../docs/STATE.md)
- **Setup:** add the plugin, choose Soloist, accept terms/create the API key, pair `Music Assistant Pairing` from the Spotify app, then select Pulsar. Whether the existing music-provider Soloist API key is reused by the plugin is not documented; budget for a second key/pairing. `[GAP]`
- **Maintenance:** MA release coupling is the main cost. Soloist’s 90-day build expiry is handled by MA’s auto-update, but if that fails, playback exits with code 10. [Soloist downloads/updates](https://developer.spotify.com/documentation/soloist/reference/downloads-and-updates)
- **ToS:** medium. The official Soloist client is intended for headless DIY, but MA’s download/run-on-your-behalf model is the gray area, not Soloist itself. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect), [Soloist GitHub](https://github.com/spotify/soloist)
- **429:** Connect playback should bypass the Web API rate limit; the local 429 is on the music provider/library path. `[INFERRED]` from the architecture; verify with G5.

**Verdict:** **primary candidate**, gated by MA version and Pulsar Sendspin stability. Do not deploy on MA 2.10.8 until the open regression is resolved or MA is pinned to 2.10.7.

#### B. Native Spotify Connect receiver on the station (Soloist / go-librespot / librespot / spotiya)

**What it is.** Run a Connect receiver directly on the Midi, writing to ALSA (or to a new Pulsar input), so the station itself is the Spotify device.

**Confirmed constraints.**

- Station userspace is **armhf** (kernel aarch64); rootfs is `ro`; Pulsar is installed under `/data` with an init script. [local: STATE.md](../../docs/STATE.md), local: Pulsar README (`projects/station/upstream/pulsar/README.md`, только локально)
- Station egress guard drops all public WAN except private LAN/DHCP/multicast/NTP; Spotify auth, dealer, and CDN endpoints are not reachable without changing the guard or routing via a LAN proxy. [local: STATE.md](../../docs/STATE.md), [local: ARCHITECTURE.md](../../docs/ARCHITECTURE.md)
- Pulsar’s documented audio path is firmware GStreamer for HTTP streams and Sendspin → ALSA dmix; no external audio input (line-in, named pipe, PulseAudio sink) is documented. local: Pulsar README (`projects/station/upstream/pulsar/README.md`, только локально)
- Soloist requires PipeWire or PulseAudio on Linux; no evidence in the project docs that the station’s Buildroot image ships either. [Soloist GitHub](https://github.com/spotify/soloist), [Soloist getting started](https://developer.spotify.com/documentation/soloist/tutorials/getting-started)
- Soloist ARMv7 build exists, but the ARMv8 build is tested on Debian Trixie/glibc 2.41; the arm32 target’s glibc baseline is not stated. The station’s local docs disagree on userspace: STATE says armhf, RESOURCES says aarch64 and rejects spotiya’s armv7 binary. Pulsar itself ships an armhf binary and runs, which supports the armhf reading. [Soloist getting started](https://developer.spotify.com/documentation/soloist/tutorials/getting-started), [local: STATE.md](../../docs/STATE.md), [local: RESOURCES.md](../../docs/RESOURCES.md)
- spotiya (librespot-based) is tested only on Station Mini 2, requires the native Alice IPC at `127.0.0.1:9875`, and does its own ducking by intercepting Alice’s recognized text. On this station `maind` is stopped, so that IPC is unavailable; Pulsar owns the voice path. spotiya README (`resources/spotiya/README.md`, только локально), [local: STATE.md](../../docs/STATE.md)
- go-librespot supports ALSA and a pipe backend, and is a single Go binary, but MA says librespot is intended for accounts created before December 2024; newer accounts generally need Soloist. [go-librespot](https://github.com/devgianlu/go-librespot), [MA Spotify provider](https://www.music-assistant.io/music-providers/spotify)

**Assessment.**

- **App presence:** only if the receiver runs and can reach Spotify. The egress guard is a hard blocker; opening it contradicts the station’s quarantine design. `[CONFIRMED]` network policy, `[INFERRED]` playback failure.
- **Switching/sync:** a native receiver would give good Connect behavior at the Spotify layer, but HA/MA would not see metadata or position; there is no bridge to the MA player entity.
- **Voice/Alice:** poor. Pulsar’s voice pipeline sends commands to HA/MA, not to a local receiver. A local receiver writing ALSA would not be ducked by Pulsar during TTS; TTS and Spotify would overlap. spotiya’s ducking depends on the native Alice IPC, which is stopped.
- **Conflicts:** ALSA dmix allows mixing, but Pulsar and the receiver would both control ALSA Master/volume, and neither knows about the other’s playback state. One Spotify account means the receiver and MA/phone will fight over the single stream.
- **Hardware:** low. Even if the binary runs, it needs an audio server (Soloist) or a carefully built ALSA client (go-librespot), plus a 90-day Soloist refresh and a rootfs/OTA-safe install.
- **Setup/maintenance:** very high. Cross-build/ABI validation, audio server installation on a read-only Buildroot image, init/supervision, WAN exception, and a voice bridge are all separate projects.
- **ToS/security:** medium–high. Native Soloist is closer to intended use than MA’s download-on-your-behalf model, but redistribution is prohibited; opening station WAN weakens the quarantine and should be treated as a security regression. [Soloist getting started](https://developer.spotify.com/documentation/soloist/tutorials/getting-started), [local: SECURITY.md](../../docs/SECURITY.md)

**Verdict:** **reject** for the current station. Revisit only as a spare-hardware lab experiment with an explicit, reversible WAN exception and no expectation of Alice/HA integration.

#### C. Spotify Connect receiver on the nettop, audio forwarded to Pulsar

**What it is.** Run Soloist or go-librespot on the nettop outside MA, then bridge its PCM output to Pulsar. Possible bridges: a custom Sendspin source client, an HTTP/Icecast stream that Pulsar’s GStreamer player can open, or a new MA “live input” path.

**Confirmed constraints.**

- The nettop is the right place for WAN and CPU: x86_64, Ubuntu 26.04, Docker, host network. [local: STATE.md](../../docs/STATE.md)
- Pulsar can play HTTP streams through firmware GStreamer; it is also a Sendspin client. local: Pulsar README (`projects/station/upstream/pulsar/README.md`, только локально)
- MA has a Sendspin Source plugin for audio coming **into** MA from Sendspin devices, and a Sendspin player provider for audio going **out** to devices; neither is a generic “take PCM from a local daemon” bridge. [MA Sendspin Source](https://www.music-assistant.io/plugins/sendspin-source), [MA Sendspin player](https://www.music-assistant.io/player-support/sendspin)
- Soloist exposes a local WebSocket API with playback control, events, and queue metadata; go-librespot exposes REST/WebSocket and MPRIS. A custom bridge could read metadata from these. [Soloist GitHub](https://github.com/spotify/soloist), [go-librespot](https://github.com/devgianlu/go-librespot)

**Assessment.**

- **App presence:** yes, if the receiver advertises correctly on the LAN. The receiver is a normal Connect device.
- **Switching/sync:** phone↔speaker works at the receiver. The bridge adds latency and can break position sync; Pulsar/MA may keep a stale state or keep buffered audio after the phone takes over.
- **Metadata/voice:** metadata stays in the receiver’s API, not in MA/HA, unless you build a bridge. Voice control is absent; Alice would still talk to HA/MA, which does not control the standalone receiver. This is the main reason C is dominated by A.
- **Conflicts:** one Spotify account/stream; the receiver and MA music provider will compete. Pulsar can only play one HTTP stream/Sendspin source at a time, so switching between C and D requires an explicit stop/start. If the bridge is an HTTP stream, Pulsar’s GStreamer player may not expose next/previous to the receiver.
- **Hardware:** nettop is capable; the bridge is the risk. A custom Sendspin source is nontrivial; an HTTP/Icecast bridge is simpler but loses metadata and tight control.
- **Setup/maintenance:** high: receiver + bridge + HA scripts + monitoring. Two services to update, two sets of logs.
- **ToS:** medium. Soloist native is intended for headless DIY; go-librespot is reverse-engineered and carries more breakage/ToS risk. [Soloist GitHub](https://github.com/spotify/soloist), [go-librespot](https://github.com/devgianlu/go-librespot)

**Verdict:** **lab fallback only**, if A’s MA plugin proves unusable and you are willing to give up MA/HA metadata and voice integration. For this stack it adds complexity without a clear benefit.

#### D. Keep only the MA Spotify music provider + voice (no Connect device)

**What it is.** Current state: MA Spotify music provider (Soloist 1.3.10.13) + Pulsar Sendspin + HA voice. No Spotify Connect plugin.

**Confirmed behavior.**

- The station does **not** appear in the Spotify app; playback is started from MA/HA. [MA Spotify provider](https://www.music-assistant.io/music-providers/spotify), [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)
- Voice path is native: Pulsar wake word → HA Assist → `media_player` / `music_assistant.play_media` on the MA Pulsar entity. [local: ha-voice README](../../projects/home-assistant/ha-voice/README.md), [local: intent_scripts.yaml](../../projects/home-assistant/ha-voice/intent_scripts.yaml)
- Spotify Premium is required; Free accounts do not work with the music provider. [MA Spotify provider](https://www.music-assistant.io/music-providers/spotify)
- The music provider depends on the Spotify Web API; MA shares one allowance and offers an optional personal Client ID to reduce rate limiting. [MA Spotify provider](https://www.music-assistant.io/music-providers/spotify)
- The local 429 (`Retry-After` ~18.5 h) blocks Liked Songs sync and `play_media`; the voice fallback text is already implemented. [local: STATE.md](../../docs/STATE.md), [local: intent_scripts.yaml](../../projects/home-assistant/ha-voice/intent_scripts.yaml)

**Assessment.**

- **App presence:** none. This is the disqualifier if “Spotify target” means the Spotify app’s device picker.
- **Switching/sync:** no phone↔speaker Spotify Connect. Metadata/position live in MA/HA; the Spotify app cannot transfer to the station.
- **Voice/Alice:** best of all options. Alice/HA controls the MA queue directly; no Connect source arbitration. `[CONFIRMED]` local scripts.
- **Conflicts:** minimal. Only the MA queue and one Spotify account. A phone playing Spotify on the same account may still take the stream, but the station is not a Connect target, so there is no device-level fight.
- **Hardware:** already working; no new risk.
- **Setup/maintenance:** zero new setup; MA release risk only.
- **429:** the main operational weakness. Voice/library depends on Web API; Connect playback would bypass it. This is the strongest argument for adding A.
- **ToS:** lowest: official Spotify Web API, personal use; still subject to developer terms and rate limits.

**Verdict:** **baseline and voice fallback**. Keep it even if A is added. It does not meet the app-target requirement by itself.

#### E. Hybrid / coexistence patterns

**E1. Connect plugin + music provider, one account, one active source (recommended default if app target is required).**  
Add A, keep D. Treat the Spotify app as “guest mode”: when the app selects the station, Connect plays; when Alice asks for a track, `play_media` replaces the Connect source; when the app selects the phone, the MA player stops. This is predictable but not simultaneous. MA’s own troubleshooting says if the MA player is “powered on” in the MA UI, MA may take control even when playback started elsewhere; “power off” the MA player when using the Spotify app. [MA troubleshooting](https://www.music-assistant.io/faq/troubleshooting)  
**Risk:** one Spotify account/stream; two Soloist sessions (music provider + Connect plugin) on the same account; 2.10.8 regression. `[INFERRED]` — needs G2/G3/G4 tests.

**E2. Separate Spotify accounts for Connect and voice (cleanest simultaneous coexistence).**  
Use account A for the Connect plugin (app/phone) and account B for the MA music provider/voice. MA docs explicitly say a second Spotify account added as its own source has its own session; Soloist’s “one at a time” limit is per account. [MA Spotify provider](https://www.music-assistant.io/music-providers/spotify)  
**Benefits:** avoids the single-stream fight; phone and voice can play different content simultaneously.  
**Costs:** two Premium accounts (Family/Duo), split libraries/likes, two device names, more setup. Voice likes/favorites apply to account B; the app’s library is account A. Soloist API keys are tied to the account that generated them, and MA checks that the pairing account matches the connected account, so budget one key/pairing per account. [Soloist getting started](https://developer.spotify.com/documentation/soloist/tutorials/getting-started), [MA Spotify provider](https://www.music-assistant.io/music-providers/spotify)  
**Risk:** MA’s multi-account handling has a reported bug where playback started by one provider can appear in another account’s Spotify app as “Music Assistant” (#6402). [MA issue #6402](https://github.com/music-assistant/support/issues/6402)

**E3. Connect-only + voice via the Spotify app (no MA music provider).**  
Use A only; disable the Spotify music provider to avoid 429/library issues. The app controls playback; Alice can at best pause/resume/next the MA external source. Voice search for artists/tracks is lost. This is acceptable only if voice music search is not required.

**E4. Source arbitration in HA.**  
Add an HA script/automation: when a voice music intent starts, call `media_player.media_stop` on the MA Pulsar entity to release the Connect device, then `play_media`; when the app starts, the external source takes over. Add a “Spotify mode” toggle that powers the MA player off/on to prevent MA from stealing control. This is a policy layer, not a technical fix; it makes behavior predictable but adds latency and edge cases. `[INFERRED]` — test with G3/G4.

**E5. Device naming discipline.**  
Use `Player name only` for Connect devices to avoid “Яндекс Станция Миди | Music Assistant” clutter; do not expose both a group and its individual members if the same account/stream would fight. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)

**Verdict:** **E1 is the pragmatic default; E2 is the cleanest if simultaneous use matters; E3 is a niche fallback; E4 is an optional UX layer.**

---

## Risk analysis

### 7. Cross-cutting conflict model

**7.1 One account, one active Spotify stream.**  
Spotify’s consumer guidance is that one account streams on one device at a time; starting elsewhere takes over. [Spotify Community KB](https://community.spotify.com/forum/en/kb/articles/834-premium-family-duo-only-one-person-can-listen-at-a-time), [Spotify Community discussion](https://community.spotify.com/forum/en/discussion/5311229/how-to-play-multiple-streams-on-different-devices)  
MA’s Soloist docs corroborate: “Each Spotify Soloist account plays one thing at a time. Starting Spotify on a second player asks you to stop the first one.” [MA Spotify provider](https://www.music-assistant.io/music-providers/spotify)  
There is a conflicting MA statement that librespot supports “playing on two players at once”; that may mean MA multiroom fan-out from one source, not two independent Spotify streams. Treat the one-stream rule as the planning assumption and test it. `[CONFIRMED]` for Soloist; `[CONFLICT]` for librespot.

**7.2 MA queue vs Connect external source.**  
The Connect plugin uses MA’s “external audio source” mechanism. The player can have either the MA queue or the Connect source active, not both. MA docs: “Stopping playback in Music Assistant releases the device in the Spotify app; moving playback to another device in the Spotify app stops the MA player.” [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)  
Implication: every voice `play_media` is a takeover, not an overlay. Every app selection is a takeover in the other direction. There is no mixing of MA queue and Spotify app playback on one player.

There is an extra naming trap: while the **music provider** (Soloist) is playing, MA shows up in the Spotify app as a device called `Music Assistant Playback`; pausing or skipping there interferes with playback, so MA’s own controls should be used. [MA Spotify provider](https://www.music-assistant.io/music-providers/spotify) The **Connect plugin** is different: it exposes the selected MA player as its own device (default `Player name | Music Assistant`). [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)

**7.3 Voice path and local Pulsar commands.**  
Current HA scripts use `media_player.media_pause/play/stop` on `media_player.iandeks_stantsiia_midi` and `music_assistant.play_media` for favorites. [local: intent_scripts.yaml](../../projects/home-assistant/ha-voice/intent_scripts.yaml)  
Pulsar’s own local commands (“стоп/пауза/продолжи/дальше”) are handled on-device; “дальше/назад” explicitly target Sendspin. local: Pulsar README (`projects/station/upstream/pulsar/README.md`, только локально)  
Therefore:
- “Пауза/продолжи/стоп” via HA may pause/stop the Connect external source, but this is `[INFERRED]` — the MA docs only explicitly describe stop/release, not pause propagation.
- “Дальше” via Pulsar’s local path may send a Sendspin command that does not map to Spotify’s next-track; the Spotify app and MA could desync. `[INFERRED]`, needs test.
- “Включи <artist>” via HA starts an MA queue and ends the Connect source. This is the biggest UX surprise.
- MA announcements/TTS are designed to pause playback during announcements; for an external Connect source this may pause/stop the Spotify stream. Not tested on Pulsar. `[INFERRED]`; see the HA integration’s announcement behavior. [HA Music Assistant integration](https://www.home-assistant.io/integrations/music_assistant)

**7.4 429/Web API split.**  
The music provider uses the Spotify Web API for library/search; MA shares one allowance and offers a personal Client ID to reduce rate limiting. [MA Spotify provider](https://www.music-assistant.io/music-providers/spotify)  
Connect playback uses the Connect protocol/Soloist, not the Web API. `[INFERRED]` from the architecture; the local 429 is on the music provider. So A can restore app playback while voice favorites remain broken. This is a genuine benefit of A, but it does not fix D’s 429.

**7.5 Network/mDNS.**  
MA’s plugin requires the phone/tablet/computer on the same network as the MA server; Spotify’s “different Wi-Fi networks” feature is explicitly not supported. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)  
Spotify’s own article says first pairing requires same Wi-Fi; off-network devices may be visible if you log into the device or disable “Local device visibility,” but that is not a promise for a third-party MA device. [Spotify Connect support](https://support.spotify.com/us/article/spotify-connect/)  
Guest VLANs, AP isolation, and mDNS blockers will break discovery. On iPhone, Spotify needs Local Network permission enabled. MA’s BIND TO setting may help in complex networks. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect), [Spotify Connect support](https://support.spotify.com/us/article/spotify-connect/)

**7.6 Version coupling and regressions.**  
MA 2.10.8 has an open regression: Spotify playback stops mid-track, with `Spotify paused ..., stopping queue` immediately before a Sendspin STOP. Multiple reporters, both Soloist and librespot, across Sendspin, AirPlay, Squeezelite, and Chrome; rollback to 2.10.7 restored stability. [MA issue #6860](https://github.com/music-assistant/support/issues/6860)  
The local MA is exactly 2.10.8. [local: STATE.md](../../docs/STATE.md)  
Separately, Connect→ESPHome Sendspin had a 60-second sync-loss/dropout report; it was closed as not planned due to inactivity, with the maintainer suggesting Wi-Fi/RF interference and buffer increases, and noting Sendspin protocol reliability work is ongoing. [MA issue #6047](https://github.com/music-assistant/support/issues/6047)  
MA release notes show active Sendspin position/streaming fixes, so this area is moving; pin and verify. [MA server releases](https://github.com/music-assistant/server/releases)

**7.7 Security/quarantine.**  
The station’s egress guard is a deliberate security control: private LAN/DHCP/NTP only, public WAN dropped. [local: SECURITY.md](../../docs/SECURITY.md)  
A native receiver on the station (B) requires Spotify WAN access, either by opening the guard or by proxying through the LAN. Both weaken the quarantine. A and C keep WAN on the nettop, which is the security-preserving architecture.

### 8. Risk register

| ID | Risk | Options affected | Likelihood | Impact | Evidence | Mitigation | Rollback |
|---|---|---|---|---|---|---|---|
| R1 | MA 2.10.8 Spotify mid-track stop (`Spotify paused ... stopping queue`) | A, D, E | **High** (open issue, local version affected; local “playback verified” was a short test) | High (playback unreliable) | [MA #6860](https://github.com/music-assistant/support/issues/6860), [local STATE](../../docs/STATE.md) | Pin MA to 2.10.7 or wait for fix; test 30+ min before deploying; keep image tag | Revert MA image/backup; disable plugin |
| R2 | Connect→Sendspin 60 s sync loss/dropout | A, E | **Medium** (reported on ESPHome Sendspin; Pulsar native untested) | High (audible dropouts) | [MA #6047](https://github.com/music-assistant/support/issues/6047) | Test 60+ min; try MA Sendspin buffer/latency settings; isolate Wi-Fi/RF; if unfixable, fall back to D or C | Disable plugin; return to MA queue |
| R3 | Single Spotify account/stream conflict: voice and app stop each other | A, B, C, E | **High** (inherent) | Medium (UX surprise) | [MA Spotify](https://www.music-assistant.io/music-providers/spotify), [Spotify KB](https://community.spotify.com/forum/en/kb/articles/834-premium-family-duo-only-one-person-can-listen-at-a-time) | Explicit source policy; use separate account (E2) for simultaneous use; announce behavior in HA | Disable plugin or switch account |
| R4 | Voice `play_media` steals the Connect session; local “next” may not map to Spotify | A, E | **High** (inherent to external source) | Medium | [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect), local Pulsar README (`projects/station/upstream/pulsar/README.md`, только локально) | Test G3; add HA script to stop/release before voice play; document “voice takes over” | Disable plugin |
| R5 | MA “powered on” player takes control while using Spotify app | A, E | **Medium** | Medium | [MA troubleshooting](https://www.music-assistant.io/faq/troubleshooting) | “Power off” the MA player when using the app; add a Spotify-mode toggle | Power player back on |
| R6 | Phone on another network/VLAN cannot discover the device | A, C, E | **High** if guest VLAN/AP isolation | Medium | [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect), [Spotify support](https://support.spotify.com/us/article/spotify-connect/) | Flat L2 or mDNS reflector; BIND TO; test from cellular/guest | N/A |
| R7 | Soloist 90-day build expiry / update failure (exit code 10) | A, B, C, E | **Medium** (recurring) | High (playback stops) | [Soloist downloads/updates](https://developer.spotify.com/documentation/soloist/reference/downloads-and-updates) | Let MA auto-update; monitor MA logs/version; calendar check; pin working build | Replace binary; MA restart |
| R8 | Soloist cannot run on Docker bridge; mDNS binds wrong interface | A, E | **Medium** | High (device not visible) | [MA Spotify](https://www.music-assistant.io/music-providers/spotify), [MA #6324](https://github.com/music-assistant/support/issues/6324) | Keep MA on host network; set BIND TO/Published IP; avoid bridge | Revert network mode |
| R9 | 429/Web API blocks library, Liked Songs, voice favorites | D, A/E voice path | **High** locally | Medium | [local STATE](../../docs/STATE.md), [MA Spotify](https://www.music-assistant.io/music-providers/spotify) | Personal Client ID; wait/retry; cache library; Connect playback bypasses for app | N/A |
| R10 | ToS/legal: MA’s Soloist download/run is not clearly allowed; librespot is reverse-engineered | A, B, C, E | **Medium** | Medium–high (account/legal) | [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect), [Soloist GitHub](https://github.com/spotify/soloist), [go-librespot](https://github.com/devgianlu/go-librespot) | Prefer official Soloist; personal use; no redistribution; do not productize; accept risk explicitly | Disable plugin/remove binary |
| R11 | Native receiver on station cannot reach Spotify due to egress guard; opening WAN weakens quarantine | B | **Very high** | High (security regression) | [local SECURITY](../../docs/SECURITY.md), [local ARCHITECTURE](../../docs/ARCHITECTURE.md) | Do not open station WAN; if experimenting, use LAN proxy with explicit, reversible rule; prefer A/C | Restore iptables/guard |
| R12 | Native receiver on station cannot use Pulsar audio path; ALSA/volume/ducking conflicts | B | **High** | High (overlap, no voice) | local Pulsar README (`projects/station/upstream/pulsar/README.md`, только локально), [Soloist GitHub](https://github.com/spotify/soloist), spotiya README (`resources/spotiya/README.md`, только локально) | Do not pursue; if lab, use a spare station and accept no voice integration | Remove service/restore rootfs |
| R13 | MA multi-account leak: playback appears in another account’s Spotify app as “Music Assistant” | E2 | **Medium** | Medium (privacy/UX) | [MA #6402](https://github.com/music-assistant/support/issues/6402) | Test with two accounts; separate MA users/providers; monitor device names | Remove second provider |
| R14 | Connect session drops mid-playback (“device became inactive”) | A, C, E | **Medium** | Medium | [MA #5575](https://github.com/music-assistant/support/issues/5575) | Update MA; monitor; retry play; keep queue/automation fallback | Restart plugin/MA |

### 9. Recommendation

**Primary (if the Spotify app device picker is a must): Option A, deployed on the nettop.**
1. **Gate on MA version.** Do not test on 2.10.8 as-is; the open mid-track stop regression affects Spotify playback. Pin to 2.10.7 or wait for a fixed release. Keep the current MA data/config backup. [MA #6860](https://github.com/music-assistant/support/issues/6860)
2. **Add the Connect plugin with Soloist**, not go-librespot, because the account is new and the local install already uses Soloist 1.3.10.13. Keep MA on host network; do not move it to a Docker bridge. [MA Spotify](https://www.music-assistant.io/music-providers/spotify), [local: stack/docker-compose.yml](../../projects/home-assistant/stack/docker-compose.yml)
3. **Select only the Pulsar player** (and optionally a group, but not both) to keep device names and stream conflicts manageable.
4. **Run the G1–G6 tests** before declaring success. The two make-or-break tests are 60+ minutes of Connect→Pulsar without dropouts, and Alice pause/next behavior while Connect is active.
5. **Keep D (music provider) for voice**, and adopt E1 source policy: voice takes over; app takes over; one active source. If simultaneous use is required, move to E2 with a second account.

**Secondary (if voice reliability is more important than app presence): Option D.**
Keep the current music provider + voice; do not add Connect until MA is stable and the Pulsar Sendspin path is proven. This is the lowest-risk state and already works except for the 429/Liked Songs sync.

**Reject for now: Option B.**
The station’s egress guard and Pulsar’s audio architecture are fundamental blockers, not install details. A native receiver would also lose Alice/HA integration and create ALSA/volume/ducking conflicts.

**Lab-only: Option C.**
Only if A’s plugin is unusable and you accept a custom bridge with no MA/HA metadata or voice control. It is dominated by A for this stack.

**Do not treat “Soloist via MA” as a licensed/partner integration.** MA itself warns Spotify’s terms do not clearly allow it; use personal, non-commercial, no redistribution, and keep rollback. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)

### 10. Test plan / gates (read-only research; execute only with approval)

| Gate | Test | Pass criterion | What it proves |
|---|---|---|---|
| **G1** | MA 2.10.7 + Connect plugin + Soloist, play 60+ min to Pulsar | No mid-track stop, no 60 s sync loss, no audible dropout | R1/R2 viability |
| **G2** | Phone→speaker, speaker→phone, pause, seek, next/previous, volume | Device appears; switching works both ways; position/volume plausible; app may lead audio by a small buffer | A’s core UX |
| **G3** | While Connect plays: “Алиса, пауза”, “продолжи”, “дальше”, “включи Radiohead” | Pause/resume/next behavior documented; voice search takes over cleanly or is blocked with a clear message | Voice coexistence |
| **G4** | Account conflict: start voice queue, then app; start app, then voice; start phone Spotify elsewhere | One source wins predictably; no double audio; no stuck device | R3/R4 |
| **G5** | With music provider 429, start Connect playback from app | Connect plays; voice favorites still report not synced | 429 split |
| **G6** | Phone on cellular/guest VLAN; toggle Local device visibility | Device visibility documented; no false promise of cross-network | R6 |
| **G7** | Disable plugin / unselect player; restart MA; play from MA queue | Pulsar returns to normal MA Sendspin playback; no station changes | Rollback |

### 11. Implementation risk summary

| Option | Technical risk | Product/ToS risk | Operational risk | Overall |
|---|---|---|---|---|
| A | Medium: MA plugin/Soloist/Sendspin coupling | Medium: MA’s Soloist use unclear | Medium: MA releases, 90-day builds | **Medium** |
| B | Very high: egress, audio server, ABI, rootfs | Medium–high: unsupported platform, redistribution | High: manual updates, OTA | **Very high** |
| C | High: custom bridge, metadata/voice gaps | Medium: Soloist/go-librespot terms | Medium–high: two services | **High** |
| D | Low: existing stack | Low–medium: Web API terms, 429 | Low | **Low** (but misses goal) |
| E | Medium–high: two providers/source policy | Medium: A + D | Medium | **Medium–high** |

---

## Evidence, assumptions, and gaps

### 12. Confirmed vs inferred

**Confirmed by primary sources or local observation:**

- MA separates the Spotify music provider from the Spotify Connect plugin; the plugin exposes any MA player/group as a Connect device, one device per selected player. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)
- MA plugin: same network required; stopping in MA releases the device; moving playback to another device stops the MA player; volume default is player-only, optional sync. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)
- MA music provider: Premium required; Free does not work; Soloist works with accounts created since Dec 2024; librespot generally does not; Soloist cannot run on Docker bridge hosts; one Soloist stream per account. [MA Spotify provider](https://www.music-assistant.io/music-providers/spotify)
- MA warns it may not distribute Soloist and Spotify’s terms do not clearly allow MA’s use; Soloist is downloaded from Spotify with consent and auto-updated. [MA plugin docs](https://www.music-assistant.io/plugins/spotify-connect)
- Soloist: official headless client; ARMv7/ARM64/x86_64 Linux; requires PipeWire/PulseAudio; builds expire in 90 days; API key needs Premium; Free can connect after setup; lossless up to 24/44.1; local WebSocket API with metadata/queue. [Soloist GitHub](https://github.com/spotify/soloist), [Soloist getting started](https://developer.spotify.com/documentation/soloist/tutorials/getting-started), [Soloist announcement](https://developer.spotify.com/blog/2026-08-13-introducing-spotify-soloist)
- go-librespot: Go Connect client; ALSA/PulseAudio/pipe backends; REST/WebSocket; MPRIS; Premium required. [go-librespot](https://github.com/devgianlu/go-librespot)
- Spotify Connect: first pairing requires same Wi-Fi; pause >10 min may require reconnect; off-network visibility is possible if logged into the device or “Local device visibility” is off. [Spotify Connect support](https://support.spotify.com/us/article/spotify-connect/)
- Station: aarch64 kernel, armhf userspace, ro rootfs, Pulsar v0.4.1, maind stopped, egress guard private-LAN-only. [local: STATE.md](../../docs/STATE.md), [local: SECURITY.md](../../docs/SECURITY.md)
- Pulsar: GStreamer HTTP + Sendspin → ALSA dmix; local next/previous target Sendspin; MA sees it as Sendspin. local: Pulsar README (`projects/station/upstream/pulsar/README.md`, только локально)
- Nettop: MA 2.10.8 on host network, Soloist 1.3.10.13, Spotify playback tested; 429 on Web API. [local: STATE.md](../../docs/STATE.md), [local: stack/docker-compose.yml](../../projects/home-assistant/stack/docker-compose.yml)
- MA 2.10.8 Spotify mid-track stop regression is open and reproduced by multiple users; 2.10.7 rollback restores stability. [MA #6860](https://github.com/music-assistant/support/issues/6860)
- Connect→ESPHome Sendspin 60 s sync loss was reported and closed as not planned; maintainer cited RF/Wi-Fi and low-latency stream buffering, with Sendspin protocol work ongoing. [MA #6047](https://github.com/music-assistant/support/issues/6047)

**Inferred / not directly tested:**

- Pulsar can be targeted by the MA Connect plugin without a Pulsar-side change. The plugin supports “any MA player,” and Pulsar is an MA Sendspin player, but no Pulsar-specific report was found. `[INFERRED]`
- Pulsar has no external audio input API. The docs only describe HTTP streams and Sendspin; absence of a documented input is not proof, but it is the working assumption. `[INFERRED]`
- Soloist’s ARMv7 build will not run on the station because of missing PipeWire/PulseAudio and/or glibc mismatch. No on-device check was performed. `[INFERRED]`
- A native receiver on the station cannot reach Spotify because of the egress guard. The guard is confirmed; whether Spotify endpoints are fully unreachable without a proxy is inferred from “public WAN DROP.” `[INFERRED]`
- MA `media_pause` on the Connect external source pauses the Spotify session; the docs only explicitly say stop releases the device. `[INFERRED]`
- Pulsar’s local “next” while Connect is active maps to Spotify next. The docs say local next targets Sendspin; mapping to the Connect source is unverified. `[INFERRED]`
- The 2.10.8 regression affects the Connect plugin, not just the music provider. The issue logs are from `music_assistant.spotify` (music provider); the Connect plugin may use a different code path. `[INFERRED]` — do not assume either way without G1.
- A second Spotify account fully isolates the two sessions. MA docs say a second account has its own session, but multi-account bugs exist (#6402). `[INFERRED]`
- End-to-end lossless quality survives MA→Sendspin→Pulsar. MA docs say Sendspin sends 16-bit; the issue log shows a 48 kHz/F32 session; Pulsar’s DAC/amp limits are unknown. `[INFERRED]`

### 13. Source conflicts and ambiguities

| Conflict | What the sources say | How to treat it |
|---|---|---|
| Station userspace: armhf vs aarch64 | STATE: “kernel aarch64; userspace armhf.” RESOURCES: “our Midi is aarch64; ready armv7 binary won’t fit,” rejecting spotiya. Pulsar ships an armhf binary and runs. | Prefer STATE + Pulsar evidence: userspace is armhf. Treat RESOURCES’ aarch64 statement as stale/incorrect. Verify with `uname -m`/`file` before any build. |
| One stream vs “librespot supports two players” | Spotify consumer KB: one stream per account. MA table: librespot “playing on two players at once: Supported.” | Assume one active Spotify stream per account for planning; test with the actual account. The MA phrase may mean MA multiroom fan-out, not two independent Spotify sessions. |
| Sendspin 16-bit vs 48 kHz/F32 | MA Sendspin docs: “Audio is sent to Sendspin players as 16 bit.” Issue #6047 log: “Sendspin session PCM format: 48000 Hz / F32.” | The docs may describe the source format or a default; the session log shows F32. Do not claim bit-perfect lossless end-to-end without measurement. |
| Same-network only vs off-network visibility | MA plugin: phone must be on same network; Spotify’s “different Wi-Fi” feature unsupported. Spotify support: devices can be visible off-Wi-Fi if logged in or Local device visibility is off. | Different contexts: MA plugin is a third-party Connect device; Spotify’s article covers official/logged-in devices. Promise home-LAN only; treat off-network as unverified. |
| Soloist ToS: official vs “terms do not clearly allow” | Spotify/Soloist presents Soloist as an official DIY client. MA warns Spotify’s terms do not clearly allow MA downloading/running it on your behalf. | Both can be true: Soloist itself is official; MA’s distribution/run model is the gray area. Do not treat MA’s use as licensed. |
| Soloist ARMv7 glibc baseline | Official docs give ARMv7 build but only state glibc 2.41 for the ARMv8 test device. | `[GAP]` for arm32 glibc/ABI; on-device `ldd --version`/run test required before B. |

### 14. Gaps

- **No Pulsar-specific Spotify Connect test.** No report or local test of MA Connect → Pulsar Sendspin was found; the closest evidence is ESPHome Sendspin (#6047). `[GAP]`
- **No on-device check for PipeWire/PulseAudio/glibc** on the Midi. `[GAP]`
- **No measured latency/position drift** for Connect→Pulsar; MA docs only warn it can occur. `[GAP]`
- **No confirmation whether MA pause propagates to the Spotify session**; only stop/release is documented. `[GAP]`
- **No confirmation whether Pulsar local next/previous maps to Spotify next/previous** when the source is Connect. `[GAP]`
- **No official Spotify statement on MA’s plugin or on Soloist-via-MA ToS.** MA’s warning is the best available; not legal advice. `[GAP]`
- **No root-cause/fix ETA for MA #6860.** It is open as of 2026-10-26; rollback is the known workaround. `[GAP]`
- **No data on whether off-network Spotify app control works for the MA plugin.** Spotify support’s off-network note is for official/logged-in devices; MA docs say same network. `[GAP]`
- **No measurement of Station Midi DAC/amp quality or resampling** in the Sendspin path. `[GAP]`
- **No confirmation that a second Spotify account avoids all MA multi-account bugs**; #6402 shows a leak. `[GAP]`
- **No test of MA announcements/TTS overlapping a Connect source** on Pulsar. `[GAP]`
- **No review of Spotify’s Commercial Hardware/partner terms.** This note assumes personal, non-commercial DIY use; productizing a Spotify target would require a separate legal/partner review. `[GAP]`

### 15. Sources

**Music Assistant**
- Spotify Connect plugin: https://www.music-assistant.io/plugins/spotify-connect
- Spotify music provider: https://www.music-assistant.io/music-providers/spotify
- Sendspin player: https://www.music-assistant.io/player-support/sendspin
- Sendspin Source plugin: https://www.music-assistant.io/plugins/sendspin-source
- Voice control: https://www.music-assistant.io/integration/voice
- Troubleshooting (player power/Connect interference): https://www.music-assistant.io/faq/troubleshooting
- Server releases: https://github.com/music-assistant/server/releases
- Issue #6860 (2.10.8 Spotify stops mid-track): https://github.com/music-assistant/support/issues/6860
- Issue #6047 (Connect→Sendspin 60 s sync loss): https://github.com/music-assistant/support/issues/6047
- Issue #6402 (multi-account “Music Assistant” playback leak): https://github.com/music-assistant/support/issues/6402
- Issue #6324 (Soloist advertised IP): https://github.com/music-assistant/support/issues/6324
- Issue #5575 (Connect session drops): https://github.com/music-assistant/support/issues/5575
- Discussion #419 (Connect/queue design history): https://github.com/orgs/music-assistant/discussions/419
- HA Music Assistant integration: https://www.home-assistant.io/integrations/music_assistant

**Spotify / Soloist**
- Soloist announcement: https://developer.spotify.com/blog/2026-08-13-introducing-spotify-soloist
- Soloist GitHub: https://github.com/spotify/soloist
- Soloist getting started: https://developer.spotify.com/documentation/soloist/tutorials/getting-started
- Soloist overview: https://developer.spotify.com/documentation/soloist/concepts/overview
- Soloist downloads/updates (90-day expiry): https://developer.spotify.com/documentation/soloist/reference/downloads-and-updates
- Spotify Connect support: https://support.spotify.com/us/article/spotify-connect/
- Spotify Community KB (one stream per account): https://community.spotify.com/forum/en/kb/articles/834-premium-family-duo-only-one-person-can-listen-at-a-time
- Spotify Community discussion (multiple streams): https://community.spotify.com/forum/en/discussion/5311229/how-to-play-multiple-streams-on-different-devices

**Other clients**
- go-librespot: https://github.com/devgianlu/go-librespot
- librespot: https://github.com/librespot-org/librespot
- spotiya: https://github.com/klockky/spotiya (local copy: spotiya README (`resources/spotiya/README.md`, только локально))

**Local lab docs**
- [STATE.md](../../docs/STATE.md)
- [ARCHITECTURE.md](../../docs/ARCHITECTURE.md)
- [SECURITY.md](../../docs/SECURITY.md)
- [PENDING.md](../../docs/PENDING.md)
- [RESOURCES.md](../../docs/RESOURCES.md)
- Pulsar README (`projects/station/upstream/pulsar/README.md`, только локально)
- [HA voice README](../../projects/home-assistant/ha-voice/README.md)
- [HA intent scripts](../../projects/home-assistant/ha-voice/intent_scripts.yaml)
- [MA compose](../../projects/home-assistant/stack/docker-compose.yml)
- [Stack README](../../projects/home-assistant/stack/README.md)
