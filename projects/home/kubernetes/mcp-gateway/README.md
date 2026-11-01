# mcp-gateway deployment (home K3s)

Kubernetes deployment of `solutions/mcp-gateway`: the single MCP gateway that
Hermes uses for infrastructure diagnostics, the two-step rollout restart and
Telegram reports.

- Solution: `solutions/mcp-gateway` (Go, distroless, one binary).
- Project: this directory — manifests, catalog, RBAC, secrets wiring, deploy.
- Namespace: `home-assistant`; one Deployment, one ClusterIP Service, no
  ingress and no NodePort.
- CI unit: `mcp-gateway` (Go + image + deploy) and `mcp-gateway-deploy`
  (catalog/RBAC/deploy contract).

```text
Hermes (hostNetwork, no kubectl)
   │  POST /mcp  Bearer (HERMES_INFRA_TOKEN)
   ▼
Service mcp-gateway:8080 ──► Deployment mcp-gateway
   │                          ├─ Prometheus  monitoring-prometheus:9090
   │                          ├─ Alertmanager monitoring-alertmanager:9093
   │                          ├─ Loki         loki:3100
   │                          ├─ Kubernetes API (ServiceAccount + RBAC)
   │                          └─ Telegram Bot API (fixed recipient)
   └─ /healthz /readyz /metrics (cluster-local only)
```

## Files

```text
manifests/deployment.yaml         Deployment + ClusterIP Service (template)
manifests/catalog.yaml            service catalog + sanitized topology overview
manifests/rbac.yaml               ServiceAccount + per-namespace read/restart Roles
manifests/external-secrets.yaml   mcp-gateway-auth, mcp-gateway-telegram, mcp-gateway-ha
manifests/network-policies.yaml   ingress from Hermes/monitoring, narrow egress
manifests/servicemonitor.yaml     Prometheus scrape of /metrics
ci/rbac.yaml                      extra ci-deploy verbs (NetworkPolicy, ServiceMonitor)
deploy.sh                         build/push/import image, render, apply, roll out
render.py                         shared scheduler renderer + MCP_GATEWAY_IMAGE
mcp-gateway.env.example           LAN_CIDR / NODE_LAN_IP placeholders
test_deploy.py                    catalog↔RBAC, manifests, render, deploy contract
catalog_contract_test.go          loads the tracked catalog with the real loader
```

Rendered manifests go to `secrets/local/rendered/mcp-gateway` (git-ignored).
Tracked files contain only placeholders/TEST-NET values.

## Catalog is the allow-list

`manifests/catalog.yaml` lists every service the gateway may read and — when
`restart.allowed` — restart. It is applied verbatim as the ConfigMap
`mcp-gateway-catalog` and mounted at `/etc/mcp-gateway/catalog.yaml`.

- Spoken aliases (ru/en) must be unambiguous; `infra_prepare_restart` resolves
  them to a catalog id, never to free text.
- Signals with `status: true` describe the **bad** state (`... == 0`,
  `... < 1`, `... > 0`); any non-empty result is a problem.
- `ignore_alerts` lists alerts that are always active by design (`Watchdog`,
  `InfoInhibitor`); `infra_status` reports them separately as
  `details.ignoredAlerts` and never counts or speaks them as a problem.
- Logs: `loki` only for namespaces Alloy ingests (`monitoring`,
  `currency-bot`, `openbao`, `kube-system`), `pods` for everything else
  (`pods/log` through the API), `none` for HAOS/MA/station. `logs.containers`
  is deliberately empty: the gateway asks the workload for its real container
  names, so a renamed container cannot silently drop logs. Loki selectors do
  use the `container` label Alloy attaches; if the live Alloy keep-regex is
  narrower than the repo values, those queries return nothing and the service
  must switch to `pods` (see Open risks).
- `restart.allowed: true` for every repository-managed Kubernetes workload,
  each with an honest Russian warning. `hermes`, `mcp-gateway` are
  `self_disrupting: true`, so the patch is deferred ~20 s and the reply is
  spoken first. The two operator-managed monitoring StatefulSets keep their
  live names — `prometheus-monitoring-prometheus` and
  `alertmanager-monitoring-alertmanager` — and their warnings say the
  Prometheus Operator owns them and may roll the pods a second time.
