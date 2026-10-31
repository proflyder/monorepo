# currency-bot deployment (home K3s)

Telegram currency bot (`solutions/currency-bot`, Go) on the single-node
`lab-k3s` cluster, with its SQLite history on a `local-retain` PVC and the
Telegram webhook delivered through a Cloudflare Tunnel.

- `solutions/currency-bot` owns scraping, alerts, the JSON API and the SQLite
  schema.
- The home server is authoritative for scheduling and storage; Telegram is only
  the delivery channel.
- The old VPS deployment (`/app/currency-bot`, H2 + Quartz + Promtail) is
  retired after the cutover below.

## Files

```text
manifests/namespace.yaml        namespace (admin-applied; cluster-scoped)
manifests/currency-bot.yaml     PVC + Deployment + ClusterIP Services (template)
manifests/cloudflared.yaml      Cloudflare Tunnel Deployment (template)
manifests/external-secrets.yaml ExternalSecrets for currency-bot/cloudflared-tunnel
manifests/network-policies.yaml ingress/egress policies (template, LAN_CIDR)
manifests/servicemonitor.yaml   Prometheus scrape of /metrics
ci/rbac.yaml                    Role/RoleBinding for home-assistant/ci-deploy
deploy.sh                       build/push/import image, render, apply, roll out
currency-bot.env.example        placeholders for secrets/home/currency-bot.env
```

Tracked templates contain only placeholders/TEST-NET values. Rendered manifests
go to `secrets/local/rendered/currency-bot` (git-ignored).

## Design choices

### Cloudflare Tunnel in the `currency-bot` namespace

The tunnel runs in the same namespace as the bot instead of a shared
`cloudflared` namespace:

- the webhook NetworkPolicy selects tunnel pods with a plain
  `podSelector` (`app.kubernetes.io/name: cloudflared`) — no namespaceSelector
  and no cross-namespace grant;
- the tunnel token is one more ExternalSecret in the same ESO apply set and the
  same CI Role;
- the tunnel has exactly one consumer (the webhook Service), so a shared
  namespace would add RBAC/ownership without reuse.

The tunnel ingress on the Cloudflare side already points at
`http://currency-bot-webhook.currency-bot.svc.cluster.local:8081`.

### NetworkPolicies

K3s enforces NetworkPolicies (kube-router). The shapes are:

- **8081 (webhook):** ingress only from `cloudflared` pods.
- **8080 (API/metrics):** ingress from the `monitoring` namespace
  (Prometheus), from the node address and LAN CIDR (Traefik is `hostNetwork`,
  so its packets are seen as node/LAN traffic), and from the pod CIDR
  (API-server proxy used by CI smoke, in-cluster clients, kubelet probes).
- **Egress:** 443/TCP to any destination plus DNS 53/UDP+TCP. Telegram and
  kurs.kz do not publish fixed ranges, so pinning ipBlocks would break silently.
- **cloudflared:** egress 7844/UDP (QUIC) and 443/TCP (fallback) plus DNS;
  ingress 2000/TCP (metrics/`/ready`) from the node and monitoring.

Trade-offs:

- A host-network process on the node can reach 8080; the Service is ClusterIP,
  so nothing outside the node/VPN can. Distinguishing Traefik from other
  host-network pods would require a CNI with host-endpoint identity (not
  kube-router).
- Allowing the LAN CIDR to 8080 preserves the source address when kube-proxy
  does not SNAT hostNetwork→ClusterIP. The HTTP route is still gated by the
  Traefik `home-allowlist` middleware; the NetworkPolicy is defence in depth,
  not the only control.
- Egress to 443 is any-destination because both upstreams are third-party
  HTTPS. If they gain stable ranges, narrow the egress ipBlock.

## One-time prerequisites

1. **Namespace and CI RBAC** (cluster-scoped, applied by an admin, not CI):

   ```bash
   export KUBECONFIG=/private/path/kubeconfig
   kubectl apply -f projects/home/kubernetes/currency-bot/manifests/namespace.yaml
   kubectl apply -f projects/home/kubernetes/currency-bot/ci/rbac.yaml
   ```

   `ci/rbac.yaml` binds the existing `home-assistant/ci-deploy`
   ServiceAccount into `currency-bot` with the same namespace-scoped verbs plus
   `services/proxy` (the CI smoke reads `/readyz` through the API server).

2. **ESO/OpenBao** is already installed:
   `projects/home/kubernetes/openbao/deploy.sh`,
   `projects/home/kubernetes/external-secrets/deploy.sh`.

