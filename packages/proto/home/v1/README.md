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

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `intercom.proto`
  - IntercomService is the signaling contract for a WebRTC call between a phone and a station. The service does not carry media: audio flows peer-to-peer over UDP (DTLS/SRTP), while this service only registers endpoints, tracks the call state machine and relays …
  - The SDP body is passed through untouched. It is never logged.
  - GetIceConfig returns STUN/TURN servers from the service configuration. An empty list means host candidates only (the LAN/VPN case); adding TURN later must not require a client change.
- `scenario.proto`
  - ScenarioService is the durable scenario-definition contract. Scenario definitions belong to the scheduler because it is the only owner of live schedules: a time trigger becomes a durable SCHEDULE_KIND_SCENARIO row, and firing it emits a DueEvent on the …
  - Optional idempotency key. Replaying the same key with an identical request returns the original scenario; a different payload is rejected with ALREADY_EXISTS.
  - Optional idempotency key for a manual run. An empty key always queues a new occurrence. A disabled scenario may still be run manually: `enabled` controls automatic time triggers, not explicit user action.
- `scheduler.proto`
  - SchedulerService is the durable schedule/timer/event contract. Transport: the server exposes this service over Connect (unary JSON/HTTP), gRPC (HTTP/2, including h2c) and gRPC-Web on the same listener.
  - Deprecated human-readable skip/failure reason, never contains secrets. New clients render `detail_text` instead.
  - queue routes an event to one executor group. Convenience alarm/reminder/ timer events use "home"; generic tasks default to "tasks". Executors lease only their own queue and never steal unknown queues.
  - Optional idempotency key. Replaying the same key with an identical request returns the original schedule; a different payload is rejected with ALREADY_EXISTS.
  - UNSPECIFIED excludes SCHEDULE_KIND_SCENARIO so alarm/reminder clients never render scenario trigger rows as schedules. Only an explicit SCHEDULE_KIND_SCENARIO query returns them.
  - ready is true only when the last tick succeeded recently. A failing tick is never masked by a stale last_tick_unix.
- `text.proto`
  - LocalizedText is a user-visible text as a stable string key plus typed arguments. Clients resolve the key through their own resources (strings.xml on Android); the server never sends a human-readable sentence in this field.
