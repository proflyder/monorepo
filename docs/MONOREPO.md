# Монорепозиторий и Bazel

Статус: **актуальная структура**. HA/voice/Hermes/TTS и station/Pulsar/BLE
разложены по `projects/`; текущий runtime — `STATE.md`.

## Горизонтальная структура

Главный принцип: **solutions переиспользуются, projects их композируют**.

```text
solutions/               переиспользуемые решения
  scheduler/             durable scheduler, SQLite queues, Connect/gRPC
  alarm/                 тот же engine под именем процесса alarm
  ha-bridge/             executor-адаптер HA/Pulsar ↔ scheduler (+ cmd/ha-control)
  mcp-gateway/           MCP Streamable HTTP: infra.read / infra.fix / infra.report
  currency-bot/          курсы kurs.kz, алерты, Telegram webhook
  voice-proxy/           OpenAI-совместимый прокси для Piper TTS
  wakegrant/             one-time logon grants for a WoL machine (PROTOCOL.md)
  windows-logon-provider/ Windows Credential Provider (Rust, Bazel)
  heartbeat/             внешний dead-man switch (Cloudflare Worker)
  bb-home-dispatcher/    плагин bb: исполнитель голосовых команд
  bb-profiles/           плагин bb: изолированные профили

packages/                общий код и контракты
  proto/                 gRPC/protobuf (+ checked-in Go stubs)
  go/                    shared Go: httpapi, ha (HA REST), scheduler (Connect-JSON)

projects/                композиции и deployment
  station/               Pulsar/ADB/BLE/provisioning
  home-assistant/        HA/MA/STT/TTS/Hermes
  home/                  station + home-assistant + scheduler/bridge/control
  mobile/android/        Kotlin/Views клиент (dev.proflyder.home)

platform/                Bazel/toolchains/deploy-шаблоны
docs/                    документация
archive/                  исторические материалы
```

### Почему так

- `solutions/scheduler` не знает, где он запущен;
- будильник, напоминание и таймер — domain conveniences поверх durable
  scheduling и очередей, а не отдельные планировщики;
- `projects/station` и `projects/home-assistant` независимы;
- `projects/home` — их композиция плюс домашний scheduler и HA bridge;
- `solutions/ha-bridge` арендует только `queue=home`; `cmd/ha-control` — только
  `queue=control` и каталог Home;
- один и тот же код не копируется между проектами;
- project-specific секреты не попадают в solution.

Пример:

```text
projects/home:
  scheduler@home      sqlite=/data/home/scheduler.db  :8098
  alarm@home          тот же engine + legacy AlarmService facade
  ha-bridge@home      SCHEDULER_ENDPOINT=127.0.0.1:8098, queue=home
  ha-control@home     каталог Home, queue=control
  android             Connect JSON → LAN/VPN HTTPS endpoint
```

Одновременно `scheduler` и `alarm` на одной БД не запускаются: это один
engine, два entrypoint-а для совместимости.

## Решения по языкам

| Слой | Язык | Почему |
|---|---|---|
| сервисы платформы: `scheduler`, HA-исполнители, `voice-proxy`, `mcp-gateway`, `currency-bot` | Rust (цель, решение 41); `voice-proxy` уже Rust, остальные пока Go | общий core-слой `packages/rust` вместо копий платформенного кода, типизированные контракты, чистый домен; переход по одному сервису через golden-фикстуры `testdata/contract/` |
| `voice-proxy` | Rust (решение 41) | `domain`/`openai`/`relay`/`services` на общем `home-*` слое: `home-config`, `home-ha`, `home-proto`, `home-worker`, `voice-contract`, `home-contracttest`; golden-фикстуры и байты payload очереди перенесены без изменений, статический musl-образ |
| `wakegrant` | Rust (решение 41, пилот) | первый потребитель общего core-слоя: `wakegrant-protocol`, `ricardo-config`, `ricardo-sqlite`, `ricardo-contracttest`; статический musl-образ, Go-код удалён |
| аудио/DSP/инференс | Rust | предсказуемая производительность, нет GC, FFI |
| `projects/mobile/android` | Kotlin + Views | server-authoritative клиент; Metro DI, single Activity, typed Fragment router |
| `solutions/heartbeat` | JavaScript ES modules | только Cloudflare Workers edge runtime: без сборки и npm, `node --test`, минимальный внешний контур |
| `solutions/bb-home-dispatcher`, `solutions/bb-profiles` | JavaScript/TypeScript (плагины bb) | исполняются внутри bb как path-источник; `node --test` без внешних сервисов |
| Windows logon provider | Rust (`windows-bindgen`/`windows-core`) | in-proc COM DLL для LogonUI; `rules_rust` + герметичный zig (`x86_64-pc-windows-gnu`) через Bazel, без Visual Studio; переносимые host-тесты и clippy под Windows |
| HA-конфиг | YAML | существующий контур, не переписывать ради модности |
| Pulsar (станция) | Rust (`rules_rust`, Zig cross) | first-party форк upstream, сборка Bazel |
| `projects/station/ble-gateway` | Rust (`zbus`, `tokio`, `rules_rust`) | прямой D-Bus без libdbus; члены общего корневого Cargo workspace, host-тесты и armhf-кросс через Bazel |
| CI-обвязка (`platform/ci`, `platform/quality`) | JavaScript (`actions/github-script`, `node --test`) | Node есть в раннере, без setup-python/pip; локально — `node` |
| Bazel-инструменты Android (`platform/android/tools`) | Go (`rules_go`, exec-конфигурация) | герметичный статический бинарник в песочнице Bazel, без `python3` на PATH; общий `go.mod`, `go vet`/`go test` |
| `platform/deploy` (`home-deploy`) и runtime health exporter | Go | все инструменты деплоя и in-cluster Job/экспортер — один distroless-бинарник, без Python в кластере (решение 35) |
| задачи на хостах | shell / Python stdlib (короткая обвязка) или Go-бинарник (большая логика) | решение 35 |
| ML wake-word, dev-утилиты станции, проверки YAML, live-e2e | Python | экосистема ML; в проде не исполняется |

