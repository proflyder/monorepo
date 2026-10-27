# alarm

Переиспользуемый сервис будильников. Один и тот же код запускается в разных
проектах с разными SQLite-базами, портами и timezone.

Сейчас это каркас: бинарник печатает `alarm` и завершается.
SQLite, scheduler, сервер и перечисленная конфигурация ниже — план,
а не реализованные возможности.

## Почему Go

- один статический бинарник и простой деплой;
- низкое потребление памяти для always-on процесса;
- быстрый HTTP/gRPC/SSE и MCP-адаптеры;
- удобная кросс-компиляция под Linux;
- Rust остаётся для DSP/аудио/инференса, а не для CRUD-планировщика.

## Структура

```text
cmd/alarm/              entrypoint
internal/app/           process wiring
internal/alarms/        alarm domain, next occurrence, repeat
internal/timers/        future timer domain
internal/reminders/     future reminder domain
internal/tasks/         future tasks
internal/notes/         future notes
internal/store/         SQLite/repositories
internal/http/          REST/Connect handlers
api/                    API notes
```

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
go test ./solutions/alarm/...
go run ./solutions/alarm/cmd/alarm
```

## API

Канонический контракт — gRPC/protobuf:
`packages/proto/ricardo/v1/alarms.proto`. Первый срез — `AlarmService`
(CRUD, enable/disable, snooze, stop). Для HA и браузера тот же сервис будет
доступен через Connect/gRPC-Web; отдельной REST-модели не будет.

## Границы

- HA/Pulsar отвечают за звонок, рингтон, TTS, громкость и restore source.
- `alarm` отвечает за расписания и состояние.
- `ha-bridge` — за голос/вебхуки HA.
- `mcp-alarm` — за Hermes/MCP.
- UI — отдельный solution, не внутри alarm.
