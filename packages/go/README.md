# packages/go

Общие Go-пакеты. Пакет добавляется сюда, когда у него появляется второй
потребитель: сейчас это `solutions/ha-bridge` и `solutions/voice-proxy`
(решение 35).

| Пакет | Назначение | Потребители |
|---|---|---|
| `httpapi` | HTTP-транспорт с включённой TLS-проверкой и без редиректов, типизированная ошибка API с классификацией transient/permanent и санитайзер серверных сообщений | `ha`, `scheduler`, `solutions/ha-bridge`, `solutions/voice-proxy` |
| `ha` | узкий Home Assistant REST client: script/service calls, `/api/states`, шаблоны, ping | `solutions/ha-bridge` (discovery, control), `solutions/voice-proxy` |
| `scheduler` | Connect-JSON клиент канонического `ricardo.v1.SchedulerService`: `ListDueEvents`, `AckEvent`, `EnqueueTask`, `GetStatus` | `solutions/ha-bridge` (executor, `cmd/ha-control`), `solutions/voice-proxy` |

Пакеты project-agnostic: внутри нет endpoint'ов, entity/script id, имён
очередей, task types и deployment-деталей — всё это передаёт вызывающий код.
Контракт scheduler'а — `packages/proto/ricardo/v1/scheduler.proto`.
