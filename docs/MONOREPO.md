# Монорепозиторий и Bazel

Статус: **актуальная структура**. HA/voice/Hermes/TTS и station/Pulsar/BLE
разложены по `projects/`; текущий runtime — `STATE.md`.

## Горизонтальная структура

Главный принцип: **solutions переиспользуются, projects их композируют**.

```text
solutions/               переиспользуемые решения
  scheduler/             durable scheduler, SQLite queues, Connect/gRPC
  alarm/                 legacy AlarmService facade над тем же engine
  ha-bridge/             executor-адаптер HA/Pulsar ↔ scheduler
  mcp-gateway/           (планируется) общий extensible MCP gateway
  mcp-alarm/             (планируется) adapter/plugin за mcp-gateway
  assistant-ui/          переиспользуемый PWA/UI

packages/                общий код и контракты
  proto/                 gRPC/protobuf (+ checked-in Go stubs)
  go/                    shared Go
  rust/                  Rust DSP/инференс

projects/                композиции и deployment
  station/               Pulsar/ADB/BLE/provisioning
  home-assistant/        HA/MA/STT/TTS/Hermes/Tailscale
  home/                  station + home-assistant + scheduler/bridge
  cloud/                 отдельный scheduler instance
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
- `projects/cloud` задаёт свой DB path, port, timezone, token;
- `solutions/ha-bridge` арендует только `queue=home` и исполняет события;
- `solutions/mcp-gateway` (future) один для всех сервисов и проектов;
- один и тот же код не копируется между проектами;
- project-specific секреты не попадают в solution.

Пример:

```text
projects/home:
  scheduler@home      sqlite=/data/home/scheduler.db  :8098
  alarm@home          тот же engine + legacy AlarmService facade
  ha-bridge@home      SCHEDULER_ENDPOINT=127.0.0.1:8098, queue=home
  android             Connect JSON → LAN/Tailscale endpoint

projects/cloud:
  scheduler@cloud     sqlite=/data/cloud/scheduler.db :8098
```

Одновременно `scheduler` и `alarm` на одной БД не запускаются: это один
engine, два entrypoint-а для совместимости.

## Решения по языкам

| Слой | Язык | Почему |
|---|---|---|
| `solutions/scheduler` | Go | always-on scheduler/queue, один бинарник, low memory, pure-Go SQLite |
| MCP/HA bridge | Go | сетевые адаптеры, простой деплой, общий код |
| аудио/DSP/инференс | Rust | предсказуемая производительность, нет GC, FFI |
| `projects/mobile/android` | Kotlin + Views | server-authoritative клиент; Metro DI, single Activity, typed Fragment router |
| HA/Pulsar edge | YAML/C++ | существующий контур, не переписывать ради модности |

Go — для control plane и durable state, Rust — для тяжёлых вычислений.
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
- `projects/mobile/android`: управление server state, Connect JSON; нативных
  phone alarm/push пока нет.
- `solutions/mcp-gateway` (future): единый MCP для всех проектов; отдельный
  MCP-сервер на проект не разворачивается.
- `solutions/assistant-ui`: UI/PWA, endpoint из конфига.
- `projects/*`: композиция, env, compose, systemd.

## Следующие этапы

1. Довести Android-клиент до рабочего vertical slice (server-authoritative).
2. Подключить `ha-bridge` к scheduler за флагом, без второго executor-а.
3. Явный cutover HA со звуком и rollback-планом.
4. `mcp-gateway` tool groups, когда появится реальный клиент.
5. Recurring generic tasks и event-trigger rules (сейчас one-shot
   `EnqueueTask`).
6. Deploy/always-on хост и наблюдаемость.
7. Перевести BLE gateway в Bazel.

## Владение тестами

Тесты относятся к своему solution/package/project и находятся рядом с
кодом. E2E композиции находятся в `projects/<name>/e2e/`. Общая
инфраструктура проверок репозитория — `platform/quality/`. Корневая
папка tests не используется; `make test` агрегирует локальные наборы.
