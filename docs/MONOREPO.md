# Монорепозиторий и Bazel

Статус: **актуальная структура**. HA/voice/Hermes/TTS и station/Pulsar/BLE
разложены по `projects/`; текущий runtime — `STATE.md`.

## Горизонтальная структура

Главный принцип: **solutions переиспользуются, projects их композируют**.

```text
solutions/               переиспользуемые решения
  alarm/                 Go-сервис будильников
  mcp-alarm/             MCP-адаптер к любому alarm endpoint
  ha-bridge/             HA/Pulsar ↔ alarm
  assistant-ui/          переиспользуемый PWA/UI

packages/                общий код и контракты
  proto/                 gRPC/protobuf
  go/                    shared Go
  rust/                  Rust DSP/инференс

projects/                композиции и deployment
  station/               Pulsar/ADB/BLE/provisioning
  home-assistant/        HA/MA/STT/TTS/Hermes/Tailscale
  home/                  station + home-assistant + alarm/MCP
  cloud/                 отдельный alarm + MCP
  mobile/                будущие клиенты (projects/mobile)

platform/                Bazel/toolchains/deploy-шаблоны
docs/                    документация
archive/                  исторические материалы
```

### Почему так

- `solutions/alarm` не знает, где он запущен;
- `projects/station` и `projects/home-assistant` независимы;
- `projects/home` — их композиция плюс домашний alarm/MCP;
- `projects/cloud` задаёт свой DB path, port, timezone, endpoint;
- `solutions/mcp-alarm` может смотреть на любой alarm;
- один и тот же код не копируется между проектами;
- project-specific секреты не попадают в solution.

Пример:

```text
projects/home:
  alarm@home       sqlite=/data/home/alarm.db   :8098
  mcp-alarm@home   ALARM_ENDPOINT=alarm-home:8098
  ha-bridge@home   ALARM_ENDPOINT=alarm-home:8098
  assistant-ui     ALARM_ENDPOINT=/assistant

projects/cloud:
  alarm@cloud      sqlite=/data/cloud/alarm.db  :8098
  mcp-alarm@cloud  ALARM_ENDPOINT=alarm-cloud:8098
```

## Решения по языкам

| Слой | Язык | Почему |
|---|---|---|
| `solutions/alarm` | Go | always-on gRPC/Connect/scheduler, один бинарник, низкая память |
| MCP/HA bridge | Go | сетевые адаптеры, простой деплой, общий код |
| аудио/DSP/инференс | Rust | предсказуемая производительность, нет GC, FFI |
| mobile | Kotlin/Swift/Flutter | после фиксации API |
| HA/Pulsar edge | YAML/C++ | существующий контур, не переписывать ради модности |

Go — для control plane, Rust — для тяжёлых вычислений. Это не «всё на одном
языке», а разделение по типу нагрузки.

## Bazel

```text
MODULE.bazel       Bzlmod: rules_go, gazelle, rules_rust, rules_proto
MODULE.bazel.lock  зафиксированные зависимости
.bazelversion      Bazel 9.3.1
.bazelrc           cache/вывод
.bazelignore       тяжёлые/чужие/legacy каталоги
BUILD.bazel        //:gazelle
go.mod             Go module для новых решений
go.work            workspace корня и отслеживаемого BLE-модуля
```

Чистый checkout поддерживает `go test ./...`; игнорируемый Pulsar installer
проверяется отдельно с `GOWORK=off`. `make test` требует
локальных upstream checkout’ов и Python-зависимостей (см. `docs/TESTING.md`).

Команды:

```bash
bazel build //...
bazel test //...
bazel run //solutions/alarm
bazel build //packages/proto/...
bazel run //:gazelle
```

`projects/station/ble-gateway` пока исключён из Bazel (`.bazelignore`) и собирается
своим Go-модулем. Его перевод — отдельная миграция, чтобы не рисковать
станцией.

## gRPC

Канонический контракт — `packages/proto/ricardo/v1/alarms.proto`.
`rules_go` + `go_grpc_library` генерируют Go/gRPC stubs. Для браузера и HA
используется Connect/gRPC-Web на том же контракте; MCP — отдельный adapter.
Подробности — `GRPC.md`.

## Границы

- HA/Pulsar: звонок, рингтон, TTS, громкость, restore source.
- `solutions/alarm`: расписания, CRUD, next_run, snooze/stop.
- `solutions/mcp-alarm`: только MCP → alarm.
- `solutions/ha-bridge`: голос/вебхуки HA → alarm.
- `solutions/assistant-ui`: UI/PWA, endpoint из конфига.
- `projects/*`: композиция, env, compose, systemd.

## Следующие этапы

1. Реализовать `alarm` vertical slice: SQLite + planner + gRPC/Connect.
2. Поднять `mcp-alarm` и `ha-bridge`.
3. Собрать `projects/home` и `projects/cloud`.
4. UI/PWA.
5. Timers/reminders/tasks/notes как отдельные solutions.
6. Перевести BLE gateway в Bazel.
7. Mobile после стабилизации контрактов.

## Владение тестами

Тесты относятся к своему solution/package/project и находятся рядом с
кодом. E2E композиции находятся в `projects/<name>/e2e/`. Общая
инфраструктура проверок репозитория — `platform/quality/`. Корневая
папка tests не используется; `make test` агрегирует локальные наборы.
