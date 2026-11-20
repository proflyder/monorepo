# pve-exporter

[prometheus-pve-exporter](https://github.com/prometheus-pve/prometheus-pve-exporter)
for Proxmox guest metrics (VM/CT status, CPU, memory, disk, uptime, onboot).
It feeds `GET /v1/system/hosts` of `mcp-gateway` and the Android «Система» tab.
Host metrics (CPU, RAM, disk, temperature) stay on the PVE node exporter
(`job="pve-node"`).

## Objects

- `manifests/external-secret.yaml` — ExternalSecret `monitoring/pve-exporter`
  renders `pve.yml` (module `default`, API token auth, `verify_ssl: false`
  because the PVE API certificate is self-signed) from OpenBao
  `secret/k8s/monitoring/pve-exporter` (`user`, `token_name`, `token_value`).
- `manifests/pve-exporter.yaml` — Deployment + ClusterIP Service `:9221`,
  image pinned by tag and digest, non-root, read-only root filesystem, no
  ServiceAccount token.
- Scrape job `pve-guests` — `observability/values/prometheus.yaml`. It is a
  multi-target probe: the private `${PVE_NODE_TARGET}` (`host:9100`) is
  relabelled to `?target=<host>` and the address is replaced by the exporter
  Service, so the PVE address never enters Git.
- Flux Kustomization `pve-exporter` (`flux/cluster/apps.yaml`), `wait: false`:
  a missing token must not block the `observability` or `platform` reconcile;
  without data the dashboard shows «нет данных» for guests.

## PVE API token (read-only)

Created once on the PVE host as root; the token value is written straight into
OpenBao and is never printed:

```bash
pveum user add pve-exporter@pve --comment "read-only metrics for prometheus-pve-exporter"
pveum acl modify / --users pve-exporter@pve --roles PVEAuditor
pveum user token add pve-exporter@pve dashboard --privsep 0 --output-format json \
  | <script that POSTs {user, token_name, token_value} to
     secret/data/k8s/monitoring/pve-exporter with options.cas=0>
```

`PVEAuditor` grants only `*.Audit` privileges (read state, no console, no
changes). `--privsep 0` makes the token inherit exactly the user's ACL.
Rotation: `pveum user token remove pve-exporter@pve dashboard`, add it again,
write the new value with CAS, then ESO refreshes the Secret within an hour
(or `mise run kubectl -- -n monitoring annotate externalsecret pve-exporter
force-sync=$(date +%s) --overwrite`) and the exporter re-reads `pve.yml` on
restart.

## Checks

```bash
mise run kubectl -- -n monitoring get pods -l app.kubernetes.io/name=pve-exporter
mise run kubectl -- get --raw \
  "/api/v1/namespaces/monitoring/services/monitoring-prometheus:9090/proxy/api/v1/query?query=pve_up"
```
