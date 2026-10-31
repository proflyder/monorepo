# OpenBao (секрет-сервер, UI, CI/поды, External Secrets)

Один сервер OpenBao `2.7.2` (chart `openbao/openbao 0.30.6`) в namespace
`openbao` на узле K3s. UI/API опубликованы как
`https://vault.<home-domain>/` через Traefik с тем же allow-list, что и
остальные домашние сервисы. Этап 1 поднимает хранилище, вход человеком и
машинные auth-методы. Этап 2 (External Secrets Operator) переносит
приложенческие Kubernetes Secrets в KV v2 `secret/k8s/*`; см.
`projects/home/kubernetes/external-secrets/README.md`. Этап 3 переносит
секреты CI-деплоя из GitHub Environment в `secret/ci/deploy-home`: GitHub
хранит только то, что нужно до входа в OpenBao (VPN и адрес сервера).

Публичные примеры используют `home.example.com` и `192.0.2.0/24`. Реальные
имена — `secrets/home/ingress.env`, данные bootstrap — `secrets/openbao/`.

## Решения

| Что | Выбор | Почему |
|---|---|---|
| Storage | `raft` на локальном PV 10 GiB | встроенный snapshot/restore, без внешней БД; один узел, HA не заявляется |
| Seal | `static` auto-unseal, ключ 32 байта в Secret `openbao-seal` | после перезагрузки узла сервер распечатывается сам; не нужен человек и внешний KMS |
| TLS | терминируется в Traefik, listener `8200` без TLS | один wildcard-сертификат и allow-list, как у остальных сервисов |
| Audit | декларативный `audit "file"` на PV 5 GiB | OpenBao 2.7 запрещает включать audit через API; конфиг переживает рестарт |
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
  ci_secrets.py      материализация secret/ci/deploy-home в файлы 600 (CI)
  test_ci_secrets.py контракты материализации и маскирования
  verify.sh          HTTPS/UI/логин/политики (+ --kubernetes: под-проба)
  values/openbao.yaml
  manifests/storage.yaml        PV data/audit/backups
  manifests/ingressroute.yaml
  manifests/backup.yaml         CronJob raft snapshot
  manifests/smoke-kubernetes.yaml
