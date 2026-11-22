# packages

Общие библиотеки и контракты, не принадлежащие одному solution.

```text
packages/proto/      protobuf/gRPC contracts
packages/contracts/  межъязыковые протоколы (wakegrant)
packages/rust/       shared Rust packages
packages/kotlin/     shared Kotlin packages (Android)
```

Правило: код, нужный только одному solution, живёт в его собственном крейте/пакете.
В `packages/` выносим только то, что используется несколькими solutions
или projects.

## Заметки к коду

Пояснения к коду `packages/kotlin/core` (в исходниках комментариев нет, см. `AGENTS.md`).

- `endpoint/ServerEndpoint.kt` — базовый адрес Connect-листенера сервера.
  Принимаются только `http(s)://host[:port][/prefix]` (IPv4, IPv6 в скобках,
  DNS-имя); голый host отклоняется, чтобы не угадывать транспорт. Учётные
  данные, query и fragment в URL запрещены: bearer-токен — отдельный секрет.
- `nav/PendingTransitions.kt` — очередь навигационных команд, пришедших,
  пока FragmentManager не может безопасно сделать commit (state saved или
  host не resumed). `TabRouteStack.push` не добавляет маршрут, уже текущий,
  чтобы не создавать вторую транзакцию.
- `operation/SuspendRunCatching.kt` — `runCatching` для suspend-блоков,
  который пробрасывает `CancellationException`: иначе работа продолжалась бы
  после смерти scope.
- `text/SpeakText.kt` — локальная проверка текста для колонки (кнопка
  «Сказать» и конструктор сценариев); сервер всё равно перепроверяет.
