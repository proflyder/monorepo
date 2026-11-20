# Home Android client

Kotlin/Android management client for the home server. The server owns
scheduling and execution; the phone manages server state and reads the Home
catalog. One Activity, Fragment screens, Metro compile-time DI, feature
API/implementation modules, and a typed router.

## Limits

Debug builds only. App Distribution builds are release-signed with a stable
personal key so upgrades keep app data and the encrypted token (see
[Signing](#signing)); the APK Bazel produces is an intermediate. Push delivery
(FCM) and the find-phone ring are implemented
([Push and find phone](#push-and-find-phone)); there is still no offline cache
or generic task UI.
Current device acceptance and remaining work are tracked in
[STATE](../../../docs/STATE.md) and [PENDING](../../../docs/PENDING.md).

## Features

- Bottom tabs: **Home**, **Scenarios**, **Time**, **System** (see
  [System dashboard](#system-dashboard)). **Time** is a `ViewPager2`
  container with a segmented **Alarms** / **Timers** / **Reminders** switch;
  a horizontal swipe and the pills drive the same page, the existing feature
  screens are reused unchanged, and deep links such as `selectTab("alarms")`
  open the Time tab on the matching section.
- Top app bar: exactly two actions, no overflow menu — **Call** (phone icon) and
  **Settings** (gear), both always visible with content descriptions from
  resources. **Settings** is not a tab: it opens as an overlay over the current
  tab and Back returns there. It is one list of navigation rows (icon, title,
  subtitle, chevron) over the app's tonal cards:
  - **Connection** → the server/token screen ([Connection](#connection));
  - **Phone notifications and find** → the push permission and registration
    screen ([Push and find phone](#push-and-find-phone));
  - **Notification history** → the local log of received messages
    ([Notification history](#notification-history));
  - **Appearance** → theme and light/dark mode ([Themes](#themes)).
- Call: the phone icon in the top app bar and the pinned **Call the station**
  tile in Home's Quick actions place a real audio call to the station through the
  intercom signaling service and Android Telecom. See
  [Call (intercom)](#call-intercom).
- Alarms: list/create/edit/enable/disable/delete, snooze (9 min), stop; one-shot
  with explicit date, daily, weekdays, weekends, custom ISO weekdays, IANA
  timezone. Server one-shot alarms stay one-shot when edited.
- Reminders: one-shot date/time in an IANA timezone; the server stores
  `onceDate` and owns delivery. No phone-side scheduling.
- Timers: start/list/pause/resume/cancel; remaining time uses the server clock
  from `GetStatus`, not the phone clock.
- Alarm multi-select: long-press enters selection; bulk enable/disable/delete
  run sequentially and stop on first failure, keeping unprocessed alarms
  selected.
- Connection: scheduler, optional Home control and optional Intercom base URLs
  plus connect/read timeouts come from Firebase Remote Config
  (`home_connection`); the bearer
  token is entered on the phone, **Save and test connection** (`GetStatus`)
  verifies the server, and a read-only section shows the active Remote Config
  values with their state and a **Refresh config** action. Explicit
  offline/auth/TLS/validation states. Settings persist across failures and are
  never cleared by a failed request.
- Loading/empty/error/content states; existing data stays visible during refresh
  or transient errors. Lists never silently become empty.

## System dashboard

- `features/system`: the **System** tab reads `GET /v1/system/{summary,hosts,
  k8s,devices}` from `mcp-gateway` through the **Home control** address (Traefik
  sends `/v1/system/` on that host to the gateway) with the same bearer token.
  Without a control address the screen shows the connection error state.
- Overview: a traffic-light card (worst level of the sections and active
  alerts, alert count, update time), the active alerts, then four cards —
  **Nettop (Proxmox)**, **Guests (VM/CT)**, **Cluster**, **Devices**. Tapping a
  card opens its detail list: host metrics; every VM/CT; namespaces → pods with
  problems first; the speaker and the phones registered for push.
- Pull-to-refresh plus an automatic refresh every 30 s while the screen is
  started (`repeatOnLifecycle(STARTED)`); the server caches each section 15 s.
  A failed refresh keeps the last data and shows a notice; with no data the
  error state has a Retry button. A missing source is «No data», never zero.
- Levels use theme attributes only: ok `?colorPrimary`, warning
  `?colorTertiary`, critical `?colorError`, no data `?colorOutline`.
- Tests: `core/network:network_test` parses the server's golden responses
  (`fixtures/system/*.json`, copied from the Rust contract);
  `features/system/impl:system_test` covers the row mapping, the ViewModel and
  Robolectric PNGs (`system-overview`, `system-overview-no-data`,
  `system-cluster`, `system-guests-dark`) in `TEST_UNDECLARED_OUTPUTS_DIR`.

## Hermes chat (network contract)

- `core/network` `HttpChatClient` talks to `mcp-gateway` `/v1/chat/*` through
  the **Home control** address (Traefik sends `/v1/chat/` on that host to the
  gateway) with the same bearer token: `page(before, limit)`,
  `send(clientId, text, replyTo)`, `retry(id)` and the SSE `events(after)`
  (`message` upserts and `delta` `{id, offset, text}`; `offset` is the UTF-16
  length of the text the delta continues). Errors arrive as
  `RemoteFailure.Server(code = <resource key>)`, e.g. `chat_text_empty`.
- `ConnectionInterceptor` keeps the request query string when it rewrites the
  URL onto the configured host.
- Tests: `ChatClientTest` reads the server's golden files
  (`fixtures/chat/*.json`, copied from
  `solutions/mcp-gateway/crates/mcp-gateway/src/chat/testdata/contract/`).

## Hermes chat (screen)

- `features/chat` is the fifth bottom tab (**Chat**, between Time and System):
  the same Hermes agent as the Telegram bot, one persistent conversation kept
  on the server, so a reinstall or a second phone sees the same history.
- Feed: bubbles (mine on the end in `?colorPrimaryContainer`, the assistant on
  the start in `?colorSurfaceContainerHigh`, a smaller corner on the tail
  side), day headers, time and a send status for my messages (clock — waiting
  for the network, check — delivered, error — not delivered), a typing bubble
  while the reply has no text yet and a caret while it streams. Assistant text
  is rendered by `ChatMarkdown` without extra libraries: bold, italic, inline
  code and fenced blocks, headings, bullet and numbered lists, Markdown links
  and bare URLs (clickable).
- Long press opens Reply / Copy / Retry / Delete (delete only for a message the
  server never accepted); a swipe to the left quotes a message. The quote is
  shown above the input and inside the sent bubble; the server forwards it to
  Hermes with the message.
- The list sticks to the bottom while new messages arrive; when scrolled up a
  small button returns to the latest messages, and reaching the top loads the
  previous page (`before=<seq>`).
- Offline queue: `ChatRepository` keeps unsent messages in DataStore
  (`chat_outbox`) with their `clientId`; it resends them one by one when the
  network returns (exponential back-off up to 30 s, `NetworkWaiter`). The server
  is idempotent on `clientId`, so a resend never duplicates. A message the server
  rejects (empty, too long, missing quote) stays in the feed with the reason and
  Retry/Delete; a failed assistant reply shows the reason and Retry.
- Live updates: while the tab is started the repository reads the SSE stream
  from its last `cursor`, applies `message` upserts and `delta` chunks
  (a delta is applied only when its `offset` equals the current text length),
  and reconnects with back-off; `401` stops the loop.
- Insets: the input panel uses `pinAboveKeyboard`, the empty/error states follow
  the list padding so they stay above the panel; `Widget.Home.TextInput.Chat`
  is the shared input style without a floating hint.
- Voice: with an empty input the send button is a microphone; hold to record
  (`AudioRecord`, 16 kHz mono PCM written as WAV — exactly what the server feeds
  to GigaAM), release to send, slide left to cancel, 2 minutes at most, shorter
  than 0.6 s is dropped. The first hold asks for `RECORD_AUDIO`. A voice bubble
  has play/pause, progress and duration (`MediaPlayer`, one at a time) and the
  server transcript in italics under it («Recognising…» until it arrives).
- Attachments: the clip button offers Photo (system photo picker, scaled to
  2048 px and re-encoded as JPEG) and Text file (≤ 256 KB, text/JSON). Picked
  items wait as chips above the input; on send they are uploaded one by one
  (`POST /v1/chat/attachments`) before the message, and the uploaded ids are
  saved in the outbox so a retry never uploads twice. A file that disappeared
  from the phone fails the message with its own key. Received media are cached
  under `files/chat-media/<id>`; a photo opens in a dialog on tap.
- Server error keys map to string resources in `ChatTexts.kt`; the server never
  sends user-facing text.
- Tests: `features/chat/impl:chat_test` (timeline reducers, outbox codec,
  repository queue with an offline/timeout/permanent failure and attachment
  uploads, feed items, Markdown, Robolectric PNGs `chat-current`,
  `chat-current-dark`, `chat-neon`, `chat-neon-dark`, `chat-neon-typing`,
  `chat-media-current` (voice with transcript and playback, file, photo) and
  `chat-media-neon-dark` (the recording panel)) and
  `app:window_insets_screenshot_test` (`insets-{neon,current}-chat-keyboard`:
  the panel sits on the keyboard and the bottom bar hides).

## Home hub (devices, live cards, quick actions)

- The catalog comes from the home control service (`GET /v1/catalog`); nothing
  is hard-coded in the app. Names, rooms, icons, and hidden flags come from Home
  Assistant.
- Devices are lights/switches; the station also adds two `volume` rows (main
  volume with mute/unmute and announcement volume) that open the shared volume
  sheet. HA scripts are no longer shown on Home; native scenarios live in the
  **Scenarios** tab and in Quick actions.
- The **live rows** block at the top is hidden when there is nothing to show: it
  merges enabled alarms, running/paused timers (with a local one-second
  countdown) and reminders that fire within the next 24 hours into at most three
  single-line rows ordered by time (icon · time · label). An alarm row has a
  compact switch (turning a recurring alarm off asks skip-once vs disable), a
  timer row has a small pause/resume button, and tapping a row opens its section.
  Data comes from the existing feature repositories through their API modules.
  The hub refreshes on resume and pull-to-refresh.
- **Quick actions** open from a FAB as a speed-dial overlay: tapping a row
  runs the action, long-pressing a device or scenario pins/unpins it. The dial
  lists pinned devices, scenarios, the "Say on the station" action, the wake
  marks and the built-in Call tile; order and set are kept in DataStore. Pins
  are changed by long-pressing a device in a room group or a native scenario in
  the scenario list (**Pin to Home** / **Unpin**). A non-light device tile
  toggles on tap; a light tile opens its brightness/color sheet on tap. On first
  launch the speak action and the native **Blackout** / **Work mode** scenarios
  (when they exist) are pinned. The built-in Call item is always present.
- Filter chips (All / Lights / Switches, plus room filters) are persisted in
  DataStore and restored on launch. Room sections are collapsible; the folded
  state is UI-only.
- The **Voice** card sits between Quick actions and the rooms and merges the
  assistant profile with the two wake marks. The profile is an HA switch
  carrying the `voice_profile` label (shown as `kind: "profile"`) and uses the
  shared pill switch — **Personal** (`turn_off`) / **Work** (`turn_on`); row
  taps do not act, only an explicit pill does. The wake chips (`ear-off` /
  `ear`) send the `script.wake_false_alarm` and `script.wake_missed` marks at
  once. Each row hides on its own, and the card disappears when neither the
  profile nor a wake script exists.
- Tapping a scenario or toggling a device enqueues a scheduler task
  `home.control` on the `control` queue (`EnqueueTask`). The phone only queues;
  the server executor owns execution. Device switches are optimistic and then
  re-read from the server; the profile uses the same optimistic update and
  re-read. Delivery is at-least-once, not exactly-once.
- Without a Home control address the tab explains what to configure; it does not
  fail silently.

### Marking the profile entity in Home Assistant

Nothing in the app or the server lists entity ids, so the profile switch is
selected by a label in Home Assistant:

1. Create the switch that holds the mode (for example a template switch over
   `input_select.ricardo_voice_profile`: on = Work, off = Personal). Its
   `on`/`off` must mean "the named profile" / "the default profile".
2. Home Assistant → **Settings → Areas, labels & zones → Labels → Create
   label**, id `voice_profile` (name "Voice profile").
3. Open the switch's settings → **Labels** and add `voice_profile`.
4. The next catalog read reports the entity as `kind: "profile"`; removing the
   label turns it back into an ordinary switch row. A different label id is set
   with `CONTROL_PROFILE_LABEL` on `ha-control`.

All feature UI is localized in English and Russian (`values-ru`); the app
label and some framework-level strings may fall back to English. Server
messages arrive as language-neutral `home.v1.LocalizedText` keys and are
resolved by `ServerText` to `@string/server_<key>` (`scheduler.error.not_found`
→ `server_scheduler_error_not_found`); an unknown key falls back to the legacy
server text, then to a generic message.

## Scenarios

The **Scenarios** tab is a builder for the scheduler's durable
`home.v1.ScenarioService` definitions. The server owns storage, validation and
time-trigger materialization; the `ha-scenario` executor consumes the
`scenario` queue, and the phone only edits the definition and queues manual
runs.

- A scenario has a name (≤ 80 bytes), description (≤ 512 bytes), up to 8
  triggers, 8 conditions and 32 ordered steps, `enabled`, and
  `continueOnError`.
- Triggers: a manual run is always available (`enabled` only gates automatic
  time triggers); a time trigger is a local wall-clock time with the phone's
  IANA zone and once/daily/weekdays/weekends/custom ISO weekdays. Cards show a
  short summary such as **Weekdays 07:30** and the server-computed next run.
- Conditions: an entity state (any exact Home Assistant state, with On/Off
  shortcuts) chosen from the live catalog, or a time window (start/end,
  optional weekdays; an empty set means every day, and end before start wraps
  over midnight).
- Steps: a catalog action (pick an entry from `GET /v1/catalog`, then one of
  its actions; lights accept brightness, color temperature, effects from
  `light.effects`, and a 0–300 s transition, while `turn_off` accepts only a
  transition), **Say on the station** (reuses `SpeakText.normalize`, 1–500
  characters), or **Pause** (1–3600 seconds, entered as seconds or minutes).
  Steps move up/down and can be removed.
- Create/Update carry an idempotency key and `expectedRevision`; a revision
  conflict (`aborted`) shows a reload dialog instead of overwriting the other
  change. Delete asks for confirmation. Manual run uses a per-tap idempotency
  key and reports "queued", not "executed".
- A newer server may send a trigger/condition/step oneof variant this build
  does not know. The app keeps the original JSON object, shows the scenario
  with an unsupported badge, and **blocks saving** (delete remains possible) so
  an older client cannot drop or corrupt fields it cannot represent.
- All limits are mirrored locally (`ScenarioValidation`) so Save is disabled
  before a request the server would reject. The UI is localized in English and
  Russian.

## Call (intercom)

The phone icon in the top app bar and the built-in **Call the station** tile
pinned first in Home's Quick actions open the lock-screen call screen. A call is a real Android call: a
self-managed Telecom `Connection`, a `phoneCall` foreground service with a
CallStyle notification, and an audio-only WebRTC peer connection.

- Signaling is `home.v1.IntercomService` over Connect/HTTP/1.1:
  `RegisterEndpoint`, `ListEndpoints`, `StartCall`, `Hangup`, `SendSignal`,
  `GetIceConfig`, and the server-streaming `Watch`. `Watch` uses the Connect
  envelope framing (1 flag byte + 4-byte big-endian length) because bidi
  streaming would require HTTP/2; the stream reconnects with a bounded backoff
  and also marks the phone online.
- The phone is always the caller: it calls the first online STATION endpoint,
  otherwise the first STATION endpoint (which then fails as `UNAVAILABLE` if
  offline), then `RINGING` → `CONNECTING` (the caller creates and sends the
  SDP offer) → `ACTIVE` after the station's answer. `Hangup` uses
  `CANCELED` while ringing and `HANGUP` otherwise.
- Media is libwebrtc `io.getstream:stream-webrtc-android` (Opus, audio only,
  unified plan). `JavaAudioDeviceModule` requests hardware AEC/NS where
  supported and falls back to software; the audio mode is
  `MODE_IN_COMMUNICATION`. `GetIceConfig` supplies STUN/TURN; an empty list
  means host candidates for the LAN/VPN case.
- `CallManager` is the single source of truth (`StateFlow<CallSnapshot>`)
  consumed by the screen, the Telecom connection and the notification. The
  state machine is plain Kotlin and unit-tested on the JVM.
- Audio output: **Phone**, **Speaker**, **Headset**, **Bluetooth <name>**. On
  API 34+ selection goes through the Telecom `CallEndpoint` API
  (`requestCallEndpointChange`, `onAvailableCallEndpointsChanged`); older
  releases use `AudioManager.setCommunicationDevice` (31+) or
  speakerphone/Bluetooth SCO flags. Bluetooth routes are hidden when
  `BLUETOOTH_CONNECT` is denied.
- Mute disables the local audio track. The in-call screen uses a proximity wake
  lock while the call is active (`showWhenLocked`/`turnScreenOn`).
- Incoming calls are architectural only: `Watch` recognises an incoming
  `RINGING` call for this endpoint and declines it. There is no incoming UI yet
  (`TODO(incoming)` in `CallManager` / `HomeConnectionService`).

### Call permissions

| Permission | When |
| --- | --- |
| `RECORD_AUDIO` | runtime, required before dialing; denial shows a controlled ended state |
| `MANAGE_OWN_CALLS` | normal, self-managed Telecom account |
| `FOREGROUND_SERVICE_PHONE_CALL` | normal, `phoneCall` service type |
| `BLUETOOTH_CONNECT` | runtime (API 31+); denial hides Bluetooth routes |
| `POST_NOTIFICATIONS` | runtime (API 33+); denial suppresses the ongoing notification, the call still works |

Without an intercom address or the shared app token the call button shows a
"not configured" message instead of attempting a request. No live call is
placed by tests or CI.



The Connection screen is reached from **Settings → Connection** (top app bar ⚙).
Both open as overlays on the current bottom tab; Back returns to the settings
list and then to that tab.

- Three base URLs, no paths, supplied by Remote Config:
  - **Server address** (scheduler): LAN `http://<node>:30098` or the home
    HTTPS name `https://alarms.<HOME_DOMAIN>` reached over the VPN.
  - **Home control address** (catalog, optional): LAN
    `http://<node>:30099` or the home HTTPS name
    `https://control.<HOME_DOMAIN>` reached over the VPN.
  - **Intercom address** (calls, optional): LAN `http://<node>:30097` or the
    home HTTPS name `https://intercom.home.<HOME_DOMAIN>` reached over the VPN.
  - The app appends the Connect RPC path or `/v1/catalog` itself. Do not put
    `/v1/catalog` into the configured URL.
- Connect and read timeouts also come from Remote Config. Invalid URLs,
  malformed JSON, or out-of-range timeouts are shown as an explicit invalid
  state; the app never crashes and no request is attempted.
- The same bearer token is used for scheduler, control and intercom. Endpoints
  and timeouts are deployment-specific and are set only in the Firebase
  console; none are embedded in the app or the repository. The token remains
  device-local and is never part of Remote Config.
- The intercom server takes that shared app token as `phone_token` in its
  Secret (ExternalSecret copies `scheduler-auth`'s `token`); the station keeps
  its own `station_token`. There is no separate call token in the app.
- Transport: Retrofit 3 on OkHttp, Connect unary JSON
  (`POST {endpoint}/home.v1.SchedulerService/{RPC}`) with typed
  kotlinx.serialization DTOs (`WireDtos.kt`), protobuf JSON field names and
  `int64` as decimal strings. `OutcomeCallAdapterFactory` maps responses and
  Connect errors to `Outcome`, `TokenRefreshAuthenticator` refreshes the bearer
  on 401. `Authorization: Bearer` is sent in the header only.
- Direct LAN `http://` is a deliberate private-network trade-off and is allowed
  by `network_security_config.xml`; HTTPS is recommended and uses the platform
  trust store. Redirects are disabled. No custom trust manager, hostname
  verifier, or certificate pinning; the system chooses LAN or VPN routing.

## Credentials and backup

- The bearer token is encrypted with a non-exportable Android Keystore
  AES-256/GCM key (`AES/GCM/NoPadding`, random IV, AAD). Only ciphertext is
  stored; the plaintext token never is. The token is entered on the phone and
  is never read from Remote Config.
- The encrypted token and UI preferences (Home filter) live in one Jetpack
  Preferences DataStore (`HomeDataStore`). Endpoints and timeouts are no longer
  read from or written to DataStore: Remote Config is authoritative. On the
  first launch after the upgrade, `SharedPreferencesMigration` imports the
  legacy `home_connection` file, so the encrypted token survives; the old
  endpoint/timeout keys remain only for that migration and are otherwise
  unused.
- Readers use a synchronous in-memory snapshot because interceptors read
  settings on I/O threads. The snapshot composes the Remote Config values with
  the device-local token and is replaced immediately when either changes.
- `android:allowBackup="false"`; backup/data-extraction rules exclude prefs,
  databases, and files. Tokens are never placed in URLs, `SavedState`, Fragment
  arguments, or logs. Diagnostics redact `Authorization`.
- If Keystore decryption fails, the token is reported unreadable and must be
  re-entered; there is no plaintext fallback.

## Build and test

Prerequisites: macOS/Linux, JDK 21, Android SDK with platform 36,
build-tools 36.0.0 and an NDK (`ANDROID_HOME`, and `ANDROID_NDK_HOME` or an
`ndk/` directory under the SDK), Bazelisk. The APK targets the phone ABI
`arm64-v8a` via `//platform/android:android_arm64`; `rules_android` needs the
NDK's C++ toolchain for that platform. Install it with
`sdkmanager "ndk;28.2.13676358"` (the version CI pins). The root contract is
`MODULE.bazel` (no Gradle, no `WORKSPACE`).

```sh
cd /path/to/yandex-station-lab
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358"  # optional under mise
mise install   # Bazelisk and the rest of mise.toml

mise exec -- bazelisk build --config=android //projects/mobile/android:app
# bazel-bin/projects/mobile/android/app/app.apk

bazelisk test --config=android //projects/mobile/android:android_tests
# Bazel build tools are Kotlin/JVM: unit tests for the generators and lint runner.
bazelisk test --config=android //platform/android/...

# Android Lint over the whole app graph (separate CI job `android / android-lint`).
bazelisk test --config=android //projects/mobile/android:android_lint

# detekt (separate CI job `android / android-detekt`): built-in rules,
# formatting and the first-party `home-rules` plug-in.
bazelisk test --config=android //projects/mobile/android:detekt

# Optional: install on a connected device (keeps app data).
adb install -r bazel-bin/projects/mobile/android/app/app.apk
```

Android Lint is **not** part of `android_tests`: it runs as its own CI unit
(`android / android-lint`) and must also be run explicitly with
`bazelisk test --config=android //projects/mobile/android:android_lint`.
Keeping it separate
means a lint finding fails that job instead of aborting `bazel test` and hiding
the unit tests' results. It uses the standalone Lint CLI from the Android SDK
`cmdline-tools` package, version-pinned to `23.0` (`sdkmanager --install
"cmdline-tools;23.0"`, or set `ANDROID_LINT` to the executable). The Bazel rule
walks the app's dependency graph, writes lint's project model for every module
(app, `core/*`, `features/*/{api,impl}`), and runs lint once with `-Werror`, so
any lint error or warning fails the target and the lint job. The unit declares
that SDK package in `platform/ci/units.mjs`, so `_android.yml` installs it only
for this job.

`lint.xml` is the shared config. It is deliberately **not** a baseline: every
finding must be fixed at the source. `tools:ignore`/`@Suppress` is allowed only
for a documented false positive, scoped to the narrowest element or module.

**detekt** is the second static-analysis gate, also a separate CI unit
(`android / android-detekt`). `detekt.yml` enables `NestedBlockDepth` (3),
`LongMethod`, `ComplexCondition`, `TooGenericExceptionCaught`, the `unused*`
rules and a focused set of formatting rules; findings are failures and there is
no baseline. `platform/android/detekt-rules` provides the first-party
`home-rules` plug-in (`NoElseIf`, `NoTry`, `LongLambda`, `NoInlineJsonInTests`,
`NoHardcodedUiText`), loaded with `--plugins` by the Bazel rule in
`platform/android/detekt.bzl`. `NoTry` forbids `try`/`catch`/`finally`
entirely: use `runCatching`, `suspendRunCatching` (rethrows
`CancellationException`) or `use { }`. Test mock responses live in
`src/test/resources/fixtures/*.json` and are read through
`dev.proflyder.home.core.testing.TestFixtures`.

**Zero-warning policy.** `bazelisk build --config=android //projects/mobile/android:app`
and `bazelisk test --config=android //projects/mobile/android:android_tests` must
not print
`warning:`/`WARNING:` from this repository's code, resources, manifests or
Bazel rules. Fix the root cause; do not silence the log. The one exception is
javac warnings from pinned third-party build tools compiled in the exec
configuration (`external/protobuf+`, `external/rules_android+` `[for tool]`
libraries): those are not ours to fix and
`platform/android/android.bazelrc` passes
`--host_javacopt=-Xlint:none -nowarn` for that configuration only. First-party
code keeps `warn = "error"` and `android_lint -Werror`, so a new warning in our
sources still fails the build.

`android_tests` covers endpoint/security rules, time/duration parsing, Connect
JSON mapping and failures, idempotency, navigation queue/restoration, the real
Metro graph via a Robolectric cold launch, a Robolectric smoke of the real
profile row (`//projects/mobile/android/features/control/impl:profile_row_test`),
the Home hub model (live-row selection/order/cap and the 24 h reminder window,
"tomorrow 07:30" text, favorites
pin/order/defaults, display-name rule) in
`//projects/mobile/android/features/control/api:api_test`, the call state
machine (dialing → connecting → active → ended, local and remote hangup,
media/ICE failure, timeout, permission denial, busy/offline), the Connect
streaming `Watch` parser over a fake HTTP server, audio-route mapping, a
Robolectric smoke of the real bottom bar and the Time-tab deep link in
`//projects/mobile/android/app:main_activity_smoke_test`, and an APK packaging
gate that verifies required runtime classes are **defined** in DEX and that the
deploy jar has no unresolved non-platform references. The gate also self-tests
that a referenced-but-undefined class is rejected. Tests do not replace
interactive device acceptance.

`platform/android/sign-release_test.sh` self-tests release signing without any
real key: it creates a throwaway keystore with a strong password, takes a small
APK (an existing Bazel build or a minimal APK built with `aapt2`), signs it and
checks the certificate, v2/v3 signatures and the fail-closed behaviour. It runs
in the `android / android-app` CI unit and skips only when the Android
build-tools are missing.

## Signing

Two signatures exist and they play different roles.

rules_android can only sign a debug APK: it invokes the signer with
`--ks <file> --ks-pass pass:android` and no alias, so any key inside Bazel's
reach would need that fixed, weak password. The release key is therefore kept
**outside Bazel** and applied to the final artifact by
`platform/android/sign-release.sh`; the APK Bazel produces is an intermediate.

### Release signing (App Distribution)

`platform/android/sign-release.sh <in.apk> <out.apk>` aligns the APK with
`zipalign -P 16 -f 4` (16 KiB pages), signs it with `apksigner` (v2 and v3),
then verifies the output and **fails closed** unless the signer certificate
SHA-256 equals the expected value. A missing or different expectation is an
error, so an APK signed by another key can never be published.

| Variable | Meaning |
| --- | --- |
| `SIGNING_KEYSTORE` | PKCS12 keystore file, outside Git (required) |
| `SIGNING_STORE_PASSWORD_FILE` / `SIGNING_STORE_PASSWORD` | keystore password (prefer the file) |
| `SIGNING_KEY_PASSWORD_FILE` / `SIGNING_KEY_PASSWORD` | key password; defaults to the store password (PKCS12 uses one password) |
| `SIGNING_KEY_ALIAS` | key alias; when unset, the only alias in the keystore is used, and several aliases are refused |
| `SIGNING_CERT_SHA256` | expected signer certificate digest, hex, colons optional (required) |
| `ANDROID_HOME` / `ANDROID_BUILD_TOOLS` | SDK root / build-tools (default `$ANDROID_HOME/build-tools/36.0.0`) |

Passwords reach `apksigner` and `keytool` only as `file:` references to private
temporary files. They are never in argv, never exported to later steps and
never logged.

The key lives only in OpenBao `ci/android-distribute`. Locally
`mise run admin:android-distribute` passes it through fnox (profile `android`):
the base64 keystore, password files, alias, expected certificate and the
Firebase service account arrive as private temp files / variables
(`SIGNING_KEYSTORE_B64_FILE` is decoded by `distribute.sh`) and are deleted when
the command exits. CI materialises the same values under `$RUNNER_TEMP`.
Fallback for manual work: `mise run secrets:file android/home-release.keystore`
(and `.store.password`, `.key.password`, `.alias`, `.cert.sha256`) into the
gitignored `secrets/android/`, which `distribute.sh` reads by default; remove
them with `mise run secrets:clean android` afterwards.

**One-time migration from the old debug-password key.** The certificate must
stay the same, otherwise every install needs an uninstall and loses its
encrypted token. Rewrap the existing key with a strong password instead of
generating a new one:

```sh
# 1. Choose a strong password and keep it out of shell history.
printf '%s' 'REPLACE-WITH-STRONG-PASSWORD' > secrets/android/home-release.store.password
cp secrets/android/home-release.store.password secrets/android/home-release.key.password
chmod 600 secrets/android/home-release.*.password

# 2. Rewrap the existing personal key (the old password only appears in a file).
printf 'android' > /tmp/old-debug.password
keytool -importkeystore \
  -srckeystore secrets/android/home-debug.keystore -srcstorepass:file /tmp/old-debug.password \
  -destkeystore secrets/android/home-release.keystore \
  -deststoretype PKCS12 -deststorepass:file secrets/android/home-release.store.password \
  -destkeypass:file secrets/android/home-release.key.password \
  -srcalias androiddebugkey -destalias home-release \
  -noprompt
rm -f /tmp/old-debug.password

# 3. Record the alias and the certificate digest.
printf 'home-release' > secrets/android/home-release.alias
keytool -list -v -keystore secrets/android/home-release.keystore \
  -storepass:file secrets/android/home-release.store.password \
  | sed -n 's/^[[:space:]]*SHA256: //p' | head -n 1 | tr -d ':' | tr '[:upper:]' '[:lower:]' \
  > secrets/android/home-release.cert.sha256

# 4. Self-test the whole path with a throwaway key.
platform/android/sign-release_test.sh
```

Then store the five values under `ci/android-distribute` in OpenBao (field names
in [SECRETS](../../../docs/SECRETS.md) and the OpenBao README), and delete the
local password files with `mise run secrets:clean android` when done. The old
`keystore_b64` / `keystore_password` fields are left untouched as a backup but
are no longer read by CI.

### Bazel debug signing (local only)

Local `bazel build --config=android //projects/mobile/android:app` still produces
an APK signed
with the personal key when `platform/android/keys/home-debug.keystore` exists,
so `adb install -r` keeps app data. That file is a **Bazel-only copy** of the
same key with the password `android` (the only password rules_android can
send). It is never distributed and never present in CI. Without it the genrule
falls back to the public `@rules_android` debug keystore and prints a warning.

```sh
keytool -importkeystore \
  -srckeystore secrets/android/home-release.keystore \
  -srcstorepass:file secrets/android/home-release.store.password \
  -destkeystore secrets/android/home-debug.keystore \
  -deststoretype PKCS12 -deststorepass android -destkeypass android \
  -srcalias home-release -destalias androiddebugkey -noprompt

mkdir -p platform/android/keys
ln -s "$PWD/secrets/android/home-debug.keystore" \
  platform/android/keys/home-debug.keystore
```

Verify the signer of any APK before installing or publishing it:

```sh
"$ANDROID_HOME/build-tools/36.0.0/apksigner" verify --print-certs <apk>
# Signer #1 certificate SHA-256 digest:
# 88ef5d61a365afbf15d290168d4ee7fd20a80be13ea4d4667ef1e521a2a504ad
```

That digest is the personal key's. Both the local Bazel APK and the
release-signed distribution APK carry it, so they update each other in place.
Builds without the key (public fallback) cannot update an existing install.

## Push and find phone

Two delivery shapes arrive over FCM from the home server (the `phone-push`
service, triggered by the `phone_find` / `phone_notify` MCP tools):

- **Find phone** — a data-only, high-priority message with
  `type=phone_find`. Nothing is drawn by the system; `HomeMessagingService`
  starts the `mediaPlayback` foreground service, which rings and vibrates.
  The ring uses `AudioAttributes.USAGE_ALARM`, raises the alarm stream volume
  to its maximum for the duration and restores it afterwards. The ringer mode is
  never changed: the alarm stream is already audible in silent and vibrate
  modes, and switching the mode to normal used to leave the phone loud after
  the ring. The request notification lives on a separate silent channel
  (`home_phone_find_v2`: no sound, no channel vibration, `bypassDnd`), because
  the `MediaPlayer` in the service is the only sound source; the old
  `home_phone_find` channel had its own alarm sound, so it is deleted on every
  start (channel settings are immutable). The notification uses
  `setOnlyAlertOnce` and never `setSilent`: `NotificationCompat.setSilent`
  puts the notification into a group with `GROUP_ALERT_SUMMARY`, and SystemUI
  then suppresses the full-screen intent (the ringing screen never opened after
  #335). The silent channel alone already keeps it quiet. A repeated start or refresh reuses the
  same player instead of layering a second melody. It opens the
  `FindPhoneActivity` over the lock screen through a full-screen intent, and
  stops — sound, vibration, notification and volume restore — on «Нашёл»,
  after 60 seconds, on notification dismiss or when the service is destroyed. A
  high-priority FCM message is exactly
  the case Android allows a background/killed app to start a foreground
  service from, so the behaviour is the same with the screen off, the app in
  the background and the app killed.
- **Notification** — `phone_notify` or a `notification` block: title, body and
  an optional `home://…` deep link on the ordinary `home_notifications`
  channel. The tap opens the app; a deep link naming a feature id selects that
  tab, anything else just opens the current tab. Every label and button is a
  string resource; the server only supplies the text.

**Device identity and registration.** The server keys a phone by the app's
stable `deviceId`, not by the FCM token, because Google rotates tokens while
the phone stays the same. The identity is a SHA-256 digest of the app-scoped
`Settings.Secure.ANDROID_ID` (salted with the app package); the raw platform id
never leaves the phone. It survives FCM token rotation and app reinstall
without a factory reset. If a device reports no `ANDROID_ID`, the app generates
a random UUID once and persists it in DataStore. A factory reset produces a new
identity, which is correct: the old installation is gone.

The app registers on the scheduler base URL and bearer it already uses
(`POST /v1/push/devices`), so no fourth endpoint and no second credential
exist. A new token updates the row of the same `deviceId`; the scheduler keeps
one token per device. Registration runs at start and again on every token
change (`onRegistered`/`onNewToken`); a failure is only shown on the settings
screen and retried on the next start. `/v1/push/devices/remove` forgets a token
FCM rejected. Without `google-services.json` (Firebase uninitialized) or
without a configured server, registration is skipped and the app starts
normally.

**Permissions screen.** «Уведомления и поиск телефона» (top app bar ⚙ →
Settings) shows the real state of the three permissions the loud path needs and
opens the matching system page for each:

| Permission | Why | Where it is granted |
| --- | --- | --- |
| `POST_NOTIFICATIONS` (13+) | draw any notification | runtime dialog / app notification settings |
| notification-policy access | `bypassDnd` actually overrides Do Not Disturb | `ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS` |
| full-screen intent (14+) | the ringing screen opens over the lock screen | `ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT` |

The screen also shows whether this phone is registered on the server (only the
last six characters of the token are displayed), offers «Обновить
регистрацию», and has a **phone name** field. The name defaults to the phone
model; the owner can set a family name such as «Стас». Saving the name calls
`POST /v1/push/devices/rename`, so the server updates the existing row without
re-registering the token, and the name is persisted in DataStore. Ricardo's
`phone_find` / `phone_notify` / `phone_list` tools match that name, including
Russian cases and diminutives, when several phones are registered.
`USE_FULL_SCREEN_INTENT`, `ACCESS_NOTIFICATION_POLICY`, `VIBRATE` and
`FOREGROUND_SERVICE_MEDIA_PLAYBACK` are declared by the push module's manifest;
`POST_NOTIFICATIONS` and `MODIFY_AUDIO_SETTINGS` by the app. A Robolectric
screenshot of the screen in the dark theme is written to the
`settings_screenshot_test` test outputs (`push-settings-dark.png`); the
device-identity and name-persistence contract runs in `identity_test`. The ring
contract — silent and vibrate modes survive start/stop, the alarm volume is
restored, a repeated start reuses one player, and the find channel is silent
with the legacy channel deleted — is pinned by
`//projects/mobile/android/features/push/impl:audio_test` and `:channels_test`.

## Notification history

Every FCM message the phone receives is also written to a local log, so the
owner can see what Ricardo sent or asked even after the tray notification is
gone. Both shapes are recorded: `phone_notify` (server title, body and validated
`home://` deep link) and `phone_find` (the localized find-phone title and the
reason the ringing screen shows; no deep link). Nothing is fetched from the
server: the history is what reached this phone, and it works offline.

- **Storage** — `NotificationHistoryStore` (`features/push/api`, pure rules) is
  backed by the app's single DataStore file
  (`DataStoreNotificationHistoryStore`, key `push_notification_history`,
  JSON). Writes read, prepend and trim inside one `edit` transaction, so the FCM
  service cannot overwrite entries recorded before any screen loaded the log.
  Retention is the newest **200** messages; a repeated id replaces its copy.
- **Writing** — `HomeMessagingService` records off the main thread through
  `NotificationHistoryRecorder` after handing the message to the ring or the
  notification; a storage failure never blocks the ring.
- **Screen** — **Settings → Notification history**: newest first, an icon per
  type (`NOTIFY` bell, `FIND` phone), title, up to two body lines, a relative
  time from `DateUtils` in the device locale, and a chevron only when the row has
  a deep link. A tap follows the message route through `Navigator.openRoute`
  (the same path a notification tap uses); an unregistered route is ignored.
  «Очистить» asks for confirmation, then removes the whole log. The empty state
  explains what will appear.
- **No secrets** — the log holds decoded message text only: no FCM token, no
  server address. `allowBackup` is off, so it stays on the device.

Tests: `//projects/mobile/android/features/push/api:api_test`
(`NotificationHistoryRules`) and
`//projects/mobile/android/features/push/impl:history_test` (real DataStore:
order, restart, unloaded writer, 200-entry bound, clear, message mapping). The
settings list, the history list and its empty state are rendered in the dark
theme by `//projects/mobile/android/features/settings/impl:screens_screenshot_test`
(`settings-hub-dark.png`, `notification-history-dark.png`,
`notification-history-empty-dark.png`).

## Firebase (Crashlytics and Remote Config)

Firebase provides crash reporting, Remote Config and Cloud Messaging (FCM).
There is no Analytics or phone-side scheduling. Remote Config supplies only
connection settings; it never supplies the bearer token.

**What is sent to Google:** Crashlytics sends crash stack traces, device model
and OS version, and the app version. FCM carries no Home credentials: the
registration token is sent to the *home server*, and Google receives only the
notification/data payload the home server sends (a find-phone request or a
title/body). Remote Config requests carry the app's
Firebase installation id and app metadata. The app does not send the bearer
token, Home catalog contents, or any custom keys; the scheduler/control
addresses arrive in the Remote Config response and are not reported back.
Firebase uses its own installation id, not the app's Home credentials.

### Remote Config connection document

The app reads one string parameter, `home_connection`. Its in-app default is
empty, which means "not configured". The value is a JSON object; unknown keys
are ignored so a future field does not break older clients.

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `schedulerBaseUrl` | string | yes | Scheduler base URL, no RPC path. |
| `controlBaseUrl` | string | no | Home control base URL; absent/empty disables the Home tab. |
| `intercomBaseUrl` | string | no | Intercom signaling base URL; absent/empty disables calls. |
| `connectTimeoutMs` | number | no | Connect timeout, default 5000, valid 1000–120000. |
| `readTimeoutMs` | number | no | Read timeout, default 20000, valid 1000–120000. |

Example value (illustrative only; use your own private deployment values):

```json
{
  "schedulerBaseUrl": "https://alarms.example.test",
  "controlBaseUrl": "http://192.0.2.28:8099",
  "intercomBaseUrl": "https://intercom.home.example.test",
  "connectTimeoutMs": 5000,
  "readTimeoutMs": 20000
}
```

Malformed JSON, a missing `schedulerBaseUrl`, an invalid URL, or an
out-of-range timeout produces an explicit invalid state on the Connection
screen; nothing crashes and no request is attempted. The Connection screen
shows the loaded addresses/timeouts read-only with a **Refresh config** action.
The app fetches on start, activates pushed real-time updates, and keeps the last
loaded values usable if a later fetch fails.

### Build wiring without Gradle

`google-services.json` is private and lives in
`secrets/android/`; Bazel reads it through the gitignored symlink
`platform/android/keys/google-services.json`:

```sh
mkdir -p platform/android/keys
ln -s "$PWD/secrets/android/google-services.json" \
  platform/android/keys/google-services.json
```

`platform/android/tools/google_services` (Kotlin) converts the JSON client for
`dev.proflyder.home` into `res/values/google_services.xml` (`google_app_id`,
`gcm_defaultSenderId`, `google_api_key`, `google_crash_reporting_api_key`,
`google_storage_bucket`, `project_id`) and the Crashlytics build id
`com.google.firebase.crashlytics.mapping_file_id` (32 hex, derived from the app
id and version). Without the link the build still succeeds, prints a warning,
emits an empty values file, and Firebase stays uninitialized; the app must and
does start normally in that state.

Collection follows the `home_crash_reporting` bool (default `true`). Turn it
off to keep Crashlytics initialized without uploads.

**One-time console steps (Crashlytics):**

1. In the Firebase console for the existing project, open **Crashlytics** and
   finish onboarding (the Android app `dev.proflyder.home` already exists in
   the project).
2. Wait for the first crash to appear; Crashlytics enables itself when the
   first report arrives.

**One-time console step (Remote Config):**

1. Open **Remote Config**, create a parameter named `home_connection` of type
   **String**, keep the in-app default empty, set the deployment value to the
   JSON document above (with private hostnames), and click **Publish**. The four
   values are deployment-specific: set them only in the console, never in the
   repository or in committed examples.

**Verify a report manually.** On a connected debug device, crash the app
process and relaunch it:

```sh
adb shell am crash dev.proflyder.home
adb shell monkey -p dev.proflyder.home 1   # or tap the launcher icon
```

`am crash` throws a `RuntimeException` in the app process, which is exactly the
uncaught exception Crashlytics records. If the device blocks `am crash`, use a
temporary debug-only crash instead and remove it before committing. Reports
usually appear in the console within a few minutes after the next launch.

## App Distribution

`projects/mobile/android/distribute.sh` builds, verifies, and uploads the APK
to Firebase App Distribution. It runs both locally and from CI.

CI runs on **every pull request that changes the Android app**, in two steps so
the APK is built exactly once:

1. `.github/workflows/_android.yml`, matrix entry `android / android-app`
   (started by `.github/workflows/ci.yml` when the router sets
   `android_distribute`: an actual `projects/mobile/android/`,
   `packages/kotlin/` or `platform/android/` change) builds the APK without any
   secret or Environment (fork PRs included):
   - takes `google-services.json` from the repository variable
     `GOOGLE_SERVICES_JSON_B64` (not a secret: the Firebase identifiers ship in
     every APK); without it the app builds with Firebase disabled and no
     release artifact is produced;
   - builds `//projects/mobile/android:app` with the release
     `versionCode`/`versionName`, runs `bazel test` with the same `--define`
     values so the APK-level tests inspect exactly the published APK, and
     uploads it unsigned as `android-app-apk` (`app.apk` plus
     `release-info.json` with the Firebase app id, version and SHA-256).

2. `.github/workflows/ci.yml` job `android-distribute` (Environment `Android`,
   OpenBao JWT role `android-distribute`) waits for the whole `android` matrix
   (so an `android-lint` finding also skips distribution), downloads that
   artifact, signs it with `platform/android/sign-release.sh` (release key and
   password files under `$RUNNER_TEMP/ci`, mode 600), updates
   `release-info.json` for the signed bytes and uploads it with
   `distribute.sh --upload-only`. It is deliberately
   **not** part of `ci-ok`: a failed upload is visible on the PR and in the run,
   but it does not block the merge. A newer push to the same PR cancels the
   upload in flight. A global invalidation such as `MODULE.bazel` still runs the
   `android` checks but does not publish a build.

`--ci` formats release notes as `PR #<number>: <title>`, the short head SHA and
the workflow run URL (number and title arrive as environment variables, never
as script source). The signing material, the certificate check and the
`admins` tester group are identical to a local run.

### Retention

Every uploaded build stays in App Distribution, so old PR builds are removed by
the daily `fad` job of `.github/workflows/cleanup.yml` (schedule plus a manual
`workflow_dispatch`, `dry_run` on by default). The window is **24 hours per
app**: releases older than that are deleted, the newest release is always kept
(that is the build testers install), and a release without a parseable
`createTime` is kept. It uses `projects.apps.releases.list` (paginated,
`orderBy=createTime desc`) and `releases:batchDelete` with the same service
account as the upload, exchanged for an OAuth token with the `cloud-platform`
scope; the app and project ids come from the same repository variable
(`GOOGLE_SERVICES_JSON_B64`) the APK build uses.

The cleanup reads `secret/ci/android-distribute` through the `Android`
Environment and the OpenBao role `android-cleanup` (bound to `cleanup.yml` on
`develop`, events `schedule`/`workflow_dispatch`; the PR-only
`android-distribute` role is not reused). The service account needs the
**Firebase App Distribution Admin** role (`roles/firebaseappdistro.admin`) in
the Firebase project to delete; without it the job fails with that hint and
deletes nothing.

```sh
# local selection dry run on a fixture: no credentials, no network
node platform/ci/fad_cleanup.mjs \
  --fixture platform/ci/testdata/fad-releases.json --now 2026-11-04T12:00:00Z
```

Local:

```sh
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358"  # optional under mise
mise run admin:android-distribute -- --dry-run   # print the command, no auth
mise run admin:android-distribute                # build, sign + upload
```

The task passes the release signing material from OpenBao (see
[Signing](#signing)); the script also needs `google-services.json`,
and refuses to upload unless `sign-release.sh` verified the personal
certificate `88ef5d61…04ad`. It reads the Firebase app id from the JSON.
Authentication uses an existing `npx firebase-tools@15.34.2 login` session or
`GOOGLE_APPLICATION_CREDENTIALS` pointing at a service-account JSON under
`secrets/android/`; credentials are never printed.

Each upload gets `versionCode` = UTC epoch seconds and
`versionName` = `0.1.0-<shortsha>`. Epoch seconds are strictly increasing per
build and fit a 32-bit versionCode until 2038; a git commit count is not
monotonic for dirty builds. Ordinary builds keep versionCode 1 / 0.1.0. Both
are build settings (`--define home_version_code=… --define
home_version_name=…`), defaulted in `platform/android/android.bazelrc` (active
with `--config=android`), so manual uploads can pass the same values.

**One-time console steps:**

1. Open **App Distribution** in the Firebase console and choose **Get
   started**.
2. Create the tester group `admins` (display name «Admins») (or set `HOME_TESTERS_GROUP`) and add your
   own email address.
3. Run `npx firebase-tools@15.34.2 login` once on a local machine that
   distributes outside CI.
4. For CI, create a GCP service account with the **Firebase App Distribution
   Admin** role and a JSON key, then store it (with the release keystore) in
   OpenBao `secret/ci/android-distribute` — see
   [OpenBao](../../home/kubernetes/openbao/README.md). `google-services.json`
   is not part of that record: CI takes it from the repository variable
   `GOOGLE_SERVICES_JSON_B64`.

## Architecture

```text
projects/mobile/android/
  app/                    single Activity, app graph, tabs, Time container, diagnostics
  core/
    model/                Android-free scheduler domain + server clock
    config/               remote connection contract/JSON parser + redaction
    remoteconfig/         Firebase Remote Config endpoint/timeout source
    settings/             DataStore + Keystore token + legacy migration
    network/              Retrofit/OkHttp Connect + control catalog + errors
    navigation/           typed destinations, FeatureEntry, FragmentRouter
    ui/                   base Fragment/ViewModel, ViewBinding, list states
  features/
    alarms/{api,impl}     domain contract / use cases + repository + UI
    reminders/{api,impl}
    timers/{api,impl}
    call/{api,impl}       intercom state machine, WebRTC, Telecom, call screen
    push/{api,impl}       FCM service, find-phone ring, permission screen
    control/{api,impl}    Home catalog, filters, scenario/device/profile actions
    scenarios/{api,impl}  scenario list/editor, draft codec and validation
    connection/{api,impl}
packages/kotlin/core/     shared Kotlin utilities (endpoint/time/nav state)
platform/android/         Bazel macros, Metro wiring, APK gate, ViewBinding gen
```

Feature implementations depend only on their own API, core modules, and shared
Kotlin utilities; no feature impl imports another feature impl. The app module
is the composition root and the only module that sees every implementation.

### DI

Metro's compiler plugin is applied to every Kotlin Android compilation through
`//platform/android:defs.bzl`. The graph in `app/.../di/AppGraph.kt` is generated
at compile time: repositories use `@ContributesBinding`, ViewModel factories and
navigation entries use `@ContributesIntoSet`, and `AppBindings` provides the
OkHttp/Retrofit clients, `ServerClock`, the single `DataStore`, and
`FeatureRegistry`. There is no runtime service locator or hand-written mock
repository.

### Server calls

Implemented scheduler RPCs: `GetStatus`, `GetSchedule`, `ListSchedules`,
`CreateSchedule`, `UpdateSchedule`, `DeleteSchedule`, `SetScheduleEnabled`,
`SnoozeSchedule`, `StopSchedule`, `StartTimer`, `ListTimers`, `PauseTimer`,
`ResumeTimer`, `CancelTimer`, and `EnqueueTask` (Home control only).
Scenario definitions use `home.v1.ScenarioService`: `ListScenarios`,
`GetScenario`, `CreateScenario`, `UpdateScenario`, `DeleteScenario`,
`SetScenarioEnabled`, and `RunScenario`. Calls use
`home.v1.IntercomService` (unary RPCs plus the server-streaming `Watch`). Home
catalog reads use `GET /v1/catalog`.

## App identity and permissions

- Application ID `dev.proflyder.home`; default version 0.1.0 (versionCode 1).
  `home_android_binary` reads `home_version_code`/`home_version_name` from
  `--define` (defaults in `platform/android/android.bazelrc`); `distribute.sh`
  overrides them for App Distribution.
- minSdk 26, target/compile SDK 36, Kotlin/JVM target 17.
- `INTERNET`; call permissions are `RECORD_AUDIO`, `MANAGE_OWN_CALLS`,
  `FOREGROUND_SERVICE_PHONE_CALL`, `BLUETOOTH_CONNECT`, `POST_NOTIFICATIONS`
  (see [Call permissions](#call-permissions)).
- Bazel output is debug-signed (personal key when present, public fallback
  otherwise); App Distribution APKs are release-signed by
  `sign-release.sh` with the same personal certificate. APKs are not
  byte-reproducible. See [Signing](#signing).
- Browsable diagnostics (logcat tag `HomeHttp` + Chucker) are gated by the
  `home_network_inspection` bool and should be disabled in release variants.

## Known limitations

- No offline cache and no generic task UI. Push delivery and the find-phone
  ring are implemented; their on-device acceptance is still pending.
- Scenario execution still depends on the server-side `ha-scenario` executor;
  the app only edits definitions and queues manual runs.
- Full interactive acceptance on a device (filters, scenario taps, device
  toggles, alarm/timer flows) is still pending; automated input is blocked by
  HyperOS.
- `EnqueueTask` is one-shot; recurring generic tasks and event-trigger rules are
  not implemented.

## Window insets and text fields

All screens are edge-to-edge (`enableEdgeToEdge()` in `MainActivity`,
`CallActivity`, `FindPhoneActivity`; manifest keeps `adjustResize`). One
mechanism in `core/ui` (`WindowInsets.kt`, no third-party library) applies
`systemBars()`, `displayCutout()` and `ime()`; no layout hard-codes status or
navigation bar sizes.

- `MainActivity`: the app bar takes the top inset, the bottom navigation takes
  the navigation-bar inset. The fragment container sits between them and its
  children receive insets without the status bar and, while the bottom
  navigation is shown, without the navigation bar and keyboard
  (`forContentBetweenBars`), so a screen never pads twice.
- While the keyboard is open the bottom navigation is hidden (GONE) on every
  screen. The keyboard covers it anyway; keeping it would lift editor panels by
  its height and leave an empty band between the panel and the keyboard. It
  returns when the keyboard closes.
- Scrolling lists and forms: `applyScrollBottomInset(extra)` — `clipToPadding
  = false` and bottom padding = initial + max(ime, navigation bar) + extra.
  Screens with a FAB pass `fabClearance()` so the last row stays above it;
  FABs, banners and the alarm selection bar use `liftAboveBottomInsets()`.
- Pinned bottom panels (the alarm editor **Save** bar): `pinAboveKeyboard(content)`
  sets the panel's bottom margin to max(ime, navigation bar), so it rests
  exactly on the keyboard with no gap; the content gets bottom padding = panel
  height + that inset. A `WindowInsetsAnimationCompat` callback translates the
  panel with the keyboard animation.
- Forms keep the focused field visible: `keepFocusedVisible()` scrolls a
  `NestedScrollView` so the focused field, together with its whole
  `TextInputLayout` (helper/error), is above the padding/panel.
- Bottom sheets use `ThemeOverlay.Home.BottomSheetDialog` (edge-to-edge,
  `adjustResize`); Material sheets pad themselves for the navigation bar.
  Alert dialogs are floating windows and are positioned by the system.

Text fields share `Widget.Home.TextInput` (also the theme's `textInputStyle`):
a Material 3 filled field on `colorSurfaceContainerHigh` with 12dp top corners
(`ui_text_input_radius`, never a capsule) and a square bottom edge. At rest a
1dp `colorOutlineVariant` underline marks it as editable; focused, the
underline becomes 2dp `colorStateOn` (primary in Current, cyan in Neon), and
the floating label, cursor and selection handles use the same accent.
Helper, error and counter text use `textAppearanceBodySmall`.

- Every field has a floating label from resources and the label is always
  enabled: the text sits under it, never pushed down by an empty label slot.
  A card or section title never repeats the label — the name field on
  «Phone notifications and find» and the token field on «Connection» carry
  their card's name and icon (`startIconDrawable`) themselves.
- Examples go into `placeholderText`, not the label.
- Free-text names (alarm, reminder, timer, scenario, phone name) get the
  clear-text end icon; the token keeps the password toggle; the speak dialog
  keeps its 500-character counter.

Buttons inside settings cards follow one rule: the action the screen exists
for is the only filled button (**Save and test connection**, editor **Save**,
**Start** timer); other card actions are tonal (**Save name**, **Update
registration**, **Refresh config**, scenario **Add …**); per-row and reset
actions (permission **Allow**, **Remove stored token**, **Delete**) are text
buttons. A screen of independent settings with no single main action
(«Phone notifications and find») has no filled button, so its card actions
look the same.

The toolbar title never clips a letter: it autosizes from 24sp down to 16sp
and may wrap onto a second line at a word boundary (`breakStrategy=balanced`,
no hyphenation).

`//projects/mobile/android/app:window_insets_screenshot_test` runs the real
activity with status/navigation/keyboard insets in the Neon and Current themes:
the Save bar bottom equals window height minus the keyboard and does not cover
the name field, the bottom navigation is hidden with the keyboard, the last
Home row ends above the FAB and the bottom navigation, and the fields use the
shared style. The same test opens «Phone notifications and find» in Russian and checks the
toolbar title is not ellipsized, clipped or broken inside a word, and inflates
every layout with a field (12+) to check the shared style: 1dp/2dp strokes,
corner radius below half the field height, a non-empty label and text inside
the field's vertical bounds. Alarm editor, connection and push screens run in
Neon and Current, dark and light. It writes
`insets-<theme>-<dark|light>-{alarm-editor-keyboard,connection,push-settings}.png`
and `insets-<theme>-home-end.png`.

## Themes

Themes are code in `core/theme`:

- `AppTheme` is the registry: key, title/summary, `ThemeOverlay.Home.*` style
  and a `ThemePalette` (a `ColorScheme` with every Material 3 role for light
  and dark; no defaults, so a missing role does not compile). Bundled:
  **Current** (the original emerald palette, unchanged), **System (Material
  You)** and **Neon** (near-black violet night, neon violet primary, electric
  cyan secondary, magenta tertiary; selected pills, chips and the navigation
  indicator are cyan on a deep violet `secondaryContainer`; FABs get a 1dp
  cyan outline and, on Android 9+, a cyan elevation glow).
- FABs share one style from `core/ui`: `Theme.Home` sets
  `floatingActionButtonStyle` / `extendedFloatingActionButtonStyle` to
  `Widget.Home.(Extended)FloatingActionButton`, which read the theme attrs
  `homeFabStrokeWidth`, `homeFabStrokeColor`, `homeFabGlowColor` and
  `homeFabElevation` (Current/System: no outline; Neon: outline and glow on
  every FAB, including quick-action items and the close button). Speed-dial
  labels use `Widget.Home.FabLabel` (a `surfaceContainerHigh` pill).
  `FabStyleScreenshotTest` checks equal strokes and unclipped glow.
- No segmented buttons anywhere (`MaterialButtonToggleGroup`, bordered
  segments): a choice between a few options is `core/ui` `PillSegments`
  (Time sections, Appearance mode). `AndroidThemeColorTests` enforces it.
- `?attr/homeSelectedContentColor` (core/theme) is the text colour of a
  selected pill: `colorPrimary` in Current, cyan in Neon.
- On/off state has one colour everywhere: `?attr/colorStateOn` and
  `?attr/colorOnStateOn` (core/theme, `ColorRole.STATE_ON`/`ON_STATE_ON`) are
  `colorPrimary`/`colorOnPrimary` in Current and System and
  `colorSecondary`/`colorOnSecondary` in Neon (cyan `#00E5FF` at night, the
  AA-safe teal `#006874` by day). `?attr/colorStateOnText` is the status text
  colour; Current light uses the darker `home_state_on_text` because
  `colorPrimary` is only 4.43:1 on cards. In System with wallpaper colours the
  fill follows the wallpaper while the text stays the Current value.
- `Widget.Home.Switch` (core/theme) is the global `materialSwitchStyle` in
  `Theme.Home` and in every theme overlay: checked track `colorStateOn`, thumb
  `colorOnStateOn`; unchecked track `colorSurfaceContainerHighest`, thumb
  `colorOnSurfaceVariant`, outline `colorOutline`.
- `core/ui` `DeviceStateViews` styles every device icon, status and card
  (Home device cards, group rows, station mini-tiles, volume rows): on is an
  accent-filled icon with `colorOnStateOn` glyph, bold accent status and a
  1dp accent card outline; off is `colorSurfaceContainerHighest` with
  `colorOnSurfaceVariant` and regular weight. The fill change animates for
  `HomeMotion.selectionDuration`. Buttons keep the neutral container look via
  `clearIcon`. `ThemeRegistryTest` checks AA for the on-state pairs;
  `DeviceStateScreenshotTest` renders on/off cards in every theme and mode,
  checks that fill and status differ, that the checked switch track equals
  the tile fill and that a titleless group row lines up with a device card.
- Applying: the View system takes colours from theme attributes, so each
  theme is a static `ThemeOverlay` applied with `Theme.applyStyle` in
  `onActivityPreCreated` (`onActivityCreated` below Android 10), before any
  view is inflated. Material You applies the Current overlay and then
  `DynamicColors.applyToActivityIfAvailable` in the same callback, so the
  wallpaper colours win on Android 12+ and older systems keep Current. A
  content-based `DynamicColorsOptions` seed was rejected: it regenerates the
  palette and cannot keep exact colours.
- The choice (`appearance_theme`, `appearance_mode`) lives in the app
  DataStore. `HomeApplication` reads it once before the first activity and sets
  `AppCompatDelegate` night mode, so the first frame is already themed. A theme
  change recreates the activity; a mode change is applied by AppCompat.
- Adding a theme: one `AppTheme` entry, its palette in `Palettes`, the
  `@color/theme_<key>_*` resources (values + values-night) and the overlay.
  Screens and the Appearance list need no changes.
- Screens use only theme attributes (`?attr/colorPrimary`,
  `MaterialColors.getColor`). `platform/quality/test_static.py`
  (`AndroidThemeColorTests`) rejects literal or palette colours in layouts,
  drawables, colour selectors and styles, and `Color.parseColor`/`rgb` in
  Kotlin. Push notifications use the chosen theme's primary as accent.
- Tests: `//projects/mobile/android/core/theme:theme_test` (every role opaque,
  WCAG AA for on-X/X pairs and text on surfaces, overlays resolve to exactly
  the palette data in light and night); `//projects/mobile/android/app:theme_screenshot_test`
  writes `theme-<key>-{home,time,appearance}-dark.png` and requires the
  theme's surface to fill the frame.

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `core/ui/.../OutlinedFloatingActionButton.kt`
  - Material `FloatingActionButton` has no stroke attributes (only `ExtendedFloatingActionButton` does), so this subclass reads `strokeWidth`/`strokeColor` from its style and paints them over the content rect with a `MaterialShapeDrawable` of the same shape. Every plain FAB in layouts uses it so the theme style applies one outline everywhere.
- Device rows (`item_control_device.xml`, `item_control_device_row.xml`) share `control_device_icon*` and `control_device_text_gap`: the single card padding (16dp) equals group inset + row bleed (8dp + 8dp), so icons and titles line up across card kinds.

- `BUILD.bazel`
  - Unit tests only. `//projects/mobile/android:android_lint` is intentionally absent: it is a separate CI unit so its findings do not hide test results.
- `app/src/main/AndroidManifest.xml`
  - WebRTC: NetworkMonitor aborts natively without network-state access. Push / Find phone (features/push/impl): library permissions are dropped by rules_android, so the app declares them.
- `app/src/main/kotlin/dev/proflyder/home/di/AndroidNetworkWaiter.kt`
  - Waits for a default network (Wi-Fi, LAN or the VPN tunnel) while the phone wakes up after Doze. A resume can race the tunnel coming back, so the first read should wait instead of immediately showing "check LAN or VPN".
- `app/src/main/kotlin/dev/proflyder/home/di/AppGraph.kt`
  - A bearer token must never be forwarded to a redirect target.
- `app/src/main/kotlin/dev/proflyder/home/di/NetworkInterceptors.kt`
  - Network diagnostics, enabled by the `home_network_inspection` bool (a release variant should override it to false so nothing below is created): - an [HttpLoggingInterceptor] (BODY) writing to logcat under "HomeHttp", - a Chucker inspector (notification + …
- `app/src/main/res/layout/app_toolbar_menu.xml` — The toolbar's custom actions menu: a rounded tonal card with one icon and label per row. It replaces the framework overflow list entirely.
- `app/src/main/res/menu/menu_main.xml` — One visible actions button. Tapping it opens the app's own rounded menu (app_toolbar_menu.xml); the framework overflow list is never used.
- `app/src/main/res/values/bools.xml`
  - Crashlytics collection. The app only sends crash stack traces, device model/OS and app version; never tokens or endpoints. Set to false to keep Crashlytics initialized but not collecting.
- `app/src/main/res/xml/backup_rules.xml` — Backup and device transfer are disabled for the whole app: the bearer token and server address are private configuration.
- `app/src/main/res/xml/network_security_config.xml` — The client talks to a user-supplied LAN address or a home HTTPS name reached over the VPN. HTTP is allowed only because users may run the server without TLS on their own network; HTTPS is recommended and cleartext is never enabled for public hosts by this app. See README "Transport security".
  - Cleartext is a documented private-LAN trade-off; see README "Transport security".
- `app/src/test/kotlin/dev/proflyder/home/ConnectionSettingsClearTest.kt`
  - A subsequent request must fail with a controlled configuration error and must not reach OkHttp/server.
- `app/src/test/kotlin/dev/proflyder/home/HomeFabActivityTest.kt`
  - First pre-draw positions the clone FAB and gives every row its start state; it must not paint the expanded state in the meantime.
- `app/src/test/kotlin/dev/proflyder/home/MainActivitySmokeTest.kt`
  - A single actions button: the framework overflow list must never exist.
- `core/config/src/main/kotlin/dev/proflyder/home/core/config/ConnectionConfig.kt`
  - Request readiness. The scheduler is never authless: an absent or unsafe token is a controlled validation failure before any HTTP request.
  - Persists only the bearer token (`null` clears it); the token is never written in plaintext. Endpoints and timeouts are owned by Remote Config.
- `core/config/src/main/kotlin/dev/proflyder/home/core/config/RemoteConnectionSource.kt`
  - Endpoint and timeout state supplied by Firebase Remote Config. The bearer token is deliberately absent: it is a device-local secret and is never read from or written to Remote Config.
  - Parses the single `home_connection` Remote Config string. Schema: ```json { "schedulerBaseUrl": "https://alarms.example.test", "controlBaseUrl": "https://control.example.test", "intercomBaseUrl": "https://intercom.example.test", "connectTimeoutMs": 5000, …
- `core/model/src/main/kotlin/dev/proflyder/home/core/model/CatalogGrouping.kt`
  - Pure grouping of the control catalog for the scenario pickers. - The device name is the group title; when a device is missing, a meaningful entry title is used, otherwise a localized "Scenarios HA"/"Other" kind fallback. Raw entity ids are never exposed.
- `core/model/src/main/kotlin/dev/proflyder/home/core/model/ControlModels.kt` — What kind of thing a catalog entry controls. Unknown server kinds map to [OTHER] and are not shown.
  - Live station volume: [level] is 0–100 and [muted] is the media player mute. Announcement volume reports the number entity and is never muted.
  - Stable localization key of a device status shown under the title (for example `catalog.status.locked` for the PC). Empty when there is no status; the server never sends display text here.
  - Client-side merge of a device's announcement volume into its main volume row. Never sent by the server and absent from the companion.
- `core/model/src/main/kotlin/dev/proflyder/home/core/model/Favorites.kt`
  - Remembers the pinned shortcuts. A null [load] means "never saved yet", so an intentionally emptied list is not replaced by the defaults again.
- `core/model/src/main/kotlin/dev/proflyder/home/core/model/RemoteFailureExt.kt` — A transport failure that another attempt can fix without user action: the server was unreachable, the wake-up after Doze reset a keep-alive socket, or a request timed out. Such a failure must not become a blocking error notice right after the app returns to the foreground.
- `core/model/src/main/kotlin/dev/proflyder/home/core/model/ScenarioModels.kt` — Domain models mirroring the home.v1 ScenarioService contract.
- `core/model/src/main/kotlin/dev/proflyder/home/core/model/SchedulerModels.kt`
  - True when the request may have reached the server even though no success response was observed. Retrying such a create/start must reuse its idempotency key. Validation/auth/configuration failures were definitely not applied and may start a new operation.
- `core/model/src/main/kotlin/dev/proflyder/home/core/model/ServerClock.kt` — Maps the phone onto the scheduler's clock.
- `core/model/src/test/kotlin/dev/proflyder/home/core/model/ServerClockTest.kt`
  - User/NTP moves the phone clock forward by one hour; elapsed time has not advanced, so the server-derived countdown must not jump.
  - Moving the wall clock backward must not rewind either.
  - Wall-clock jump by an hour must not expire the timer.
- `core/navigation/src/main/kotlin/dev/proflyder/home/core/navigation/Destination.kt`
  - A typed navigation target. Feature modules define concrete data classes (for example `AlarmEditorDestination(id)`) and the router resolves them through [FeatureEntry]. Only stable identifiers belong in [arguments];
- `core/navigation/src/main/kotlin/dev/proflyder/home/core/navigation/FragmentRouter.kt`
  - Small typed Fragment router. - FragmentManager owns the real back stack and restores fragments after process death. The restored fragment/back stack is authoritative; saved route strings are validated against it instead of recreating it blindly.
  - No actual restored fragment. A saved bundle alone must not leave the container blank: rebuild the selected tab root.
  - Do not silently consume Back while the host cannot run a transaction: queue it and drain after resume. At the real root return false so the Activity can finish instead of swallowing future back presses forever.
  - An unknown/stale fragment must not be treated as route truth.
  - Route state and FragmentManager diverged. Re-sync once and consume this Back, but do not trap the user at a phantom inner route.
  - Overlay on the current tab: do not change the selected bottom tab and add a real back-stack entry so Back returns to where the user opened it from.
- `core/network/src/main/kotlin/dev/proflyder/home/core/network/ConnectIntercomClient.kt`
  - Opens the long-lived server-streaming `Watch` RPC and reconnects with a bounded backoff when it drops. The flow never completes on its own; the caller cancels it. The first connection marks this endpoint online.
  - Connect JSON client for home.v1.IntercomService. Unary RPCs use `application/json`; `Watch` is server streaming and therefore uses the Connect streaming envelope framing over HTTP/1.1 (`application/connect+json`): a 1-byte flags word, a 4-byte big-endian …
  - No diagnostic interceptors here: HttpLoggingInterceptor(BODY) and Chucker read the whole response before returning it, and Watch never ends — the phone would never see RINGING -> CONNECTING and never send its offer.
- `core/network/src/main/kotlin/dev/proflyder/home/core/network/ConnectScenarioClient.kt`
  - Connect unary JSON/HTTP client for home.v1.ScenarioService. It uses the scheduler endpoint and bearer token, with the same security properties as [ConnectSchedulerClient]: redirects are refused (the token must never follow a Location), the token travels only …
- `core/network/src/main/kotlin/dev/proflyder/home/core/network/ConnectSchedulerClient.kt`
  - Connect unary JSON/HTTP client for home.v1.SchedulerService. Security properties: - redirects (HTTP and HTTPS) are disabled; a bearer token can never be forwarded to a Location host, - the token travels only in the Authorization header, never in the URL, - …
- `core/network/src/main/kotlin/dev/proflyder/home/core/network/ConnectionInterceptor.kt`
  - Internal request marker: a long-lived server stream (connect `Watch`) must not inherit the unary read timeout. The interceptor removes the header.
  - Applies the *current* user settings to every request, so the Retrofit interfaces only declare relative paths: - rewrites the URL onto the configured endpoint (including a path prefix), - adds the bearer token (header only, never in the URL) and fixed headers, …
  - Defensive clamping: corrupted persisted settings must never make OkHttp reject a timeout.
- `core/network/src/main/kotlin/dev/proflyder/home/core/network/ControlCatalogClient.kt`
  - Wire format of GET /v1/catalog and of the scheduler task that carries a command.
  - Task payload: a catalog id and an action only, never an HA service or entity.
  - Payload of a speak action. The text is user content: it travels only in the task payload and is never put in a URL, a log line or an error.
  - An unknown kind stays OTHER, so an older app never drives a command it does not understand.
  - Retrofit implementation ([ControlApi]). Same safety rules as [ConnectSchedulerClient]: no redirects (the bearer token must never follow a Location), token only in the Authorization header, system trust store.
- `core/network/src/main/kotlin/dev/proflyder/home/core/network/HttpSupport.kt`
  - Failure of a unary Connect call before a decoded response: transport errors keep their [toRemoteFailure] mapping, malformed JSON becomes a protocol failure, and anything else is a bug the caller should rethrow.
- `core/network/src/main/kotlin/dev/proflyder/home/core/network/OutcomeCallAdapterFactory.kt`
  - Lets an API method declare `Outcome<T>` instead of `Response<T>`. Success bodies are already decoded by the kotlinx converter. Anything else becomes one of the existing [RemoteFailure] values: Connect error envelopes (including their `LocalizedText` detail) …
- `core/network/src/main/kotlin/dev/proflyder/home/core/network/PushDevices.kt`
  - Wire format of `/v1/push/devices`.
- `core/network/src/main/kotlin/dev/proflyder/home/core/network/ReadRetry.kt`
  - Platform network state. The JVM build (and every unit test) is always online; Android supplies an implementation that actually waits for Wi-Fi or the VPN. [awaitAvailable] blocks on the calling OkHttp thread, never on the UI thread.
  - Retries idempotent reads (`GET`, Connect `List*`/`Get*` RPCs) after a transport failure. This is what hides the red "check LAN or VPN" alert after the phone wakes up: the first attempt hits a dead keep-alive socket or a still-sleeping tunnel, the next attempt …
  - A cancelled call is not an error: the coroutine requested it. OkHttp reports cancellation as `IOException("Canceled")`, which must never reach the user as a network failure.
- `core/network/src/main/kotlin/dev/proflyder/home/core/network/Retrofits.kt`
  - Builds the Retrofit instances. Each gets its own OkHttpClient derived from [baseClient] (shared connection pool and dispatcher) with: 1. [ReadRetryInterceptor] (retry idempotent reads after wake-up failures), 2.
- `core/network/src/main/kotlin/dev/proflyder/home/core/network/TokenRefreshAuthenticator.kt`
  - Refreshes the bearer token once after a 401 pushed from a rotated token. ConnectionInterceptor is an application interceptor and does not rerun for an authenticator follow-up, so this class sets the header itself from the current settings.
- `core/network/src/main/kotlin/dev/proflyder/home/core/network/WireDtos.kt`
  - Wire DTOs of the Connect/JSON APIs. Protobuf JSON sends `int64` as a decimal string, so every such field uses [Int64StringSerializer]; it also accepts a plain number because servers differ slightly in codec versions.
- `core/network/src/test/kotlin/dev/proflyder/home/core/network/ControlCatalogClientTest.kt`
  - Normalization happens before this point; the codec must not drop or escape Cyrillic text.
- `core/network/src/test/kotlin/dev/proflyder/home/core/network/ScenarioJsonTest.kt`
  - Saving the same scenario again must not drop the retained objects.
- `core/remoteconfig/src/main/kotlin/dev/proflyder/home/core/remoteconfig/FirebaseRemoteConnectionSource.kt`
  - Firebase Remote Config implementation of [RemoteConnectionSource]. Defaults are set locally, the latest activated values are fetched on start, and a real-time config listener activates pushed changes.
- `core/settings/src/main/kotlin/dev/proflyder/home/core/settings/AndroidConnectionSettingsStore.kt`
  - Stores the bearer token as Keystore-encrypted ciphertext and composes it with the endpoints/timeouts supplied by [RemoteConnectionSource]. The manifest disables Android backup, so the token never leaves the device.
- `core/testing/src/main/kotlin/dev/proflyder/home/core/testing/TestFixtures.kt` — Loads a JSON payload from the test classpath (`src/test/resources/fixtures`).
- `core/ui/src/main/kotlin/dev/proflyder/home/core/ui/AppForeground.kt` — When the app last returned to the foreground. Waking the phone races the Wi-Fi/VPN tunnel: a transient network failure in the first seconds after a resume is expected and must not become a blocking error notice.
- `core/ui/src/main/kotlin/dev/proflyder/home/core/ui/BaseFragment.kt`
  - Base for screens backed by a generated ViewBinding. The binding exists strictly between [onCreateView] and [onDestroyView]; it is released there so the Fragment never leaks its view hierarchy.
- `core/ui/src/main/kotlin/dev/proflyder/home/core/ui/BaseListAdapter.kt`
  - A typed DiffUtil change payload: the set of [A] aspects that changed on one row. A `getChangePayload` returning it makes the list animator reuse the existing holder (no crossfade) and lets [BaseViewHolder.bindPayloads] update only the affected views.
- `core/ui/src/main/kotlin/dev/proflyder/home/core/ui/Motion.kt`
  - Clear the pending end action first: cancelling must not run the previous exit's finalizer, so an interrupted exit keeps its current translation and alpha and the new animation continues from there.
- `core/ui/src/main/kotlin/dev/proflyder/home/core/ui/Notices.kt`
  - The single way the app shows transient feedback: a rounded tonal pill with a status icon, floating above the view tagged [R.id.home_notice_anchor] so it never covers navigation. A sideways or downward swipe dismisses it; a tap never does.
  - The activity root is not a CoordinatorLayout, so Material never installs a SwipeDismissBehavior: the notice must own the gesture.
- `core/ui/src/main/kotlin/dev/proflyder/home/core/ui/PillSegments.kt`
  - A row of compact text pills: the selected one sits on a stadium (the same shape and colour as the bottom navigation indicator) that slides to the newly selected pill. Used for the Time sections and the voice profile, so both switches look identical.
- `core/ui/src/main/kotlin/dev/proflyder/home/core/ui/ServerText.kt`
  - Resolves a server `LocalizedText.key` to its local resource. Resources are named mechanically after the key: `scheduler.error.not_found` is `@string/server_scheduler_error_not_found` (prefix `server_`, dots become underscores), so adding a server key never …
- `core/ui/src/main/res/drawable/ui_bg_pill_segment.xml` — Touch feedback of one pill. The selected stadium is drawn by PillSegments itself so it can slide between pills.
- `core/ui/src/main/res/values/styles.xml`
  - Large light clock digits, fixed-width so they do not jitter while counting.
- `detekt.yml` — detekt configuration for the Home Android client.
- `distribute.sh` — Build the Home Android app and distribute it to Firebase App Distribution.
  - Release signing material. Locally `mise run admin:android-distribute` passes it from OpenBao through fnox (temp files, removed on exit). The keystore is stored base64-encoded, so SIGNING_KEYSTORE_B64_FILE is decoded into a private temp file here.
  - Release notes for a GitHub Actions run. On a PR the job exports PR_NUMBER and PR_TITLE (the title is untrusted text, so it arrives through the environment, never as shell source).
- `features/alarms/api/src/main/kotlin/dev/proflyder/home/feature/alarms/AlarmEditorDraft.kt`
  - Non-secret editor draft persisted in SavedStateHandle. It intentionally holds no token, endpoint credentials, or schedule payload. A fresh draft defaults to [defaultAlarmTime] so a new alarm never starts in the past, and to a one-shot alarm dated today.
- `features/alarms/api/src/main/kotlin/dev/proflyder/home/feature/alarms/AlarmEditorTime.kt`
  - Wall time [offsetMinutes] after [now]. The offset is always counted from the current time, never from a previously selected value, so repeated taps do not accumulate. Minutes are not rounded and crossing midnight wraps.
- `features/alarms/api/src/main/kotlin/dev/proflyder/home/feature/alarms/AlarmLabel.kt` — The alarm name is optional in the editor. A blank or whitespace-only name keeps the localized default, so the server always receives a non-empty label. The API stays resource-free; the caller supplies the string from `values/`.
- `features/alarms/impl/src/main/kotlin/dev/proflyder/home/feature/alarms/presentation/AlarmAdapter.kt`
  - Selection motion is only for the same alarm changing state. A recycled holder bound to a different alarm must show that alarm's selection in its end state, never animate the previous row's selection into the new one.
  - Programmatic `isChecked` must not be reported as a user toggle.
- `features/alarms/impl/src/main/kotlin/dev/proflyder/home/feature/alarms/presentation/AlarmEditorViewModel.kt`
  - Applies the sheet's choice. A custom day set that covers a preset is stored as that preset, so "Пн–Пт" never lingers as a custom list.
- `features/alarms/impl/src/main/kotlin/dev/proflyder/home/feature/alarms/presentation/AlarmRepeatRules.kt`
  - Day sets are ISO weekdays: 1 is Monday, 7 is Sunday. A custom selection that covers exactly a preset is stored as that preset, so the editor and the home server never show two different names for the same days.
- `features/alarms/impl/src/main/kotlin/dev/proflyder/home/feature/alarms/presentation/AlarmsFragment.kt`
  - Keep the existing list visible while a refresh is in flight; do not replace it with a spinner.
- `features/alarms/impl/src/main/res/color/alarms_day_toggle_background.xml` — Round day toggle: a filled primary circle when checked, an outlined transparent circle otherwise.
- `features/alarms/impl/src/main/res/color/alarms_delete_ripple.xml` — The delete ripple is the theme's error color at the Material pressed alpha, so the round state layer belongs to the red button rather than to the neutral card.
- `features/alarms/impl/src/main/res/layout/item_alarm.xml`
  - Idle: switch plus delete. Selection mode: both are gone and the circular check indicator takes their place, inset by the card padding so it never touches the rounded corner.
- `features/alarms/impl/src/main/res/layout/sheet_alarm_repeat.xml` — Modal repeat sheet. The rounded top and the drag handle live on the sheet content itself, so the sheet keeps the app's dark surface when the dialog window is transparent.
- `features/alarms/impl/src/main/res/values/alarms_dimens.xml` — Fallback before the bottom bar is measured; matches its real height: a 64dp icon+label action plus the 6dp bar padding on both sides.
- `features/alarms/impl/src/main/res/values/styles.xml`
  - Seven round day toggles on one row. The size is fixed, not weighted: each toggle keeps a 44dp circle while the equal cells spread them across the sheet width, so a two-letter label never gets clipped.
  - Contextual bulk action: the icon sits above a small one-line label, so the three cells stay readable at 360dp with a 1.15 font scale. The button never wraps its text; a longer translation is ellipsized.
- `features/alarms/impl/src/test/kotlin/dev/proflyder/home/feature/alarms/domain/SaveAlarmLabelTest.kt`
  - The server must never see an empty alarm name: the editor field is optional and [SaveAlarm] fills the localized default where the request is formed.
- `features/call/api/src/main/kotlin/dev/proflyder/home/feature/call/AudioRoutes.kt`
  - Maps the current output devices to the route list. Built-in routes are always offered (they do not depend on a hot-plugged device); wired and Bluetooth appear only while such a device is connected.
- `features/call/api/src/main/kotlin/dev/proflyder/home/feature/call/CallController.kt`
  - Single source of truth for the live call. UI, Telecom connection, foreground service and notification observe [state] and never keep their own copy.
- `features/call/api/src/main/kotlin/dev/proflyder/home/feature/call/CallManager.kt`
  - Media starts only after the server accepted the call, so a busy or offline station never opens the microphone.
- `features/call/api/src/main/kotlin/dev/proflyder/home/feature/call/CallMedia.kt`
  - Releases the microphone and the peer connection. Idempotent.
- `features/call/impl/src/main/kotlin/dev/proflyder/home/feature/call/telecom/TelecomCoordinator.kt`
  - Registers (or refreshes) the self-managed account and returns its handle, or null when Telecom is unavailable or refuses. `getPhoneAccount` is not used: some OEM builds (MIUI, Android 16) guard it with READ_PHONE_NUMBERS, while re-registering our own account …
- `features/connection/api/src/main/kotlin/dev/proflyder/home/feature/connection/ConnectionRepository.kt`
  - Calls home.v1.SchedulerService/GetStatus; never uses mock data.
  - The token is never displayed; blank input means "keep what is stored".
- `features/control/api/src/main/kotlin/dev/proflyder/home/feature/control/CompactControls.kt`
  - One pure rule deciding how a control inside a device card is drawn: a round compact tile or a normal row. A tile is a binary on/off control — it accepts both turn_on and turn_off — inside a card that holds at least two controls.
- `features/control/api/src/main/kotlin/dev/proflyder/home/feature/control/ControlItems.kt`
  - A section header. [section] identifies the section (not its title), so a room that happens to be called "Home" never collides with another section. [summary] is a short right-aligned note ("2 of 4 on").
  - The profile is a mode: only the segmented pills act, never a row tap.
  - The chip counts describe primary controls only: a device's settings are part of it and never add a chip or a count of their own.
  - The chips: All, then the device categories that exist, then each room (only when there is more than one, otherwise rooms add nothing).
  - [leading] items (the live rows and the pinned quick actions) come first, then the single voice card (a mode and wake marks, not content, shown whatever the filter is), then available devices grouped by room and, inside a room, by device, then unavailable …
  - Visibility comes first: a conditional entry whose target is not in the requested state must never reach cards, chips, counters or the "Unavailable" section.
  - Dead rows keep the old behaviour: they follow the applied filter too, and a lone setting never shows up as its own unavailable device.
  - A group is shown when one of its primary controls matches the filter; its settings then follow the device, never the chip.
- `features/control/api/src/main/kotlin/dev/proflyder/home/feature/control/ControlRepository.kt`
  - Queues "speak this text on the station". The text is user content: it only ever goes into the task payload, never into a URL or a log line.
- `features/control/api/src/main/kotlin/dev/proflyder/home/feature/control/SpeakText.kt` — Backwards-compatible name for the shared speech rules now living in `packages/kotlin/core`. The Home screen and the scenario builder must agree on exactly the same normalization.
- `features/control/api/src/main/kotlin/dev/proflyder/home/feature/control/SpeedDial.kt` — One row of the Home quick-actions dial. The dial is built from the existing quick tiles (call, speak, pinned devices and scenarios) plus the resolved wake-feedback marks, so a newly pinned favourite appears automatically.
- `features/control/api/src/main/kotlin/dev/proflyder/home/feature/control/WakeFeedback.kt`
  - Resolves the two Home Assistant scripts that mark a wake event from the catalog. They are ordinary personal scenarios whose entity ids are part of the house contract (`projects/home-assistant/ha-voice/scripts.yaml`), so they are matched by id and never by …
- `features/control/api/src/test/kotlin/dev/proflyder/home/feature/control/ControlItemsTest.kt`
  - Its section and key never collide with a room or another section.
  - A standalone row is titled by its device, never by the short entity title or the id fallback.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/presentation/ControlAdapter.kt`
  - The single "Voice" card: the profile is a mode, not a device, so only an explicit pill acts (never a row tap); the two wake marks send at once. Programmatic selection during binding must never look like a user choice.
  - A switch flip is optimistic; put it back until the server confirms. The recurring-alarm skip/disable dialog then decides what actually happens, so a cancelled dialog never leaves a false off state.
  - Programmatic `isChecked` must not be reported as a user toggle.
  - Do not animate the thumb when the state is already the target one.
  - Set the programmatic state before installing the listener, so binding a row can never look like a user toggle.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/presentation/ControlIcons.kt`
  - The voice profile is a mode, not a device: its "message" hint must not reuse the speak artwork.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/presentation/ControlViewModel.kt`
  - Loads the live rows and pinned shortcuts. A failing source simply contributes no rows: the hub is optional decoration over the catalog, so one unreachable feature must not replace the whole Home screen.
  - Never persist defaults computed from a catalog that failed to load.
  - The executor applies commands asynchronously, so a refresh right after a tap can still return the old state. The tapped value is therefore kept (see [expectedPower]) until the server confirms it or [EXPECT_TTL_MS] passes;
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/presentation/SpeakDialog.kt`
  - The "say on the station" dialog: a multiline text with a 500-character counter. "Say" stays disabled until the text is speakable; the text itself is never logged, only put into the queued command.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/presentation/VolumeSheet.kt`
  - Bottom sheet of the station volume: the main level with a mute toggle and the announcement level. A command is sent when a slider is released, not while dragging.
  - Hardware volume keys adjust the station by five percent per press. The event is consumed even at the edge (0 or 100), so the phone's own stream never changes while the sheet is open.
- `features/control/impl/src/main/res/drawable/control_tile_ripple.xml` — Round pressed highlight for a compact tile: the oval mask keeps the ripple inside the icon circle instead of the square tile cell.
- `features/control/impl/src/main/res/layout/item_control_hub.xml` — Compact "next up" block: a short list of live rows, hidden when empty. Rows fill the card edge to edge so a row ripple is clipped by the card corners instead of stopping short of them.
- `features/control/impl/src/main/res/layout/item_control_quick_tile.xml` — A compact quick action: accent icon on the left, a label of up to two lines next to it. Two tiles share a row; every tile has the same height.
- `features/control/impl/src/main/res/layout/item_control_voice.xml` — One compact voice card: the profile switch and the two wake marks. Rows are hidden independently; the whole card exists only when one of them is.
- `features/control/impl/src/main/res/layout/overlay_control_speed_dial.xml` — Full-window speed dial: scrim above the bottom navigation, item FABs and a copy of the root FAB. Nothing here paints a panel background.
- `features/control/impl/src/test/kotlin/dev/proflyder/home/feature/control/presentation/ControlProfileRowTest.kt`
  - Robolectric smoke test of the voice card profile row: it must inflate with real resources, show the current side, hide the row when there is no profile, and report only *user* choices — the programmatic selection during binding must never send a command.
  - Re-binding a row never replays a command.
  - Payload binding is programmatic and must never look like a user toggle.
- `features/push/api/src/main/kotlin/dev/proflyder/home/feature/push/NotificationHistory.kt`
  - One FCM message this phone actually received, as the history screen shows it. The text is the decoded payload, already bounded by [PushPayloads], and [deepLink] is the validated `home://…` route (never an arbitrary URL).
  - Retention rules, kept pure so they are unit-tested without Android: newest first, a repeated [NotificationHistoryEntry.id] replaces its older copy, and the log never grows past [MAX_ENTRIES].
  - Local log of received messages. It is written by the FCM service (which runs with the app killed) and read by the history screen, so it persists to disk and never depends on the server being reachable.
- `features/push/api/src/main/kotlin/dev/proflyder/home/feature/push/PushModels.kt`
  - When the server last accepted it (unix millis); 0 = never.
  - Registers the token digest for display: never show the whole secret.
  - Deep link carried by a notification. Only the app's own scheme is accepted, so a server-supplied string can never open another application.
- `features/push/api/src/test/kotlin/dev/proflyder/home/feature/push/PushApiTest.kt`
  - Built from parts so the literal can never look like a real credential to a secret scanner.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/AndroidPushDeviceIdentity.kt`
  - The raw ANDROID_ID never leaves the phone: it is hashed with an app salt below, and only the digest is registered. The lint warning targets code that reports hardware identifiers as-is.
  - App-specific salt: the server never sees the raw ANDROID_ID.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/DataStoreNotificationHistoryStore.kt`
  - The received-message log in the app-wide DataStore, one JSON value under its own key (the notification text is not a credential; `allowBackup` is off, so it never leaves the device).
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/DataStorePushRegistrationStore.kt`
  - Last accepted registration in the app-wide DataStore. The token, its timestamp and the owner-visible name are stored: the token is not a credential for the home server (the bearer token is), but it is still never logged or shown in full, and `allowBackup` is …
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/FindPhoneActivity.kt`
  - The screen a full-screen intent opens: it turns the display on, shows over the lock screen and offers one button, «Нашёл». Pressing it (or Back, or the notification action) stops the ring through the same [FindPhoneController] the service uses.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/FindPhoneAudio.kt`
  - The loud part of find-phone. `USAGE_ALARM` audio plays through the alarm stream, which is exempt from the silent and vibrate ringer modes, and the stream volume is raised to its maximum for the duration of the ring, then restored exactly as it was.
  - Starts the alarm tone and the repeating vibration. Idempotent. VIBRATE is install-time and declared here.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/FindPhoneService.kt`
  - Foreground service that keeps the ringing process alive. Started by [FindPhoneController] from a **high-priority FCM data message**, which is exactly the case Android allows a background app to start a foreground service from.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/FirebasePushRegistrar.kt`
  - Registers this device's FCM token with the home scheduler. The token is registered: * at start ([start]) — so a server that lost the registry or a phone that was reinstalled re-registers without waiting for Google to rotate the token;
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/HomeMessagingService.kt`
  - FCM entry point. * `onRegistered` — a new or rotated token is registered immediately; a failure is retried on the next app start (`onUnregistered` forgets the old one).
  - History is written off the main thread; a kept message never blocks the ring.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/NotificationHistoryCodec.kt`
  - One stored row. It mirrors [NotificationHistoryEntry] instead of serializing the API model directly, so a renamed field never breaks an existing file and an unknown type downgrades to an ordinary notification.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/PushChannels.kt`
  - The app's two notification channels. Channels are created once and their properties cannot be changed later, so a channel id must never be reused with different settings: Android keeps the original sound and vibration of `home_phone_find`, which is why the …
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/PushEntryPoint.kt` — Process-wide access to the push collaborators for components the framework instantiates itself (`FirebaseMessagingService`, the foreground service, the ringing activity). They cannot be constructor-injected by Metro, so the application installs this entry point in `onCreate`, exactly like the call feature does.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/PushNotifier.kt`
  - Draws the two notification shapes. All user-visible text comes from resources; a server message supplies the title and body of an ordinary notification and nothing else.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/PushPayloads.kt` — One decoded FCM message. The server sends two shapes:
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/PushPermissions.kt`
  - Do Not Disturb access lives outside the app's own settings page.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/TextLimits.kt` — Truncates to the server's byte bound without splitting a UTF-8 character.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/presentation/PushSettingsFragment.kt`
  - «Уведомления и поиск телефона»: the real state of the three permissions the loud path needs and a button that opens the matching system page for each.
- `features/push/impl/src/main/res/layout/activity_find_phone.xml` — The ringing screen: one button, the reason, and nothing else that could be misread at 3 a.m.
- `features/push/impl/src/main/res/layout/fragment_push_settings.xml` — Settings screen: three permission rows, a ring summary and the device registration, each section a tonal card with its own icon. Every visible string comes from resources.
- `features/push/impl/src/test/kotlin/dev/proflyder/home/feature/push/AllPushImplTests.kt`
  - One ring stops on «Нашёл» or after a minute, never later.
- `features/push/impl/src/test/kotlin/dev/proflyder/home/feature/push/FindPhoneAudioTest.kt`
  - The ring must be audible without leaving the phone loud. `USAGE_ALARM` already plays through silent and vibrate modes, so the ringer mode is never changed: the old code switched it to normal and restored only the alarm volume, which permanently took the phone …
- `features/push/impl/src/test/kotlin/dev/proflyder/home/feature/push/NotificationHistoryStoreTest.kt`
  - The history is written by the FCM service, which can run before any screen loaded the log. These tests use the real DataStore file, so a message is never lost by an unbounded read-modify-write.
- `features/push/impl/src/test/kotlin/dev/proflyder/home/feature/push/PushDeviceIdentityTest.kt`
  - The stable device identity survives token rotation and is never the raw platform id.
- `features/reminders/impl/src/main/kotlin/dev/proflyder/home/feature/reminders/presentation/ReminderAdapter.kt`
  - Programmatic `isChecked` must not be reported as a user toggle.
- `features/scenarios/api/src/main/kotlin/dev/proflyder/home/feature/scenarios/ScenarioEditorDraft.kt`
  - Non-secret editor draft persisted in SavedStateHandle. It intentionally holds no token, endpoint credentials, or request payload. [hasUnsupportedParts] is derived from the retained unknown oneof variants: a definition created by a newer server can be opened …
- `features/scenarios/impl/src/main/kotlin/dev/proflyder/home/feature/scenarios/presentation/CatalogPickerDialog.kt`
  - Catalog picker grouped by room and then by device. Group titles never contain raw entity ids: a missing device falls back to a meaningful entry title or a localized "Scenarios HA"/"Other" label.
- `features/scenarios/impl/src/main/kotlin/dev/proflyder/home/feature/scenarios/presentation/LightParamsDialog.kt`
  - A stale effect from an older payload must not be sent if the light no longer reports it; the executor rejects unknown effects.
- `features/scenarios/impl/src/main/kotlin/dev/proflyder/home/feature/scenarios/presentation/ScenarioAdapter.kt`
  - Programmatic `isChecked` must not be reported as a user toggle.
- `features/scenarios/impl/src/main/kotlin/dev/proflyder/home/feature/scenarios/presentation/ScenarioFormat.kt`
  - Prefer the human device name when HA only sent a technical entity title ("Light", "Switch"); never repeat the same name twice.
- `features/settings/impl/src/main/kotlin/dev/proflyder/home/feature/settings/presentation/NotificationHistoryViewModel.kt`
  - Reads the local log only: the history is what the phone received, so it never asks the server (which may well be unreachable right now).
- `features/settings/impl/src/main/res/layout/fragment_notification_history.xml` — The local received-message log. The header row holds the count and the clear action; the body is either the list or the empty state.
- `features/settings/impl/src/main/res/layout/fragment_settings.xml` — Settings hub: three navigation rows over tonal cards, one per settings area. Every visible string comes from resources.
- `features/settings/impl/src/main/res/layout/item_notification_history.xml` — One received message. The type icon carries the spoken label; the chevron appears only when the message has an in-app link.
- `lint.xml` — Android Lint gate for the Home Android client.
- `release_info.py` — Read or write the metadata of a distributable Home APK.
