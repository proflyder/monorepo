# packages/rust

Общий core-слой first-party Rust: крейты с двумя и более потребителями
(решения 40, 41).

- Пакет не импортирует `projects/**` и не содержит проектных endpoint'ов,
  путей БД или deployment-специфики.
- Крейты — члены корневого Cargo workspace; зависимости — через
  `[workspace.dependencies]`, линты — `[lints] workspace = true`.
- Новый крейт — только под реального второго потребителя.

```bash
bazel test //packages/rust/...
cargo test -p <crate>
```

| Крейт | Назначение |
|---|---|
| `home-telemetry` | bootstrap `tracing`: уровень, stderr/файл, текст или JSON, свои `Layer` |
| `home-runtime` | `shutdown_signal()` и `Supervisor` (`spawn` для `Send`, `spawn_local` для `!Send`, `shutdown(timeout)` возвращает незавершившиеся) |
| `home-actor` | `Handle<C>` и `channel()` — канал команд актора |
| `home-linux` | errno-текст без `(os error N)`, sysfs, kernel ABI станции (fb, i2c, evdev, tty) поверх `rustix` |
| `home-secret` | `Secret`/`Token` (zeroize, без значения в `Debug`, constant-time сравнение), файловые токены — только unix |
| `home-http` | axum bearer-авторизация, `healthz`, лимиты, graceful `serve` |
| `home-config` | `clap` + env: секрет из `NAME_FILE`/`NAME`, границы значений |
| `home-sqlite` | `rusqlite` за single-writer потоком, WAL, миграции через `schema_meta` |
| `home-contracttest` | раннер golden-фикстур HTTP-контрактов ([README](home-contracttest/README.md)) |
| `home-proto` | закоммиченный Rust-код `home.v1` (`connectrpc` + `buffa`) и клиенты scheduler/scenario |
| `home-worker` | цикл lease → handle → ack с backoff поверх `home-proto` |
| `home-ha` | клиент Home Assistant REST, ошибка `Transient`/`Permanent` |
| `home-clients` | read-only Prometheus, Alertmanager, Loki |
| `home-telegram` | Telegram `sendMessage` с экранированием и разбиением по 4096 |
| `home-tls` | системный CA-бандл для `reqwest` (решение 45) |
| `forced-ssh` | one-exec SSH-клиент для forced command с pinned `known_hosts` |
| `voice-contract` | serde-типы голосового API mcp-gateway `/v1/voice/*` |
| `wakegrant-protocol` | переносимый wire-слой wakegrant, без I/O, собирается под Windows ([PROTOCOL](../contracts/wakegrant/PROTOCOL.md)) |

## Заметки к коду

- `home-config`: если `NAME_FILE` задан, его содержимое авторитетно — ошибка
  чтения или пустой файл не откатываются на `NAME`, чтобы сломанный Secret не
  подменялся устаревшим env.
- `home-ha`: аутентифицированный клиент не следует редиректам (bearer не
  уходит на чужой хост, 3xx — ошибка); тело неуспешного ответа не читается.
  Клиент паникует «No provider set», если не установлен crypto provider —
  использовать `home-tls`.
- `home-tls`: явно заданный `SSL_CERT_FILE` никогда не подменяется дефолтом —
  опечатка в деплое должна быть ошибкой.
- `home-worker`: неизвестное wire-значение action не считается FIRE —
  решает handler; цикл не возвращает ошибку, сбои очереди ретраятся с backoff.
- `home-runtime`: серверы спавнят задачу на соединение, поэтому завершённые
  handle не накапливаются.
- `forced-ssh`: stderr отбрасывается, ошибки не содержат ключей, имени команды
  и payload; SSH-сертификаты не принимаются.
- `voice-contract`: неизвестный `Route` сохраняется как `Route::Unknown`, и
  прокси не должен его пересылать.
- `wakegrant-protocol`: ответы сервера читаются толерантно (новое поле не
  должно запереть машину — решает MAC); MAC другого purpose отвергается.
