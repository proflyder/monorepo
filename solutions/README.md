# solutions

Горизонтальные переиспользуемые решения. Каждое solution — самостоятельный
компонент с собственным API/бинарником, но **без привязки к конкретному
проекту, базе или инстансу**.

| Solution | Язык | Назначение |
|---|---|---|
| `alarm/` | Go | сервис будильников: CRUD, repeat, next_run, snooze/stop |
| `mcp-alarm/` | Go | MCP-адаптер к любому инстансу alarm по endpoint |
| `ha-bridge/` | Go/Python | мост HA/Pulsar ↔ alarm: voice intents, ring, stop/snooze |
| `assistant-ui/` | Go + HTMX | переиспользуемый PWA/UI, endpoint задаётся конфигом |

## Правила

- solution не знает про `projects/home` или `projects/cloud`;
- один и тот же `alarm` запускается в разных проектах с разными SQLite/портами;
- один и тот же `mcp-alarm` может смотреть на домашний или облачный alarm;
- общие контракты — в `packages/proto`, общий код — в `packages/go`;
- секреты, endpoint и DB path приходят из env/конфига проекта;
- `solutions/` не импортирует `projects/` и не содержит project-specific
  compose/env.

## Пример

```text
projects/home:
  alarm@home       sqlite=/data/home/alarm.db   :8098
  mcp-alarm@home   ALARM_ENDPOINT=127.0.0.1:8098
  ha-bridge@home   ALARM_ENDPOINT=127.0.0.1:8098
  assistant-ui     ALARM_ENDPOINT=/assistant

projects/cloud:
  alarm@cloud      sqlite=/data/cloud/alarm.db   :8098
  mcp-alarm@cloud  ALARM_ENDPOINT=alarm-cloud:8098
```

Это один и тот же код, разные инстансы и конфигурация.
