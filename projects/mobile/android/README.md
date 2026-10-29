# Home Android client

The Android management client for the home server's `ricardo.v1.SchedulerService`.
It is a foundation for a broader home app: one Activity, Fragment screens, real
Metro-generated DI, feature API/implementation modules, and a small typed router.

## What is implemented

- Bottom tabs for **Alarms**, **Timers**, **Reminders**, **Home**, and **Connection**.
- Alarms: list, create, edit, enable/disable, delete, snooze (9 min default), stop.
  Repeats: one-shot with an explicit date, daily, weekdays, weekends, custom ISO
  weekdays. Timezone is IANA. Existing server one-shot alarms are shown and
  edited as one-shot; they are never silently converted to daily.
- Reminders: one-shot date/time in an IANA timezone, create, edit, enable/disable,
  delete. The server stores the `onceDate`; the phone never schedules it.
- Timers: start, list, pause, resume, cancel. Remaining time is computed from the
  server clock returned by `GetStatus`, not from the phone clock alone.
- Alarm multi-select: long-press a card to enter selection mode, tap to add or
  remove cards, then bulk **Enable / Disable / Delete** (delete asks for
  confirmation). Back leaves selection mode first. Bulk actions run sequentially
  against the server; on the first failure the run stops, the error is shown and
  the unprocessed alarms stay selected for a retry.
- Home: scenarios and devices (lights, switches) from a server-side catalog —
  nothing is hard-coded in the app. The list comes from the home control service
  (`GET /v1/catalog`, optional "Home control address" in Connection); commands go
  through the scheduler as a `home.control` task on the `control` queue, so the
  phone only queues them and the server executes. Scenarios run on tap; devices
  have an on/off switch (optimistic, then re-read from the server). Pull to
  refresh; without the address the tab explains what to set. See
  `solutions/ha-bridge/README.md` (ha-control).
- Connection: endpoint, required bearer token, connect/read timeouts, explicit
  **Save and test connection** using `GetStatus`, and honest offline/auth/TLS
  states. A token can be deliberately removed; the next request fails with a
  controlled missing-token error before any HTTP call. Settings are not cleared
  when a request fails.
- Loading, empty, error, and retry states. Existing list data stays visible during
  refresh or transient errors; lists do not silently become empty. Timers refresh
  from the server periodically and when a countdown reaches zero.
- Create/start requests retain the same idempotency key across ambiguous network
  retries (same payload and endpoint); success, a definite rejection, or an input
  change starts a new logical operation. Alarm/reminder editor drafts and their
  create keys are kept in `SavedStateHandle` across process recreation; the timer
  start key lives in the ViewModel for the process lifetime. No tokens are saved.

The home server owns scheduling and execution. The app does **not** use
`AlarmManager`, FCM, a foreground timer service, or an embedded VPN. Tailscale
connectivity is provided by the Tailscale app and system routing. Phone reminder
notifications and the live HA scheduler migration are **not** implemented in this
foundation.

## Build

Prerequisites:

- macOS/Linux with JDK 21 and an Android SDK containing platform 36 and
  build-tools 36.0.0 (`ANDROID_HOME` must point at it).
- Bazelisk. This checkout keeps an ignored copy at `secrets/local-tools/bazelisk`;
  no system-wide install is required. Bazel itself is downloaded by Bazelisk and
  the root contract is `MODULE.bazel` (no Gradle, no `WORKSPACE`).

```sh
cd /path/to/yandex-station-lab
export ANDROID_HOME="$HOME/Library/Android/sdk"
export PATH="$PWD/secrets/local-tools:$PATH"

# Debug-signed APK (this target compiles the real Metro application graph).
bazelisk build //projects/mobile/android:app

# APK path
ls -l bazel-bin/projects/mobile/android/app/app.apk

# Optional install on a connected device.
adb install -r bazel-bin/projects/mobile/android/app/app.apk
```

If `ANDROID_HOME` is unset, the rules_android SDK extension creates an empty SDK
repository and the APK target fails. No machine-specific SDK path is committed.

### Tests

```sh
export ANDROID_HOME="$HOME/Library/Android/sdk"
export PATH="$PWD/secrets/local-tools:$PATH"

bazelisk test //projects/mobile/android:android_tests
```

The JVM tests cover endpoint normalization/security, duration/time/weekday input,
endpoint config redaction, required/header-safe tokens, server-clock monotonic
behavior across wall-clock jumps, Connect JSON request/response mapping,
malformed list-field rejection, 8 MiB response bounds, disabled redirects,
timeouts, offline/auth failures, idempotency-key retention/reset, editor draft
round-trips and completion retention without a collector, plus navigation
pending/queue/restoration state. Robolectric additionally covers a real
`HomeApplication` + `MainActivity` cold launch and root Fragment attachment, plus
token clearing. No physical-device/emulator run was performed.

## Verified artifact

