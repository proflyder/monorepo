# External Secrets Operator

ESO в namespace `external-secrets` читает KV v2 `secret/` в OpenBao через
Kubernetes auth и создаёт Kubernetes Secrets в namespace приложений.
Источник правды — OpenBao; карта путей и процедура записи — `docs/SECRETS.md`.
Версии chart/образа (по digest) — `EXTERNAL_SECRETS_*` в
`projects/home/kubernetes/versions.sh`.

| Файл | Что |
|---|---|
| `kustomization.yaml` | Flux entry point (`platform-external-secrets`): namespace, HelmRelease, store, values |
| `manifests/helmrelease.yaml` | ESO release (живое имя/namespace) |
| `manifests/clustersecretstore.yaml` | `ClusterSecretStore/openbao` → `http://openbao.openbao.svc:8200`, `path: secret`, `version: v2`, роль `external-secrets` |
| `deploy.sh` | аварийное ручное применение ExternalSecrets приложений (`mise run admin:external-secrets`; `--skip-secrets` — только проверка оператора/store) |
| `test_external_secrets.py` | контракты путей и владения |

ExternalSecret-манифесты лежат рядом со своими проектами и применяются их
Flux Kustomization. На чистом кластере первую установку ESO делает человек
(иначе не из чего создать `flux-settings`) — `../flux/README.md`.

Права в OpenBao (`openbao/configure.py`): политика `k8s-read` — только
чтение `secret/data/k8s/*` и metadata; роль `external-secrets` для SA
`external-secrets/external-secrets`, TTL 20 мин / max 1 ч, audience по
умолчанию.

## Как добавить секрет

1. Записать значение в `secret/k8s/<ns>/<name>` (`docs/SECRETS.md`: GET →
   merge → POST с `options.cas`, значения не печатать), добавить путь в
   `platform/workstation/catalog.json` и `docs/SECRETS.md`.
2. ExternalSecret рядом с проектом по образцу существующих:
   `secretStoreRef: {name: openbao, kind: ClusterSecretStore}`,
   `key: k8s/<ns>/<name>`, `creationPolicy: Owner`, `deletionPolicy: Retain`,
   `refreshInterval: 1h`; включить в `kustomization.yaml` компонента. Если
   секрет приложения — добавить манифест и в список `deploy.sh`.
3. Merge в `develop`; проверить `get externalsecret <name>` → `SecretSynced`.

Перехват уже существующего Secret безопасен: с `creationPolicy: Owner` ESO
делает `update` того же объекта (UID не меняется, поды не рестартуют).

## Ротация и отказ

```bash
mise run kubectl -- -n <ns> annotate externalsecret <name> force-sync="$(date +%s)" --overwrite
```

Потребители, читающие секрет только при старте (Grafana, Hermes, scheduler,
HA-исполнители), подхватят значение после контролируемого rollout. Если ESO
недоступен, существующие Secrets остаются; `deploy.sh` падает fail-closed и
не пересоздаёт значения из локальных файлов.
