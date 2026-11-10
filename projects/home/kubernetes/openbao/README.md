# OpenBao (секрет-сервер, UI, CI/поды, External Secrets)

Один сервер OpenBao `2.7.2` (chart `openbao/openbao 0.30.6`) в namespace
`openbao` на узле K3s. UI/API опубликованы как
`https://vault.<home-domain>/` через Traefik с тем же allow-list, что и
остальные домашние сервисы. Этап 1 поднимает хранилище, вход человеком и
машинные auth-методы. Этап 2 (External Secrets Operator) переносит
приложенческие Kubernetes Secrets в KV v2 `secret/k8s/*`; см.
`projects/home/kubernetes/external-secrets/README.md`. Секреты home-сервисов
живут в `secret/k8s/*` и `secret/k8s/flux-system/*`; CI их не читает — Flux
применяет workload'ы внутри кластера. JWT-метод обслуживает только деплой
прошивки станции: роль `deploy-station` читает `secret/ci/deploy-station`.
Pulsar лежит в этом же монорепозитории, поэтому роль привязана к нему и
Environment `Station`.

Публичные примеры используют `home.example.com` и `192.0.2.0/24`. Реальные
имена — `secrets/home/ingress.env`, данные bootstrap — `secrets/openbao/`.

## Решения

| Что | Выбор | Почему |
|---|---|---|
| Storage | `raft` на локальном PV 10 GiB | встроенный snapshot/restore, без внешней БД; один узел, HA не заявляется |
| Seal | `static` auto-unseal, ключ 32 байта в Secret `openbao-seal` | после перезагрузки узла сервер распечатывается сам; не нужен человек и внешний KMS |
| TLS | терминируется в Traefik, listener `8200` без TLS | один wildcard-сертификат и allow-list, как у остальных сервисов |
| Audit | декларативный `audit "file" "stdout"` → Alloy → Loki | OpenBao 2.7 запрещает включать audit через API; kubelet ротирует лог, полный диск не блокирует запросы |
| Человек | `userpass` + политика `admin`, пароль в `secrets/openbao/` | UI поддерживает логин/пароль; TOTP включается отдельно (см. ниже) |
| CI | `jwt` mount `github`, GitHub OIDC, без статических токенов | role привязан к repository/environment/ref и короткому TTL |
| Поды | `kubernetes` auth через TokenReview | SA-идентичность; используется External Secrets Operator |

Trade-off static seal: ключ лежит рядом с данными (Kubernetes Secret на том же
узле). Компромисс узла даёт доступ и к ключу, и к raft-данным. Это осознанная
цена auto-unseal без внешнего KMS; при появлении KMS/HSM стоит перейти на
`awskms`/`pkcs11`/transit. Ключ не входит в snapshot'ы, поэтому одна копия
PV без Secret бесполезна. Recovery keys и root-токен в Git не попадают.

## Файлы

```text
projects/home/kubernetes/openbao/
  deploy.sh          namespace, seal Secret, PV/PVC, helm, DNS, IngressRoute
  bootstrap.sh       operator init (один раз), configure.py, отзыв root-токена
  configure.py       KV v2, auth-методы, политики, роли, тестовые записи, TOTP
  test_configure.py  контракты ролей/политик (в т.ч. deploy-station)
  verify.sh          HTTPS/UI/логин/политики (+ --kubernetes: под-проба)
  restore_drill.py   restore snapshot на локальном стенде и сверка с live
                     (docs/REBUILD.md §6); test_restore_drill.py — его тесты
  values/openbao.yaml
  manifests/storage.yaml        PV data/backups
  manifests/ingressroute.yaml
  manifests/backup.yaml         CronJob raft snapshot
  manifests/smoke-kubernetes.yaml
```

Версии — `projects/home/kubernetes/versions.sh`: `OPENBAO_CHART_VERSION`,
`OPENBAO_IMAGE_VERSION`, `OPENBAO_IMAGE_DIGEST` (образ по digest).

## Установка и обновление

```bash
# первый раз: каталоги PV на узле
export OPENBAO_NODE_SSH='ssh -i … -o UserKnownHostsFile=… labuser@192.0.2.27'
mise run admin:openbao
mise run admin:openbao-bootstrap
mise run admin:openbao-verify -- --kubernetes
```

