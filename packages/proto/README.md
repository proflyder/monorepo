# packages/proto

Общие protobuf-контракты. Канонический API — `home/v1/scheduler.proto`
(`home.v1.SchedulerService`) и `home/v1/alarms.proto`
(`home.v1.AlarmService`). Описание пакета — [`home/v1/README.md`](home/v1/README.md);
транспорт (Connect JSON/binary и gRPC-Web по HTTP/1.1; gRPC/h2c не отдаётся) —
[`docs/GRPC.md`](../../docs/GRPC.md).

```bash
bazel build //packages/proto/...
```
