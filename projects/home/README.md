# projects/home

Композиция домашнего проекта. Объединяет два независимых проекта:

```text
projects/station          Pulsar/ADB/BLE/provisioning
projects/home-assistant   HA/MA/STT/TTS/Hermes/Tailscale
```

И добавляет домашние solutions:

```text
solutions/scheduler       durable расписания/таймеры/задачи, SQLite outbox
solutions/ha-bridge       executor очереди home → HA scripts
solutions/alarm           legacy instance=home (AlarmService facade)
solutions/ha-control      каталог Home + executor queue=control
```

## Зачем отдельно

- станция и HA развиваются независимо;
- HA можно перезапускать/обновлять без перепрошивки станции;
- один и тот же scheduler/bridge можно направить на другой инстанс;
- project-specific config не попадает в solutions.

## Планировщик и мост

`solutions/scheduler` — server-authoritative планировщик: SQLite durable
очереди (`home` для alarms/reminders/timers, `tasks` для one-shot
`EnqueueTask`), lease/ack, Connect/gRPC API. Внешний брокер (Kafka) не
используется; версия контракта — `packages/proto/ricardo/v1/scheduler.proto`.

`solutions/ha-bridge` — исполнитель: забирает только `home`, вызывает
allowlisted HA scripts (`alarm_start`, `schedule_announce`, `alarm_stop`),
подтверждает только после 2xx HA. По умолчанию dry-run; live cutover не
выполнен, текущий HA остаётся владельцем звука и расписаний.

Локальный процессный E2E (реальные бинарники + fake HA на loopback, без живых
HA/ADB/Tailscale): `projects/home/e2e/scheduler_smoke.py`.

Рабочая Kubernetes-композиция и pinned Helm releases — [kubernetes/](kubernetes/README.md).
HA Core перенесён в HAOS VM; K3s VM содержит MA/Matter/STT/TTS/Hermes и
observability. Старые HA data paths сохранены как архив/PV, но не являются
текущим writer. Legacy alarm/MCP/UI solutions остаются отдельными дополнениями;
Hermes и другие проекты в будущем используют один общий MCP gateway, а не
MCP внутри каждого сервиса.
