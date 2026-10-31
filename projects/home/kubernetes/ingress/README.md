# HTTPS ingress (`*.home.example.com`)

Traefik + cert-manager дают LAN/VPN-доступ к домашним сервисам по именам с
валидным сертификатом Let's Encrypt: `ha`, `grafana`, `alarms`, `control` под
приватным суффиксом (пример — `home.example.com`). Прямые LAN-адреса и
NodePort не удаляются и остаются аварийным путём.

## Состав

| Компонент | Namespace | Назначение |
|---|---|---|
| Traefik `42.2.0` | `traefik` | HTTP/HTTPS вход, редирект 80→443, IP allow-list |
| cert-manager `v1.21.3` | `cert-manager` | ACME DNS-01 Cloudflare, wildcard-сертификат |
| ClusterIssuer | cluster | `letsencrypt-staging`, `letsencrypt-prod` |
| Certificate `home-wildcard` | `traefik` | `*.home.example.com` → Secret `home-wildcard-tls` |
| IngressRoute ×4 | `traefik` | маршруты к Service в `monitoring`/`home-assistant` |

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
  ExternalSecret; `deploy.sh --bootstrap-secrets` — только первая установка
  без OpenBao/ESO.

## Приватные значения

Публичные файлы используют примеры `home.example.com` и `192.0.2.0/24`.
Реальные значения — в untracked `secrets/home/ingress.env`:

```bash
HOME_DOMAIN=home.example.com
LAN_CIDR=192.0.2.0/24
HAOS_ADDRESS=192.0.2.28
CLOUDFLARE_API_TOKEN_FILE=/private/path/cloudflare.env
```

`CLOUDFLARE_API_TOKEN_FILE` содержит `CLOUDFLARE_API_TOKEN=<token>` (Zone DNS
Edit только на зону; IP-фильтр — домашний внешний адрес). Токен читается
скриптом из файла и попадает только в Kubernetes Secret; в argv, логи и Git он
не передаётся.

DNS: A-записи `ha`, `grafana`, `alarms`, `control` → узел K3s, Cloudflare
**DNS only** (не proxied), TTL auto. Wildcard-сертификат не покрывает apex.

## Деплой

```bash
export KUBECONFIG=/private/path/kubeconfig
# 1) проверка DNS-01 на staging (не тратит prod-лимит)
./projects/home/kubernetes/ingress/deploy.sh --staging
# 2) боевой сертификат Let's Encrypt prod + маршруты
./projects/home/kubernetes/ingress/deploy.sh
```

Скрипт идемпотентен: `helm upgrade --install`, `kubectl apply`, Secret
пересоздаётся из файла. Порядок: cert-manager → Secret → ClusterIssuer →
Certificate → Traefik → Middleware/IngressRoute. Версии чартов — в
`../versions.sh` (`TRAEFIK_CHART_VERSION`, `CERT_MANAGER_CHART_VERSION`).

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
kubectl -n cert-manager get pods
kubectl -n traefik get pods
kubectl -n traefik get certificate,secret
kubectl -n traefik get ingressroute,middleware
kubectl -n traefik logs deploy/traefik --tail=50
kubectl -n cert-manager get challenge,order
```

HA после запросов: нет `ip_ban` для адреса узла, `http://192.0.2.28:8123`
по-прежнему отвечает 200.

## Откат

Маршруты и прокси можно убрать, не трогая приложения:

```bash
kubectl -n traefik delete ingressroute --all
helm uninstall traefik -n traefik
kubectl delete certificate home-wildcard -n traefik
helm uninstall cert-manager -n cert-manager
kubectl delete clusterissuer letsencrypt-staging letsencrypt-prod
kubectl delete namespace traefik cert-manager
```

Прямые LAN-адреса и NodePort 30098/30099/30300 продолжают работать
независимо от этого каталога. DNS-записи удаляются отдельно через Cloudflare
API.
