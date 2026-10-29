# Scheduler deployment (home K3s)

Durable `ricardo.v1.SchedulerService` plus the Home Assistant executor
(`ha-bridge`) on the single-node `lab-k3s` cluster.

- `solutions/scheduler` owns schedules, timers, occurrences, outbox and leases.
- `solutions/ha-bridge` leases the `home` queue and calls only allowlisted HA
  scripts (`alarm_start`, `schedule_announce`, `alarm_stop`). It never owns
  schedule state.
- Home Assistant performs audio only. Voice intents in
  `projects/home-assistant/ha-voice` call the canonical API.

## Files

```text
manifests/scheduler.yaml   PVC + Deployment + NodePort Service (template)
manifests/bridge.yaml      ledger PVC + executor Deployment (template)
manifests/control.yaml     ha-control Deployment (template)
manifests/control-service.yaml  ha-control ClusterIP + NodePort
nft/scheduler-nodeport.nft NodePort source filter (template)
../tailscale/control-ingress.yaml  tailnet route for ha-control
scheduler.env.example      placeholders; copy to secrets/home/scheduler.env
render.py                  strict ${VAR} renderer (unknown/empty => fail)
deploy.sh                  build, release, firewall, apply, mode transitions
backup.sh                  quiesced SQLite + ledger backup with checksums
preflight.py               cutover gate (legacy writers off, scheduler ready)
```

Tracked templates contain only placeholders/TEST-NET values. Real addresses,
CIDRs and the NodePort live in `secrets/home/scheduler.env` (git-ignored).
Rendered manifests go to `secrets/local/rendered/scheduler`.

## One-time prerequisites

```bash
kubectl -n home-assistant create secret generic scheduler-auth \
  --from-file=token=secrets/home/scheduler.token
kubectl -n home-assistant create secret generic ha-bridge-ha \
  --from-file=token=secrets/home/ha.token
```

Both tokens are read from files at runtime (`SCHEDULER_TOKEN_FILE`,
`HA_TOKEN_FILE`) and never appear in process arguments, probe arguments or
Kubernetes events.

## Deploy

```bash
NODE_SSH=root@node ./projects/home/kubernetes/scheduler/deploy.sh            # install/update, preserve mode
NODE_SSH=root@node ./projects/home/kubernetes/scheduler/deploy.sh --shadow   # dry-run, 0 replicas
HA_TOKEN_FILE=secrets/home/ha.token \
NODE_SSH=root@node ./projects/home/kubernetes/scheduler/deploy.sh --execute  # cutover
NODE_SSH=root@node ./projects/home/kubernetes/scheduler/deploy.sh --skip-build
```

- Builds with Bazel (`@rules_go//go/toolchain:linux_amd64`), stages an immutable
  release under `/opt/ricardo/releases/<build-id>` and atomically switches
  `/opt/ricardo/current`. Pods mount that path, so a new build rolls the pod.
- Installs the NodePort source filter **before** the Service is exposed.
- Re-running without `--execute`/`--shadow` preserves the bridge mode and
  replica count; it only rolls the build annotation.
- `--execute` runs `preflight.py` first and refuses if any legacy HA writer is
  still on, an alarm is ringing, or the scheduler is not ready.

## ha-control

`ha-control` is the phone-facing catalog and control executor, a separate
process from the alarm bridge.

- Serves `GET /v1/catalog` (bearer) built live from HA; no catalog file.
- Leases only the dedicated `control` queue (`CONTROL_QUEUE`; `home`/`tasks`
  are rejected) and executes only entries present in the catalog.
- Shares the scheduler bearer: the app has one token field, so
  `CONTROL_TOKEN_FILE` points at the same secret.
- Lights, switches and scenes are listed dynamically. A script appears only
  when it carries the native HA label `scenario` (`CONTROL_SCENARIO_LABEL`,
  default `scenario`); icons, areas and names are display-only. Infrastructure
  scripts (alarm helpers, RPC wrappers) stay hidden. Assign the label in HA
  (Settings > Automations & scenes > Scripts > Add label) to the scripts that
  are personal scenarios. `CONTROL_LABEL` was removed and the process refuses
  to start if it is still set.
- `CONTROL_EXECUTE=false` serves the catalog only. `deploy.sh --control-execute`
  runs a fail-closed preflight (refuses when GetStatus reports pending/leased
  events on the `control` queue) and only then enables execution;
  `--control-catalog` disables it again. Default reruns preserve the current
  execute mode and replica count. Regression: `test_deploy_control.sh`.
