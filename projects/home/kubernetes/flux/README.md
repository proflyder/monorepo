# Flux — pull-based GitOps for home infrastructure

Flux runs in the `flux-system` namespace and reconciles the home K3s cluster
from `github.com/proflyder/monorepo` (`develop`). The model is pull-only: Flux
clones the repository with a read-only deploy key and applies changes from
inside the cluster. CI never gets cluster credentials for infrastructure.

Reconciled by Flux today:

| Unit | Namespace | What |
|---|---|---|
| `platform` (umbrella) | all platform namespaces | applies and waits for the four children below |
| `platform-external-secrets` | `external-secrets` | ESO release and `ClusterSecretStore/openbao` |
| `platform-cert-manager` | `cert-manager` | cert-manager release (CRDs via `CreateReplace`) |
| `platform-traefik` | `traefik` | Traefik release (hostNetwork `80/443`) |
| `platform-routes` | `traefik`, `cert-manager`, `monitoring` | wildcard Certificate, ClusterIssuers, allow-list, shared IngressRoutes, HA EndpointSlice, Cloudflare solver Secret |
| `observability` | `monitoring` | kube-prometheus-stack, Loki, Alloy, project dashboards, Grafana NodePort Service, monitoring Secrets |
| `observability-rules` | `monitoring` | backup-freshness PrometheusRule |
| `vaultwarden` | `vaultwarden` | password manager, PVC, route, admin Secret |
| `nexus` / `nexus-provision` | `nexus` | artifact cache and REST provisioning |
| `home-app-pins` | `flux-system` | `home-app-images` ConfigMap from the machine branch `deploy/home-apps` |
| `app-wakegrant` | `home-assistant` | pilot application, applied by Flux since the 6.1 cutover |
| `app-intercom` | `home-assistant` | signaling service, Flux-owned since the 6.2 cutover |
| `app-currency-bot` | `currency-bot` | bot and Cloudflare Tunnel, Flux-owned since the 6.3 cutover |
| `app-mcp-gateway` | `home-assistant` | gateway and catalog, Flux-owned since the 6.4 cutover |
| `app-ha-executors` | `home-assistant` | scheduler + ha-bridge + ha-control + ha-scenario, Flux-owned since the 6.5 cutover |
| `app-hermes` | `home-assistant` / `flux-system` | `HelmRelease/runtime`, Flux-owned since the 6.6 cutover |

All deployable application units are Flux-owned: `deploy-home` only commits
their verified digests to `deploy/home-apps`, and Flux applies the workloads
from `develop`. `suspend` must stay
absent/false; a manual `flux suspend` is the rollback/drill lever, not a
supported steady state.

Still manual, on purpose:

- **OpenBao** (raft, seal, bootstrap, Kubernetes auth). It is the source of
  every Secret and must be able to recover without GitOps.
- **Host firewalls** (`management-firewall/`, `voice-firewall/`): nftables on
  the K3s VM, outside the Kubernetes API.
- **Cloudflare DNS records**: `ha`, `grafana`, `alarms`, `control`, `currency`,
  `intercom`, `passwords`, `vault`, `nexus` are created with
  `cloudflare_dns.py` (or the `nexus` procedure).
- **Application services** are Flux-owned: `wakegrant`, `intercom`,
  `currency-bot`, `mcp-gateway`, the HA executors and Hermes have
  `Kustomization`s in `apps.yaml`, so `deploy-home` only writes their digests to
  the pins branch. Their `ExternalSecrets` are owned by their Flux manifest
  sets; `projects/home/kubernetes/external-secrets/deploy.sh` remains an
  emergency admin tool.

## Components

Only the minimum three controllers are installed, at Flux `v2.10.0`:

| Component | Purpose |
|---|---|
| `source-controller` | clones the GitRepository, produces the artifact |
| `kustomize-controller` | builds Kustomizations and applies them |
| `helm-controller` | reconciles HelmRelease objects |

`notification-controller`, image automation and `source-watcher` are
intentionally absent. `gotk-components.yaml` is the pinned
`flux install --export` output; the only local changes are the resource-limit
patches in `patches/`.

## Layout

