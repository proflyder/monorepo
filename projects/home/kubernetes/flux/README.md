# Flux — pull-based GitOps for home infrastructure

Flux (`v2.10.0`, only `source-controller`, `kustomize-controller`,
`helm-controller`) runs in `flux-system` and reconciles the home K3s cluster
from `github.com/proflyder/monorepo` (`develop`) with a read-only deploy key.
CI never gets cluster credentials. `gotk-components.yaml` is the pinned
`flux install --export` output; local changes are only the resource-limit
patches in `patches/`.

## What Flux owns

| Kustomization | Path |
|---|---|
| `platform` (umbrella, `wait: true`) | `flux/cluster/platform` |
| `platform-external-secrets` | `external-secrets` |
| `platform-cert-manager` | `ingress/cert-manager` |
| `platform-traefik` | `ingress/traefik` |
| `platform-routes` | `ingress/routes` |
| `observability` / `observability-rules` | `observability`, `observability/rules` |
| `vaultwarden` | `vaultwarden` |
| `nexus` / `nexus-provision` | `nexus/manifests`, `nexus/provision` |
| `pve-exporter` | `pve-exporter/manifests` |
| `home-app-pins` | `./pins` of branch `deploy/home-apps` |
| `app-wakegrant`, `app-intercom`, `app-phone-push`, `app-secrets-gateway`, `app-currency-bot`, `app-mcp-gateway` | `<unit>/manifests` |
| `app-ha-executors` | `scheduler/manifests` (scheduler + ha-bridge + ha-control + ha-scenario) |
| `app-ha-config-sync` | `ha-config-sync/manifests` |
| `app-hermes` | `projects/home-assistant/deploy/flux` (`HelmRelease/runtime`) |
| `app-hermes-work` | `projects/home-assistant/hermes/work` (`HelmRelease/hermes-work`, `HERMES_INSTANCE=work`) |

Paths are relative to `projects/home/kubernetes/` unless absolute. Order:
`platform-external-secrets -> platform-cert-manager -> platform-traefik ->
platform-routes`, all wrapped by `platform`; everything else `dependsOn:
platform`.

Still manual, on purpose: OpenBao (source of every Secret, must recover
without GitOps), host firewalls (`management-firewall/`, `voice-firewall/`),
Cloudflare DNS records (`cloudflare_dns.py`).
`external-secrets/deploy.sh` remains an emergency admin tool only.

## Application images (pins branch)

Manifests live in `develop`; verified image digests live in the orphan machine
branch `deploy/home-apps` (DECISIONS 54–55). The `pins` job of `deploy-home`
(`platform/ci/pins.mjs`, no VPN/OpenBao/kubeconfig) verifies provenance and
commits keys to `pins/home-app-images.yaml` (`ConfigMap/home-app-images` in
`flux-system`, plus `proflyder.dev/*` source annotations). Pushes race between
units, so the writer refetches, reapplies its keys and retries. Flux reads the
branch via `GitRepository/flux-pins` and every app Kustomization substitutes
its `*_IMAGE` key and `BUILD_ID` through `postBuild.substituteFrom`. `develop`
is never written by CI or Flux. `cloudflared` is not CI-built and stays a
`versions.sh` literal.

## Rollback

`flux suspend` of the Kustomization plus a revert of the pins commit, never
`kubectl rollout undo`. `suspend: true` is a rollback/drill lever, not a steady
state. Do not delete a Kustomization: `prune: true` would remove the workload;
app PVCs carry `kustomize.toolkit.fluxcd.io/prune: disabled` for that reason.
For Hermes, `HelmRelease/runtime` remediation rolls back a failed upgrade or
smoke hook.

## Private values

Secrets are materialised from OpenBao by `external-secrets.yaml` (bootstrap
bundle, `deletionPolicy: Retain`); paths and procedures are in
`docs/SECRETS.md`.

`secret/k8s/flux-system/settings` → Secret `flux-settings`, substituted into
platform Kustomizations:

