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
- HTTPS-маршрут `intercom.home.<home-suffix>` — в
  `projects/home/kubernetes/ingress/manifests/routes.yaml`
  (`INTERCOM_HOST`), а не здесь: namespace/allow-list/TLS принадлежат
  ingress-юниту. Адрес для клиентов: `https://intercom.home.<home-suffix>/`
  (LAN/VPN, `home-allowlist`, wildcard TLS). В Firebase Remote Config это
  значение поля `intercomBaseUrl` в JSON-параметре `home_connection`:

  ```json
  {"intercomBaseUrl": "https://intercom.home.<home-suffix>"}
  ```

  Без завершающего `/v1`, без `:30097` и без `/` в конце. Для имени через
  Traefik обязателен `https`; аварийный LAN NodePort допускает `http`.
- `manifests` ingress-юнита также создаёт NetworkPolicy `intercom-ingress`
  (`intercom-networkpolicy.yaml`): к pod `intercom:8099` допускаются LAN/VPN
  (`LAN_CIDR`, включая узел с Traefik в `hostNetwork`), pod CIDR и kubelet
  probes. NodePort-путь идёт с тех же адресов и политикой не блокируется.

`Watch` — долгоживущий server-streaming ответ Connect поверх HTTP/1.1.
Маршрут явно задаёт `responseForwarding.flushInterval: 100ms`, а
`ingress/values/traefik.yaml` — `writeTimeout: 0s` и `idleTimeout: 1800s`,
поэтому Traefik не буферизует кадры и не обрывает стрим по таймауту.
NodePort `${INTERCOM_NODE_PORT}` (`30097` на стенде) сохраняется как
аварийный путь на случай проблем с DNS/Traefik.

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

## Проверка внутреннего имени

С LAN/VPN, с bearer-токеном приложения (тот же `phone_token` из
`secret/k8s/home-assistant/scheduler-auth`):

```bash
# Токен — общий токен приложения. Держите его в локальном private-файле и не
# печатайте: TOKEN=$(tr -d '\r\n' < secrets/home/intercom.token)
TOKEN=...

curl -fsS --resolve "intercom.home.<home-suffix>:443:<NODE_IP>" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{}' \
  "https://intercom.home.<home-suffix>/home.v1.IntercomService/ListEndpoints"
```

Ожидается `{}` (пустое repeated-поле protobuf JSON опускает) или
`{"endpoints":[...]}` с зарегистрированными endpoint.
Проверка `Watch` (стрим не должен обрываться):

```bash
curl -sS -N --http1.1 --max-time 120 \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' -d '{}' \
  "https://intercom.home.<home-suffix>/home.v1.IntercomService/Watch"
```

Стрим держится открытым до `--max-time` (или до события), без ответа
`504`/обрыва через 60 секунд. Резервный путь:
`curl -H "Authorization: Bearer $TOKEN" http://<NODE_IP>:30097/healthz`.
