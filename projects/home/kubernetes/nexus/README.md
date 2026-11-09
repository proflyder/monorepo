# Nexus — in-house artifact cache for CI

A single-replica Sonatype Nexus Repository OSS on the home K3s node. CI
downloads Maven artifacts, npm packages and PyPI wheels through it; Nexus
fetches each object once and serves later requests from a local volume, so a
registry outage does not fail CI.

Bazel does not take GitHub release assets, codeload archives or Node.js
distributions from Nexus (`platform/bazel/downloader.cfg` has no rewrite for
them). These archives are large, and twenty parallel runners pulling them
through the home uplink and the tunnel broke with `Premature EOF`. They come
from the Bazel repository cache that the develop `warm-caches` job saves and
PR jobs restore. The `github`, `github-codeload` and `nodejs-dist` proxies
stay provisioned for manual use.

Internal UI/admin lives at `https://nexus.<home-domain>/` (LAN/VPN only).
CI uses `https://ci-nexus.<public-zone>/repository/` (public, read-only
account).

Nexus is the pilot for Flux GitOps: the workload, routes and provisioning Job
are reconciled from `develop` by the `nexus` and `nexus-provision`
Kustomizations in `projects/home/kubernetes/flux/`.

## What is in the box

| Repository | Type | Upstream | Consumer |
|---|---|---|---|
| `maven-public` | group | `maven-central`, `google-maven` | manual use only |
| `maven-central` | proxy | `https://repo1.maven.org/maven2/` | Bazel / rules_jvm_external |
| `google-maven` | proxy | `https://maven.google.com/` | Bazel / rules_jvm_external |
| `github` | raw proxy | `https://github.com/` | manual use only |
| `github-codeload` | raw proxy | `https://codeload.github.com/` | manual use only |
| `nodejs-dist` | raw proxy | `https://nodejs.org/dist/` | manual use only |
| `npmjs` | npm proxy | `https://registry.npmjs.org/` | npm |
| `pypi` | pypi proxy | `https://pypi.org/` | pip |
| `crates` | cargo proxy | `https://index.crates.io/` | optional; skipped if the format is unavailable |

Anonymous access is disabled. The `ci-reader` role grants only
`nx-repository-view-*-*-{read,browse}`; its password and the `admin` password
come from OpenBao through External Secrets.

## Layout

```text
projects/home/kubernetes/nexus/
├── README.md
├── test_deploy.py               # static contracts, no cluster needed
├── manifests/                   # reconciled by the `nexus` Kustomization
│   ├── kustomization.yaml       # image pins, resource list
│   ├── namespace.yaml
│   ├── external-secrets.yaml    # k8s/nexus/nexus-secrets
│   ├── nexus.yaml               # Service + StatefulSet + 50Gi PVC
│   ├── network-policies.yaml
│   └── ingressroute.yaml        # public /repository/*, internal everything else
└── provision/                   # reconciled by the `nexus-provision` Kustomization
    ├── kustomization.yaml       # configMapGenerator, curl image pin
    ├── provision.sh             # idempotent REST provisioning (runs in-cluster)
    └── job.yaml
```

Deployment-specific values (`NEXUS_HOST`, `NEXUS_PUBLIC_HOST`, `NODE_LAN_IP`,
) come from OpenBao through the `flux-settings` Secret and are
substituted by Flux; they are not committed. Image tags and digests are pinned
in the two `kustomization.yaml` files and mirrored in
`projects/home/kubernetes/versions.sh`.

## Deploy

```bash
# one-time toolkit install (owner)
mise run admin:flux-bootstrap

# normal change: merge to develop, then watch
mise run kubectl -- get kustomizations -A
mise run kubectl -- -n nexus get statefulset,pod,svc
```

The `nexus` Kustomization applies `manifests/`; the `nexus-provision`
Kustomization applies the provisioning Job and depends on `nexus`.

`provision.sh` waits for `/service/rest/v1/status` and converges blob store,
repositories, anonymous access, role and user through the REST API.
`configMapGenerator` puts a content hash in the ConfigMap name, and the Job
mounts that name; when the script changes, the immutable Job is recreated by
Flux (`force: true`) and provisioning runs again. The Job is safe to delete and
re-run by hand:

```bash
mise run kubectl -- -n nexus delete job nexus-provision
mise run kubectl -- -n flux-system annotate kustomization nexus-provision \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite
```

The first boot seeds `/nexus-data/admin.password` from the OpenBao secret. This
happens only before `/nexus-data/db` exists; after that Nexus authenticates
from its H2 database and the init step is a no-op.

### DNS (manual)

The internal `A` record is intentionally outside Flux:

```bash
mise run secrets:file home/vpn/cloudflare.env
python3 projects/home/kubernetes/cloudflare_dns.py \
  secrets/home/vpn/cloudflare.env nexus.<home-domain> <node-lan-ip>
mise run secrets:clean
```

The public tunnel hostname is managed with the existing `home-k3s` tunnel, as
described under “Internet exposure”.

## Adding another proxy repository

1. Add an `ensure_repo` (or `optional_repo`) block to `provision.sh` with the
   format, type, name and JSON body, using the `ci-cache` blob store.
2. Add the rewrite rule to `platform/bazel/downloader.cfg` if Bazel should use
   it, and/or the environment variable in
   `.github/actions/nexus-cache/action.yml` for npm/pip.