3. **OpenBao values** (never in Git; use the procedure in
   `projects/home/kubernetes/external-secrets/README.md`):

   | OpenBao path | Keys | Kubernetes Secret |
   |---|---|---|
   | `secret/k8s/currency-bot` | `BOT_TOKEN`, `CHAT_ID`, `WEBHOOK_SECRET`, `API_TOKEN` | `currency-bot/currency-bot` |
   | `secret/k8s/cloudflared` | `TUNNEL_TOKEN` | `currency-bot/cloudflared-tunnel` |

   After writing the values, apply the ExternalSecrets (or run
   `external-secrets/deploy.sh`) and wait for `Ready`.

4. **Deploy-time env** in `secrets/home/currency-bot.env`
   (copy `currency-bot.env.example`):

   ```bash
   WEBHOOK_URL=https://<public webhook host>/telegram/webhook
   LAN_CIDR=<LAN CIDR>
   NODE_LAN_IP=<K3s node address>
   ```

   `WEBHOOK_URL` is not secret but it is deployment-specific, so it is never
   committed. CI reads it from the `currency_bot_env` key of
   `secret/ci/deploy-home` (same file text). Add that key to the OpenBao secret
   before the first CI deploy.

5. **Cloudflare side:** create/confirm the tunnel, its credentials token and a
   public hostname route `<public webhook host> → http://currency-bot-webhook.currency-bot.svc.cluster.local:8081`.
   These are Cloudflare operations and are not automated here.

6. **DNS/TLS for the internal route:** `currency.<HOME_DOMAIN>` A record →
   node (DNS only), and run `projects/home/kubernetes/ingress/deploy.sh` so the
   IngressRoute and wildcard certificate include the new host.

## Deploy

Manual (workstation, imports the image into the node over SSH):

```bash
export KUBECONFIG=/private/path/kubeconfig
NODE_SSH=labuser@node ./projects/home/kubernetes/currency-bot/deploy.sh
```

CI (`deploy-home`, push to `develop`): the `currency-bot` unit is routed from
`platform/ci/units.yml`, the image is built/pushed by Bazel, then `deploy.sh`
runs with `PUSH_IMAGES=1` and `PRIVATE_ENV=$CI_DIR/currency-bot.env`.

`deploy.sh` is idempotent and:

1. builds `//solutions/currency-bot:currency-bot_image` and resolves the
   digest (never a moving tag);
2. renders the manifests and applies the namespace if it is missing;
3. waits for both ExternalSecrets to be Ready and verifies the Secrets exist
   (ESO owns them; CI stays read-only and never applies ExternalSecrets);
4. applies NetworkPolicies and the ServiceMonitor;
5. converges the `currency-bot` Deployment (PVC/Services via `apply`) and waits
   for rollout;
6. converges the `cloudflared` Deployment and waits for rollout.

It reuses the scheduler's generic `converge.py`/`rollback.py` and strict
`render.py`; support objects (PVC, Services, policies, ServiceMonitor,
ExternalSecrets) are applied, Deployments are replaced so removed fields do not
linger.

Checks:

```bash
kubectl -n currency-bot get deploy,pod,svc,externalsecret
kubectl -n currency-bot get endpointslices
kubectl -n currency-bot logs deploy/cloudflared --tail=50
kubectl -n currency-bot get --raw \
  "/api/v1/namespaces/currency-bot/services/http:currency-bot:8080/proxy/readyz"
```

## Cutover from the VPS (H2 → SQLite)

The importer is `solutions/currency-bot/cmd/currency-bot-import`. It reads the
H2 CSV exports and is idempotent (existing timestamps are skipped).

1. **Prepare the new deployment** (steps 1–6 above), but keep `cloudflared` at
   0 replicas / do not point Telegram at it yet. Deploying the bot pod is safe:
   it writes history but the webhook is still the old one.

2. **Stop the old bot** so H2 is not open twice (compose deployment):

   ```bash
   docker compose stop currency-bot
   ```

3. **Export both tables** from the old image (the fat jar contains H2):

   ```bash
   mkdir -p export
   docker compose run --rm -v "$PWD/export:/export" --entrypoint sh currency-bot -c '
   java -cp /app/app.jar org.h2.tools.Shell \
     -url "jdbc:h2:file:/app/data/currency-history;IFEXISTS=TRUE" \
     -user sa -password "" \
     -sql "CALL CSVWRITE('"'"'/export/currency_history.csv'"'"', '"'"'SELECT * FROM CURRENCY_HISTORY'"'"');
           CALL CSVWRITE('"'"'/export/sent_alerts.csv'"'"', '"'"'SELECT * FROM SENT_ALERTS'"'"');"
   '
   ```

   See `solutions/currency-bot/README.md` for a non-compose one-off container.
   Migrate at least 35 days of history (alert lookbacks) and **all**
   `sent_alerts` rows, otherwise the next run re-sends already delivered alerts.

