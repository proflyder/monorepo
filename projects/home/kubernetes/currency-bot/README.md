# currency-bot deployment (home K3s)

Telegram currency bot (`solutions/currency-bot`, Rust) on the single-node
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
- CI unit `currency-bot` (Rust crate + musl image) is green; Flux
  (`Kustomization/app-currency-bot`) applies it. Private env lives in OpenBao
  `secret/k8s/flux-system/app-settings`.
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

1. **Namespace** (cluster-scoped, applied by an admin; Flux does not own it):

   ```bash
   mise run kubectl -- apply -f projects/home/kubernetes/currency-bot/manifests/namespace.yaml
   ```

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

4. **Deploy-time env** in OpenBao `secret/k8s/flux-system/app-settings`
   (ESO materialises `Secret/flux-app-settings`; Flux substitutes it):

   | Key | Meaning |
   |---|---|
   | `CURRENCY_WEBHOOK_URL` | `https://tg.example.com/telegram/webhook` |
   | `LAN_CIDR` | LAN range for NetworkPolicy |
   | `NODE_LAN_IP` | K3s node address (ingress allow-list) |
   | `CURRENCY_BOT_NODE_PORT` | LAN NodePort for `GET /api/latest`, e.g. `30100` |

   `CURRENCY_BOT_NODE_PORT` must be free on the node (scheduler uses
   `30098/30099`) and is also rendered into the host nftables filter.

4b. **Host nftables filter** (root on the K3s node, once and after a CIDR or
   port change, not part of CI):

   ```bash
   ssh <node> sudo env PRIVATE_ENV=/path/to/currency-bot.env \
     bash projects/home/kubernetes/currency-bot/nft/apply.sh
   ```

   It installs `table inet home_currency_bot` and leaves the Kubernetes and
   scheduler tables untouched.

5. **Cloudflare side:** create/confirm the tunnel `home-k3s`, its credentials
   token and a public hostname route `tg.example.com → http://currency-bot-webhook.currency-bot.svc.cluster.local:8081`
   restricted to `^/telegram/webhook$`. These are Cloudflare operations and are
   not automated here.

6. **DNS/TLS for the internal route:** `currency.<HOME_DOMAIN>` A record →
   node (DNS only, manual `cloudflare_dns.py`). The IngressRoute and wildcard
   certificate are already reconciled by Flux (`platform-routes`).

## Deploy

Flux applies the unit (`Kustomization/app-currency-bot`) from `develop`, with
digests from `deploy/home-apps` and values from `Secret/flux-app-settings`.
After a merge, `deploy-home`'s `pins` job promotes the verified digest; there
is no CI VPN, kubeconfig or deploy script.

Checks:
Checks:

```bash
mise run kubectl -- -n currency-bot get deploy,pod,svc,externalsecret
mise run kubectl -- -n currency-bot get endpointslices
mise run kubectl -- -n currency-bot logs deploy/cloudflared --tail=50

# LAN NodePort used by HA Assist (Bearer; token never printed)
curl -fsS "http://<K3s node>:<CURRENCY_BOT_NODE_PORT>/api/latest" \
  -H "Authorization: Bearer $API_TOKEN"
mise run kubectl -- -n currency-bot get --raw \
  "/api/v1/namespaces/currency-bot/services/http:currency-bot:8080/proxy/readyz"
```

## Rollback

Revert the pins commit that changed `CURRENCY_BOT_IMAGE` (or
`git checkout` the previous manifest commit) and Flux re-applies it within
1–2 minutes. `cloudflared` is pinned by a reviewed `versions.sh` literal, so it
rolls back with a normal PR. For a drill, freeze first:

```bash
mise run kubectl -- -n flux-system suspend kustomization/app-currency-bot
# revert the pins commit, then resume
mise run kubectl -- -n flux-system resume kustomization/app-currency-bot
```

Do not delete the Kustomization: `prune: true` would remove the workload; every
PVC carries `kustomize.toolkit.fluxcd.io/prune: disabled`.

Full removal (history is lost unless the PVC is backed up first):

```bash
mise run kubectl -- -n currency-bot delete deployment cloudflared currency-bot
mise run kubectl -- -n currency-bot delete pvc currency-bot-data   # deletes SQLite
mise run kubectl -- delete namespace currency-bot
```

Also remove the Cloudflare tunnel route and call Telegram `deleteWebhook`.
