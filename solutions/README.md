# solutions

Горизонтальные переиспользуемые решения. Каждое solution — самостоятельный
компонент с собственным API/бинарником, но **без привязки к конкретному
проекту, базе или инстансу**.

| Solution | Язык | Назначение | Статус |
|---|---|---|---|
| `scheduler/` | Go | durable scheduler: alarms/reminders/timers как conveniences, SQLite outbox/queue, leases/ack, Connect/gRPC | развёрнут |
| `alarm/` | Go | совместимый entrypoint с именем `alarm` над тем же engine (`AlarmService` отдаёт и `scheduler`) | не развёрнут |
| `ha-bridge/` | Go | executor-адаптер: арендует `queue=home`, запускает HA audio scripts, подтверждает события; `cmd/ha-control` — каталог Home и очередь `control` | развёрнут |
| `currency-bot/` | Go | курсы kurs.kz → SQLite, алерты и API; Telegram webhook | развёрнут |
| `mcp-gateway/` | Go | MCP Streamable HTTP шлюз: группы `infra.read`, `infra.fix`, `infra.report`; каталог сервисов из env | развёрнут |
| `voice-proxy/` | Go | OpenAI-совместимый прокси перед LLM: делает ответы пригодными для Piper TTS | развёрнут |
| `wakegrant/` | Go | одноразовые гранты входа для ПК, разбуженного HA (`packages/contracts/wakegrant`) | развёрнут |
| `heartbeat/` | JS | Cloudflare Worker: внешний dead-man switch для Alertmanager `Watchdog` | развёрнут |
| `bb-home-dispatcher/` | TS | плагин bb: детерминированный исполнитель голосовых команд от `mcp-gateway` | развёрнут |
| `bb-profiles/` | TS | плагин bb: изолированные профили (логины, конфиги, провайдеры, машины) | развёрнут |
| `windows-logon-provider/` | C++17 | Windows Credential Provider: плитка входа только при гранте `wakegrant`; конфигурация приходит при установке; публикуется в GHCR как OCI-артефакт | установлен на домашнем ПК |

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

## Инстансы

Конкретные инстансы, endpoint, DB path и секреты описывает проект
(`projects/home/kubernetes/`, `projects/home/proxmox/`). Один и тот же код
запускается с разной конфигурацией; два executor-а на одни и те же live
schedules одновременно не включаются.
