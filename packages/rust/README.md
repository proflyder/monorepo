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
| `home-telemetry` | bootstrap `tracing` + `tracing-subscriber`: `LevelFilter` (`RUST_LOG` как уровень), stderr или append-файл, локальное время до старта рантайма, дополнительные `Layer`, отключение встроенного `fmt`-слоя (`without_default_layer`) для собственного формата; `LogFormat::{Text,Json}` и JSON-события с уровнями и структурными полями. |
| `home-runtime` | `shutdown_signal()` (SIGTERM/SIGINT) и `Supervisor` над `CancellationToken` + `TaskTracker`: `spawn(name, fut)` для `Send`-задач, `spawn_local(name, fut)` для `!Send` внутри `LocalSet`, именованный реестр и `shutdown(timeout) -> Vec<&'static str>` с именами не завершившихся. |
| `home-actor` | `Handle<C>` (`Clone`, `send(&self, C)`, `is_closed`) и `channel()` — канал команд актора; актор завершается с закрытием канала. Спавн задачи — через `home-runtime::Supervisor`, примитив не привязан к `!Send`. |
| `home-linux` | Unix-хелперы: `errno_text`/`io_error_text` (C `strerror` без `(os error N)`) и sysfs `write_file`/`write_int_fd`/`trim` поверх `rustix`; `unsafe` только внутри крейта. |
| `home-secret` | `Secret`/`Token`: `Zeroize`-буферы, `Debug` без значения, сравнение через `subtle::ConstantTimeEq`; файловые `load_or_create` (`0600`/`0700`) и генераторы `numeric(n)`/`hex(n)` — unix-only, чтобы крейт собирался под `x86_64-pc-windows-gnu`. |
| `home-http` | axum bearer-авторизация на `home-secret::Token` (`with_bearer`, JSON-ошибка 401), `healthz`, лимит тела и request timeout поверх `tower-http`; `serve`/`serve_with_connect_info` с graceful shutdown. |
| `home-config` | `clap` derive с `env`: секрет из `NAME`/`NAME_FILE` (без значения в ошибке), listen-адрес, длительности и целые с границами, `LOG_LEVEL`. |
| `home-sqlite` | `rusqlite` (bundled) за single-writer потоком: `Db::call`/`call_blocking`, прагмы WAL/busy_timeout/foreign_keys и упорядоченные миграции через `schema_meta` (схема v2 сохранена). |
| `home-contracttest` | Единственный раннер нормативного формата golden-фикстур (`packages/rust/home-contracttest/README.md`): `axum::Router` через `tower::ServiceExt::oneshot`, capture/`{{name}}`, `<uuid>`/`<base64-32>`/`<any>`, структурное и побайтное сравнение. |
| `wakegrant-protocol` | Переносимый wire-слой wakegrant (v1 и `logon`/`unlock`): HMAC-SHA256 сообщения, purpose, base64, serde-запросы/ответы и проверка MAC; без I/O и tokio, собирается под Windows. |
| `voice-contract` | Serde-типы голосового API mcp-gateway `/v1/voice/{utterance,result,updates,state}`: точное JSON-соответствие серверу и клиенту, `Route::{Hermes,Bb,Local,Unknown}`; без I/O. |
| `home-ha` | Типизированный клиент Home Assistant REST (`state`, `call_service`, `call_script`, `render_template`, `ping`) на `reqwest`; bearer из `home-secret`, ошибка `Transient`/`Permanent`. |
| `home-clients` | Read-only клиенты наблюдаемости на `reqwest`: Prometheus instant query, Alertmanager v2 active alerts и Loki `query_range` backward; те же `Transient`/`Permanent`, wire-структуры и лимиты ответа. |
| `home-tls` | Публичные корни для общего workspace `reqwest`: читает PEM-бандл из `SSL_CERT_FILE` или `/etc/ssl/certs/ca-certificates.crt` (fallback macOS `/etc/ssl/cert.pem`), добавляет `reqwest::Certificate` в builder, ставит ring provider; отсутствующий/пустой бандл — ошибка на старте. Нужен потому, что `reqwest` собирается с `rustls-no-provider` и доверяет только явно заданным корням (`tls_certs_only`), а включать встроенные `webpki-roots` нельзя: фичи унифицируются с прошивкой станции. |
| `home-telegram` | Telegram Bot API `sendMessage`: HTML-экранирование, разбиение по 4096 рун на границе строки/символа, `Transient`/`Permanent`; получатель и redaction — у вызывающего. |
| `home-proto` | Закоммиченный Rust-код `home.v1` (`connectrpc` + `buffa`, JSON поверх HTTP/1.1) и тонкий клиент scheduler: `enqueue_task`, `list_due_events`, `ack_event`, `get_status`; перегенерация — `generate.sh`. |
| `home-worker` | Общий цикл lease → handle → ack: трейты `Queue`/`Handler`/`Gate`, `Outcome::{Delivered,Skipped,Retry}`, экспоненциальный backoff и отмена через `CancellationToken`; `SchedulerQueue` поверх `home-proto`. |

