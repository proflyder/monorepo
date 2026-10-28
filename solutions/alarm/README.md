# alarm

Переиспользуемый сервис будильников. Один и тот же код запускается в разных
проектах с разными SQLite-базами, портами и timezone.

Реализация живёт в `solutions/scheduler` (durable SQLite engine, planner,
outbox, Connect/gRPC). Этот бинарник запускает тот же engine и дополнительно
отдаёт legacy `AlarmService` facade поверх общих schedules
(`kind=SCHEDULE_KIND_ALARM`). Это один процесс и одна БД, а не второй
планировщик; одновременно запускать `scheduler` и `alarm` на одной БД нельзя.

## Почему Go

- один статический бинарник и простой деплой;
- низкое потребление памяти для always-on процесса;
- быстрый HTTP/gRPC/SSE и MCP-адаптеры;
- удобная кросс-компиляция под Linux;
- Rust остаётся для DSP/аудио/инференса, а не для CRUD-планировщика.

## Структура

```text
cmd/alarm/              entrypoint: scheduler app + legacy AlarmService facade
internal/app/           process identity
api/                    API notes
```

Домены, store, planner и сервер находятся в `solutions/scheduler`; README
там описывает контракт, DST-политику, stop/snooze и очередь задач.

UI и MCP вынесены в отдельные solutions:

```text
solutions/assistant-ui
solutions/mcp-alarm
solutions/ha-bridge
```

## Конфигурация инстанса

Проект задаёт:

```text
ALARM_DB_PATH=/data/home/alarm.db
ALARM_LISTEN=127.0.0.1:8098
ALARM_TIMEZONE=Asia/Almaty
ALARM_PUBLIC_URL=...
```

Никаких захардкоженных путей `home`/`cloud` внутри сервиса.

## Сборка

```bash
bazel build //solutions/alarm
bazel test //solutions/alarm/...
bazel run //solutions/alarm
```

Без Bazel:

```bash
go test ./solutions/alarm/... ./solutions/scheduler/... ./packages/...
go run ./solutions/alarm/cmd/alarm
```

## API

Канонические контракты — `packages/proto/ricardo/v1/scheduler.proto`
(`SchedulerService`, основной) и `alarms.proto` (legacy `AlarmService`).
Оба сервиса отдаются на одном listener через Connect (JSON/HTTP), gRPC
(HTTP/2, h2c) и gRPC-Web. Отдельной REST-модели нет. Все 8 legacy RPC
реализованы как view над общим engine; `Alarm.last_ring_unix` — последний
emission, не подтверждение звука.

## Границы

- HA/Pulsar отвечают за звонок, рингтон, TTS, громкость и restore source.
- `alarm` отвечает за расписания и состояние.
- `ha-bridge` — за голос/вебхуки HA.
- `mcp-alarm` — за Hermes/MCP.
- UI — отдельный solution, не внутри alarm.
