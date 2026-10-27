# gRPC contract

Статус: **контракт и Bazel-кодогенерация готовы, сервер ещё впереди**.

## Решение

Канонический API — **protobuf/gRPC**, не REST-first.

```text
packages/proto/ricardo/v1/*.proto   ← единственный контракт
solutions/alarm                     ← gRPC server + scheduler + store
solutions/assistant-ui              ← Connect/gRPC-Web client
solutions/mcp-alarm                 ← MCP adapter (отдельная поверхность)
solutions/ha-bridge                 ← HA/Pulsar ↔ alarm
```

Первый вертикальный срез — `AlarmService` в
`packages/proto/ricardo/v1/alarms.proto`.

## Транспорт для разных клиентов

Чистый gRPC не умеет HA `rest_command` и обычный браузер. Поэтому:

- **gRPC** — основной протокол для Go/Rust/mobile;
- **Connect** (wire-compatible с gRPC) — тот же сервис для браузера и HA;
- **gRPC-Web** — альтернатива, если понадобится web-прокси;
- **MCP** — отдельный HTTP-протокол для Hermes, не замена gRPC.

Это не два API: один protobuf-контракт и один сервер, разные транспорты.

## Ошибки

- `INVALID_ARGUMENT` — нет времени, bad repeat/weekday;
- `NOT_FOUND` — неизвестный alarm id;
- `ALREADY_EXISTS` — конфликт idempotency key;
- `FAILED_PRECONDITION` — нельзя остановить/отложить незвонящий будильник;
- `UNAVAILABLE` — HA/Pulsar bridge недоступен.

Mutating RPC принимают idempotency key, чтобы retry не создавал дубли.

## Время

- `hour`, `minute`, `timezone` — локальное правило;
- `next_run_unix`, `snooze_until_unix`, `last_ring_unix` — UTC epoch seconds;
- пересчёт next occurrence — только в alarm scheduler;
- DST и смена timezone — обязательные тесты.

## Auth

- gRPC: bearer token в metadata, отдельный токен для HA bridge;
- UI: собственная сессия + CSRF;
- MCP: отдельный токен;
- сеть: только loopback/LAN/Tailscale, не публичный интернет.

## Следующие шаги

1. Сгенерировать/подключить Go stubs (уже есть Bazel target).
2. Реализовать SQLite store и planner.
3. Поднять gRPC/Connect server.
4. Добавить HA bridge и MCP.
5. Только после этого подключать UI.
