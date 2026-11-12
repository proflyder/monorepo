# projects/home/kubernetes/ha-config-sync

Flux-owned deployment of the generic [`solutions/ha-config-sync`](../../../../solutions/ha-config-sync/README.md)
service for the home project.

It delivers the deliverable files of `projects/home-assistant/ha-voice`
(`sync.yaml` lists the globs and the reload/restart action) to the HAOS host
`/mnt/data/supervisor/homeassistant` through a forced-command SSH agent, validates
Home Assistant's configuration and rolls back on any failure.

## What lives here

| Path | Purpose |
|---|---|
| `BUILD.bazel` | static musl image `ha-config-sync` with `/opt/ha-voice` and `/opt/ha-voice-sync.yaml` |
| `manifests/` | Deployment, Service, ServiceMonitor, ExternalSecret, NetworkPolicy, PrometheusRule, kustomization |
| `bootstrap.sh` | owner bootstrap: agent install, key in OpenBao, one `authorized_keys` line |
| `openbao_kv.py` | narrow KV helper used only by the bootstrap (never prints values) |
| `test_manifests.py` | hardening, secret wiring, network egress, alert contract |

The service is generic: endpoints, token file and paths come from env in the
Deployment; the project owns only the file set + manifest baked into the image.

## Security model

- One ed25519 key from OpenBao `k8s/home-assistant/ha-config-sync`, accepted on
  HAOS only through
  `restrict,command="HA_CONFIG_AGENT_ALLOWED_FROM=<K3s node LAN IP> /mnt/data/ha-config-sync/agent.sh"`.
  HAOS dropbear has no authorized_keys host restriction and no port/agent/PTY
  forwarding is allowed; the agent checks the client address from `$SSH_CLIENT`
  itself.
- Controlling side pins `known_hosts` as `[<HAOS address>]:22222`.
- The agent and the controller both enforce the same path allowlist and deny
  `configuration.yaml`, `secrets.yaml`, `scenes.yaml`.
- The pod runs as uid 65534, read-only root, all capabilities dropped, no
  ServiceAccount token. Egress is only DNS and HAOS `:22222`/`:8123`.
- The HA token is the existing `HERMES_HA_TOKEN` from
  `k8s/home-assistant/hermes-environment`; no new HA credential is created.

## Owner steps

Before merging:

```bash
mise run admin:ha-config-sync-bootstrap -- --dry-run
mise run admin:ha-config-sync-bootstrap
```

The task is idempotent. It installs the agent as
`/mnt/data/ha-config-sync/agent.sh` (root, 0755), creates
`/mnt/data/ha-config-sync/{staging,backups}`, writes `ssh_private_key` and
`known_hosts` to OpenBao only when missing, and keeps exactly one
`ha-config-sync` line in `/root/.ssh/authorized_keys` (owner lines kept). It
prints verification commands and never prints secret values.

After merging and Flux convergence:

```bash
mise run kubectl -- -n home-assistant rollout status deployment/ha-config-sync
mise run kubectl -- -n home-assistant logs deployment/ha-config-sync --tail=50
mise run kubectl -- -n home-assistant port-forward svc/ha-config-sync 19119:9119
curl -s localhost:19119/metrics | grep ha_config_sync
```

Check `/report ha-config-sync` in the voice assistant and the
`HaConfig*` alerts in Alertmanager. The first cycle runs immediately; a clean
host reports `ha_config_sync_drift_files 0`, `last_check_success 1` and
`apply_total{result="success"}` after the first delivery.

If HAOS is already in sync, no `apply` is recorded — only the check metrics
move. A restart-only change (`shell_commands.yaml`) is deferred while
`input_boolean.alarm_ringing` is on and reported as
`ha_config_sync_restart_pending 1` until it completes.

## Pause and rollback

- Pause: set `SYNC_MODE=observe` (measure only) or `replicas: 0` in
  `manifests/deployment.yaml`, merge to `develop`, let Flux apply. Never
  `kubectl edit` the live Deployment: Flux reverts it.
- Rollback an image: revert the `deploy/home-apps` commit that changed
  `HA_CONFIG_SYNC_IMAGE`; Flux re-applies the previous digest and the previous
  file set within 1–2 minutes. The agent and key on HAOS are unchanged.
- Emergency manual delivery: copy the needed YAML through HA File
  Editor/Samba/SSH add-on and run Check configuration → Restart. The next
  cycle reasserts the repository state.

## Alerts

`manifests/prometheusrule.yaml` (namespace `home-assistant`, warning, component
`ha-config`): `HaConfigDrift` (15 m), `HaConfigSyncFailing` (30 m),
`HaConfigSyncRolledBack` (1 h increase), `HaConfigSyncDown` (15 m absent).
