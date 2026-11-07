# home-deploy (Rust port)

Rust implementation of the `home-deploy` deploy CLI. It is the executable
contract: `platform/deploy/resolve.sh`, the CI image and the in-cluster Jobs all
use `//platform/deploy-rs:home-deploy`. The former Go implementation has been
removed; `testdata/frozen` keeps the captured Go/Python outputs as the parity
fixtures (they are frozen data, not generated at test time).

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
src/http.rs           JSON HTTP client for preflight / assist-smoke
src/preflight.rs      read-only cutover checks
src/hermes.rs         Hermes rollout, smoke Job and Assist client
src/hermesconfig.rs   Hermes config merge, plan, diff and apply
src/diff.rs           unified diff for `render-config --dry-run`
src/oci.rs            layout index/manifest, OCI and docker archives
src/oci/json.rs       ordered JSON with Python `json.dumps` formatting
src/oci/archive.rs    Python-compatible USTAR writer
src/bin/parity.rs     frozen-fixture runner
tests/parity.sh       Bazel sh_test wrapper for the runner
testdata/frozen/      contract fixtures captured from the Go binary
```

Every subcommand is ported: `env get`, `build-id`, `render`, `eso names`,
`secret opaque`, `runtime split`, `oci digest`, `oci pack`/`oci docker`,
`converge`, `state`, `preflight`, `hermes rollout`, `hermes render-config` and
`hermes assist-smoke`.

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
| state | 6 | record (live/absent/fail-closed), mark, rollback and first-install delete |
| preflight | 8 | fake HTTP: ok, alarm, legacy writer, endpoint mismatch, not ready, HTTP/JSON errors |
| hermes rollout | 5 | no-op, upgrade + smoke, smoke failure + rollback, `--rollback-only` |

All 57 cases pass byte for byte: CLI/env/build-id/render (26), `eso names`,
`secret opaque` (2), `runtime split`, four OCI cases, four converge cases, six
state cases, eight preflight cases and five Hermes rollout cases. There are no
skipped cases. The parity runner starts a local HTTP server for the preflight
cases and normalises the clock-derived smoke Job name.

Fake `kubectl`/`helm` scripts record every argv (with `-f` payloads and stdin)
under the case's `trace/`; the smoke Job name is normalised because it is
derived from the clock. Preflight cases ship the HTTP response map in
`http.json`.

## Frozen fixtures

`testdata/frozen` was captured from the Go implementation and the Python
renderer before the Go code was removed. The fixtures are frozen data: there is
no generator in the tree any more. Treat a changed expected file as a deliberate
contract change and review it like code; the parity test must stay green.

## Checks

```bash
cargo fmt --all --check
cargo clippy -p home-deploy --all-targets -- -D warnings
cargo test -p home-deploy
bazel test //platform/deploy-rs/...   # unit_tests, clippy, rustfmt, parity
```
