# Home Android client

Kotlin/Android management client for the home server. The server owns
scheduling and execution; the phone manages server state, reads the Home
catalog and is an optional delivery channel (push, find phone). One Activity,
Fragment screens, Metro compile-time DI, feature API/implementation modules,
a typed router. Device acceptance and open work: [STATE](../../../docs/STATE.md),
[PENDING](../../../docs/PENDING.md).

## Features

- Bottom tabs: **Home** (catalog, live rows, quick-actions dial),
  **Scenarios** (builder for `home.v1.ScenarioService`), **Time** (pager over
  Alarms / Timers / Reminders), **Chat** (Hermes, one server-side
  conversation), **System** (home dashboard from `mcp-gateway`).
- Top app bar: **Call** (audio call to the station) and **Settings** (overlay
  over the current tab): Connection, Phone notifications and find, VPN home,
  Notification history, Appearance; debug builds also show **Design system**.
- Access follows the token: the app reads `GET /v1/me` on start, when the
  bearer token changes, on the Access screen's pull-to-refresh, after a
  `SyncSignal.Stale` foreground return (the 5-minute staleness window) and at
  most once per 3 seconds after a `403 module_forbidden`. It hides tabs,
  settings rows, quick tiles, toolbar actions and the action menu whose module
  the token lacks. Besides the HTTP body `{"error":"module_forbidden"}` that
  refresh also accepts the Connect `home.v1.ModuleForbidden` detail, and the
  screen renders the localized no-access state, not a generic server error.
- **System → Access** (only with the `admin` module) lists named application
  tokens with their modules, creates a named token with module switches
  (the plaintext value is shown once with Copy), renames tokens, toggles
  modules and revokes them with confirmation. The owner token is immutable;
  `400`, `409` and `429` have dedicated localized messages.
- Push over FCM: ordinary notifications with a `home://` deep link and the
  loud find-phone ring. Built-in VPN home (WireGuard, OpenConnect hand-off).
- No offline cache and no generic task UI. `EnqueueTask` is one-shot; the
  phone only queues, the server executors (`ha-control`, `ha-scenario`) run.

## Build and test

Prerequisites: macOS/Linux, JDK 21, Android SDK platform 36 and build-tools
36.0.0, NDK `28.2.13676358` (`ANDROID_HOME`, `ANDROID_NDK_HOME` or `ndk/`
under the SDK), tools from `mise.toml`. The APK targets `arm64-v8a` via
`//platform/android:android_arm64`. Root contract is `MODULE.bazel`: no
Gradle, no `WORKSPACE`.

```sh
mise install
bazelisk build --config=android //projects/mobile/android:app
bazelisk test  --config=android //projects/mobile/android:android_tests
bazelisk test  --config=android //platform/android/...
bazelisk test  --config=android //projects/mobile/android:android_lint
bazelisk test  --config=android //projects/mobile/android:detekt
adb install -r bazel-bin/projects/mobile/android/app/app.apk
```

- **Android Lint** is not part of `android_tests`, but CI runs it in the
  same `android / android-app` job and the same `bazel test --keep_going`
  call as the APK, the tests and detekt, so compilation happens once and a
  finding does not hide test results: the job summary lists every failing
  test, Lint and detekt target together. It uses
  the SDK `cmdline-tools;23.0` Lint CLI (`@android_lint_sdk`, a symlink of
  `ANDROID_HOME` whose jars are action inputs) with partial analysis: one
  cacheable `AndroidLintAnalyze` action per module (`--analyze-only` over that
  module's lint model: its sources, resources, manifest, compile classpath and
  the resources/jars of its module dependencies), then the test runs
  `--report-only` over all module models and fails on any error or warning
  (`-Werror`). Editing one module re-analyzes only that module.
  `lint.xml` is not a baseline; `tools:ignore`/`@Suppress` only for a
  documented false positive, scoped to the narrowest element. The two
  `Overdraw` path ignores in `lint.xml` exist because the Overdraw detector
  reads the application theme only from a manifest visited in the same Lint
  process; per-module analysis of a library cannot see the app manifest and
  falls back to `Theme.Holo`. The `UnusedResources` ignore covers only the
  generated `app/res/values/google_services.xml`: CI lints the app with the
  real `google-services.json` (same job as the release APK), and the Firebase
  SDK reads those strings by name at runtime, so Lint cannot see their use.
