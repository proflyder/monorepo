# projects/cloud

Отдельный инстанс alarm без домашнего HA/Pulsar. Пример горизонтальной
композиции: тот же код, другая БД и endpoint.

```text
solutions/alarm        instance=cloud, db=/data/cloud/alarm.db
solutions/mcp-alarm    endpoint=cloud alarm
```

HA bridge и домашний UI сюда не входят. Позже можно добавить отдельный UI
или направить `mcp-alarm` на облачный инстанс из другого проекта.
