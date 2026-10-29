# scheduler

Durable планировщик будильников, напоминаний, таймеров и generic-задач.
Один процесс, одна SQLite-база, один поток тиков. Сервис **не проигрывает
звук**: он фиксирует occurrence в durable outbox, а executor (HA bridge,
Android, будущий агент) забирает событие, выполняет действие и подтверждает
его.

Статус: durable core, Connect/gRPC API, legacy `AlarmService` facade и
backend-тесты реализованы. Deploy, HA/Pulsar-интеграция и Android-клиент —
отдельные изменения; live HA по-прежнему владеет звуком.

## Компоненты

```text
solutions/scheduler/
  cmd/scheduler/            entrypoint (scheduler)
  app/                      общая сборка процесса (используется и alarm)
  internal/config/          env, валидация, loopback-default
  internal/idgen/           UUIDv7 ресурсы, UUIDv5 события
  internal/planner/         чистая рекуррентность + IANA/DST
  internal/store/           SQLite schema, транзакции, tick, outbox
  internal/engine/          tick loop, purge, readiness
  internal/api/             Connect/gRPC handlers, auth, alarm facade
  scripts/generate-proto.sh перегенерация checked-in Go stubs
```

`//solutions/alarm/cmd/alarm` запускает тот же engine и дополнительно отдаёт
legacy `ricardo.v1.AlarmService`. Это один процесс и одна БД, не второй
планировщик.

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

Один listener отдаёт **все три протокола** через connect-go handlers:
Connect unary (JSON и binary), gRPC over HTTP/2 (включая h2c) и gRPC-Web.
Отдельного REST API нет. `Content-Type: application/json` +
`/ricardo.v1.SchedulerService/<Method>` — это Connect JSON, а не hand-written
REST. Для gRPC нужен HTTP/2; h2c работает на loopback/LAN, TLS-termination —
задача deployment-слоя.

Auth: `Authorization: Bearer <token>` на каждом RPC. Сравнение
constant-time по SHA-256; токен не логируется. `SCHEDULER_CONSUMERS`
опционально ограничивает `consumer_id` allowlist.

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
- recurrence не сдвигается snooze/stop.

`DueEvent.action`: `FIRE` — начать occurrence, `STOP` — остановить звук.
Control events **не перезаписывают** пользовательский `payload_json`; причина
лежит в `detail`. Trigger и control имеют разные стабильные dedupe identity.

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

MCP в этом срезе **не реализуется**. Будущая схема: один общий extensible MCP
gateway для проектов (Hermes/HA) с доменной группой scheduler, а не
отдельный MCP-сервер на каждый проект. Scheduler остаётся transport/domain neutral и
не импортирует Hermes/HA. Будущие tool groups gateway: `scheduler.schedules.*`
(create/list/update/enable/snooze/stop), `scheduler.timers.*`
(start/pause/resume/cancel/list), `scheduler.tasks.*` (enqueue/cancel/status),
`scheduler.events.*` (lease/ack только для executor-роли). Gateway отвечает
за auth/tenant mapping; scheduler — за durable state.

## Хранилище

SQLite (`modernc.org/sqlite`, pure Go, без CGO), WAL, `busy_timeout`,
одно соединение (`MaxOpenConns=1`) => один writer. Tick выполняет
event-insert и advance источника в одной транзакции. `BEGIN IMMEDIATE`
не нужен, пока процесс один; второй процесс на той же БД не поддерживается.
Schema version проверяется: БД новее бинарника отвергается.

## Конфигурация

`SCHEDULER_*` и legacy fallback `ALARM_*`:

```text
SCHEDULER_DB_PATH=/data/home/scheduler.db   (ALARM_DB_PATH; default scheduler.db)
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

## Ограничения текущего среза

- нет deploy-манифеста и выбора always-on хоста;
- нет HA bridge, Android-клиента, MCP и scenario executor;
- нет recurring generic tasks (`SCHEDULE_KIND_TASK`);
- нет streaming `WatchEvents`;
- SQLite рассчитан на один процесс; горизонтальное масштабирование
  потребует другого storage/broker;
- звук, громкость, рингтон и restore source остаются у HA/Pulsar.
