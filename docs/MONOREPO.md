# Монорепозиторий и Bazel

Статус: **актуальная структура**. HA/voice/Hermes/TTS и station/Pulsar/BLE
разложены по `projects/`; текущий runtime — `STATE.md`.

## Горизонтальная структура

Главный принцип: **solutions переиспользуются, projects их композируют**.

```text
solutions/               переиспользуемые решения
  scheduler/             durable scheduler, SQLite queues, Connect/gRPC-Web
  ha-bridge/             executor-адаптер HA/Pulsar ↔ scheduler
                         (Rust-крейт: бинари ha-bridge, ha-control и ha-scenario)
  mcp-gateway/           MCP Streamable HTTP: infra.read / infra.fix / infra.report
  currency-bot/          курсы kurs.kz, алерты, Telegram webhook
  voice-proxy/           OpenAI-совместимый прокси для Piper TTS
  wakegrant/             one-time logon grants for a WoL machine (PROTOCOL.md)
  windows-logon-provider/ Windows Credential Provider (Rust, Bazel)
  heartbeat/             внешний dead-man switch (Cloudflare Worker)
  bb-home-dispatcher/    плагин bb: исполнитель голосовых команд
  bb-profiles/           плагин bb: изолированные профили

packages/                общий код и контракты
  proto/                 gRPC/protobuf (сгенерированные контракты)
  contracts/             межъязыковые протоколы (wakegrant)
  rust/                  общие Rust-крейты
  kotlin/                общий Kotlin-код (Android)

projects/                композиции и deployment
  station/               Pulsar/ADB/BLE/provisioning
  home-assistant/        HA/MA/STT/TTS/Hermes
  home/                  station + home-assistant + scheduler/bridge/control
  mobile/android/        Kotlin/Views клиент (dev.proflyder.home)

platform/                Bazel/toolchains/deploy-шаблоны
docs/                    документация
```

### Почему так

- `solutions/scheduler` не знает, где он запущен;
- будильник, напоминание и таймер — domain conveniences поверх durable
  scheduling и очередей, а не отдельные планировщики;
- `projects/station` и `projects/home-assistant` независимы;
- `projects/home` — их композиция плюс домашний scheduler и HA bridge;
- `solutions/ha-bridge` арендует только `queue=home`; бинарь `ha-control`
  того же крейта — только `queue=control` и каталог Home;
- один и тот же код не копируется между проектами;
- project-specific секреты не попадают в solution.

Пример:

```text
projects/home:
  scheduler@home      sqlite=/data/home/scheduler.db  :8098
  ha-bridge@home      SCHEDULER_ENDPOINT=127.0.0.1:8098, queue=home
  ha-control@home     каталог Home, queue=control
  android             Connect JSON → LAN/VPN HTTPS endpoint
```

Единственный process scheduler — Rust-крейт
`solutions/scheduler/crates/scheduler`, currency-bot — крейт
`solutions/currency-bot/crates/currency-bot`.

## Решения по языкам

