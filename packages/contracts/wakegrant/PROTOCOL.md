# wakegrant protocol

One-time grants for a machine controlled by Home Assistant. A grant lets the
machine's component perform **one** action within a short TTL. There are two
purposes:

- `logon` — sign the owner in on a **cold boot**, right after Wake-on-LAN;
- `unlock` — unlock the workstation of a PC that is already running but locked.

A logon grant must never unlock a running session, and an unlock grant must
never be used for logon: the two purposes are stored, claimed and authenticated
separately. Every error is fail-closed: no grant, normal sign-in/lock screen.

Roles:

- **issuer** — Home Assistant (bearer token `issuer_token`), before it sends
  Wake-on-LAN or when asked to unlock the running PC;
- **machine** — the Windows component on the PC, identified by `machine`
  (`^[a-z0-9-]{1,32}$`) and a 32-byte per-machine key `K` shared with the
  service.

Transport: HTTP/1.1 + JSON (UTF-8). Both directions of the machine exchange
are authenticated with HMAC-SHA256 over a server nonce, so the protocol is
safe over plain HTTP on the LAN (no secret is ever transmitted; a replayed or
forged message is rejected). HTTPS may be added in front without changes.

Encoding: `b64` = standard base64 with padding. `hmac(K, s)` =
HMAC-SHA256 with key `K` over the UTF-8 bytes of `s`, output `b64`.
Fields are joined with `\n` (0x0A), no trailing newline.

## Compatibility with v1

The original v1 protocol had no `purpose` field and always meant `logon`.
A request that **omits** `purpose` keeps the v1 wire format byte for byte:
it selects the `logon` grant and uses the v1 claim/grant MAC messages below.
An explicit `"purpose":"logon"` (or `"unlock"`) selects the purpose-aware
messages, which append the purpose as a final line. This lets the already
installed v1 Windows provider keep working while the unlock feature is added.

Long-poll (`waitSeconds`) is only defined together with an explicit `purpose`;
a request without `purpose` is never delayed.

## POST /v1/grants (issuer)

Header `Authorization: Bearer <issuer_token>`.

Request `{"machine":"komp","purpose":"logon","ttlSeconds":180}`:

- `purpose` — `logon` (default when omitted) or `unlock`;
- `ttlSeconds` — 30..600, default 180.

Creates a grant. An existing unconsumed grant for the same
`(machine, purpose)` is replaced; a grant of the other purpose is untouched.
So a pending logon grant does not disappear when an unlock grant is issued.

Response 201 `{"grantId":"<uuid>","expiresAt":"<RFC3339 UTC>"}`.
Errors: 401 bad token, 400 bad body/unknown machine/unknown purpose.

## POST /v1/challenge (machine)

Request `{"machine":"komp"}`. Response 200
`{"nonce":"<b64 of 32 random bytes>","expiresIn":60}`.
A nonce is single-use, bound to `machine`, valid 60 s. Unknown machine → 400.
Rate limit: 60 challenges/min per machine and per source IP → 429.

## POST /v1/claim (machine)

### v1 form (no `purpose`)

Request:

```json
{"machine":"komp","nonce":"<b64>","mac":"<hmac(K, \"claim\\n\" + machine + \"\\n\" + nonce)>"}
```

Claim MAC message: `claim\n<machine>\n<nonce>`.

### Purpose-aware form

```json
{"machine":"komp","nonce":"<b64>","purpose":"unlock",
 "mac":"<hmac(K, \"claim\\n\" + machine + \"\\n\" + nonce + \"\\n\" + purpose)>",
 "waitSeconds":20}
```

Claim MAC message: `claim\n<machine>\n<nonce>\n<purpose>`.

- `purpose` — `logon` or `unlock`; selects which grant may be consumed.
- `waitSeconds` — optional long-poll, 1..25, **only with `purpose`**. If no
  grant of that purpose exists yet, the server holds the request until one is
  issued or the wait elapses, then answers normally. At most 2 concurrent
  waits per machine are allowed; a third wait is answered `429` (see below).

Server: verify nonce exists, unexpired, unused and bound to `machine`; mark it
used; verify `mac` (constant time). Then atomically consume the newest grant
`WHERE machine=? AND purpose=? AND consumed=0 AND expires_at>now`. The nonce is
consumed at the start of the request, before any waiting, exactly as in v1.

Response 200 (always 200 for a well-formed authenticated request):

v1 (no `purpose`):

```json
{"granted":true,"grantId":"<uuid>","mac":"<hmac(K, \"grant\\n\" + machine + \"\\n\" + nonce + \"\\n\" + \"1\" + \"\\n\" + grantId)>"}
{"granted":false,"mac":"<hmac(K, \"grant\\n\" + machine + \"\\n\" + nonce + \"\\n\" + \"0\" + \"\\n\")>"}
```

purpose-aware:

```json
{"granted":true,"grantId":"<uuid>","mac":"<hmac(K, \"grant\\n\" + machine + \"\\n\" + nonce + \"\\n\" + \"1\" + \"\\n\" + grantId + \"\\n\" + purpose)>"}
{"granted":false,"mac":"<hmac(K, \"grant\\n\" + machine + \"\\n\" + nonce + \"\\n\" + \"0\" + \"\\n\" + \"\" + \"\\n\" + purpose)>"}
```

In the denied message the empty field between `0` and `purpose` is the absent
`grantId`, matching the v1 trailing empty grantId field.

The machine MUST verify the response `mac` with its own nonce (and purpose)
before acting; an invalid or missing `mac` is treated as `granted:false`.
Errors (no mac): 400 malformed/bad purpose/bad wait, 401 bad mac / bad or
reused nonce, 429 rate limit or too many concurrent waits.

Long-poll notes: a long-poll request counts once against the 60/min claim
budget, so one wait every ~25 s stays well under it. A server restart or
client disconnect terminates the wait without a MAC (fail-closed).

## GET /healthz

`200 ok`.

## Test vector

`K` = bytes 0x00..0x1f, `machine` = `komp`,
`nonce` = `AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=` (bytes 0x00..0x1f),
`grantId` = `00000000-0000-4000-8000-000000000001`.
Expected values (both implementations MUST pin them in tests):

v1 (no purpose):

| message | mac |
|---|---|
| claim | `+YedSn/aeRZqRNKKzkUS8Dy2aq/dVLkXbh9iX91L9XU=` |
| grant, granted | `UAADAs7pUxn/EN42woIH5K0qrH2YB+bd1SYZVKP//FI=` |
| grant, denied | `I6ClmYKmmfO6wodeKXOobAtJj/SgQ7C/ieNumDYrj1Y=` |

purpose-aware, `purpose` = `logon`:

| message | mac |
|---|---|
| claim | `AMPMcTPW1kuBTUp8sWtF4weIoI/Vtcp0R+cNBK+xdB0=` |
| grant, granted | `kmFSwXXu4g4wylTSIf9DSYht1v28pG4jboomTnocI0k=` |
| grant, denied | `HkwjpcROboJ+1uR7P8Rydoo2JvNFHx8iOEI0BS0G7K8=` |

purpose-aware, `purpose` = `unlock`:

| message | mac |
|---|---|
| claim | `jCy3/2JoHClgcwP3xBRHbyQNwGOsloT8uKvgu6fR7H0=` |
| grant, granted | `55kYSfTNqLYnhU6jg+WfuZaUkThoZehudbJPPuP/JH0=` |
| grant, denied | `OcN4e3gJUGVCUF8chuwGQ0FnhU9vRJ0HowN3zReozxc=` |
