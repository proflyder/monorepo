# packages/go

Общие Go-пакеты. Пакет добавляется сюда, когда у него появляется второй
потребитель (решение 35).

| Пакет | Назначение | Потребители |
|---|---|---|
| `contracttest` | golden-раннер HTTP-контрактов: JSON-фикстуры, `capture`/`{{name}}`, плейсхолдеры, структурное и побайтное сравнение, `-update` | тесты `solutions/mcp-gateway` |

Go-клиенты `httpapi`, `ha` и `scheduler` удалены вместе с Go-реализациями
scheduler и ha-bridge/ha-control. Единственный delivery-контракт расписаний —
`home.v1` (`packages/proto/home/v1`), клиенты на нём — Rust `home-proto`,
Android и HA.

Пакеты project-agnostic: внутри нет endpoint'ов, entity/script id, имён
очередей, task types и deployment-деталей — всё это передаёт вызывающий код.
