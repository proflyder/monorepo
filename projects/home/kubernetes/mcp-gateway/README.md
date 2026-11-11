# mcp-gateway deployment (home K3s)

Kubernetes deployment of `solutions/mcp-gateway`: the single MCP gateway that
Hermes uses for infrastructure diagnostics, the two-step rollout restart and
Telegram reports.

- Solution: `solutions/mcp-gateway` (Rust, static musl, distroless, one binary).
- Project: this directory — manifests, catalog, RBAC, secrets wiring, deploy.
- Namespace: `home-assistant`; one Deployment, one ClusterIP Service, no
  ingress and no NodePort.
- CI unit: `mcp-gateway` (Bazel: Rust crate tests/clippy/fmt + musl image +
  deploy) and `mcp-gateway-deploy` (catalog/RBAC/deploy contract).

```text
Hermes (hostNetwork, no kubectl)          voice proxy (in-cluster)
   │  POST /mcp  Bearer (HERMES_INFRA_TOKEN)  │  POST /v1/voice/*  Bearer (voice)
   ▼                                         ▼
Service mcp-gateway:8080 ──► Deployment mcp-gateway
   │                          ├─ Prometheus  monitoring-prometheus:9090
   │                          ├─ Alertmanager monitoring-alertmanager:9093
   │                          ├─ Loki         loki:3100
   │                          ├─ Kubernetes API (ServiceAccount + RBAC)
   │                          ├─ Telegram Bot API (fixed recipient)
   │                          └─ SSH :22 ──► bb host forced command ──► bb plugin
   └─ /healthz /readyz /metrics (cluster-local only)
```

## Files

```text
manifests/deployment.yaml         Deployment + ClusterIP Service (template)
manifests/catalog.yaml            service catalog + sanitized topology overview
manifests/bb-targets.yaml         personal-only spoken allowlist for bb spawns
manifests/rbac.yaml               ServiceAccount + per-namespace read/restart Roles
manifests/external-secrets.yaml   auth, telegram, ha, bb-ssh, voice secrets
manifests/network-policies.yaml   ingress from Hermes/monitoring, narrow egress
manifests/servicemonitor.yaml     Prometheus scrape of /metrics
test_deploy.py                    catalog↔RBAC and manifest contracts
```

Flux renders `catalog.yaml`/`bb-targets.yaml` into the catalog ConfigMap and
substitutes the private placeholders; tracked files contain only
placeholders/TEST-NET values.

## Catalog is the allow-list

`manifests/catalog.yaml` lists every service the gateway may read and — when
`restart.allowed` — restart. It is applied verbatim as the ConfigMap
`mcp-gateway-catalog-<hash>` (content-hashed, so a catalog change rolls the pod:
the gateway reads it once at start) and mounted at `/etc/mcp-gateway/catalog.yaml`.

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
  `${STATION_IP}`; Flux substitutes them from `Secret/flux-app-settings`, so
  tracked files never contain real host addresses.
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
✅ Всё работает: 43 из 43

В норме
✅ Голос: Hermes, GigaAM, Whisper, Piper, Runtime Health Exporter
✅ Дом: Home Assistant, Music Assistant, Станция Midi (Pulsar)
✅ Планировщик: Scheduler, HA Bridge, HA Control, HA Scenario
✅ Мониторинг (8)
✅ Секреты и сеть (9)
✅ Приложения: MCP Gateway, Currency Bot, Cloudflared, Intercom, Wake Grant
✅ Платформа: Nexus, Flux (приостановлено 1), Каталог отчёта
```

`Flux` is a report-only check: every Kustomization and HelmRelease must be
`Ready=True`; a suspended resource is counted in the note but never reported as
a problem. `Каталог отчёта` is the self-maintenance check: every
Deployment/StatefulSet/DaemonSet in the cluster must be a catalog service or
listed in the catalog's `ignore_workloads` (`namespace/Kind/name`), otherwise
the report says `не в каталоге отчёта: ns/Kind/name`. Adding a service without
a catalog entry therefore shows up on the next report instead of staying
invisible.

With problems the failing services come first, for example:

```text
Отчёт по инфраструктуре, 01.11 19:52
⚠️ Есть проблемы: 2 из 43

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
The tracked catalog and bb allow-list are loaded by the real Rust validators
in `//solutions/mcp-gateway/crates/mcp-gateway:deploy_contract_tests` (the
former Go `catalog_contract_test.go` is gone); an invalid duplicate-id catalog
is rejected in the same test.

## RBAC

- ServiceAccount `home-assistant/mcp-gateway`.
- ONE cluster-wide ClusterRole `mcp-gateway-read` (read-only, `get/list`):
  pods, pods/log, events, services, deployments, statefulsets, daemonsets,
  replicasets, plus Flux `kustomizations` and `helmreleases`. The cluster-wide
  grant is what lets the report enumerate workloads in every namespace.
