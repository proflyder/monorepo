# ricardo.v1

Канонический protobuf-контракт. Источник истины для Go-сервера, будущих
Rust/mobile клиентов и MCP-адаптера.

## Транспорт

- канонический API — gRPC/protobuf;
- сервер `solutions/alarm` реализует `AlarmService`;
- для браузера/HA используется gRPC-Web/Connect на том же контракте, без
  отдельной REST-модели;
- MCP остаётся отдельной внешней поверхностью для Hermes.

## Важно

`next_run_unix` и `snooze_until_unix` — UTC epoch seconds. Локальное время
будильника (`hour`, `minute`, `timezone`, `repeat`) хранится отдельно;
летнее время и смена таймзоны пересчитываются планировщиком.