4. **Import into the live PVC** with the dedicated importer image. The runtime
   image is distroless and only contains the bot, so build the one-off
   `currency-bot-import` image, ship the CSVs in a ConfigMap, scale the bot to
   zero (the PVC is `ReadWriteOnce`) and run one Job:

   ```bash
   # build + push the import helper once (digest-pinned, same distroless base)
   bazel run //solutions/currency-bot:currency-bot-import_push \
     --platforms=@rules_go//go/toolchain:linux_amd64 \
     -- --repository ghcr.io/proflyder/currency-bot-import --tag migration
   digest=$(python3 platform/ci/oci_image.py digest \
     bazel-bin/solutions/currency-bot/currency-bot-import_image)
   import_image="ghcr.io/proflyder/currency-bot-import@${digest}"

   kubectl -n currency-bot create configmap currency-import \
     --from-file=history=export/currency_history.csv \
     --from-file=alerts=export/sent_alerts.csv
   kubectl -n currency-bot scale deployment/currency-bot --replicas=0
   kubectl -n currency-bot wait --for=delete pod \
     -l app.kubernetes.io/name=currency-bot --timeout=120s
   ```

   ```yaml
   # migration-job.yaml
   apiVersion: batch/v1
   kind: Job
   metadata:
     name: currency-import
     namespace: currency-bot
   spec:
     backoffLimit: 0
     template:
       spec:
         restartPolicy: Never
         securityContext:
           runAsNonRoot: true
           runAsUser: 65534
           runAsGroup: 65534
           fsGroup: 65534
         containers:
           - name: import
             image: ghcr.io/proflyder/currency-bot-import@sha256:<digest>
             args:
               - -db
               - /data/currency-bot.db
               - -history
               - /import/history
               - -alerts
               - /import/alerts
             volumeMounts:
               - name: data
                 mountPath: /data
               - name: import
                 mountPath: /import
                 readOnly: true
         volumes:
           - name: data
             persistentVolumeClaim:
               claimName: currency-bot-data
           - name: import
             configMap:
               name: currency-import
   ```

   ```bash
   kubectl -n currency-bot apply -f migration-job.yaml
   kubectl -n currency-bot wait --for=condition=complete job/currency-import --timeout=600s
   kubectl -n currency-bot logs job/currency-import
   kubectl -n currency-bot delete job/currency-import
   kubectl -n currency-bot delete configmap currency-import
   kubectl -n currency-bot scale deployment/currency-bot --replicas=1
   ```

   The importer is idempotent; a re-run skips timestamps already present.

5. **Switch the webhook**:
   - scale `cloudflared` to 1 and confirm the tunnel is connected;
   - call Telegram `setWebhook` for the new public URL with
     `secret_token=<WEBHOOK_SECRET>` and `allowed_updates=["message"]`. The bot
     also calls `setWebhook` at startup when `WEBHOOK_URL` is set, so a restart
     of the new pod is an alternative;
   - send `/trigger` from the allowed chat and confirm the reply arrives once.

6. **Stop the old containers** and disable the old deploy:

   ```bash
   docker compose down
   ```

   Keep the old volume and `.env` until one green week has passed.

7. **Retire old monitoring**: the VPS Promtail/Prometheus currency job and the
   four jsonnet dashboards are replaced by the ServiceMonitor and the Home
   dashboards (delivered by a separate observability change).

## Rollback

`deploy.sh` records each Deployment before it changes it and writes an
`.applied` marker only after the change. CI restores exactly those Deployments
with the generic scheduler script:

```bash
python3 projects/home/kubernetes/scheduler/rollback.py \
  --state-dir secrets/local/rendered/currency-bot/state \
  --namespace currency-bot
```

This restores image, env, replicas and probes from the recorded object without
`kubectl rollout undo`. Support objects (ExternalSecrets, NetworkPolicies,
ServiceMonitor) are idempotent and intentionally not rolled back; the previous
manifests can be re-applied by checking out the previous commit and re-running
`deploy.sh`.

Full removal (history is lost unless the PVC is backed up first):

```bash
kubectl -n currency-bot delete deployment cloudflared currency-bot
kubectl -n currency-bot delete pvc currency-bot-data   # deletes SQLite
kubectl delete namespace currency-bot
kubectl delete -f projects/home/kubernetes/currency-bot/ci/rbac.yaml
```

Also remove the Cloudflare tunnel route and call Telegram `deleteWebhook` or
point `setWebhook` back at the old service.

## Manual checks before the first CI deploy

- [ ] `kubectl -n currency-bot get secret currency-bot cloudflared-tunnel`
- [ ] `kubectl -n currency-bot get externalsecret` → both `SecretSynced=True`
- [ ] `kubectl auth can-i -n currency-bot get deployments` as the CI identity
- [ ] `secret/ci/deploy-home` has `currency_bot_env` with `WEBHOOK_URL`,
      `LAN_CIDR`, `NODE_LAN_IP`
- [ ] `currency.<HOME_DOMAIN>` DNS record and ingress route exist
- [ ] Cloudflare tunnel route targets the webhook Service
