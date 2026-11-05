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
| `ricardo-telemetry` | bootstrap `tracing` + `tracing-subscriber`: `LevelFilter` (`RUST_LOG` как уровень), stderr или append-файл, локальное время до старта рантайма, дополнительные `Layer`, отключение встроенного `fmt`-слоя (`without_default_layer`) для собственного формата. |
| `ricardo-runtime` | `shutdown_signal()` (SIGTERM/SIGINT) и `Supervisor` над `CancellationToken` + `TaskTracker`: `spawn(name, fut)` для `Send`-задач, `spawn_local(name, fut)` для `!Send` внутри `LocalSet`, именованный реестр и `shutdown(timeout) -> Vec<&'static str>` с именами не завершившихся. |
| `ricardo-actor` | `Handle<C>` (`Clone`, `send(&self, C)`, `is_closed`) и `channel()` — канал команд актора; актор завершается с закрытием канала. Спавн задачи — через `ricardo-runtime::Supervisor`, примитив не привязан к `!Send`. |
| `ricardo-linux` | Unix-хелперы: `errno_text`/`io_error_text` (C `strerror` без `(os error N)`) и sysfs `write_file`/`write_int_fd`/`trim` поверх `rustix`; `unsafe` только внутри крейта. |
| `ricardo-secret` | `Secret`/`Token`: `Zeroize`-буферы, `Debug` без значения, сравнение через `subtle::ConstantTimeEq`; файловые `load_or_create` (`0600`/`0700`) и генераторы `numeric(n)`/`hex(n)` — unix-only, чтобы крейт собирался под `x86_64-pc-windows-gnu`. |
| `ricardo-http` | axum bearer-авторизация на `ricardo-secret::Token` (`with_bearer`) и `serve(listener, router, CancellationToken)` с graceful shutdown. |

Крейты добавляются только под реального потребителя: BLE gateway — первый
потребитель всего слоя, logon provider использует переносимые `Secret`/
`Token` из `ricardo-secret`.
