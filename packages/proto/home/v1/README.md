# home.v1 protobuf

Канонические контракты домашнего планировщика:

- `scheduler.proto` — `SchedulerService`: durable schedules, timers,
  `ListDueEvents`/`AckEvent`, generic `EnqueueTask`/`CancelTask`, `GetStatus`.
  Основной контракт.
- `scenario.proto` — `ScenarioService`: durable определения сценариев
  (триггеры/условия/шаги), `RunScenario` в очередь `scenario`. Исполнение —
  отдельный executor, не этот сервис.
- `alarms.proto` — legacy `AlarmService`, wire-совместимый facade над теми же
  schedules (`kind=SCHEDULE_KIND_ALARM`). Не второй планировщик.
- `text.proto` — `LocalizedText` (`key` + typed `args`): пользовательский текст
  сервера как стабильный ключ, который клиент переводит своими ресурсами.
  Старые текстовые поля остаются для совместимости и помечены deprecated.

`package home.v1`. Это внешний контракт: имена полей, номера и enum-значения
заморожены. JSON-форма — стандартный protobuf mapping (lowerCamelCase,
`int64` как decimal string, enum по имени), поэтому переименование пакета не
меняет тела запросов и ответов — только путь
`/home.v1.SchedulerService/<Method>`.

## Транспорт

Один listener отдаёт Connect JSON/binary, gRPC-Web (HTTP/1.1) и, когда это
включено в конкретной реализации, gRPC (HTTP/2, h2c). REST-фасада нет.
Семантика — `docs/GRPC.md`.

## Локализация ошибок

Connect-ошибка несёт legacy `message` и, для новых клиентов, detail
`home.v1.LocalizedText`: `type` detail и JSON `debug` с `key`/`args`. Android
сначала переводит ключ через `strings.xml`, затем откатывается на `message`,
затем на общий текст. Сервер не кладёт в `key` человекочитаемую фразу.

## Сгенерированные типы

В репозитории коммитятся только Rust-стабы
(`packages/rust/home-proto/src/generated`), их перегенерация —
`packages/rust/home-proto/generate.sh`. Android-клиент использует
типизированные kotlinx.serialization DTO
(`projects/mobile/android/core/network/src/main/kotlin/dev/proflyder/home/core/network/WireDtos.kt`). Go-стабы удалены вместе
с Go-сервером и его клиентами.

Bazel собирает `proto_library` как схему для descriptor tooling и других
языков.
