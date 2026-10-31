# Scheduler deployment (home K3s)

Durable `ricardo.v1.SchedulerService` plus the Home Assistant executors
(`ha-bridge`, `ha-control`) on the single-node `lab-k3s` cluster.

- `solutions/scheduler` owns schedules, timers, occurrences, outbox and leases.
- `solutions/ha-bridge` leases the `home` queue and calls only allowlisted HA
  scripts (`alarm_start`, `schedule_announce`, `alarm_stop`). It never owns
  schedule state.
- `ha-control` leases only the `control` queue and serves the phone catalog.
- Home Assistant performs audio only. Voice intents in
  `projects/home-assistant/ha-voice` call the canonical API.

## Images

Each service runs as its own distroless image, built hermetically by Bazel and
referenced by digest in the manifests:

```text
projects/home/kubernetes/scheduler/BUILD.bazel
  :scheduler_image   :scheduler_load   :scheduler_push    ghcr.io/proflyder/scheduler
  :ha-bridge_image   :ha-bridge_load   :ha-bridge_push    ghcr.io/proflyder/ha-bridge
  :ha-control_image  :ha-control_load  :ha-control_push   ghcr.io/proflyder/ha-control
```

- `rules_oci` assembles one static linux/amd64 Go binary on
  `gcr.io/distroless/static-debian12:nonroot`, pinned by digest in
  `MODULE.bazel`. No Docker daemon and no `RUN` steps are involved, so the same
  targets build on macOS and on the GitHub runner.
- `platform/ci/oci_image.py` reads the resulting OCI layout: it prints the
  manifest digest, writes an OCI archive for `ctr images import` or a
  docker archive for local smoke tests. The image is referenced as
  `ghcr.io/proflyder/<name>@sha256:...`, never by a moving tag.
- The image has no shell. Probes and deploy checks run the binary itself:
  `-healthcheck` (authenticated readiness) and `-status` (raw GetStatus JSON).
- Pods keep `runAsUser/fsGroup 65534`, matching the existing SQLite and ledger
  file owners on the `local-retain` PVCs, so the migration needs no chown.

## Files

```text
manifests/scheduler.yaml        PVC + Deployment + NodePort Service (template)
manifests/bridge.yaml           ledger PVC + executor Deployment (template)
manifests/control.yaml          ha-control Deployment (template)
manifests/control-service.yaml  ha-control ClusterIP + NodePort
manifests/preflight-job.yaml    in-cluster cutover preflight Job (template)
nft/scheduler-nodeport.nft      NodePort source filter (template, host config)
nft/apply.sh                    install/refresh that filter as root on the node
scheduler.env.example           placeholders; copy to secrets/home/scheduler.env
render.py                       strict ${VAR} renderer (unknown/empty => fail)
deploy.sh                       build/push/import images, apply, mode transitions
backup.sh                       quiesced SQLite + ledger backup with checksums
preflight.py                    safety preflight (retired writers absent/off, scheduler ready)
rollback.py                     restore exactly the Deployments this deploy applied
ci/rbac.yaml                    ServiceAccount/Role for CI kubectl
```

Tracked templates contain only placeholders/TEST-NET values. Real addresses,
CIDRs, NodePorts and image digests are rendered at deploy time; rendered
manifests go to `secrets/local/rendered/scheduler` (git-ignored).

## One-time prerequisites

```bash
kubectl -n home-assistant create secret generic scheduler-auth \
  --from-file=token=secrets/home/scheduler.token
kubectl -n home-assistant create secret generic ha-bridge-ha \
  --from-file=token=secrets/home/ha.token
kubectl -n home-assistant apply -f projects/home/kubernetes/scheduler/ci/rbac.yaml
```

Both tokens are read from files at runtime (`SCHEDULER_TOKEN_FILE`,
`HA_TOKEN_FILE`) and never appear in process arguments, probe arguments or
Kubernetes events.

The NodePort source filter is host configuration, applied once (and after any
CIDR/port change) as root on the node:

```bash
ssh <node> sudo env PRIVATE_ENV=/path/to/scheduler.env \
  bash projects/home/kubernetes/scheduler/nft/apply.sh
```

## Deploy (manual, from the workstation)

`deploy.sh` builds the images, imports them into the node's containerd over SSH
(no registry push), renders the manifests with the new digests and applies
them. `NODE_SSH` is the K3s node; `KUBECONFIG` must reach the API (LAN, or the
documented SSH tunnel).

```bash
NODE_SSH=labuser@node KUBECONFIG=/path/kubeconfig \
  ./projects/home/kubernetes/scheduler/deploy.sh                   # preserve modes
NODE_SSH=labuser@node ./projects/home/kubernetes/scheduler/deploy.sh --shadow
HA_TOKEN_FILE=secrets/home/ha.token NODE_SSH=labuser@node \
  ./projects/home/kubernetes/scheduler/deploy.sh --execute         # preflight, then 1 replica
./projects/home/kubernetes/scheduler/deploy.sh --only scheduler    # one unit only
```

- Re-running without `--execute`/`--shadow` preserves the bridge/control modes
  and replica counts; it only rolls the new image digest.
- `--only` selects deployable groups (`scheduler`, `ha-bridge` which also rolls
  `ha-control`, `ha-control`); the default is `all`.