| Key | Used by |
|---|---|
| `NEXUS_HOST` | Nexus IngressRoute |
| `NEXUS_PUBLIC_HOST` | public Nexus route (Cloudflare Tunnel) |
| `NODE_LAN_IP` | Flux NetworkPolicy, Vaultwarden ingress |
| `HOME_DOMAIN` | wildcard certificate, `*.home` routes, Grafana root URL, Vaultwarden domain |
| `LAN_CIDR` | `home-allowlist` Middleware, intercom/Vaultwarden ingress |
| `HAOS_ADDRESS` | `home-assistant-external` EndpointSlice |
| `PVE_NODE_TARGET` | `pve-node` scrape target (`host:9100`) |

Other paths: `secret/k8s/flux-system/git-deploy-key` (`identity`,
`known_hosts`), `secret/k8s/flux-system/app-settings` → `flux-app-settings`
(NodePorts, `HA_URL`, LAN addresses, station scrape target for app
Kustomizations), `secret/k8s/flux-system/runtime-values` (field `values`) →
`runtime-values` for `HelmRelease/runtime`.

## First bootstrap (owner)

Prerequisites: OpenBao running with the `external-secrets` Kubernetes-auth
role; ESO and `ClusterSecretStore/openbao` already installed (chicken-and-egg:
install once with Helm from `external-secrets/values/external-secrets.yaml`
and the digest from `versions.sh`; Flux adopts it afterwards).

1. Generate an ed25519 deploy key and `ssh-keyscan -t ed25519 github.com`,
   check the fingerprint, add the public key as a read-only GitHub deploy key,
   write private key + `known_hosts` to `git-deploy-key` (GET → merge → POST
   with `options.cas`, temp file `0600`, removed afterwards).
2. Fill `secret/k8s/flux-system/settings` with the keys above.
3. `mise run admin:flux-bootstrap` — applies toolkit and ExternalSecrets,
   waits for the Secrets, applies `sync.yaml`, requests a reconcile.
   Idempotent, prints no secrets.

## Constraints

- HelmReleases set `releaseName` and `storageNamespace` explicitly; otherwise
  helm-controller stores releases in `flux-system` as `<ns>-<name>` and
  duplicates instead of adopting live releases. Values come from generated
  ConfigMaps of `values/*.yaml`; chart versions are checked against
  `versions.sh` by the tests.
- `install/upgrade.remediation.retries: 0`, drift detection off: a failed
  platform upgrade stops for a human, no automatic uninstall.
- `upgrade.crds: CreateReplace` only for cert-manager.
- PVC protection: Grafana PVC `helm.sh/resource-policy: keep`; Vaultwarden PVC
  also `prune: disabled`; Prometheus/Alertmanager/Loki PVCs come from
  StatefulSets with `Retain`.
- `flux-system-chart-egress` allows HTTPS to `0.0.0.0/0` except private
  ranges for `source-controller` only (chart CDNs change addresses).
- Delivery alerts (`FluxKustomizationNotReady`, `HelmReleaseNotReady`,
  `PinsStale`) read `gotk_resource_info` built by kube-state-metrics
  `customResourceState`: Flux controllers do not export readiness metrics.
- Manual `kubectl apply` on Flux-owned objects is reverted on reconcile.

## Day-to-day

```bash
mise run kubectl -- get kustomizations,helmreleases,gitrepositories -A
mise run kubectl -- -n flux-system annotate kustomization flux-system \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite
mise run kubectl -- -n flux-system suspend kustomization/vaultwarden
mise run kubectl -- -n flux-system resume kustomization/vaultwarden
```

New service: Kustomize directory without secret values (`${PLACEHOLDER}` +
key in `flux-settings`/`flux-app-settings`), Kustomization CR under
`cluster/` with `dependsOn`, listed in `cluster/kustomization.yaml`, static
tests next to it.

## Tests

`mise run test flux-deploy` (or `python3 -m unittest
projects/home/kubernetes/flux/test_flux.py`) renders every Kustomization with
`kubectl kustomize` and checks controllers, limits, NetworkPolicy, image pins,
release adoption, dependency chain, ExternalSecret paths and absence of
committed secrets.