- `mcp-gateway-restart` only in namespaces with restartable workloads, verb
  `patch` only, restricted with `resourceNames` equal to the catalog.
- `kube-system` and `flux-system` are read-only: their workloads belong to K3s
  and to the Flux bootstrap.
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
  6053/8928/8099, Proxmox 8006, VPN 443), bb host `${BB_CT_IP}:22` only, DNS 53
  (UDP/TCP), Telegram 443. Telegram publishes no stable ranges, so 443 is any-destination — the
  same trade-off as currency-bot.
- K3s (kube-router) enforces NetworkPolicies. Prefer allowing over breaking:
  if the CNI ever stops attributing hostNetwork sources to the node address,
  the ingress rules still cover the pod CIDR.

## One-time prerequisites (operator)

Never paste secret values into chat or a commit. Use the private paths below.

1. **Gateway↔Hermes bearer token** (value never printed):

   ```bash
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

4. **bb bridge key pair and pinned host key** — generate an ed25519 key that
   the bb host accepts **only** for the forced command, and pin the host key:

   ```bash
   ssh-keygen -t ed25519 -N '' -C mcp-gateway-bb \
     -f secrets/local/mcp-gateway-bb_ed25519
   # On the bb host, install the public key with the forced command and the
   # source restriction (see projects/home/proxmox/bb/README.md), then:
   ssh-keyscan -t ed25519 <BB_CT_IP> > secrets/local/mcp-gateway-bb_known_hosts
   bao kv put secret/k8s/home-assistant/mcp-gateway-bb-ssh \
     id_ed25519=@secrets/local/mcp-gateway-bb_ed25519 \
     known_hosts=@secrets/local/mcp-gateway-bb_known_hosts
   ```

   Never reuse the workstation key, and never grant a shell. `ssh-keyscan`
   output must be verified against the host once before it is pinned; after
   that the pinned file is the only trust anchor.

5. **Voice token** — a dedicated bearer token, different from the MCP token:

   ```bash
   bao kv put secret/k8s/home-assistant/mcp-gateway-voice \
     token="$(openssl rand -hex 32)"
   ```

   The value goes to the voice proxy's `HERMES_VOICE_GATEWAY_TOKEN(_FILE)`;
   it is rejected on
   `/mcp`. The MCP token is rejected on `/v1/voice/*`.

6. **Admin manifests** (cluster/namespace objects, applied once):

   ```bash
   mise run kubectl -- apply -f projects/home/kubernetes/mcp-gateway/manifests/rbac.yaml
   mise run kubectl -- apply -f projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml
   mise run kubectl -- -n home-assistant wait --for=condition=Ready \
     externalsecret/mcp-gateway-auth externalsecret/mcp-gateway-telegram \
     externalsecret/mcp-gateway-ha externalsecret/mcp-gateway-bb-ssh \
     externalsecret/mcp-gateway-voice --timeout=120s
   ```

7. **Deploy-time env** in OpenBao `secret/k8s/flux-system/app-settings`
   (ESO creates `Secret/flux-app-settings`; Flux substitutes it):

   ```text
   LAN_CIDR                 <LAN CIDR>
   NODE_LAN_IP              <K3s node address>
   HAOS_IP                  <HAOS VM address>
   STATION_IP               <station address>
   BB_CT_IP                 <bb host address>
   BB_PROJECT_STATION_LAB   <station-lab projectId or exact name>
   BB_PROJECT_ENGLISH_NOTES <english-learning-notes projectId or exact name>
   BB_DISPATCHER_THREAD_ID  <permanent work dispatcher thread id>
   ```

8. **ExternalSecrets in the ESO apply set.** `external-secrets/deploy.sh`
   keeps an explicit list of ExternalSecret manifests;
   `projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml` is in
   it, so one `mise run admin:external-secrets` covers it. Flux applies the
   same objects; the emergency tool remains for a manual re-apply.

## Deploy

Flux applies the unit (`Kustomization/app-mcp-gateway`) from `develop`: the
digest comes from `deploy/home-apps`, the catalog ConfigMap is generated from
`catalog.yaml`/`bb-targets.yaml`, and `${...}` placeholders come from
`Secret/flux-app-settings`. After a merge, `deploy-home`'s `pins` job promotes
the verified digest; there is no CI VPN, kubeconfig or deploy script.

Checks:

```bash
mise run kubectl -- -n home-assistant get deploy,pod,svc,externalsecret -l app.kubernetes.io/name=mcp-gateway
mise run kubectl -- -n home-assistant logs deploy/mcp-gateway --tail=50
mise run kubectl -- -n home-assistant get --raw \
  "/api/v1/namespaces/home-assistant/services/http:mcp-gateway:8080/proxy/readyz"
mise run kubectl -- -n home-assistant exec deploy/mcp-gateway -- /usr/local/bin/mcp-gateway -healthcheck
mise run kubectl -- auth can-i --as=system:serviceaccount:home-assistant:mcp-gateway \
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

## bb group and voice entry point

Rollout order (never partially enabled):

1. On the bb host, install/refresh the bridge config and the restricted key
   (`projects/home/proxmox/bb/README.md`), then verify from the K3s node:
   `ssh -i <key> -o UserKnownHostsFile=<known_hosts> bb@<BB_CT_IP>` prints one
   JSON refusal only (no shell).
2. Put the OpenBao keys from prerequisites 4–5; wait for
   `externalsecret/mcp-gateway-bb-ssh` and `mcp-gateway-voice` to be Ready.
3. Deploy with `BB_CT_IP`, `BB_PROJECT_STATION_LAB`,
   `BB_PROJECT_ENGLISH_NOTES`, `BB_DISPATCHER_THREAD_ID`
   present. The gateway starts with `bb.read,bb.act` enabled and the voice
   routes mounted only when the voice token exists.
4. Point the voice proxy at the Service with the voice token; the MCP token is
   not accepted there.

Checks:

```bash
mise run kubectl -- -n home-assistant get secret mcp-gateway-bb-ssh mcp-gateway-voice
mise run kubectl -- -n home-assistant log deploy/mcp-gateway | grep -E '"bb"|"voice"'
# voice state (cluster-local only; the proxy image is distroless, so use a
# kubectl-side port-forward instead of exec; the token is never printed)
mise run kubectl -- -n home-assistant port-forward svc/mcp-gateway 18081:8080 &
VOICE_TOKEN=$(mise run kubectl -- -n home-assistant get secret mcp-gateway-voice \
  -o jsonpath='{.data.token}' | base64 -d)
curl -fsS -H "Authorization: Bearer $VOICE_TOKEN" \
  http://127.0.0.1:18081/v1/voice/state
kill %1 2>/dev/null || true
unset VOICE_TOKEN
```

- `bb_status` and `bb_prepare_spawn` only ever expose `profile == personal`.
  Threads with a null/unknown profile are dropped before the answer is built,
  so work titles, keys and topics cannot reach Hermes.
- `profile=work` speech goes to `MCP_BB_DISPATCHER_THREAD_ID`; a bb failure
  stays on `route:"bb"` and never falls back to Hermes.
- Agent conversation mode is in-memory and disappears on rollout; the next
  phrase routes normally (personal → Hermes, work → dispatcher).
- Egress to the bb host is exactly `${BB_CT_IP}:22`; no plugin HTTP route is
  reachable from the cluster.

## Rollback

Revert the `deploy/home-apps` commit that changed `MCP_GATEWAY_IMAGE`; Flux
re-applies the previous digest and the catalog from Git. For a drill, freeze
first with `flux suspend kustomization/app-mcp-gateway`, then resume after the
revert. Do not delete the Kustomization (`prune: true` removes the workload).
Full removal:

```bash
mise run kubectl -- -n flux-system suspend kustomization/app-mcp-gateway
mise run kubectl -- -n home-assistant delete deployment,service mcp-gateway
mise run kubectl -- -n home-assistant get configmap -o name | grep mcp-gateway-catalog | xargs mise run kubectl -- -n home-assistant delete
mise run kubectl -- -n home-assistant delete networkpolicy mcp-gateway-ingress mcp-gateway-egress
mise run kubectl -- -n home-assistant delete servicemonitor mcp-gateway
mise run kubectl -- delete -f projects/home/kubernetes/mcp-gateway/manifests/rbac.yaml
mise run kubectl -- delete -f projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml
```

## Open risks

- HAOS, Music Assistant and the station have no Kubernetes metrics; their
  status now comes from external catalog checks (HA REST, MA TCP/HTTP, station
  TCP and HA entities). A check proves reachability, not application-level
  correctness, and `ha_unavailable_summary` is limited to the configured
  domains to avoid noise from permanently unavailable entities.
- `TELEGRAM_ALLOWED_USERS` must exist in `hermes-environment`; without it
  `mcp-gateway-telegram` stays not Ready and the deploy fails closed on purpose.
- Restarting `traefik`, `openbao` or a monitoring component is technically
  possible after confirmation and briefly degrades the whole home; the
  warnings state the consequence, and RBAC cannot express "but not during an
  alarm".
- The catalog assumes Alloy ingests `monitoring|currency-bot|openbao`; if the
  live Alloy keep-regex is narrower, Loki-backed log queries return nothing and
  the pods/log fallback must be used per service.
