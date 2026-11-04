# alarm

Совместимый entrypoint с именем процесса `alarm` над тем же durable engine,
что и `solutions/scheduler`: `cmd/alarm` вызывает `scheduler/app.Run` и
отличается только именем в логах. Legacy `AlarmService`
(`packages/proto/ricardo/v1/alarms.proto`) отдаёт и сам `scheduler`, поэтому
в домашнем проекте этот бинарник не развёрнут.

Контракт, DST-политика, stop/snooze, очередь задач и конфигурация
(`SCHEDULER_*`, с fallback на `ALARM_DB_PATH`/`ALARM_LISTEN`) описаны в
[`../scheduler/README.md`](../scheduler/README.md). Одновременно запускать
`scheduler` и `alarm` на одной БД нельзя.

```bash
bazel build //solutions/alarm
bazel test //solutions/alarm/...
```
