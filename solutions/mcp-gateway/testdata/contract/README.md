# Contract fixtures (frozen)

HTTP golden fixtures captured from the former Go implementation and replayed
against the Rust gateway by `crates/mcp-gateway/tests/contract.rs` and
`tests/voice.rs` through `packages/rust/home-contracttest`.

The directory split mirrors the former Go test packages:

| Directory | Contents |
|---|---|
| `server/` | MCP transport and bearer auth (`/mcp`, `/healthz`, `/readyz`, `/metrics`) |
| `tools/` | Every MCP tool group and the `infra://catalog` resource |
| `voice/` | `/v1/voice/*` routes |

These files are **data, not tests**: the Go generators that produced them were
deleted with the Go implementation. Do not regenerate them; a behaviour change
is a deliberate fixture edit with the reason recorded in the PR. The format is
described in `packages/rust/home-contracttest` and the byte-level Go snapshot is
kept for reference in `../reference/mcp_tools_list_go_snapshot.json`.

MCP frames are compared semantically (`compare: semantic`): `ttlMs`,
`cacheScope`, `structuredContent` and `idempotentHint:false` are optional Go SDK
fields and are not part of the expected subset. Auth (`auth_rejection`),
`/healthz`, `/readyz`, `/metrics` and SSE framing stay byte-exact.

`crates/mcp-gateway/tests/deploy_contract.rs` additionally loads the tracked
deployment catalog and bb allow-list with the real validators; the fixture
directories above are unrelated to that deploy data.
