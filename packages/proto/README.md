# packages/proto

Общие protobuf-контракты для Go, Kotlin и HA-клиентов. Канонический API —
`ricardo/v1/scheduler.proto` (`SchedulerService`), legacy —
`ricardo/v1/alarms.proto` (`AlarmService`). Описание пакета и checked-in
stubs — [`ricardo/v1/README.md`](ricardo/v1/README.md); транспорт
(Connect/gRPC/gRPC-Web) — [`docs/GRPC.md`](../../docs/GRPC.md).

```bash
bazel build //packages/proto/...
```
