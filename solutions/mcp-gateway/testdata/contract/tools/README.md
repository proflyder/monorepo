# Tool contract fixtures

Format: `packages/rust/home-contracttest`. The files are replayed by the Rust
test `tool_contract_fixtures` (`crates/mcp-gateway/tests/contract.rs`) through
the real HTTP layer (`/mcp`, auth) and fake backends. The former Go
`TestToolContractFixtures` generator was deleted with the Go implementation;
these files are frozen data.

- `compare: semantic` — frames are compared as subsets (objects) with exact
  arrays, so tool names, descriptions, `inputSchema`, `readOnlyHint` /
  `destructiveHint`, `speech` and `details` are fixed. Go-SDK-only `ttlMs`,
  `cacheScope`, `structuredContent` and `idempotentHint:false` are optional.
- `format: sse` — `event: message\ndata: …\n\n` framing is byte-exact, the
  JSON payload uses `compare`.
- One file is one live scenario: steps share an MCP session and the test's
  `with_before_step` hook mutates the fake world between calls (alerts, signals,
  errors, agent mode, clock).