```text
projects/home/kubernetes/flux/
├── gotk-components.yaml      generated Flux install manifests (v2.10.0)
├── kustomization.yaml        bootstrap bundle: toolkit + ExternalSecrets + patches
├── patches/                  small-node CPU/memory limits for the 3 controllers
├── external-secrets.yaml     deploy key and settings, materialised from OpenBao
├── sync.yaml                 GitRepository + root Kustomization (applied once)
├── bootstrap.sh              owner-run, idempotent install
└── cluster/                  reconciled by Flux, not by bootstrap
    ├── kustomization.yaml
    ├── network-policy.yaml
    ├── apps.yaml             pins source + every app Kustomization
    ├── platform.yaml          umbrella for the platform chain
    ├── platform/
    │   ├── external-secrets.yaml
    │   ├── cert-manager.yaml
    │   ├── traefik.yaml
    │   └── routes.yaml
    ├── observability.yaml
    ├── observability-rules.yaml
    ├── vaultwarden.yaml
    ├── nexus.yaml
    └── nexus-provision.yaml
```

Component manifests stay next to their project and are pointed at by the
Kustomization `path`:

| Kustomization | Path |
|---|---|
| `platform` | `projects/home/kubernetes/flux/cluster/platform` |
| `platform-external-secrets` | `projects/home/kubernetes/external-secrets` |
| `platform-cert-manager` | `projects/home/kubernetes/ingress/cert-manager` |
| `platform-traefik` | `projects/home/kubernetes/ingress/traefik` |
| `platform-routes` | `projects/home/kubernetes/ingress/routes` |
| `observability` | `projects/home/kubernetes/observability` |
| `observability-rules` | `projects/home/kubernetes/observability/rules` |
| `vaultwarden` | `projects/home/kubernetes/vaultwarden` |
| `nexus` | `projects/home/kubernetes/nexus/manifests` |
| `nexus-provision` | `projects/home/kubernetes/nexus/provision` |

Helm charts keep the same release name and namespace as the live releases
(`helm list -A`): each HelmRelease sets `releaseName` and `storageNamespace`
explicitly, because helm-controller otherwise defaults to
`<targetNamespace>-<name>` and would store release secrets in `flux-system`
(creating duplicates instead of adopting the live releases). Values come from generated ConfigMaps built from the reviewed
files (`values/*.yaml`); chart versions are pinned in the HelmRelease and
checked against `projects/home/kubernetes/versions.sh` by the tests.

## Application images (pins branch)

Application manifests stay in `develop`, but the verified image digests live in
a separate machine branch, `deploy/home-apps` (variant B of
DECISIONS 54–55). CI verifies provenance and commits the digest there;
Flux reads the branch with its own `GitRepository/flux-pins` (same read-only
deploy key) and publishes `ConfigMap/home-app-images` through
`Kustomization/home-app-pins`. `develop` stays PR-only and Flux never writes to
Git.

The branch is an orphan with exactly two files:

```text
deploy/home-apps
├── pins/kustomization.yaml       kind: Kustomization, resources: [home-app-images.yaml]
└── pins/home-app-images.yaml     ConfigMap/home-app-images in flux-system
```

`pins/home-app-images.yaml` (values are digests, not secrets):

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: home-app-images
  namespace: flux-system
  annotations:
    proflyder.dev/source-commit: "<40-hex merge commit>"
    proflyder.dev/source-tree: "<40-hex tree tag>"
    proflyder.dev/verified-at: "2026-11-10T12:00:00Z"
    proflyder.dev/run: "https://github.com/proflyder/monorepo/actions/runs/<id>"
data:
  BUILD_ID: "<commit12>"
  SCHEDULER_IMAGE: "ghcr.io/proflyder/scheduler@sha256:<64-hex>"
  HA_BRIDGE_IMAGE: "ghcr.io/proflyder/ha-bridge@sha256:<64-hex>"
  HA_CONTROL_IMAGE: "ghcr.io/proflyder/ha-control@sha256:<64-hex>"
  HA_SCENARIO_IMAGE: "ghcr.io/proflyder/ha-scenario@sha256:<64-hex>"
  WAKEGRANT_IMAGE: "ghcr.io/proflyder/wakegrant@sha256:<64-hex>"
  INTERCOM_IMAGE: "ghcr.io/proflyder/intercom@sha256:<64-hex>"
  CURRENCY_BOT_IMAGE: "ghcr.io/proflyder/currency-bot@sha256:<64-hex>"
  MCP_GATEWAY_IMAGE: "ghcr.io/proflyder/mcp-gateway@sha256:<64-hex>"
  HERMES_IMAGE: "ghcr.io/proflyder/hermes@sha256:<64-hex>"
  VOICE_PROXY_IMAGE: "ghcr.io/proflyder/voice-proxy@sha256:<64-hex>"
  HEALTH_EXPORTER_IMAGE: "ghcr.io/proflyder/health-exporter@sha256:<64-hex>"
  HOME_DEPLOY_IMAGE: "ghcr.io/proflyder/home-deploy@sha256:<64-hex>"
