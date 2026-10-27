# packages/proto

Общие контракты между Go, Rust, mobile и будущими клиентами.

Первый контракт:

```text
ricardo/v1/alarms.proto   AlarmService
```

Сборка:

```bash
bazel build //packages/proto/...
```

`go_grpc_library` генерирует Go + gRPC stubs из protobuf. Канонический API —
gRPC/protobuf; для HA и браузера используем Connect/gRPC-Web на том же
контракте. Детали — [`docs/GRPC.md`](../../docs/GRPC.md).
