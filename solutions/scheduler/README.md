# scheduler

Durable планировщик будильников, напоминаний, таймеров и generic-задач.
Один процесс, одна SQLite-база, один поток тиков. Сервис **не проигрывает
звук**: он фиксирует occurrence в durable outbox, а executor (HA bridge,
Android, будущий агент) забирает событие, выполняет действие и подтверждает
его.

Статус: durable core, Connect API, legacy `AlarmService` facade, тесты и
K3s-деплой. scheduler — действующий владелец будильников, таймеров и
напоминаний; HA выполняет звук по событиям исполнителя. Android-клиент —
`projects/mobile/android`.

Работающий процесс и контейнерный образ — Rust-крейт `crates/scheduler`
(static `x86_64-unknown-linux-musl`, `rules_rust`). Go-код (`cmd/`, `app/`,
`internal/`) сохранён как эталон контракта, владелец golden-фикстур и
источник для отката образа по digest; он больше не входит в деплой.

## Компоненты Rust-сервиса

```text
crates/scheduler/
  src/domain/               чистая рекуррентность, ids, hashes, validation
  src/store/                SQLite schema, транзакции, tick, outbox, purge
  src/api/                  Connect handlers, bearer auth, alarm facade
  src/config.rs             env 1:1 с Go (SCHEDULER_* + legacy ALARM_*)
  src/engine.rs             tick/purge loop, readiness на home-runtime
  src/app.rs                composition root, graceful shutdown
  src/health.rs             -healthcheck / -status для distroless
  src/main.rs               entrypoint

cmd/scheduler/            Go-эталон entrypoint (не деплоится)
app/                      Go-эталон сборки процесса (не деплоится)
internal/                 Go-эталон: config, idgen, planner, store, engine, api
scripts/generate-proto.sh перегенерация checked-in Go stubs
```

Legacy `ricardo.v1.AlarmService` отдаётся на том же listener, что и
`SchedulerService`. `//solutions/alarm/cmd/alarm` — тот же engine под именем
процесса `alarm`, не второй планировщик.

## Контракт

`packages/proto/ricardo/v1/scheduler.proto`, package `ricardo.v1`,
service `SchedulerService`:

- schedules: `CreateSchedule`, `GetSchedule`, `ListSchedules`,
  `UpdateSchedule`, `DeleteSchedule`, `SetScheduleEnabled`,
  `SnoozeSchedule`, `StopSchedule`;
- timers: `StartTimer`, `GetTimer`, `ListTimers`, `PauseTimer`,
  `ResumeTimer`, `CancelTimer`;
- outbox: `ListDueEvents`, `AckEvent`;
- generic queue: `EnqueueTask`, `CancelTask`;
- status: `GetStatus`.

Streaming (`WatchEvents`) отложен; unary-контракта достаточно для мобильного
клиента и HA bridge. Field names в Connect JSON — lowerCamelCase, `int64` —
decimal string (стандартный protobuf JSON). Пустые/дефолтные поля могут
отсутствовать в JSON.

### Транспорты

Один listener отдаёт Connect unary (JSON и binary) и gRPC-Web по HTTP/1.1.
Отдельного REST API нет. `Content-Type: application/json` +
`/ricardo.v1.SchedulerService/<Method>` — это Connect JSON, а не hand-written
REST.

Отличие Rust-сервиса от прежнего Go: `connectrpc` собран только с feature
`json` (без `hyper/http2`), поэтому gRPC over h2c сейчас не отдаётся. gRPC-Web
и Connect unary работают; живых клиентов, которым нужен h2c, не было.
Добавление h2c — отдельное решение с проверкой зависимостей образа.

Auth: `Authorization: Bearer <token>` на каждом RPC. Сравнение
constant-time по SHA-256; токен не логируется. `SCHEDULER_CONSUMERS`
опционально ограничивает `consumer_id` allowlist.

### Env и образ

Переменные окружения и их приоритеты — 1:1 с Go: `SCHEDULER_DB_PATH`,
`SCHEDULER_LISTEN`, `SCHEDULER_TIMEZONE`, `SCHEDULER_TOKEN(_FILE)`,
`SCHEDULER_CONSUMERS`, `SCHEDULER_MISSED_GRACE_SECONDS`,
`SCHEDULER_RING_WINDOW_SECONDS`, `SCHEDULER_EVENT_RETENTION_SECONDS`,
`SCHEDULER_MAX_TIMER_SECONDS`, `SCHEDULER_MAX_TIMERS`,
`SCHEDULER_MAX_PAYLOAD_BYTES`, `SCHEDULER_EVENT_MAX_ATTEMPTS`,
`SCHEDULER_TICK_MS`; legacy `ALARM_DB_PATH/LISTEN/TIMEZONE` — fallback.
`SCHEDULER_TOKEN` побеждает `SCHEDULER_TOKEN_FILE`.

Образ `//projects/home/kubernetes/scheduler:scheduler_image` — static musl
бинарь на distroless, `user 65534:65534`, entrypoint
`/usr/local/bin/scheduler`. Durable база — `/data/scheduler.db`; UID и
права PVC совпадают с прежним Go-образом, поэтому существующий том
открывается без миграции. `scheduler -healthcheck` и `scheduler -status`
читают токен из файла и не кладут его в аргументы.

