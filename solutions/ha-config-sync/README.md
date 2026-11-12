# ha-config-sync

Delivers a fixed set of Home Assistant YAML files to a Home Assistant OS host,
keeps them in sync on an interval and rolls back automatically when Home
Assistant rejects the result.

The solution is generic: it has no project endpoint, file list or token baked
in. A project composes an image with its desired files plus a manifest and
supplies the environment below.

## How a cycle works

1. Ask the forced-command SSH agent for its protocol version. A mismatch is
   logged and reported, and the cycle stops.
2. Ask the agent for the SHA-256 of every managed path and compare with the
   files carried by the service. Only different files (including `MISSING`)
   are drifted.
3. `SYNC_MODE=observe` stops here; otherwise an empty drift set stops here.
4. `apply`: the agent stages the exact bytes, verifies SHA-256, backs up the
   previous files (recording which paths did not exist) and moves the staged
   files into place.
5. `POST /api/config/core/check_config` must answer `"result": "valid"`.
6. Every distinct reload service of the drifted files is called once.
7. If a drifted file is marked `restart: true`, the optional guard entity must
   be `off`; otherwise the restart is deferred **without a rollback** and kept
   in memory (`ha_config_sync_restart_pending 1`): because the files are
   already on disk, every later cycle retries the guard, the restart and the
   wait even when there is no drift. A permitted restart calls
   `homeassistant.restart`; a transport error, timeout or 5xx from that call is
   treated as "restart in progress" (only a 4xx is a failure), then the service
   first watches `GET /api/` go down (up to 90 s) and waits for it to answer
   again (up to five minutes).
8. Re-read the hashes; they must equal the desired set.
9. Any failure in steps 4–8 restores the backup, repeats the same reload plan
   and restarts again if it had restarted. `rolled_back` means the files were
   restored; a reload or restart error during rollback is logged only. A rolled-back desired digest is
   retried with exponential backoff: five minutes, doubling, capped at six
   hours. A new desired digest resets the backoff.
10. After a successful cycle the agent prunes backups to the newest ten.

Logs are structured JSON. They name files and outcomes but never tokens or file
contents; the `check_config` error text is truncated before it is logged.

## Environment

| Variable | Required | Meaning |
|---|---|---|
| `HA_URL` | yes | Home Assistant base URL, `http(s)://host:port`, no path. |
| `HA_TOKEN_FILE` | yes | File with a long-lived bearer token; trimmed. |
| `SSH_ADDR` | yes | Home Assistant OS host SSH endpoint, `host:port`. |
| `SSH_USER` | yes | User owning the restricted key. |
| `SSH_KEY_FILE` | yes | Unencrypted private key for the agent. |
| `SSH_KNOWN_HOSTS_FILE` | yes | Pinned host key(s) for `SSH_ADDR`. |
| `DESIRED_DIR` | yes | Directory with the exact files to deliver. |
| `MANIFEST_FILE` | yes | Manifest YAML (below). |
| `SYNC_INTERVAL` | no | Seconds between cycles, default `300`. |
| `SYNC_MODE` | no | `apply` (default) or `observe`. |
| `METRICS_ADDR` | no | Listen address, default `0.0.0.0:9119`. |
| `RESTART_GUARD_ENTITY` | no | Entity that must be `off` before a restart. |

`DESIRED_DIR` is the authority for what may be written. Every regular file
under it must match the managed path pattern; anything else (a README, a test,
a symlink) is a start-up error.

## Manifest

```yaml
files:
  - glob: "scripts.yaml"
    reload: ["script.reload"]
  - glob: "custom_sentences/ru/*.yaml"
    reload: ["conversation.reload"]
  - glob: "shell_commands.yaml"
    restart: true
```

The schema is also in [`testdata/manifest.example.yaml`](testdata/manifest.example.yaml).
Validation rules:

* `files` must exist and be non-empty;
* globs are relative, segment-aware (`*`/`?` never cross `/`), and must match at
  least one desired file;
* every desired file is covered by exactly one entry (overlaps are rejected);
* a reload service must be `<domain>.<service>`;
* the path pattern is
  `^(custom_sentences/[a-z]{2}/)?[a-z0-9_]+\.yaml$`;
* `configuration.yaml`, `secrets.yaml` and `scenes.yaml` are always denied.

## Agent protocol

