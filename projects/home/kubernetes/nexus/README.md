# Nexus — artifact cache for CI

Single-replica Sonatype Nexus Repository OSS on the home K3s node (namespace
`nexus`, 50Gi `local-retain` volume — a cache, losing it only forces a cold
refill). CI uses it as a **fallback** for Bazel Maven/PyPI/npm downloads:
upstream first, Nexus when the direct URL fails with 429/5xx or a dropped
connection; CI pip/npm steps point at the Nexus proxies.

GitHub release assets, codeload archives and Node.js distributions are **not**
taken from Nexus (`platform/bazel/downloader.cfg` has no rewrite for them):
large archives through the home uplink broke parallel runners with
`Premature EOF`. They come from the Bazel repository cache saved by the
develop `warm-rust` job. The `github`, `github-codeload`, `nodejs-dist`
proxies stay provisioned for manual use.

- Internal UI/admin: `https://nexus.<home-domain>/` (LAN/VPN, `home-allowlist`).
- CI: `https://ci-nexus.<public-zone>/repository/` through the existing
  `home-k3s` Cloudflare Tunnel (cloudflared in `currency-bot`), path
  `^/repository/` only; UI and `/service/rest/*` are not routed publicly. The
  public name is one label under the zone because Universal SSL does not cover
  `*.home.<public-zone>`.

| Repository | Type | Upstream |
|---|---|---|
| `maven-public` | group | `maven-central`, `google-maven` |
| `maven-central` | proxy | `https://repo1.maven.org/maven2/` |
| `google-maven` | proxy | `https://maven.google.com/` |
| `github` / `github-codeload` / `nodejs-dist` | raw proxy | GitHub, codeload, `nodejs.org/dist` |
| `npmjs` | npm proxy | `https://registry.npmjs.org/` |
| `pypi` | pypi proxy | `https://pypi.org/` |
| `crates` | cargo proxy | `https://index.crates.io/` (skipped if the format is unavailable) |

Anonymous access is off; role `ci-reader` grants only
`nx-repository-view-*-*-{read,browse}`. Repositories do not auto-block on
upstream 5xx (one transient answer used to take a repository offline and
poison the negative cache).

## Deploy

Flux: `Kustomization/nexus` applies `manifests/`, `nexus-provision` (depends
on `nexus`) applies `provision/`. `NEXUS_HOST`, `NEXUS_PUBLIC_HOST`,
`NODE_LAN_IP` come from `flux-settings`; image pins are in the two
`kustomization.yaml` files, mirrored in `projects/home/kubernetes/versions.sh`.

`provision.sh` converges blob store, repositories, anonymous access, role and
user over REST. The ConfigMap name carries the script hash, so a changed script
makes Flux (`force: true`) recreate the Job; to rerun by hand delete
`job/nexus-provision` and reconcile `kustomization/nexus-provision`.

First boot seeds `/nexus-data/admin.password` from OpenBao only before
`/nexus-data/db` exists; afterwards the init step is a no-op.

DNS `nexus.<home-domain>` is manual (`cloudflare_dns.py`, see
`projects/home/kubernetes/README.md`); the public hostname belongs to the
tunnel configuration (Cloudflare API, `secret/infra/cloudflare/currency-bot-tunnel`).

## Secrets and CI switch

`secret/k8s/nexus/nexus-secrets` (`admin_password`, `ci_reader_password`),
see `docs/SECRETS.md`. CI uses Nexus only when the repository variable
`NEXUS_HOST` and secret `NEXUS_CI_PASSWORD` are set;
`.github/actions/nexus-cache/action.yml` probes
`https://$NEXUS_HOST/repository/npmjs/-/ping` and writes nothing when Nexus is
unreachable. Disable: `gh variable delete NEXUS_HOST`. Local builds never read
the Nexus config (rendered into `$RUNNER_TEMP` by the action).

Rotation:

- `ci-reader`: new value in OpenBao, `force-sync` the ExternalSecret, recreate
  the provisioning Job, update `NEXUS_CI_PASSWORD`.
- `admin`: change in the Nexus UI or `PUT /service/rest/v1/security/users/admin/change-password`
  first, then write OpenBao. Updating OpenBao alone does not rotate a live
  instance, and provisioning fails while they disagree.

## Adding a proxy

1. `ensure_repo`/`optional_repo` block in `provision.sh` (blob store `ci-cache`).
2. Rewrite in `platform/bazel/downloader.cfg` and/or env in
   `.github/actions/nexus-cache/action.yml`.
3. Row in the table above; merge — Flux reruns provisioning.

## Resources

CPU request 200m, no CPU limit (metadata bursts); memory 1Gi / 2500Mi; heap
1Gi (`-Xms` = `-Xmx`), direct buffers 1Gi. Cold start on an empty volume takes
minutes; probe budgets are generous on purpose.

## Checks and rollback

```bash
mise run test nexus-deploy
mise run kubectl -- -n nexus get statefulset,pod,svc
```

Rollback: revert the commit on `develop`. Emergency: suspend
`kustomization/nexus-provision` and `kustomization/nexus`, then apply a
previous commit by hand. Full removal: delete both Kustomizations, then the
namespace (never the namespace while Flux owns objects in it); the internal A
record separately.
