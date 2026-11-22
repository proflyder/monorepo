# Scheduler deployment (home K3s)

Durable `home.v1.SchedulerService` plus the Home Assistant executors on the
single-node K3s cluster, namespace `home-assistant`:

| Workload | Image | Role |
|---|---|---|
| `scheduler` | `ghcr.io/proflyder/scheduler` | schedules, timers, outbox, leases; SQLite on PVC `scheduler-data` |
| `ha-bridge` | `ghcr.io/proflyder/ha-bridge` | leases the `home` queue, calls only allowlisted HA scripts; ledger on `ha-bridge-data` |
| `ha-control` | `ghcr.io/proflyder/ha-control` | phone catalog (`GET /v1/catalog`) and the `control` queue |
| `ha-scenario` | `ghcr.io/proflyder/ha-scenario` | `scenario` queue executor of `ScenarioService` definitions; ledger on `ha-scenario-data` |

Behaviour and API: `solutions/scheduler/README.md`,
`solutions/ha-bridge/README.md`. Home Assistant performs audio only; voice
intents in `projects/home-assistant/ha-voice` call the API.

Images are distroless static musl binaries built by Bazel (`BUILD.bazel`:
`:<name>_image`, `_load`, `_push`), referenced by digest only. No shell:
probes run the binary with `-healthcheck` (authenticated `GetStatus`) and
`-status`. Pods run as `65534`, matching the existing file owners on the
`local-retain` PVCs.

## Deploy (Flux)

`Kustomization/app-ha-executors` applies `manifests/` from `develop` with
digests from `deploy/home-apps` (`SCHEDULER_IMAGE`, `HA_BRIDGE_IMAGE`,
`HA_CONTROL_IMAGE`, `HA_SCENARIO_IMAGE`). Steady state — one replica each,
`BRIDGE_EXECUTE=true`, `CONTROL_EXECUTE=true`, `SCENARIO_EXECUTE=true` — is
pinned in `manifests/kustomization.yaml`; a mode change is a Git change. PVCs
carry `prune: disabled`.

Secrets: ESO from `secret/k8s/home-assistant/scheduler-auth` and
`ha-bridge-ha` (`docs/SECRETS.md`), read at runtime from files
(`SCHEDULER_TOKEN_FILE`, `HA_TOKEN_FILE`), never from arguments.

## Rollback

Revert the pins commit (optionally `flux suspend kustomization/app-ha-executors`
first). Never `kubectl rollout undo`. Rolling the binary back after a schema
migration needs a database restore from a pre-migration copy (the older binary
rejects the newer schema).

## Admin script (drills, mode transitions)

`deploy.sh` builds the images, imports them into the node's containerd over SSH
(`PUSH_IMAGES=1` pushes to GHCR instead), renders templates with
`home-deploy render` into `secrets/local/rendered/scheduler` and applies. Private
values from `PRIVATE_ENV` (default `secrets/home/scheduler.env`, template
`scheduler.env.example`). For a drill: `flux suspend`, run the script, resume.

```bash
NODE_SSH=labuser@node KUBECONFIG=… ./projects/home/kubernetes/scheduler/deploy.sh             # preserve modes
NODE_SSH=labuser@node ./projects/home/kubernetes/scheduler/deploy.sh --shadow                # bridge dry-run, 0 replicas
HA_TOKEN_FILE=secrets/home/ha.token NODE_SSH=labuser@node \
  ./projects/home/kubernetes/scheduler/deploy.sh --execute                                    # preflight, then 1 replica
./projects/home/kubernetes/scheduler/deploy.sh --only scheduler                              # scheduler | ha-bridge | ha-control
./projects/home/kubernetes/scheduler/deploy.sh --control-execute | --control-catalog
```

- Default reruns preserve modes and replica counts.
- `--execute` preflight refuses if a legacy HA writer is on, an alarm is
  ringing, the scheduler is not ready or the HA endpoint differs.
  `PREFLIGHT_IN_CLUSTER=1` runs it as the Job `preflight-job.yaml`.
- `--control-execute` refuses while `GetStatus` reports pending/leased
  `control` events. Regression: `test_deploy_control.sh`.

## Network and auth

- `scheduler` NodePort `SCHEDULER_NODE_PORT` (default 30098) and `ha-control`
  NodePort `CONTROL_NODE_PORT` (default 30099) for the LAN; HTTPS
  `alarms.<HOME_DOMAIN>` / `control.<HOME_DOMAIN>` through Traefik
  (`projects/home/kubernetes/ingress/`). Every RPC needs a bearer token.
- Do not back a proxied route with a selectorless Service pointing at this
  node's own NodePort: single-node ClusterIP DNAT to the node NodePort does not
  re-enter kube-proxy's NodePort hook.
- `nft/scheduler-nodeport.nft` drops the scheduler, control and intercom
  NodePorts in `prerouting` unless the source is the LAN or the pod CIDR (IPv6
  dropped). Host config, applied manually after a CIDR/port change:
  `ssh <node> sudo env PRIVATE_ENV=… bash projects/home/kubernetes/scheduler/nft/apply.sh`;
  persisted by `home-scheduler-firewall.service`.

## ha-control and ha-scenario notes

- `ha-control` shares the scheduler bearer (`CONTROL_TOKEN_FILE`). A script is
  listed only with the HA label `scenario` (`CONTROL_SCENARIO_LABEL`);
  `CONTROL_LABEL` is refused at start. The scheduler lease allowlist must
  include both `ha-bridge-home` and `ha-control-home`.
- `ha-scenario`: `SCENARIO_LEASE_SECONDS` (default 3600) must cover a whole
  run, since an ack after lease expiry is rejected; one delay step is capped at
  3600 s. Retries only before the first step; an interrupted run is acked
  `FAILED interrupted`, never replayed. Disable by scaling to 0 or
  `SCENARIO_EXECUTE=false` (through Git for the steady state).
- Household scenarios live in `scenarios/*.json` and are seeded idempotently
  by label (never overwriting owner edits):

  ```bash
  SCHEDULER_URL=http://<node>:30098 SCHEDULER_TOKEN_FILE=secrets/home/scheduler.token \
    node projects/home/kubernetes/scheduler/scenarios/seed.mjs [--dry-run]
  ```

  Renaming a scenario label breaks the voice command: HA resolves scenarios by
  exact label, case-insensitively.

## Backup and restore

`./backup.sh [dir]` records replica counts, scales the writers to 0 (fails
closed if a writer pod remains), copies the SQLite and ledger volumes with
SHA-256 sums and restores the counts. Default output
`local-data/local/backups/scheduler/<timestamp>`. Restore —
`docs/REBUILD.md` («Scheduler из SQLite»).

## Limits

- Single writer: never two scheduler replicas on the same PVC (`Recreate`).
- At-least-once delivery: a crash between the HA call and the ACK can repeat
  a side effect; audibility is never claimed.
- STOP for timer/reminder is `ACK_SKIPPED`: the announce is one-shot and a
  global stop could silence another alarm. Emergency audio stop — HA script
  `alarm_stop`.
