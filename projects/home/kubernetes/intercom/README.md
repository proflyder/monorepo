# intercom в K3s

Сигналинг звонка телефон ↔ колонка; код и контракт —
[`solutions/intercom`](../../../../solutions/intercom/README.md).

- `manifests/intercom.yaml` — Deployment (1 реплика, `Recreate`: состояние
  звонков in-memory), NodePort `${INTERCOM_NODE_PORT}`, пробы
  `intercom -healthcheck`.
- `manifests/external-secrets.yaml` — `ExternalSecret/intercom`.
- HTTPS-маршрут `intercom.<HOME_DOMAIN>` и NetworkPolicy `intercom-ingress`
  принадлежат ingress-юниту (`../ingress/routes/`): LAN/VPN, pod CIDR,
  kubelet probes.

## Деплой

Flux (`Kustomization/app-intercom`) применяет `manifests/` из `develop`,
образ `INTERCOM_IMAGE` — из `deploy/home-apps`. Секреты (`docs/SECRETS.md`):

- `secret/k8s/home-assistant/scheduler-auth#token` → `phone_token` (общий
  токен приложения);
- `secret/k8s/home-assistant/intercom#station_token` (≥32 случайных байта, не
  равен токену приложения);
- `secret/k8s/flux-system/app-settings#INTERCOM_NODE_PORT`.

NodePort фильтруется на узле `../scheduler/nft` (LAN/VPN и pod CIDR); после
смены порта или сети переустановить фильтр. NodePort — аварийный путь на
случай проблем с DNS/Traefik.

Откат — revert pins-коммита (при необходимости сначала
`flux suspend kustomization/app-intercom`); ручного deploy-скрипта нет.

## Клиенты

Firebase Remote Config, JSON-параметр `home_connection`:

```json
{"intercomBaseUrl": "https://intercom.home.<home-suffix>"}
```

Без `/v1`, без порта и без `/` в конце; через Traefik обязателен `https`,
аварийный NodePort допускает `http`.

`Watch` — долгий server-streaming Connect поверх HTTP/1.1: маршрут задаёт
`flushInterval: 100ms`, Traefik — `writeTimeout: 0s`, `idleTimeout: 1800s`.

## Проверка

```bash
TOKEN=$(tr -d '\r\n' < secrets/home/intercom.token)
curl -fsS -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d '{}' \
  "https://intercom.home.<home-suffix>/home.v1.IntercomService/ListEndpoints"
curl -sS -N --http1.1 --max-time 120 -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' -d '{}' \
  "https://intercom.home.<home-suffix>/home.v1.IntercomService/Watch"
```

`ListEndpoints` → `{}` или `{"endpoints":[...]}`; `Watch` держится до
`--max-time` без `504`/обрыва через 60 с.
