# Golden fixtures

Frozen behaviour of the Go installer, captured before it was removed (P3.9). The Rust crate
`pulsar-installer` reads these files from its tests; they are data, not generated at build time.

| File | Source in the Go installer | Rust test |
|---|---|---|
| `usage.txt` | `main.go` `usage` | `cli::tests::usage_matches_go_golden` |
| `init_script_*.txt`, `write_init_cmd.json` | `autostart.go` | `scripts::tests` |
| `rootfs_guard.json`, `root_mount.json` | `autostart.go` | `scripts::tests` |
| `rootkeeper.json`, `hook_active.json` | `rootkeeper.go` and its tests | `scripts::tests` |
| `probe.json` | `probe.go` (`modelName`, `abis`, `archOf`, `quasarDeviceType`, `stationCode`) | `probe::tests` |
| `platformtools.json` | `platformtools.go` pins | `platformtools::tests::pins_match_go_golden` |
| `sh_raw.json` | `adb.go` `shRaw` with a fake adb | `adb::tests::shell_output_matches_go_golden` |
| `discover.json`, hex-поля `query_*` в `discover.json` | `discover.go` / `discover_test.go` | `discover::tests` |
| `deploy_nossh.json`, `deploy_ssh.json` | `deploy.go` `deployFiles` + `bootAutostart` with a recording fake adb (`--ssh` off/on) | `deploy::tests` |
| `health_ok.json`, `health_no_marker.json` | `deploy.go` `healthCheck` with fake logs | `deploy::tests` |

`deploy_*.json` records the exact adb argv (including the `__rc=` echo) and stdout/stderr;
local paths are normalised to `<ROOT>`/`<TMP>`. Everything was generated from the Go installer
except the two `rootfs_guard.json` cases `uppercase verity` / `uppercase dm=`, which were added
by hand during review. Regenerate the rest only from a Go checkout at the pre-P3.9 commit; the
generator itself was intentionally not kept.