| Слой | Язык | Почему |
|---|---|---|
| сервисы платформы: `scheduler`, HA-исполнители, `voice-proxy`, `mcp-gateway`, `currency-bot` | Rust (решение 41) | общий core-слой `packages/rust` вместо копий платформенного кода, типизированные контракты, чистый домен; переход по одному сервису через golden-фикстуры `testdata/contract/` |
| `voice-proxy` | Rust (решение 41) | `domain`/`openai`/`relay`/`services` на общем `home-*` слое: `home-config`, `home-ha`, `home-proto`, `home-worker`, `voice-contract`, `home-contracttest`; golden-фикстуры и байты payload очереди перенесены без изменений, статический musl-образ |
| `wakegrant` | Rust (решение 41, пилот) | первый потребитель общего core-слоя: `wakegrant-protocol`, `home-config`, `home-sqlite`, `home-contracttest`; статический musl-образ |
| аудио/DSP/инференс | Rust | предсказуемая производительность, нет GC, FFI |
| `projects/mobile/android` | Kotlin + Views | server-authoritative клиент; Metro DI, single Activity, typed Fragment router |
| `solutions/heartbeat` | JavaScript ES modules | только Cloudflare Workers edge runtime: без сборки и npm, `node --test`, минимальный внешний контур |
| `solutions/bb-home-dispatcher`, `solutions/bb-profiles` | JavaScript/TypeScript (плагины bb) | исполняются внутри bb как path-источник; `node --test` без внешних сервисов |
| Windows logon provider | Rust (`windows-bindgen`/`windows-core`) | in-proc COM DLL для LogonUI; `rules_rust` + герметичный zig (`x86_64-pc-windows-gnu`) через Bazel, без Visual Studio; переносимые host-тесты и clippy под Windows |
| HA-конфиг | YAML | существующий контур, не переписывать ради модности |
| Pulsar (станция) | Rust (`rules_rust`, Zig cross) | first-party форк upstream, сборка Bazel |
| `projects/station/ble-gateway` | Rust (`zbus`, `tokio`, `rules_rust`) | прямой D-Bus без libdbus; члены общего корневого Cargo workspace, host-тесты и armhf-кросс через Bazel |
| CI-обвязка (`platform/ci`, `platform/quality`) | JavaScript (`actions/github-script`, `node --test`) | Node из `mise.toml` (CI — `jdx/mise-action`), без pip; локально — `mise install` |
| Bazel-инструменты Android (`platform/android/tools`) | Kotlin/JVM (`rules_kotlin`, exec-конфигурация) | герметичные `kt_jvm_binary` в песочнице Bazel, тот же язык, что и Android-приложение; тесты — JUnit `kt_jvm_test` |
| `platform/deploy-rs` (`home-deploy`) | Rust (`rules_rust`, static musl) | все инструменты деплоя и in-cluster Job — один distroless-бинарник, без Python в кластере; `platform/deploy/resolve.sh` остаётся точкой входа скриптов |
| runtime health exporter | Rust (`projects/home-assistant/deploy/health-exporter/crates/health-exporter`) | как остальные сервисы |
| задачи на хостах | shell / Python stdlib (короткая обвязка) или Rust-бинарник (большая логика) | Go в репозитории нет |
| ML wake-word, dev-утилиты станции, проверки YAML, live-e2e | Python | экосистема ML; в проде не исполняется |

Rust — для сервисов, станции, CronJob/Job, инструментов деплоя
(`platform/deploy-rs`) и серверных Bazel-инструментов. JavaScript — только для
edge Worker, которому нужен runtime Cloudflare, а не общий backend-стек.
Инструменты Android-сборки идут на языке Android-стека — Kotlin. В репозитории
нет Go: языки — Rust, Kotlin, Python, JS/TS.
Mobile не владеет расписанием и не заменяет серверные будильники.

## Bazel

```text
MODULE.bazel       Bzlmod: rules_rust, rules_proto, rules_oci, Android
MODULE.bazel.lock  зафиксированные зависимости
.bazelversion      Bazel 9.3.2
.bazelrc           cache/вывод
.bazelignore       тяжёлые/чужие/legacy каталоги
BUILD.bazel        exports_files корня
Cargo.toml         единый Cargo workspace всего first-party Rust
Cargo.lock         единственный lockfile workspace (cargo)
crate-universe.lock  generated Bazel lock для @crates
rust-toolchain.toml  канал 1.99.1 и targets всех Rust-целей
```

Инструменты Android-сборки в `platform/android/tools` — Kotlin/JVM-программы
(`kt_jvm_binary` в exec-конфигурации), а их тесты — JUnit `kt_jvm_test`; genrule'ы
ViewBinding/google-services и `android_lint` запускают их по `$(execpath)`, не
требуя `python3` в песочнице. Python в Android-сборке не используется.

