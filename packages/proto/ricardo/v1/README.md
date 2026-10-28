# ricardo.v1 protobuf

Канонические контракты:

- `scheduler.proto` — `SchedulerService`: durable schedules, timers,
  `ListDueEvents`/`AckEvent`, generic `EnqueueTask`/`CancelTask`, `GetStatus`.
  Основной контракт домашнего планировщика.
- `alarms.proto` — legacy `AlarmService`, wire-совместимый facade над теми же
  schedules (`kind=SCHEDULE_KIND_ALARM`). Не второй планировщик.

`package ricardo.v1`, `go_package` →
`ricardo.local/monorepo/packages/proto/ricardo/v1`.

## Транспорт

Один listener отдаёт Connect JSON/binary, gRPC (HTTP/2, h2c) и gRPC-Web через
connect-go handlers. JSON — стандартный protobuf mapping: lowerCamelCase,
`int64` как decimal string, enum по имени. REST-фасада нет. Детали и
семантика — `docs/GRPC.md`.

## Сгенерированный Go

`.pb.go` и `.connect.go` лежат в этом каталоге и коммитятся. Это позволяет
`go test ./packages/... ./solutions/...` без Bazel/protoc. Перегенерация:

```bash
PATH=secrets/local-tools/protoc/bin:secrets/local-tools/bin:$PATH \
  solutions/scheduler/scripts/generate-proto.sh
```

Pinned: protoc 33.4, protoc-gen-go v1.36.x, protoc-gen-connect-go v1.21.x.

Bazel собирает checked-in файлы через `go_library`; `proto_library` остаётся
схемой для других языков/descriptor tooling. `go_proto_library` для этих
файлов не используется, чтобы не генерировать те же Go-типы повторно.
Публичные aliases `alarms_go_proto` и `scheduler_go_proto` указывают на
`//packages/proto/ricardo/v1:ricardo`.