Крейты добавляются только под реального потребителя: BLE gateway — первый
потребитель runtime-слоя, logon provider использует `home-secret` и
`wakegrant-protocol`, а `solutions/wakegrant` — первый потребитель
`home-config`, `home-sqlite` и `home-contracttest`. `voice-contract`,
`home-ha`, `home-proto` и `home-worker` — общий серверный слой для
voice-proxy и следующих сервисов решения 41.

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `forced-ssh/src/lib.rs` — One-exec SSH client for forced commands with pinned `known_hosts`.
  - One call failure. It never contains key material or a payload.
  - The diagnostic detail. It never contains key material or a payload.
  - stderr is diagnostics only; it is dropped so a warning can never be mistaken for the answer. The command name and payload are never included in an error.
  - Certificates need an authority we deliberately do not trust.
- `home-actor/src/lib.rs` — Minimal actor primitive shared by first-party daemons.
- `home-clients/src/alertmanager.rs` — Minimal read-only Alertmanager v2 API client.
- `home-clients/src/lib.rs` — Read-only HTTP clients for the home observability stack.
  - One read-only backend call failure. It carries the operation and a safe status/detail, never request headers or response bodies.
  - Build the shared `reqwest` client. The workspace builds `reqwest` with `rustls-no-provider`; `home_tls` installs the ring provider and pins the operating system's root bundle with `tls_certs_only`, so an `https://` backend (Prometheus behind a private TLS …
- `home-clients/src/loki.rs` — Minimal read-only Loki query client.
  - Build a client. `base_url` is the Loki root, for example `http://loki.example:3100`.
- `home-clients/src/prometheus.rs` — Minimal read-only Prometheus HTTP API client: one instant query.
  - Series labels (`metric` in the Prometheus wire format).
  - Build a client. `base_url` is the API root, for example `http://prometheus.example:9090`.
- `home-config/src/lib.rs` — Typed service configuration built on `clap` derive with the `env` feature.
  - Why a configuration value was rejected. Every variant names the variable (or file) and never echoes a secret.
  - Reads a secret with the `NAME_FILE`-first priority of ha-bridge and ha-control. If `NAME_FILE` is set (and not blank), its contents are authoritative: a read or empty-file error is returned instead of falling back to `NAME`, so a broken mounted Secret never …
  - A broken file must not fall back to a stale env value.
- `home-ha/src/lib.rs` — Narrow Home Assistant client shared by the home services.
  - PEM bundle used for the private-CA WebSocket connector; never logged.
  - Build a client. `base_url` must not contain credentials or a path. The workspace builds `reqwest` with rustls and without a default crypto provider; building a client panics ("No provider set") unless one is installed.
  - Like [`Client::new_with_ca_pem`], but reads the PEM bundle from `path`. The value is never logged.
  - An authenticated client must not forward its bearer to a redirect target; a 3xx is surfaced as a status error instead.
  - Read every entity with its state and attributes. The response is the Home Assistant `/api/states` array. Known attributes are typed in [`EntityAttributes`];
  - Render a Home Assistant template and return the text. The template is always built by the caller, never taken from an untrusted client.
  - The body of a failed response is never read or logged: it may carry entity attributes or other deployment detail.