.github/workflows/openbao-smoke.yml   GitHub OIDC smoke (workflow_dispatch)
```

Версии — `projects/home/kubernetes/versions.sh`: `OPENBAO_CHART_VERSION`,
`OPENBAO_IMAGE_VERSION`, `OPENBAO_IMAGE_DIGEST` (образ по digest).

## Установка и обновление

```bash
export KUBECONFIG=/private/path/kubeconfig
# первый раз: каталоги PV на узле
export OPENBAO_NODE_SSH='ssh -i … -o UserKnownHostsFile=… labuser@192.0.2.27'
./projects/home/kubernetes/openbao/deploy.sh
./projects/home/kubernetes/openbao/bootstrap.sh
./projects/home/kubernetes/openbao/verify.sh --kubernetes
```

`deploy.sh` идемпотентен: namespace, Secret `openbao-seal` (ключ создаётся
локально при первом запуске), PV/PVC, `helm upgrade --install`, A-запись
Cloudflare (DNS only), IngressRoute. `--skip-dns` не трогает DNS.

Обновление версии: поправить `versions.sh`, выполнить `deploy.sh`, затем
удалить под (`kubectl -n openbao delete pod openbao-0`) — StatefulSet
использует `OnDelete`, auto-unseal распечатает сервер сам. Перед обновлением
снять snapshot (см. «Backup»).

## Bootstrap и ключи

`bootstrap.sh` при первом запуске делает `bao operator init` с
`-recovery-shares=5 -recovery-threshold=3` (при auto-unseal unseal-ключей нет,
есть recovery keys). Ответ пишется в `secrets/openbao/init.json` (600):
recovery keys + root token. Затем `configure.py` применяет конфигурацию, а
начальный root-токен **отзывается** и удаляется из файла.

Recovery keys — единственный путь выпустить новый root-токен. Их нужно
сохранить вне узла (менеджер паролей/офлайн-носитель).

### Временный root-токен на OpenBao 2.7

С версии 2.6 команда `bao operator generate-root` использует
аутентифицированный `/sys/generate-root-token`, поэтому без действующего
admin-токена она возвращает `403`. Легаси-эндпоинты `/sys/generate-root/*`
по умолчанию отключены (`disable_unauthed_generate_root_endpoints = true`).
Чтобы выпустить root из recovery keys, включите их на короткое время:

```bash
export KUBECONFIG=/private/path/kubeconfig
# 1. временно разрешить легаси-эндпоинты (только на время операции)
helm get values openbao -n openbao -o yaml > /tmp/openbao-values.yaml
# добавить в listener: disable_unauthed_generate_root_endpoints = false
helm upgrade openbao openbao/openbao --version 0.30.6 -n openbao \
  -f /tmp/openbao-values.yaml
kubectl -n openbao delete pod openbao-0     # OnDelete применяет конфиг
kubectl -n openbao wait --for=condition=Ready pod/openbao-0 --timeout=180s

# 2. инициализировать попытку (nonce/otp сохранить), ввести 3 recovery key
kubectl -n openbao exec openbao-0 -- /bin/sh -ec \
  'BAO_ADDR=http://127.0.0.1:8200 bao operator generate-root -init -format=json'
# для каждого ключа: POST /v1/sys/generate-root/update {"key":..., "nonce":...}
# или CLI: bao operator generate-root -nonce=<nonce> -   (ключ на stdin)

# 3. декодировать encoded_token (XOR с otp) и проверить токен
# 4. отозвать root: curl -X POST -H "X-Vault-Token: <root>" \
#      http://127.0.0.1:8200/v1/auth/token/revoke-self
# 5. вернуть исходные values (без параметра) и удалить под
helm upgrade openbao openbao/openbao --version 0.30.6 -n openbao \
  -f /tmp/openbao-values.original.yaml
kubectl -n openbao delete pod openbao-0
```

Окно с открытыми легаси-эндпоинтами — секунды; endpoint доступен только
внутри кластера и через уже аутентифицированный Traefik allow-list, но
включать его «навсегда» нельзя. Токен и recovery keys не печатать; файлы —
600, удалять после работы.

`configure.py` можно запускать повторно с любым admin-токеном
(`BAO_TOKEN=… ./configure.py --addr https://vault.<domain> --resolve-ip <node>`);
он не меняет существующие пароли и записи.

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
(per-entity TOTP, `generate=false`) и **не** включает enforcement, чтобы не
заблокировать вход до регистрации устройства:

1. войти в UI, `Access → Multi-factor authentication` (или профиль) и
   зарегистрировать TOTP для сущности человека;
2. включить enforcement:
   `./bootstrap.sh --enforce-totp` (нужен admin-токен) или
   `curl -X POST …/v1/identity/mfa/login-enforcement/userpass-totp` с
   `mfa_method_ids` и accessor'ом `userpass/`;
3. проверить вход: логин попросит 6-значный код.

## Auth-методы и политики

| Mount | Кто | Роль/пользователь | Политика |
|---|---|---|---|
| `userpass` | человек | имя из `secrets/openbao/username` | `admin` |
| `github` (`jwt`) | GitHub Actions | `deploy-home` | `ci-deploy` |
| `kubernetes` | External Secrets Operator | `external-secrets` | `k8s-read` |
| `kubernetes` | smoke-под | `k8s-smoke` | `k8s-read` |
| `kubernetes` | CronJob backup | `backup` | `backup` |

`ci-deploy`: read ровно `secret/data/ci/test` и
`secret/data/ci/deploy-home`; list `secret/metadata/ci` и read metadata этих
двух записей; запись, другие пути и другой namespace запрещены. Политика
намеренно перечисляет пути явно: новая запись `secret/ci/<name>` требует
правки `configure.py` и повторного применения.

`k8s-read`: read `secret/data/k8s/*`, list/read metadata; запись запрещена.

`backup`: только `read sys/storage/raft/snapshot`.

Роль `deploy-home` (`auth/github/role/deploy-home`):

- `oidc_discovery_url`/`bound_issuer` = `https://token.actions.githubusercontent.com`;
- `bound_audiences` = `https://vault.<home-domain>` (workflow запрашивает
  ровно этот audience);
- в workflow `environment:` пишется с той же буквы, что в `bound_claims`
  (`Home`): claim сохраняет регистр из YAML;
- `configure.py` через локальный port-forward запускать с
  `OPENBAO_AUDIENCE=https://vault.<home-domain>`, иначе роль привяжется к
  loopback-адресу (скрипт теперь отказывает в таком audience);
- `bound_claims`: `repository=<owner>/<repo>` (берётся из `git remote origin`, можно
  переопределить `OPENBAO_GITHUB_REPOSITORY`), `environment=Home`
  (каноническое имя environment в GitHub — `Home`, хотя в YAML пишется
  `home`), `ref=refs/heads/develop`;
- `user_claim=job_workflow_ref`, TTL 10 мин, max 15 мин.

Проверить клеймы у себя: `gh api repos/<owner>/<repo>/environments` возвращает
каноническое имя; в OIDC-токене для environment-джобы
`sub=repo:<owner>/<repo>:environment:Home`.

Ключи KV v2: `secret/ci/test` → `{probe: ok-ci}`,
`secret/ci/deploy-home` → `{kubeconfig, scheduler_env, runtime_values}` (см.
«CI-секреты»), `secret/k8s/test` → `{probe: ok-k8s}`. Значения `test`
не секретные и нужны только для smoke-проверок.

## Kubernetes auth

Chart включает `authDelegator` — ServiceAccount `openbao` получает
`system:auth-delegator`, поэтому TokenReview разрешён. Проверка:

```bash
KUBECONFIG=… kubectl auth can-i create tokenreviews.authentication.k8s.io \
  --as=system:serviceaccount:openbao:openbao
./projects/home/kubernetes/openbao/verify.sh --kubernetes
```

Проба создаёт namespace `openbao-smoke`, SA `openbao-smoke`, Job: логин через
`auth/kubernetes/login`, чтение `secret/k8s/test`, отказ на
`secret/ci/test` и на запись. Логи печатают только результат.

Роль `external-secrets` привязана к SA
`external-secrets/external-secrets` и используется External Secrets Operator;
для неё отдельных изменений политики не требуется.

## External Secrets Operator (этап 2)

ESO читает `secret/k8s/<namespace>/<secret-name>` и создаёт Kubernetes Secret
с тем же именем и ключами в namespace приложения. Установка и таблица
секретов — `projects/home/kubernetes/external-secrets/README.md`.

```bash
export KUBECONFIG=/private/path/kubeconfig
./projects/home/kubernetes/external-secrets/deploy.sh
kubectl get clustersecretstore openbao
kubectl get externalsecret -A
```

Перенесены: `scheduler-auth`, `ha-bridge-ha`, `hermes-environment`,
`grafana-admin`, `cloudflare-api-token`. Не переносятся: `openbao-seal`
(курица и яйцо), helm release Secrets, SA token Secrets, TLS-сертификаты
cert-manager и служебные Secrets контроллеров.

Добавление/ротация значения — только в OpenBao; локальные файлы в `secrets/`
больше не источник правды. Немедленная синхронизация:

```bash
kubectl -n <ns> annotate externalsecret <name> force-sync="$(date +%s)" --overwrite
```

## CI (GitHub OIDC)

### CI-секреты (этап 3)

В GitHub Environment `Home` остаются только значения, нужные до входа в
OpenBao:

| Secret | Зачем |
|---|---|
| `VPN_SERVER`, `VPN_CERT_P12_B64`, `VPN_CERT_PASSWORD` | подключение в VPN-группу `ci` |
| `OPENBAO_ADDR` | базовый URL без завершающего слэша; он же audience OIDC |
| `OPENBAO_ROLE` | опционально, по умолчанию `deploy-home` |

Остальные значения лежат в OpenBao в одной записи `secret/ci/deploy-home`
(KV v2, исходный текст, не base64; многострочные значения — с переводами
строк):

| Ключ | Файл в CI (`$RUNNER_TEMP/ci`, 600) | Кто использует |
|---|---|---|
| `kubeconfig` | `kubeconfig` | все deploy-юниты (kubectl/helm/smoke) |
| `scheduler_env` | `scheduler.env` | bazel-юниты (`scheduler`, `ha-bridge`) |
| `runtime_values` | `runtime-values.yaml` | docker-юнит `hermes` (rollout/откат) |

`K3S_NODE` в GitHub больше не нужен: VPN-проверка идёт через
`$OPENBAO_ADDR/v1/sys/health`, а адрес API уже внутри kubeconfig. GitHub
хранит только то, без чего не дойти до OpenBao; `kubeconfig`,
`scheduler_env` и `runtime_values` удаляются из Environment после первого
успешного прогона.

Запись и обновление (нужен admin/root-токен, после работы отозвать):

```bash
export BAO_ADDR=https://vault.<home-domain>
export BAO_TOKEN=...            # admin-токен, не печатать
jq -n \
  --rawfile kubeconfig secrets/ci/github/kubeconfig.yaml \
  --rawfile scheduler_env secrets/home/scheduler.env \
  --rawfile runtime_values "$RUNTIME_PRIVATE_VALUES" \
  '{data: {kubeconfig: $kubeconfig, scheduler_env: $scheduler_env,
           runtime_values: $runtime_values}}' \
| curl -sS --fail -X POST -H "X-Vault-Token: $BAO_TOKEN" \
    --data-binary @- "$BAO_ADDR/v1/secret/data/ci/deploy-home" >/dev/null
```

`jq --rawfile` сохраняет переводы строк байт-в-байт; после записи сверить
SHA-256 прочитанного значения с локальным файлом. Ротация значения не меняет
workflow: обновить ключ, при необходимости перезапустить `deploy-home`
(токен роли живёт ≤15 мин и не хранится).

### Поток deploy-home

`deploy-home.yml` (`environment: home`, `permissions: id-token: write` только
у deploy-джобы; у `plan` секретов нет) выполняет шаги строго по порядку:

1. материализует VPN-сертификат и пароль в `$RUNNER_TEMP/ci` (600);
2. поднимает VPN-группу `ci` и ждёт `200/…` от `$OPENBAO_ADDR/v1/sys/health`;
3. запрашивает GitHub OIDC-токен с audience `OPENBAO_ADDR`, логинится в
   `auth/github` ролью `deploy-home`, читает `secret/ci/deploy-home`;
4. `ci_secrets.py` пишет только нужные юниту файлы (600) и регистрирует
   `::add-mask::` для каждой значимой строки значения; сами значения в лог не
   печатаются;
5. отдельный шаг `if: always()` отзывает токен OpenBao (`revoke-self`);
6. дальше — прежние kubectl/helm шаги, smoke и откат; cleanup удаляет
   `$RUNNER_TEMP/ci`.

Политика `ci-deploy` допускает только read `secret/ci/test` и
`secret/ci/deploy-home` (плюс metadata); запись и `secret/k8s/*` запрещены —
это проверяет `openbao-smoke.yml`, который также убеждается, что в
`secret/ci/deploy-home` есть все три ключа (печатаются только имена ключей).

Сеть: группа `ci` пула `10.77.1.0/24` маршрутизируется только до узла K3s,
nftables `forward` разрешает `tcp/{443,6443}`, `postrouting` маскирует в адрес
VPN-CT. Traefik видит LAN-адрес и пропускает через `home-allowlist`. Через 443
группа `ci` технически достаёт и другие `*.home`-имена, но они за собственной
аутентификацией; это принятый trade-off вместо отдельного порта.

## Backup / restore

CronJob `openbao-raft-snapshot` (03:17 UTC) логинится через Kubernetes auth
ролью `backup` и пишет snapshot в `/srv/k3s/volumes/openbao-backups`
(PVC `openbao-backups`), хранит 7 последних файлов.

```bash
# ручной snapshot
kubectl -n openbao create job --from=cronjob/openbao-raft-snapshot snapshot-manual
kubectl -n openbao logs job/snapshot-manual

# восстановление: скопировать файл с backup-PVC в под и применить
# (PVC подключён только к CronJob, поэтому читаем его временным подом)
kubectl -n openbao run snapshot-reader --rm -it --restart=Never \
  --image=quay.io/openbao/openbao --overrides='{"spec":{"containers":[{"name":"c","image":"quay.io/openbao/openbao","command":["sleep","600"],"volumeMounts":[{"name":"b","mountPath":"/b"}]}],"volumes":[{"name":"b","persistentVolumeClaim":{"claimName":"openbao-backups"}}]}}' -- sh
# во втором терминале:
kubectl -n openbao cp snapshot-reader:/b/<file>.snap ./restore.snap
kubectl -n openbao cp ./restore.snap openbao-0:/tmp/restore.snap
kubectl -n openbao exec openbao-0 -- \
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
  живут ≤15 мин и не хранятся. Смена значений деплоя — обновление
  `secret/ci/deploy-home` (см. «CI-секреты»), workflow не меняется.
- **Root**: только через recovery keys (`generate-root`), после работ токен
  отозвать.
- **Приложенческие секреты**: менять значение в OpenBao
  (`secret/k8s/<ns>/<name>`), затем `force-sync` соответствующего
  ExternalSecret; Kubernetes Secret обновится без пересоздания.
- **Образ/чарт**: `versions.sh` + `deploy.sh` + удаление пода.

## Проверки

```bash
export KUBECONFIG=/private/path/kubeconfig
./projects/home/kubernetes/openbao/verify.sh --kubernetes
python3 -m unittest discover -s projects/home/kubernetes/openbao -p 'test_*.py'
kubectl -n openbao get pod,svc,pvc
kubectl -n traefik get ingressroute openbao
kubectl -n openbao logs openbao-0 --tail=50
kubectl get clustersecretstore openbao
kubectl get externalsecret -A
```

Ожидаемо: `initialized=true`, `sealed=false`, `recovery_seal=true`,
`storage_type=raft`; после `kubectl delete pod openbao-0` под снова
`Ready` без unseal человеком.

## Диагностика

- `503` от Traefik: под не Ready (sealed/не инициализирован) или allow-list.
- `error parsing 'audit'`: у `audit` два label — тип и путь
  (`audit "file" "file" { options { file_path = … } }`).
- `invalid audience (aud) claim`: у k8s-роли остался `audience`; в
  `configure.py` он пустой, потому что подовый токен проецируется с
  audience API-сервера.
- `cannot enable audit device via API`: используйте декларативный `audit`
  в конфиге (2.7+).