3. Add a row to the table above, commit, and merge to `develop`. Flux reruns
   the provisioning Job because the script hash changed.

The raw `github` proxy (not used by CI) follows GitHub's redirect to
`objects.githubusercontent.com`; Nexus's HTTP client follows redirects for
raw proxies. Verify a real release asset after an upgrade:

```bash
curl -fsS -u ci-reader:<password> \
  https://ci-nexus.<public-zone>/repository/github/cli/cli/releases/download/v2.62.0/gh_2.62.0_linux_amd64.tar.gz \
  -o /dev/null
```

If a future Nexus release stops following that redirect, add a second raw
proxy for `objects.githubusercontent.com` and point the downloader rule at it;
the redirect target is a signed URL, so it cannot be rewritten per object.

## Rotating the `ci-reader` password

1. Write the new value to OpenBao `secret/k8s/nexus/nexus-secrets`
   (`ci_reader_password`), keeping `admin_password` unchanged.
2. Force ESO to sync (`annotate externalsecret nexus-secrets force-sync=...`),
   then recreate the provisioning Job as above. It PUTs the user with the new
   password.
3. Update the GitHub secret `NEXUS_CI_PASSWORD` with the same value. CI reads
   it on the next run; no workflow change is needed.

Rotating the `admin` password is different: change it through the Nexus UI or
`PUT /service/rest/v1/security/users/admin/change-password`, then write the new
value to OpenBao. The init container only seeds a fresh volume, so updating
OpenBao alone does not rotate a live instance, and the provisioning Job will
fail if OpenBao and the live password disagree.

## Disabling the CI switch

CI uses Nexus only when the repository variable `NEXUS_HOST` is set and the
secret `NEXUS_CI_PASSWORD` is present. To fall back to direct downloads:

```bash
gh variable delete NEXUS_HOST
```

`.github/actions/nexus-cache/action.yml` also probes
`https://$NEXUS_HOST/service/rest/v1/status` and leaves the build on the
original URLs when Nexus is unreachable, so an outage degrades to the old
behaviour instead of breaking CI. To enable again, set `NEXUS_HOST` (for
example `ci-nexus.example.com`) and the secret.

Local developer builds never read the Nexus files: the downloader config is
rendered by the CI action into `$RUNNER_TEMP`, and `~/.netrc` is written only
in that action.

## Resource usage

| Resource | Request | Limit |
|---|---|---|
| CPU | 200m | — (no CPU limit, so short metadata bursts are not throttled) |
| Memory | 1Gi | 2500Mi |
| JVM heap | 1Gi (`-Xms` = `-Xmx`) | direct buffers 1Gi |
| Volume | 50Gi `local-retain` | — |

Expect roughly 1.5–2.2 GiB resident after warm-up and tens of megabytes per
cached artifact, dominated by Maven jars and GitHub tarballs. The volume lives
on `/srv/k3s` on the single node; it is a cache, so losing it only forces a
cold refill.

## Internet exposure

The internal name is an ordinary `home-allowlist` route. CI needs the existing
public path, not a new inbound port:

1. The existing remotely-managed tunnel `home-k3s` (cloudflared in the
   `currency-bot` namespace) carries a second route: hostname
   `ci-nexus.<public-zone>`, path `^/repository/`, origin
   `http://nexus.nexus.svc.cluster.local:8081`. Every other path falls through
   to the tunnel's `http_status:404`. cloudflared's egress policy opens only
   `nexus:8081` for this. The route and its proxied CNAME are managed through
   the Cloudflare API (token `CF_TUNNEL_API_TOKEN` in
   `secret/infra/cloudflare/currency-bot-tunnel`).
   The public name sits directly under the Cloudflare zone (one label, e.g.
   `ci-nexus.<public-zone>`): Universal SSL does not cover
   `*.home.<public-zone>`. The `ci-nexus.home.*` Traefik route stays for LAN
   checks only.
2. Set the GitHub repository variable `NEXUS_HOST=ci-nexus.<public-zone>`.
3. Set the GitHub repository secret `NEXUS_CI_PASSWORD` to the
   `ci_reader_password` value from OpenBao.

The public route allows only `/repository/*` (enforced by the tunnel path
rule); the Traefik limits apply to the internal name. The UI and `/service/rest/*` are not
routed on the public hostname.

## Checks

```bash
mise run test nexus-deploy
mise run test flux-deploy
mise run lint:shell
mise run lint:actions
mise run audit
```

## Rollback

Nexus is reconciled from Git. To roll back, revert the offending commit on
`develop`; Flux applies the previous manifests. The provisioning Job reruns
only when `provision.sh` changed. For an emergency local rollback, suspend the
Kustomizations first, then re-apply a previous commit manually:

```bash
mise run kubectl -- -n flux-system suspend kustomization/nexus-provision
mise run kubectl -- -n flux-system suspend kustomization/nexus
```

To remove the cache entirely (data included), delete the two Flux
Kustomizations and then the namespace; do not delete the namespace while Flux
still owns objects in it:

```bash
mise run kubectl -- -n flux-system delete kustomization nexus-provision nexus
mise run kubectl -- delete namespace nexus
```

The internal A record is deleted separately through the Cloudflare API; the
public tunnel hostname is owned by the tunnel configuration.
