# Scheduler deployment (home K3s)

Durable `home.v1.SchedulerService` plus the Home Assistant executors
(`ha-bridge`, `ha-control`, `ha-scenario`) on the single-node `lab-k3s` cluster.

- `solutions/scheduler` owns schedules, timers, occurrences, outbox and leases.
- `solutions/ha-bridge` leases the `home` queue and calls only allowlisted HA
  scripts (`alarm_start`, `schedule_announce`, `alarm_stop`). It never owns
  schedule state.
- `ha-control` leases only the `control` queue and serves the phone catalog.
- `ha-scenario` leases only the `scenario` queue, reads definitions through
  `ScenarioService/GetScenario`, evaluates conditions and runs catalog/speak/
  delay steps. It owns a separate ledger and consumer id; a STOP cancels a
  running delay.
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
  :ha-scenario_image :ha-scenario_load :ha-scenario_push  ghcr.io/proflyder/ha-scenario
```

- `rules_oci` assembles one static linux/amd64 binary on
  `gcr.io/distroless/static-debian12:nonroot`, pinned by digest in
  `MODULE.bazel`. The scheduler binary comes from the Rust crate
  `//solutions/scheduler/crates/scheduler:scheduler` via `rules_rust` (static
  musl); `ha-bridge`, `ha-control` and `ha-scenario` are the binaries of
  `//solutions/ha-bridge/crates/ha-bridge`, also rebuilt for static musl. The
  former Go implementations are deleted. No Docker daemon and no `RUN` steps
  are involved, so the same targets build on macOS and on the GitHub runner.
- `home-deploy oci` (`platform/deploy-rs`, decision 35) reads the resulting OCI
  layout: its `digest` prints the manifest digest, `pack` writes an OCI archive
  for `ctr images import` and `docker` a docker archive for local smoke tests. The image is
  referenced as `ghcr.io/proflyder/<name>@sha256:...`, never by a moving tag.
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
manifests/scenario.yaml         ha-scenario ledger PVC + executor Deployment
manifests/preflight-job.yaml    in-cluster cutover preflight Job (template)
manifests/external-secrets.yaml ExternalSecrets for scheduler-auth/ha-bridge-ha
scenarios/                      household scenario definitions + idempotent seed
nft/scheduler-nodeport.nft      NodePort source filter (template, host config)
nft/apply.sh                    install/refresh that filter as root on the node
scheduler.env.example           placeholders; copy to secrets/home/scheduler.env
deploy.sh                       build/push/import images, apply, mode transitions
                                (templates rendered by `home-deploy render`)
backup.sh                       quiesced SQLite + ledger backup with checksums
                                (preflight/state/converge: `home-deploy`, platform/deploy-rs)
