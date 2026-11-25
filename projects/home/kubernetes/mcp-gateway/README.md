# mcp-gateway deployment (home K3s)

Kubernetes deployment of `solutions/mcp-gateway` (behaviour, tools, report
format and API — in the solution README): the single MCP gateway used by
Hermes for infrastructure diagnostics, two-step rollout restarts, Telegram
reports, the `bb.*` and `time` tool groups, the voice entry point, the app chat
and the Android system dashboard.

- Namespace `home-assistant`; one Deployment, ClusterIP Service `:8080`, PVCs
  `mcp-gateway-chat` and `mcp-gateway-media` (`local-retain`,
  `prune: disabled`).
- Applied by Flux (`Kustomization/app-mcp-gateway`, path `manifests/`): image
  digest `MCP_GATEWAY_IMAGE` from `deploy/home-apps`, `${…}` from
  `Secret/flux-app-settings`. `catalog.yaml`, `bb-targets.yaml` and
  `chat-prompt.txt` become the content-hashed ConfigMap
  `mcp-gateway-catalog-<hash>`: the gateway reads it once at start, so every
  change rolls the pod.
- RBAC and ExternalSecrets are part of the same Flux set.
- CI units: `mcp-gateway` (crate + image) and `mcp-gateway-deploy`
  (`test_deploy.py`, catalog ↔ RBAC contract). The tracked catalog and bb
  allow-list are also loaded by the Rust validators in
  `//solutions/mcp-gateway/crates/mcp-gateway:deploy_contract_tests`.

## Catalog and RBAC

`manifests/catalog.yaml` is the allow-list: every service the gateway may read
and, with `restart.allowed`, restart (Russian warning required;
`self_disrupting` targets such as `hermes` and `mcp-gateway` defer the patch
~20 s so the reply is spoken first). External targets (HAOS, Music Assistant,
station) are read-only checks with `${HAOS_IP}`/`${STATION_IP}` placeholders.
Every Deployment/StatefulSet/DaemonSet in the cluster must be a catalog
service or listed in `ignore_workloads`, otherwise the report flags it; the
same invariant is checked statically by `KubernetesCatalogCoverageTests`
(`platform/quality/test_static.py`, `mise run test:static`) for every
manifest under `projects/home/kubernetes`.

- ClusterRole `mcp-gateway-read`: `get/list` on pods, pods/log, events,
  services, workloads, replicasets, Flux kustomizations/helmreleases.
- `mcp-gateway-restart` Roles: verb `patch` only, `resourceNames` equal to the
  restartable catalog workloads
  (`test_write_resource_names_equal_catalog_restartable_workloads`).
- `kube-system`, `flux-system` read-only. Hermes has
  `automountServiceAccountToken: false`; cluster access goes only through this
  pod. The gateway never deletes, scales, execs or applies.

## Network

- Ingress `8080`: node address (Hermes is `hostNetwork`, kubelet probes), LAN
  CIDR, pod CIDR `10.42.0.0/16` (Traefik, voice proxy), `monitoring` (scrape).
  `/mcp` and `/v1/voice/*` need their bearer tokens and are not routed by
  Traefik; only `Host(control.${HOME_DOMAIN})` with `PathPrefix` `/v1/system/`,
  `/v1/chat/` or `/v1/hermes/` is (`ingress/routes`, app-токены проверяются
  модулями `system`/`chat`; `/v1/hermes/*` — модуль `chat`).
- Egress: kube-API 6443, monitoring 9090/9093/3100, catalog checks on
  `${LAN_CIDR}`, bb host exactly `${BB_CT_IP}:22`, DNS, and 443 to any
  destination for Telegram (no stable ranges).

## Secrets

OpenBao paths (procedure and map — `docs/SECRETS.md`), all under
`secret/k8s/home-assistant/`:

| Path | Purpose |
|---|---|
| `mcp-gateway-auth` (`token`) | Hermes → `/mcp` bearer; Hermes reads the same key as `HERMES_INFRA_TOKEN` |
| `mcp-gateway-voice` (`token`) | voice proxy → `/v1/voice/*`; rejected on `/mcp` and vice versa |
| `hermes-environment` (`TELEGRAM_BOT_TOKEN`, `TELEGRAM_ALLOWED_USERS`) | infra bot; first allowed user is the fixed report recipient. Missing keys keep `mcp-gateway-telegram` not Ready (fail closed) |
| `ha-bridge-ha` (`token`) | HA REST checks, shared with ha-bridge |
| `mcp-gateway-bb-ssh` (`id_ed25519`, `known_hosts`) | dedicated key accepted by the bb host only for the forced command (`projects/home/proxmox/bb/README.md`); host key verified once, then pinned |
| `scheduler-auth`, `hermes-chat-relay`, `chat-alerts` (`token`) | time tools / service-token, chat relay to Hermes, chat alerts; app-токены приходят из snapshot scheduler |

Generate new tokens with `openssl rand -hex 32` straight into OpenBao; never
print values. `flux-app-settings` keys used here: `LAN_CIDR`, `NODE_LAN_IP`,
`HAOS_IP`, `STATION_IP`, `BB_CT_IP`, `BB_PROJECT_STATION_LAB`,
`BB_PROJECT_ENGLISH_NOTES`, `BB_DISPATCHER_THREAD_ID`.

bb group: only `profile == personal` threads are ever exposed; `profile=work`
speech goes to the dispatcher thread, a bb failure never falls back to Hermes.
Enable in order: bb host bridge → OpenBao keys → `flux-app-settings` keys →
voice proxy pointed at the Service.

## Checks

```bash
mise run kubectl -- -n home-assistant get deploy,pod,svc,externalsecret -l app.kubernetes.io/name=mcp-gateway
mise run kubectl -- -n home-assistant exec deploy/mcp-gateway -- /usr/local/bin/mcp-gateway -healthcheck
mise run kubectl -- auth can-i --as=system:serviceaccount:home-assistant:mcp-gateway \
  patch deployment/scheduler -n home-assistant
python3 -m unittest projects/home/kubernetes/mcp-gateway/test_deploy.py
```

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

PVCs `mcp-gateway-chat`/`mcp-gateway-media` stay and are deleted by hand only
if the chat history is not needed.

## Known limits

- External checks prove reachability, not application correctness;
  `ha_unavailable_summary` covers only configured domains.
- Restarting `traefik`, `openbao` or monitoring after confirmation briefly
  degrades the whole home; RBAC cannot express "not during an alarm".
- Loki-backed logs assume Alloy ingests `monitoring|currency-bot|openbao|kube-system`;
  if the live keep-regex is narrower, switch that service to `logs: pods`.
