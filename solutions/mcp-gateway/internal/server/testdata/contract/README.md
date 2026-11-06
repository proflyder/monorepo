# Contract fixtures

Формат фикстур и семантика `capture`, `{{name}}`, плейсхолдеров, режима
`compare: semantic` и `response.format: sse` описаны в
`../../../../../../packages/go/contracttest/README.md`.

MCP-кадры сравниваются семантически: `ttlMs`, `cacheScope`,
`structuredContent` и `idempotentHint:false` — необязательные поля Go SDK и не
входят в ожидаемое подмножество. Байтовыми остаются auth (`auth_rejection`),
`/healthz`, `/readyz`, `/metrics` и SSE-обрамление.

Перегенерация: `go test ./<пакет> -run TestContractFixtures -update`.
