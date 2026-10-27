# ha-bridge

Мост между Home Assistant/Pulsar и выбранным инстансом `solutions/alarm`.

## Задачи

- принимать локальные voice intents от HA;
- вызывать alarm API;
- по событию `ring` запускать HA `alarm_start`/рингтон/TTS;
- принимать `stop`/`snooze` из HA и возвращать их в alarm;
- не хранить собственное расписание и не владеть БД будильников.

## Конфигурация

```text
ALARM_ENDPOINT=http://127.0.0.1:8098
HA_URL=http://127.0.0.1:8123
HA_TOKEN=...
BRIDGE_LISTEN=127.0.0.1:8099
```

Для другого проекта меняется только `ALARM_ENDPOINT`; код моста тот же.