- External targets (HAOS, Music Assistant, station) are read-only: the gateway
  cannot restart anything outside Kubernetes. Their availability comes from
  catalog `checks:`: HA REST (`http` with `auth: ha` plus
  `ha_unavailable_summary`), MA TCP + web root, and station TCP ports plus
  `ha_state`/`ha_template` for the ESPHome entities and the Pulsar version.
  The catalog, the Deployment and the NetworkPolicy carry `${HAOS_IP}` /
  `${STATION_IP}`; `deploy.sh` renders them from the private env, so tracked
  files never contain real host addresses.
- `infra_report` is the full-report tool used by the Telegram `/report` skill.
  The text is cognitive-first: a header, an emoji verdict, then «Проблемы»
  (one block per failing service with ready/restarts/failing check/entities),
  «В норме» (one short line per catalog `group`), and «Алерты» only when
  active. Images, age and versions stay in the structured `details` JSON;
  `verbose: true` appends a compact «Версии» section. The text is capped at
  3500 characters. `title` replaces the first alias everywhere a human name is
  shown, and `group_order` fixes the section order.
- Human-facing clock times use `MCP_TIMEZONE` (the Deployment sets
  `Asia/Almaty`, matching the Alertmanager Telegram template); the header is
  rendered in that zone, while `details.checkedAt` stays UTC RFC3339. The
  binary embeds IANA tzdata, so the distroless image needs no zoneinfo files.
- `containers[0]` names the container whose image represents a workload
  (`prometheus`, `alertmanager`, …), so a config-reloader sidecar cannot
  become the reported image. Without it the gateway matches the workload name
  or its last dash-segment, then falls back to pod spec order — never to the
  alphabetical `containerStatuses` order.

Rendered sample (all healthy):

```text
Отчёт по инфраструктуре, 01.11 19:52
✅ Всё работает: 30 из 30

В норме
✅ Голос: Hermes, GigaAM, Whisper, Piper, Runtime Health Exporter
✅ Дом: Home Assistant, Music Assistant, Станция Midi (Pulsar)
✅ Планировщик: Scheduler, HA Bridge, HA Control
✅ Мониторинг (8)
✅ Секреты и сеть (8)
✅ Приложения: MCP Gateway, Currency Bot, Cloudflared
```

With problems the failing services come first, for example:

```text
Отчёт по инфраструктуре, 01.11 19:52
⚠️ Есть проблемы: 2 из 30

Проблемы
❌ Home Assistant — недоступно сущностей: 1 (media_player.kitchen)
❌ Prometheus — готовы 0 из 1
   рестартов 1ч: 0

В норме
✅ Голос: Hermes, GigaAM, Whisper, Piper, Runtime Health Exporter
...
```

The catalog and RBAC are checked against each other: `patch` resourceNames in
`manifests/rbac.yaml` must equal the restartable catalog workloads
(`test_deploy.py::test_write_resource_names_equal_catalog_restartable_workloads`).
`catalog_contract_test.go` builds the real binary and proves the tracked
catalog loads (and that an invalid one refuses to start).

## RBAC

- ServiceAccount `home-assistant/mcp-gateway`; no ClusterRole.
- `mcp-gateway-read` in `home-assistant`, `monitoring`, `currency-bot`,
  `openbao`, `external-secrets`, `traefik`, `cert-manager`, `kube-system`:
  `get/list/watch` on pods, pods/log, events, services, deployments,
  statefulsets, daemonsets, replicasets.
- `mcp-gateway-restart` only in namespaces with restartable workloads, verb
  `patch` only, restricted with `resourceNames`.
- `kube-system` is read-only; no repository-managed workload lives there.
- Hermes keeps `automountServiceAccountToken: false`; all cluster access goes
  through this pod.

## Network

- Ingress to 8080: the node address (`NODE_LAN_IP`), the LAN CIDR, the pod CIDR
  (`10.42.0.0/16`) and the `monitoring` namespace (scrape). Hermes is
  `hostNetwork`, so its packets carry the node address and cannot be selected
  by a pod selector; the same applies to kubelet probes. The Service is
  ClusterIP, so this only exposes the gateway to the node and the cluster, and
  `/mcp` still requires the bearer token.
- Egress: kube-API 6443 (service CIDR `10.43.0.0/16` and the node), monitoring
  9090/9093/3100, external checks on the `${LAN_CIDR}` (HAOS 8123/8095, station
  6053/8928/8099), DNS 53 (UDP/TCP), Telegram 443. Telegram publishes no
  stable ranges, so 443 is any-destination — the same trade-off as
  currency-bot.
