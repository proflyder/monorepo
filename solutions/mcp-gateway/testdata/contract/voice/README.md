# Voice contract fixtures

Replayed by `voice_contract_fixtures` (`crates/mcp-gateway/tests/voice.rs`)
through `/v1/voice/*` behind the separate voice token, with a fake bb
dispatcher and a deterministic clock.

Response bodies are compared structurally (`exact`): the voice API is the
project's own JSON, not MCP SDK frames. `speech` texts and `completedSeq` are
part of the contract — `completedSeq` is the voice-proxy reply-queue
idempotency key.
