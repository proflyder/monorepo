# HTTPS ingress (`*.home.example.com`)

Traefik + cert-manager дают LAN/VPN-доступ к домашним сервисам по именам под
приватным суффиксом (пример — `home.example.com`) с wildcard-сертификатом
Let's Encrypt. Прямые LAN-адреса и NodePort остаются аварийным путём.

| Каталог | Flux Kustomization | Что |
|---|---|---|
| `cert-manager/` | `platform-cert-manager` | HelmRelease cert-manager (CRDs `CreateReplace`) |
| `traefik/` | `platform-traefik` | HelmRelease Traefik (`hostNetwork`, 80/443, редирект 80→443) |
| `routes/` | `platform-routes` | ClusterIssuers `letsencrypt-staging`/`-prod`, Certificate `home-wildcard`, Middleware `home-allowlist`, IngressRoutes, EndpointSlice HAOS, NetworkPolicy `intercom-ingress`, ExternalSecret `cloudflare-api-token` |

Маршруты (`routes/routes.yaml`): `ha` → HAOS, `grafana`, `alarms` → scheduler,
`currency` → currency-bot, `control` → ha-control (`/v1/system/`,
`/v1/chat/` и `/v1/hermes/` → mcp-gateway), `secrets` → secrets-gateway (owner-only API
секретов), `intercom`. `vault` обслуживает
`openbao/deploy.sh`, `passwords` — `vaultwarden/`, `nexus` — `nexus/`.

Применение и порядок — `../flux/README.md`; версии чартов — `../versions.sh`
(дублируются в HelmRelease, проверяется тестом). Приватные `HOME_DOMAIN`,
`LAN_CIDR`, `HAOS_ADDRESS` подставляются из `flux-settings`. DNS A-записи
(DNS only, не proxied; wildcard не покрывает apex) — вручную,
`../README.md`.

## Неочевидное

- Traefik в `hostNetwork`, поэтому HA видит источником адрес узла и доверяет
  `http.trusted_proxies: <node>/32`; расширять до pod-CIDR не нужно.
  Dashboard/API не публикуются; Prometheus-метрики Traefik выключены (9100
  занят node-exporter).
- Порты 80/443 требуют `NET_BIND_SERVICE`: от UID 65532 capability не доходит
  до effective set (containerd не даёт ambient capabilities, у бинарника нет
  file capabilities). Поэтому root с `drop: [ALL]` + `add: [NET_BIND_SERVICE]`,
  read-only root, без privilege escalation.
- Все маршруты и сертификат — в namespace `traefik` (один Certificate вместо
  копий TLS Secret; нужен `allowCrossNamespace=true`).
- Solver-секрет `cloudflare-api-token` — в namespace `cert-manager`: для
  ClusterIssuer секреты резолвятся в cluster resource namespace. Значение —
  `secret/k8s/cert-manager/cloudflare-api-token` (Zone DNS Edit на одну зону,
  см. `docs/SECRETS.md`).
- `home-allowlist`: LAN CIDR (VPN-клиенты NAT-ятся в адрес LAN), pod CIDR,
  loopback.
- `intercom` `Watch` — долгий server-streaming: маршрут с
  `flushInterval: 100ms`, у `websecure` `readTimeout: 60s`,
  `writeTimeout: 0s`, `idleTimeout: 1800s`.
- Staging-сертификат в Flux не входит; для проверки DNS-01 временно применить
  отдельный `Certificate` с `letsencrypt-staging` и удалить.

## Проверки

```bash
HOME_DOMAIN=home.example.com NODE_IP=192.0.2.27 ./projects/home/kubernetes/ingress/verify.sh
python3 -m unittest projects/home/kubernetes/ingress/test_templates.py
mise run kubectl -- -n traefik get certificate,ingressroute,middleware
mise run kubectl -- -n cert-manager get challenge,order
```

`verify.sh` (read-only): DNS через авторитативный NS, HTTPS-коды, редирект,
health Grafana/control/scheduler, issuer (не staging), WebSocket HA → 101.
После проверок у HA не должно быть `ip_ban` для адреса узла.

## Откат

Revert в `develop`; для аварии — `flux suspend` нужной Kustomization.
Удаление `platform-routes` уберёт маршруты, не трогая приложения; прямые LAN
NodePort (scheduler, control, Grafana) продолжают работать. DNS-записи
удаляются отдельно.