- K3s (kube-router) enforces NetworkPolicies. Prefer allowing over breaking:
  if the CNI ever stops attributing hostNetwork sources to the node address,
  the ingress rules still cover the pod CIDR.

## One-time prerequisites (operator)

Never paste secret values into chat or a commit. Use the private paths below.

1. **Gateway↔Hermes bearer token** (value never printed):

   ```bash
   export KUBECONFIG=/private/path/kubeconfig
   bao kv put secret/k8s/home-assistant/mcp-gateway-auth \
     token="$(openssl rand -hex 32)"
   ```

   The same value is read by Hermes as `HERMES_INFRA_TOKEN`
   (`ExternalSecret/hermes-environment` gains a `data` entry reading
   `k8s/home-assistant/mcp-gateway-auth`, key `token`). Rotating it means
   patching the OpenBao key and restarting both pods.

2. **Telegram bot keys** — one dedicated infra bot whose owner already pressed
   `/start`. Keep them in a private env file, then patch OpenBao without
   echoing:

   ```bash
   # secrets/local/hermes-telegram.env (mode 600, git-ignored)
   #   TELEGRAM_BOT_TOKEN=<new infra bot token>
   #   TELEGRAM_ALLOWED_USERS=<owner numeric user id>   # also the DM chat id
   python3 - <<'PY'
   import json, pathlib, subprocess
   wanted = {"TELEGRAM_BOT_TOKEN", "TELEGRAM_ALLOWED_USERS"}
   data = {}
   for line in pathlib.Path("secrets/local/hermes-telegram.env").read_text().splitlines():
       line = line.strip()
       if not line or line.startswith("#"):
           continue
       key, _, value = line.partition("=")
       if key in wanted:
           data[key] = value.strip().strip('"').strip("'")
   missing = wanted - data.keys()
   if missing:
       raise SystemExit(f"missing keys: {sorted(missing)}")
   subprocess.run(
       ["bao", "kv", "patch", "secret/k8s/home-assistant/hermes-environment", "-"],
       input=json.dumps(data).encode(), check=True)
   print("patched keys:", ", ".join(sorted(data)))
   PY
   ```

   The gateway reads `TELEGRAM_BOT_TOKEN` and takes the first entry of
   `TELEGRAM_ALLOWED_USERS` as the fixed report recipient. The currency-bot bot
   stays dedicated to currency-bot.

3. **Home Assistant REST token** — no new secret: `ExternalSecret/mcp-gateway-ha`
   reads the same OpenBao key as the scheduler bridge,
   `secret/k8s/home-assistant/ha-bridge-ha`, property `token`
   (`HA_TOKEN_FILE` in `ha-bridge`). Rotating the HA token means patching that
   one key and restarting `ha-bridge` and `mcp-gateway`.

4. **Admin manifests** (cluster/namespace objects, applied once):

   ```bash
   kubectl apply -f projects/home/kubernetes/mcp-gateway/manifests/rbac.yaml
   kubectl apply -f projects/home/kubernetes/mcp-gateway/ci/rbac.yaml
   kubectl apply -f projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml
   kubectl -n home-assistant wait --for=condition=Ready \
     externalsecret/mcp-gateway-auth externalsecret/mcp-gateway-telegram \
     externalsecret/mcp-gateway-ha --timeout=120s
   ```

   `ci/rbac.yaml` only adds what the scheduler Role for `home-assistant`
   lacks: NetworkPolicy and ServiceMonitor, plus `get` on `services/proxy` for
   the gateway's Service so the post-deploy smoke can read `/readyz`.

5. **Deploy-time env** in `secrets/home/mcp-gateway.env` (copy the example):

   ```bash
   LAN_CIDR=<LAN CIDR>
   NODE_LAN_IP=<K3s node address>
   HAOS_IP=<HAOS VM address>
   STATION_IP=<station address>
   ```

   CI reads the same text from `secret/ci/deploy-home` key
   `mcp_gateway_env`:

   ```bash
   # one-time, content from the private file, value not printed
   bao kv patch secret/ci/deploy-home \
     mcp_gateway_env="$(cat secrets/home/mcp-gateway.env)"
   ```

6. **ExternalSecrets in the ESO apply set.** `external-secrets/deploy.sh`
   keeps an explicit list of ExternalSecret manifests. Until
   `projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml` is
   added to that list (a separate change outside this unit), apply it with the
   command in step 4. The CI job fails closed if any ExternalSecret is
   missing or not Ready.

