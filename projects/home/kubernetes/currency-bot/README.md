# currency-bot deployment (home K3s)

Telegram currency bot (`solutions/currency-bot`, Rust; scraping, alerts, API
and SQLite schema live there) in namespace `currency-bot`:

- Deployment `currency-bot` — one SQLite writer on PVC `currency-bot-data`
  (1 GiB `local-retain`, `prune: disabled`); Services `currency-bot` (`:8080`
  API/metrics, also NodePort `CURRENCY_BOT_NODE_PORT` for HA Assist) and
  `currency-bot-webhook` (`:8081`).
- Deployment `cloudflared` — remotely-managed Cloudflare Tunnel `home-k3s`.
  Public route `tg.<zone>` → `currency-bot-webhook:8081`, path
  `^/telegram/webhook$` only (everything else 404). The same tunnel carries the
  public Nexus route (`../nexus/README.md`).
- LAN/VPN API `https://currency.<HOME_DOMAIN>/api/*` (Bearer, `home-allowlist`).

## Deploy

Flux (`Kustomization/app-currency-bot`, path `manifests/`): image
`CURRENCY_BOT_IMAGE` from `deploy/home-apps`; `cloudflared` is a reviewed
`versions.sh` literal. The namespace is cluster-scoped and admin-applied once
(`mise run kubectl -- apply -f .../manifests/namespace.yaml`); Flux never
creates or prunes it.

Secrets (`docs/SECRETS.md`):

| OpenBao path | Keys |
|---|---|
| `secret/k8s/currency-bot/currency-bot` | `BOT_TOKEN`, `CHAT_ID`, `WEBHOOK_SECRET`, `API_TOKEN` |
| `secret/k8s/currency-bot/cloudflared-tunnel` | `TUNNEL_TOKEN` |
| `secret/k8s/flux-system/app-settings` | `CURRENCY_WEBHOOK_URL`, `CURRENCY_BOT_NODE_PORT`, `LAN_CIDR`, `NODE_LAN_IP` |

Manual, outside Flux:

- Host nftables filter for the NodePort (`LAN_CIDR` + pod CIDR, IPv6 dropped,
  own `table inet home_currency_bot`), after any CIDR/port change:
  `ssh <node> sudo env PRIVATE_ENV=… bash projects/home/kubernetes/currency-bot/nft/apply.sh`.
  The NodePort must not collide with the scheduler ports `30098/30099`.
- Cloudflare tunnel, its token and the public hostname route.
- DNS `currency.<HOME_DOMAIN>` (`cloudflare_dns.py`, `../README.md`).

## Network

- `8081`: ingress only from `cloudflared` pods (same namespace — no
  cross-namespace grant, which is why the tunnel lives here).
- `8080`: `monitoring`, node address and LAN CIDR (Traefik is `hostNetwork`;
  HAOS hits the NodePort with its LAN address), pod CIDR. The route is still
  gated by `home-allowlist`; the policy is defence in depth.
- Egress 443 to any destination (Telegram and the rate source publish no stable
  ranges) and DNS. `cloudflared`: egress 7844/UDP and 443/TCP, ingress 2000
  (metrics/ready) from node and monitoring.

## Checks

```bash
mise run kubectl -- -n currency-bot get deploy,pod,svc,externalsecret
mise run kubectl -- -n currency-bot logs deploy/cloudflared --tail=50
curl -fsS "http://<node>:<CURRENCY_BOT_NODE_PORT>/api/latest" -H "Authorization: Bearer $API_TOKEN"
```

## Rollback

Revert the pins commit that changed `CURRENCY_BOT_IMAGE`; Flux re-applies the
previous digest. `cloudflared` rolls back with a normal PR. For a drill:
`flux suspend kustomization/app-currency-bot`, revert, resume. Do not delete
the Kustomization (`prune: true` would remove the workload).

Full removal loses history unless the PVC is backed up: suspend, delete both
Deployments, the PVC and the namespace, remove the tunnel route and call
Telegram `deleteWebhook`.