- `home-http/src/lib.rs` — Bearer authentication and graceful serving for axum control APIs.
- `home-linux/BUILD.bazel`
  - The same sources with the test-only helpers (a pty pair). Only test targets depend on it, so a production build never compiles them.
- `home-linux/Cargo.toml`
  - Test-only helpers (a pseudo-terminal pair); never enabled by a production build.
- `home-linux/src/lib.rs` — Small Unix helpers shared by the first-party daemons.
  - C `strerror` text of an I/O error, without Rust's " (os error N)" suffix. Errors that do not wrap an OS errno keep their own message.
- `home-linux/src/linux.rs` — Linux kernel ABIs shared by the station daemons: framebuffer, i2c-dev, evdev and the local wall clock. The ioctl numbers and struct layouts are kernel ABI, not library constants, so they are reproduced here byte for byte.
  - `struct fb_var_screeninfo` (linux/fb.h): 40 32-bit fields. Only handed back to the driver (FBIOPAN_DISPLAY takes the offsets read by FBIOGET_VSCREENINFO), never interpreted.
  - `struct i2c_msg` of linux/i2c-dev.h. `I2C_RDWR` is the same number on every Linux ABI.
- `home-linux/src/tty.rs` — A serial port (tty) of the station: opened read-write, never as the controlling terminal, configured raw 8N1 at 115200 without flow control, and handed to the runtime for reads.
  - Opens `path` read-write, never as the controlling terminal (std adds `O_CLOEXEC`): non-blocking for a link that reads through the runtime, blocking for a flasher that polls before every read.
  - `ENOTTY` unless this is a terminal: never write menu keys or an image into a regular file.
  - Hands the port to the reactor for read readiness (writes never wait for it).
- `home-proto/generate.sh` — Regenerate the checked-in home.v1 Rust stubs.
- `home-proto/src/lib.rs` — Rust bindings for the canonical `home.v1` Connect contract and a thin scheduler client.
- `home-proto/src/scenario.rs` — Minimal Connect-JSON client for `home.v1.ScenarioService`.
  - A scenario call failure. `Debug` and `Display` never print the bearer: the Connect message is redacted against the configured token and bounded.
  - Build a client. `base_url` is the scheduler base (`http://127.0.0.1:8098`); a trailing service suffix is tolerated.
  - Like [`ScenarioClient::new_with_ca_pem`], but reads the bundle from `path`. The file content is never logged.
- `home-proto/src/scheduler.rs` — Minimal Connect-JSON client for `home.v1.SchedulerService`.
  - A scheduler call failure. `Debug` and `Display` never print the bearer: the Connect message is redacted against the configured token and bounded.
  - Build a client. `base_url` is the scheduler base (`http://127.0.0.1:8098`); a trailing `/home.v1.SchedulerService` is tolerated, like the Go client.
  - Like [`SchedulerClient::new_with_ca_pem`], but reads the bundle from `path`. The file content is never logged.
  - Report a terminal outcome. `detail` is truncated to the proto limit. A replay of a terminal ack is idempotent on the server: it returns the already-known event instead of failing.
- `home-proto/src/text.rs` — Builders for `home.v1.LocalizedText`.
- `home-proto/src/transport.rs` — HTTP/1.1 transport for the connectrpc client over the workspace `reqwest`.
- `home-runtime/src/lib.rs` — Runtime plumbing shared by first-party daemons: a thin [`Supervisor`] over [`CancellationToken`] and [`TaskTracker`], plus a Unix shutdown signal.
  - Servers spawn one task per connection: finished handles must not accumulate.