`deploy.sh` идемпотентен: namespace, Secret `openbao-seal` (ключ создаётся
локально при первом запуске), PV/PVC, `helm upgrade --install`, A-запись
Cloudflare (DNS only), IngressRoute. `--skip-dns` не трогает DNS.

Обновление версии: поправить `versions.sh`, выполнить `deploy.sh`, затем
удалить под (`mise run kubectl -- -n openbao delete pod openbao-0`) — StatefulSet
использует `OnDelete`, auto-unseal распечатает сервер сам. Перед обновлением
снять snapshot (см. «Backup»).

## Bootstrap и ключи

`bootstrap.sh` при первом запуске делает `bao operator init` с
`-recovery-shares=5 -recovery-threshold=3` (при auto-unseal unseal-ключей нет,
есть recovery keys). Ответ пишется в `secrets/openbao/init.json` (600):
recovery keys + root token. Затем `configure.py` применяет конфигурацию, а
начальный root-токен **отзывается** и удаляется из файла.

Recovery keys — единственный путь выпустить новый root-токен. После
bootstrap `init.json` и `seal.key` переносятся в менеджер паролей (Bitwarden
`Home recovery`) и удаляются с диска; `deploy.sh` без локального `seal.key`
сохраняет живой Secret `openbao-seal` и никогда не генерирует ключ поверх него.

### Временный root-токен на OpenBao 2.7

С версии 2.6 команда `bao operator generate-root` использует
аутентифицированный `/sys/generate-root-token`, поэтому без действующего
admin-токена она возвращает `403`. Легаси-эндпоинты `/sys/generate-root/*`
по умолчанию отключены (`disable_unauthed_generate_root_endpoints = true`).
Чтобы выпустить root из recovery keys, включите их на короткое время:

```bash
# 1. временно разрешить легаси-эндпоинты (только на время операции)
mise run helm -- get values openbao -n openbao -o yaml > /tmp/openbao-values.yaml
# добавить в listener: disable_unauthed_generate_root_endpoints = false
mise run helm -- upgrade openbao openbao/openbao --version 0.30.6 -n openbao \
  -f /tmp/openbao-values.yaml
mise run kubectl -- -n openbao delete pod openbao-0     # OnDelete применяет конфиг
mise run kubectl -- -n openbao wait --for=condition=Ready pod/openbao-0 --timeout=180s

# 2. инициализировать попытку (nonce/otp сохранить), ввести 3 recovery key
mise run kubectl -- -n openbao exec openbao-0 -- /bin/sh -ec \
  'BAO_ADDR=http://127.0.0.1:8200 bao operator generate-root -init -format=json'
# для каждого ключа: POST /v1/sys/generate-root/update {"key":..., "nonce":...}
# или CLI: bao operator generate-root -nonce=<nonce> -   (ключ на stdin)

# 3. декодировать encoded_token (XOR с otp) и проверить токен
# 4. отозвать root: curl -X POST -H "X-Vault-Token: <root>" \
#      http://127.0.0.1:8200/v1/auth/token/revoke-self
# 5. вернуть исходные values (без параметра) и удалить под
mise run helm -- upgrade openbao openbao/openbao --version 0.30.6 -n openbao \
  -f /tmp/openbao-values.original.yaml
mise run kubectl -- -n openbao delete pod openbao-0
```

Окно с открытыми легаси-эндпоинтами — секунды; endpoint доступен только
внутри кластера и через уже аутентифицированный Traefik allow-list, но
включать его «навсегда» нельзя. Токен и recovery keys не печатать; файлы —
600, удалять после работы.

`configure.py` можно запускать повторно с любым admin-токеном
(`BAO_TOKEN=… ./configure.py --addr https://vault.<domain> --resolve-ip <node>`);
он не меняет существующие пароли и записи. Второй GitHub-репозиторий
включается явно:
`OPENBAO_STATION_REPOSITORY=<owner>/<station-repo>` (или
`--station-repository …`); без него роль `deploy-station` не создаётся.

## Вход человеком

- URL: `https://vault.<home-domain>/ui/`, метод **Username**.
- Имя пользователя — `secrets/openbao/username` (600), политика `admin`, TTL 8 ч (max 24 ч).
- Пароль задаёт человек и хранит его в менеджере паролей; файла
  `secrets/openbao/human.pass` в рабочем дереве нет. Смена — UI
  `Access → Authentication methods → userpass → <user> → Edit` или
  `bao write auth/userpass/users/<user> password=…` (нужен admin-токен).
  `verify.sh` пропускает live-логин, если файла пароля нет.