`MODULE.bazel` — глобальная CI/deploy-инвалидация, сужаемая по структурному
диффу base/head. Если меняется только Maven-расширение rules_jvm_external
(`maven.install(...)`/`maven.<tag>(...)`, `use_repo(maven, ...)`),
`affected.mjs` маршрутизирует изменение как `MODULE.bazel#maven` только в
android (меняется Maven/APK-замыкание). Если меняются только native-входы —
`rust.toolchain`/crate_universe, `zig_sdk_repository`, регистрация
`@zig_0_14_1//` toolchain'ов, `single_version_override(module_name =
"rules_rust")` и сторонние C/C++ станции через `_SENDSPIN_BAZEL`
`http_archive` — путь становится `MODULE.bazel#native` (платформы `native` и
`bazel`). Смешанный дифф даёт оба виртуальных пути. Остальные правки —
`bazel_dep`, `oci`, python — остаются глобальными. `.bazelrc`
сужается до `.bazelrc#cache` (ни одной платформы), только если дифф меняет
исключительно cache/output-флаги; `platform/bazel/zig/**` и
`platform/bazel/windows/**` — только `native` (`platform/bazel#native`).
Подробности и белый список флагов — `docs/TESTING.md`.

Чистый checkout проверяется `bazel test //solutions/... //packages/...`; Go в
репозитории нет. Pulsar installer — Rust-крейт
`projects/station/pulsar/installer`, проверяется Bazel-целями
`//projects/station/pulsar/installer/...`.
`mise run test` пропускает (`SKIP`) тесты, которым нужны игнорируемые локальные
источники (см. `docs/TESTING.md`).

Команды:

```bash
bazel build //solutions/scheduler/... //solutions/ha-bridge/... //packages/proto/...
bazel test  //solutions/scheduler/... //solutions/ha-bridge/...

bazel test //packages/...
```

`projects/station/ble-gateway` — Rust-крейт `zbus`/`tokio` на `rules_rust`:
`//projects/station/ble-gateway:ble-gateway` собирается armhf тем же zig
toolchain'ом, что и Pulsar. Android собирается root-инструментами в
`projects/mobile/android`.

### Rust: единый workspace

Весь first-party Rust — один Cargo workspace в корне (`/Cargo.toml`,
`/Cargo.lock`, `/rust-toolchain.toml`): Pulsar (`satellite/crates/*`), BLE
gateway, Windows logon provider и станционный helper. Общий
`[workspace.dependencies]` пинит точные версии, `[workspace.package]` задаёт
edition 2024 и `rust-version`, `[workspace.lints]` — единый набор rust/clippy
предупреждений (CI собирает с `-D warnings`). Общий код с двумя и более
потребителями живёт в `packages/rust/`; solution/package не импортирует
project. Сейчас там `home-telemetry`, `home-runtime`, `home-actor`,
`home-linux`, `home-secret` и `home-http` — bootstrap логов,
shutdown/supervisor, handle актора, errno и sysfs, zeroizing-токены и
axum bearer/serve; BLE gateway — первый потребитель, Pulsar использует их в
демоне.

Bazel читает тот же workspace: одно расширение `crate.from_cargo(name =
"crates", ...)` с `//:Cargo.lock` и сгенерированным
`//:crate-universe.lock` порождает `@crates//` для всех Rust-целей. Каждый
crate-пакет объявляет `extract_cargo_lints(... workspace = "//:Cargo.toml")` и
передаёт его в `lint_config`, поэтому `cargo clippy` и `rust_clippy_test`
линтуют ровно `[workspace.lints]`. `MODULE.bazel` держит объединённый набор
`supported_platform_triples` (host macOS/Linux, обе станционные архитектуры,
`x86_64-pc-windows-gnu` и статический `x86_64-unknown-linux-musl` для
сервисных образов).

