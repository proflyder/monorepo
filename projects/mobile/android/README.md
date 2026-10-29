# Home Android client

Kotlin/Android management client for the home server. The server owns
scheduling and execution; the phone manages server state and reads the Home
catalog. One Activity, Fragment screens, Metro compile-time DI, feature
API/implementation modules, and a typed router.

## Limits

Debug-signed only. No FCM, phone-side background execution, offline cache,
or generic task UI. Current device acceptance and remaining work are tracked
in [STATE](../../../docs/STATE.md) and [PENDING](../../../docs/PENDING.md).

## Features

- Bottom tabs: **Home**, **Alarms**, **Timers**, **Reminders**, **Connection**.
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
- Connection: scheduler base URL, optional Home control base URL, required
  bearer token, connect/read timeouts, **Save and test connection**
  (`GetStatus`), explicit offline/auth/TLS/validation states. Settings persist
  across failures and are never cleared by a failed request.
- Loading/empty/error/content states; existing data stays visible during refresh
  or transient errors. Lists never silently become empty.

## Home (scenarios and devices)

- The catalog comes from the home control service (`GET /v1/catalog`); nothing
  is hard-coded in the app. Names, rooms, icons, and hidden flags come from Home
  Assistant.
- Scenarios are HA scripts carrying the native `scenario` label. Devices are
  lights/switches. Icons and rooms are display metadata only.
- Tapping a scenario or toggling a device enqueues a scheduler task
  `home.control` on the `control` queue (`EnqueueTask`). The phone only queues;
  the server executor owns execution. Device switches are optimistic and then
  re-read from the server. Delivery is at-least-once, not exactly-once.
- Filter chips (All / Scenarios / Lights / Switches, plus room filters) are
  persisted in DataStore and restored on launch.
- Without a Home control address the tab explains what to configure; it does not
  fail silently.

## Connection and networking

- Two base URLs, no paths:
  - **Server address** (scheduler): LAN `http://<node>:30098` or Tailscale
    `https://pl-scheduler.<tailnet>.ts.net`.
  - **Home control address** (catalog): LAN `http://<node>:30099` or Tailscale
    `https://pl-control.<tailnet>.ts.net`.
  - The app appends the Connect RPC path or `/v1/catalog` itself. Do not paste
    `/v1/catalog` into the field.
- The same bearer token is used for scheduler and control. Endpoints and tokens
  are deployment-specific; none are embedded in the app.
- Transport: Retrofit on OkHttp, Connect unary JSON
  (`POST {endpoint}/ricardo.v1.SchedulerService/{RPC}`), protobuf JSON field
  names, `int64` as decimal strings. `Authorization: Bearer` is sent in the
  header only.
- Direct LAN `http://` is a deliberate private-network trade-off and is allowed
  by `network_security_config.xml`; HTTPS is recommended and uses the platform
  trust store. Redirects are disabled. No custom trust manager, hostname
  verifier, or certificate pinning; the system chooses LAN or Tailscale routing.

## Credentials and backup

- The bearer token is encrypted with a non-exportable Android Keystore
  AES-256/GCM key (`AES/GCM/NoPadding`, random IV, AAD). Only ciphertext is
  stored; the plaintext token never is.
- Endpoint/timeouts/token and UI preferences (Home filter) live in one Jetpack
  Preferences DataStore (`HomeDataStore`). On the first launch after the
  upgrade, `SharedPreferencesMigration` imports the legacy `home_connection`
  file, so saved endpoints and the encrypted token survive. Keep this migration
  until no supported install can predate DataStore.
- Readers use a synchronous in-memory snapshot loaded once at startup because
  interceptors read settings on I/O threads.
- `android:allowBackup="false"`; backup/data-extraction rules exclude prefs,
  databases, and files. Tokens are never placed in URLs, `SavedState`, Fragment
  arguments, or logs. Diagnostics redact `Authorization`.
- If Keystore decryption fails, the token is reported unreadable and must be
  re-entered; there is no plaintext fallback.

## Build and test

Prerequisites: macOS/Linux, JDK 21, Android SDK with platform 36 and
build-tools 36.0.0 (`ANDROID_HOME`), Bazelisk. The root contract is
`MODULE.bazel` (no Gradle, no `WORKSPACE`).

```sh
cd /path/to/yandex-station-lab
export ANDROID_HOME="$HOME/Library/Android/sdk"
export PATH="$PWD/secrets/local-tools:$PATH"

bazelisk build //projects/mobile/android:app
# bazel-bin/projects/mobile/android/app/app.apk

bazelisk test //projects/mobile/android:android_tests

# Optional: install on a connected device (keeps app data).
adb install -r bazel-bin/projects/mobile/android/app/app.apk
```

`android_tests` covers endpoint/security rules, time/duration parsing, Connect
JSON mapping and failures, idempotency, navigation queue/restoration, the real
Metro graph via a Robolectric cold launch, and an APK packaging gate that
verifies required runtime classes are **defined** in DEX and that the deploy jar
has no unresolved non-platform references. The gate also self-tests that a
referenced-but-undefined class is rejected. Tests do not replace interactive
device acceptance.

## Architecture

```text
projects/mobile/android/
  app/                    single Activity, app graph, tabs, diagnostics
  core/
    model/                Android-free scheduler domain + server clock
    config/               connection settings contract + redaction
    settings/             DataStore + Keystore token + legacy migration
    network/              Retrofit/OkHttp Connect + control catalog + errors
    navigation/           typed destinations, FeatureEntry, FragmentRouter
    ui/                   base Fragment/ViewModel, ViewBinding, list states
  features/
    alarms/{api,impl}     domain contract / use cases + repository + UI
    reminders/{api,impl}
    timers/{api,impl}
    control/{api,impl}    Home catalog, filters, scenario/device actions
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
`ResumeTimer`, `CancelTimer`, and `EnqueueTask` (Home control only). Home
catalog reads use `GET /v1/catalog`.

## App identity and permissions

- Application ID `dev.proflyder.home`; version 0.1.0 (versionCode 1).
- minSdk 26, target/compile SDK 36, Kotlin/JVM target 17.
- Only `android.permission.INTERNET`.
- Debug-signed; debug APKs are not byte-reproducible.
- Browsable diagnostics (logcat tag `HomeHttp` + Chucker) are gated by the
  `home_network_inspection` bool and should be disabled in release variants.

## Known limitations

- No phone-side notifications/FCM, no background execution, no offline cache.
- No generic task/scenario editor UI; Home executes existing catalog entries.
- Full interactive acceptance on a device (filters, scenario taps, device
  toggles, alarm/timer flows) is still pending; automated input is blocked by
  HyperOS.
- `EnqueueTask` is one-shot; recurring generic tasks and event-trigger rules are
  not implemented.
