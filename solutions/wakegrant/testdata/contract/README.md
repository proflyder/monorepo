# wakegrant contract fixtures

Golden HTTP transcripts for `packages/contracts/wakegrant/PROTOCOL.md`, in the
normative format from `packages/go/contracttest/README.md`.

They are replayed unchanged by the Rust `home-contracttest` runner in
`solutions/wakegrant/crates/wakegrant/src/contract_tests.rs`. The fixed clock
is `2026-11-03T12:00:00Z`; the PROTOCOL.md vector key, nonce and grant id are
seeded per scenario. Do not edit a fixture to make an implementation pass —
change `PROTOCOL.md` first, then both implementations and the fixture.