Политика `admin`: полный доступ (`*` c `sudo`), но запрещены
`sys/seal`, `sys/step-down`, `sys/raw/*`, `sys/replication/*`, `sys/rotate`,
`sys/rekey/*`, `sys/rekey-recovery-key/*` — операции, которые ломают
доступность или обходят аудит, остаются за recovery keys и админом узла.

### TOTP

OpenBao поддерживает login MFA. `configure.py` создаёт метод `human-totp`
(per-entity TOTP, SHA1/6/30 с, issuer `OpenBao home`). **Состояние:**
устройство зарегистрировано, enforcement на `auth/userpass` включён; вход в UI требует пароль и 6-значный код. Повторный запуск
`configure.py` без флага enforcement не трогает и сообщает его состояние.

Регистрация/перевыпуск устройства (UI самостоятельной регистрации нет):

1. admin-токен свежей сессии; `entity_id` — из `auth/token/lookup-self`;
2. `POST /v1/identity/mfa/method/totp/admin-generate` с `method_id`
   (`secrets/openbao/totp-method-id`) и `entity_id` → `data.barcode` (PNG,
   base64): показать QR только локально, отсканировать, файл удалить;
   `admin-destroy` сбрасывает старый секрет;
3. включить enforcement: `configure.py --enforce-totp` или
   `POST /v1/identity/mfa/login-enforcement/userpass-totp` с `mfa_method_ids`
   и accessor'ом `userpass/`;
4. проверить вход в UI: после пароля спрашивается код.

Admin-токен держать до успешного входа с кодом — им же можно снять
enforcement (`DELETE …/login-enforcement/userpass-totp`). Потерян телефон —
recovery keys → `operator generate-root` → `admin-destroy` + `admin-generate`.

## Auth-методы и политики

| Mount | Кто | Роль/пользователь | Политика |
|---|---|---|---|
| `userpass` | человек | имя из `secrets/openbao/username` | `admin` |
| `github` (`jwt`) | GitHub Actions, Android APK sign + App Distribution | `android-distribute` | `ci-android` |
| `github` (`jwt`) | GitHub Actions, station deploy | `deploy-station` | `ci-station` |
| `kubernetes` | External Secrets Operator | `external-secrets` | `k8s-read` |
| `kubernetes` | smoke-под | `k8s-smoke` | `k8s-read` |
| `kubernetes` | CronJob backup | `backup` | `backup` |

Политика `ci-deploy` и роль `deploy-home` удалены (этап 6.7): home-сервисы
применяет Flux, CI больше не читает `secret/ci/*` для них. `secret/ci/test`
остаётся несекретным negative-probe для k8s-auth smoke.

`k8s-read`: read `secret/data/k8s/*`, list/read metadata; запись запрещена.

`operator-kv` (orphan periodic token владельца, `secrets/openbao/operator.token`):
create/read/update/patch `secret/data/{ci,k8s,infra}/*`, list/read metadata;
delete, `sys/*`, `auth/*` запрещены. `secret/infra/*` — секреты доступа
владельца (SSH-ключи, CA, VPN, бэкапы, admin kubeconfig); карта путей —
`docs/SECRETS.md`, токен машины и восстановление файлов — `docs/WORKSTATION.md`
(`mise run secrets:login`); деплой и проверка — `mise run admin:openbao`,
`admin:openbao-verify` (секреты через fnox).

`ci-station`: read ровно `secret/data/ci/deploy-station` и
`secret/metadata/ci/deploy-station`; ни `secret/ci/test`, ни `secret/k8s/*`,
ни запись политика не разрешает. Создаётся только вместе с
параметром `--station-repository`.

`ci-android`: read ровно `secret/data/ci/android-distribute` и
`secret/metadata/ci/android-distribute`; `secret/ci/test`,
`secret/ci/deploy-station`, `secret/k8s/*` и запись запрещены. Политика
создаётся всегда вместе с ролью `android-distribute`.

`backup`: только `read sys/storage/raft/snapshot`.

Роль `deploy-home` не создаётся: `configure.py` больше не регистрирует её, а
`deploy-home.yml` не содержит job с `environment: Home`.

