# Монорепозиторий и Bazel

Статус: **актуальная структура**. HA/voice/Hermes/TTS и station/Pulsar/BLE
разложены по `projects/`; текущий runtime — `STATE.md`.

## Горизонтальная структура

Главный принцип: **solutions переиспользуются, projects их композируют**.

```text
solutions/               переиспользуемые решения
  scheduler/             durable scheduler, SQLite queues, Connect/gRPC
  alarm/                 legacy AlarmService facade над тем же engine
  ha-bridge/             executor-адаптер HA/Pulsar ↔ scheduler (+ cmd/ha-control)
  mcp-gateway/           MCP Streamable HTTP: infra.read / infra.fix / infra.report
  wakegrant/             one-time logon grants for a WoL machine (PROTOCOL.md)
  heartbeat/             внешний dead-man switch (Cloudflare Worker)

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
| `solutions/scheduler` | Go | always-on scheduler/queue, один бинарник, low memory, pure-Go SQLite |
| MCP/HA bridge | Go | сетевые адаптеры, простой деплой, общий код |
| `solutions/voice-proxy` | Go | OpenAI-совместимый streaming-прокси с маршрутизацией и durable-очередью; один distroless-бинарник рядом с Hermes |
| аудио/DSP/инференс | Rust | предсказуемая производительность, нет GC, FFI |
| `projects/mobile/android` | Kotlin + Views | server-authoritative клиент; Metro DI, single Activity, typed Fragment router |
| `solutions/heartbeat` | JavaScript ES modules | только Cloudflare Workers edge runtime: без сборки и npm, `node --test`, минимальный внешний контур |
| `solutions/bb-home-dispatcher`, `solutions/bb-profiles` | JavaScript/TypeScript (плагины bb) | исполняются внутри bb как path-источник; `node --test` без внешних сервисов |
| Windows logon provider | C++17 | in-proc COM DLL для LogonUI; кросс-сборка llvm-mingw с macOS/CI без Visual Studio, переносимые host-тесты |
| HA/Pulsar edge | YAML/C++ | существующий контур, не переписывать ради модности |
| CI-обвязка (`platform/ci`, `platform/quality`) | JavaScript (`actions/github-script`, `node --test`) | Node есть в раннере, без setup-python/pip; локально — `node` |
| задачи на хостах | shell / Python stdlib (короткая обвязка) или Go-бинарник (большая логика) | решение 35 |
| ML wake-word, dev-утилиты станции, проверки YAML, live-e2e | Python | экосистема ML; в проде не исполняется |

Go — для control plane, durable state и всего, что исполняется в k3s (включая
CronJob и инструменты деплоя; решение 35), Rust — для тяжёлых вычислений,
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
go.mod             Go module для новых решений
go.work            workspace корня и отслеживаемого BLE-модуля
```

`MODULE.bazel` — глобальная CI/deploy-инвалидация. Исключение: если его diff
затрагивает только Maven-расширение rules_jvm_external
(`maven.install(...)`/`maven.<tag>(...)`, `use_repo(maven, ...)`) и
комментарии, `affected.mjs` маршрутизирует изменение как `MODULE.bazel#maven`
только в android (меняется Maven/APK-замыкание). Остальные правки —
`bazel_dep`, toolchain, `go_deps`, `oci`, python — остаются глобальными.

Чистый checkout поддерживает `go test ./solutions/scheduler/... ./solutions/alarm/... ./packages/...`;
игнорируемый Pulsar installer проверяется отдельно с `GOWORK=off`.
`make test` требует локальных upstream checkout’ов и Python-зависимостей
(см. `docs/TESTING.md`).

Команды:

```bash
bazel build //solutions/scheduler/... //solutions/alarm/... //packages/proto/...
bazel test  //solutions/scheduler/... //solutions/alarm/...

bazel test //packages/...
bazel run //:gazelle
```

`projects/station/ble-gateway` пока исключён из Bazel (`.bazelignore`) и
собирается своим Go-модулем. Его перевод — отдельная миграция, чтобы не
рисковать станцией. Android собирается root-инструментами в
`projects/mobile/android`.

`solutions/windows-logon-provider` — переиспользуемый компонент, тоже вне
Bazel: его контракт — `build.sh` (кросс-сборка pinned llvm-mingw `20261020`) и
`run-tests.sh` (host clang++), без BUILD-целей и toolchain-правил. В CI это
платформа `native` (`_native.yml`): проверка sha256 архива, host-тесты,
кросс-сборка и artifact. Значения `ServiceUrl`/`Machine`/`UserName` приходят
из проекта при установке. На push в `develop` `deploy-home` публикует файлы
как публичный OCI-артефакт GHCR (ORAS, `VERSION`, digest и provenance);
домашний ПК только читает готовый digest. Переход на Bazel — отдельная задача,
а не текущее состояние.

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
- `projects/*`: композиция, env, compose, systemd.

## Следующие этапы

Открытые задачи — только `PENDING.md`.

## Владение тестами

Тесты относятся к своему solution/package/project и находятся рядом с
кодом. E2E композиции находятся в `projects/<name>/e2e/`. Общая
инфраструктура проверок репозитория — `platform/quality/`. Корневая
папка tests не используется; `make test` агрегирует локальные наборы.