## Recurrence и DST

Правило хранится как локальное wall time + IANA timezone + repeat:
`REPEAT_KIND_ONCE/DAILY/WEEKDAYS/WEEKENDS/CUSTOM` (ISO 1=Mon..7=Sun).
Reminder — `SCHEDULE_KIND_REMINDER`, обязательно `REPEAT_KIND_ONCE` с
`once_date`.

Детерминированная политика (реализована и покрыта тестами):

- несуществующее local time (spring-forward gap) **пропускается**: в этот
  день occurrence нет, берётся следующий подходящий день;
- неоднозначное local time (fall-back overlap) — берётся **ранний** UTC
  instant;
- `time.Date` normalization не используется как решение: кандидат
  проверяется чтением компонент в зоне;
- даты перебираются по UTC-календарю, поэтому пропущенная civil date
  (например `Pacific/Apia` 2011-12-30) не зацикливает планировщик.

## Политика опозданий и повторов

- grace `SCHEDULER_MISSED_GRACE_SECONDS` (600 по умолчанию);
- occurrence в пределах grace эмитится как `FIRE`;
- более старый occurrence фиксируется как явный `SKIPPED` event с
  `detail=missed_grace`, после чего расписание **coalesce**-ится к первому
  occurrence после `now`. Пропущенные дни не разворачиваются в лавину
  событий и не блокируют таймеры;
- timer, просроченный на любой срок, эмитится один раз (countdown завершился,
  пока процесс лежал);
- `ONCE` после emission выключается; `snooze` для него всё равно доступен,
  пока активно ringing window.

## Ringing window, stop и snooze

Активное occurrence хранится в schedule (`active_occurrence_unix`,
`active_until_unix`) **отдельно** от transport-состояния события
(`PENDING/LEASED/DELIVERED`). Поэтому:

- `DELIVERED` ack означает «executor принял действие», но не стирает
  возможность `Stop`/`Snooze`;
- `SnoozeSchedule` ставит немедленный `STOP` control event и отложенный
  `FIRE` (тот же occurrence, `snooze_seq+1`);
- `StopSchedule` ставит немедленный `STOP` и отменяет pending triggers;
- повторный `Stop` в том же окне идемпотентен (тот же STOP event);
- `Snooze` после `Stop` возвращает `FAILED_PRECONDITION`;
- пока идёт snooze-ожидание, повторный snooze разрешён (active snooze
  semantics);
- `UpdateSchedule` и `SetScheduleEnabled(true)` во время активного occurrence
  (звонок или pending snooze) закрывают его durable STOP
  (`detail=rule_changed`) со старым label/payload и отменяют pending `FIRE`
  этого occurrence; `Stop` сразу после правки идемпотентен;
- `SetScheduleEnabled(false)` окно не закрывает: STOP придёт по истечении
  `RingWindow`;
- recurrence не сдвигается snooze/stop.

`DueEvent.action`: `FIRE` — начать occurrence, `STOP` — остановить звук.
Control events **не перезаписывают** пользовательский `payload_json`; причина
лежит в `detail`. Trigger и control имеют разные стабильные dedupe identity.

### Истечение ringing window

`RingWindow` (`SCHEDULER_RING_WINDOW_SECONDS`, 900 по умолчанию) — это
**максимальная длительность одного срабатывания**, а не grace доставки.
Scheduler владеет lifecycle occurrence, поэтому истечение окна закрывает
срабатывание, а не только чистит state:

- в той же SQLite-транзакции tick: pending/leased `FIRE` **этого же**
  occurrence помечаются `SKIPPED` (`detail=ring_window_expired`), чтобы
  поздняя доставка не начала звук после закрытия окна;
- для `SCHEDULE_KIND_ALARM` создаётся ровно одна логическая STOP-запись с
  `occurrence_unix` истёкшего occurrence и стабильным dedupe key
  `expiry:stop:<kind>:<id>:<occurrence>`; повторный tick, restart и
  lease-redelivery возвращают тот же event id. Доставка — штатная durable
  очередь с at-least-once (не exactly-once side effect), retry ограничен
  `SCHEDULER_EVENT_MAX_ATTEMPTS`; HA-сторона делает повторный STOP no-op,
  если звонок уже молчит;
- окно и snooze очищаются, revision растёт;
- для reminder/timer глобальный STOP не создаётся (у них нет безопасного
  общего stop); их pending `FIRE` также отменяется;
- `StopSchedule` в пределах `RingWindow` после expiry идемпотентен (тот же
  STOP replay), `SnoozeSchedule` остаётся `FAILED_PRECONDITION`;
- STOP истёкшего occurrence не трогает следующий occurrence того же
  расписания: bridge сверяет source+occurrence, HA — source+FIRE event id.


## Durable outbox, leases, tasks

- `events.dedupe_key` уникален; event id = UUIDv5(dedupe identity), поэтому
  повторный tick, redelivery и restart возвращают тот же id;
