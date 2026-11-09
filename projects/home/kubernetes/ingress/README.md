# HTTPS ingress (`*.home.example.com`)

Traefik + cert-manager дают LAN/VPN-доступ к домашним сервисам по именам с
валидным сертификатом Let's Encrypt: `ha`, `grafana`, `alarms`, `control`,
`currency` и `intercom` под приватным суффиксом (пример — `home.example.com`). Прямые LAN-адреса и
NodePort не удаляются и остаются аварийным путём.

## Состав

| Компонент | Namespace | Назначение |
|---|---|---|
| Traefik `42.2.0` | `traefik` | HTTP/HTTPS вход, редирект 80→443, IP allow-list |
| cert-manager `v1.21.3` | `cert-manager` | ACME DNS-01 Cloudflare, wildcard-сертификат |
| ClusterIssuer | cluster | `letsencrypt-staging`, `letsencrypt-prod` |
| Certificate `home-wildcard` | `traefik` | `*.home.example.com` → Secret `home-wildcard-tls` |
| IngressRoute ×6 | `traefik` | маршруты к Service в `monitoring`/`home-assistant`/`currency-bot` |
| NetworkPolicy `intercom-ingress` | `home-assistant` | LAN/VPN/NodePort → `intercom:8099`; Traefik работает в `hostNetwork`, поэтому источник — адрес узла или LAN-клиент |

Маршрут `vault.<домен>` и его DNS-запись обслуживает
`projects/home/kubernetes/openbao/deploy.sh` — тот же namespace, allow-list и
wildcard-сертификат.

Traefik работает в `hostNetwork` и слушает 80/443 на узле K3s. Поэтому HA
видит источником `192.0.2.27` (адрес узла) и продолжает доверять
`http.trusted_proxies: 192.0.2.27/32`; расширять `trusted_proxies` до pod-CIDR
не нужно. Dashboard/API не публикуются, access log включён.

Порты `80/443` в host-network namespace требуют `NET_BIND_SERVICE`. Chart по
умолчанию запускает Traefik от UID 65532, а containerd не выдаёт ambient
capabilities non-root-процессу и у бинарника нет file capabilities — capability
не доходит до effective set, и под падает с `bind: permission denied`. Поэтому
контейнер запускается от root с `drop: [ALL]` + `add: [NET_BIND_SERVICE]`,
read-only root и без privilege escalation. Менять
`net.ipv4.ip_unprivileged_port_start` на узле не нужно.

## Namespace и TLS Secret

Все маршруты и wildcard-сертификат живут в одном namespace `traefik`:

- один `Certificate` на весь суффикс вместо копии TLS Secret в каждом
  namespace (меньше ACME-заказов и нет рассинхронизации продления);
- `IngressRoute` ссылается на Service в другом namespace только при
  `providers.kubernetesCRD.allowCrossNamespace=true`;
- Secret `cloudflare-api-token` создаётся в namespace `cert-manager`: для
  `ClusterIssuer` solver-секреты резолвятся в cluster resource namespace
  (`cert-manager`), а не в namespace Certificate; проверено на staging-
  сертификате. Значение хранится в OpenBao
  (`secret/k8s/cert-manager/cloudflare-api-token`) и материализуется
  ExternalSecret (Flux `platform-routes`).

## Приватные значения

Публичные файлы используют примеры `home.example.com` и `192.0.2.0/24`.
Реальные значения — в untracked `secrets/home/ingress.env`:

```bash
HOME_DOMAIN=home.example.com
LAN_CIDR=192.0.2.0/24
NODE_IP=192.0.2.27
HAOS_ADDRESS=192.0.2.28
CLOUDFLARE_API_TOKEN_FILE=/private/path/cloudflare.env
```

`NODE_IP` — LAN-адрес узла K3s: A-записи создаются вручную через
`projects/home/kubernetes/cloudflare_dns.py`. Flux DNS не трогает.
`HOME_DOMAIN`, `LAN_CIDR` и `HAOS_ADDRESS` дублируются в
`secret/k8s/flux-system/settings` и подставляются в манифесты.

`CLOUDFLARE_API_TOKEN_FILE` содержит `CLOUDFLARE_API_TOKEN=<token>` (Zone DNS
Edit только на зону; IP-фильтр — домашний внешний адрес). Токен попадает
только в Kubernetes Secret (`secret/k8s/cert-manager/cloudflare-api-token` →
ESO); в argv, логи и Git он не передаётся.

