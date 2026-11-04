# currency-bot deployment (home K3s)

Telegram currency bot (`solutions/currency-bot`, Go) on the single-node
`lab-k3s` cluster, with its SQLite history on a `local-retain` PVC and the
Telegram webhook delivered through a Cloudflare Tunnel.

- `solutions/currency-bot` owns scraping, alerts, the JSON API and the SQLite
  schema.
- The home server is authoritative for scheduling and storage; Telegram is only
  the delivery channel.

## Current state

- Namespace `currency-bot`: Deployment `currency-bot` (PVC `currency-bot-data`,
  1 GiB `local-retain`, digest-pinned image) and Deployment `cloudflared`
  (remotely-managed Cloudflare Tunnel `home-k3s`).
- Public webhook `https://tg.example.com/telegram/webhook`; the tunnel route is
  path-scoped to `^/telegram/webhook$` and everything else returns 404.
  Verified: no secret token → 401, valid token → 200; Telegram `getWebhookInfo`
  points at the tunnel with `allowed_updates=["message"]`.
- LAN/VPN API `https://currency.home.<domain>/api/*` (Bearer, `home-allowlist`);
  `ServiceMonitor` scrapes `/metrics` (verified `up=1`).
- CI units `currency-bot` (Go, image, deploy) and `currency-bot-deploy`
  (deploy-script contract test) are green; the deploy env lives in OpenBao
  `secret/ci/deploy-home` key `currency_bot_env`.
- Observability: Alloy ships the namespace's logs with secret redaction and the
  Grafana dashboard `currency-bot` (uid `currency-bot`) is live.

## Files

```text
manifests/namespace.yaml        namespace (admin-applied; cluster-scoped)
manifests/currency-bot.yaml     PVC + Deployment + ClusterIP Services (template)
manifests/cloudflared.yaml      Cloudflare Tunnel Deployment (template)
manifests/external-secrets.yaml ExternalSecrets for currency-bot/cloudflared-tunnel
manifests/network-policies.yaml ingress/egress policies (template, LAN_CIDR)
manifests/servicemonitor.yaml   Prometheus scrape of /metrics
nft/currency-bot-nodeport.nft   API NodePort source filter (host config, template)
nft/apply.sh                    install/refresh that filter as root on the node
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
  The same Service is a `NodePort` for HA Assist: HAOS keeps its LAN source
  address, which the LAN CIDR rule admits. The host nftables filter
  (`nft/currency-bot-nodeport.nft`, installed by `nft/apply.sh`) narrows the
  NodePort itself to `LAN_CIDR` + pod CIDR and drops IPv6 before kube-proxy
  DNAT, exactly like the scheduler NodePort filter.
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
   | `secret/k8s/currency-bot/currency-bot` | `BOT_TOKEN`, `CHAT_ID`, `WEBHOOK_SECRET`, `API_TOKEN` | `currency-bot/currency-bot` |
   | `secret/k8s/currency-bot/cloudflared-tunnel` | `TUNNEL_TOKEN` | `currency-bot/cloudflared-tunnel` |

   After writing the values, apply the ExternalSecrets (or run
   `external-secrets/deploy.sh`) and wait for `Ready`.

4. **Deploy-time env** in `secrets/home/currency-bot.env`
   (copy `currency-bot.env.example`):

   ```bash
   WEBHOOK_URL=https://tg.example.com/telegram/webhook
   LAN_CIDR=<LAN CIDR>
   NODE_LAN_IP=<K3s node address>
   CURRENCY_BOT_NODE_PORT=<unused NodePort, e.g. 30100>
   ```

   `WEBHOOK_URL` is not secret but it is deployment-specific, so it is never
   committed. `CURRENCY_BOT_NODE_PORT` is the LAN NodePort for
   `GET /api/latest` used by HA Assist; it must be free on the node (scheduler
   uses `30098/30099`) and is also rendered into the host nftables filter.
   CI reads it from the `currency_bot_env` key of `secret/ci/deploy-home`
   (same file text); that key is already provisioned.

4b. **Host nftables filter** (root on the K3s node, once and after a CIDR or
   port change, not part of CI):

   ```bash
   ssh <node> sudo env PRIVATE_ENV=/path/to/currency-bot.env \
     bash projects/home/kubernetes/currency-bot/nft/apply.sh
   ```

   It installs `table inet ricardo_currency_bot` and leaves the Kubernetes and
   scheduler tables untouched.

5. **Cloudflare side:** create/confirm the tunnel `home-k3s`, its credentials
   token and a public hostname route `tg.example.com → http://currency-bot-webhook.currency-bot.svc.cluster.local:8081`
   restricted to `^/telegram/webhook$`. These are Cloudflare operations and are
   not automated here.

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
`platform/ci/units.mjs`, the image is built/pushed by Bazel, then `deploy.sh`
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

It reuses the generic `home-deploy converge`/`home-deploy state rollback` and the strict
`home-deploy render --unit currency-bot` renderer (`platform/deploy`, decision
35); support objects (PVC, Services, policies, ServiceMonitor, ExternalSecrets)
are applied, Deployments are replaced so removed fields do not linger.

Checks:

```bash
kubectl -n currency-bot get deploy,pod,svc,externalsecret
kubectl -n currency-bot get endpointslices
kubectl -n currency-bot logs deploy/cloudflared --tail=50

# LAN NodePort used by HA Assist (Bearer; token never printed)
curl -fsS "http://<K3s node>:<CURRENCY_BOT_NODE_PORT>/api/latest" \
  -H "Authorization: Bearer $API_TOKEN"
kubectl -n currency-bot get --raw \
  "/api/v1/namespaces/currency-bot/services/http:currency-bot:8080/proxy/readyz"
```

## Rollback

`deploy.sh` records each Deployment before it changes it and writes an
`.applied` marker only after the change. CI restores exactly those Deployments
with the generic scheduler script:

```bash
home-deploy state rollback \
  --state-dir secrets/local/rendered/currency-bot/state \
  --namespace currency-bot
```

This restores image, env, replicas and probes from the recorded object without
`kubectl rollout undo`. Support objects (ExternalSecrets, NetworkPolicies,
ServiceMonitor) are idempotent and intentionally not rolled back; the previous
manifests can be re-applied by checking out the previous commit and re-running
`deploy.sh`.

There is no fallback outside K3s: roll back with the script above or fix
forward.

Full removal (history is lost unless the PVC is backed up first):

```bash
kubectl -n currency-bot delete deployment cloudflared currency-bot
kubectl -n currency-bot delete pvc currency-bot-data   # deletes SQLite
kubectl delete namespace currency-bot
kubectl delete -f projects/home/kubernetes/currency-bot/ci/rbac.yaml
```

Also remove the Cloudflare tunnel route and call Telegram `deleteWebhook`.