Роль `deploy-station` (`auth/github/role/deploy-station`) деплоит прошивку
станции через OpenBao из этого монорепозитория:

- `oidc_discovery_url`/`bound_issuer` = `https://token.actions.githubusercontent.com`;
- `bound_audiences` = публичный URL OpenBao (`https://vault.<home-domain>`);
- `bound_claims`: `repository=<owner>/<repo>` (монорепозиторий) и
  `environment=Station`. Репозиторий передаётся как
  `--station-repository <owner>/<repo>` или через
  `OPENBAO_STATION_REPOSITORY`; в публичной документации используются только
  обезличенные имена;
- claim `ref` намеренно не задан: job `deploy-station` запускается из
  `deploy-home.yml` после merge в `develop`, а какие refs допустимы,
  ограничивает GitHub Environment `Station` (protection rules/rulesets), а не
  OIDC-роль;
- `user_claim=job_workflow_ref`, `token_policies=["ci-station"]`, TTL 10 мин,
  max 15 мин, `clock_skew=60`.

Без `--station-repository` политика `ci-station` и роль `deploy-station` не
создаются, остальная конфигурация не меняется. Повторный запуск с параметром
идемпотентно обновляет роль.

Роль `android-distribute` (`auth/github/role/…`) обслуживает подпись и загрузку
Android APK в Firebase App Distribution внутри PR CI: APK без секретов собирает
matrix-запись `android / android-app` (`_android.yml`), а подписывает и
загружает job `android-distribute` файла `ci.yml`. Привязка по claims
`repository`, `environment=Android`, `ref=refs/pull/*/merge`,
`event_name=pull_request` и
`job_workflow_ref=<owner>/<repo>/.github/workflows/ci.yml@refs/pull/*/merge`:

- токен может выпустить только этот job на merge-ref конкретного PR;
- `user_claim=job_workflow_ref`, `token_policies=["ci-android"]`, TTL 10 мин,
  max 15 мин, `clock_skew=60`.

Модель доверия: OIDC получают только PR из веток этого же репозитория — у PR
из форка прогон read-only, без Environment-секретов и без права на роль, поэтому
подделать `repository`/`job_workflow_ref` нельзя. Остаточный риск осознан: тот,
кто может пушить ветку в репозиторий, может изменить `_android.yml`/`ci.yml` и
получить секреты сборки Android. Для репозитория с одним владельцем это
приемлемо; роли дают read ровно одной записи `secret/ci/android-distribute` и
TTL 10 минут. Роль `deploy-station` остаётся с точными claims и без глобов.

Ключи KV v2: `secret/ci/test` → `{probe: ok-ci}` (negative-probe),
`secret/ci/deploy-station` → `{ha_url, ha_token, entity_id}`
(см. «Station-секреты»), `secret/ci/android-distribute` →
`{keystore_b64, keystore_password, google_services_json,
firebase_service_account_json}` (см. «Android-секреты»), `secret/k8s/test` →
`{probe: ok-k8s}`. Значения `test` не секретные и нужны только для
smoke-проверок.

## Kubernetes auth

Chart включает `authDelegator` — ServiceAccount `openbao` получает
`system:auth-delegator`, поэтому TokenReview разрешён. Проверка:

```bash
KUBECONFIG=… kubectl auth can-i create tokenreviews.authentication.k8s.io \
  --as=system:serviceaccount:openbao:openbao
mise run admin:openbao-verify -- --kubernetes
```

Проба создаёт namespace `openbao-smoke`, SA `openbao-smoke`, Job: логин через
`auth/kubernetes/login`, чтение `secret/k8s/test`, отказ на
`secret/ci/test` и на запись. Логи печатают только результат.

Роль `external-secrets` привязана к SA
`external-secrets/external-secrets` и используется External Secrets Operator;
для неё отдельных изменений политики не требуется.

## External Secrets Operator

ESO читает `secret/k8s/<namespace>/<secret-name>` и создаёт Kubernetes Secret
с тем же именем и ключами в namespace приложения. Установка и таблица
секретов — `projects/home/kubernetes/external-secrets/README.md`.

```bash
mise run admin:external-secrets
mise run kubectl -- get clustersecretstore openbao
mise run kubectl -- get externalsecret -A
```

