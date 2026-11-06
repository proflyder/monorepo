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
| `home-telemetry` | bootstrap `tracing` + `tracing-subscriber`: `LevelFilter` (`RUST_LOG` как уровень), stderr или append-файл, локальное время до старта рантайма, дополнительные `Layer`, отключение встроенного `fmt`-слоя (`without_default_layer`) для собственного формата; `LogFormat::{Text,Json}` и JSON-события в духе Go `slog`. |
| `home-runtime` | `shutdown_signal()` (SIGTERM/SIGINT) и `Supervisor` над `CancellationToken` + `TaskTracker`: `spawn(name, fut)` для `Send`-задач, `spawn_local(name, fut)` для `!Send` внутри `LocalSet`, именованный реестр и `shutdown(timeout) -> Vec<&'static str>` с именами не завершившихся. |
| `home-actor` | `Handle<C>` (`Clone`, `send(&self, C)`, `is_closed`) и `channel()` — канал команд актора; актор завершается с закрытием канала. Спавн задачи — через `home-runtime::Supervisor`, примитив не привязан к `!Send`. |
| `home-linux` | Unix-хелперы: `errno_text`/`io_error_text` (C `strerror` без `(os error N)`) и sysfs `write_file`/`write_int_fd`/`trim` поверх `rustix`; `unsafe` только внутри крейта. |
| `home-secret` | `Secret`/`Token`: `Zeroize`-буферы, `Debug` без значения, сравнение через `subtle::ConstantTimeEq`; файловые `load_or_create` (`0600`/`0700`) и генераторы `numeric(n)`/`hex(n)` — unix-only, чтобы крейт собирался под `x86_64-pc-windows-gnu`. |
| `home-http` | axum bearer-авторизация на `home-secret::Token` (`with_bearer`, JSON-ошибка 401), `healthz`, лимит тела и request timeout поверх `tower-http`; `serve`/`serve_with_connect_info` с graceful shutdown. |
| `home-config` | `clap` derive с `env`: секрет из `NAME`/`NAME_FILE` (без значения в ошибке), listen-адрес, длительности и целые с границами, `LOG_LEVEL`. |
| `home-sqlite` | `rusqlite` (bundled) за single-writer потоком: `Db::call`/`call_blocking`, прагмы WAL/busy_timeout/foreign_keys и упорядоченные миграции через `schema_meta` (совместимо с Go v2). |
| `home-contracttest` | Rust-раннер нормативного формата `packages/go/contracttest`: `axum::Router` через `tower::ServiceExt::oneshot`, capture/`{{name}}`, `<uuid>`/`<base64-32>`/`<any>`, структурное и побайтное сравнение. |
| `wakegrant-protocol` | Переносимый wire-слой wakegrant (v1 и `logon`/`unlock`): HMAC-SHA256 сообщения, purpose, base64, serde-запросы/ответы и проверка MAC; без I/O и tokio, собирается под Windows. |
| `voice-contract` | Serde-типы голосового API mcp-gateway `/v1/voice/{utterance,result,updates,state}`: точное JSON-соответствие Go-серверу и клиенту, `Route::{Hermes,Bb,Local,Unknown}`; без I/O. |
| `home-ha` | Типизированный клиент Home Assistant REST (`state`, `call_service`, `call_script`, `render_template`, `ping`) на `reqwest`; bearer из `home-secret`, ошибка `Transient`/`Permanent` как в `packages/go/httpapi`. |
| `home-clients` | Read-only клиенты наблюдаемости на `reqwest`: Prometheus instant query, Alertmanager v2 active alerts и Loki `query_range` backward; те же `Transient`/`Permanent`, wire-структуры и лимиты ответа, что у Go-реализации mcp-gateway. |
| `home-telegram` | Telegram Bot API `sendMessage`: HTML-экранирование, разбиение по 4096 рун на границе строки/символа, `Transient`/`Permanent`; получатель и redaction — у вызывающего. |
| `home-proto` | Закоммиченный Rust-код `home.v1` (`connectrpc` + `buffa`, JSON поверх HTTP/1.1) и тонкий клиент scheduler: `enqueue_task`, `list_due_events`, `ack_event`, `get_status`; перегенерация — `generate.sh`. |
| `home-worker` | Общий цикл lease → handle → ack: трейты `Queue`/`Handler`/`Gate`, `Outcome::{Delivered,Skipped,Retry}`, экспоненциальный backoff и отмена через `CancellationToken`; `SchedulerQueue` поверх `home-proto`. |

Крейты добавляются только под реального потребителя: BLE gateway — первый
потребитель runtime-слоя, logon provider использует `home-secret` и
`wakegrant-protocol`, а `solutions/wakegrant` — первый потребитель
`home-config`, `home-sqlite` и `home-contracttest`. `voice-contract`,
`home-ha`, `home-proto` и `home-worker` — общий серверный слой для
voice-proxy и следующих сервисов решения 41.
