# Tool contract fixtures

Формат и семантика — `packages/go/contracttest/README.md`. Файлы здесь
проигрываются Go-тестом `TestToolContractFixtures` через настоящий HTTP-слой
(`/mcp`, auth) и фейковые бэкенды из `contract_test.go`; тот же набор обязан
воспроизвести Rust-раннер `packages/rust/home-contracttest`.

Особенности:

- `compare: semantic` — кадры MCP сравниваются по смыслу: объекты как
  подмножества, массивы точные. Поэтому имена инструментов, описания,
  `inputSchema`, аннотации `readOnlyHint`/`destructiveHint`, `speech` и
  `details` зафиксированы, а Go-специфичные `ttlMs`, `cacheScope`,
  `structuredContent` и `idempotentHint:false` не обязательны (нормализатор
  `dropSDKFields` убирает их и при `-update`).
- `format: sse` — обрамление `event: message\ndata: …\n\n` проверяется
  побайтно, JSON-payload сравнивается по `compare`.
- Один файл — один живой сценарий: шаги идут в одной MCP-сессии, а
  `WithBeforeStep` меняет фейковый мир между вызовами (алерты, сигналы,
  ошибки, режим агента, часы).

Справочный байтовый снимок Go SDK — `../../../../testdata/reference/`.

Перегенерация: `go test ./solutions/mcp-gateway/internal/tools -run TestToolContractFixtures -update`.