Перенесены: `scheduler-auth`, `ha-bridge-ha`, `hermes-environment`,
`grafana-admin`, `cloudflare-api-token`. Не переносятся: `openbao-seal`
(курица и яйцо), helm release Secrets, SA token Secrets, TLS-сертификаты
cert-manager и служебные Secrets контроллеров.

Добавление/ротация значения — только в OpenBao; локальные файлы в `secrets/`
больше не источник правды. Немедленная синхронизация:

```bash
mise run kubectl -- -n <ns> annotate externalsecret <name> force-sync="$(date +%s)" --overwrite
```

## CI (GitHub OIDC)

### CI-секреты

`deploy-home` больше не входит в VPN и не читает OpenBao: home-сервисы
применяет Flux, а CI только пинует проверенные digest'ы в `deploy/home-apps`.
Поэтому Environment `Home`, `secret/ci/deploy-home`, kubeconfig и
`scheduler.env`/`runtime_values` в CI удалены. Приватные значения приложений
лежат в `secret/k8s/flux-system/app-settings` (ESO → `flux-app-settings`),
values Hermes — в `secret/k8s/flux-system/runtime-values` (поле `values`);
их читает ESO через Kubernetes auth, не GitHub OIDC.

Environment `Android` (matrix-запись `android-app` в
`.github/workflows/_android.yml` и job `android-distribute` в `ci.yml`) хранит
те же четыре значения доступа: `VPN_SERVER`, `VPN_CERT_P12_B64`,
`VPN_CERT_PASSWORD`, `OPENBAO_ADDR`; роль выбирается в workflow
(`android-distribute`), отдельного `OPENBAO_ROLE` нет. Environment объявляет
только job `android-distribute` в `ci.yml`.

Deployment branch policy этого Environment **должна разрешать PR-head-ветки**:
правила вида `*` и `*/*` (protected branches и теги — как удобно). Ограничение
ref'а делает роль OpenBao (`refs/pull/*/merge` + `job_workflow_ref`), а не
Environment: если оставить политику только на `develop`, job упадёт ещё до
получения OIDC-токена. Секреты и роль доступны и PR-прогонам, но только из
этого репозитория: форки не получают ни секретов Environment, ни OIDC-токена
(сборка и тесты у них идут без подписи, с явным notice).

### Android-секреты (App Distribution)

`secret/ci/android-distribute` читает роль `android-distribute` (ключ подписи и
Firebase service account) с политикой `ci-android`; `google-services.json` CI
берёт из переменной репозитория `GOOGLE_SERVICES_JSON_B64`. Подпись APK делает
`platform/android/sign-release.sh`
поверх собранного Bazel APK; ключ и пароли в CI не попадают в Bazel и в env
последующих шагов. Значения — исходный текст (не base64), кроме keystore,
который хранится как base64:

| Ключ | Откуда | Зачем |
|---|---|---|
| `signing_keystore_b64` | `base64 < secrets/android/home-release.keystore` | release-подпись APK (apksigner); сертификат остаётся `88ef5d61…04ad` |
| `signing_store_password` | сильный пароль хранилища | `apksigner --ks-pass file:…` |
| `signing_key_password` | пароль ключа (PKCS12 — тот же, что у хранилища) | `apksigner --key-pass file:…` |
| `signing_key_alias` | alias ключа, например `home-release` | `apksigner --ks-key-alias` |
| `signing_cert_sha256` | SHA-256 сертификата, hex | fail-closed проверка подписи перед публикацией |
| `google_services_json` | `secrets/android/google-services.json` | `google_app_id` и Firebase-ресурсы |
| `firebase_service_account_json` | JSON-ключ сервисного аккаунта GCP | аутентификация `firebase-tools appdistribution:distribute` |

Старые `keystore_b64`/`keystore_password` (debug-ключ с паролем `android`) и
`keystore_password = android` workflow больше не читает: они остаются бэкапом
ключа. Пароль `android` в OpenBao для подписи не нужен.

Сервисный аккаунт создаётся один раз: GCP console → **IAM & Admin** →
**Service accounts** → создать аккаунт → роль **Firebase App Distribution
Admin** → **Keys** → **Add key** → JSON. Ключ сохраняется локально как
`secrets/android/firebase-distribute-sa.json` (600, вне Git); в GitHub он не
попадает.

Запись и обновление (нужен admin/root-токен, после работы отозвать). Значения
дописываются к текущему объекту, чтобы не потерять `google_services_json` и
`firebase_service_account_json`:

