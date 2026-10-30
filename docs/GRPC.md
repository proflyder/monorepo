# gRPC / Connect contract

Статус: **развёрнуто**. `solutions/scheduler` — durable SQLite engine,
Connect/gRPC, outbox, auth; `ha-bridge` (очередь `home`) и `cmd/ha-control`
(каталог Home, очередь `control`) работают в K3s; HA-звук остаётся у HA, а
расписания принадлежат scheduler. Android-клиент использует тот же контракт.

## Канонические контракты

```text
packages/proto/ricardo/v1/scheduler.proto   SchedulerService (основной)
packages/proto/ricardo/v1/alarms.proto      AlarmService (legacy facade)
```

`SchedulerService` — один protobuf-контракт для расписаний, таймеров,
durable outbox и generic-очереди задач:

- schedules: `CreateSchedule`, `GetSchedule`, `ListSchedules`,
  `UpdateSchedule`, `DeleteSchedule`, `SetScheduleEnabled`,
  `SnoozeSchedule`, `StopSchedule`;
- timers: `StartTimer`, `GetTimer`, `ListTimers`, `PauseTimer`,
  `ResumeTimer`, `CancelTimer`;
- outbox: `ListDueEvents`, `AckEvent`;
- queue: `EnqueueTask`, `CancelTask`;
- status: `GetStatus`.

`AlarmService` сохранён как wire-совместимый facade над теми же schedules
(`SCHEDULE_KIND_ALARM`). Оба сервиса отдаются одним процессом и одной БД; это
не два планировщика. `Alarm.revision` (поле 12) добавлено additive для
optimistic concurrency.

Streaming (`WatchEvents`) не реализован; unary-контракта достаточно для
текущего среза. Recurring generic task schedules (`SCHEDULE_KIND_TASK`) тоже
не реализованы — generic-очередь пока one-shot (`EnqueueTask`).

## Транспорт

Один listener (`SCHEDULER_LISTEN`, по умолчанию `127.0.0.1:8098`) отдаёт
все протоколы через connect-go handlers:

- **Connect unary** — JSON (`application/json`) и binary
  (`application/proto`);
- **gRPC** — HTTP/2, включая h2c на loopback/LAN;
- **gRPC-Web** — для браузерных клиентов за прокси.

Отдельного REST-фасада нет: `/ricardo.v1.SchedulerService/CreateSchedule` с
JSON body — это Connect JSON поверх того же protobuf, а не hand-written API.
TLS/проксирование наружу — задача deployment-слоя; встроенного TLS нет.

Правила protobuf JSON:

- имена полей — lowerCamelCase (`nextDueUnix`, `consumerId`);
- `int64` — decimal string; `int32` — число;
- enum — строковое имя (`SCHEDULE_KIND_ALARM`, `REPEAT_KIND_DAILY`);
- поля с дефолтным значением могут отсутствовать в ответе.

Пример (loopback, тестовый токен подставляется из окружения; не логируется):

```bash
curl -sS http://127.0.0.1:8098/ricardo.v1.SchedulerService/CreateSchedule \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $SCHEDULER_TOKEN" \
  -d '{"label":"wake","kind":"SCHEDULE_KIND_ALARM","timezone":"UTC",
       "at":{"hour":7,"minute":30},"repeat":"REPEAT_KIND_DAILY","enabled":true}'
```

## Auth и лимиты

- `Authorization: Bearer <token>` обязателен для каждого RPC; сравнение
  constant-time по SHA-256;
- токен задаётся `SCHEDULER_TOKEN` или `SCHEDULER_TOKEN_FILE`; без него
  процесс не стартует;
- `SCHEDULER_CONSUMERS` — опциональный allowlist `consumer_id` для
  executor-ов;
- тело запроса/ответа ограничено 1 MiB, заголовки — 16 KiB, read/write
  timeouts и `MaxHeaderBytes` заданы на HTTP-сервере;
- значения токенов и payload не пишутся в логи; `GetStatus.last_tick_error`
  ограничен и не содержит секретов.

## Семантика, которую должен знать клиент

- **Время:** правило хранится как local wall time + IANA timezone. В
  spring-forward gap occurrence пропускается, в fall-back overlap берётся
  ранний UTC instant. `next_due_unix` — UTC epoch seconds.
- **Missed grace:** 600 с по умолчанию. Более старый occurrence даёт явный
  `SKIPPED` event, затем расписание перескакивает к следующему occurrence
  после `now` (coalescing, без лавины).
- **Ringing window:** активное occurrence хранится в schedule
  (`active_occurrence_unix`, `active_until_unix`) отдельно от transport
  состояния события. `DELIVERED` ack не запрещает `Stop`/`Snooze`.
  `RingWindow` — максимальная длительность срабатывания: tick закрывает
  истёкшее окно и в той же транзакции ставит durable `STOP` для этого
  occurrence (alarm), отменяя pending `FIRE` того же occurrence. executor
  обязан идемпотентно принять такой STOP и не затрагивать более новый
  occurrence.