- `ListDueEvents` арендует события (`lease_seconds`, default 60) и повышает
  `attempts`; expired lease возвращается в `PENDING`, после
  `SCHEDULER_EVENT_MAX_ATTEMPTS` — `FAILED`;
- первый `AckEvent` требует `LEASED`, точного `consumer_id` и неистёкшего
  lease; невалидный ack не создаёт consumer heartbeat. Terminal replay
  идемпотентен;
- `EnqueueTask` — реальная durable generic-очередь: `task_type`, JSON payload
  (≤16 KiB), `run_at_unix`, `queue`, idempotency key. Один shot; recurring
  generic task schedules пока не реализованы (`SCHEDULE_KIND_TASK` не
  добавлен);
- queues: convenience alarm/reminder/timer события идут в `home` с
  `task_type=alarm.fire|reminder.fire|timer.fire` (плюс `.stop` для control);
  generic tasks по умолчанию в `tasks`. Executor арендует только свою
  очередь; `ListDueEvents.queue=""` — admin/all;
- `CancelTask` отменяет только `PENDING` task event; leased → precondition;
- `GetStatus` показывает server clock, backlog, known queues, consumer
  liveness, `ready` и last tick error. Он не утверждает, что звук был слышен.
- `SCHEDULER_EVENT_RETENTION_SECONDS` чистит только terminal events и старые
  idempotency keys.

## Broker / MCP boundary

Сейчас внешнего broker нет: SQLite inbox/outbox достаточен для одного
always-on home-сервиса. Kafka/NATS можно добавить позже как транспорт
доставки, не меняя canonical protobuf API: `EnqueueTask`/`ListDueEvents`/
`AckEvent` остаются контрактом, а broker станет ещё одним consumer/producer.

MCP живёт в общем `solutions/mcp-gateway`, а не в отдельном сервере на
проект. Scheduler остаётся transport/domain neutral и не импортирует
Hermes/HA. Группы `scheduler.*` в gateway пока нет; когда появится, gateway
отвечает за auth/tenant mapping, а scheduler — за durable state.

## Хранилище

SQLite (`modernc.org/sqlite`, pure Go, без CGO), WAL, `busy_timeout`,
одно соединение (`MaxOpenConns=1`) => один writer. Tick выполняет
event-insert и advance источника в одной транзакции. `BEGIN IMMEDIATE`
не нужен, пока процесс один; второй процесс на той же БД не поддерживается.
Schema version проверяется: БД новее бинарника отвергается.

## Конфигурация

`SCHEDULER_*` и legacy fallback `ALARM_*`:

```text
SCHEDULER_DB_PATH=scheduler.db              (ALARM_DB_PATH)
SCHEDULER_LISTEN=127.0.0.1:8098             (ALARM_LISTEN; loopback default)
SCHEDULER_TIMEZONE=UTC                      (ALARM_TIMEZONE; default для пустого tz)
SCHEDULER_TOKEN=...                         (или SCHEDULER_TOKEN_FILE)
SCHEDULER_MISSED_GRACE_SECONDS=600
SCHEDULER_RING_WINDOW_SECONDS=900
SCHEDULER_EVENT_RETENTION_SECONDS=2592000
SCHEDULER_MAX_TIMERS=64
SCHEDULER_MAX_TIMER_SECONDS=604800
SCHEDULER_MAX_PAYLOAD_BYTES=16384
SCHEDULER_EVENT_MAX_ATTEMPTS=5
SCHEDULER_TICK_MS=1000
SCHEDULER_CONSUMERS=ha-bridge,android       (optional allowlist)
```

Токен обязателен: без него процесс не стартует. Значения секретов не
логируются. Для минимальных хостов IANA tzdata встроена в бинарник.

## Сборка и проверки

```bash
go test ./solutions/scheduler/... ./solutions/alarm/... ./packages/...

bazel build //solutions/scheduler/... //solutions/alarm/... //packages/proto/...
bazel test  //solutions/scheduler/... //solutions/alarm/...
```

Go stubs лежат в репозитории, поэтому `go test` работает без Bazel. Bazel не
генерирует те же Go-типы повторно: `packages/proto/ricardo/v1/BUILD.bazel`
собирает checked-in `.pb.go` через `go_library`, а `proto_library` остаётся
схемой. Перегенерация — `solutions/scheduler/scripts/generate-proto.sh`.

## Legacy AlarmService facade

Все 8 RPC `AlarmService` реализованы поверх того же store
(`kind=SCHEDULE_KIND_ALARM`). `Alarm.revision` (поле 12, additive) даёт
optimistic concurrency для `UpdateAlarm`. `last_ring_unix` означает
«последний emission», а не «прозвучал». Audibility доказывается только
executor-ом и не гарантируется этим сервисом.

## Ограничения

- нет scenario executor;
- нет recurring generic tasks (`SCHEDULE_KIND_TASK`);
- нет streaming `WatchEvents`;
- SQLite рассчитан на один процесс; горизонтальное масштабирование
  потребует другого storage/broker;
- звук, громкость, рингтон и restore source остаются у HA/Pulsar.
