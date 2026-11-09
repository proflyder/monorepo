# External Secrets Operator

ESO `2.14.0` (chart `2.14.0`, образ по digest) в namespace
`external-secrets`. Он читает KV v2 `secret/` в OpenBao через Kubernetes auth
и материализует Kubernetes Secrets в namespace приложений.

Источник правды для приложенческих секретов — **OpenBao**. Локальные копии в
`secrets/` остаются только как бэкап/для bootstrap и не используются
deploy-скриптами.

Публичные примеры используют `home.example.com`. Реальные адреса — в
`secrets/`.

## Состав

```text
projects/home/kubernetes/external-secrets/
  kustomization.yaml              Flux entry point (namespace, HelmRelease, store, values)
  manifests/helmrelease.yaml      ESO release (live name/namespace, digest-pinned image)
  manifests/clustersecretstore.yaml
  deploy.sh                       только application ExternalSecrets (не оператор)
  test_external_secrets.py        статические контракты путей/владения
```

ExternalSecret-манифесты лежат рядом с проектами:

| Файл | Secret | OpenBao path |
|---|---|---|
| `projects/home/kubernetes/scheduler/manifests/external-secrets.yaml` | `home-assistant/scheduler-auth` | `k8s/home-assistant/scheduler-auth` |
| то же | `home-assistant/ha-bridge-ha` | `k8s/home-assistant/ha-bridge-ha` |
| `projects/home-assistant/deploy/manifests/external-secrets.yaml` | `home-assistant/hermes-environment` | `k8s/home-assistant/hermes-environment` |
| `projects/home/kubernetes/ingress/routes/external-secret-cloudflare.yaml` | `cert-manager/cloudflare-api-token` | `k8s/cert-manager/cloudflare-api-token` |
| `projects/home/kubernetes/observability/manifests/external-secret-grafana-admin.yaml` | `monitoring/grafana-admin` | `k8s/monitoring/grafana-admin` |
| `projects/home/kubernetes/observability/manifests/external-secret-alertmanager-telegram.yaml` | `monitoring/alertmanager-telegram` | `k8s/home-assistant/hermes-environment` + `k8s/monitoring/heartbeat` (template data); применяет Flux `observability` |
| `projects/home/kubernetes/currency-bot/manifests/external-secrets.yaml` | `currency-bot/currency-bot` | `k8s/currency-bot/currency-bot` |
| то же | `currency-bot/cloudflared-tunnel` | `k8s/currency-bot/cloudflared-tunnel` |
| `projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml` | `home-assistant/mcp-gateway-*` | `k8s/home-assistant/mcp-gateway-*` |
| `projects/home/kubernetes/wakegrant/manifests/external-secrets.yaml` | `home-assistant/wakegrant` | `k8s/home-assistant/wakegrant` |
| `projects/home/kubernetes/vaultwarden/manifests/external-secrets.yaml` | `vaultwarden/vaultwarden-admin` | `k8s/vaultwarden/vaultwarden-admin` |

Версии — `projects/home/kubernetes/versions.sh`
(`EXTERNAL_SECRETS_CHART_VERSION`, `EXTERNAL_SECRETS_IMAGE_VERSION`,
`EXTERNAL_SECRETS_IMAGE_DIGEST`).

## Установка

Оператор, `ClusterSecretStore` и инфраструктурные ExternalSecrets —
под Flux (`platform-external-secrets` и компонентные Kustomization):

```bash
mise run admin:openbao
mise run admin:openbao-bootstrap      # роли/политики, включая external-secrets
mise run admin:flux-bootstrap         # или merge в develop после bootstrap
```

`deploy.sh` больше не ставит Helm-релиз: он применяет только ExternalSecrets
приложений, которые остаются на `deploy-home` (scheduler, Hermes,
currency-bot, mcp-gateway, wakegrant) и ждёт `Ready`. `--skip-secrets` — только
проверка готовности оператора/store.

На чистом кластере первая установка ESO всё ещё ручная (иначе `flux-settings`
не из чего создать); см. `projects/home/kubernetes/flux/README.md`.

Проверка:

