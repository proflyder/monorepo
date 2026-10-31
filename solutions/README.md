# solutions

Горизонтальные переиспользуемые решения. Каждое solution — самостоятельный
компонент с собственным API/бинарником, но **без привязки к конкретному
проекту, базе или инстансу**.

| Solution | Язык | Назначение | Статус |
|---|---|---|---|
| `scheduler/` | Go | durable scheduler: alarms/reminders/timers как conveniences, SQLite outbox/queue, leases/ack, Connect/gRPC | развёрнут |
| `alarm/` | Go | legacy `AlarmService` facade над тем же engine + совместимый entrypoint | реализован facade |
| `ha-bridge/` | Go | executor-адаптер: арендует `queue=home`, запускает HA audio scripts, подтверждает события; `cmd/ha-control` — каталог Home и очередь `control` | развёрнут |
| `currency-bot/` | Go | курсы kurs.kz → SQLite, алерты и API; Telegram webhook через Cloudflare Tunnel, импорт H2 | развёрнут |

## Правила

- solution не знает про `projects/home`;
- один и тот же `scheduler` запускается в разных проектах с разными
  SQLite/портами/токенами/timezone;
- будильник, напоминание и таймер — conveniences над durable scheduling и
  очередями, а не отдельные планировщики;
- generic `EnqueueTask` — реальная one-shot очередь; recurring generic tasks
  и event-trigger rules — future;
- executor арендует только свои очереди: HA bridge — `home`, generic
  workers — `tasks`, scenario изолирован до появления executor-а;
- общие контракты — в `packages/proto`, общий код — в `packages/go`;
- секреты, endpoint и DB path приходят из env/конфига проекта;
- `solutions/` не импортирует `projects/` и не содержит project-specific
  compose/env;
- MCP не разворачивается per-project; будущие tool groups живут в одном
  `mcp-gateway`.

## Пример

```text
projects/home:
  scheduler@home    sqlite=/data/home/scheduler.db  :8098
  alarm@home        тот же engine + legacy AlarmService facade
  ha-bridge@home    SCHEDULER_ENDPOINT=127.0.0.1:8098, queue=home
  ha-control@home   каталог Home, queue=control
  currency-bot@home sqlite=/data/currency-bot.db :8080/:8081, webhook через tunnel
  android           Connect JSON → LAN/VPN HTTPS endpoint
```

Это один и тот же код, разные инстансы и конфигурация. Два executor-а на
одни и те же live schedules одновременно не включаются.
