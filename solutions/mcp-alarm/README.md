# mcp-alarm

MCP-адаптер, который умеет работать с **любым** инстансом `solutions/alarm`.
Никакой собственной БД и никакой привязки к домашнему проекту.

## Конфигурация

```text
ALARM_ENDPOINT=http://127.0.0.1:8098
ALARM_MCP_TOKEN=...
ALARM_MCP_LISTEN=127.0.0.1:8644
```

Примеры:

```text
projects/home:  ALARM_ENDPOINT=alarm-home:8098
projects/cloud: ALARM_ENDPOINT=alarm-cloud:8098
```

## Tools

```text
alarm_create
alarm_get
alarm_list
alarm_update
alarm_delete
alarm_set_enabled
alarm_snooze
alarm_stop
```

MCP — отдельная поверхность для Hermes; локальный голос ходит в alarm
напрямую через Connect/gRPC, без LLM.

## Почему отдельно

Один и тот же MCP-код можно направить на домашний или облачный alarm.
Проект задаёт только endpoint и токен. Это и есть горизонтальная композиция.