- Target: `//projects/mobile/android:app`
- APK: `bazel-bin/projects/mobile/android/app/app.apk`
- Package: `dev.proflyder.home`, min SDK 26, target/compile SDK 36, debug-signed
- Permissions: `android.permission.INTERNET`
- Debug APKs are not byte-reproducible (signing/timestamps). Check the current
  artifact locally with:
  `shasum -a 256 bazel-bin/projects/mobile/android/app/app.apk`

## Module layout

```
projects/mobile/android/
  app/                    single Activity, app graph, toolbar + bottom tabs
  core/
    model/                Android-free scheduler domain + ServerClock
    config/               connection settings contract + redaction
    settings/             Android Keystore token + SharedPreferences store
    network/              Connect unary JSON transport, DTO mapping, errors
    navigation/           Destination/FeatureEntry + typed FragmentRouter
    ui/                   BaseFragment/BaseViewModel, ViewBinding, factory registry, errors, list states
  features/
    alarms/{api,impl}     domain contract / use cases+repo+UI+entry
    reminders/{api,impl}
    timers/{api,impl}
    connection/{api,impl}
packages/kotlin/core/     shared Kotlin utilities (endpoint/time/nav state)
platform/android/         shared Bazel macros + Metro compiler plugin wiring
```

Feature implementations depend only on their own API, core modules, and shared
Kotlin utilities; no feature impl imports another feature impl. The app module is
the composition root and the only module that sees every implementation.

### DI

Metro's compiler plugin (`dev.zacsweers.metro.compiler`, `compile_phase = True`)
is applied to every Kotlin Android compilation through
`//platform/android:defs.bzl`. The application graph in
`app/.../di/AppGraph.kt` is generated at compile time by Metro:

- feature repositories use `@ContributesBinding(AppScope::class)`,
- ViewModel factory contributors and navigation entries use
  `@ContributesIntoSet(AppScope::class)`,
- `AppBindings` provides the OkHttp client, `SchedulerClient`, `ServerClock`, and
  `FeatureRegistry`,
- `bazelisk build //projects/mobile/android:app` fails if the graph is
  invalid; the APK contains the generated `AppGraph$Impl`.
- There is no runtime service locator or hand-written mock repository. Fragments
  request their ViewModel factory from the Activity host, which delegates to the
  graph-built `ViewModelFactoryRegistry`.

### UI building blocks

- **ViewBinding.** rules_android has no ViewBinding and `androidx.viewbinding` is not in
  the pinned Maven closure, so `metro_android_library(view_binding = True, ...)`
  (`//platform/android:defs.bzl`) runs `platform/android/gen_view_binding.py` over the
  module's `res/layout/*.xml` and compiles the result with the module. Each layout
  `fragment_foo.xml` yields `FragmentFooBinding` in `<custom_package>.databinding`,
  with a typed `val` per `android:id` (camelCase), `inflate(...)` and `bind(view)`,
  implementing `core.ui.ViewBinding`. `<merge>` roots are rejected; only the default
  `layout/` directory is bound. Modules using it depend on `core/ui`.
  Generator tests: `python3 -I platform/android/gen_view_binding_test.py`.
- **`BaseFragment<VB>`** inflates the binding in `onCreateView`, exposes `binding` only
  between view creation and destruction, and offers `observe` (lifecycle-aware flow
  collection), `repeatOnView`, `showMessage`/`showFailure` and `confirm`. Subclasses
  implement `onBindingCreated` and pass `FooBinding.Companion::inflate`.
- **`BaseViewModel<S>`** owns immutable UI state: `state` is read-only, changes go through
  `updateState { copy(...) }`, `currentState` and `launch` are helpers. Savable editors
  compute their initial state from `SavedStateHandle` in the super call.
- **`ListStateViews`** shows exactly one of loading / empty / error / content;
  `showContent(refreshing = true)` keeps the list visible during a refresh.

### Navigation

`core/navigation` exposes typed `Destination` objects and `FeatureEntry`
registrations. `FragmentRouter` uses the support `FragmentManager` back stack and
the AndroidX `OnBackPressedDispatcher`. Navigation commands issued while the host
is paused or the FragmentManager has saved state are queued and drained only on
resume; the pure queue and per-tab route state are unit-tested. Process-death
restoration relies on FragmentManager plus a saved-state bundle containing only
tab ids and route keys (never tokens or endpoints).

## Server transport

Connect unary JSON/HTTP:

```
POST {endpoint}/ricardo.v1.SchedulerService/{RPC}
Content-Type: application/json
Connect-Protocol-Version: 1
Authorization: Bearer <optional token>
```

- Field names are protobuf JSON lowerCamelCase; `int64` values are decimal
  strings; enum values use proto names such as `SCHEDULE_KIND_ALARM`,
  `REPEAT_KIND_CUSTOM`, and `TIMER_STATE_RUNNING`.
