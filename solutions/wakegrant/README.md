# wakegrant

One-time grants for a machine controlled by Home Assistant. The contract is
[`packages/contracts/wakegrant/PROTOCOL.md`](../../packages/contracts/wakegrant/PROTOCOL.md);
this directory implements both the v1 (no `purpose`) wire format and the
purpose-aware extension, and pins every test vector.

```text
HA switch.komp.turn_on
  → POST /v1/grants  {"purpose":"logon"}      grant in SQLite (PVC)
  → Wake-on-LAN
PC logon component
  → POST /v1/challenge                        single-use nonce, 60 s
  → POST /v1/claim (HMAC)                     one logon grant consumed

HA «разблокируй комп»
  → POST /v1/grants  {"purpose":"unlock","ttlSeconds":60}
PC unlock component
  → POST /v1/claim {"purpose":"unlock","waitSeconds":20}   long-poll
```

Every error is fail-closed: no grant or a failed MAC means the normal
sign-in/lock screen. Both machine messages are authenticated with HMAC-SHA256
over a server nonce, so nothing secret travels over the LAN.

`logon` and `unlock` are separate: a grant is only ever consumed by a claim of
the same purpose, and a new grant replaces the previous one only within
`(machine, purpose)`.

## API

| method | path | auth | result |
|---|---|---|---|
| `POST` | `/v1/grants` | `Authorization: Bearer <issuer_token>` | `201 {"grantId","expiresAt"}` |
| `POST` | `/v1/challenge` | none | `200 {"nonce","expiresIn":60}` |
| `POST` | `/v1/claim` | HMAC in body | `200 {"granted","grantId?","mac"}` |
| `GET` | `/healthz` | none | `200 ok` |

`purpose` on `/v1/grants` is `logon` (default when omitted) or `unlock`.
`ttlSeconds` is 30..600 (default 180, capped by `WAKE_GRANT_MAX_TTL_SECONDS`).

On `/v1/claim`, omitting `purpose` keeps the v1 format byte for byte (logon).
An explicit `purpose` appends it as the last MAC line, and `waitSeconds`
(1..25) turns the claim into a long-poll for that purpose. At most 2 waits per
machine may be outstanding; the next one gets `429`.

Challenge rate limit is 60/min per machine and per source IP, answered as
`429`; each long-poll claim counts once, so one wait per ~25 s fits easily.

The service is transport-agnostic: run it behind HTTPS or a private network
without protocol changes. It contains no project-specific endpoint or secret.

## Configuration

All values come from the environment; secrets are files only.

| variable | default | meaning |
|---|---|---|
| `WAKE_GRANT_LISTEN` | `127.0.0.1:8099` | HTTP listen address |
| `WAKE_GRANT_DB_PATH` | `wakegrant.db` | SQLite file (PVC in the home deployment) |
| `WAKE_GRANT_ISSUER_TOKEN_FILE` | — (required) | issuer bearer token |
| `WAKE_GRANT_ISSUER_TOKEN` | — | direct token value (wins over `_FILE`) |
| `WAKE_GRANT_MACHINES_FILE` | — (required) | machine → key map |
| `WAKE_GRANT_MAX_TTL_SECONDS` | `600` | operator cap inside 30..600 |
| `LOG_LEVEL` | `info` | `debug`, `info`, `warn`, `error` |

The machines file is a mounted Secret, either a JSON object
`{"komp":"<base64 32-byte key>"}` or `machine=base64key` lines. Every key must
decode to exactly 32 bytes; otherwise the process refuses to start.

## Behaviour notes

- The grant lives in SQLite (`WAL`, one connection) so a pod restart between
  Wake-on-LAN and the claim does not lose it. Nonces are in memory: a restart
  only forces the machine to request a fresh challenge.
- Schema version 2 adds `grants.purpose`; migration defaults every existing row
  to `logon`, which is exactly what a v1 grant meant.
- `claim` consumes the nonce **before** verifying the MAC, as the protocol
  orders it, so a forged request burns the nonce and replays always fail.
- Concurrent claims for the same `(machine, purpose)` race on a single
  `UPDATE ... RETURNING` statement; exactly one wins.
- Long-poll waiting is per `(machine, purpose)` and capped at 2 waits per
  machine. A grant creation broadcasts to that queue after the SQLite commit;
  the waiter re-reads the store, so a grant created between the first lookup
  and registering cannot be missed. Server shutdown or client disconnect ends
  the wait with no MAC (fail-closed).
- The HTTP `WriteTimeout` is 40 s so the 25 s long-poll is never cut off.
- Structured JSON logs contain only `machine`, `outcome` and `grantId`. Nonces,
  MACs, tokens and keys are never logged.

## Implementation

Rust, decision 41 pilot. Crates live in `crates/wakegrant`:

| module | contents |
|---|---|
| `domain` | purposes, TTL/wait rules, nonce store, rate limiter, long-poll waiters; pure logic, time via the `Clock` trait |
| `store` | SQL over `home-sqlite` (single writer, `UPDATE … RETURNING` consumption) |
| `api` | axum routes, bearer via `home-http`, long-poll on `tokio::sync::Notify`, 4 KiB body limit, 40 s request timeout |
| `config` | `clap`/`home-config`; the same environment names as the Go service |
| `app` | composition root, `Supervisor` and the periodic grant cleanup |

The binary is a static `x86_64-unknown-linux-musl` executable (zig C toolchain)
on the same distroless static base, user and entrypoint as before. `-healthcheck`
is preserved for the Deployment probe.

The schema is Go version 2 (`schema_meta`): an existing PVC database opens in
place and no table is recreated.

## Tests

```bash
cargo test -p wakegrant
bazel test //solutions/wakegrant/... //packages/rust/...
```

Covered: both PROTOCOL.md test vectors and all four golden HTTP fixtures
(unchanged from the Go implementation), full issue → challenge → claim flow,
store newest-grant/purpose isolation, an existing Go v2 database, nonce replay
and expiry, rate limiting, waiter cap, domain TTL/wait bounds and machines-file
parsing. The fixtures live in `testdata/contract/` and are replayed by
`home-contracttest`; there is no `-update` mode on the Rust side.

Deployment lives in
[`projects/home/kubernetes/wakegrant`](../../projects/home/kubernetes/wakegrant).
