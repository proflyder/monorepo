# wakegrant protocol v1

One-time "logon grants" for a machine woken by Home Assistant. A grant lets the
machine's logon component sign the owner in **once** within a short TTL. Every
error is fail-closed: no grant, normal sign-in screen.

Roles:

- **issuer** — Home Assistant (bearer token `issuer_token`), right before it
  sends the Wake-on-LAN packet;
- **machine** — the logon component on the PC, identified by `machine`
  (`^[a-z0-9-]{1,32}$`) and a 32-byte per-machine key `K` shared with the
  service.

Transport: HTTP/1.1 + JSON (UTF-8). Both directions of the machine exchange
are authenticated with HMAC-SHA256 over a server nonce, so the protocol is
safe over plain HTTP on the LAN (no secret is ever transmitted; a replayed or
forged message is rejected). HTTPS may be added in front without changes.

Encoding: `b64` = standard base64 with padding. `hmac(K, s)` =
HMAC-SHA256 with key `K` over the UTF-8 bytes of `s`, output `b64`.
Fields are joined with `\n` (0x0A), no trailing newline.

## POST /v1/grants (issuer)

Header `Authorization: Bearer <issuer_token>`.

Request `{"machine":"komp","ttlSeconds":180}` (`ttlSeconds` 30..600,
default 180). Creates a grant; an existing unconsumed grant for the same
machine is replaced (only the newest is valid).

Response 201 `{"grantId":"<uuid>","expiresAt":"<RFC3339 UTC>"}`.
Errors: 401 bad token, 400 bad body/unknown machine.

## POST /v1/challenge (machine)

Request `{"machine":"komp"}`. Response 200
`{"nonce":"<b64 of 32 random bytes>","expiresIn":60}`.
A nonce is single-use, bound to `machine`, valid 60 s. Unknown machine → 400.
Rate limit: 60 challenges/min per machine and per source IP → 429.

## POST /v1/claim (machine)

Request:

```json
{"machine":"komp","nonce":"<b64>","mac":"<hmac(K, \"claim\\n\" + machine + \"\\n\" + nonce)>"}
```

Server: verify nonce exists, unexpired, unused and bound to `machine`; mark it
used; verify `mac` (constant time). Then atomically consume the newest grant
`WHERE machine=? AND consumed=0 AND expires_at>now`.

Response 200 (always 200 for a well-formed authenticated request):

```json
{"granted":true,"grantId":"<uuid>","mac":"<hmac(K, \"grant\\n\" + machine + \"\\n\" + nonce + \"\\n\" + \"1\" + \"\\n\" + grantId)>"}
{"granted":false,"mac":"<hmac(K, \"grant\\n\" + machine + \"\\n\" + nonce + \"\\n\" + \"0\" + \"\\n\")>"}
```

The machine MUST verify the response `mac` with its own nonce before acting;
an invalid or missing `mac` is treated as `granted:false`.
Errors (no mac): 400 malformed, 401 bad mac / bad or reused nonce, 429.

## GET /healthz

`200 ok`.

## Test vector

`K` = bytes 0x00..0x1f, `machine` = `komp`,
`nonce` = `AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=` (bytes 0x00..0x1f),
`grantId` = `00000000-0000-4000-8000-000000000001`.
Expected values (both implementations MUST pin them in tests):

| message | mac |
|---|---|
| claim | `+YedSn/aeRZqRNKKzkUS8Dy2aq/dVLkXbh9iX91L9XU=` |
| grant, granted | `UAADAs7pUxn/EN42woIH5K0qrH2YB+bd1SYZVKP//FI=` |
| grant, denied | `I6ClmYKmmfO6wodeKXOobAtJj/SgQ7C/ieNumDYrj1Y=` |