```bash
export BAO_ADDR=https://vault.<home-domain>
export BAO_TOKEN=...            # admin-токен, не печатать
current="$(curl -sS --fail -H "X-Vault-Token: $BAO_TOKEN" \
  "$BAO_ADDR/v1/secret/data/ci/android-distribute" | jq -c '.data.data')"
jq -n --argjson current "$current" \
  --arg signing_keystore_b64 "$(base64 < secrets/android/home-release.keystore)" \
  --rawfile signing_store_password secrets/android/home-release.store.password \
  --rawfile signing_key_password secrets/android/home-release.key.password \
  --rawfile signing_key_alias secrets/android/home-release.alias \
  --rawfile signing_cert_sha256 secrets/android/home-release.cert.sha256 \
  '{data: ($current + {signing_keystore_b64: $signing_keystore_b64,
                       signing_store_password: $signing_store_password,
                       signing_key_password: $signing_key_password,
                       signing_key_alias: $signing_key_alias,
                       signing_cert_sha256: $signing_cert_sha256})}' \
| curl -sS --fail -X POST -H "X-Vault-Token: $BAO_TOKEN" \
    --data-binary @- "$BAO_ADDR/v1/secret/data/ci/android-distribute" >/dev/null
```

`jq --rawfile` сохраняет переводы строк; `sign-release.sh` их срезает, поэтому
файлы паролей могут заканчиваться новой строкой. base64 читается без переносов.
Ротация ключа — повторная запись `signing_*`; workflow не меняется. Пароли
локально не остаются: `mise run secrets:clean` удаляет восстановленные файлы.

### Station-секреты (деплой прошивки станции)

`secret/ci/deploy-station` — отдельная запись для второго репозитория; её
читает только политика `ci-station`:

| Ключ | Зачем |
|---|---|
| `ha_url` | базовый URL Home Assistant, например `https://ha.example.test` |
| `ha_token` | long-lived token Home Assistant для деплой-задачи |
| `entity_id` | entity станции в HA, на которую нацелен деплой |

Значения пишутся из файлов через `@file` или из stdin, чтобы не попасть в
argv, историю shell и логи:

```bash
export BAO_ADDR=https://vault.<home-domain>
export BAO_TOKEN=...            # admin-токен, не печатать
bao kv put -mount=secret ci/deploy-station \
  ha_url=@secrets/ci/pulsar/ha_url \
  ha_token=@secrets/ci/pulsar/ha_token \
  entity_id=@secrets/ci/pulsar/entity_id
```

Файлы — 600 в `secrets/` (вне Git), они же источник для ротации. Проверка — `bao kv get
-mount=secret ci/deploy-station` (печатает значения; не в CI-логе). Ротация —
тот же вызов, workflow не меняется: токен роли живёт ≤15 мин и не хранится.

### Поток `deploy-station`

`deploy-home.yml` (job `deploy-station`, `environment: Station`,
`permissions: id-token: write`) выполняет шаги строго по порядку:

1. материализует VPN-сертификат `ci-pulsar` и пароль в `$RUNNER_TEMP/ci`
   (600);
2. поднимает VPN-группу `ci` и ждёт `200/…` от `$OPENBAO_ADDR/v1/sys/health`;
3. `hashicorp/vault-action` (OIDC `method: jwt`, `path: github`, audience =
   `OPENBAO_ADDR`, роль `deploy-station`) читает `secret/ci/deploy-station`;
   action маскирует значения и отдаёт их только как step outputs;
4. `ha_firmware_deploy.py` ставит опубликованную версию через HA
   (`update_entity` + `update.install`) и ждёт `installed_version`;
5. cleanup отключает VPN и удаляет `$RUNNER_TEMP/ci`.

Политика `ci-station` допускает только read `secret/ci/deploy-station` (плюс
metadata); запись, `secret/ci/*` других записей и `secret/k8s/*` запрещены.
Роль живёт ≤15 мин и не хранится.

Сеть: группа `ci` пула `10.77.1.0/24` маршрутизируется только до узла K3s,
nftables `forward` разрешает только `tcp/443`, `postrouting` маскирует в адрес
VPN-CT. Traefik видит LAN-адрес и пропускает через `home-allowlist`. Правило
`6443` и сертификат `ci-github` удалены: CI-деплой home-сервисов больше не
существует.

## Backup / restore