`projects/station/pulsar` — Rust-хост на `rules_rust`: BUILD-цели лежат в
`satellite/crates/*`, а тесты/fmt/clippy идут юнитом `pulsar-bazel`
(`_unit.yml`). Станции собирает тот же Bazel: герметичный zig 0.14.1
cc-toolchain (`platform/bazel/zig/`) с платформами `aarch64_linux`/`armhf_linux`,
sendspin-cpp и C/C++-зависимости — `http_archive` + BUILD-overlay'и в
`satellite/third_party/bazel/`. Юнит `pulsar` (`_native.yml`) собирает обе
архитектуры на каждом PR и публикует `tree-<hash>`; на merge в `develop`
`native-release` только продвигает этот digest на релизные теги.
Установщик — Rust-крейт того же workspace, radio/ssh остаются своими
скриптами. Цели Linux-only; на macOS `bazel build //...` их пропускает.
`.bazelignore` исключает сгенерированные каталоги `satellite/` и игнорируемые
локальные каталоги (`secrets`, `resources`, …).

`solutions/windows-logon-provider` — Rust-компонент на том же Bazel:
`packages/rust/wakegrant-protocol` (переносимый слой, общий с сервисом),
`crates/provider` (COM DLL) и `crates/cp-selftest` (self-test):
`serde`/`serde_json`, `base64`, `thiserror`, `http::Uri` и `home-secret` в
protocol; `tracing` с провайдерским Win32-писателем журнала в DLL.
`MODULE.bazel` добавляет
`x86_64-pc-windows-gnu` в Rust-тулчейн и регистрирует zig cc_toolchain
(`platform/bazel/windows:x86_64_windows`), поэтому DLL линкуется без Visual
Studio и Windows SDK. Два патча rules_rust добавляют gnu-трипль и убирают
MSVC-only import library; при обновлении rules_rust их надо перепроверить.
`build.sh`/`run-tests.sh` — тонкие обёртки над `bazel build`/`bazel test`
(host-тесты + rustfmt/clippy, включая Windows-цели); CI — платформа `native`
(`_native.yml`) с pinned Bazelisk. Значения `ServiceUrl`/`Machine`/`UserName`
приходят из проекта при установке. PR CI собирает файлы и публикует их как
публичный OCI-артефакт GHCR под `tree-<hash>` (ORAS, `VERSION`, digest и
provenance); на push в `develop` `deploy-home` только продвигает проверенный
digest, а домашний ПК читает готовый артефакт.

## gRPC / Connect

Канонические контракты — `packages/proto/home/v1/scheduler.proto`
(`home.v1.SchedulerService`, основной) и `alarms.proto`
(`home.v1.AlarmService`, legacy facade). Один listener отдаёт Connect
JSON/binary и gRPC-Web по HTTP/1.1; отдельного REST API нет. Rust-стабы
лежат в репозитории и перегенерируются pinned-скриптом
`packages/rust/home-proto/generate.sh`; `proto_library` остаётся канонической
схемой. Подробности — `GRPC.md`.

## Границы

- HA/Pulsar: звонок, рингтон, TTS, громкость, restore source.
- `solutions/scheduler`: durable schedules, timers, outbox, generic queue,
  leases/ack, server clock.
- `solutions/ha-bridge`: executor adapter; арендует `queue=home`, вызывает HA
  scripts, подтверждает события. Не владеет расписанием.
- Общий golden-раннер контрактов живёт в `packages/rust/home-contracttest`.
- `solutions/bb-home-dispatcher` и `solutions/bb-profiles`: плагины bb, а не
  сервисы. Диспетчер ничего не знает о проектах, кроме ответов `profiles`;
  профиль и allowlist провайдеров/машин живут только в `profiles`, голосовой
  шлюз видит их через RPC. HTTP-маршрут плагина — loopback, наружу его
  публикует только SSH forced command.
- `projects/mobile/android`: управление server state, Connect JSON; нативных
  phone alarm/push пока нет.
- `projects/*`: композиция, env, манифесты K3s, systemd.

## Следующие этапы

Открытые задачи — только `PENDING.md`.

## Владение тестами