```

Keys are written by the `pins` job (`platform/ci/pins.mjs`) for every
deployable unit; `cloudflared` is not CI-built and stays a reviewed
`versions.sh` literal. `BUILD_ID` is the `proflyder.dev/build`
annotation on the non-Hermes workloads; every other key is a digest-pinned
image. Every key is consumed by exactly one app Kustomization.

Current live digests (read-only, safe to copy into the initial pins commit):

```bash
# one unit
mise run kubectl -- -n home-assistant get deployment scheduler \
  -o jsonpath='{.spec.template.spec.containers[0].image}'

# every application workload of the six units
mise run kubectl -- -n home-assistant get deploy \
  scheduler ha-bridge ha-control ha-scenario intercom mcp-gateway \
  -o jsonpath='{range .items[*]}{.metadata.name}{"\t"}{.spec.template.spec.containers[0].image}{"\n"}{end}'
mise run kubectl -- -n currency-bot get deploy \
  -o jsonpath='{range .items[*]}{.metadata.name}{"\t"}{.spec.template.spec.containers[0].image}{"\n"}{end}'
mise run kubectl -- -n home-assistant get deploy \
  hermes runtime-health-exporter \
  -o jsonpath='{range .items[*]}{.metadata.name}{"\t"}{.spec.template.spec.containers[*].image}{"\n"}{end}'