- **detekt** (`android / android-app`, same Bazel call): `detekt.yml` plus the first-party
  `home-rules` plug-in in `platform/android/detekt-rules`. There is no
  baseline; every finding fails.

  | Rule | Enforces |
  | --- | --- |
  | `NoTry` | no `try`/`catch`/`finally`; use `runCatching`, `suspendRunCatching` (rethrows cancellation), `use {}` |
  | `NoElseIf` | no `else if` chains |
  | `LongLambda` | short lambdas |
  | `NoHardcodedUiText` | UI text only from resources |
  | `NoInlineJsonInTests` | test payloads in `src/test/resources/fixtures/*.json`, read via `core/testing` `TestFixtures` |
  | `PublicInImpl` | top-level declarations in `*/impl/src/main/` are `internal`/`private` (exceptions: [Metro and internal](#metro-and-internal)) |
  | `NoDirectStateWrite` | a `BaseViewModel` changes state only through `updateState` |
  | `NoUiOwnedControlState` | UI never flips `isChecked`/`isSelected`/`toggle()` itself |

- **Zero warnings.** Building the app and `android_tests` must not print
  `warning:` from first-party code, resources, manifests or rules (`warn =
  "error"`). The only silenced warnings are javac's for pinned third-party
  exec tools (`--host_javacopt=-Xlint:none -nowarn` in
  `platform/android/android.bazelrc`).
- `android_tests` include the real Metro graph (`AppGraphBindingsTest`
  resolves every ViewModel and startable), Robolectric smokes and screenshot
  tests (PNG in `TEST_UNDECLARED_OUTPUTS_DIR`) and an APK packaging gate:
  required runtime classes are defined in DEX and the deploy jar has no
  unresolved non-platform references. Tests do not replace device acceptance;
  no test places a live call.
- `platform/android/sign-release_test.sh` self-tests release signing with a
  throwaway key in the `android / android-app` unit.

## Android Studio

Plugin: **Bazel for Android Studio** (ASwB, `com.google.idea.bazel.aswb`,
[Marketplace](https://plugins.jetbrains.com/plugin/9185-bazel-for-android-studio)).
Its build must match the IDE build exactly (Settings → Plugins → Marketplace
picks it; AS 2026.2.2 = `262.10968.63`). Since 2025 ASwB is developed in AOSP
and ships only query sync; the JetBrains Bazel plugin does not support Android
targets, and Gradle sync is not used here.

1. Make the SDK visible to the IDE process (it runs Bazel itself; without
   `ANDROID_HOME` `bazel query` fails on `@androidsdk//:aapt2`):
   `launchctl setenv ANDROID_HOME ~/Library/Android/sdk` and
   `launchctl setenv ANDROID_NDK_HOME ~/Library/Android/sdk/ndk/28.2.13676358`,
   then restart Android Studio (or start it from a shell with both set).
2. Settings → Bazel Settings → Bazel binary: `bazelisk` from `mise`
   (`mise which bazelisk`).
3. File → Import Bazel Project → workspace = repository root → *Import project
   view file* → `projects/mobile/android/.bazelproject`.
4. Sync. Highlighting comes from source; to resolve generated code (R,
   view binding, Metro) and dependencies run *Bazel → Build dependencies* on
   a file or *Enable analysis* for the working set. The plugin writes its aspect
   into `.aswb/` (ignored by Git) and builds with `--config=android`.
5. Run/Debug: Bazel run configuration for `//projects/mobile/android/app:app`
   (`android_binary`, ABI `arm64-v8a`); unit tests via the gutter icon or a
   Bazel test configuration for a `kt_android_local_test`/`kt_jvm_test` target.

Known limitations: no Gradle-style Build Variants, Compose preview or Layout
Inspector live updates; generated code appears only after *Build
dependencies*; the first dependency build is a full Android build (minutes);
an ASwB update after an IDE update is required, the plugin is pinned to one
IDE build.

## Signing

`rules_android` signs only with `--ks-pass pass:android` and no alias, so the
release key never enters Bazel. The APK from Bazel is an intermediate.

- **Release** (App Distribution): `platform/android/sign-release.sh <in> <out>`
  runs `zipalign -P 16 -f 4`, `apksigner` v2+v3 and **fails closed** unless the
  signer certificate SHA-256 equals `SIGNING_CERT_SHA256`. Inputs:
  `SIGNING_KEYSTORE`, `SIGNING_STORE_PASSWORD_FILE` (or `_PASSWORD`),
  `SIGNING_KEY_PASSWORD_FILE` (defaults to the store password),
  `SIGNING_KEY_ALIAS` (single alias when unset), `ANDROID_BUILD_TOOLS`.
  Passwords reach the tools only as `file:` references, never argv or logs.
  The key lives in OpenBao `ci/android-distribute` (fields:
  [SECRETS](../../../docs/SECRETS.md)); locally `mise run
  admin:android-distribute` passes it through fnox profile `android`.
- **Local debug**: when `platform/android/keys/home-debug.keystore` exists
  (gitignored symlink to a Bazel-only copy of the same key with password
  `android`), local APKs carry the personal certificate and `adb install -r`
  keeps app data. Without it Bazel falls back to the public debug key with a
  warning, and that APK cannot update an existing install.

  ```sh
  keytool -importkeystore \
    -srckeystore secrets/android/home-release.keystore \
    -srcstorepass:file secrets/android/home-release.store.password \
    -destkeystore secrets/android/home-debug.keystore \
    -deststoretype PKCS12 -deststorepass android -destkeypass android \
    -srcalias home-release -destalias androiddebugkey -noprompt
  ln -s "$PWD/secrets/android/home-debug.keystore" platform/android/keys/home-debug.keystore
  ```

- Check any APK with `apksigner verify --print-certs <apk>`; the personal
  certificate digest is `88ef5d61…04ad`. The certificate must never change:
  a new key forces an uninstall and loses the encrypted token.

## Distribution

`projects/mobile/android/distribute.sh` builds, verifies and uploads to
Firebase App Distribution (tester group `admins`, or `HOME_TESTERS_GROUP`).

- **CI**, on every PR that changes `projects/mobile/android/`,
  `packages/kotlin/` or `platform/android/`: `_android.yml` builds the APK once
  without secrets, with the release `versionCode`/`versionName`, runs
  `bazel test` with the same `--define`s and uploads `android-app-apk`
  (`app.apk` + `release-info.json`). `_android-release.yml` (Environment
  `Android`, OpenBao role `android-distribute`) waits for the whole `android`
  matrix, signs, and runs `distribute.sh --upload-only`. Distribution is not
  part of `ci-ok`. A global invalidation (e.g. `MODULE.bazel`) runs the
  checks but publishes nothing. `google-services.json` comes from the
  repository variable `GOOGLE_SERVICES_JSON_B64` (not a secret).
- **Local**: `mise run admin:android-distribute -- --dry-run`, then without
  `--dry-run`. Firebase auth: an existing `npx firebase-tools@15.34.2 login`
  or `GOOGLE_APPLICATION_CREDENTIALS`.
- **Versions**: uploads use `versionCode` = UTC epoch seconds (monotonic, fits
  32 bits until 2038) and `versionName` = `0.1.0-<shortsha>`; ordinary builds
  keep 1 / 0.1.0. Both are `--define home_version_code/home_version_name`,
  defaulted in `platform/android/android.bazelrc`.
- **Retention**: the daily `fad` job of `_cleanup.yml` deletes releases older
  than 24 h per app, always keeping the newest. Role `android-cleanup`; the
  service account needs `roles/firebaseappdistro.admin`. Dry run on a
  fixture: `node platform/ci/fad_cleanup.mjs --fixture
  platform/ci/testdata/fad-releases.json --now 2026-11-04T12:00:00Z`.

## Firebase and connection settings

Firebase provides Crashlytics, Remote Config and FCM; no Analytics. Crashlytics
sends stack traces, device model/OS and app version; collection follows the
`home_crash_reporting` bool. The bearer token, catalog contents and custom keys
are never sent to Google.

`google-services.json` is private (`secrets/android/`); Bazel reads it through
the gitignored symlink `platform/android/keys/google-services.json`, and
`platform/android/tools/google_services` turns it into
`res/values/google_services.xml` plus the Crashlytics mapping id. Without the
link the build succeeds with a warning and the app runs with Firebase
uninitialized.

Remote Config parameter `home_connection` (String, in-app default empty =
not configured) holds the endpoints; it never holds the token. Unknown keys
are ignored.

| Field | Required | Meaning |
| --- | --- | --- |
| `schedulerBaseUrl` | yes | scheduler base URL, no RPC path |
| `controlBaseUrl` | no | Home control (catalog, chat, system); empty disables those tabs |
| `intercomBaseUrl` | no | intercom signaling; empty disables calls |
| `connectTimeoutMs` / `readTimeoutMs` | no | default 5000 / 20000, valid 1000–120000 |

```json
{
  "schedulerBaseUrl": "https://alarms.example.test",
  "controlBaseUrl": "http://192.0.2.28:8099",
  "intercomBaseUrl": "https://intercom.home.example.test",
  "connectTimeoutMs": 5000,
  "readTimeoutMs": 20000
}
```

Real values are set only in the Firebase console. Base URLs carry no paths:
the app appends the Connect RPC path or `/v1/...` itself. Invalid JSON, URL or
timeout is an explicit invalid state on the Connection screen, never a crash
or a request. The last loaded values stay usable when a fetch fails.

The one bearer token (scheduler, control, intercom, push and VPN
registration) is entered on the phone and stored only as ciphertext under a
non-exportable Keystore AES-256/GCM key in the app DataStore. If decryption
fails the token must be re-entered; there is no plaintext fallback. Backup and
device transfer are disabled; tokens never go into URLs, `SavedState`,
arguments or logs, and diagnostics redact `Authorization`.

## Architecture

```text
projects/mobile/android/
  app/                    single Activity (chrome only), AppGraph, diagnostics
  core/
    model/                Android-free domain: Outcome/RemoteFailure, Clock, ServerClock, DeviceIdentity
    data/                 EntityStore, EntitySource, PagedList, FeatureScope (Android-free)
    runtime/              app scope, IoDispatcher, AppStartable, AppForeground, tickerFlow, Backoff
    config/ remoteconfig/ connection contract and its Firebase source
    settings/             DataStore + Keystore token
    network/{api,impl}    HomeHttp (Retrofit/OkHttp), Connect clients, /v1/me and access admin, RouteGate
    access/               AppAccessSession: token modules and module gating (Android-free)
    navigation/           typed routes, FeatureEntry, FragmentRouter, AppChromeState, DeepLinkResolver
    ui/ presentation/     base fragment/adapter, insets, StateViewModel, ScreenFragment, binders
    theme/ designsystem/  themes; every visual component (see core/designsystem/README.md)
    testing/              TestFixtures
  features/<name>/{api,impl}
packages/kotlin/core/     shared Kotlin utilities
platform/android/         Bazel macros, Metro wiring, APK gate, signing, detekt rules
```

### Paged lists

Cursor lists are assembled from core pieces; a screen declares only its model,
its source and its own actions.

1. The model implements `Keyed` (`core/model`); no `keyOf` lambdas.
2. The data source implements `PagedSource<T>` (`core/data`) and returns
   `Page<T>(items, next)`; `PagedList` owns the only
   `StateFlow<PagedState<T>>`. `PagingReducer` is pure: `Refresh`, `Reload`
   (merge the first page into the loaded tail), `LoadMore`, `Retry`; transitions
   and `LoadState` are tested without coroutines.
3. The repository implements `PagedSource`, owns `PagedList`, and exposes domain
   commands. Mutations are `ListEdit.Update` / `Remove` / `UpdateAll` committed
   through `PagedList.commit`, which rolls back only the edited keys on failure.
   `hide(key)` filters an item from the current state and from every incoming
   page (pending delete); `reveal(key)` restores it; `drop(key)` removes a
   server-confirmed item without hiding its key. Sync signals dispatch
   `PagingAction.Reload` or `drop`, never a full reset.
4. The screen ViewModel extends `PagedStateViewModel<T, S, I, E>` where `S :
   PagedScreenState<T>` (`pages`, `pendingRemoval`). The base owns
   `UndoableRemoval` and a private `PagingHandler`, and exposes
   `dispatch(PagingIntent)`, `dispatch(ListItemIntent)` and the normal
   `dispatch(I)`; every action enters the ViewModel. The screen intent keeps
   only its own actions (Open/ToggleRead/ReadAll/Clear). The base starts
   collection itself in `init` and receives a `derive: (pages, pendingRemoval)
   -> S` lambda in its constructor; the screen derives rows/day sections and
   `hasUnread` there with its injected `Clock`, so the fragment only renders
   slices. The lambda must capture constructor parameters/locals, never fields
   initialised after `super`; then `Dispatchers.Main.immediate` is safe and no
   `startPaging()` call is needed. A failed removal commit (revealed item) maps
   to `E` via `pagingFailure`.
5. The fragment wires the list with one `PagingBinder` call (RecyclerView,
   `ListStateViews`, content adapter, failure text, `PagingIntent` sink,
   optional `SwipeRefreshLayout`) and renders with
   `render(S::pages, binder::render)`; the binder has no flow subscription and
   no second state channel. `UndoBinder` renders `pendingRemoval` through
   `UndoNotice` and sends `ListItemIntent.Undo`. `PagingBinder` lives in
   `core/presentation` next to the shared intents, because `core/ui` cannot
   import them without a dependency cycle; it only uses core-ui views and the
   footer adapter. It installs `ConcatAdapter(content, PagingFooterAdapter)`,
   `loadMoreNearEnd`, retry and loading/empty/error rendering. Swipes are
   declared with `SwipeAction(direction, SwipeStyle, onSwiped)`/
   `swipeAction(...)` and `SwipeActions(left, right, itemAt)`;
   `SwipeStyle.Delete`/`ToggleRead` keep icons and theme attrs in core/ui, and
   each action calls `viewModel.dispatch` itself. The callback resets the
   holder after every swipe and draws only the active action background, so no
   plaque remains.

The screen state keeps `PagedState<T>` as one field; day sections, counters and
badges are selectors in `render`.

### Module rules

- A feature is `api` (contracts, pure rules) + `impl` (data, domain,
  presentation). An impl depends only on its own api, core modules and
  `packages/kotlin`; no impl imports another impl. `app` is the composition
  root and the only module that links every impl.
- Features and `app` depend on `core/network/api`; `app` links
  `core/network/impl` so Metro sees its contributions. `HomeHttp` is the only
  Retrofit/OkHttp factory: redirects off (a bearer never follows a
  `Location`), only `@Idempotent` calls are retried, streams (chat SSE,
  intercom `Watch`) have no read timeout and no inspection interceptors.
  A DNS or connection-establishment failure is retried for about 2 s
  (three attempts) before it becomes `RemoteFailure.Offline`, so a lookup that
  fails right after resume or a network switch does not flash "offline".
- `RouteGate.await()` holds every request until all `RouteCondition`s pass
  (network present, settings loaded, VPN tunnel not switching), at most 20 s.
  A retry of an `@Idempotent` call waits for the gate again, and reopening
  the gate after a tunnel switch evicts the OkHttp connection pool
  (`ConnectionHealth`), so a refresh right after VPN up/down succeeds.
- VPN auto-connect yields to the user: a manual on/off (toolbar, tile, VPN
  screen, notification) is stored in `UserVpnIntent` (one `StateFlow`, bound
  to the physical networks it was made on). While it holds, the policy does
  nothing; a different network clears it and the policy decides again.
- Every tunnel start carries a `ConnectOrigin`. `USER` always runs;
  `AUTO_POLICY` runs only when the network is classified `Away` at start;
  `NETWORK_RESTART` (endpoint change after a network switch) needs a known
  network. Only `USER` failures away from home show the "no handshake"
  fallback and its notification; an `AUTO_POLICY` failure is logged and ends
  as `Disconnected`, a failure once the network turns out to be home is
  `NotNeededAtHome`, a failed restart quietly ends as `Disconnected`.
- Android restarts the WireGuard `VpnService` with an empty intent after the
  process is killed (sticky service) and for always-on VPN; both arrive as
  `SystemVpnStarts`. They never connect directly: `AutoConnectCoordinator`
  waits up to 15 s for the environment and asks the same `AutoConnectPolicy`
  (background: only "auto-connect away" and only `Away`, a manual off holds).
- A cold process start starts `AppStartable`s but not user data work: the
  auto-connect watcher and every network startable (`/v1/me`, Remote Config,
  push registration and token fetch, VPN profile sync) wait for
  `AppActivation`. It flips on the first foreground activity or a real sync
  signal (a push). Binding the VPN tile or starting a service alone therefore
  keeps the process silent; a `SystemVpnStarts` restart is still evaluated,
  so always-on VPN keeps working.
- `Unknown` never connects and is never guessed from a broken probe. `Home`
  needs the home host resolved to a private address that accepts TCP on a
  non-VPN network; `Away` needs that host resolved but not answering; no
  resolved address at all (DNS failure, a blocked network, no endpoint yet)
  stays `Unknown`. In the foreground the route shows "checking" for at most
  15 s, then the decision is logged as `ENVIRONMENT_TIMEOUT`. A network
  blocked for the app (Doze, data saver) is `Unknown`, not `Away`, so a failed
  home probe during sleep does not look like leaving home. When the same
  network was already classified `Home`, a blocked-status flap keeps `Home`
  instead of toggling `Home`/`Unknown`; a different handle still starts as
  `Unknown`.
- Decisions are logged: tag `VpnAutoConnect` (source, action, reason,
  environment, foreground, settings, manual wish; actions at WARN),
  `VpnController` (origin, `atHome`, skipped starts, exhausted outcome) and
  `VpnEnvironment` (classification changes).
  Nothing blocks an OkHttp or main thread; there is no `runBlocking` in core.
- Network inspection (logcat `HomeHttp`, Chucker) lives in `app/diagnostics`.
  It is opt-in: only `--define=home_network_inspection=on` on a debug install
  links it. Release and Firebase distribution builds carry no inspection
  interceptors, Chucker notifications are off even when linked, and the HTTP
  body logger is enabled only in a debuggable build.

### One-way data flow

- Every screen is a `ScreenFragment` (or `ScreenBottomSheet`) on a
  `StateViewModel<S, I, E>`: one `StateFlow<S>` changed only through
  `updateState`, typed intents in (`dispatch`), one-shot events out. Messages,
  dialogs and navigation are events; a ViewModel has no `Context`.
- Screen lifecycle has two hooks. `onScreenCreate` runs in `onCreate`, once
  per instance: the event collector, typed result listeners, activity
  result/permission launchers. `onViewReady` runs per view: adapter, click
  listeners, binders and render subscriptions on `viewLifecycleOwner`.
- Events sit in `ScreenEvents` (a queue in the ViewModel) until a screen takes
  them. The collector runs from `CREATED`; each event waits for its own
  lifecycle state: `RESUMED` by default (navigation, dialogs), `STARTED` for an
  event that implements `DeliveredWhenStarted` (notices). An event is removed
  only when delivered, so a destroyed view or a rotation loses nothing.
- Rendering goes by slices: `render(selector) { value -> }` maps the state,
  skips equal values (also across stop/start) and runs on `STARTED`.
  `render(state)` stays optional for tiny screens. Derived data (rows,
  selection summary, speed dial) is computed in `derive(previous, next)` of the
  ViewModel and stored in the state, recomputed only when its sources change.
- Loading is explicit: a feature repository loads its store once when its
  scope is created; showing a screen or switching tabs never makes a request,
  the screen reads the store. The server is asked again only on pull-to-refresh,
  on a `SyncSignal` (`core/data` `SyncSignals`) and after the app's own
  mutations (they update the store). `EntitySync` connects a store to the
  signals: `Changed(entity, upsert, id)` fetches that one entity (or reloads
  the list when the feature has no single fetch, e.g. timers),
  `Changed(entity, delete, id)` removes it locally, `Stale` reloads the list.
  Signals come from silent FCM pushes (`type=sync`, decoded by `PushInbox`,
  sent by the scheduler after every mutation and engine tick, never to the
  device named in `Home-Device-Id`) and from `AppSyncSignals`, which emits
  `Stale` when the app returns from background after `StalenessPolicy`
  (5 minutes). A signal reaches only live feature scopes; a scope created later
  loads fresh data anyway. The control catalog (device states owned by Home
  Assistant, no change feed) keeps its foreground polling. `ScreenVisibility`
  is fed by the screen's view lifecycle, never by `subscriptionCount`; tickers
  use `visibility.visible`.
- Pull-to-refresh names its scrolling views: `setupPullToRefresh(list,
  appBar) { }` installs `OnChildScrollUpCallback`, so a `SwipeRefreshLayout`
  whose direct child is a wrapper (`FrameLayout`, `CoordinatorLayout` that does
  not forward nested scroll) still refreshes only at the top of the list. A
  `RecyclerView` is at the top when its first laid-out child is adapter
  position 0 and starts at `paddingTop` (`isAtTop()`); `canScrollVertically`
  is not used for lists because a zero-height first row (the empty hub on the
  home screen) makes its offset estimate positive at the very top.
- Sticky section headers (`StickySectionHeaders`) pin to the visible top of
  the list (`paddingTop` with `clipToPadding`, otherwise the list edge), fill
  the whole width with the window background and are pushed out by the next
  header.
- Results between screens and sheets are typed: `object X : ResultKey<T>(T.serializer())`
  with a `@Serializable` value; the sender calls `setResult(X, value)`, the
  host registers `onResult(X) { value -> Intent }` in `onScreenCreate`
  (child fragment manager, owner = the screen), and the value reaches the
  ViewModel as an intent. Fragment results are kept while the host is stopped,
  on the back stack or being recreated. `OptionsSheet`/`WheelSheet` use
  `OptionsResult`/`WheelResult` keys. `AppEventBus` carries only signals
  between features that share no repository; "data changed" events are not
  used.
- Scenario form dialogs edit a form that is part of the editor state (draft,
  validation, process death), so they share the host editor's ViewModel through
  `hostScreenViewModels()` instead of returning a result.
- Several ViewModels per screen: `attach(viewModel, onEvent)` delivers events
  and visibility of a secondary ViewModel, `render(viewModel, selector)` draws
  its state. A section of another feature is a `SectionFragment` with its own
  ViewModel, placed into a slot with `section(R.id.slot, Key)`; the key and the
  contract live in that feature's api (`UpcomingAlarmsSection :
  ScreenSectionKey`), the fragment is contributed as `ScreenSection` from impl.
  The home screen shows the alarms section next to the control ViewModel.
- Flow ViewModels: a fragment that is a `FlowMember` gets `by
  flowViewModel()` — one instance for every screen and sheet of the flow, kept
  while any of them is alive (also on the back stack) and cleared when the
  last one leaves. Actions shared by a list and its editor (delete with
  confirmation and undo, toggle) live in such an actions ViewModel
  (`AlarmsActionsViewModel`); an editor may use its flow's ViewModels, nothing
  reaches them through the Activity.
- ViewModel scopes: a screen's own (`screenViewModel()`), or `tabViewModels()`
  for list screens that must keep their state while the router replaces the tab
  (alarms, reminders, timers, scenarios, chat). The tab scope lives in
  `TabScopes`, keyed by the fragment's place in the tree, not by the Activity.
- Lists are declared in XML: `app:layoutManager`, `app:spanCount`,
  `app:stackFromEnd`, `tools:listitem`; detekt `NoCodeLayoutManager` rejects
  layout managers in code except a grid that installs a `SpanSizeLookup`
  (control).
- Controls never own state: `SwitchBinder`, `CheckableBinder`,
  `RadioGroupBinder`, `TextBinder` and `PillSegments` send an intent and keep
  the last rendered value until the state changes; programmatic rendering
  never reports back.
- No `onResume` refresh. A feature repository is the single source of truth:
  it implements `EntitySource` (`core/data`) over an `EntityStore` —
  `StateFlow<Map<K, V>>`/`items` plus `load: StateFlow<ListLoadState>`. Every
  mutation goes through the repository and updates the store from the server
  answer (`commit`); toggles and deletes are `optimistic` and roll back on
  failure; a background sync merges and keeps local writes made while it was in
  flight. Lists, editors and the home screen subscribe to the store; an editor
  closes itself when its entity disappears. `LatestLauncher` makes the last
  request win.
- Long server lists use the reusable paging stack; see `Paged lists` below.
  Notification history is owned by the scheduler and fetched this way; the app
  has no local history copy. A one-shot startup cleanup removes the obsolete
  `push_notification_history` key left by earlier versions.
- Lists use `KeyedDiff`, which always returns a payload, so a partial change
  rebinds the same holder (`aspectPayload` per aspect) and never cross-fades.
- Editors share `EditorDraftSession`: draft, the create idempotency key bound
  to the endpoint, and a completion that survives process death, in
  `SavedStateHandle`. A request that may have reached the server is retried
  with the same key.
- Navigation is typed: a destination owns its route and `RouteCodec`, the
  router passes Parcelable `RouteArgs`, a ViewModel emits `OpenRoute`.
  Deep links (push, notification history) go through `DeepLinkResolver` and
  `FeatureEntry.parse`; only the app's own `home://` scheme is accepted.
- `MainActivity` has no feature knowledge: it renders `AppChromeState` and
  attaches `Set<ToolbarContribution>`, `Set<ToolbarAction>`,
  `Set<DeepLinkSource>`. Home quick-action tiles of other features come from
  `Set<QuickTileProvider>`.

### Shared infrastructure

- Logging goes through `AppLogger` (`core/model`): `DebugAppLogger` or
  `ReleaseAppLogger` (warn and above) by `AppEnvironment`, `FakeAppLogger` in
  tests. detekt forbids `android.util.Log` outside `LogcatAppLogger`.
- `AnalyticsTracker` is a `CompositeAnalyticsTracker` over
  `Set<AnalyticsBackend>`; each backend declares the environments it runs in
  (today only `LogAnalyticsBackend` for debug). Every resumed `TrackedScreen`
  (`ScreenFragment`, `ScreenBottomSheet`) is reported automatically.
- Bottom sheets with their own state extend `ScreenBottomSheet<VB, S, I, E>`
  (same model as `ScreenFragment`, `by screenViewModel()`). Plain choices use
  `OptionsSheet` and `WheelSheet` from `core/presentation`; results come back
  through typed `ResultKey`s.
- Sectioned lists: the adapter implements `SectionedAdapter`; decorations
  `StickySectionHeaders().attachTo(list)` (a tap on the pinned header goes to
  the header), `GroupDividers` (only between rows of a group) and `GroupCards`
  (rounded first and last row of a group).
- A notice with an action is a `RouteNotice(message, route)` shown by
  `showNotice`: a snackbar with "Open" that opens the typed `Route`.
- Back handlers are registered with `addBackCallback`; `backDispatcher`
  prefers the enclosing dialog over the activity.

### DI

Feature data lives in a feature scope, not in `AppScope`:

- each data feature has a Metro `@GraphExtension(XScope::class)` (`XScope` in
  its api) with its repository `@SingleIn(XScope::class)`, and an
  `XFeatureScope` (`FeatureScope<XGraph>` from `FeatureScopes`);
- an owner takes a lease: a ViewModel through `scope.viewModel { graph -> }`
  (released in `onCleared`), a service in `onCreate`/`onDestroy`, a toolbar
  contribution for the Activity lifetime (`LeaseRelease`). All owners get the
  same instance; the scope is cleared 5 s after the last release, or at once
  when the app goes to the background with no owners;
- another feature leases a repository through `FeatureScope<XRepository>`
  exported from the impl (home leases timers, reminders and scenarios).

Metro's compiler plugin runs on every Kotlin Android compilation
(`//platform/android:defs.bzl`). `AppGraph` exposes only `appearance`,
`startables` and `inject(MainActivity)`; everything else is a contribution.

- Work that starts with the process is an `AppStartable`
  (`@ContributesIntoSet(binding = binding<AppStartable>())`);
  `HomeApplication` only iterates them. Nobody creates its own
  `CoroutineScope(SupervisorJob() + …)`.
- ViewModels are `internal`; a public `XViewModelSource` with an `internal
  constructor` is contributed into the `ViewModelSource` map with
  `@ClassKey`. Screens use `by screenViewModel()` / `by injectedViewModels()`.
- Framework-created components (services, tile, other activities) use member
  injection through a feature `@ContributesTo` injector (`VpnInjector`,
  `PushInjector`, `CallInjector`); there are no `object` service locators.

#### Metro and internal

Metro silently skips `internal` contributions from another module: the code
compiles, but the binding never reaches `AppGraph`. Therefore:

- implementations are `internal` + `@Inject` and are bound in one public
  `XBindings` interface (`@ContributesTo`, members `@Binds internal` /
  `@IntoSet`), e.g. `VpnBindings`, `CallBindings`;
- a class that is itself the contribution (`*ViewModelSource`,
  `*FeatureEntry`, `*ToolbarContribution`, `AppStartable`s) is public with an
  `internal constructor`, and its public members expose only API types;
- the only other public declarations in impl are custom views referenced from
  layouts (generated bindings are public).

`PublicInImpl` encodes exactly these exceptions. Tests in other modules
navigate by route string, not by an impl destination.

## Constraints worth knowing

- **Push identity.** The server keys a phone by `deviceId` = SHA-256 of the
  app-salted `ANDROID_ID` (random UUID if absent); the raw id never leaves the
  phone. The DataStore key stays `push_device_id`; the VPN profile uses the
  same id. Registration (`POST /v1/push/devices` on the scheduler URL) is
  reactive on token, server, name and version, plus every 24 h.
  «Update registration» re-requests a missing FCM token (a failed fetch at
  start is not final) and reports a typed result: no FCM token, server or
  token not set, or the server failure text; never a generic error.
- **Call foreground service** claims `phoneCall|microphone` (microphone only
  when `RECORD_AUDIO` is granted, Android 11+): with only `phoneCall` the
  microphone is cut once the call screen stops, e.g. when the proximity
  sensor blanks it. The proximity lock is held only on the earpiece route.
  If Telecom refuses the self-managed connection, the call is placed
  directly instead of hanging in «Dialing».
- **Call screen closing.** The screen closes on `IDLE` only after the call
  it started has left `IDLE` at least once. The decision is made in a single
  collector of the controller state, reading the current flags directly; it
  must not come out of the `combine` that renders the screen: `combine` may
  deliver the cleared «awaiting start» flag before its own copy of `DIALING`
  and then judge a stale `IDLE`, which closed the screen ~40 ms after start.
- **Find phone.** A high-priority FCM data message is the case Android allows
  a killed app to start a foreground service from. The ring uses
  `USAGE_ALARM` at max alarm volume and restores it; the ringer mode is never
  changed. The channel `home_phone_find_v2` is silent (the player is the only
  sound); the old `home_phone_find` is deleted on start because channel
  settings are immutable. Never `NotificationCompat.setSilent`: its group
  alert behaviour makes SystemUI suppress the full-screen intent. Loud-path
  permissions: `POST_NOTIFICATIONS`, notification-policy access (DND bypass),
  full-screen intent (14+).
- **Manifest.** `rules_android` drops library permissions, so the app manifest
  declares push and call permissions; WebRTC's NetworkMonitor aborts without
  network-state access.
- **WireGuard statistics** are read via reflection from the backend's UAPI
  text (`GoBackendUapi`), not `GoBackend.getStatistics`: its
  `Statistics$PeerStats` record extends the D8 global synthetic
  `com.android.tools.r8.RecordTag`, which `rules_android` drops without dex
  optimisation. `apk_runtime_closure` lists `RecordTag` as optional; nothing
  may touch `Statistics`. `TransportProtocolsTest` breaks if the private
  members are renamed.
- **VPN routing.** The home router does not hairpin UDP, so at home WireGuard
  uses `peer.lanEndpoint`. Home detection needs no location permission: the
  scheduler host must resolve to a private address on a non-VPN network and
  accept TCP. External transports (OpenConnect via Cisco Secure Client or
  OpenConnect app) start only from a button; their status is "reported by the
  system", any system VPN counts.
- **Calls.** The phone is always the caller; media starts only after the
  server accepted the call. `Watch` uses Connect envelope framing over
  HTTP/1.1 and is open only during a call. Audio follows call state
  (`CallAudioSession`), not the screen, so rotation does not mute. Incoming
  calls are declined.
- **Server text.** Server messages are `LocalizedText` keys resolved to
  `@string/server_<key with dots as underscores>`; the server never sends
  display text. UI is English and Russian.
- **Scenarios.** Unknown oneof variants from a newer server are kept as raw
  JSON and block saving, so an old client cannot corrupt them. Limits mirror
  the server (`ScenarioValidation`).
- **Assistant profile switch** is the HA switch labelled `voice_profile`
  (`on` = named profile, `off` = default); another label id is set with
  `CONTROL_PROFILE_LABEL` on `ha-control`. Wake marks are the
  `script.wake_false_alarm` / `script.wake_missed` ids from
  `projects/home-assistant/ha-voice/scripts.yaml`.
- **Insets.** Everything is edge-to-edge through one mechanism in
  `core/ui/WindowInsets.kt`; no layout hard-codes bar sizes. The bottom
  navigation hides while the keyboard is open; pinned panels use
  `pinAboveKeyboard`, lists `applyScrollBottomInset`.
- **Themes** are `ThemeOverlay`s applied in `onActivityPreCreated` before
  inflation (Material You: dynamic colours over Emerald). Stored keys are
  `emerald`, `system`, `neon`; a retired key stays in `AppTheme.legacyKeys`
  (`current` reads as `emerald`), so a rename never drops the user's choice.
  Screens use only theme
  attributes; `AndroidThemeColorTests` rejects literal colours and segmented
  buttons. No effect exists in only one theme. Adding a theme: one `AppTheme`
  entry, its palette, `@color/theme_<key>_*` and the overlay. Components and
  tokens: [core/designsystem/README.md](core/designsystem/README.md).
- **Cleartext LAN HTTP** is a deliberate private-network trade-off in
  `network_security_config.xml`; HTTPS uses the platform trust store, no
  pinning or custom trust managers.

## App identity

`dev.proflyder.home`, minSdk 26, target/compile SDK 36, JVM target 17.

## Launcher icon per theme

Each `AppTheme` has an `activity-alias` `dev.proflyder.home.launcher.<Name>`
targeting `MainActivity`, with its own adaptive icon
(`app_ic_launcher_<theme>`), built by one rule for every theme from colour
roles in `launcher_icon_colors.xml`: `background` = the theme's surface
container high, `glow` = its primary container (a wide radial halo),
`symbol` = its primary (the house and a soft primary light at the halo's
centre), plus a shared monochrome layer. Roles reference the theme
palette, so icons follow light/dark mode; the system theme maps the same roles
to wallpaper colours on API 31+. Only the alias of
`AppTheme.DEFAULT` is enabled in the manifest. `LauncherIcons`
(`core/launchericon/api`) is implemented by `PackageManagerLauncherIcons`
(`core/launchericon/impl`), which enables the target alias first, then
disables the others with `DONT_KILL_APP`, skipping writes when the state
already matches. `LauncherIconSync` (an `AppStartable`) waits for the stored
appearance and applies every distinct theme, so the startup sync and theme
changes share one path. `MainActivity` stays exported without a launcher
filter, so explicit intents and `getLaunchIntentForPackage` (push, VPN
notifications) keep working. A new `AppTheme` entry fails compilation in
`LauncherAliases` until its alias and icon exist.

## Notification icon per theme

Every notification uses its monochrome small icon tinted with the current
theme's primary (`NotificationAccent`). `phone-push` sends notifications
data-only, so the app, not the FCM SDK, builds them even in the background;
the `default_notification_*` meta-data in the push manifest only covers
messages still carrying a `notification` block. HyperOS in its own
notification style draws the cached application icon instead of the small
icon; that is a system choice and the app does not work around it.

## Notification bell

`NotificationBellContribution` (settings feature) adds a bell to the toolbar
end slot on every tab; it opens the notification history (`settings/history`),
which is no longer listed in Settings. Every received notification is stored
unread (`NotificationHistoryEntry.read`); tapping an entry or "Mark all read"
marks it read, and the bell badge shows the unread count (capped at `99+`).
Entries saved before read state existed decode as read, so an upgrade does not
light the badge with old history.
