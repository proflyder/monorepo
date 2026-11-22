# Монорепозиторий и Bazel

Текущий runtime — `STATE.md`; причины решений — `DECISIONS.md`.

## Горизонтальная структура

Главный принцип: **solutions переиспользуются, projects их композируют**.

```text
solutions/               переиспользуемые решения
  scheduler/             durable scheduler, SQLite queues, Connect/gRPC-Web
  ha-bridge/             executor-адаптер HA ↔ scheduler (бинари ha-bridge, ha-control, ha-scenario)
  ha-config-sync/        pull-синхронизация конфигурации HA
  mcp-gateway/           MCP Streamable HTTP: infra.read / infra.fix / infra.report, чат приложения
  currency-bot/          курсы, алерты, Telegram webhook
  voice-proxy/           голосовой прокси для Hermes/Piper TTS
  intercom/              сигналинг WebRTC-звонка телефон ↔ колонка
  phone-push/            доставка на телефон через FCM
  wakegrant/             одноразовые гранты входа для WoL-машины
  windows-logon-provider/ Windows Credential Provider (Rust, Bazel)
  heartbeat/             внешний dead-man switch (Cloudflare Worker)
  bb-home-dispatcher/    плагин bb: исполнитель голосовых команд
  bb-profiles/           плагин bb: изолированные профили

packages/                общий код и контракты
  proto/                 protobuf-контракты home.v1
  contracts/             межъязыковые протоколы (wakegrant)
  rust/                  общие Rust-крейты
  kotlin/                общий Kotlin-код (Android)

projects/                композиции и deployment
  station/               Pulsar/ADB/BLE/provisioning
  home-assistant/        HA/MA/STT/TTS/Hermes
  home/                  Proxmox, K3s, Flux, бэкапы, композиция сервисов
  mobile/android/        Kotlin/Views клиент (dev.proflyder.home)

platform/                Bazel/toolchains, CI, deploy-инструменты, quality
```

- Solution не импортирует project и не содержит project-specific endpoint,
  путь БД или секрет; один solution запускается в разных projects.
- Будильник, напоминание и таймер — domain conveniences поверх durable
  scheduling и очередей, а не отдельные планировщики.
- `projects/station` и `projects/home-assistant` независимы; `projects/home`
  их композирует.
- Код с двумя и более потребителями — в `packages/`, не копируется.

## Языки

Правило — решение 35, сервисы — решение 41.

| Слой | Язык |
|---|---|
| сервисы платформы, `home-deploy`, health exporter | Rust на `packages/rust` |
| Pulsar, BLE gateway, станционные helper'ы | Rust (`rules_rust`, zig cross) |
| Windows logon provider | Rust (`windows-bindgen`, zig `x86_64-pc-windows-gnu`, решение 39) |
| `projects/mobile/android`, `platform/android/tools` | Kotlin (Views, Metro DI, single Activity) |
| `solutions/heartbeat` | JavaScript ES modules (Cloudflare Workers, без npm) |
| плагины bb | JavaScript/TypeScript |
| CI-обвязка (`platform/ci`, `platform/quality`) | JavaScript (`node --test`, без npm) |
| HA-конфиг | YAML |
| хостовая обвязка | shell / Python stdlib |
| ML wake-word, dev-утилиты станции, проверки YAML, live-e2e | Python |

Go в репозитории нет.

## Bazel

```text
MODULE.bazel         Bzlmod: rules_rust, rules_proto, rules_oci, rules_kotlin, Android
MODULE.bazel.lock    коммитится
.bazelversion        Bazel 9.3.2
.bazelrc             общие флаги; Android — platform/android/android.bazelrc (--config=android)
Cargo.toml           единый Cargo workspace всего first-party Rust
Cargo.lock           единственный lockfile
crate-universe.lock  сгенерированный lock для @crates
rust-toolchain.toml  канал 1.99.1 и targets всех Rust-целей
```

`WORKSPACE` не добавлять, toolchain-настройки по приложениям не дублировать.
Маршрутизация CI по правкам `MODULE.bazel`/`.bazelrc` — `TESTING.md`.

### Rust: единый workspace

- Один `/Cargo.toml` (`[workspace.dependencies]` — точные пины,
  `[workspace.lints]` — единые линты), одно расширение
  `crate.from_cargo(name = "crates", ...)` → `@crates//` для всех целей.
- Каждый crate-пакет передаёт `extract_cargo_lints(... workspace =
  "//:Cargo.toml")` в `lint_config`, поэтому `cargo clippy` и
  `rust_clippy_test` линтуют одно и то же.
- После добавления зависимости: `Cargo.toml`/`Cargo.lock`, затем
  `CARGO_BAZEL_REPIN=1 bazel build ...`, затем `bazel build` и `bazel test`.
- Pulsar и BLE gateway собираются для станции герметичным zig cc-toolchain
  (`platform/bazel/zig`, `aarch64_linux`/`armhf_linux`); цели Linux-only, на
  macOS пропускаются.

## Контракты RPC

Канонические схемы — `packages/proto/home/v1/*.proto`; один listener отдаёт
Connect JSON/binary и gRPC-Web, отдельного REST API нет. Подробно — `GRPC.md`.

## Границы

- HA/Pulsar: звонок, рингтон, TTS, громкость, restore source.
- `solutions/scheduler`: durable schedules, timers, outbox, generic queue,
  leases/ack, server clock, определения сценариев.
- `solutions/ha-bridge`: executor adapter; `ha-bridge` арендует `queue=home`,
  `ha-control` — `queue=control` и каталог Home, `ha-scenario` — `scenario`.
  Не владеет расписанием.
- Плагины bb — не сервисы: профиль и allowlist живут только в `bb-profiles`,
  HTTP-маршрут диспетчера — loopback, наружу его публикует только SSH forced
  command (решение 28).
- `projects/mobile/android`: управляет server state, не владеет расписанием.
- `projects/*`: композиция, env, манифесты K3s, systemd.

## Заметки к коду

- `.bazelrc`: `clippy_flag=-Dwarnings` обязателен — уровни из
  `[workspace.lints]` равны `warn`, и без флага новое предупреждение clippy
  проходит и локально, и в PR CI.
- `MODULE.bazel`: rules_rust не знает `*-windows-gnu` (Bazel не отличает их
  от msvc по `@platforms`), отсюда патчи; у `x86_64-unknown-linux-musl` те же
  constraints, что у glibc-хоста, поэтому musl-toolchain для сервисных образов
  выбирается явно.
- `.github/actions/nexus-cache`: проверка здоровья идёт по пути, который
  отдаёт публичный ingress (`/repository/npmjs/-/ping`), а не по
  `/service/rest`; при недоступности rewrite не включается.
- `_pin.yml` вызывается с `max-parallel: 1`, а не job-level `concurrency`:
  GitHub держит один pending job на группу и отменяет остальную матрицу.
- `_android-release.yml`: release notes передаются через env, а не
  интерполяцией в скрипт, — заголовок PR не может внедрить shell.