Тесты относятся к своему solution/package/project и находятся рядом с
кодом. E2E композиции находятся в `projects/<name>/e2e/`. Общая
инфраструктура проверок репозитория — `platform/quality/`. Корневая
папка tests не используется; `mise run test` агрегирует локальные наборы.

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `.bazelrc` — Android-only flags (version defines, the Java toolchain of the Android build tools, protoc/dex workers) live in `platform/android/android.bazelrc` and are activated by `--config=android` in every Android task (the mise unit dispatcher, the whole-workspace mise tasks, `distribute.sh` and the CI cache warm build).
  - `rust_clippy_test` only exits non-zero on rustc errors: the levels from the root Cargo.toml `[workspace.lints]` (forwarded through `extract_cargo_lints`) are `warn`, so without this flag a new clippy warning passes both the local build and PR CI.
  - The cache is shared by every checkout/worktree and has no limit by default. The server evicts the least recently used entries in the background while idle; builds never fail on it.
- `.github/actions/bazel-repository-cache/action.yml` — The Bazel repository cache (downloaded archives: GitHub releases, crates.io, Maven jars) is content-addressed and is *not* written by PR jobs. Only the host-Rust family is archived (`bazel-repo-rust-<os>-<hash>`): Android's downloads are mostly Maven JARs that Nexus already serves, so caching them cost more budget than it saved.
- `.github/actions/nexus-cache/action.yml` — Single switch: the caller passes the repository variable NEXUS_HOST and the repository secret NEXUS_CI_PASSWORD. An empty value, a missing secret (fork PRs) or an unhealthy Nexus leaves the build on the direct upstream URLs, so CI degrades to the normal path instead of failing.
  - Probe through the path the public ingress actually exposes (the admin `/service/rest` API is not routed there). npm ping is tiny and cached upstream. If the tunnel or the pod is down, do not rewrite anything: a rewrite replaces the original URL.
- `.github/workflows/_android-release.yml` — Sign and distribute the Home Android APK to Firebase App Distribution.
  - Release-note inputs for `distribute.sh --ci --upload-only`. Passed as environment data, never interpolated into the script body, so a PR title cannot inject shell.
  - Upload-only: the script verifies the APK digest against release-info.json and never builds or signs anything. The release key is applied to the already-tested APK; release-info.json is rewritten for the signed bytes and the upload verifies that digest.