- `--execute` runs the preflight first and refuses if a legacy HA writer is on,
  an alarm is ringing, the scheduler is not ready or the HA endpoint differs.
- The preflight runs locally by default; `PREFLIGHT_IN_CLUSTER=1` runs it as a
  one-shot Job next to the scheduler (used by CI, which cannot reach HA).
- Pushing instead of importing (`PUSH_IMAGES=1`, needs `docker login ghcr.io`)
  is the CI path; the digest is verified against the built image before apply.

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
  scripts (alarm helpers, RPC wrappers) stay hidden. `CONTROL_LABEL` was
  removed and the process refuses to start if it is still set.
- `CONTROL_EXECUTE=false` serves the catalog only. `deploy.sh --control-execute`
  runs a fail-closed preflight (refuses when GetStatus reports pending/leased
  events on the `control` queue) and only then enables execution;
  `--control-catalog` disables it again. Default reruns preserve the current
  execute mode and replica count. Regression: `test_deploy_control.sh`.
- Delivery is at-least-once: a crash between the HA call and the ACK can repeat
  a command. The catalog id/action payload is authorization-checked per run,
  but the executor is not exactly-once.
- HA's `repeat` delivery for `control` shares the scheduler event lease; the
  scheduler allowlist must include both `ha-bridge-home` and `ha-control-home`.

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
- `nft/scheduler-nodeport.nft` drops the NodePort in `prerouting` before
  kube-proxy DNAT unless the source is the home LAN or the pod CIDR; IPv6
  NodePort is dropped. Persistence: `ricardo-scheduler-firewall.service` on the
  node boots from `/etc/ricardo-scheduler-nodeport.nft`.
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

Database restore: scale the scheduler to 0, restore the files from the latest
`backup.sh` output into the `scheduler-data` PVC, then scale back to 1 and
verify `GetStatus`.

## Bounded rollout

`deploy.sh` is the full path: it also rolls `ha-control`. When a change only
touches one service, use `--only`:

```bash
./projects/home/kubernetes/scheduler/deploy.sh --only scheduler
```

## CI deploy (GitHub Actions, environment `home`)

`.github/workflows/deploy-home.yml` deploys this stack from `develop` with
kubectl only — no SSH and no root helper.

- `plan` routes the pushed range (`before..sha`) through the CI unit map
  (`platform/ci/units.yml`) and produces the affected **deployable** units
  (`deploy.images` / `deploy.workloads`). A documentation-only push deploys
  nothing; a shared Go package change deploys every importer; an unknown or
  zero `before` falls back to every deployable unit.
- The deploy matrix runs affected units in parallel; each unit is
  serialised by `concurrency: deploy-home-<unit>` and never cancelled.
  Image-only units (Hermes/voice-proxy) skip the VPN and the cluster.
- Each job logs in to GHCR with the job-scoped `GITHUB_TOKEN`
  (`packages: write` on that job only), runs `deploy.sh` with `PUSH_IMAGES=1`
  and `PREFLIGHT_IN_CLUSTER=1`, then verifies rollout, `-healthcheck`,
  `-status` and both NodePorts.
- Images are pushed with the `rules_oci` `oci_push` targets (crane), not
  `docker push`: crane stores the exact OCI manifest that was built, so the
  registry digest equals the built digest. `deploy.sh` fails if they differ —
  this is what "deploy exactly the pushed digest" means. (A `docker push`
  rewrites the manifest into Docker media types and can add an attestation
  index, which changes the digest.)
- Rollback is driven by recorded state, not `kubectl rollout undo`.
  `deploy.sh` captures each Deployment before it changes it
  (`<name>.before.json`, or `<name>.absent`) and writes `<name>.applied` only
  after the apply/patch succeeds. `rollback.py` restores the remembered spec
  with a merge patch, and only when the generation actually changed. A failure
  during build, push, rendering or preflight leaves no marker, so the live
  Deployments are untouched.
- Push runs preserve mode only; `execute`, `shadow`, `control-execute`
  and `control-catalog` are manual `workflow_dispatch` inputs for the
  `ha-bridge` unit and still go through the fail-closed preflight.

Least-privilege access:

- VPN certificate user `ci`, group `ci`, its own pool and a route only to the
  K3s node; the VPN firewall allows only `tcp/6443` there (no SSH).
- The K3s node firewall (`ricardo_k3s`) accepts `6443` only from the VPN
  container address, which masquerades the isolated `ci` pool.
- Kubernetes ServiceAccount `ci-deploy` in `home-assistant` (Role in
  `ci/rbac.yaml`): deployments/replicasets/pods/exec/services/configmaps/
  secrets/PVCs in that namespace only, plus `deployments/scale` and one-shot
  `jobs` for the preflight. No other namespace, no cluster-scoped access.
- The kubeconfig points at `https://<K3S_NODE>:6443`; the API certificate
  already carries the node IP in its SAN, so no k3s restart or `tls-san` change
  is needed.
- The cutover preflight runs as a Job in `home-assistant`, so the runner needs
  neither HA access nor the HA/scheduler tokens.

## Limits

- Single writer: never run two scheduler replicas on the same PVC.
- At-least-once delivery; a crash between the HA call and the ACK can repeat a
  side effect. Audibility is never claimed by the scheduler or the bridge.
- STOP for timer/reminder is acknowledged as `ACK_SKIPPED`; the announce is
  one-shot and a global stop could silence another alarm.
