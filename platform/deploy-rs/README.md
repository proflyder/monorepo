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
src/hermesdrift.rs    read-only Hermes config drift exporter (`/metrics`)
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
`converge`, `state`, `preflight`, `hermes rollout`, `hermes render-config`,
`hermes drift-exporter` and `hermes assist-smoke`.

## Hermes config delivery

The reviewed overlay lives in `projects/home-assistant/hermes` and is baked into
the `hermes-config` image at `/opt/hermes-overlay`
(`//projects/home-assistant/hermes:hermes-config_image`, repository
`ghcr.io/proflyder/hermes-config`). The image holds the static `home-deploy`
binary as its entrypoint and sets `HERMES_OVERLAY_DIR=/opt/hermes-overlay`, so
the same image runs both the init container (`hermes render-config`) and the
drift sidecar (`hermes drift-exporter`).

Overlay path precedence for both commands:

1. an explicit flag (`--fragment`, `--context`, `--soul`, `--voice-prompt`,
   `--telegram-prompt`, `--skills`);
2. `--source-dir DIR`, which fills every unset path with `DIR/<file>`;
3. `$HERMES_OVERLAY_DIR`;
4. the repository overlay (`projects/home-assistant/hermes`).

`render-config` keeps the parsed YAML tree as the contract: when Hermes rewrites
`config.yaml` with its own emitter (quotes, comments, indentation), the merged
tree is still equal, so no write and no `.prev` rotation happen. A real semantic
change writes the file, keeps one `.prev` backup (mode 600), prints a redacted
diff and still refuses TEST-NET/example placeholders unless
`--allow-placeholders` is passed. `--dry-run` and `--check` never write.

`drift-exporter` runs the same plan read-only, immediately at start and then
every `--interval` seconds (default 300), and serves Prometheus text on
`GET /metrics` (and `GET /healthz`). It exposes:

- `hermes_config_drift{file="<path relative to HERMES_HOME>"} 1` for every file
  the plan would rewrite;
- `hermes_config_drift_files <n>`;
- `hermes_config_check_success 1|0` (0 when the plan errors; the redacted error
  goes to stderr);
- `hermes_config_last_check_timestamp_seconds <unix>`;
- `hermes_config_overlay_info{digest="<sha256 of the overlay inputs>"} 1`.

The exporter never writes to `HERMES_HOME` and a failed check keeps the process
running so the alert fires instead of the sidecar crash-looping.

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

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `src/bin/parity.rs` — Parity runner for the frozen contract fixtures.
  - Recursively copies a fixture case so a test run never writes into the checkout (render outputs and fake command traces are generated files).
- `src/buildid.rs` — Short deploy build identifiers.
- `src/cli.rs` — CLI parsing and dispatch.
  - Only `render-config` defines this flag; `drift-exporter` never allows placeholders, so an absent id means false rather than a panic.
- `src/converge.rs` — Replaces a live Deployment with its rendered manifest.
- `src/diff.rs` — Small difflib-compatible unified diff for `hermes render-config --dry-run`.
- `src/envfile.rs` — Private `KEY=VALUE` files.
- `src/gojson.rs` — `encoding/json.Marshal`-compatible serialization of `serde_json::Value`.
- `src/hermes.rs` — Promotes Hermes/voice-proxy/health-exporter digests into the `runtime` Helm release.
  - Performs one Assist turn through the in-cluster HA API and prints the JSON result line the rollout parses. The token is read from the mounted Secret file and is never printed. The exit status is 1 on every failure;
- `src/hermesconfig.rs` — Renders the reviewed Hermes overlay into a live HERMES_HOME.
  - Only a semantic change is a write: formatting-only differences (Hermes rewrites the file itself) must not churn the config or `.prev`.
- `src/hermesdrift.rs` — Read-only Hermes config drift exporter.
  - Builds one read-only plan and converts it into a metric result. The second element carries the unredacted error text for the caller to log; metrics never include it.
- `src/http.rs` — Small HTTP client shared by preflight and the Assist smoke check.
- `src/lib.rs` — Home deployment tooling.
- `src/manifests.rs` — Manifest helpers used by the deploy scripts.
- `src/oci.rs` — Inspects and packages the OCI image layouts produced by `rules_oci`.
- `src/oci/archive.rs` — Python-compatible USTAR writer used by the OCI archives.
- `src/oci/json.rs` — Ordered JSON with Python `json.dumps` formatting.
- `src/preflight.rs` — Read-only safety checks for `deploy.sh --execute`.
- `src/process.rs` — Subprocess runner for `kubectl` (and later `helm`).
- `src/render.rs` — Literal `${KEY}` substitution for the private deploy templates.
- `src/runtimevalues.rs` — Splits a `helm template` render and the private runtime values into the two ownership domains of the runtime chart.
- `src/state.rs` — Deploy rollback markers.
  - Remembers the live Deployment name (or its absence) and clears the applied marker. `state_dir` must already exist. The probe is fail-closed: only a NotFound answer from the API server means "the Deployment does not exist".
- `src/yaml.rs` — Ordered YAML values and a `go.yaml.in/yaml/v2`-compatible emitter.
- `tests/parity.sh` — Parity gate: run the Rust binary over the frozen Go contract fixtures.
