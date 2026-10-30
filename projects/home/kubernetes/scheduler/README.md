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
scheduler.env.example      placeholders; copy to secrets/home/scheduler.env
render.py                  strict ${VAR} renderer (unknown/empty => fail)
deploy.sh                  build, release, firewall, apply, mode transitions
backup.sh                  quiesced SQLite + ledger backup with checksums
preflight.py               safety preflight (retired writers absent/off, scheduler ready)
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

`NODE_SSH` is the K3s node (`labuser@192.0.2.27` on this stand, key
`secrets/proxmox-install/proxmox_admin`). `deploy.sh` calls plain `ssh`/`scp`
without key flags, so bind that key in `~/.ssh/config` or load it into
`ssh-agent` first.

```bash
NODE_SSH=root@node ./projects/home/kubernetes/scheduler/deploy.sh            # install/update, preserve mode
NODE_SSH=root@node ./projects/home/kubernetes/scheduler/deploy.sh --shadow   # dry-run, 0 replicas
HA_TOKEN_FILE=secrets/home/ha.token \
NODE_SSH=root@node ./projects/home/kubernetes/scheduler/deploy.sh --execute  # start executor
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
- HTTPS (LAN/VPN): `https://control.<HOME_DOMAIN>` and
  `https://alarms.<HOME_DOMAIN>` through Traefik; see
  `projects/home/kubernetes/ingress/`.

## Network and auth

- `scheduler` Service is a NodePort (`SCHEDULER_NODE_PORT`, default 30098) for
  the LAN (HA voice + Android on the home network).
- The HTTPS names terminate at Traefik `IngressRoute` objects in namespace
  `traefik` and forward to the `scheduler` / `ha-control` ClusterIP Services.
- Do not back a proxied route with a selectorless Service pointing at this
  node's own NodePort: single-node ClusterIP DNAT to the node NodePort does not
  re-enter kube-proxy's NodePort hook.
- `nft` drops the NodePort in `prerouting` before kube-proxy DNAT unless the
  source is the home LAN or the pod CIDR; IPv6 NodePort is dropped.
  Persistence: `ricardo-scheduler-firewall.service` on the node.
- Every RPC requires `Authorization: Bearer <token>`; unauthenticated requests
  get HTTP 401. HA uses the LAN endpoint; Android can use either the LAN
  endpoint or the HTTPS name over the VPN. Both terminate at the same Service.
- Readiness/liveness are authenticated: the probe runs the binary itself with
  `-healthcheck`, which reads the token file and calls `GetStatus`.

## Persistence and backup

- SQLite on PVC `scheduler-data` (`local-retain`), one pod, `Recreate`.
- Bridge ledger on PVC `ha-bridge-data`.
- `./backup.sh [dir]` records the real replica counts, scales both writers to 0,
  fails closed if a writer pod remains, copies both volumes, writes SHA-256
  sums, and restores the recorded counts. Default output is
  `secrets/local/backups/scheduler/<timestamp>`; never inside the repo tree.

## Operation

- `deploy.sh --shadow`: bridge dry-run, 0 replicas; scheduler keeps its data
  and stops executing.
- `deploy.sh --execute`: safety preflight (retired writers absent/off, no
  ringing alarm, scheduler ready) and then starts the executor at 1 replica.
- Default reruns preserve the current executor mode/replicas; `--control-*`
  toggles only ha-control.
- Readiness: `GetStatus` shows a live `ha-bridge-home` consumer and the expected
  `enabledSchedules`; a delivered event is acknowledged only after HA returns
  2xx. Delivery is at-least-once, not proof of audibility.
- Emergency audio stop: HA script `alarm_stop` stops the current sound
  independently of scheduler ownership; canonical stop/snooze remains
  `StopSchedule`/`SnoozeSchedule`.

Database restore (current procedure): scale the scheduler to 0, restore the
files from the latest `backup.sh` output into the `scheduler-data` PVC, then
scale back to 1 and verify `GetStatus`.

## Bounded rollout (scheduler + bridge only)

`deploy.sh` is the full path: it also rolls `ha-control`, reinstalls the
NodePort filter. When a change only
touches `solutions/scheduler` / the alarm bridge, do not run it; stage and roll
just the two workloads:

