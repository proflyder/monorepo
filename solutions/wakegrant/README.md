# wakegrant

One-time logon grants for a machine woken by Home Assistant. The contract is
[`packages/contracts/wakegrant/PROTOCOL.md`](../../packages/contracts/wakegrant/PROTOCOL.md);
this directory implements v1 exactly and pins its test vector.

```text
HA switch.komp.turn_on
  → POST /v1/grants  (issuer bearer)          grant in SQLite (PVC)
  → Wake-on-LAN
PC logon component
  → POST /v1/challenge                        single-use nonce, 60 s
  → POST /v1/claim (HMAC)                     one grant consumed, atomically
```

Every error is fail-closed: no grant or a failed MAC means the normal sign-in
screen. Both machine messages are authenticated with HMAC-SHA256 over a server
nonce, so nothing secret travels over the LAN.

## API

| method | path | auth | result |
|---|---|---|---|
| `POST` | `/v1/grants` | `Authorization: Bearer <issuer_token>` | `201 {"grantId","expiresAt"}` |
| `POST` | `/v1/challenge` | none | `200 {"nonce","expiresIn":60}` |
| `POST` | `/v1/claim` | HMAC in body | `200 {"granted","grantId?","mac"}` |
| `GET` | `/healthz` | none | `200 ok` |

`ttlSeconds` is 30..600 (default 180, capped by `WAKE_GRANT_MAX_TTL_SECONDS`).
A new grant for the same machine replaces the previous unconsumed one: only the
newest is valid. Challenge rate limit is 30/min per machine and per source IP,
answered as `429`.

The service is transport-agnostic: run it behind HTTPS or a private network
without protocol changes. It contains no project-specific endpoint or secret.

## Configuration

All values come from the environment; secrets are files only.

| variable | default | meaning |
|---|---|---|
| `WAKE_GRANT_LISTEN` | `127.0.0.1:8099` | HTTP listen address |
| `WAKE_GRANT_DB_PATH` | `wakegrant.db` | SQLite file (PVC in the home deployment) |
| `WAKE_GRANT_ISSUER_TOKEN_FILE` | — (required) | issuer bearer token |
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
- `claim` consumes the nonce **before** verifying the MAC, as the protocol
  orders it, so a forged request burns the nonce and replays always fail.
- Concurrent claims for the same machine race on a single `UPDATE ... RETURNING`
  statement; exactly one wins.
- Structured JSON logs contain only `machine`, `outcome` and `grantId`. Nonces,
  MACs, tokens and keys are never logged.

## Tests

```bash
go test -race ./solutions/wakegrant/...
bazel test //solutions/wakegrant/...
```

Covered: the PROTOCOL.md test vector, full issue → challenge → claim flow,
restart between grant and claim, nonce replay, wrong machine, expired grant,
expired nonce, concurrent double claim, newest-grant-wins, bad bearer, bad MAC,
rate limit, malformed JSON, TTL bounds and machines-file parsing.

Deployment lives in
[`projects/home/kubernetes/wakegrant`](../../projects/home/kubernetes/wakegrant).