- Delivery is at-least-once: a crash between the HA call and the ACK can repeat
  a command. The catalog id/action payload is authorization-checked per run,
  but the executor is not exactly-once.
- HA's `repeat` delivery for `control` shares the scheduler event lease; the
  scheduler allowlist must include both `ha-bridge-home` and `ha-control-home`
  (see `manifests/scheduler.yaml`).

Endpoints (private values in `secrets/home/scheduler.env`):

- LAN: `http://<node>:${CONTROL_NODE_PORT}` (default 30099), same nft source
  filter as the scheduler NodePort.
- Tailnet: `https://<TAILNET_CONTROL_HOSTNAME>.<tailnet>.ts.net`, dedicated
  Ingress; `pl-home` and the scheduler ingress are unchanged.

## Network and auth

- `scheduler` Service is a NodePort (`SCHEDULER_NODE_PORT`, default 30098) for
  the LAN (HA voice + Android on the home network).
- A dedicated Tailscale Ingress (`projects/home/kubernetes/tailscale/scheduler-ingress.yaml`)
  exposes the same Service on `<TAILNET_SCHED_HOSTNAME>.<tailnet>.ts.net`,
  tailnet-only, TLS by the operator. The existing `pl-home` ingress still
  routes `/` to HAOS and `/grafana` to Grafana; the scheduler does not use a
  path prefix on it.
- Do not back a tailnet route with a selectorless Service pointing at this
  node's own NodePort: single-node ClusterIP DNAT to the node NodePort does not
  re-enter kube-proxy's NodePort hook.
- `nft` drops the NodePort in `prerouting` before kube-proxy DNAT unless the
  source is the home LAN, pod CIDR or Tailscale CGNAT; IPv6 NodePort is dropped.
  Persistence: `ricardo-scheduler-firewall.service` on the node.
- Every RPC requires `Authorization: Bearer <token>`; unauthenticated requests
  get HTTP 401. HA uses the LAN endpoint; Android can use either the LAN
  endpoint or the tailnet HTTPS hostname. Both terminate at the same Service.
- Readiness/liveness are authenticated: the probe runs the binary itself with
  `-healthcheck`, which reads the token file and calls `GetStatus`.

## Persistence and backup

- SQLite on PVC `scheduler-data` (`local-retain`), one pod, `Recreate`.
- Bridge ledger on PVC `ha-bridge-data`.
- `./backup.sh [dir]` records the real replica counts, scales both writers to 0,
  fails closed if a writer pod remains, copies both volumes, writes SHA-256
  sums, and restores the recorded counts. Default output is
  `secrets/local/backups/scheduler/<timestamp>`; never inside the repo tree.

## Cutover

1. Enable voice adapters in HA (they are installed and validated separately).
2. Quiesce the old owner: `automation.alarm_scheduler`,
   `schedules_alarm_dispatcher`, `schedules_reminder_dispatcher`,
   `schedules_timer_finished` off. They ship with `initial_state: false`.
3. Snapshot and import state (see `projects/home/scheduler-migration`), verify.
4. `deploy.sh --execute` (preflight + bridge execute).
5. Verify: scheduler `GetStatus` shows a live `ha-bridge-home` consumer,
   `enabledSchedules` matches the import, and a scheduled occurrence produces
   `DELIVERED` only after HA returns 2xx.

## Rollback

Order matters: stop the new owners **before** restoring the old writer.

1. `deploy.sh --shadow` (bridge dry-run, 0 replicas). Scheduler keeps its data
   but stops being an executor.
2. Re-enable the legacy HA automations (`initial_state` back to true or
   `automation.turn_on`) only after the bridge is stopped. Never run both.
3. Restore the previous HA dataset if required. Do not overwrite schedules
   created in the scheduler after cutover with the pre-cutover snapshot; the
   migration mapping is the reverse-reconciliation record.
4. To stop audio immediately, use the HA script `alarm_stop` (executor-level
   emergency stop); it is independent of scheduler ownership.

## Limits

- Single writer: never run two scheduler replicas on the same PVC.
- At-least-once delivery; a crash between the HA call and the ACK can repeat a
  side effect. Audibility is never claimed by the scheduler or the bridge.
- STOP for timer/reminder is acknowledged as `ACK_SKIPPED`; the announce is
  one-shot and a global stop could silence another alarm.