CronJob `openbao-raft-snapshot` (03:17 UTC) логинится через Kubernetes auth
ролью `backup` и пишет snapshot в `/srv/k3s/volumes/openbao-backups`
(PVC `openbao-backups`), хранит 7 последних файлов.

```bash
# ручной snapshot
mise run kubectl -- -n openbao create job --from=cronjob/openbao-raft-snapshot snapshot-manual
mise run kubectl -- -n openbao logs job/snapshot-manual

# восстановление: скопировать файл с backup-PVC в под и применить
# (PVC подключён только к CronJob, поэтому читаем его временным подом)
mise run kubectl -- -n openbao run snapshot-reader --rm -it --restart=Never \
  --image=quay.io/openbao/openbao --overrides='{"spec":{"containers":[{"name":"c","image":"quay.io/openbao/openbao","command":["sleep","600"],"volumeMounts":[{"name":"b","mountPath":"/b"}]}],"volumes":[{"name":"b","persistentVolumeClaim":{"claimName":"openbao-backups"}}]}}' -- sh
# во втором терминале:
mise run kubectl -- -n openbao cp snapshot-reader:/b/<file>.snap ./restore.snap
mise run kubectl -- -n openbao cp ./restore.snap openbao-0:/tmp/restore.snap
mise run kubectl -- -n openbao exec openbao-0 -- \
  bao operator raft snapshot restore -force /tmp/restore.snap
```

`-force` перезаписывает raft целиком; перед восстановлением остановить
потребителей секретов. Проверка restore на отдельном стенде — открытый пункт
(`docs/PENDING.md`).

## Ротация

- **Seal key**: сгенерировать новый 32-байтный ключ, положить в
  `secrets/openbao/seal.key.new`, обновить `current_key_id` (например
  `static-2`), указать старый как `previous_key`, применить, дождаться
  unseal, затем убрать `previous_key` и перезапустить.
- **Пароль человека**: задаёт человек, меняет через UI или API; файла
  `human.pass` нет.
- **CI**: смена role/audience — правка `configure.py`, повторный запуск; токены
  живут ≤15 мин и не хранятся. Деплой home-сервисов идёт через Flux, поэтому
  CI-значений для них нет; станция — `secret/ci/deploy-station` и роль
  `deploy-station`.
- **Root**: только через recovery keys (`generate-root`), после работ токен
  отозвать.
- **Приложенческие секреты**: менять значение в OpenBao
  (`secret/k8s/<ns>/<name>`), затем `force-sync` соответствующего
  ExternalSecret; Kubernetes Secret обновится без пересоздания.
- **Образ/чарт**: `versions.sh` + `deploy.sh` + удаление пода.

## Проверки

```bash
mise run admin:openbao-verify -- --kubernetes
python3 -m unittest discover -s projects/home/kubernetes/openbao -p 'test_*.py'
mise run kubectl -- -n openbao get pod,svc,pvc
mise run kubectl -- -n traefik get ingressroute openbao
mise run kubectl -- -n openbao logs openbao-0 --tail=50
mise run kubectl -- get clustersecretstore openbao
mise run kubectl -- get externalsecret -A
```

Ожидаемо: `initialized=true`, `sealed=false`, `recovery_seal=true`,
`storage_type=raft`; после `mise run kubectl -- delete pod openbao-0` под снова
`Ready` без unseal человеком.

## Диагностика

- `503` от Traefik: под не Ready (sealed/не инициализирован) или allow-list.
- `error parsing 'audit'`: у `audit` два label — тип и путь
  (`audit "file" "stdout" { options { file_path = "stdout" } }`).
- `invalid audience (aud) claim`: у k8s-роли остался `audience`; в
  `configure.py` он пустой, потому что подовый токен проецируется с
  audience API-сервера.
- `failed to setup audit table` / `unable to open … for writing` в цикле
  `post-unseal setup` → `pre-seal teardown`: в raft осталось устройство,
  чей путь больше не смонтирован (так было при переходе `file/` → `stdout/`).
  Временно смонтировать `emptyDir` в старый путь (`server.volumes` и
  `volumeMounts`), перезапустить под: устройство откроется, декларативный
  конфиг его отключит. Затем убрать `emptyDir` и перезапустить ещё раз.
- `cannot enable audit device via API`: используйте декларативный `audit`
  в конфиге (2.7+).