DNS: A-записи `ha`, `grafana`, `alarms`, `control`, `currency`, `intercom`,
`passwords`, `vault` → узел K3s (`NODE_IP`), Cloudflare **DNS only**
(не proxied), TTL auto. Wildcard-сертификат не покрывает apex.

`intercom.home.<home-suffix>` — сервис сигналинга звонков (`home-assistant/intercom:8099`).
`Watch` — долгоживущий server-streaming ответ Connect поверх HTTP/1.1;
маршрут задаёт `responseForwarding.flushInterval: 100ms`, а в
`values/traefik.yaml` для `websecure` выставлены `readTimeout: 60s`,
`writeTimeout: 0s`, `idleTimeout: 1800s`, чтобы ответ не буферизовался и не
обрывался по таймауту. NodePort остаётся аварийным путём.

`currency.<домен>` — внутренний JSON API бота (`/api/*`, `/metrics`), доступен
только из LAN/VPN через `home-allowlist`. Telegram webhook идёт отдельным
путём через Cloudflare Tunnel и этот маршрут не использует.

## Деплой

Всё, кроме DNS-записей, reconciled Flux (см. `../flux/README.md`):

| Kustomization | Каталог |
|---|---|
| `platform-cert-manager` | `cert-manager/` |
| `platform-traefik` | `traefik/` |
| `platform-routes` | `routes/` |

Umbrella `platform` применяет их по цепочке
ESO → cert-manager → Traefik → маршруты. Helm-релизы сохраняют живые имена и
namespace (`cert-manager`, `traefik`), поэтому принимаются как upgrade.
Версии чартов — в `../versions.sh` (`TRAEFIK_CHART_VERSION`,
`CERT_MANAGER_CHART_VERSION`) и продублированы в HelmRelease (проверяется
тестом).

```bash
# после merge в develop
mise run kubectl -- -n flux-system annotate kustomization/platform \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite
mise run kubectl -- get kustomizations -A
```

DNS-записи (ручной шаг, один раз или при смене адреса):

```bash
mise run secrets:file home/vpn/cloudflare.env
python3 projects/home/kubernetes/cloudflare_dns.py \
  secrets/home/vpn/cloudflare.env <host>.<home-domain> <node-lan-ip>
mise run secrets:clean
```

Staging-сертификат в Flux не входит: DNS-01 проверялся при первой установке.
Для повторной проверки можно временно применить отдельный `Certificate` с
`letsencrypt-staging` вручную и удалить его после проверки.

## Проверки

```bash
HOME_DOMAIN=home.example.com NODE_IP=192.0.2.27 \
  ./projects/home/kubernetes/ingress/verify.sh
```

Проверяются: DNS через авторитативный NS Cloudflare, HTTPS-коды четырёх
хостов, редирект 80→443, `/api/health` Grafana, `/healthz` и `/v1/catalog`
control, `GetStatus` scheduler с bearer, issuer Let's Encrypt (не staging),
WebSocket HA `/api/websocket` → 101. Токены читаются из `secrets/home/*.token` и
не печатаются.

Ручные проверки:

```bash
mise run kubectl -- -n cert-manager get pods
mise run kubectl -- -n traefik get pods
mise run kubectl -- -n traefik get certificate,secret
mise run kubectl -- -n traefik get ingressroute,middleware
mise run kubectl -- -n traefik logs deploy/traefik --tail=50
mise run kubectl -- -n cert-manager get challenge,order
```

HA после запросов: нет `ip_ban` для адреса узла, `http://192.0.2.28:8123`
по-прежнему отвечает 200.

## Откат

Маршруты и прокси можно убрать, не трогая приложения:

```bash
mise run kubectl -- -n traefik delete ingressroute --all
helm uninstall traefik -n traefik
mise run kubectl -- delete certificate home-wildcard -n traefik
helm uninstall cert-manager -n cert-manager
mise run kubectl -- delete clusterissuer letsencrypt-staging letsencrypt-prod
mise run kubectl -- delete namespace traefik cert-manager
```

Прямые LAN-адреса и NodePort 30098/30099/30300 продолжают работать
независимо от этого каталога. DNS-записи удаляются отдельно через Cloudflare
API.
