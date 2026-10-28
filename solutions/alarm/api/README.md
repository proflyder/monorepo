# alarm API

Канонический контракт — `packages/proto/ricardo/v1/scheduler.proto`
(`SchedulerService`) и legacy `alarms.proto` (`AlarmService`).

Оба сервиса отдаются одним сервером через Connect JSON, gRPC (HTTP/2, h2c) и
gRPC-Web. `AlarmService` — facade над теми же durable schedules; звук
исполняет внешний executor, сервис лишь фиксирует события и leases.