- Implemented calls: `GetStatus`, `GetSchedule`, `ListSchedules`,
  `CreateSchedule`, `UpdateSchedule`, `DeleteSchedule`, `SetScheduleEnabled`,
  `SnoozeSchedule`, `StopSchedule`, `StartTimer`, `ListTimers`, `PauseTimer`,
  `ResumeTimer`, `CancelTimer`.
- Generic task queue calls (`EnqueueTask`, `CancelTask`) and scenario execution
  are not exposed in this client slice.
- HTTP/HTTPS redirects are disabled in both the base and per-timeout OkHttp
  clients. A bearer token therefore cannot be forwarded to a `Location` host; a
  3xx response becomes an explicit protocol failure. A test covers this.
- TLS uses the platform trust store. There is no custom trust manager, hostname
  verifier override, or certificate pinning.
- The socket is not bound to Wi-Fi; Android/system routing chooses LAN or the
  Tailscale interface.

### Transport security / cleartext rationale

`res/xml/network_security_config.xml` permits cleartext because the user may run
the server at an explicit local or Tailscale `http://` address when they have not
configured TLS. This is a deliberate, documented trade-off for a private home
network, not a default public endpoint. HTTPS is recommended and uses normal
system verification. Tokens are still sent only in the Authorization header, so
they are not exposed in URLs or logs.

### Endpoint rules

The endpoint must be a full `http://` or `https://` URL and may be a LAN IPv4
address, a bracketed IPv6 literal, `localhost`, or a Tailscale MagicDNS name,
optionally with a path prefix. It rejects:

- credentials or userinfo (`user:pass@host`),
- query strings and fragments (common token leaks),
- unsupported schemes, missing schemes,
- invalid ports, invalid IPv4 octets, malformed DNS labels,
- dot segments, percent-encoded traversal, backslashes, and double slashes.

The canonical URL is stored, and `toString()` is safe for diagnostics.

### Credentials and backup

- The token is encrypted with a non-exportable Android Keystore AES-256/GCM key
  (`AES/GCM/NoPadding`, random IV, AAD). Ciphertext is stored in
  SharedPreferences; plaintext token never is.
- If Keystore decryption fails, the UI reports the token as unreadable and asks
  the user to enter it again; it does not silently downgrade to plaintext.
- `android:allowBackup="false"` plus full-backup and data-extraction rules exclude
  shared preferences, databases, and files from backup/device transfer.
- The token input disables Android view-state saving, and tokens are never placed in
  URLs, `SavedState`, Fragment arguments, or logs.
  Connection diagnostics use a redacted `ConnectionConfig.toString()`.

## Maven dependencies and locks

Dependencies are pinned in `maven_install.json` in this directory, which
contains resolved versions and checksums. To regenerate after changing `MODULE.bazel` artifacts:

```sh
bazelisk run @maven//:pin
```

`MODULE.bazel.lock` is the Bazel module lock and is committed for reproducible
toolchains. The Maven lock is referenced from the root `MODULE.bazel`.

Verified toolchain choices:

- `rules_kotlin` 2.4.20 (Kotlin 2.4.20 compiler)
- `rules_android` 0.7.3 (matches rules_kotlin's Android dependency)
- `rules_jvm_external` 7.1
- Metro 1.5.0 (`runtime-jvm` + compiler plugin)
- compile/target SDK 36, min SDK 26, JDK 21 for Bazel tools, Kotlin/JVM target 17
- Bazel 9.3.1 via Bazelisk

Primary sources verified during setup:

- https://github.com/bazel-contrib/rules_kotlin/releases/tag/v2.4.20
- https://github.com/bazelbuild/rules_android/releases/tag/v0.7.3
- https://bcr.bazel.build/modules/rules_jvm_external/7.1
- https://github.com/ZacSweers/metro/blob/main/docs/compatibility.md
- https://github.com/ZacSweers/metro/blob/main/docs/dependency-graphs.md
- https://connectrpc.com/docs/protocol
- https://developer.android.com/privacy-and-security/keystore
- https://developer.android.com/privacy-and-security/security-config
- https://developer.android.com/guide/topics/data/autobackup

## Known limitations

- No phone-side notifications, FCM, or background execution. Reminder delivery to
  the phone is a future optional channel; the server remains authoritative.
- No offline cache: lists and actions require the server. Failures are shown as
  offline/auth/timeout/TLS/server errors rather than stale fake data.
- No generic task/scenario UI; no live HA scheduler cutover.
- Runtime UI is not yet validated on a physical device or emulator. Robolectric
  covers a cold launch of the real app graph/Activity, root-fragment attachment,
  token-clear storage, and Fragment lifecycle/queue restoration (14 navigation
  tests); full interactive flows remain device-test work.
- The app is a single deployment composition; a future multi-module project can
  reuse `platform/android` macros and `packages/kotlin/core`.