`agent/ha-config-agent.sh` is the forced command (POSIX `sh`, `set -eu`). The
client requests a verb, which the host exposes as `$SSH_ORIGINAL_COMMAND`, and
sends the payload on stdin. Every path is validated against the same pattern
and deny list in both the controller and the agent; `..`, absolute paths and
symlinks at any managed component are rejected.

| Verb | stdin | stdout |
|---|---|---|
| `version` | — | `ha-config-agent <protocol>` |
| `hashes` | one relative path per line | `<sha256\|MISSING> <path>` per line |
| `apply` | `<path> <sha256> <base64 content>` per line | backup id |
| `restore <id>` | — | — |
| `prune <keep>` | — | — |

The agent runs with `set -f` (no pathname expansion), so a verb argument such
as `*` stays literal and is rejected by validation. `apply` writes into
`staging/`, verifies each SHA-256 and only then creates
`backups/<UTC timestamp>/` with `files/` (previous copies), `paths` (everything
touched) and `absent` (paths that did not exist). Files move into the config
directory with `mv` on the same filesystem; a failed move restores the backup it
just made before returning an error. `restore <id>` validates the id format,
restores the copied files and removes the ones recorded as absent. Calls are
serialised with `flock` (a `mkdir` fallback is used on development hosts
without it). A failure prints `ERROR <reason>` on stderr and exits non-zero.

The config root `/mnt/data/supervisor/homeassistant` and state root
`/mnt/data/ha-config-sync` are fixed. The environment variables
`HA_CONFIG_AGENT_ROOT` and `HA_CONFIG_AGENT_STATE` exist only for the tests;
dropbear does not forward client environment.

The protocol version lives in `PROTOCOL_VERSION` in the script and in
`src/protocol.rs`; a unit test asserts they match.

## Security model

* The key is useful only for the agent: the `authorized_keys` line is
  `restrict,command="HA_CONFIG_AGENT_ALLOWED_FROM=<node ip> /mnt/data/ha-config-sync/agent.sh"`.
  HAOS dropbear does **not** support an authorized_keys host restriction, so the
  source address is
  enforced by the agent: when `HA_CONFIG_AGENT_ALLOWED_FROM` is set, the first
  field of `$SSH_CLIENT` (fallback `$SSH_CONNECTION`) must match one of the
  space/comma separated addresses, otherwise the agent prints
  `ERROR source not allowed`. `restrict` disables port, agent and PTY
  forwarding on dropbear.
* `known_hosts` is pinned in the controller and host-key verification follows
  OpenSSH patterns (comma lists, `!` negation, wildcards, hashed names,
  `@revoked`).
* Plain `configuration.yaml`, `secrets.yaml`, `scenes.yaml` and every path
  outside the manifest cannot be written even if the controller is compromised:
  the agent re-validates every path independently.
* The agent never trusts a client-provided content digest: it recomputes the
  SHA-256 after decoding and refuses a mismatch before touching the config
  directory.

## Metrics

Prometheus text on `METRICS_ADDR/metrics`, liveness on `/healthz`.

| Series | Meaning |
|---|---|
| `ha_config_sync_drift_files` | number of drifted files |
| `ha_config_sync_drift{file="…"} 1` | one series per currently drifted file |
| `ha_config_sync_managed_files` | files in the desired set |
| `ha_config_sync_last_check_success` | `1` when the last cycle completed cleanly |
| `ha_config_sync_last_check_timestamp_seconds` | last cycle end |
| `ha_config_sync_last_apply_timestamp_seconds` | last apply attempt end |
| `ha_config_sync_apply_total{result="success\|rolled_back\|failed\|deferred"}` | apply outcomes; `rolled_back` means files restored |
| `ha_config_sync_restart_pending` | `1` while a guard-deferred restart is outstanding |
| `ha_config_sync_agent_info{version="…"} 1` | reported agent protocol |
| `ha_config_sync_desired_info{digest="…"} 1` | desired-set digest |
| `ha_config_sync_mode{mode="apply\|observe"} 1` | configured mode |

## Development

```sh
cargo test -p ha-config-sync -p forced-ssh -p mcp-gateway
cargo clippy -p ha-config-sync --all-targets -- -D warnings
bazel test //solutions/ha-config-sync/... //packages/rust/forced-ssh/...
```

`tests/agent.rs` embeds the real script, runs it with `sh` against a temporary
root and covers hashes, apply, restore (including files that did not exist
before), path escapes, unknown verbs and pruning.
