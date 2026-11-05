# packages/rust

Общий core-слой first-party Rust. Здесь живут крейты, у которых не меньше
двух потребителей (станция, BLE gateway, Windows logon provider, инструменты).

- Пакет не импортирует `projects/**` и не содержит проектных endpoint'ов, путей
  БД или deployment-специфики: один и тот же крейт подключается в разных
  проектах (см. `docs/MONOREPO.md`).
- Крейты — члены корневого Cargo workspace (`/Cargo.toml`), зависимости
  объявляются через `[workspace.dependencies]`, линты — через
  `[lints] workspace = true`.
- `bazel test //packages/rust/...` запускает unit-тесты, `clippy`
  (`-D warnings`) и `rustfmt`; те же цели перечислены в CI-юните
  `ble-gateway`.

| Крейт | Назначение |
|---|---|
| `ricardo-telemetry` | bootstrap `tracing` + `tracing-subscriber`: `LevelFilter` (`RUST_LOG` как уровень), stderr или append-файл, локальное время до старта рантайма, дополнительные `Layer` (Pulsar подключит ESPHome-sink). |
| `ricardo-runtime` | `shutdown_signal()` (SIGTERM/SIGINT) и `Supervisor` над `CancellationToken` + `TaskTracker`: единый способ запустить долгоживущие задачи и дождаться их на shutdown. |
| `ricardo-secret` | `Secret`/`Token`: `Zeroize`-буферы, `Debug` без значения, сравнение через `subtle::ConstantTimeEq`; файловые `load_or_create` (`0600`/`0700`) и генераторы `numeric(n)`/`hex(n)` — unix-only, чтобы крейт собирался под `x86_64-pc-windows-gnu`. |
| `ricardo-http` | axum bearer-авторизация на `ricardo-secret::Token` (`with_bearer`) и `serve(listener, router, CancellationToken)` с graceful shutdown. |

Крейты добавляются только под реального потребителя: BLE gateway — первый
потребитель всего слоя, logon provider использует переносимые `Secret`/
`Token` из `ricardo-secret`.