- `.github/workflows/_android.yml` — Reusable Android platform workflow: one matrix entry per affected application (or shared Kotlin library) unit, plus the dedicated Android Lint and detekt units. The unit is selected by the Bazel reverse dependency graph in `_ci.yml` and runs every declared target (Bazel's cache skips unaffected tests).
  - The app is always built from its declared target: this entry owns the distributable APK, so a narrowed (possibly empty) build list must not leave the artifact missing. BAZEL_EXTRA_ARGS carries the release versionCode/versionName.
  - The apk_abi_test log is only printed on failure (--test_output=errors), so repeat the ABI check on the built APK to show the packaged ABIs in the job log ("APK ABIs: arm64-v8a").
  - The tested, still debug-signed APK plus its release metadata. Only `android-distribute` applies the release key (sign-release.sh), so this job never sees signing secrets.
- `.github/workflows/_ci.yml` — Smart CI: only the platforms and units a change actually touches.
  - Firebase App Distribution of the Home Android app on every PR that changes it: the `android-app` matrix entry builds and tests exactly one APK (`android-app-apk`, no secrets involved); this job signs those bytes with the release key and uploads them.
  - One matrix entry per affected native build: the Windows logon provider (Bazel + rules_rust, hermetic zig toolchain for x86_64-pc-windows-gnu) and the Rust Pulsar tree (Bazel for both arches, installer, radio, ssh;
  - Expensive regression with real linux/amd64 release binaries. Only when the scheduler, ha-bridge (ha-control) or the Kubernetes deployment changed.
- `.github/workflows/_cleanup.yml` — Housekeeping pipeline (`workflow_call`), called by two thin wrappers with one trigger each: `cleanup.yml` (daily schedule, always real) and `cleanup-manual.yml` (`workflow_dispatch`, dry run by default). The caller states `dry_run`; `github.event_name` is the caller's event and is never used to branch. A failure in either job leaves the other untouched.
  - Only the Firebase service account: the cleanup never signs or uploads.
- `.github/workflows/_deploy-control.yml` — Mocked-kubectl regression for deploy.sh control mode. It needs the Rust home-deploy CLI (the script resolves HOME_DEPLOY) and pre-resolved image digests, so no image build or cluster access is involved.
- `.github/workflows/_deploy-home.yml` — Home application release from `develop`. Every deployable unit is applied by Flux: this workflow only promotes the digests the merged PR built and attested and commits them to the machine branch `deploy/home-apps`, which Flux reads. The unit map is `platform/ci/units.mjs`, so adding a deployable unit does not need a workflow edit.
  - Save the caches that PR jobs restore. `ci.yml` does not run on push to `develop`, so this is the only place where a develop-side cache is written. It runs on push only, in parallel with `plan`, with no Environment, no secrets and no VPN;
  - A cold Android app build is the bulk of this job (the deploy itself only starts after `plan`, so the extra minutes do not delay a roll-out).
  - Every host Rust build/test target; the deploy itself only starts after `plan`, so the extra minutes do not delay a roll-out.
  - Native units build once too (DECISIONS 36). `_native.yml` promotes the digest the merged PR built and attested under `tree-<hash>`: it checks the tag exists, verifies provenance against this repository's CI workflows, then attaches the unit's release tags …
- `.github/workflows/_js.yml` — Reusable JavaScript platform workflow: one matrix entry per unit. Edge Workers in this repository have no package manager and no build step, so the job only needs an official Node distribution and the built-in test runner.
- `.github/workflows/_native.yml` — Reusable native platform workflow. Two kinds of non-Bazel build share it:
  - Build once, deploy the same bytes: on the deploy-home path, promote the artifact the merged PR already built and attested under `tree-<hash>` to the unit's release tags. A missing or unverified tree tag fails this job, which also stops every build job below;
  - Restore-only: the host-Rust family archive covers the common crates and toolchains this cross-build also resolves. Nothing on develop saves a native-specific repository cache, so a miss just downloads the zig/ Windows archives (and a PR never writes them …
  - Restore-only: the host-Rust family archive covers the common crates and toolchains this cross-build also resolves. Nothing on develop saves a native-specific repository cache, so a miss just downloads the zig/ Windows archives (and a PR never writes them …
  - Action-output disk cache (.bazelrc only); keyed like the pulsar-bazel unit so both restore the same Rust/C++ objects. Restore-only: see the logon job above for why PRs never write these entries.
  - Restore-only: the host-Rust family archive covers the common crates and toolchains this cross-build also resolves. Nothing on develop saves a native-specific repository cache, so a miss just downloads the zig/ Windows archives (and a PR never writes them …
  - Publish the artifact of the same-repo PR build: `ghcr.io/proflyder/<repository>:tree-<git tree hash>` (build once). deploy-home only promotes that digest; it never builds, so this job does not run on the deploy path.
- `.github/workflows/_pin.yml` — Promote the PR-built digest of one deployable unit and commit its pins.
  - Every deployable unit is Flux-owned: Flux applies the workload from `deploy/home-apps`, so CI only has to publish the verified digest there.
  - One caller job per unit. The caller runs them with `max-parallel: 1` (a job-level `concurrency` group must not be used: GitHub keeps only one pending job per group and cancels the rest of a matrix).
  - The correctness gate: a digest whose `tree-<hash>` tag or provenance does not verify is never written to the branch.
- `.github/workflows/_python.yml` — One job for every affected Python unit. A CI-global PR used to fan out one `python / <unit>` job per unit (~22 jobs, most finishing in 12-20 s of pure runner/setup overhead). The units share the same checkout, Python, pip and (when needed) Bazel/mise setup, so this job runs their `mise run test <id>` commands sequentially, one `::group::` per unit, and fails at the end with the list of failing …
  - The test commands come from the unit map via `mise run test <unit>`; tools come from mise-action's install_args, never from `mise run` itself.
- `.github/workflows/_quality.yml` — Reusable repository gate: audit, static gates, unit-map invariants, workflow linting and secret scanning. Cheap enough to run on every change, including documentation-only ones, which is why it is never routed away.
  - Every gate runs through the same mise task a workstation uses; tools come from mise-action's install_args, never from `mise run` itself.
- `.github/workflows/_station-deploy.yml` — Install one published Pulsar release on the station through Home Assistant.
  - The release job. `_deploy-home.yml` calls it after a merge promoted the release (push mode), after an explicit `units=pulsar` re-deploy of the currently published `latest` (manual mode), or `deploy-services.yml` after an honest rebuild.
- `.github/workflows/_unit.yml` — One Linux unit job: checks, container images and their provenance.
  - Restore-only in pull requests: a PR-scoped cache is invisible to every other PR, so writing it only spends the repository's 10 GB budget and the minutes of upload. Only a develop push (`warm-rust`) writes;
  - Per-index digests for the static attestation steps below. The image order is the unit map's `deploy.images`; `home-deploy` is appended by build-images.sh only when the unit does not own it, so it never shifts the leading indices.
- `.github/workflows/android-release.yml` — Manual Android release, run by the owner from `develop`.
- `.github/workflows/ci-manual.yml` — Manual twin of `ci.yml`: the same reusable pipeline (`_ci.yml`), run on demand over every unit instead of the diff of a pull request.
- `.github/workflows/ci.yml` — Pull-request entry point of the CI pipeline. One trigger only: the whole pipeline lives in `_ci.yml` (workflow_call), and `ci-manual.yml` is the `workflow_dispatch` twin that calls the same reusable. Never add a second event here — see AGENTS.md ("CI: один триггер на воркфлоу").
- `.github/workflows/cleanup-manual.yml` — Manual twin of the daily `cleanup.yml`, and a dry run by default: both jobs report what they would delete and delete nothing. Same reusable pipeline (`_cleanup.yml`), one trigger only.
- `.github/workflows/cleanup.yml` — Daily housekeeping entry point. One trigger only: the jobs live in `_cleanup.yml` (workflow_call), and `cleanup-manual.yml` is the `workflow_dispatch` twin. A scheduled run is always real.
- `.github/workflows/deploy-home-manual.yml` — Manual twin of `deploy-home.yml`: re-pin the current `develop` and, if asked, re-deploy the station. Same reusable pipeline (`_deploy-home.yml`, `mode: manual`), one trigger only.
- `.github/workflows/deploy-home.yml` — Post-merge release of the home units. This file is the `push` entry point only: the pipeline lives in `_deploy-home.yml` (workflow_call), and `deploy-home-manual.yml` is the `workflow_dispatch` twin that calls it with `mode: manual`.
  - The same group as the manual wrapper: a release and a manual run never overlap.
- `.github/workflows/deploy-services.yml` — Manual rebuild-and-deploy of the home units from the current `develop`.
  - One rebuild at a time: two runs would race on the same tree tags and pins.
- `Cargo.toml` — One Cargo workspace for every first-party Rust crate in the monorepo.
  - SHA-1 of canonical idempotency requests (DECISIONS 41): the same digest the Legacy Go store persisted, so a restarted or migrated scheduler replays keys. It is already in the lock as `uuid`'s own v5 digest;
  - home.v1 over the official Connect runtime: `connectrpc` transports and serves the protocol, `buffa` owns protobuf messages and protojson (DECISIONS 41, spike #134). Only the client and JSON codec are needed by the home services' executor/queue clients.
- `MODULE.bazel`
  - rules_rust omits the *-windows-gnu triples because Bazel cannot distinguish them from *-windows-msvc through @platforms constraints.
  - Static musl service images (wakegrant and the other home services). rules_rust gives x86_64-unknown-linux-musl the same constraints as the glibc host triple (os:linux + cpu:x86_64), so on a Linux exec host the glibc toolchain also matched …
  - Third-party C/C++ of the station's Sendspin player, pinned to the revisions in satellite/third_party/fetch.sh (sendspin-cpp's own host build). The archives come from GitHub (tags resolve to the same commit as before);
- `fnox.toml` — Secrets for local scripts — references to OpenBao only, never values.
- `mise.toml` — Workstation and CI toolchain — the single source of tool versions.