- **Control events:** `SnoozeSchedule` ставит немедленный `STOP` + отложенный
  `FIRE`; `StopSchedule` — немедленный `STOP`. `DueEvent.action`:
  `FIRE`/`STOP`; `UNSPECIFIED` от старых строк трактуется как `FIRE`.
  Control events не меняют пользовательский `payload_json`; причина в
  `detail`.
- **Recurrence:** snooze/stop не сдвигают расписание. `ONCE` выключается
  после emission, но может быть snoozed в активном окне.
- **Idempotency:** `CreateSchedule`, `StartTimer`, `EnqueueTask` принимают
  ключ; повтор с тем же payload возвращает исходный ресурс, другой payload —
  `ALREADY_EXISTS`.
- **Revision:** `UpdateSchedule.expected_revision` ≠ 0 и не совпал —
  `ABORTED`. Engine-изменения (fire, snooze, stop, истечение окна) повышают
  revision, поэтому stale UI не перезапишет новое состояние.
- **Outbox:** `ListDueEvents` арендует события (`lease_seconds`), expired
  lease возвращается с тем же id, после лимита attempts — `FAILED`. Первый
  `AckEvent` требует `LEASED` + точный `consumer_id` + неистёкший lease.
- **Очереди:** convenience alarm/reminder/timer идут в `home` с
  `task_type=alarm.fire|reminder.fire|timer.fire` (`.stop` для control).
  Generic `EnqueueTask` — в `tasks` (default) или указанную очередь
  (`home.control` — в `control`). Executor арендует только свою очередь;
  `queue=""` — admin/all.
  Scenario-события изолированы в очереди `scenario` и не попадают в home,
  пока scenario executor не реализован.
- **Звук:** сервис никогда не утверждает, что будильник прозвучал.
  `GetStatus` отдаёт server clock, backlog, known queues, consumer liveness,
  `ready` и last tick error. Audibility подтверждает только executor.

## Ошибки

| Код | Когда |
|---|---|
| `INVALID_ARGUMENT` | bad timezone/time/date/weekday, payload >16 KiB, duration вне границ, bad task_type/queue |
| `NOT_FOUND` | неизвестный id или удалённый ресурс |
| `ALREADY_EXISTS` | idempotency key с другим payload |
| `ABORTED` | revision conflict |
| `FAILED_PRECONDITION` | snooze/stop не в ringing window, cancel leased task, ack без активного lease, неверный consumer |
| `PERMISSION_DENIED` | consumer вне allowlist |
| `RESOURCE_EXHAUSTED` | превышен лимит активных таймеров |
| `UNAUTHENTICATED` | нет/неверный bearer |
| `INTERNAL` | внутренняя ошибка без деталей клиенту |

## Сгенерированные исходники

Go stubs (`.pb.go`, `.connect.go`) **лежат в репозитории**, чтобы
`go test ./packages/...` работал без Bazel и protoc. Перегенерация:

```bash
PATH=secrets/local-tools/protoc/bin:secrets/local-tools/bin:$PATH \
  solutions/scheduler/scripts/generate-proto.sh
```

Pinned toolchain: protoc 33.4, protoc-gen-go v1.36.x, protoc-gen-connect-go
v1.21.x. Bazel не генерирует те же Go-типы повторно:
`packages/proto/ricardo/v1/BUILD.bazel` собирает checked-in файлы через
`go_library`; `proto_library` остаётся канонической схемой. Прежние публичные
target-имена (`alarms_go_proto`, `scheduler_go_proto`) сохранены как aliases.

## Клиенты

- **Android** (`projects/mobile/android`, namespace `dev.proflyder.home`,
  Views + Kotlin + Metro, single Activity + typed Fragment router) говорит
  Connect JSON по тому же контракту. Сервер авторитетен для расписаний и
  событий; нативных phone alarm и push в этом срезе нет. Транспорт —
  LAN/VPN; TLS-терминация — на Traefik.
- **HA bridge** арендует только `queue=home`, маппит `alarm.fire` →
  `alarm_start`, reminder/timer → announce script, `STOP` → `alarm_stop`.
- **ha-control** отдаёт `GET /v1/catalog` из live HA (сценарии — скрипты с
  меткой `scenario`) и исполняет только `home.control` из очереди `control`.

## Что не реализовано

- streaming `WatchEvents`;
- recurring generic tasks и event-trigger rules;
- scenario executor (`SCHEDULE_KIND_SCENARIO` зарезервирован);
- MCP gateway (только при появлении реального клиента);
- Kafka/external broker (SQLite inbox/outbox достаточен для одного сервиса;
  broker можно добавить позже без изменения контракта).
