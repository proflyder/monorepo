# Voice contract fixtures

Формат — `packages/go/contracttest/README.md`. Файлы проигрываются
`TestVoiceContractFixtures` через `/v1/voice/*` за отдельным voice-токеном, с
фейковым диспетчером bb и детерминированными часами.

Тела ответов сравниваются структурно (`exact`), потому что голосовой API —
это собственный JSON-контракт `voice-contract`, а не кадры MCP SDK. Тексты
(`speech`) и `completedSeq` входят в контракт: `completedSeq` — ключ
идемпотентности очереди ответов voice-proxy.

Перегенерация: `go test ./solutions/mcp-gateway/internal/voice -run TestVoiceContractFixtures -update`.
