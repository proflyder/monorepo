# home.v1 protobuf

- `scheduler.proto` — `SchedulerService`: schedules, timers,
  `ListDueEvents`/`AckEvent`, `EnqueueTask`/`CancelTask`, `GetStatus`.
- `scenario.proto` — `ScenarioService`: определения сценариев и `RunScenario`
  в очередь `scenario`; исполняет `ha-scenario`, не scheduler.
- `alarms.proto` — legacy `AlarmService`, wire-совместимый facade над
  schedules (`SCHEDULE_KIND_ALARM`), не второй планировщик.
- `intercom.proto` — `IntercomService`: сигналинг WebRTC-звонка
  (`solutions/intercom`), медиа не несёт.
- `text.proto` — `LocalizedText` (`key` + `args`): текст сервера как
  стабильный ключ, который клиент переводит своими ресурсами.

Это внешний контракт: имена полей, номера и enum-значения заморожены, правки
только additive. Транспорт и семантика — [`docs/GRPC.md`](../../../../docs/GRPC.md).

## Локализация ошибок

Connect-ошибка несёт `message` и detail `home.v1.LocalizedText`. Android
переводит ключ через `strings.xml`, затем откатывается на `message`, затем на
общий текст. В `key` сервер никогда не кладёт человекочитаемую фразу.

## Сгенерированные типы

Коммитятся только Rust-стабы (`packages/rust/home-proto/src/generated`,
перегенерация — `generate.sh`). Android использует kotlinx.serialization DTO
(`projects/mobile/android/core/network/impl/.../WireDtos.kt`). `proto_library`
в `BUILD.bazel` — схема для descriptor tooling.

## Заметки к коду

- `ListSchedules` с `SCHEDULE_KIND_UNSPECIFIED` не возвращает строки
  `SCHEDULE_KIND_SCENARIO`: их видно только при явном запросе этого kind.
- `RunScenario` разрешён и для выключенного сценария: `enabled` управляет
  только автоматическими time-триггерами.
- `GetStatus.ready` истинно только при недавнем успешном tick; устаревший
  `last_tick_unix` не маскирует падающий tick.
- Тела SDP в intercom передаются без изменений и не логируются; пустой список
  `GetIceConfig` — только host-кандидаты (LAN/VPN).