```bash
mise run kubectl -- -n external-secrets get pods
mise run kubectl -- get clustersecretstore openbao
mise run kubectl -- get externalsecret -A
```

## Права OpenBao

`projects/home/kubernetes/openbao/configure.py` создаёт:

- policy `k8s-read`: `read` на `secret/data/k8s/*` и
  `read`/`list` на `secret/metadata/k8s*`; записи нет;
- Kubernetes auth роль `external-secrets` для SA
  `external-secrets/external-secrets`, TTL 20 мин / max 1 ч, audience по
  умолчанию (проецированный токен пода).

`ClusterSecretStore/openbao` ходит на in-cluster Service
`http://openbao.openbao.svc:8200` (TLS терминируется в Traefik только для
внешних клиентов). Store использует `path: secret`, `version: v2`.

## Как добавить секрет

1. Создать Secret в OpenBao (значение не печатать; файл/переменная только на
   время операции):

   ```bash
   # локальный JSON 600, ключи = ключи Kubernetes Secret
   install -m 600 /dev/null /tmp/secret.json
   # ...заполнить {"key":"value"}...
   mise run kubectl -- -n openbao cp /tmp/secret.json openbao-0:/tmp/secret.json
   mise run kubectl -- -n openbao exec openbao-0 -- /bin/sh -ec \
     'BAO_ADDR=http://127.0.0.1:8200 BAO_TOKEN="$BAO_TOKEN" bao kv put secret/k8s/<ns>/<name> @/tmp/secret.json'
   mise run kubectl -- -n openbao exec openbao-0 -- rm -f /tmp/secret.json
   shred -u /tmp/secret.json
   ```

   (`BAO_TOKEN` — admin-токен на время операции; см. README OpenBao.)

2. Добавить ExternalSecret рядом с проектом по образцу существующих:
   `secretStoreRef: openbao/ClusterSecretStore`, `dataFrom.extract.key:
   k8s/<ns>/<name>`, `target.name` = имя Secret,
   `creationPolicy: Owner`, `deletionPolicy: Retain`, `refreshInterval: 1h`.
   Для инфраструктурного секрета — в каталог компонента, которым владеет Flux
   (`ingress/routes`, `observability/manifests`, `vaultwarden/manifests`), и
   добавить его в `kustomization.yaml` компонента. Для сервиса на
   `deploy-home` — рядом с сервисом и в список `deploy.sh`.

3. Merge в `develop` (Flux) либо `mise run admin:external-secrets` для
   application-секретов; проверить
   `mise run kubectl -- -n <ns> get externalsecret <name>` → `SecretSynced`.

## Перехват существующего Secret

При миграции Secret уже существует. ESO с `creationPolicy: Owner` делает
`get` + `update` того же объекта: UID не меняется, поды не перезапускаются,
значения перезаписываются только при расхождении. `deletionPolicy: Retain`
страхует от удаления Secret при удалении ExternalSecret.

Порядок миграции (выполнено на живом кластере):

1. снять хеши значений исходных Secret;
2. записать те же значения в `secret/k8s/<ns>/<name>`;
3. применить ExternalSecret;
4. дождаться `SecretSynced=True`;
5. сверить SHA-256 по каждому ключу и UID Secret; поды не должны
   рестартовать.

## Ротация

ESO обновляет Secret по `refreshInterval` (1 ч). Немедленно:

```bash
mise run kubectl -- -n <ns> annotate externalsecret <name> force-sync="$(date +%s)" --overwrite
mise run kubectl -- -n <ns> get externalsecret <name> \
  -o jsonpath='{.status.conditions[?(@.type=="Ready")].reason}{"\n"}'
```

После ротации значения в OpenBao менять только там; локальные файлы в
`secrets/` — бэкап, не источник. Потребители, читающие Secret только при
старте (Grafana, Hermes), подхватят новое значение при следующем рестарте;
scheduler/HA-токены читаются при старте процесса, поэтому для их смены нужен
контролируемый rollout.

## Отказ ESO

Если ESO недоступен, существующие Secrets остаются на месте (Kubernetes не
удаляет их). `deploy.sh` и CI проверяют `Ready` перед выкаткой и падают
fail-closed, а не пересоздают значения из локальных файлов.