```bash
# 1. build immutable linux/amd64 binaries and copy them to dist/ as deploy.sh does
bazel build //solutions/scheduler/cmd/scheduler //solutions/ha-bridge/cmd/ha-bridge \
  --platforms=@rules_go//go/toolchain:linux_amd64
# 2. stage under /opt/ricardo/releases/<build-id> with the node's sudo account,
#    verify sha256 of both binaries, then atomically switch /opt/ricardo/current
# 3. roll only the two deployments by patching the build annotation
kubectl -n home-assistant patch deployment/scheduler --type merge \
  -p '{"spec":{"template":{"metadata":{"annotations":{"ricardo.dev/build":"<build-id>"}}}}}'
kubectl -n home-assistant patch deployment/ha-bridge --type merge \
  -p '{"spec":{"template":{"metadata":{"annotations":{"ricardo.dev/build":"<build-id>"}}}}}'
```

Annotation patching preserves replicas, env, `hostPath` and PVCs. Update HA
scripts (`ha core check` + `script.reload`) **before** rolling the bridge when
the contract changes, and leave `ha-control` and the NodePort filter alone. Verify afterwards: rollout status, authenticated
`-healthcheck`, `GetStatus.ready`, live consumers, unchanged `ListSchedules`.

## CI deploy (GitHub Actions, environment `home`)

`.github/workflows/deploy-home.yml` deploys this stack from `develop` through
the home VPN. Push runs **preserve** only; `execute`, `shadow`,
`control-execute` and `control-catalog` are manual `workflow_dispatch` inputs
and still go through the normal `deploy.sh` preflight (no bypass, no forced
writer). `concurrency: deploy-home` is never cancelled, so there is still one
deploy at a time.

Least-privilege access:

- VPN certificate user `ci`, group `ci`, its own pool and a route only to the
  K3s node; the VPN firewall allows only `tcp/22` there.
- SSH user `ci-deploy` on the node: key only, `from` the VPN container
  address, `restrict,port-forwarding,permitopen="127.0.0.1:6443"`. It cannot
  run arbitrary root commands: `sudo -n /usr/local/sbin/ricardo-ci-deploy`
  exposes only `stage`, `firewall`, `activate` and `current`. The helper lives
  in `ci/ricardo-ci-deploy` and is provisioned by `ci/provision-node.sh`.
- `firewall` takes only values (`LAN_CIDR`, node address, two NodePorts) and
  renders the root-owned template `ci/scheduler-nodeport.nft.tmpl`, installed as
  `/usr/local/share/ricardo-ci/scheduler-nodeport.nft.tmpl`. CI never supplies
  nftables text, paths or `include` directives: CIDR/address are validated with
  Python `ipaddress`, ports must be `30000–32767`, the rendered ruleset is
  syntax-checked, and only `table inet ricardo_scheduler` is replaced.
- Kubernetes ServiceAccount `ci-deploy` in `home-assistant` (Role in
  `ci/rbac.yaml`): deployments/replicasets/pods/exec/services/configmaps/
  secrets/PVCs in that namespace only, plus `deployments/scale`. No other
  namespace, no cluster-scoped access. Long-lived token Secret
  `ci-deploy-token`; kubeconfig points at `https://127.0.0.1:16443` and is
  only usable through the SSH tunnel.

`deploy.sh` additions used by CI:

- `SSH_KEY`/`SSH_OPTS` select the deploy key without touching `~/.ssh/config`.
- `REMOTE_STAGE=/var/lib/ricardo-ci/stage` stages binaries through the fixed
  root helper instead of writing `/opt/ricardo` directly.
- `PREFLIGHT_REMOTE=1` runs the same `preflight.py` on the K3s node, so the
  runner never needs HA or NodePort access.

The workflow writes the secrets from the `home` Environment to files (never
into logs), opens the VPN and the SSH tunnel, runs the deploy in preserve mode
by default, verifies `scheduler`/`ha-control`/`ha-bridge` healthchecks and both
NodePorts, and on failure restores the previous release symlinks and rolls the
Deployments back. `if: always()` disconnects the VPN and removes the temporary
files. Secret file names are listed in `secrets/ci/github/README.txt`.

## Limits

- Single writer: never run two scheduler replicas on the same PVC.
- At-least-once delivery; a crash between the HA call and the ACK can repeat a
  side effect. Audibility is never claimed by the scheduler or the bridge.
- STOP for timer/reminder is acknowledged as `ACK_SKIPPED`; the announce is
  one-shot and a global stop could silence another alarm.
