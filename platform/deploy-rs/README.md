# home-deploy (Rust port)

Rust replacement for the Go `platform/deploy` command. It lives in
`platform/deploy-rs/` until the cutover so a tooling-only change does not match
every deploy unit's `platform/deploy/**` path; the Go binary remains the
executable contract in the meantime. The cutover moves the crate to
`platform/deploy` and the Go code is then deleted.

## Layout

```text
Cargo.toml            crate manifest (member of the root workspace)
BUILD.bazel           rust_binary home-deploy, unit_tests, clippy, rustfmt, parity
src/main.rs           process entry point
src/lib.rs            shared usage text and module list
src/cli.rs            clap definitions + Go-`flag` single-dash normalisation
src/envfile.rs        strict `KEY=VALUE` parser and permissive `env get` lookup
src/buildid.rs        `<revision>-<12 hex of sha256>`
src/render.rs         literal `${KEY}` substitution, allowlists, rollout fingerprint
src/yaml.rs           ordered YAML tree + yaml.v2-compatible emitter
src/manifests.rs      `eso names` and the bootstrap `secret opaque` JSON
src/runtimevalues.rs  storage/workload split and health-exporter pin
src/process.rs        kubectl runner (system + fake for tests)
src/converge.rs       Deployment converge with preserve rules
src/state.rs          rollback markers (fail-closed record)
src/gojson.rs         `encoding/json.Marshal`-compatible payloads
src/oci.rs            layout index/manifest, OCI and docker archives
src/oci/json.rs       ordered JSON with Python `json.dumps` formatting
src/oci/archive.rs    Python-compatible USTAR writer
src/bin/parity.rs     frozen-fixture runner
tests/parity.sh       Bazel sh_test wrapper for the runner
testdata/frozen/      contract fixtures captured from the Go binary
```

`env get`, `build-id`, `render`, `eso names`, `secret opaque`, `runtime split`,
`oci digest`, `oci pack`/`oci docker`, `converge` and `state` are ported.
`preflight` and `hermes` are declared in the CLI, print `not implemented yet`
and exit with status 2 until their modules land.

## Frozen contract fixtures

`testdata/frozen/manifest.json` lists every case: argv, working directory,
expected exit code, stdout/stderr files, output files, and either
`"status": "implemented"` or `"status": "skip"` with the reason. The parity
test runs the Rust binary on every implemented case in a temporary copy of the
case directory and compares bytes; skipped cases are printed with their reason
so the remaining scope is visible.

Captured from the Go binary (57 cases):

| Group | Cases | Notes |
|---|---|---|
| CLI / env / build-id | 13 | no args, `--help`, unknown command, single-dash flags, env quoting and duplicates |
| render | 13 | every unit (scheduler, currency-bot, mcp-gateway, wakegrant, ingress, nft) plus refusals, `--set` override and fingerprint |
| eso / secret / runtime split | 4 | name lists, opaque Secret JSON, storage/workload split |
| OCI | 4 | digest, OCI archive, docker archive, single-dash flags |
| converge | 4 | create, manifest-wins, drift-preserve, support objects with fake kubectl traces |
| state | 7 | record (live/absent/fail-closed), mark, rollback and first-install delete |
| preflight | 8 | fake HTTP: ok, alarm, legacy writer, endpoint mismatch, not ready, HTTP/JSON errors |
| hermes rollout | 5 | no-op, upgrade + smoke, smoke failure + rollback, `--rollback-only` |

44 cases pass byte for byte: CLI/env/build-id/render (26), `eso names`,
`secret opaque` (2), `runtime split`, all four OCI cases (digest plus the OCI
and docker archives), four converge cases and six state cases. The other 13
(preflight and hermes) are skipped with a reason.

Fake `kubectl`/`helm` scripts record every argv (with `-f` payloads and stdin)
under the case's `trace/`; the smoke Job name is normalised because it is
derived from the clock. Preflight cases ship the HTTP response map in
`http.json`.

## Regenerating

The generator drives the Go binary, so it only works while the Go code exists:

```bash
platform/deploy-rs/testdata/frozen/generate.py
# or with an already built binary:
platform/deploy-rs/testdata/frozen/generate.py --go-bin /path/to/home-deploy
```

It rebuilds `./platform/deploy/cmd/home-deploy` when `--go-bin` is omitted and
rewrites `cases/` and `manifest.json`. Review the diff like any other contract
change: a changed expected file is a behaviour change that the Rust port must
copy deliberately.

## Checks

```bash
cargo fmt --all --check
cargo clippy -p home-deploy --all-targets -- -D warnings
cargo test -p home-deploy
bazel test //platform/deploy-rs/...   # unit_tests, clippy, rustfmt, parity
```
