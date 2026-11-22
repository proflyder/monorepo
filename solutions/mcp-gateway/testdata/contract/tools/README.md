# Tool contract fixtures

Replayed by `tool_contract_fixtures` (`crates/mcp-gateway/tests/contract.rs`)
through the real HTTP layer (`/mcp`, auth) and fake backends. Comparison rules
are in [`../README.md`](../README.md); `format: sse` keeps
`event: message\ndata: …\n\n` framing byte-exact while the JSON payload uses
`compare`.

One file is one live scenario: steps share an MCP session and the test's
`with_before_step` hook mutates the fake world between calls (alerts, signals,
errors, agent mode, clock).
