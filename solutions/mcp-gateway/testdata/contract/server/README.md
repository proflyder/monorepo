# MCP server contract fixtures

Format and semantics: `packages/rust/home-contracttest`. The files are replayed
by the Rust test `server_contract_fixtures` (`crates/mcp-gateway/tests/contract.rs`)
through the real HTTP layer (`/mcp`, auth) and fake backends. The Go production
tests of the same name were deleted with the Go implementation; these files are
frozen data.

- `auth_rejection`, `/healthz`, `/readyz`, `/metrics` answers are byte-exact.
- MCP frames are compared semantically: tool names, descriptions,
  `inputSchema`, `readOnlyHint`/`destructiveHint`, `speech` and `details` are
  fixed, while Go-SDK-only `ttlMs`, `cacheScope`, `structuredContent` and
  `idempotentHint:false` are optional.

The reference byte snapshot of the former Go SDK output is kept in
`../../reference/mcp_tools_list_go_snapshot.json`.