Rust — для долгоживущих сервисов (решение 41), станции и тяжёлых вычислений,
Go — для инструментов деплоя, CronJob/Job и Bazel-инструментов (решение 35),
JavaScript — только для edge Worker, которому нужен runtime Cloudflare, а не
общий backend-стек.
Mobile не владеет расписанием и не заменяет серверные будильники.

## Bazel

```text
MODULE.bazel       Bzlmod: rules_go, gazelle, rules_rust, rules_proto, Android
MODULE.bazel.lock  зафиксированные зависимости
.bazelversion      Bazel 9.3.1
.bazelrc           cache/вывод
.bazelignore       тяжёлые/чужие/legacy каталоги
BUILD.bazel        //:gazelle
Cargo.toml         единый Cargo workspace всего first-party Rust
Cargo.lock         единственный lockfile workspace (cargo)
crate-universe.lock  generated Bazel lock для @crates
rust-toolchain.toml  канал 1.99.1 и targets всех Rust-целей
go.mod             Go module для новых решений
go.work            workspace корня и отслеживаемого BLE-модуля
```

Инструменты Android-сборки в `platform/android/tools` — Go-программы
(`go_binary` в exec-конфигурации), а их тесты — `go_test`; genrule'ы
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
`bazel_dep`, `go_deps`, `oci`, python — остаются глобальными. `.bazelrc`
сужается до `.bazelrc#cache` (ни одной платформы), только если дифф меняет
исключительно cache/output-флаги; `platform/bazel/zig/**` и
`platform/bazel/windows/**` — только `native` (`platform/bazel#native`).
Подробности и белый список флагов — `docs/TESTING.md`.

Чистый checkout поддерживает `go test ./solutions/scheduler/... ./solutions/alarm/... ./packages/...`;
Pulsar installer — отдельный Go-модуль, проверяется с `GOWORK=off`.
`make test` пропускает (`SKIP`) тесты, которым нужны игнорируемые локальные
источники (см. `docs/TESTING.md`).

Команды:

```bash
bazel build //solutions/scheduler/... //solutions/alarm/... //packages/proto/...
bazel test  //solutions/scheduler/... //solutions/alarm/...

bazel test //packages/...
bazel run //:gazelle
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
(`_bazel.yml`). Станции собирает тот же Bazel: герметичный zig 0.14.1
cc-toolchain (`platform/bazel/zig/`) с платформами `aarch64_linux`/`armhf_linux`,
sendspin-cpp и C/C++-зависимости — `http_archive` + BUILD-overlay'и в
`satellite/third_party/bazel/`. Юнит `pulsar` (`_native.yml`) собирает обе
архитектуры и релиз Bazel-путём, installer/radio/ssh остаются своими
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
приходят из проекта при установке. На push в `develop` `deploy-home`
публикует файлы как публичный OCI-артефакт GHCR (ORAS, `VERSION`, digest и
provenance); домашний ПК только читает готовый digest.

## gRPC / Connect

Канонические контракты — `packages/proto/ricardo/v1/scheduler.proto`
(`SchedulerService`, основной) и `alarms.proto` (`AlarmService`, legacy
facade). Один listener отдаёт Connect JSON/binary, gRPC (HTTP/2, h2c) и
gRPC-Web; отдельного REST API нет. Go stubs лежат в репозитории и
перегенерируются pinned-скриптом
`solutions/scheduler/scripts/generate-proto.sh`; Bazel собирает их через
`go_library` и не дублирует кодогенерацию. Подробности — `GRPC.md`.

## Границы

- HA/Pulsar: звонок, рингтон, TTS, громкость, restore source.
- `solutions/scheduler`: durable schedules, timers, outbox, generic queue,
  leases/ack, server clock.
- `solutions/alarm`: legacy `AlarmService` view над тем же engine.
- `solutions/ha-bridge`: executor adapter; арендует `queue=home`, вызывает HA
  scripts, подтверждает события. Не владеет расписанием.
- `packages/go`: общие клиенты (`httpapi`, `ha`, `scheduler`) без проектной
  привязки; endpoints, очереди и entity id задаёт потребитель, а не пакет.
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
папка tests не используется; `make test` агрегирует локальные наборы.
