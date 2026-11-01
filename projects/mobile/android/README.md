# Home Android client

Kotlin/Android management client for the home server. The server owns
scheduling and execution; the phone manages server state and reads the Home
catalog. One Activity, Fragment screens, Metro compile-time DI, feature
API/implementation modules, and a typed router.

## Limits

Debug-signed only, with a stable personal key so upgrades keep app data and the
encrypted token (see [Signing](#signing)). No FCM messaging, phone-side
background execution, offline cache, or generic task UI. Current device
acceptance and remaining work are tracked in [STATE](../../../docs/STATE.md) and
[PENDING](../../../docs/PENDING.md).

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
  - **Server address** (scheduler): LAN `http://<node>:30098` or the home
    HTTPS name `https://alarms.<HOME_DOMAIN>` reached over the VPN.
  - **Home control address** (catalog): LAN `http://<node>:30099` or the home
    HTTPS name `https://control.<HOME_DOMAIN>` reached over the VPN.
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
  verifier, or certificate pinning; the system chooses LAN or VPN routing.

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
bazelisk test //platform/android:google_services_test

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

## Signing

Local builds are signed with a stable personal debug key so that reinstalling
over an existing build keeps app data, including the Keystore-encrypted bearer
token. The key is **private and never committed**: it lives in
`secrets/android/home-debug.keystore`, which is outside Bazel's workspace
coverage (`secrets/` is in `.bazelignore`) and reached through the gitignored
symlink `platform/android/keys/home-debug.keystore`. Back the file up; losing it
means one uninstall and re-entering the token.

Create the key once (password and alias are fixed by the Bazel signing flow):

```sh
keytool -genkeypair \
  -keystore secrets/android/home-debug.keystore \
  -storetype PKCS12 -storepass android -keypass android \
  -alias androiddebugkey -keyalg RSA -keysize 4096 -validity 10950 \
  -dname "CN=Home Debug"

mkdir -p platform/android/keys
ln -s "$PWD/secrets/android/home-debug.keystore" \
  platform/android/keys/home-debug.keystore
```

`//platform/android:home_debug_keystore` copies that file when it exists and
otherwise falls back to the public `@rules_android` debug keystore, printing a
warning to stderr, so CI and clean checkouts still build. `android_binary` is
invoked with `--ks <file> --ks-pass pass:android` and no alias, so the keystore
must hold exactly one entry with password `android` (the command above does).

Verify the signer of the APK you are about to install:

```sh
"$ANDROID_HOME/build-tools/36.0.0/apksigner" verify --print-certs \
  bazel-bin/projects/mobile/android/app/app.apk
# Signer #1 certificate SHA-256 digest:
# 88ef5d61a365afbf15d290168d4ee7fd20a80be13ea4d4667ef1e521a2a504ad
```

That SHA-256 is the personal key's. The fallback key has a different digest, so
**install only builds signed with the personal key**: local builds, and CI
builds from `android-distribute` (which materialises the same key from
OpenBao). Other machines' builds without the key use the public fallback and
will not update in place. Moving from the old debug key to this one requires
one uninstall (app data, including the token, is cleared).

## Firebase (Crashlytics)

Crash reporting is the only Firebase service. There is no FCM, Analytics,
Remote Config, or phone-side scheduling.

**What is sent to Google:** crash stack traces, device model and OS version,
and the app version. The app does not send the bearer token, scheduler or
control endpoints, Home catalog contents, or any custom keys. Crashlytics uses
its own Firebase installation id, not the app's Home credentials.

**Build wiring without Gradle.** `google-services.json` is private and lives in
`secrets/android/`; Bazel reads it through the gitignored symlink
`platform/android/keys/google-services.json`:

```sh
mkdir -p platform/android/keys
ln -s "$PWD/secrets/android/google-services.json" \
  platform/android/keys/google-services.json
```

`platform/android/google_services.py` converts the JSON client for
`dev.proflyder.home` into `res/values/google_services.xml` (`google_app_id`,
`gcm_defaultSenderId`, `google_api_key`, `google_crash_reporting_api_key`,
`google_storage_bucket`, `project_id`) and the Crashlytics build id
`com.google.firebase.crashlytics.mapping_file_id` (32 hex, derived from the app
id and version). Without the link the build still succeeds, prints a warning,
emits an empty values file, and Firebase stays uninitialized; the app must and
does start normally in that state.

Collection follows the `home_crash_reporting` bool (default `true`). Turn it
off to keep Crashlytics initialized without uploads.

**One-time console steps:**

1. In the Firebase console for the existing project, open **Crashlytics** and
   finish onboarding (the Android app `dev.proflyder.home` already exists in
   the project).
2. Wait for the first crash to appear; Crashlytics enables itself when the
   first report arrives.

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
to Firebase App Distribution. It runs both locally and from the
`android-distribute` workflow on a push to `develop`.

CI (`.github/workflows/android-distribute.yml`) uses the GitHub Environment
`Android` and reads `secret/ci/android-distribute` from OpenBao through the
`android-distribute` JWT role. It materialises the keystore and
`google-services.json` into `platform/android/keys/` (mode 600) and the
Firebase service account into `$RUNNER_TEMP/ci/firebase-sa.json`, disconnects
the VPN, then runs:

```sh
projects/mobile/android/distribute.sh --ci
```

`--ci` formats release notes as the squash-merge commit subject, the short SHA
and the workflow run URL. The APK and the signer check are identical to a local
run, and every push uploads to the `owner` tester group.

Local:

```sh
export ANDROID_HOME="$HOME/Library/Android/sdk"
projects/mobile/android/distribute.sh --dry-run   # print the command, no auth
projects/mobile/android/distribute.sh             # build + upload
```

The script requires both private links (`home-debug.keystore` and
`google-services.json`), refuses to upload unless the APK signer is the
personal key `88ef5d61…04ad`, and reads the Firebase app id from the JSON.
Authentication uses an existing `npx firebase-tools@15.34.2 login` session or
`GOOGLE_APPLICATION_CREDENTIALS` pointing at a service-account JSON under
`secrets/android/`; credentials are never printed.

Each upload gets `versionCode` = UTC epoch seconds and
`versionName` = `0.1.0-<shortsha>`. Epoch seconds are strictly increasing per
build and fit a 32-bit versionCode until 2038; a git commit count is not
monotonic for dirty builds. Ordinary builds keep versionCode 1 / 0.1.0. Both
are build settings (`--define home_version_code=… --define
home_version_name=…`), so manual uploads can pass the same values.

**One-time console steps:**

1. Open **App Distribution** in the Firebase console and choose **Get
   started**.
2. Create the tester group `owner` (or set `HOME_TESTERS_GROUP`) and add your
   own email address.
3. Run `npx firebase-tools@15.34.2 login` once on a local machine that
   distributes outside CI.
4. For CI, create a GCP service account with the **Firebase App Distribution
   Admin** role and a JSON key, then store it (with the keystore and
   `google-services.json`) in OpenBao `secret/ci/android-distribute` — see
   [OpenBao](../../home/kubernetes/openbao/README.md).

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

- Application ID `dev.proflyder.home`; default version 0.1.0 (versionCode 1).
  `home_android_binary` reads `home_version_code`/`home_version_name` from
  `--define`; `distribute.sh` overrides them for App Distribution.
- minSdk 26, target/compile SDK 36, Kotlin/JVM target 17.
- Only `android.permission.INTERNET`.
- Debug-signed with the personal debug key when present; debug APKs are not
  byte-reproducible. See [Signing](#signing).
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
