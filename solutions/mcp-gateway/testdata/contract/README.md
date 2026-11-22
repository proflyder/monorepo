# Contract fixtures (frozen)

HTTP golden fixtures replayed against the gateway by
`crates/mcp-gateway/tests/contract.rs` and `tests/voice.rs` through
[`packages/rust/home-contracttest`](../../../../packages/rust/home-contracttest/README.md).

| Directory | Contents |
|---|---|
| `server/` | MCP transport and bearer auth (`/mcp`, `/healthz`, `/readyz`, `/metrics`) |
| `tools/` | Every MCP tool group and the `infra://catalog` resource |
| `voice/` | `/v1/voice/*` routes |

These files are data, not tests: there is no generator. A behaviour change is a
deliberate fixture edit with the reason recorded in the PR. The byte-level
reference snapshot of `tools/list` is `../reference/mcp_tools_list_go_snapshot.json`.

MCP frames are compared semantically (`compare: semantic`): `ttlMs`,
`cacheScope`, `structuredContent` and `idempotentHint:false` are optional.
Auth (`auth_rejection`), `/healthz`, `/readyz`, `/metrics` and SSE framing
stay byte-exact.
