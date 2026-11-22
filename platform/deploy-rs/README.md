# home-deploy

The `home-deploy` deploy CLI (Rust). `platform/deploy/resolve.sh`, the image and
the in-cluster Jobs all use `//platform/deploy-rs:home-deploy`. Subcommands —
[`platform/deploy/README.md`](../deploy/README.md).

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

`testdata/frozen/manifest.json` lists every case (argv, exit code,
stdout/stderr, output files). The `parity` test runs the binary on each case in
a temporary copy and compares bytes, including OCI archives; fake
`kubectl`/`helm` record argv and payloads under the case's `trace/`, preflight
cases get a local HTTP server from `http.json`. There is no generator: a changed
expected file is a deliberate contract change and is reviewed like code. Some
emitters (`yaml.rs`, `gojson.rs`, `oci/json.rs`, `oci/archive.rs`, single-dash
flags in `cli.rs`) exist only to keep these bytes stable.

## Checks

```bash
cargo fmt --all --check
cargo clippy -p home-deploy --all-targets -- -D warnings
cargo test -p home-deploy
bazel test //platform/deploy-rs/...   # unit_tests, clippy, rustfmt, parity
```

## Заметки к коду

- `state record` is fail-closed: only a NotFound answer from the API server
  means the Deployment does not exist.
- `hermes render-config` writes only on a semantic change; formatting-only
  differences do not touch the file or `.prev`.
- `drift-exporter` never allows placeholders and never puts the error text into
  metrics.
