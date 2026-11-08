# intercom в K3s

Deployment/Service сигналинга звонка; код и контракт —
[`solutions/intercom`](../../../../solutions/intercom/README.md).

## Состав

- `manifests/intercom.yaml` — Deployment (1 реплика, `Recreate`: состояние
  звонков in-memory), NodePort `${INTERCOM_NODE_PORT}`, пробы
  `intercom -healthcheck`.
- `manifests/external-secrets.yaml` — ESO `ExternalSecret/intercom` из
  OpenBao: `phone_token` — из `secret/k8s/home-assistant/scheduler-auth`
  (`token`, общий токен приложения), `station_token` — из
  `secret/k8s/home-assistant/intercom`.
- HTTPS-маршрут `intercom.<домен>` — в
  `projects/home/kubernetes/ingress/manifests/routes.yaml`
  (`INTERCOM_HOST`), а не здесь: namespace/allow-list/TLS принадлежат
  ingress-юниту.

## Деплой

Юнит `intercom` в `platform/ci/units.mjs` — полноценный deploy-юнит:
PR CI собирает `ghcr.io/proflyder/intercom` и публикует `tree-<hash>`,
merge в `develop` продвигает digest и катит Deployment через
`deploy.sh` (build-once, `DEPLOY_NO_BUILD=1`). Приватные значения:

- `secret/k8s/home-assistant/scheduler-auth` → `token` (общий токен приложения);
- `secret/k8s/home-assistant/intercom` → `station_token` (device secret,
  ≥32 случайных байта, не равен токену приложения);
- `intercom_env` в `secret/ci/deploy-home`: `INTERCOM_NODE_PORT`;
- локально `secrets/home/intercom.env` (по образцу
  `intercom.env.example`) нужен только для ручного `deploy.sh`.

NodePort закрыт фильтром `projects/home/kubernetes/scheduler/nft`:
источник — только LAN/VPN и pod CIDR. После изменения порта/сети
переустановить фильтр на узле (см. `docs/OPERATIONS.md`).