## Deploy

Manual (workstation, imports the image into the node over SSH):

```bash
export KUBECONFIG=/private/path/kubeconfig
NODE_SSH=labuser@node ./projects/home/kubernetes/mcp-gateway/deploy.sh
```

CI (`deploy-home`, push to `develop`): the `mcp-gateway` unit comes from
`platform/ci/units.yml`; the image is built/pushed by Bazel, then `deploy.sh`
runs with `PUSH_IMAGES=1` and `PRIVATE_ENV=$CI_DIR/mcp-gateway.env`.

`deploy.sh` is idempotent and:

1. builds `//solutions/mcp-gateway:mcp-gateway_image` and resolves the digest
   (never a moving tag);
2. renders the manifests (no half-rendered input is possible);
3. verifies the ServiceAccount and all three ExternalSecrets exist and are Ready;
4. renders `manifests/catalog.yaml` (host placeholders) and applies it as the
   catalog ConfigMap;
5. applies NetworkPolicies and the ServiceMonitor;
6. converges the Deployment and waits for the rollout;
7. runs the binary's `-healthcheck` through `kubectl exec`, which loads the
   mounted token/catalog/groups and calls `/healthz` on loopback.

Checks:

```bash
kubectl -n home-assistant get deploy,pod,svc,externalsecret -l app.kubernetes.io/name=mcp-gateway
kubectl -n home-assistant logs deploy/mcp-gateway --tail=50
kubectl -n home-assistant get --raw \
  "/api/v1/namespaces/home-assistant/services/http:mcp-gateway:8080/proxy/readyz"
kubectl -n home-assistant exec deploy/mcp-gateway -- /usr/local/bin/mcp-gateway -healthcheck
kubectl auth can-i --as=system:serviceaccount:home-assistant:mcp-gateway \
  patch deployment/scheduler -n home-assistant
```

## Restart policy (voice)

- `infra_prepare_restart` returns a Russian question with the catalog warning;
  `infra_confirm` executes one pending action (TTL 90 s). Cooldown 5 min per
  workload, read from the object annotation `mcp-gateway/last-restart`.
- `self_disrupting` targets (`hermes`, `mcp-gateway`) defer the patch 20 s.
- The gateway never deletes, scales, execs or applies anything; the only verb
  is the rollout-restart annotation patch.
- `mcp-gateway` restarting itself is expected to interrupt the current reply —
  that is why the warning says so and the patch is deferred.

## Rollback

`deploy.sh` records the Deployment before changing it and writes an `.applied`
marker after a successful apply. CI restores exactly those objects:

```bash
python3 projects/home/kubernetes/scheduler/rollback.py \
  --state-dir secrets/local/rendered/mcp-gateway/state \
  --namespace home-assistant
```

Support objects (ConfigMap, NetworkPolicy, ServiceMonitor, ExternalSecrets)
are idempotent and not rolled back; re-apply the previous commit if needed.
Full removal:

```bash
kubectl -n home-assistant delete deployment,service mcp-gateway
kubectl -n home-assistant delete configmap mcp-gateway-catalog
kubectl -n home-assistant delete networkpolicy mcp-gateway-ingress mcp-gateway-egress
kubectl -n home-assistant delete servicemonitor mcp-gateway
kubectl delete -f projects/home/kubernetes/mcp-gateway/manifests/rbac.yaml
kubectl delete -f projects/home/kubernetes/mcp-gateway/ci/rbac.yaml
kubectl delete -f projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml
```

## Open risks

- HAOS, Music Assistant and the station have no Kubernetes metrics; their
  status now comes from external catalog checks (HA REST, MA TCP/HTTP, station
  TCP and HA entities). A check proves reachability, not application-level
  correctness, and `ha_unavailable_summary` is limited to the configured
  domains to avoid noise from permanently unavailable entities.
- `TELEGRAM_ALLOWED_USERS` must exist in `hermes-environment`; until the
  Hermes/Telegram rollout writes it, `mcp-gateway-telegram` stays not Ready and
  the deploy fails closed on purpose.
- Restarting `traefik`, `openbao` or a monitoring component is technically
  possible after confirmation and briefly degrades the whole home; the
  warnings state the consequence, and RBAC cannot express "but not during an
  alarm".
- The catalog assumes Alloy ingests `monitoring|currency-bot|openbao`; if the
  live Alloy keep-regex is narrower, Loki-backed log queries return nothing and
  the pods/log fallback must be used per service.
