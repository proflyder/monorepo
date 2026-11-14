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

- Bottom tabs: **Home**, **Scenarios**, **Time**. **Time** is a `ViewPager2`
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
    ([Notification history](#notification-history)).
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
  to its maximum for the duration and restores it afterwards, bypasses silent
  mode and Do Not Disturb (channel `home_phone_find`, `bypassDnd`), opens the
  `FindPhoneActivity` over the lock screen through a full-screen intent, and
  stops on «Нашёл» or after 60 seconds. A high-priority FCM message is exactly
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
device-identity and name-persistence contract runs in `identity_test`.

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