- `home-secret/src/lib.rs` — Redacted secrets and token files for first-party home services.
  - A secret string that never leaks through `Debug`, `Display` or `Clone` (cloning copies the value into another zeroizing buffer). Call [`Secret::expose`] at the exact place a protocol needs the plaintext.
- `home-sqlite/src/lib.rs` — One SQLite connection owned by one thread.
- `home-telegram/src/lib.rs` — Telegram Bot API `sendMessage` delivery for the home services.
  - One Bot API call failure. It never contains the bot token or a message body.
  - Build a sender. `api_url` is the Bot API root (`https://api.telegram.org` or a local fake); `token` and `chat_id` are trimmed of whitespace.
- `home-telemetry/src/lib.rs` — `tracing` bootstrap shared by the daemons in this repository.
- `home-tls/src/lib.rs` — Public HTTPS roots for the shared workspace `reqwest` client.
  - Resolve the bundle path: `SSL_CERT_FILE`, else the first default that exists. An explicit `SSL_CERT_FILE` is never silently replaced by a default: a typo in the deployment must surface as an error.
  - Add every system root to `builder`. Extra roots never narrow the client: they only extend the trust anchors, so internal HTTP/HTTPS clients can use this unconditionally.
  - The workspace default: no roots at all. This is the incident's failure mode, kept as a regression check for the test CA.
- `home-worker/src/lib.rs` — The shared executor loop: lease → handle → ack.
  - Opaque `payload_json`. Treat it as data only, never as a service call or entity identifier.
  - Report a terminal outcome. `Outcome::Retry` never reaches this method.
  - Run until `cancel` is cancelled. It never returns an error: a queue failure is retried with backoff, matching the Go executors.
  - The event may still be leased; do not spin on the rest of this batch.
- `home-worker/src/scheduler_queue.rs`
  - An unknown wire value must not be guessed as FIRE: the handler decides whether it can act on it.
- `voice-contract/src/lib.rs` — Typed wire contract of the mcp-gateway voice API.
  - Where the gateway routed one phrase. Only [`Route::Hermes`], [`Route::Bb`] and [`Route::Local`] are known to the proxy. Any other value is preserved as [`Route::Unknown`]; the proxy must treat it as an unavailable agent server and must not forward it.
  - The turn outcome. `status` is never omitted.
  - Sequence of the completed turn, used as the queue idempotency key.
- `wakegrant-protocol/src/base64.rs` — Standard base64 with padding.
- `wakegrant-protocol/src/error.rs` — Log-safe protocol error: a short static phrase, never a secret, key, password or MAC, so it can be written to the logon log as is.
- `wakegrant-protocol/src/lib.rs` — wakegrant wire protocol shared by the service and the Windows provider.
- `wakegrant-protocol/src/purpose.rs` — The two wakegrant purposes: `logon` after a cold boot and `unlock` for a running but locked workstation.
- `wakegrant-protocol/src/response.rs`
  - Tolerant reader: unknown fields are ignored. Authenticity comes from the response MAC, so a new server field must not lock the machine out.
  - Short, log-safe reason; never contains secrets.
  - Verifies the v1 server response for `nonce` with `key`. Only [`ClaimOutcome::Granted`] may arm the provider; every other outcome is fail-closed.
- `wakegrant-protocol/src/secret.rs` — Plaintext payload of `secret.bin` after DPAPI machine-scope decryption: `{"version":1,"password":"<utf-8>","key":"<base64 of 32 bytes>"}`. Password and key are wiped on drop and redacted in `Debug`.
- `wakegrant-protocol/src/wire.rs` — Size-limited typed JSON decoding shared by every wire message.
- `wakegrant-protocol/tests/protocol.rs`
  - A response MAC of the other purpose is rejected: a logon grant can never unlock and an unlock grant can never log on.
  - Server responses are tolerant readers: a new field in wakegrant must not lock the machine out, and the MAC still decides.
