# Home Android client

Kotlin/Android management client for the home server. The server owns
scheduling and execution; the phone manages server state and reads the Home
catalog. One Activity, Fragment screens, Metro compile-time DI, feature
API/implementation modules, and a typed router.

## Limits

Debug builds only. App Distribution builds are release-signed with a stable
personal key so upgrades keep app data and the encrypted token (see
[Signing](#signing)); the APK Bazel produces is an intermediate. No FCM
messaging, phone-side background execution, offline cache, or generic task UI.
Current device acceptance and remaining work are tracked in
[STATE](../../../docs/STATE.md) and [PENDING](../../../docs/PENDING.md).

## Features

- Bottom tabs: **Home**, **Scenarios**, **Time**. **Time** is a container with an
  in-screen switch for **Alarms**, **Timers** and **Reminders**; the existing
  feature screens are reused unchanged and deep links such as
  `selectTab("alarms")` open the Time tab on the matching section. **Connection**
  is not a tab; the settings icon (⚙) in the top app bar opens it as an overlay
  over the current tab, and Back returns to that tab.
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
- Connection: scheduler and optional Home control base URLs plus connect/read
  timeouts come from Firebase Remote Config (`home_connection`); the bearer
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
- Devices are lights/switches. HA scripts are no longer shown on Home; native
  scenarios live in the **Scenarios** tab and in Quick actions.
- The **live carousel** at the top shows only relevant cards and is hidden when
  there is nothing to show: the nearest enabled alarm (or the ringing one, with
  **Stop**/**Snooze**), every running/paused timer with a local one-second
  countdown, the next native scenario (`next_due`, **Run now**) and the next
  reminder. Tapping a card opens its section; card actions are sent to the
  server. Data comes from the existing feature repositories through their API
  modules. The hub refreshes on resume and pull-to-refresh.
- **Quick actions** are pinned tiles. Long-press a device in a room group or a
  native scenario in the scenario list to **Pin to Home** / **Unpin**. Tiles are
  ordered by pin time and kept in DataStore. A device tile toggles on tap (a
  light opens its brightness/color sheet on long press), a scenario tile runs it,
  and the "Say on the station" tile opens the speak dialog. On first launch the
  speak action and the native **Blackout** / **Work mode** scenarios (when they
  exist) are pinned. With no pins the screen shows
  "Long-press a device to pin it here."
- Filter chips (All / Lights / Switches, plus room filters) are persisted in
  DataStore and restored on launch. Room sections are collapsible; the folded
  state is UI-only.
- The **voice profile** (an HA switch carrying the `voice_profile` label, shown
  as `kind: "profile"`) stays pinned above the rooms: two segmented buttons,
  **Personal** (`turn_off`) and **Work** (`turn_on`). Row taps do not act, only
  an explicit side does.
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

The Home screen, profile strings and the Scenarios feature are localized in
English and Russian (`values-ru`); other screens fall back to English for
strings without a translation.

## Scenarios

The **Scenarios** tab is a builder for the scheduler's durable
`home.v1.ScenarioService` definitions. The server owns storage, validation,
time-trigger materialization, and (once the `ha-scenario` executor exists)
execution; the phone only edits the definition and queues manual runs.

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

## Connection and networking

The Connection screen is reached from the settings icon (⚙) in the top app bar.
It opens as an overlay on the current bottom tab; Back returns to that tab.

- Two base URLs, no paths, supplied by Remote Config:
  - **Server address** (scheduler): LAN `http://<node>:30098` or the home
    HTTPS name `https://alarms.<HOME_DOMAIN>` reached over the VPN.
  - **Home control address** (catalog, optional): LAN
    `http://<node>:30099` or the home HTTPS name
    `https://control.<HOME_DOMAIN>` reached over the VPN.
  - The app appends the Connect RPC path or `/v1/catalog` itself. Do not put
    `/v1/catalog` into the configured URL.
- Connect and read timeouts also come from Remote Config. Invalid URLs,
  malformed JSON, or out-of-range timeouts are shown as an explicit invalid
  state; the app never crashes and no request is attempted.
- The same bearer token is used for scheduler and control. Endpoints and
  timeouts are deployment-specific and are set only in the Firebase console;
  none are embedded in the app or the repository. The token remains
  device-local and is never part of Remote Config.
- Transport: Retrofit on OkHttp, Connect unary JSON
  (`POST {endpoint}/home.v1.SchedulerService/{RPC}`), protobuf JSON field
  names, `int64` as decimal strings. `Authorization: Bearer` is sent in the
  header only.
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

Prerequisites: macOS/Linux, JDK 21, Android SDK with platform 36 and
build-tools 36.0.0 (`ANDROID_HOME`), Bazelisk. The root contract is
`MODULE.bazel` (no Gradle, no `WORKSPACE`).

```sh
cd /path/to/yandex-station-lab
export ANDROID_HOME="$HOME/Library/Android/sdk"
mise install   # Bazelisk and the rest of mise.toml

mise exec -- bazelisk build //projects/mobile/android:app
# bazel-bin/projects/mobile/android/app/app.apk

bazelisk test //projects/mobile/android:android_tests
# Bazel build tools are Kotlin/JVM: unit tests for the generators and lint runner.
bazelisk test //platform/android/...

# Android Lint over the whole app graph (separate CI job `android / android-lint`).
bazelisk test //projects/mobile/android:android_lint

# Optional: install on a connected device (keeps app data).
adb install -r bazel-bin/projects/mobile/android/app/app.apk
```

Android Lint is **not** part of `android_tests`: it runs as its own CI unit
(`android / android-lint`) and must also be run explicitly with
`bazelisk test //projects/mobile/android:android_lint`. Keeping it separate
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

**Zero-warning policy.** `bazelisk build //projects/mobile/android:app` and
`bazelisk test //projects/mobile/android:android_tests` must not print
`warning:`/`WARNING:` from this repository's code, resources, manifests or
Bazel rules. Fix the root cause; do not silence the log. Warnings that come
from a pinned third-party toolchain (for example javac deprecations inside
protobuf/rules_android sources) are tracked in the change that introduces them and reported rather
than hidden.

`android_tests` covers endpoint/security rules, time/duration parsing, Connect
JSON mapping and failures, idempotency, navigation queue/restoration, the real
Metro graph via a Robolectric cold launch, a Robolectric smoke of the real
profile row (`//projects/mobile/android/features/control/impl:profile_row_test`),
the Home hub model (carousel selection/order, "tomorrow 07:30" text, favorites
pin/order/defaults, display-name rule) in
`//projects/mobile/android/features/control/api:api_test`, a Robolectric smoke
of the real bottom bar and the Time-tab deep link in
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

Local `bazel build //projects/mobile/android:app` still produces an APK signed
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

## Firebase (Crashlytics and Remote Config)

Firebase provides crash reporting and Remote Config. There is no FCM,
Analytics, or phone-side scheduling. Remote Config supplies only connection
settings; it never supplies the bearer token.

**What is sent to Google:** Crashlytics sends crash stack traces, device model
and OS version, and the app version. Remote Config requests carry the app's
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
| `connectTimeoutMs` | number | no | Connect timeout, default 5000, valid 1000–120000. |
| `readTimeoutMs` | number | no | Read timeout, default 20000, valid 1000–120000. |

Example value (illustrative only; use your own private deployment values):

```json
{
  "schedulerBaseUrl": "https://alarms.example.test",
  "controlBaseUrl": "http://192.0.2.28:8099",
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

CI runs on **every pull request that changes the Android app**:
`.github/workflows/ci.yml` job `android-distribute` needs the `changes` routing
and the `android` build, and starts only when the router sets
`android_distribute` (an actual `projects/mobile/android/`, `packages/kotlin/`
or `platform/android/` change). A global invalidation such as `MODULE.bazel`
still runs the `android` checks but does not publish a build. The job:

- runs only for pull requests whose head branch lives in this repository: a
  fork PR gets a read-only token without Environment secrets, so it can never
  read the signing material;
- uses the GitHub Environment `Android` and reads
  `secret/ci/android-distribute` from OpenBao through the `android-distribute`
  JWT role, whose claims are pinned to pull-request merge refs of `ci.yml`
  (`refs/pull/*/merge`);
- materialises `google-services.json` into `platform/android/keys/` (mode 600)
  and the release key, its password files, alias, expected certificate digest
  and the Firebase service account into `$RUNNER_TEMP/ci` (mode 600/700);
  passwords stay in files and never enter the environment of later steps,
  disconnects the VPN, then runs:

```sh
projects/mobile/android/distribute.sh --ci
```

- builds the APK with Bazel (debug-signed intermediate, no personal key in CI),
  signs the final APK with `platform/android/sign-release.sh` and uploads that
  artifact;
- is deliberately **not** part of `ci-ok`: a failed upload is visible on the PR
  and in the run, but it does not block the merge. A newer push to the same PR
  cancels the upload in flight.

`--ci` formats release notes as `PR #<number>: <title>`, the short head SHA and
the workflow run URL (number and title arrive as environment variables, never
as script source). The signing material, the certificate check and the
`admins` tester group are identical to a local run.

Local:

```sh
export ANDROID_HOME="$HOME/Library/Android/sdk"
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
home_version_name=…`), so manual uploads can pass the same values.

**One-time console steps:**

1. Open **App Distribution** in the Firebase console and choose **Get
   started**.
2. Create the tester group `admins` (display name «Admins») (or set `HOME_TESTERS_GROUP`) and add your
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
`SetScenarioEnabled`, and `RunScenario`. Home catalog reads use
`GET /v1/catalog`.

## App identity and permissions

- Application ID `dev.proflyder.home`; default version 0.1.0 (versionCode 1).
  `home_android_binary` reads `home_version_code`/`home_version_name` from
  `--define`; `distribute.sh` overrides them for App Distribution.
- minSdk 26, target/compile SDK 36, Kotlin/JVM target 17.
- Only `android.permission.INTERNET`.
- Bazel output is debug-signed (personal key when present, public fallback
  otherwise); App Distribution APKs are release-signed by
  `sign-release.sh` with the same personal certificate. APKs are not
  byte-reproducible. See [Signing](#signing).
- Browsable diagnostics (logcat tag `HomeHttp` + Chucker) are gated by the
  `home_network_inspection` bool and should be disabled in release variants.

## Known limitations

- No phone-side notifications/FCM, no background execution, no offline cache.
- Scenario execution still depends on the server-side `ha-scenario` executor;
  the app only edits definitions and queues manual runs.
- Full interactive acceptance on a device (filters, scenario taps, device
  toggles, alarm/timer flows) is still pending; automated input is blocked by
  HyperOS.
- `EnqueueTask` is one-shot; recurring generic tasks and event-trigger rules are
  not implemented.
