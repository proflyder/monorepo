# projects/home

Композиция домашнего проекта. Объединяет два независимых проекта:

```text
projects/station          Pulsar/ADB/BLE/provisioning
projects/home-assistant   HA/MA/STT/TTS/Hermes
```

И добавляет домашние solutions:

```text
solutions/scheduler       durable расписания/таймеры/задачи, SQLite outbox
solutions/ha-bridge       executors: home → HA scripts, control → каталог Home
solutions/intercom        сигналинг WebRTC-звонка телефон ↔ колонка
```

Windows-десктоп «комп» (WoL, OpenSSH forced command) настраивается в
`windows-pc/README.md`; HA-часть — в `projects/home-assistant/ha-voice/`.

## Зачем отдельно

- станция и HA развиваются независимо;
- HA можно перезапускать/обновлять без перепрошивки станции;
- один и тот же scheduler/bridge можно направить на другой инстанс;
- project-specific config не попадает в solutions.

## Планировщик и мост

`solutions/scheduler` — server-authoritative планировщик: SQLite durable
очереди (`home` для alarms/reminders/timers, `tasks` для one-shot
`EnqueueTask`), lease/ack, Connect unary + gRPC-Web по HTTP/1.1
(gRPC/h2c не отдаётся). Внешний брокер (Kafka) не
используется; версия контракта — `packages/proto/home/v1/scheduler.proto`.

`solutions/ha-bridge` — исполнитель: забирает только `home`, вызывает
allowlisted HA scripts (`alarm_start`, `schedule_announce`, `alarm_stop`),
подтверждает только после 2xx HA. Работает в режиме execute (1 реплика,
очередь `home`); `ha-control` обслуживает `control`, `ha-scenario` — очередь
`scenario`. scheduler — действующий владелец расписаний, HA остаётся
исполнителем звука. Второй исполнитель `home` не запущен.

Локальный процессный E2E (реальные бинарники + fake HA на loopback, без живых
HA/ADB): `projects/home/e2e/scheduler_smoke.py`.

Рабочая Kubernetes-композиция и pinned Helm releases — [kubernetes/](kubernetes/README.md).
HA Core живёт в HAOS VM; MA и Matter — add-ons в HAOS. K3s VM содержит
STT/TTS, Hermes, scheduler/ha-bridge/ha-control/ha-scenario, intercom,
OpenBao/ESO, mcp-gateway, Nexus и observability. Hermes и другие проекты
используют один общий `mcp-gateway`, а не MCP внутри каждого сервиса; Nexus,
платформу, observability и Vaultwarden разворачивает Flux, ручными остаются
OpenBao, host-firewall и Cloudflare DNS, сервисы приложений — тоже Flux из
pins (`deploy-home` только пинует digest'ы).