```

Tracked templates contain only placeholders/TEST-NET values. Real addresses,
CIDRs, NodePorts and image digests are rendered at deploy time; rendered
manifests go to `secrets/local/rendered/scheduler` (git-ignored).

## One-time prerequisites

```bash
# Secrets are owned by External Secrets Operator and sourced from OpenBao
# (secret/k8s/home-assistant/{scheduler-auth,ha-bridge-ha}). Apply once:
mise run admin:external-secrets
```

Both tokens are read from files at runtime (`SCHEDULER_TOKEN_FILE`,
`HA_TOKEN_FILE`) and never appear in process arguments, probe arguments or
Kubernetes events. Local files `secrets/home/*.token` remain a backup and for
the manual preflight only; they no longer create the Secrets.

The NodePort source filter is host configuration, applied once (and after any
CIDR/port change) as root on the node:

```bash
ssh <node> sudo env PRIVATE_ENV=/path/to/scheduler.env \
  bash projects/home/kubernetes/scheduler/nft/apply.sh
```

## Deploy (Flux; manual script for drills)

Flux applies the stack (`Kustomization/app-ha-executors`) from `develop` with
digests from `deploy/home-apps` and steady-state modes/replicas pinned in
`manifests/kustomization.yaml`. `deploy.sh` remains the admin tool for mode
transitions, cutover preflight and local debugging: it builds the images,
imports them into the node's containerd over SSH (no registry push), renders the
manifests with the new digests and applies them. Private values come from
`PRIVATE_ENV` (default `secrets/home/scheduler.env`, git-ignored);
`KUBECONFIG` must reach the API (LAN or the documented SSH tunnel).

```bash
NODE_SSH=labuser@node KUBECONFIG=/path/kubeconfig \
  ./projects/home/kubernetes/scheduler/deploy.sh                   # preserve modes
NODE_SSH=labuser@node ./projects/home/kubernetes/scheduler/deploy.sh --shadow
HA_TOKEN_FILE=secrets/home/ha.token NODE_SSH=labuser@node \
  ./projects/home/kubernetes/scheduler/deploy.sh --execute         # preflight, then 1 replica
PRIVATE_ENV=secrets/home/scheduler.env KUBECONFIG=/path/kubeconfig \
  ./projects/home/kubernetes/scheduler/deploy.sh --only scheduler   # one group
```

- Re-running without `--execute`/`--shadow` preserves the bridge/control/
  scenario modes and replica counts; it only rolls the new image digest.
- `--only` selects deployable groups (`scheduler`, `ha-bridge` which also rolls
  `ha-control` and `ha-scenario`, `ha-control`); the default is `all`.
- `--execute` runs the preflight first and refuses if a legacy HA writer is on,
  an alarm is ringing, the scheduler is not ready or the HA endpoint differs.
- The preflight runs locally by default; `PREFLIGHT_IN_CLUSTER=1` runs it as a
  one-shot Job next to the scheduler (works from a workstation that cannot reach HA).
- `PUSH_IMAGES=1` (needs `docker login ghcr.io`) pushes instead of importing
  locally; the digest is verified against the built image before apply. CI never
  calls this script: it only pins the verified digest.

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

## ha-scenario

The durable executor of `home.v1.ScenarioService` definitions. Scheduler stores
and validates them and emits `scenario.fire` / `scenario.stop` on the
dedicated `scenario` queue; this process owns the `ha-scenario-home` consumer
and a separate `/data/scenario-ledger.json` on PVC `ha-scenario-data`.

- Starts with `SCENARIO_EXECUTE=true`, 1 replica. There are no scenarios at
  roll-out time, so the queue is idle.
- Conditions: `entity_state` (exact HA state) and `time_window` (timezone of
  the first time trigger or `SCENARIO_TIMEZONE`, window across midnight,
  ISO weekdays). Not met → `SKIPPED`.
- Steps run in order against the live catalog: `catalog` (`entry`/`action`/
  `params_json`), `speak` (same `script.speak_text` path as
  `action.station_speak`), `delay` (cancelled by STOP). `continue_on_error`
  decides whether the first failure stops the run; the ack detail is `step N:
  <reason>` and never contains speak text.
- Retries without an ack only before the first step (scheduler/HA/catalog
  unavailable). After a step, a failure is terminal; the ledger marks
  `started` before the first step, so an interrupted run is acked `FAILED
  interrupted` instead of being replayed.
- `SCENARIO_LEASE_SECONDS` (default 3600) must cover a whole run: an ack after
  the lease expired is rejected by the scheduler. A single delay step is capped
  at 3600 s by the proto contract.
- One scenario never runs twice at once: a second FIRE is `SKIPPED` «already
  running». `GetStatus.scenario_execution_supported` is the scheduler
  `SCENARIO_EXECUTION=true` flag, set in `manifests/scheduler.yaml`.
- Disable: `mise run kubectl -- -n home-assistant scale deployment/ha-scenario --replicas=0`
  or `set env ... SCENARIO_EXECUTE=false`; reruns preserve both.

Household definitions («Блэкаут», «Сел работать») live in
`scenarios/*.json` and are seeded idempotently by label:

```bash
SCHEDULER_URL=http://<node>:30098 SCHEDULER_TOKEN_FILE=secrets/home/scheduler.token \
  node projects/home/kubernetes/scheduler/scenarios/seed.mjs [--dry-run]
node --test projects/home/kubernetes/scheduler/scenarios/seed.test.mjs
```

The seed reads `ListScenarios`, creates only labels that are still missing
(`idempotencyKey=seed-<slug>`) and never overwrites an existing definition,
because the owner may edit it in the app. Renaming a scenario label breaks the
voice command: HA resolves scenarios by exact label, case-insensitively.

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
- `nft/scheduler-nodeport.nft` drops the scheduler, control and intercom
  NodePorts in `prerouting` before kube-proxy DNAT unless the source is the
  home LAN or the pod CIDR; IPv6 NodePort is dropped. The private env file
  needs `SCHEDULER_NODE_PORT`, `CONTROL_NODE_PORT` and `INTERCOM_NODE_PORT`. Persistence: `home-scheduler-firewall.service` on the
  node boots from `/etc/home-scheduler-nodeport.nft`.
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
  `local-data/local/backups/scheduler/<timestamp>`; never inside the repo tree.

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
verify `GetStatus`. Backups are schema-agnostic whole-file copies: a v1 copy
restored into this release migrates on startup. Rolling the binary back after
the database has migrated requires restoring a v1 copy, because the older
binary rejects schema v2.

## Bounded rollout

`deploy.sh` is the full path: it also rolls `ha-control` and `ha-scenario`.
When a change only touches one service, use `--only`:

```bash
PRIVATE_ENV=secrets/home/scheduler.env KUBECONFIG=/path/kubeconfig \
  ./projects/home/kubernetes/scheduler/deploy.sh --only scheduler
```

## Flux and CI

`.github/workflows/deploy-home.yml` no longer touches the cluster. On a push to
`develop` the `pins` job promotes the verified image digests into the
`deploy/home-apps` branch; `Kustomization/app-ha-executors` applies
`manifests/kustomization.yaml` from `develop` with those digests and waits for
the workloads. There is no VPN, OpenBao, kubeconfig or `ci-deploy` on that path.

- `plan` routes the pushed range (`before..sha`) through the CI unit map
  (`platform/ci/units.mjs`) and selects the affected deployable units. A
  documentation-only push pins nothing; a shared proto/Rust package change
  selects its importers; an unknown or zero `before` falls back to every
  deployable unit.
- `pins` runs per unit with `max-parallel: 1` (a job-level concurrency group would cancel the queued matrix jobs); image promotion
  verifies the `tree-<hash>` tag, provenance and digest read-back, then commits
  only that unit's keys. A failed promotion keeps the previous pin (fail
  closed) and the old workload keeps running.
- Steady state (one replica each, `BRIDGE_EXECUTE=true`,
  `CONTROL_EXECUTE=true`, `SCENARIO_EXECUTE=true`) lives in the Flux patch.
  Mode changes are a Git change; a drill uses
  `flux suspend kustomization/app-ha-executors` plus `deploy.sh`, then resume.
- Rollback is a revert of the pins commit (or `flux suspend` first). Do not use
  `kubectl rollout undo`: it bypasses Git and loses the audit trail.
- `test_deploy_control.sh` / `_deploy-control.yml` stay as the mocked
  `deploy.sh --control-*` regression; the admin script is still what a drill
  runs.

Access: Flux runs in-cluster with its own ServiceAccount; there is no CI
kubeconfig anymore. Host-side NodePort filtering remains manual (`nft/apply.sh`),
and the cutover preflight still runs as a Job (`preflight-job.yaml`,
`PREFLIGHT_IN_CLUSTER=1`) or locally with HA/scheduler token files.

## Limits

- Single writer: never run two scheduler replicas on the same PVC.
- At-least-once delivery; a crash between the HA call and the ACK can repeat a
  side effect. Audibility is never claimed by the scheduler or the bridge.
- STOP for timer/reminder is acknowledged as `ACK_SKIPPED`; the announce is
  one-shot and a global stop could silence another alarm.
