# alarm API

Канонический контракт — protobuf/gRPC в `packages/proto/ricardo/v1/`.
Первая поверхность — `AlarmService`; REST-first схемы не будет.

Для HA и браузера тот же сервис будет доступен через Connect/gRPC-Web.
MCP — отдельный adapter в `solutions/mcp-alarm`.
