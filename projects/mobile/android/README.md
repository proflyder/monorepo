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
  - **VPN home** → the built-in VPN screen ([VPN](#vpn));
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

## VPN

`features/vpn` (api/impl) is the single entry point home from outside: a
**VPN** row in Settings, a **VPN home** tile in Home's Quick actions (pinned
after **Call the station**), a Quick Settings tile and the status
notification all open or drive the same controller.

- Profile: the phone generates a WireGuard key pair (`com.wireguard.crypto`),
  seals the private key with an Android Keystore AES-GCM key
  (`KeystoreTokenCipher`, alias `dev.proflyder.home.vpn.secrets.v1`) in the app
  DataStore and sends only the public key to the scheduler:
  `POST /v1/vpn/profile {deviceId, publicKey, deviceName}` on the scheduler
  base URL with the same bearer as `/v1/push/devices`; `deviceId` is the push
  device id. The answer is stored and reused offline. The first request runs
  at app start (works at home or through the fallback); an unreachable API,
  missing token, `400/409/429/503` each have their own text and a saved
  profile stays usable. **Reissue** rotates the key pair and re-posts it;
  **Forget device** calls `POST /v1/vpn/profile/remove`, then deletes the
  profile, the key and the imported certificate.
- Transports: `VpnController` (api) knows no protocol. Each transport is an
  internal `VpnTransport` in `impl/transport/<protocol>/`, contributed into
  `Set<VpnTransport>` by `VpnBindings`, with its own id (the key of its
  settings in the profile), order, handshake timeout, `status` flow and
  settings decoder. `VpnTransportChain` orders them: automatic transports are
  tried in order, external ones (they hand over to another app) start only
  from a button. `VpnProfile` (core/model) carries
  `transports: Map<VpnProtocolId, TransportSettings>`; core/network copies
  every object field of the server profile into it untouched.
- WireGuard (`transport/wireguard`, `GoBackend`, its own `VpnService`): the
  tunnel is built by `WireGuardConfigText` from `WireGuardSettings`;
  **All traffic** uses `allowedIps`, **Home only** uses `splitAllowedIps`
  (falls back to `allowedIps` when empty). Changing the route reconnects an
  active tunnel.
- Endpoint: every `up()` gets a `TunnelRequest(profile, settings, route,
  atHome)`; on the home network WireGuard uses `peer.lanEndpoint`, elsewhere
  `endpoint` (an empty `lanEndpoint` keeps `endpoint`). The home router does
  not hairpin UDP, so the public name does not answer from the home Wi-Fi.
  While a tunnel is up the controller follows `NetworkEnvironment` and asks
  the transport `needsRestart(current, next)`; WireGuard says yes when the
  endpoint changes and is brought up again.
- Fallback: when no automatic transport gets a handshake in its timeout
  (WireGuard 10 s), it is stopped and nothing else starts by itself. Outside
  home the screen shows **WireGuard did not answer** with one **Open …**
  button per external transport (`VpnConnection.FallbackRequired(options)`);
  at home it shows **Home network: VPN not needed**. The OpenConnect
  transport (`transport/openconnect`) is an external client:
  `anyconnect://connect/?name=…&host=<ocserv host>/<group>` (Cisco Secure
  Client), or the OpenConnect app (`app.openconnect`) when no `anyconnect`
  handler exists. Group follows the route (`group` or `splitGroup`). Its
  status comes from a `TRANSPORT_VPN` network callback and is shown as
  "reported by the system, not verified": any system VPN counts, the app
  cannot see whose it is.
- ocserv authentication (the server does not issue certificates): pill tabs
  **Certificate** / **Password + code**. A `.p12` is picked with the system
  file picker, checked with its password (`KeyStore` PKCS12, must contain a
  key, ≤ 64 KB), stored sealed and offered to the system credential storage
  (`KeyChain.createInstallIntent`) where the VPN client picks it. The password
  is not stored. With password + code the client asks for both.
- Home network: no location or Wi-Fi permissions. `NetworkEnvironment`
  (api) is a `StateFlow<HomeNetworkState>` (`Unknown` / `Offline` /
  `Home(networks)` / `Away(networks)`); `AndroidNetworkEnvironment` builds it
  from a non-VPN `registerNetworkCallback` (`callbackFlow` with `awaitClose`,
  so the callback goes away with the last subscriber, `WhileSubscribed`),
  settles bursts (`settle`: the first change at once, later ones after 1.5 s
  of quiet) and probes each non-VPN network with its own resolver and
  `socketFactory`: the scheduler host must resolve to a private address
  (RFC 1918 or IPv6 ULA) and accept a TCP connect. The handles of the
  networks are part of the state, so Wi-Fi → another Wi-Fi, Wi-Fi ↔ mobile
  and the way back home are new states, while our own tunnel is not.
- Automatic connect: `AutoConnectCoordinator : AppStartable` combines
  `AppForeground`, `autoConnect`, whether the tunnel is up and, while any of
  them needs it, `NetworkEnvironment`. Each distinct trigger (foreground,
  network, `autoConnect`) goes through `AutoConnectPolicy.decide`: away from
  home + profile + tunnel down + (**Connect outside home**, or foreground and
  **Connect when the app opens**, on by default) → connect; at home +
  **Connect outside home** + tunnel up → disconnect; missing permission in
  the foreground → the consent dialog once (`permissionAsked`, delivered to
  `VpnActivityHost` through `permissionRequests`). The tunnel state is read
  when a decision is made, not part of the trigger, so a failed attempt or a
  manual toggle-off does not restart the tunnel until the network or the
  foreground changes. While the first check and a connect are pending
  `VpnRouteCondition` keeps `RouteGate` closed (every REST call and stream
  suspends in `RouteGate.await()`, at most 20 s) and the strip under the
  toolbar says **Connecting to home…**. For a guaranteed tunnel use Android
  always-on VPN: the system starts `GoBackend$VpnService`, the transport's
  `systemStarts` flow asks the controller to connect.
- Controller: `ChainVpnController` is an actor: `connect`, `connectVia`,
  `reconnect` and `disconnect` are commands in one channel, executed in order
  by one coroutine that cancels and joins the running session before the
  next one, so a disconnect cannot race a connect. Traffic is a cold flow
  (`VpnController.traffic`), polled only while the VPN screen watches it.
- Toolbar toggle: an icon button (`Widget.Home.IconButton.Standard.M`,
  `VpnToolbarToggle`) on the main screens shows off / connecting / on; tap
  toggles the tunnel (asking for the permission when needed), long press opens
  the VPN screen. The Quick Settings tile stays.
- The VPN screen hides the bottom tab bar (`FeatureEntry.hidesBottomBar`,
  `Navigator.currentHidesBottomBar`); content then takes the navigation-bar
  inset itself.
- Status: the screen shows state, transport, connected time (`tickerFlow`
  while the screen is visible) and traffic (WireGuard). `VpnStatusServiceStarter`
  starts `VpnStatusService` when the tunnel becomes active; it is a foreground service
  (`specialUse`, subtype `vpn_tunnel_status`) with the status and
  **Disconnect**; it stops itself when the VPN is off. `VpnTileService` is a
  toggleable Quick Settings tile; without the VPN permission it opens the VPN
  screen (`home://vpn/main` route).
- Tests: `features/vpn/impl:screen_test` — `AutoConnectPolicyTest`,
  `AutoConnectCoordinatorTest` (fake environment, foreground, settings and
  controller: connect on open, manual disconnect respected, background
  auto-connect and home disconnect, permission asked once),
  `ChainVpnControllerTest` (a third fake transport joins the chain without
  changes, external fallback offered only away, command order, restart on a
  network change, permission/profile failures), `TransportProtocolsTest`
  (routes, endpoints, config text, OpenConnect link, UAPI stats, the private
  `GoBackend` members the statistics adapter reads), `EnvironmentRulesTest`,
  `StoredVpnProfileTest`, toolbar toggle PNGs
  `vpn-toolbar-{off,connecting,on}.png` and Robolectric PNGs `vpn-light.png`,
  `vpn-dark.png`, `vpn-fallback-dark.png`, `vpn-home-dark.png`,
  `vpn-external-dark.png` in `TEST_UNDECLARED_OUTPUTS_DIR`;
  `core/network/impl:impl_test` (`VpnProfileClientTest`, golden
  `fixtures/vpn/*.json`).

### Notes to the code (VPN)

- WireGuard statistics are read from the backend's UAPI text
  (`GoBackend.wgGetConfig(currentTunnelHandle)` via reflection in the
  `GoBackendUapi` adapter behind `WireGuardStatsSource`, parsed by
  `WireGuardUapi`) instead of `GoBackend.getStatistics`;
  `TransportProtocolsTest` fails when a library update renames those private
  members. The AAR ships
  `Statistics$PeerStats` as a desugared record whose superclass is the D8
  global synthetic `com.android.tools.r8.RecordTag`; `rules_android` keeps
  global synthetics only with dex optimisation, so the class is absent from
  the APK and loading `PeerStats` would crash. `apk_runtime_closure` lists
  `RecordTag` as optional for that reason; nothing in the app may touch
  `Statistics`.
- Native OpenConnect (next step): build `libopenconnect` (LGPL) with its
  dependencies (GnuTLS + nettle/gmp or OpenSSL, libxml2, zlib, lz4, stoken
  optional) for `arm64-v8a` with the NDK toolchain as `cc_library`/foreign
  rules targets, add a small JNI bridge (the ics-openconnect `LibOpenConnect`
  API: `setupTunFd`, `obtainCookie`, `makeCstpConnection`, `mainloop`,
  `setClientCert`, auth form callbacks), and an own `VpnService` that hands
  the tun fd to it. It plugs in as another `VpnTransport` (a new class in
  `transport/<protocol>/` and one `@Binds @IntoSet` line in `VpnBindings`); the `.p12`
  password would then have to be stored sealed next to the container.
  Estimated scope: 3–5 days, mostly the hermetic C dependency build and
  licence/notice handling.
- The stored profile keeps the server's transport objects as JSON
  (`StoredVpnProfile.transports`). A profile saved in the older flat format
  does not decode; `VpnProfileSync` registers again with the stored key as
  soon as the server is configured, so the device keeps its keys and id.
- Internal implementations reach the graph through `VpnBindings`
  (`@Binds internal`), the public `VpnViewModelSource` and the
  `VpnActivityHost` factory, both with internal constructors.

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
- Pull-to-refresh plus an automatic refresh every 30 s while the screen
  observes the data: `PollingSystemRepository` owns `StateFlow<SystemState>`
  (`stateIn(WhileSubscribed)` over `tickerFlow` and refresh requests), so the
  poll stops 5 s after the last screen stops collecting and the fragment has no
  timer; the server caches each section 15 s.
  A failed refresh keeps the last data and shows a notice; with no data the
  error state has a Retry button. A missing source is «No data», never zero.
- Levels use theme attributes only: ok `?colorPrimary`, warning
  `?colorTertiary`, critical `?colorError`, no data `?colorOutline`.
- Tests: `core/network/impl:impl_test` parses the server's golden responses
  (`fixtures/system/*.json`, copied from the Rust contract);
  `features/system/impl:system_test` covers the row mapping, the ViewModel and
  Robolectric PNGs (`system-overview`, `system-overview-no-data`,
  `system-cluster`, `system-guests-dark`) in `TEST_UNDECLARED_OUTPUTS_DIR`.

## Hermes chat (network contract)

- `core/network/impl` `HttpChatClient` talks to `mcp-gateway` `/v1/chat/*` through
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
  network returns (`Backoff` up to 30 s after `RouteGate.await()`). The server
  is idempotent on `clientId`, so a resend never duplicates. A message the server
  rejects (empty, too long, missing quote) stays in the feed with the reason and
  Retry/Delete; a failed assistant reply shows the reason and Retry.
- Live updates: while the tab is started the repository reads the SSE stream
  from its last `cursor`, applies `message` upserts and `delta` chunks
  (a delta is applied only when its `offset` equals the current text length),
  and reconnects with back-off; `401` stops the loop.
- Insets: the input panel uses `pinAboveKeyboard`, the empty/error states follow
  the list padding so they stay above the panel; `Widget.Home.TextInput.Chat`
  (core/designsystem) is the message field: no underline, `ds_radius_l` on
  all corners, `colorSurfaceContainerHigh` fill, a 1dp `colorStateOn` outline
  only while focused, no floating hint; the same in every theme.
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
- Alerts: Alertmanager alerts arrive as cards (name, severity chip, summary,
  description, namespace / object, since → resolved time, Runbook). Firing
  critical uses `?colorErrorContainer`, warning `?colorTertiaryContainer`, a
  resolved card turns neutral with «✅ Resolved» in place. «Investigate» (or
  long press → Reply) quotes the card, whose text carries the fingerprint, so
  Hermes runs the same `infra_alert` investigation as a Telegram reply.
- Push: the gateway sends `home://chat/message/<id>` for a new alert and for a
  finished reply. `PushChatGate` skips the notification while the Chat tab is
  started (`ChatFocus.isVisible`); a tap — from the in-app notification or the
  system tray (`deepLink` data extra) — opens the tab and scrolls to that
  message (`ChatFocus.request`).
- Server error keys map to string resources in `ChatTexts.kt`; the server never
  sends user-facing text.
- Tests: `features/chat/impl:chat_test` (timeline reducers, outbox codec,
  repository queue with an offline/timeout/permanent failure and attachment
  uploads, feed items, Markdown, Robolectric PNGs `chat-current`,
  `chat-current-dark`, `chat-neon`, `chat-neon-dark`, `chat-neon-typing`,
  `chat-media-current` (voice with transcript and playback, file, photo) and
  `chat-media-neon-dark` (the recording panel), `chat-alerts-current`,
  `chat-alerts-neon-dark`) and
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
  The catalog and the hub are `StateFlow`s in the data layer
  (`ConnectControlRepository`, `HomeHubFeed`) with one refresh policy
  (`ControlRefreshPolicy`): they reload while the app is in the foreground and
  someone observes them (immediately on return, then every 30 s), on
  pull-to-refresh and after commands. Screens never refresh on resume.
- **Quick actions** open from a FAB as a speed-dial overlay: tapping a row
  runs the action, long-pressing a device or scenario pins/unpins it. The dial
  lists pinned devices, scenarios, the "Say on the station" action, the wake
  marks and the tiles other features contribute through `QuickTileProvider`
  (`@ContributesIntoSet`: Call from `features/call`, VPN from `features/vpn`);
  order and set are kept in DataStore. Pins
  are changed by long-pressing a device in a room group or a native scenario in
  the scenario list (**Pin to Home** / **Unpin**). A non-light device tile
  toggles on tap; a light tile opens its brightness/color sheet on tap. On first
  launch the speak action and the native **Blackout** / **Work mode** scenarios
  (when they exist) are pinned. Contributed tiles are always present. Whether
  the dial is open is ViewModel state, like every other control on the screen.
- Filter chips (All / Lights / Switches, plus room filters) are persisted in
  DataStore (`ControlPreferences.filter` flow) and restored on launch. Room
  sections are collapsible; the folded state lives in the ViewModel.
- The assistant profile is an HA switch carrying the `voice_profile` label
  (shown as `kind: "profile"`); it is switched from the toolbar chip on the Home
  tab through `VoiceProfileSwitch` — **Personal** (`turn_off`) / **Work**
  (`turn_on`). The wake marks (`script.wake_false_alarm`,
  `script.wake_missed`) are items of the quick-actions dial.
- Tapping a scenario or toggling a device enqueues a scheduler task
  `home.control` on the `control` queue (`EnqueueTask`). The phone only queues;
  the server executor owns execution. Device switches, light adjustments and
  volume are optimistic: `OptimisticDeviceState` overlays the expected value on
  the catalog until the server reports it or 15 s pass, and the repository
  re-reads the catalog on the settle schedule of the policy (no polling in the
  ViewModel). A failed command drops the expectation and reloads. Delivery is
  at-least-once, not exactly-once.
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
  and also marks the phone online. `Watch` is open only while a call session is
  in progress: the session opens it before `ListEndpoints` and closes it when
  the call ends, so an idle app keeps no stream.
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
- `CallController.state` (`StateFlow<CallSnapshot>`) is the single source of
  truth consumed by the screen, the Telecom connection, the audio route and the
  notification. `features/call/api` holds only contracts (`CallController`,
  `CallLauncher`, `CallSnapshot`/`CallPhase`/`CallFailure`); the state machine
  (`session/CallManager`) and everything else are `internal` in `impl`.
- Audio output: **Phone**, **Speaker**, **Headset**, **Bluetooth <name>**. On
  API 34+ selection goes through the Telecom `CallEndpoint` API
  (`requestCallEndpointChange`, `onAvailableCallEndpointsChanged`); older
  releases use `AudioManager.setCommunicationDevice` (31+) or
  speakerphone/Bluetooth SCO flags. Bluetooth routes are hidden when
  `BLUETOOTH_CONNECT` is denied.
- Mute disables the local audio track. The in-call screen uses a proximity wake
  lock while the call is active (`showWhenLocked`/`turnScreenOn`).

### Call: заметки к коду

- Аудиомаршрут живёт по состоянию звонка, а не по экрану: `CallAudioSession`
  (`AppStartable`) включает `MODE_IN_COMMUNICATION` на `ACTIVE` и выключает на
  любой другой фазе. `CallActivity` аудио не трогает, поэтому поворот или
  закрытие экрана не глушат звонок (Robolectric `CallActivityRotationTest`).
- Уведомление и foreground-сервис запускает `CallForegroundStarter`
  (`AppStartable`) на переходе в живой звонок; `CallForegroundService` —
  `LifecycleService`, наблюдает состояние в `lifecycleScope` и сам
  останавливается на `ENDED`/`IDLE`. Тексты фаз, причин и ошибок — один
  `CallTexts` для экрана и уведомления.
- Экран: `CallViewModel` (`StateViewModel`) собирает `CallScreenState` из
  `CallController.state`, маршрутов аудио, флагов экрана и `tickerFlow` по
  инъектируемому `Clock` (только в `ACTIVE`). Разрешения, открытие листа
  маршрутов, mute, «назад» и закрытие — намерения `CallIntent`; разовые
  действия (`RequestPermissions`, `Finish`, `MoveToBack`) — `CallEvent`.
  Activity только рисует (`CallScreen`, `CallRouteSheet`,
  `ProximityScreenLock`) и шлёт намерения.
- DI: Metro не видит `internal`-вклады из другого модуля, поэтому все
  реализации `internal` с `@Inject`, а связывание — в одном публичном
  `CallBindings` (`internal @Binds`/`@IntoSet`). `HomeConnectionService`/
  `CallForegroundService` получают зависимости member injection через
  `CallInjector`.
- Incoming calls are architectural only: while a session is open `Watch`
  recognises an incoming `RINGING` call for this endpoint and declines it. There is no incoming UI yet
  (`HomeConnectionService` cancels incoming connections).

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
  endpoint/timeout keys are not read by the app.
- Nothing reads DataStore or decrypts the token on the main thread or at graph
  creation. `AndroidConnectionSettingsStore` exposes `config` and
  `tokenStatus` as `StateFlow` built with `stateIn` from the DataStore flow
  (decoded on the I/O dispatcher, `StoredTokenSource`) and from
  `RemoteConnectionSource.state`. Until the first read `tokenStatus` is
  `LOADING` and `loaded` emits `false`; the connection screen shows a loading
  line and saving waits for `loaded`. `updateToken` writes DataStore and
  returns once `config` reflects the new token, so DataStore stays the only
  source of truth.
- Interceptors read `config.value` on OkHttp threads. Requests are gated:
  `ConnectionSettingsLoadedCondition` is a `RouteCondition`, so
  `RouteGate.await()` holds a request until the token and the cached Remote
  Config are loaded.
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
rules and a focused set of formatting rules; findings are failures.
`platform/android/detekt-rules` provides the first-party
`home-rules` plug-in (`NoElseIf`, `NoTry`, `LongLambda`, `NoInlineJsonInTests`,
`NoHardcodedUiText`, `PublicInImpl`, `NoDirectStateWrite`, `NoUiOwnedControlState`), loaded with `--plugins` by the Bazel rule in
`platform/android/detekt.bzl`. `NoTry` forbids `try`/`catch`/`finally`
entirely: use `runCatching`, `suspendRunCatching` (rethrows
`CancellationException`) or `use { }`. `NoElseIf` forbids only `else if` chains (a third branch); a plain `if`/`else`
is allowed. `PublicInImpl` requires every top-level declaration under
`*/impl/src/main/` to be `internal` or `private`. `NoDirectStateWrite` forbids
`mutableState.value =`, `+=` and `mutableState.update/emit/tryEmit/...` inside
classes extending `BaseViewModel`, except in `updateState` itself.
`NoUiOwnedControlState` enforces one-way UI data flow: it reports
`isChecked`/`isSelected`/`isActivated`/`checked` set to `!` of itself or set
inside a `setOn*Listener`/`addOn*Listener`/`doOn*` lambda, and `x.toggle()` on a
non-ViewModel receiver. The UI sends an intent and renders the value from the
ViewModel's StateFlow; local UI flags are not detectable and stay a review item.
There is no detekt baseline: every rule applies without exceptions, and the
only allowed public declarations in impl are listed in the Metro rule
(Architecture → DI → Notes to the code). Test mock responses live in
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
Home rows (`//projects/mobile/android/features/control/impl:control_rows_test`),
the Home hub model (live-row selection/order/cap and the 24 h reminder window,
"tomorrow 07:30" text, favorites pin/order/defaults, display-name rule, catalog
refresh policy and optimistic state, tap routing) in
`//projects/mobile/android/features/control/impl:control_logic_test`, the call state
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
  channel. The tap opens the app through its launch intent;
  `PushDeepLinkSource` hands the route to `DeepLinkResolver`, which parses it
  with the typed `FeatureEntry.parse` of the owning feature (a feature id
  selects that tab, `alarms/editor/<id>` opens the editor). An unknown route
  just opens the current tab. Every label and button is a
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
one token per device. Registration is reactive: `HomePushRegistrar`
(an `AppStartable`) combines the FCM token (`PushTokenSource`), the configured
server (`ConnectionSettingsStore.config`), the phone name and the app version,
and registers whenever that combination changes and is due
(`PushRegistrationPolicy`): the token, the name, the version or the server
differ from the last successful registration (persisted in DataStore together
with its time), or that registration is older than 24 hours. Configuring the
server or a rotated token therefore registers without an app restart.
«Обновить регистрацию» always registers; saving a new name registers it once.
A failure saves nothing, is shown on the settings screen and is retried on the
next change or start; the rules are pinned by `:registration_policy_test` and
`:registrar_test` (fake token, settings and API flows). `/v1/push/devices/remove`
forgets a token FCM rejected. Without `google-services.json` (Firebase
uninitialized) the token source stays empty, and without a configured server
nothing is sent; the app starts normally.

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
model; the owner can set a family name such as «Стас». Saving the name
persists it in DataStore and registers it on the existing `deviceId` row (the
same `POST /v1/push/devices`, so the server keeps one row per device). Ricardo's
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
empty, which means "not configured". `FirebaseRemoteConnectionSource` is an
`AppStartable`: the Firebase instance is created lazily on the I/O dispatcher
in `start()`, the cached value is published, and every fetch goes through one
`fetch(force)` under a mutex (start: `force = false`, the screen's refresh:
`force = true`; real-time updates only activate under the same mutex). The
document is decoded into the `@Serializable` `RemoteConnectionDocument`;
optional addresses go through `parseOptionalEndpoint`, and a wrong JSON type
is reported for the field it belongs to. The value is a JSON object; unknown keys
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
  app/                    single Activity (chrome only), app graph, diagnostics
  core/
    model/                Android-free domain: Outcome/RemoteFailure, Clock, ServerClock, DeviceIdentity
    runtime/              app CoroutineScope, IoDispatcher, Clock/AppForeground/DeviceIdentity impls,
                          AppStartable, tickerFlow, Backoff
    config/               remote connection contract/JSON parser + redaction
    remoteconfig/         Firebase Remote Config endpoint/timeout source
    settings/             DataStore + Keystore token + legacy migration
    network/              Retrofit/OkHttp Connect + control catalog + errors
    navigation/           typed routes (RoutePath/RouteCodec/RouteArgs), FeatureEntry, FragmentRouter,
                          AppChromeState, Route, DeepLinkResolver, ToolbarContribution/ToolbarAction
    ui/                   base Fragment/ViewModel, ViewBinding, list states
    presentation/         StateViewModel/ScreenFragment, binders, list loading, editor drafts
  features/
    alarms/{api,impl}     domain contract, ordering / validating use cases + repository + UI
    reminders/{api,impl}
    timers/{api,impl}
    call/{api,impl}       intercom state machine, WebRTC, Telecom, call screen
    push/{api,impl}       FCM service, find-phone ring, permission screen
    control/{api,impl}    Home catalog, filters, scenario/device/profile actions
    scenarios/{api,impl}  scenario list/editor, draft codec and validation
    connection/{api,impl}
    system/{api,impl}     System tab: polling repository, overview and detail screens
    time/{api,impl}       Time tab: pager over alarms, timers and reminders
packages/kotlin/core/     shared Kotlin utilities (endpoint/time/nav state)
platform/android/         Bazel macros, Metro wiring, APK gate, ViewBinding gen
```

Feature implementations depend only on their own API, core modules, and shared
Kotlin utilities; no feature impl imports another feature impl. The app module
is the composition root and the only module that sees every implementation.

### DI

Metro's compiler plugin is applied to every Kotlin Android compilation through
`//platform/android:defs.bzl`. The graph in `app/.../di/AppGraph.kt` is generated
at compile time and exposes only `appearance`, `startables` and
`inject(MainActivity)`; everything else is reached through contributions.
Repositories use `@ContributesBinding`, navigation entries `@ContributesIntoSet`,
`core/runtime` contributes `RuntimeBindings` (app `CoroutineScope`,
`@IoDispatcher`, `Clock`, `ServerClock`, the `Set<AppStartable>` multibinding),
and `AppBindings` provides the OkHttp/Retrofit clients, `ServerClockSync`, the
single `DataStore`, `FeatureRegistry` and the `Set<Interceptor>` multibinding.

- **Start.** Work that begins with the process is an `AppStartable`
  contributed with `@ContributesIntoSet(binding = binding<AppStartable>())`
  (VPN controller, auto-connect coordinator, status service starter and
  profile sync through `VpnBindings`, push registrar, call service
  starter, `ConnectionHealth`, foreground, appearance).
  `HomeApplication` only iterates `graph.startables`. No class creates its own
  `CoroutineScope(SupervisorJob() + …)`.
- **Chrome.** `MainActivity` owns no feature knowledge: it builds the
  `FragmentRouter`, renders one `AppChromeState` (tab, title, back, overlay,
  bottom bar) through `ToolbarChrome` and `BottomTabs`, and attaches the
  `Set<ToolbarContribution>` (VPN toggle and route banner, voice-profile chip)
  into the toolbar slots. The actions menu is the `Set<ToolbarAction>`
  (call, settings); deep links come from `Set<DeepLinkSource>` through
  `DeepLinkResolver`.
- **ViewModels.** One rule for every screen. A ViewModel is an `internal`
  class; next to it a public `XViewModelSource` with an `internal constructor`
  is contributed with `@ContributesIntoMap` + `@ClassKey(XViewModel::class)`
  into the `ViewModelSource` map (or the `SavedStateViewModelFactory` map for
  editors that need a `SavedStateHandle`). The source takes `() -> XViewModel`
  when the ViewModel is `@Inject`, or builds it from its dependencies when the
  constructor needs derived values (draft session, resource strings).
  `ViewModelFactoryRegistry` (core/ui) is the single `ViewModelProvider.Factory`;
  `HomeApplication` exposes it as `ViewModelFactoryProvider`, and screens use
  `by injectedViewModels()`, `by injectedActivityViewModels()` or
  `by screenViewModel()` (activities: `ComponentActivity.injectedViewModels()`).
  `AppGraphBindingsTest` resolves every ViewModel from the real graph.
- **Framework-created components** (services, tile, activities other than
  `MainActivity`) use member injection: `@Inject internal lateinit var` fields
  and `appGraph<FeatureInjector>().inject(this)` in `onCreate`, where
  `FeatureInjector` is a `@ContributesTo(AppScope::class)` interface in the
  feature impl (`VpnInjector`, `PushInjector`, `CallInjector`). There are no
  `object` service locators. Helpers that need an Activity
  (`VpnActivityHost`) are assisted-injected and created through their factory.
- **Network inspection** (logcat `HomeHttp` + Chucker) lives in
  `//projects/mobile/android/app/diagnostics` and contributes its interceptors
  into `@NetworkInspection Set<Interceptor>` (`HomeHttp` adds them to unary
  clients only); the build drops the module with
  `--define=home_network_inspection=off`, leaving the set empty.

#### Notes to the code

- **Metro and `internal` — one rule.** Metro silently skips `internal`
  contributions from another module: an `internal` class with
  `@ContributesBinding`/`@ContributesIntoSet`/`@ContributesIntoMap`/
  `@ContributesTo` in a feature impl compiles but never reaches `AppGraph`
  (`Set<FeatureEntry>` is missing a binding, a ViewModel is missing at runtime,
  a startable never starts). Hence:
  - implementations are `internal` with `@Inject` and are bound to their API
    type in one public `XBindings` interface (`@ContributesTo`, members
    `@Binds internal`/`@IntoSet`), e.g. `VpnBindings`, `CallBindings`;
  - a class that is itself the contribution (`*ViewModelSource`, `*FeatureEntry`,
    `*ToolbarContribution`, `AppStartable`s such as `PushInbox`) is public with an
    `internal constructor`, so its internal dependencies stay invisible outside
    the module; its public members expose only API types (`rootDestination:
    Destination`, not the internal destination object);
  - `@ContributesTo` injector interfaces (`VpnInjector`, …) expose only
    `inject(...)` for framework-created components.
  The only other public declarations in `*/impl/src/main/` are custom views
  referenced from layouts (`VoiceWaveView`): generated view bindings are
  public and expose them. detekt `PublicInImpl` encodes exactly these
  exceptions (`@Inject`/`@Contributes*`/`@BindingContainer` annotations, member
  injection, `View`/`ViewGroup`/layout supertypes); there is no detekt baseline,
  every other public declaration fails the build. Tests in other modules
  navigate by route string (`Navigator.openRoute`), not by an impl destination.
  `AppGraphBindingsTest` checks every ViewModel and startable resolves.
- Navigation is typed. A destination owns its route string and a
  `RouteCodec` (usually its companion) that parses it back; `FeatureEntry.routes`
  lists the codecs, so `parse`, `supports` and `FeatureRegistry.entryForRoute`
  never compare prefixes. The router passes `RouteArgs` (Parcelable) as the
  fragment arguments; a fragment reads its destination with
  `by destination(Codec)` and a ViewModel with `SavedStateHandle.destination(Codec)`.
  Screens never call the navigator: a ViewModel emits `OpenRoute(Route)` and
  `ScreenFragment.navigate` applies it. A route opened before the first resume
  (a cold-start deep link) is applied on top of the initial tab.
- Every screen is a `ScreenFragment` on a `StateViewModel`, including static
  ones (`DesignCatalogFragment`), so navigation and messages always arrive as
  events. The Time pager keeps the selected section in `TimeViewModel`
  (`SavedStateHandle`), the pager and the pills only render it.
- `TimeFragment` reads `FeatureRegistry` through the `TimeGraph` accessor
  (`appGraph<TimeGraph>()`): a fragment is created by the router, not by the
  graph, and an internal fragment cannot be member-injected from `AppGraph`.
- `NotificationPermission` (core/ui) is the single `POST_NOTIFICATIONS` +
  "notifications enabled" check for push and the VPN status notification.

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
- Browsable diagnostics (logcat tag `HomeHttp` + Chucker) come from the
  `app/diagnostics` module; build with `--define=home_network_inspection=off`
  to leave them out.

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

Text fields share `Widget.Home.TextInput` (also the theme's `textInputStyle`)
and look like a small card: a rectangle with 16dp on all four corners
(`ds_radius_l`, never a capsule) on `colorSurfaceContainerHigh`, one
tone above the `colorSurfaceContainer` card it sits on, with no underline
(`boxStrokeWidth` 0). A 1dp `colorOutlineVariant` outline follows the corners
at rest; focused it becomes 2dp `colorStateOn` (primary in Emerald, cyan in
Neon), with an error 2dp `colorError`. The label stays small inside the box
above the text (filled mode, so it never cuts the outline), and the floating
label, cursor and selection handles use the accent. Material draws no outline
around a filled box, so the theme's `viewInflaterClass` (`HomeViewInflater`)
inflates every `TextInputLayout` as `CardTextInputLayout`, which draws that
outline over the box; layouts keep the plain `TextInputLayout` tag and no
`style`. Helper, error and counter text use `textAppearanceBodySmall`.

- Every field has a floating label from resources and the label is always
  enabled: the text sits under it, never pushed down by an empty label slot.
  A card or section title never repeats the label — the name field on
  «Phone notifications and find» and the token field on «Connection» carry
  their card's name and icon (`startIconDrawable`) themselves.
- Examples go into `placeholderText`, not the label.
- Free-text names (alarm, reminder, timer, scenario, phone name) get the
  clear-text end icon; the token keeps the password toggle; the speak dialog
  keeps its 500-character counter.

Buttons inside cards follow one rule (`core/designsystem` styles, see its
README): every card action is size M — 40dp high (`ds_control_m`, no insets),
as wide as its text with 16dp side padding but at least 120dp
(`ds_button_min_width`), aligned to
the start of the card content; two actions of one card are a `ButtonRow`:
in a row with the main one on the right when both fit, otherwise stacked full
width with the main one on top (Connection in Russian at 360dp stacks **Save
and test connection** above **Remove stored token**). Button text never wraps
or gets cut. It has 16dp corners (`ds_radius_l`) and
`textAppearanceLabelLarge`. The action the screen exists for is
`Widget.Home.Button.Filled.M` (**Save and test connection**); other actions
are `Widget.Home.Button.Tonal.M` (**Refresh config**, **Save name**,
**Update registration**, permission **Allow**); destructive ones are
`Widget.Home.Button.Destructive.M` (outlined: transparent, 1dp `colorError`
stroke and `colorError` text: **Remove stored token**, shown only while a token
is stored). Text buttons are not used inside cards, so nothing is indented away
from the card's content edge. A screen of independent settings with no single
main action («Phone notifications and find») has no filled button. Editor
save bars use `Widget.Home.Button.Filled.L` (full width, 48dp). Every
settings card is `Widget.Home.Card` (filled `colorSurfaceContainer`, no
stroke) and a card title uses `textAppearanceTitleMedium`.

The toolbar title is the design-system `ToolbarTitle`: it picks the largest
size from 24sp down to 16sp at which the title fits in two lines, wrapping only
at a word boundary (balanced, no hyphenation, no ellipsis). The earlier
AppCompat autoSize did nothing because the title is `wrap_content`.
`//projects/mobile/android/app:narrow_screens_test` checks «Уведомления и
поиск телефона» at 360dp and 411dp and the Connection actions at 360dp.

«Phone notifications and find» shows each permission as a `ListRow`: icon,
title, state (in `colorError` while not allowed) and a trailing tonal
open-in-new icon button; the whole row opens the same system screen. Status
lines under a field (Connection token state, errors) use
`Widget.Home.FieldSupportingText`, so they share the field helper's 16dp
indent with 8dp between blocks.

`//projects/mobile/android/app:window_insets_screenshot_test` runs the real
activity with status/navigation/keyboard insets in the Neon and Emerald themes:
the Save bar bottom equals window height minus the keyboard and does not cover
the name field, the bottom navigation is hidden with the keyboard, the last
Home row ends above the FAB and the bottom navigation, and the fields use the
shared style. The same test opens «Phone notifications and find» in Russian and checks the
toolbar title is not ellipsized, clipped or broken inside a word, and inflates
every layout with a field (12+) to check the shared style: no underline,
all four corners 16dp, a 1dp `colorOutlineVariant` outline at rest and 2dp
`colorStateOn` focused, a non-empty label inside the box and text inside the
field's vertical bounds. On Connection and «Phone notifications and find»
every `MaterialButton` inside a card has the same 44dp height and corner, a
width from 120dp to less than the card, starts at the content edge, the alarm
editor Save is 48dp, and every card is a `Widget.Home.Card`. Alarm editor, connection and push screens run in
Neon and Emerald, dark and light. It writes
`insets-<theme>-<dark|light>-{alarm-editor-keyboard,connection,push-settings}.png`
and `insets-<theme>-home-end.png`.

## Design system

Every visual component, style and token lives in `core/designsystem`
(depends on `core/theme`; every screen module depends on it). The component
table, size/variant rules, tokens and how to add a component or a theme are in
[core/designsystem/README.md](core/designsystem/README.md). `core/ui` keeps
only non-visual utilities (base fragment/adapter/view model, insets, motion
helpers, notices, server text). Debug builds show **Design system** in
Settings: the catalog of every component in every variant, size and state.
`//projects/mobile/android/core/designsystem:designsystem_test` checks button
sizes, `colorError` on destructive buttons, one FAB outline in every theme and
renders the catalog (`catalog-<theme>-<dark|light>-<section>.png`);
`//projects/mobile/android/app:key_screens_screenshot_test` writes
`screen-<theme>-<mode>-<screen>.png` for Home, Scenarios, Time, Chat, System,
Connection and the alarm editor.

### Notes to the code (design system)

- Shared action labels are `ds_action_*` (`delete`, `cancel`, `retry`,
  `pause`, `resume`, `save`, `open_connection`) and the product name is
  `ds_product_name`; features do not copy them into their own strings.
- `PillSegments` never changes its own selection: a tap only calls
  `onSelectionRequested`, the selection moves when `render` calls `select`.
  Screens connect it through `PillSegmentsBinder` / `ScreenFragment.bindPills`
  (Appearance, Time, VPN, the catalog).
- Lists never flash on a partial change. `KeyedDiff` always returns a payload
  (`ItemChanged` when no aspect function is given), so `RecyclerView` rebinds the
  same holder instead of cross-fading two; `BaseListAdapter` turns off change
  animations. Rows with a toggle declare aspects with `aspectPayload` and
  rebind only what changed (`AlarmAspect`, `ReminderAspect`, `ScenarioAspect`);
  the dim of a disabled row fades with `renderDimmed`.
- Banners that push content (`vpn_route_banner`, `control_states_banner`)
  appear and disappear through `RevealBinder`: height and alpha animate
  together with `HomeMotion` timings. The visibility comes from the screen's
  ViewModel (`VpnToolbarViewModel.state.routeBannerVisible`).

## Themes

Themes are code in `core/theme`:

- `AppTheme` is the registry: key, title/summary, `ThemeOverlay.Home.*` style
  and a `ThemePalette` (a `ColorScheme` with every Material 3 role for light
  and dark; no defaults, so a missing role does not compile). Bundled:
  **Emerald** («Изумруд», key `current`, the original emerald palette,
  unchanged), **System** («Системная», key `system`, Material You) and
  **Neon** (near-black violet night, neon violet primary, electric
  cyan secondary, magenta tertiary; selected pills, chips and the navigation
  indicator are cyan on a deep violet `secondaryContainer`). Only display
  names changed in the rename; the stored keys stay, so a saved choice is kept
  (`ThemeRegistryTest`, `ThemeNamesTest`).
- No effect exists in only one theme: themes differ in colour, never in
  whether an outline, glow or accent is present. Audit (core/theme, core/designsystem):
  FAB outline was 0dp in Emerald/System and 1dp in Neon, and the FAB glow was
  black in Emerald/System; now every theme has a 1dp outline
  (`theme_fab_stroke`) and a glow in its accent (Emerald/System:
  `colorPrimary` with the soft 6dp elevation; Neon: cyan with 16dp). Already
  uniform: the on-device card outline (`DeviceStateViews`, `colorStateOn` in
  all themes), selected Appearance card outline, switch, pills, section
  labels. `ThemeEffectsTest` checks it.
- FABs share one style from `core/designsystem`: `Theme.Home` sets
  `floatingActionButtonStyle` / `extendedFloatingActionButtonStyle` to
  `Widget.Home.(Extended)FloatingActionButton`, which read the theme attrs
  `homeFabStrokeWidth`, `homeFabStrokeColor`, `homeFabGlowColor` and
  `homeFabElevation` (outline and glow on every FAB in every theme,
  including quick-action items, the close button and the chat scroll-down
  FAB, `Widget.Home.FloatingActionButton.SmallSecondary`). Speed-dial
  labels use `Widget.Home.FabLabel` (a `surfaceContainerHigh` pill).
  `FabStyleScreenshotTest` checks a 1dp stroke in every theme, equal strokes
  and unclipped glow.
- Popup menus share one component, `core/designsystem` `HomePopupMenu`: a menu resource
  (items with icons) rendered as `ui_popup_menu` — a `colorSurfaceContainer`
  card with the card radius, 48dp rows, 24dp icon on the left,
  `TextAppearance.Home.PopupMenuItem`, `Animation.Home.PopupMenu`. Used by
  the toolbar «⋮» menu, the chat attach menu (Photo, Text file) and the chat
  message menu. `Theme.Home` also sets `popupMenuStyle`/`listPopupWindowStyle`
  to `Widget.Home.PopupMenu`/`Widget.Home.ListPopupWindow` (same background,
  radius and text) for any framework popup. `toolbar_menu_screenshot_test`
  checks both menus and writes `menu-<key>-{actions,chat-attach,appearance}-dark.png`.
- No segmented buttons anywhere (`MaterialButtonToggleGroup`, bordered
  segments): a choice between a few options is `core/designsystem` `PillSegments`
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
- `core/designsystem` `DeviceStateViews` styles every device icon, status and card
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
  DataStore. `AppearanceController.state` is a `StateFlow` over the DataStore
  flow (read on the I/O dispatcher, default `Appearance()` until loaded); there
  is no blocking read at startup. `ThemeApplier` applies the current theme in
  `onActivityPreCreated`, remembers which theme each activity got and, while
  `loaded` is `false`, holds the first frame with a pre-draw listener. Each
  stored value is applied on the main dispatcher: night mode through
  `AppCompatDelegate`, and every activity created with another theme is
  recreated. So a theme change from the Appearance screen and a stored theme
  that arrives after the first activity follow the same path; the screen only
  dispatches `AppearanceIntent`.
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

- `core/designsystem/.../OutlinedFloatingActionButton.kt`
  - Material `FloatingActionButton` has no stroke attributes (only `ExtendedFloatingActionButton` does), so this subclass reads `strokeWidth`/`strokeColor` from its style and paints them over the content rect with a `MaterialShapeDrawable` of the same shape. Every plain FAB in layouts uses it so the theme style applies one outline everywhere.
- Device rows (`item_control_device.xml`, `item_control_device_row.xml`) share `control_device_icon*` and `control_device_text_gap`: the single card padding (16dp) equals group inset + row bleed (8dp + 8dp), so icons and titles line up across card kinds.

- `BUILD.bazel`
  - Unit tests only. `//projects/mobile/android:android_lint` is intentionally absent: it is a separate CI unit so its findings do not hide test results.
- `app/src/main/AndroidManifest.xml`
  - WebRTC: NetworkMonitor aborts natively without network-state access. Push / Find phone (features/push/impl): library permissions are dropped by rules_android, so the app declares them.
- `app/src/main/kotlin/dev/proflyder/home/di/AppGraph.kt`
  - A bearer token must never be forwarded to a redirect target.
- `app/src/main/res/menu/menu_main.xml` — One visible actions button. Tapping it opens the shared `HomePopupMenu` (core/designsystem); the framework overflow list is never used.
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
- `core/network/api` — client contracts (`SchedulerClient`, `ScenarioClient`, `IntercomClient`, `ControlCatalogClient`, `SystemDashboardClient`, `ChatClient`, `PushDevicesApi`, `VpnProfileApi`), `RouteGate`/`RouteCondition`, `@Idempotent`, `@NetworkInspection`, `StreamUnauthorizedException`. Features and `app` depend only on this module; `app` also links `core/network/impl` so Metro sees the contributions.
- `core/network/impl` — everything is `internal` except what Metro must reach from the graph module (`@Inject`/`@Contributes*` classes: clients, `HomeHttp`, `ConditionRouteGate`, `ActiveNetworkCondition`, `ConnectionHealth`, `NetworkBindings`); detekt `PublicInImpl` exempts exactly these annotations.
- `core/network/impl/.../HomeHttp.kt`
  - The only Retrofit/OkHttp factory. `rest(api, target)` builds one Retrofit per client on a per-target OkHttpClient derived from the shared base client (shared pool and dispatcher): redirects off (a bearer token must never follow a Location), `ConnectionInterceptor`, `TimeZoneInterceptor`, `TokenRefreshAuthenticator`, debug inspection interceptors. `stream(target, request, decoder)` is the one long-lived stream path (chat SSE, intercom `Watch`): no read timeout, no OkHttp connection retry, no inspection interceptors (HttpLoggingInterceptor BODY and Chucker read the whole body and a stream never ends), the call is cancelled with the collecting coroutine, 401/403 become `StreamUnauthorizedException`, other non-2xx an `IOException`. Reconnects are the caller's `reconnecting(Backoff)`. The placeholder base URL lives only here.
- `core/network/impl/.../OutcomeCallAdapterFactory.kt`
  - Retrofit `suspend fun …(): Outcome<T>` methods. The adapter wraps the Retrofit call into a `Call<Outcome<T>>` whose `enqueue` runs in the app scope: `RouteGate.await()`, then the asynchronous OkHttp call (`awaitResponse`), so no client needs `withContext(IO)` and nothing blocks an OkHttp thread. Only methods marked `@Idempotent` (GET and Connect `Get*`/`List*`) are retried after a transport failure: up to 3 attempts, `delay(150 ms × attempt)`. A cancelled call is rethrown as cancellation, never shown as a network failure. Connect error envelopes become `RemoteFailure.Server(httpStatus, ConnectCode, message, reason, textKey, textArgs)`; `reason` keeps the raw server code (application codes such as `chat_text_empty`).
- `core/network/impl/.../RouteGates.kt`
  - `ConditionRouteGate` combines every contributed `RouteCondition` (`ActiveNetworkCondition` from `DefaultNetwork`, `VpnRouteCondition` while the VPN connects on open) and waits for all of them for at most `ROUTE_WAIT` (20 s), the single "wait for the route" timeout of the app. After the timeout the call goes out and fails normally.
- `core/network/impl/.../ConnectionHealth.kt`
  - `AppStartable` that evicts the shared connection pool when the app returns to the foreground after at least 15 s in the background (dead keep-alive sockets after Doze) and when the default network changes. Activities do not know OkHttp.
- `core/network/impl/.../Target.kt`
  - Sealed target: endpoint field, `endpointOf(config)`, whether the Connect protocol header is sent, and the failure for a missing endpoint (scheduler: not configured; control/intercom: validation of their field).
- `core/network/impl/.../ConnectionInterceptor.kt`
  - Applies the *current* settings to every request, so Retrofit interfaces declare only relative paths: rewrites the URL onto the configured endpoint (including a path prefix), the bearer token only in the Authorization header, `User-Agent: home-android/<versionName>` (`AppVersion`), clamped timeouts (corrupted persisted settings must never make OkHttp reject a timeout).
- `core/network/impl/.../ConnectIntercomClient.kt`
  - Connect JSON client for `home.v1.IntercomService`. `Watch` is server streaming over HTTP/1.1 with the Connect envelope (`application/connect+json`: a 1-byte flags word, a 4-byte big-endian length, the JSON message); it reconnects with `Backoff` (1 s → 30 s, reset after a stream that ended cleanly) and never completes on its own.
- `core/network/impl/.../ControlCatalogClient.kt`
  - Wire format of GET /v1/catalog. An unknown kind stays OTHER, so an older app never drives a command it does not understand. Task payloads for commands live in `features/control/impl` (`ControlTaskPayloads`): a catalog id and an action only, never an HA service or entity; speak text travels only in the payload.
- `core/network/impl/.../HttpSupport.kt`
  - One response-size limit (`requireWithinLimit`, 8 MiB) for decoded bodies and error bodies. Transport errors keep their `toRemoteFailure` mapping, malformed JSON becomes a protocol failure.
- `core/network/impl/.../TokenRefreshAuthenticator.kt`
  - Refreshes the bearer token once after a 401 pushed from a rotated token. ConnectionInterceptor is an application interceptor and does not rerun for an authenticator follow-up, so this class sets the header itself from the current settings.
- `core/network/impl/.../WireDtos.kt`
  - Wire DTOs of the Connect/JSON APIs and the one `NetworkJson` configuration. Protobuf JSON sends `int64` as a decimal string, so every such field uses `Int64StringSerializer`; it also accepts a plain number because servers differ slightly in codec versions.
- `core/network/impl/src/test/.../ScenarioJsonTest.kt`
  - Saving the same scenario again must not drop the retained objects.
- `core/model/.../LocalJson.kt` — the one `Json` for on-device storage and feature payloads (VPN profile, notification history, scenario drafts and step params, chat outbox, Remote Config document); tolerant: unknown keys ignored, nulls omitted, invalid values coerced to defaults.
- `core/remoteconfig/src/main/kotlin/dev/proflyder/home/core/remoteconfig/FirebaseRemoteConnectionSource.kt`
  - Firebase Remote Config implementation of [RemoteConnectionSource]. Defaults are set locally, the latest activated values are fetched on start, and a real-time config listener activates pushed changes.
- `core/presentation/src/main/kotlin/dev/proflyder/home/core/presentation/StateViewModel.kt`
  - One state, typed intents in, one-shot events out. Events go through an unlimited `Channel`: an event emitted while the screen is stopped waits for the next `STARTED` collector and is delivered exactly once (not across process death). Background and rationale: `docs/audits/android-architecture.md`, section 4.
- `core/presentation/src/main/kotlin/dev/proflyder/home/core/presentation/ControlBinder.kt`
  - `SwitchBinder` keeps the control showing the last rendered state: a click only sends the wanted value as an intent and snaps back; the new position arrives from the ViewModel.
- `core/presentation/src/main/kotlin/dev/proflyder/home/core/presentation/AppEventBus.kt`
  - App-wide typed events so screens kept in the back stack refresh in the background. Buffer 64 with `DROP_OLDEST`; publishers never suspend. Use only when the source of truth is not already a shared `StateFlow`.
- `core/presentation/src/main/kotlin/dev/proflyder/home/core/presentation/LatestLauncher.kt`
  - Latest request wins: launching cancels the previous job, so a stale refresh cannot overwrite a fresher result.
- `core/presentation/src/main/kotlin/dev/proflyder/home/core/presentation/ListLoad.kt`
  - `ListLoadState` (refreshing + failure) replaces the per-screen `refreshing`/`loadError` fields; `ListRefresher` turns `suspend () -> Outcome<T>` into `LoadStep`s on a `LatestLauncher`. A soft refresh keeps the current indicator, a hard one shows it. A failure while items are visible is an event, not a field to consume.
- `core/presentation/src/main/kotlin/dev/proflyder/home/core/presentation/StateViewModel.kt` (visibility)
  - `screenVisible` is derived from the state's subscription count, so "the screen is shown" is a flow in the ViewModel: list screens refresh on `screenShown`, timers tick only while visible. No `onResume` hooks.
- `core/presentation/src/main/kotlin/dev/proflyder/home/core/presentation/ExclusiveActions.kt`
  - One row action at a time per list: `run` and the sequential `runAll` (bulk) report the busy key and the first failure; successes are applied by the caller's reducer.
- `core/presentation/src/main/kotlin/dev/proflyder/home/core/presentation/EditorDraftSession.kt`
  - Shared editor plumbing for alarms and reminders: draft codec, idempotent create key bound to the endpoint, completion that survives process death, and `EditorStatus` (loading/saving/loadFailed/error).
- `core/presentation/src/main/kotlin/dev/proflyder/home/core/presentation/Binders.kt`
  - `CheckableBinder`, `RadioGroupBinder` and `TextBinder` follow `SwitchBinder`: user input becomes an intent, programmatic rendering never reports back, and checkable controls snap back to the rendered value until the state changes.
- `core/settings/src/main/kotlin/dev/proflyder/home/core/settings/AndroidConnectionSettingsStore.kt`
  - Stores the bearer token as Keystore-encrypted ciphertext and composes it with the endpoints/timeouts supplied by [RemoteConnectionSource]. The manifest disables Android backup, so the token never leaves the device.
- `core/testing/src/main/kotlin/dev/proflyder/home/core/testing/TestFixtures.kt` — Loads a JSON payload from the test classpath (`src/test/resources/fixtures`).
- `core/ui/src/main/kotlin/dev/proflyder/home/core/ui/FailureNoticePolicy.kt` — `ForegroundGraceNoticePolicy` hides transient network failures for 6 s after the process returns to the foreground (`AppForeground.state` from `ProcessLifecycleOwner`, stamped with the injected `Clock`); `BaseFragment` asks the hosting Activity through `FailureNoticePolicyProvider`. Waking the phone races the Wi-Fi/VPN tunnel: a transient network failure in the first seconds after a resume is expected and must not become a blocking error notice.
- `core/ui/src/main/kotlin/dev/proflyder/home/core/ui/BaseFragment.kt`
  - Base for screens backed by a generated ViewBinding. The binding exists strictly between [onCreateView] and [onDestroyView]; it is released there so the Fragment never leaks its view hierarchy.
- `core/ui/src/main/kotlin/dev/proflyder/home/core/ui/BaseListAdapter.kt`
  - A typed DiffUtil change payload: the set of [A] aspects that changed on one row. A `getChangePayload` returning it makes the list animator reuse the existing holder (no crossfade) and lets [BaseViewHolder.bindPayloads] update only the affected views.
- `core/ui/src/main/kotlin/dev/proflyder/home/core/ui/Motion.kt`
  - Clear the pending end action first: cancelling must not run the previous exit's finalizer, so an interrupted exit keeps its current translation and alpha and the new animation continues from there.
- `core/ui/src/main/kotlin/dev/proflyder/home/core/ui/Notices.kt`
  - The single way the app shows transient feedback: a rounded tonal pill with a status icon, floating above the view tagged [R.id.home_notice_anchor] so it never covers navigation. A sideways or downward swipe dismisses it; a tap never does.
  - The activity root is not a CoordinatorLayout, so Material never installs a SwipeDismissBehavior: the notice must own the gesture.
- `core/designsystem/src/main/kotlin/dev/proflyder/home/core/designsystem/PillSegments.kt`
  - A row of compact text pills: the selected one sits on a stadium (the same shape and colour as the bottom navigation indicator) that slides to the newly selected pill. Used for the Time sections and the voice profile, so both switches look identical.
- `core/ui/src/main/kotlin/dev/proflyder/home/core/ui/ServerText.kt`
  - Resolves a server `LocalizedText.key` to its local resource. Resources are named mechanically after the key: `scheduler.error.not_found` is `@string/server_scheduler_error_not_found` (prefix `server_`, dots become underscores), so adding a server key never …
- `core/designsystem/src/main/res/drawable/ds_bg_pill_segment.xml` — Touch feedback of one pill. The selected stadium is drawn by PillSegments itself so it can slide between pills.
- `core/designsystem/src/main/res/values/styles.xml`
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
  - The enable switch is a `SwitchBinder`: a tap sends `AlarmsIntent.Toggle` and the switch keeps the rendered value. Turning off a repeating alarm first asks (skip once / turn off) through `AlarmsEvent.AskDisable`; cancelling the dialog needs no restore because the switch never moved.
- `features/alarms/impl/src/main/kotlin/dev/proflyder/home/feature/alarms/presentation/AlarmEditorViewModel.kt`
  - The draft, the pending create idempotency key (with the endpoint it was made for) and the completion live in `SavedStateHandle` through `EditorDraftSession`. An edit drops the key; a timeout keeps it, so a retry after process death reuses it; a different server gets a new key. A completion survives process death and is delivered once as `AlarmEditorEvent.Completed`.
  - The repeat sheet is `AlarmRepeatSheetViewModel` on `StateViewModel`; the radio list and the day toggles are binders that only send intents.
  - Applies the sheet's choice. A custom day set that covers a preset is stored as that preset, so "Пн–Пт" never lingers as a custom list.
- `features/alarms/impl/src/main/kotlin/dev/proflyder/home/feature/alarms/presentation/AlarmRepeatRules.kt`
  - Day sets are ISO weekdays: 1 is Monday, 7 is Sunday. A custom selection that covers exactly a preset is stored as that preset, so the editor and the home server never show two different names for the same days.
- `features/alarms/impl/src/main/kotlin/dev/proflyder/home/feature/alarms/presentation/AlarmsFragment.kt`
  - Keep the existing list visible while a refresh is in flight; do not replace it with a spinner.
  - Alarms, reminders and timers have no `onResume` refresh. Their list ViewModels refresh when the screen starts collecting state (`StateViewModel.screenShown`, also after the app returns to the foreground) and when the matching editor publishes `AlarmsChanged`/`RemindersChanged` on `AppEventBus`, so a list kept alive in the Time pager is already fresh when shown. A stale load cannot overwrite a newer one (`ListRefresher` runs on `LatestLauncher`).
  - Dialogs (delete, bulk delete, disable choice) are events from the ViewModel; the fragment only shows them and sends the chosen intent.
- `features/alarms/impl/src/main/res/layout/item_alarm.xml`
  - Idle: switch plus delete. Selection mode: both are gone and the circular check indicator takes their place, inset by the card padding so it never touches the rounded corner.
- `features/alarms/impl/src/main/res/layout/sheet_alarm_repeat.xml` — Modal repeat sheet. The rounded top and the drag handle live on the sheet content itself, so the sheet keeps the app's dark surface when the dialog window is transparent.
- `features/alarms/impl/src/main/res/values/alarms_dimens.xml` — Fallback before the bottom bar is measured; matches its real height: a 64dp icon+label action plus the 6dp bar padding on both sides.
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
- `features/control/api/src/main/kotlin/dev/proflyder/home/feature/control/ControlRepository.kt`
  - `catalog` is the single source of truth for Home devices: a `StateFlow` with `configured`/`loading`/`failure` and the last good catalog (a failed reload keeps it).
  - Queues "speak this text on the station". The text is user content: it only ever goes into the task payload, never into a URL or a log line.
  - `VoiceProfileSwitch` exposes the assistant profile for the toolbar chip without the app knowing about the catalog.
- `features/control/api/src/main/kotlin/dev/proflyder/home/feature/control/QuickTileProvider.kt` — A feature contributes a dial tile with `@ContributesIntoSet`; `activate()` either handles the tap itself or asks Home to open a feature tab. Home knows no feature by name.
- `features/control/api/src/main/kotlin/dev/proflyder/home/feature/control/SpeakText.kt` — Backwards-compatible name for the shared speech rules now living in `packages/kotlin/core`. The Home screen and the scenario builder must agree on exactly the same normalization.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/CompactControls.kt`
  - One pure rule deciding how a control inside a device card is drawn: a round compact tile or a normal row. A tile is a binary on/off control — it accepts both turn_on and turn_off — inside a card that holds at least two controls.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/ControlItems.kt`
  - A section header. [section] identifies the section (not its title), so a room that happens to be called "Home" never collides with another section. [summary] is a short right-aligned note ("2 of 4 on").
  - The chip counts describe primary controls only: a device's settings are part of it and never add a chip or a count of their own.
  - The chips: All, then the device categories that exist, then each room (only when there is more than one, otherwise rooms add nothing).
  - [leading] items (the live rows) come first, then available devices grouped by room and, inside a room, by device, then unavailable ones. The profile entry is never a row.
  - Visibility comes first: a conditional entry whose target is not in the requested state must never reach cards, chips, counters or the "Unavailable" section.
  - Dead rows follow the applied filter too, and a lone setting never shows up as its own unavailable device.
  - A group is shown when one of its primary controls matches the filter; its settings then follow the device, never the chip.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/SpeedDial.kt` — One row of the Home quick-actions dial: the contributed tiles, the pinned devices, scenarios and speak action, then the resolved wake-feedback marks, so a newly pinned favourite appears automatically.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/WakeFeedback.kt`
  - Resolves the two Home Assistant scripts that mark a wake event from the catalog. They are ordinary personal scenarios whose entity ids are part of the house contract (`projects/home-assistant/ha-voice/scripts.yaml`), so they are matched by id and never by label.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/data/ConnectControlRepository.kt`
  - The catalog is shared through `stateIn(appScope, WhileSubscribed)`; fetching runs only on triggers of `RefreshTriggers` (foreground ticker, explicit refresh, settle steps), so nothing polls in the background.
  - The executor applies commands asynchronously, so a read right after a tap can still return the old value. The command records an `Expectation`; `OptimisticDeviceState` overlays it until the server agrees or the TTL passes, and the settle steps of `ControlRefreshPolicy` stop as soon as it is met.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/data/HomeHubFeed.kt`
  - A failing source keeps its last known list: the hub is optional decoration over the catalog, so one unreachable feature must not empty the whole block.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/presentation/ControlViewModel.kt`
  - The ViewModel only dispatches intents to delegates: `ControlScreenSources` (combines catalog, hub with a one-second ticker while a timer runs, favorites, filter, pending sets and layout), `DeviceCommands`, `HubCommands`, `ControlFavorites`, `ControlLayoutState`, `QuickTileLauncher`. Tap routing is the pure `ControlTaps`.
  - Default favorites are persisted once (`saveIfAbsent`) and only from a loaded catalog and scenario list, so a failed load never stores a wrong default.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/presentation/EntryRowBinder.kt`
  - One binding for an entry row in a standalone card and inside a device group (power, volume, button variants). Switches render through `SwitchBinder`: a click only sends the intent and the position comes back with the state. The recurring-alarm switch in the hub works the same way, so a cancelled skip/disable dialog never leaves a false off state.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/presentation/ControlIcons.kt`
  - The voice profile is a mode, not a device: its "message" hint must not reuse the speak artwork.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/presentation/SpeakDialog.kt`
  - The "say on the station" dialog: a multiline text with a 500-character counter. "Say" stays disabled until the text is speakable; the text itself is never logged, only put into the queued command.
- `features/control/impl/src/main/kotlin/dev/proflyder/home/feature/control/presentation/VolumeSheet.kt`
  - Bottom sheet of the station volume: the main level with a mute toggle and the announcement level. A command is sent when a slider is released, not while dragging.
  - Hardware volume keys adjust the station by five percent per press. The event is consumed even at the edge (0 or 100), so the phone's own stream never changes while the sheet is open.
- `features/control/impl/src/main/res/drawable/control_tile_ripple.xml` — Round pressed highlight for a compact tile: the oval mask keeps the ripple inside the icon circle instead of the square tile cell.
- `features/control/impl/src/main/res/layout/item_control_hub.xml` — Compact "next up" block: a short list of live rows, hidden when empty. Rows fill the card edge to edge so a row ripple is clipped by the card corners instead of stopping short of them.
- `features/control/impl/src/main/res/layout/overlay_control_speed_dial.xml` — Full-window speed dial: scrim above the bottom navigation, item FABs and a copy of the root FAB. Nothing here paints a panel background.
- `features/control/impl/src/test/kotlin/dev/proflyder/home/feature/control/presentation/ControlRowsTest.kt`
  - Robolectric smoke of the real rows: a tap only sends an intent, the switch keeps the rendered state until the next state arrives, and re-binding a group keeps its row views.
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
- `core/runtime/src/main/kotlin/dev/proflyder/home/core/runtime/AndroidDeviceIdentity.kt` — `DeviceIdentity` (core/model) shared by push registration and VPN profiles; the DataStore key stays `push_device_id` so existing installs keep their id.
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
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/FirebasePushTokenSource.kt`
  - The only place that touches `FirebaseMessaging`: it reads the current token at start and publishes refreshed or revoked tokens as `StateFlow<String?>`.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/HomePushRegistrar.kt`
  - `combine(token, server, name)` → due check → register, under one `Mutex`, so a rename and the flow never send the same registration twice. `busy` comes from the in-flight counter (`InFlight`), which is decremented even when the call throws.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/HomeMessagingService.kt`
  - FCM entry point. It only decodes the message and forwards tokens and messages to `PushInbox`, which works in the app scope: the service's own lifetime ends right after the callback and must not cancel a token or history write.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/NotificationHistoryCodec.kt`
  - One stored row. It mirrors [NotificationHistoryEntry] instead of serializing the API model directly, so a renamed field never breaks an existing file and an unknown type downgrades to an ordinary notification.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/PushChannels.kt`
  - The app's two notification channels. Channels are created once and their properties cannot be changed later, so a channel id must never be reused with different settings: Android keeps the original sound and vibration of `home_phone_find`, which is why the …
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/PushNotifier.kt`
  - Draws the two notification shapes. All user-visible text comes from resources; a server message supplies the title and body of an ordinary notification and nothing else.
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/PushPayloads.kt` — One decoded FCM message. The server sends two shapes:
- `features/push/impl/src/main/kotlin/dev/proflyder/home/feature/push/AndroidPushPermissions.kt`
  - Do Not Disturb access lives outside the app's own settings page. The settings screen re-reads the permissions whenever it starts collecting state (`screenShown`), so returning from a system page shows the new state without an `onResume` hook.
- `packages/kotlin/core/src/main/kotlin/dev/proflyder/home/core/text/Utf8Limit.kt` — Truncates to the server's byte bound without splitting a UTF-8 character.
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
- `core/runtime/src/test/kotlin/dev/proflyder/home/core/runtime/DeviceIdentityTest.kt`
  - The stable device identity survives token rotation and is never the raw platform id.
- `features/reminders/impl/src/main/kotlin/dev/proflyder/home/feature/reminders/presentation/ReminderAdapter.kt`
  - The enable switch is a `SwitchBinder`: its position changes only when the server answer reaches the state.
- `features/reminders/impl/src/main/kotlin/dev/proflyder/home/feature/reminders/presentation/ReminderEditorViewModel.kt`
  - Same editor contract as alarms: `EditorDraftSession` keeps the draft, the pending create key and the completion in `SavedStateHandle`.
- `features/timers/impl/src/main/kotlin/dev/proflyder/home/feature/timers/presentation/TimersViewModel.kt`
  - The countdown is a `tickerFlow(1 s)` that runs only while the screen collects state (`screenVisible`). A tick recomputes `rows` from the server clock; the adapter diffs by timer id and rebinds only the remaining time (`TimerAspect.REMAINING`). `TimerRefreshPolicy` reloads every 30 s and once per 10 s after a running timer reaches zero; it reads the injected `Clock`, never `System.nanoTime`.
  - The start form (label, duration, inline error) lives in the state; presets are `TIMER_PRESETS: List<Duration>` and fill the field through `Duration.toTimerInput()`.
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
- `core/model/src/main/kotlin/dev/proflyder/home/core/model/Outcome.kt` — `Outcome`/`RemoteFailure` live here, not in the scheduler models. `RemoteFailure.Server.httpStatus` carries the HTTP status as a number (callers such as `VpnProblems` map 400/409/429/503 on it instead of parsing `"http_409"`); `code` is the Connect error code and may be blank. `RemoteFailure.Storage(reason: StorageReason)` has no text: `ErrorText` maps each reason to a resource, and the chat outbox maps `ATTACHMENT_MISSING` to its permanent failure key.
- `core/model/src/main/kotlin/dev/proflyder/home/core/model/ServerClockSync.kt` — The server clock is synced by one service (`SchedulerServerClockSync`, `GetStatus`), used by both timers and the Home hub; `TimerRepository` no longer owns clock sync.
- `core/runtime/src/main/kotlin/dev/proflyder/home/core/runtime/TickerFlow.kt` — `tickerFlow(period, initialDelay)` emits at once and then every period while collected; collect it in a ViewModel instead of `while (true) { delay() }` in UI.
- `core/runtime/src/main/kotlin/dev/proflyder/home/core/runtime/Backoff.kt` — `Backoff(initial, max, factor)` and `Flow.retryWithBackoff`; the attempt counter is the one `retryWhen` passes, so a long-lived stream keeps growing towards `max` until it is resubscribed. `Flow.reconnecting(backoff, shouldRetry)` is for streams that must stay connected: it resubscribes after a clean end (after `initial`) and after a retryable failure (growing delay, reset by a clean end).
- `core/runtime/src/main/kotlin/dev/proflyder/home/core/runtime/AndroidDefaultNetwork.kt` — `DefaultNetwork.handle`: the current default network handle (`null` when offline) from `registerDefaultNetworkCallback`, shared while observed.
- `core/runtime/src/main/kotlin/dev/proflyder/home/core/runtime/AndroidAppVersion.kt` — `AppVersion.name` from the package `versionName`.
- `core/runtime/src/main/kotlin/dev/proflyder/home/core/runtime/ProcessAppForeground.kt` — Single source of `AppForeground.state`; registered on `ProcessLifecycleOwner` as an `AppStartable`, so no Activity writes foreground state.
