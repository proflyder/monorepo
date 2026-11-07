# Voice contract fixtures

Format: `packages/rust/home-contracttest`. The files are replayed by the Rust
test `voice_contract_fixtures` (`crates/mcp-gateway/tests/voice.rs`) through
`/v1/voice/*` behind the separate voice token, with a fake bb dispatcher and a
deterministic clock. The former Go generator was deleted; these files are frozen
data.

Response bodies are compared structurally (`exact`): the voice API is the
project's own `voice-contract` JSON, not MCP SDK frames. Texts (`speech`) and
`completedSeq` are part of the contract — `completedSeq` is the voice-proxy
reply-queue idempotency key.
