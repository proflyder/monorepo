# Flux — pull-based GitOps for home infrastructure

Flux runs in the `flux-system` namespace and reconciles a small part of the
home K3s cluster from `github.com/proflyder/monorepo` (`develop`). The pilot is
Nexus (`projects/home/kubernetes/nexus`); everything else stays on
`deploy-home` or its existing admin script.

The model is pull-only: Flux clones the repository with a read-only deploy key
and applies changes from inside the cluster. CI never gets cluster credentials
for infrastructure, and `deploy-home` keeps owning application services.

## Components

Only the minimum three controllers are installed, at Flux `v2.10.0`:

| Component | Purpose |
|---|---|
| `source-controller` | clones the GitRepository, produces the artifact |
| `kustomize-controller` | builds Kustomizations and applies them |
| `helm-controller` | reconciles HelmRelease objects (not used by Nexus yet) |

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
    ├── network-policy.yaml   flux-system egress: DNS, API, GitHub only
    ├── nexus.yaml            Kustomization for the Nexus workload
    └── nexus-provision.yaml  Kustomization for the provisioning Job
```

## First bootstrap (owner, manual)

Prerequisites: the External Secrets Operator, `ClusterSecretStore/openbao` and
the Traefik routes already exist.

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
`secret/k8s/flux-system/settings` with these keys (no values in Git):

| Key | Meaning |
|---|---|
| `NEXUS_HOST` | internal Nexus name under the home suffix |
| `NEXUS_PUBLIC_HOST` | Cloudflare Tunnel hostname used by CI |
| `NODE_LAN_IP` | K3s node LAN address (NetworkPolicy, DNS) |

ESO materialises this as the `flux-settings` Secret; Flux uses it for
`postBuild` substitution.

### 3. Install

```bash
mise run admin:flux-bootstrap
```

The script applies the toolkit and ExternalSecrets, waits for the Secrets,
applies `sync.yaml`, and requests an immediate reconcile. It never reads or
prints secret values and is safe to run again.

### 4. DNS (manual, intentionally outside Flux)

The internal Nexus `A` record still points at the K3s node. Create it once:

```bash
mise run secrets:file home/vpn/cloudflare.env
python3 projects/home/kubernetes/cloudflare_dns.py \
  secrets/home/vpn/cloudflare.env nexus.<home-domain> <node-lan-ip>
mise run secrets:clean
```

The public `ci-nexus.<public-zone>` hostname belongs to the existing
`home-k3s` Cloudflare Tunnel and is managed with it (see the Nexus README).
Neither record is reconciled by Flux.

## Day-to-day

```bash
# what is reconciled and whether it is ready
mise run kubectl -- get kustomizations -A
mise run kubectl -- get gitrepositories -A
mise run kubectl -- -n flux-system get pods

# force a reconcile after pushing a commit
mise run kubectl -- -n flux-system annotate kustomization flux-system \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite
mise run kubectl -- -n flux-system annotate kustomization nexus \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite

# pause / resume reconciliation
mise run kubectl -- -n flux-system annotate kustomization nexus \
  reconcile.fluxcd.io/requestedAt- --overwrite
mise run kubectl -- -n flux-system suspend kustomization/nexus
mise run kubectl -- -n flux-system resume kustomization/nexus
```

Changes to infrastructure under Flux are made by merging to `develop`; do not
run a second `kubectl apply` against those resources. Manual cluster edits are
reverted on the next reconcile.

## Adding another service later

1. Turn its manifests into a Kustomize directory with no secret values and no
   template placeholders that Kustomize cannot resolve.
2. Put private values in the `flux-settings` Secret (or a second ESO Secret)
   and use Flux `postBuild.substituteFrom`.
3. Add a `Kustomization` CR under `cluster/` with `dependsOn` on the
   resources it needs.
4. Add static tests next to the service and extend this README.

Non-sensitive infrastructure can stay on `deploy-home`; the pilot does not
move ingress, Vaultwarden, observability, OpenBao or application services.

## Tests

```bash
mise run test flux-deploy
```

The suite renders the toolkit bundle, the root Kustomization and the Nexus
Kustomizations with `kubectl kustomize`, then checks the controller set,
resource limits, NetworkPolicy ports, image pins, ExternalSecret paths and the
absence of committed secrets.