```

`Kustomization/app-wakegrant` is the pilot and is **enabled** (DECISIONS 54): Flux applies the live workload from `develop` and substitutes
`WAKEGRANT_IMAGE`/`BUILD_ID` from the pins ConfigMap plus
`WAKE_GRANT_NODE_PORT` from `Secret/flux-app-settings`. `deploy-home` no longer
runs `wakegrant/deploy.sh` (the script is deleted), so its `pins` job (no VPN,
no OpenBao, no kubeconfig) promotes the PR-built digest,
verifies provenance and commits only `BUILD_ID`, `WAKEGRANT_IMAGE` and the
`proflyder.dev/*` annotations to `deploy/home-apps` via
`platform/ci/pins.mjs`. Pushes race with other units, so the writer refetches,
reapplies the same keys and retries; `develop` is never written.

The other app units (6.2–6.6) are cut over the same way: a Kustomization in
`apps.yaml` with `suspend: false`, and images substituted from the pins
ConfigMap. `deploy-home`'s `pins` job writes their keys and has no VPN, OpenBao
or kubeconfig. The 6.7 cleanup removed the old CI path (`deploy` job,
`ci-deploy`, VPN certificate `ci-github`, OpenBao `secret/ci/deploy-home`);
live leftovers are the owner's commands listed in `docs/OPERATIONS.md`
(«Доступ CI»).

Manifest paths for the app units:

| Kustomization | Path | Notes |
|---|---|---|
| `app-intercom` | `projects/home/kubernetes/intercom/manifests` | no PVC |
| `app-currency-bot` | `projects/home/kubernetes/currency-bot/manifests` | namespace stays admin-owned |
| `app-mcp-gateway` | `projects/home/kubernetes/mcp-gateway/manifests` | catalog via `configMapGenerator` |
| `app-ha-executors` | `projects/home/kubernetes/scheduler/manifests` | one set for scheduler + three executors; steady-state modes pinned |
| `app-hermes` | `projects/home-assistant/deploy/flux` | `HelmRelease/runtime`, values from `runtime-values` |

Rollback is `suspend: true` plus a pins revert, never `kubectl rollout undo`.
For Hermes, `HelmRelease/runtime` remediation rolls back on a failed upgrade or
smoke hook; a pins revert re-applies the previous digests. Do not delete a
Kustomization: `prune: true` would remove the workload, and every app PVC
(`wakegrant-data`, `scheduler-data`, `ha-bridge-data`, `ha-scenario-data`,
`currency-bot-data`) carries `kustomize.toolkit.fluxcd.io/prune: disabled` for
exactly that reason.

Delivery alerts (`FluxKustomizationNotReady`, `HelmReleaseNotReady`,
`PinsStale`) live in `projects/home/kubernetes/observability/rules`. They read
`gotk_resource_info`, which kube-state-metrics builds from the Flux CRDs
(`customResourceState` in `observability/values/prometheus.yaml`, list/watch
RBAC included): Flux 2.x controllers only export reconcile duration and token
metrics, so controller metrics alone cannot answer "is it Ready". The
kube-state-metrics configmap collector still feeds `kube_configmap_info` for
`PinsStale`. `PodMonitor/flux-controllers` keeps scraping the controller
`gotk_*` metrics, and `flux-system-ingress` allows only the monitoring
namespace on port 8080.

## Private values (`flux-settings`)

`secret/k8s/flux-system/settings` is materialised by ESO as the
`flux-settings` Secret and used by Kustomization `postBuild.substituteFrom`.

| Key | Used by |
|---|---|
| `NEXUS_HOST` | Nexus IngressRoute |
| `NEXUS_PUBLIC_HOST` | public Nexus route (Cloudflare Tunnel) |
| `NODE_LAN_IP` | Flux NetworkPolicy, Vaultwarden ingress |
| `HOME_DOMAIN` | wildcard certificate, every `*.home` route, Grafana root URL, Vaultwarden domain |
| `LAN_CIDR` | `home-allowlist` Middleware, intercom/Vaultwarden ingress |
| `HAOS_ADDRESS` | `home-assistant-external` EndpointSlice |
| `PVE_NODE_TARGET` | `pve-node` scrape target (`host:9100`) |

No value is committed to Git. `HOME_DOMAIN`, `LAN_CIDR` and `HAOS_ADDRESS`
come from the private `secrets/home/ingress.env`; `PVE_NODE_TARGET` from the
private observability overlay.

Application values are separate:

| OpenBao path | Secret | Consumed by |
|---|---|---|
| `secret/k8s/flux-system/app-settings` | `flux-app-settings` | `postBuild.substituteFrom` of every app Kustomization (NodePorts, `HA_URL`, LAN/host addresses, bb project ids) |
| `secret/k8s/flux-system/runtime-values` (field `values`) | `runtime-values` | `HelmRelease/runtime` `valuesFrom` (Hermes workload values, no `storage` block) |

Both are declared by `ExternalSecret`s in `flux/external-secrets.yaml`
(bootstrap bundle) with `deletionPolicy: Retain`. `app-settings` and
`runtime-values` are also documented in `docs/SECRETS.md` once created.

## First bootstrap (owner, manual)

Prerequisites:

- OpenBao is running and `configure.py` has created the `external-secrets`
  Kubernetes-auth role;
- **External Secrets Operator and `ClusterSecretStore/openbao` already exist**.
  On a clean cluster this is the one chicken-and-egg step: install ESO once
  with Helm (the values file is
  `projects/home/kubernetes/external-secrets/values/external-secrets.yaml`
  with the digest from `versions.sh`), or restore it from backup. After Flux
  is running it owns the release and later upgrades come from Git.

### 1. Create the read-only deploy key

The key exists only in OpenBao; never commit it. On the workstation:

```bash
install -d -m 700 "$TMPDIR/flux-key"
ssh-keygen -t ed25519 -N '' -C 'flux-git-deploy' -f "$TMPDIR/flux-key/id_ed25519"
ssh-keyscan -t ed25519 github.com > "$TMPDIR/flux-key/known_hosts"
```

Review the printed `known_hosts` fingerprint against GitHub's published SSH
fingerprints, then add the **public** key
(`$TMPDIR/flux-key/id_ed25519.pub`) to GitHub:

`Repository → Settings → Deploy keys → Add deploy key`, read-only
(`Allow write access` off).

Write the private key and `known_hosts` to OpenBao
(`secret/k8s/flux-system/git-deploy-key`, keys `identity` and `known_hosts`).
Follow the GET → merge → POST with `options.cas` rule from
`docs/SECRETS.md`; do not print values. A temporary JSON file must be `0600`
and removed afterwards.

### 2. Write the deployment settings

From the private `secrets/home/ingress.env` values, create
`secret/k8s/flux-system/settings` with the keys in the table above (no values
in Git). `secret/k8s/flux-system/settings` already holds `NEXUS_*` and
`NODE_LAN_IP`; add the four new keys with the same GET → merge → POST
procedure.

### 3. Install

```bash
mise run admin:flux-bootstrap
```

The script applies the toolkit and ExternalSecrets, waits for the Secrets,
applies `sync.yaml`, and requests an immediate reconcile. It never reads or
prints secret values and is safe to run again.

### 4. DNS (manual, intentionally outside Flux)

Create or correct the A records once with the private Cloudflare token:

```bash
mise run secrets:file home/vpn/cloudflare.env
python3 projects/home/kubernetes/cloudflare_dns.py \
  secrets/home/vpn/cloudflare.env <host>.<home-domain> <node-lan-ip>
mise run secrets:clean
```

Hosts: `ha`, `grafana`, `alarms`, `control`, `currency`, `intercom`,
`passwords`, `vault`. The public `ci-nexus.<public-zone>` hostname belongs to
the existing `home-k3s` Cloudflare Tunnel. Neither record is reconciled by
Flux.

## Adoption and ordering

Kustomizations form a dependency chain:

```text
platform-external-secrets -> platform-cert-manager -> platform-traefik -> platform-routes
                              (all wrapped by `platform`, wait: true)
        platform ──> observability ──> observability-rules
        platform ──> vaultwarden
```

- `platform` is the only entry point for the platform chain: it applies the
  four child Kustomizations and waits for all of them. Nothing outside the
  chain repeats the ESO -> cert-manager -> Traefik order.
- `platform-external-secrets` must be Ready first: every other unit consumes
  its CRDs and the `flux-settings` Secret.
- cert-manager and Traefik exist before the route CRs. Both releases keep the
  live name/namespace so Helm adopts them; `upgrade.crds: CreateReplace` only
  upgrades cert-manager CRDs.
- Kustomizations use `wait: true`, so a dependency is only considered Ready
  after its HelmRelease reports Ready.
- PVCs are protected at each ownership layer: the Grafana PVC (managed by the
  chart) carries `helm.sh/resource-policy: keep`; Vaultwarden's plain PVC has
  both that annotation and `kustomize.toolkit.fluxcd.io/prune: disabled`;
  Prometheus/Alertmanager/Loki PVCs are created by their StatefulSets with
  `Retain` (Loki `whenDeleted: Retain`), so Helm never owns them.
- Helm upgrades are not self-remediated: every HelmRelease sets
  `install/upgrade.remediation.retries: 0`, and drift detection stays off. A
  failed upgrade stops and is fixed by a human, never by an automatic
  uninstall/rollback.

### Network egress

`flux-system-egress` keeps all controllers on DNS, the API, source artifacts
and GitHub git ranges. `flux-system-chart-egress` adds HTTPS (`443`) to
`0.0.0.0/0` **except** `10/8`, `172.16/12`, `192.168/16`, `100.64/10` and
`169.254/16` for the `source-controller` only, because chart indexes and
tarballs live on CDNs whose addresses change.

## Day-to-day

```bash
# what is reconciled and whether it is ready
mise run kubectl -- get kustomizations -A
mise run kubectl -- get helmreleases -A
mise run kubectl -- get gitrepositories -A
mise run kubectl -- -n flux-system get pods

# force a reconcile after pushing a commit
mise run kubectl -- -n flux-system annotate kustomization flux-system \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite
mise run kubectl -- -n flux-system annotate kustomization observability \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite

# pause / resume one unit
mise run kubectl -- -n flux-system suspend kustomization/vaultwarden
mise run kubectl -- -n flux-system resume kustomization/vaultwarden
```

Changes to infrastructure under Flux are made by merging to `develop`; do not
run a second `kubectl apply` against those resources. Manual cluster edits are
reverted on the next reconcile.

## Adding another service later

1. Turn its manifests into a Kustomize directory with no secret values; keep
   private values as `${PLACEHOLDER}` and add the key to `flux-settings`.
2. For Helm, pin the chart version and reuse the live release name/namespace;
   put the values file next to the component and load it through a generated
   ConfigMap (`disableNameSuffixHash: true`).
3. Add a `Kustomization` CR under `cluster/` with `dependsOn` on the units it
   needs, and add it to `cluster/kustomization.yaml`.
4. Add static tests next to the service and extend this README.

## Tests

```bash
mise run test flux-deploy
```

The suite renders the toolkit bundle, the root Kustomization, every component
Kustomization and the Nexus Kustomizations with `kubectl kustomize`, then
checks the controller set, resource limits, NetworkPolicy ports, image pins,
release adoption (name, namespace, chart version, values source), the
dependency chain, ExternalSecret paths and the absence of committed secrets.
