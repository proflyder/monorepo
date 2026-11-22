# OpenBao (секрет-сервер, UI, CI/поды, External Secrets)

Один сервер OpenBao (chart `openbao/openbao`, версии и digest —
`OPENBAO_*` в `projects/home/kubernetes/versions.sh`) в namespace `openbao` на
узле K3s. UI/API — `https://vault.<home-domain>/` через Traefik с общим
allow-list. Разворачивается вручную, не Flux: OpenBao — источник всех Secret и
должен восстанавливаться без GitOps. Карта путей KV и процедура записи —
`docs/SECRETS.md`; секреты приложений доставляет ESO
(`projects/home/kubernetes/external-secrets/README.md`).

Публичные примеры используют `home.example.com` и `192.0.2.0/24`; реальные
имена — `secrets/home/ingress.env`.

## Решения

| Что | Выбор | Почему |
|---|---|---|
| Storage | `raft` на локальном PV 10 GiB | snapshot/restore без внешней БД; один узел, HA не заявляется |
| Seal | `static` auto-unseal, 32-байтный ключ в Secret `openbao-seal` | сервер распечатывается сам после перезагрузки, без человека и KMS |
| TLS | терминируется в Traefik, listener `8200` без TLS | общий wildcard-сертификат и allow-list |
| Audit | декларативный `audit "file" "stdout"` → Alloy → Loki | 2.7 запрещает включать audit через API; полный диск не блокирует запросы |
| Человек | `userpass` + политика `admin` + TOTP | вход в UI логином/паролем |
| CI | `jwt` mount `github` (GitHub OIDC), без статических токенов | роли привязаны к repository/environment/ref, TTL 10 мин |
| Поды | `kubernetes` auth через TokenReview | идентичность SA, используется ESO |

Цена static seal: ключ лежит на том же узле, что и raft-данные; компромисс узла
даёт оба. При появлении KMS/HSM — перейти на `awskms`/`pkcs11`/transit. Ключ не
входит в snapshot: копия PV без Secret бесполезна. Единственная копия ключа и
recovery keys вне кластера — у владельца в Bitwarden (`Home recovery`).

## Установка и обновление

```bash
export OPENBAO_NODE_SSH='ssh -i … -o UserKnownHostsFile=… labuser@192.0.2.27'
mise run admin:openbao
mise run admin:openbao-bootstrap
mise run admin:openbao-verify -- --kubernetes
```

`deploy.sh` идемпотентен: namespace, Secret `openbao-seal`, PV/PVC,
`helm upgrade --install`, A-запись Cloudflare (`--skip-dns` не трогает DNS),
IngressRoute. Без локального `seal.key` он сохраняет живой `openbao-seal` и
никогда не генерирует ключ поверх.

Обновление: снять snapshot, поправить `versions.sh`, `mise run admin:openbao`,
затем `mise run kubectl -- -n openbao delete pod openbao-0` — StatefulSet
`OnDelete`, auto-unseal распечатает сам.

`bootstrap.sh` (один раз): `bao operator init -recovery-shares=5
-recovery-threshold=3`, `configure.py`, отзыв начального root-токена.
`configure.py` можно перезапускать с любым admin-токеном
(`BAO_TOKEN=… ./configure.py --addr https://vault.<domain> --resolve-ip <node>`);
существующие пароли и записи он не меняет. Роль `deploy-station` и политика
`ci-station` создаются только с `--station-repository <owner>/<repo>` (или
`OPENBAO_STATION_REPOSITORY`).

## Временный root-токен (2.6+)

`bao operator generate-root` на 2.6+ идёт в аутентифицированный
`/sys/generate-root-token` и без admin-токена даёт `403`; легаси
`/sys/generate-root/*` выключены (`disable_unauthed_generate_root_endpoints =
true`). Выпуск root из recovery keys:

1. `mise run helm -- get values openbao -n openbao -o yaml` → сохранить
   оригинал, в копию добавить в listener
   `disable_unauthed_generate_root_endpoints = false`, `helm upgrade` с той же
   версией chart, удалить под `openbao-0`, дождаться Ready.
2. `bao operator generate-root -init -format=json` внутри пода (сохранить
   nonce/otp), ввести 3 recovery key (`-nonce=<nonce> -`, ключ на stdin).
3. Декодировать `encoded_token` с otp, выполнить работу, отозвать токен
   (`auth/token/revoke-self`).
4. Вернуть исходные values, удалить под.

Окно с открытыми легаси-эндпоинтами держать минимальным. Токен и ключи не
печатать, временные файлы `600` и удалять.

## Вход человеком и TOTP

- `https://vault.<home-domain>/ui/`, метод **Username**; имя —
  `secrets/openbao/username`, политика `admin`, TTL 8 ч (max 24 ч). Пароль
  только у человека (файла нет; `verify.sh` тогда пропускает live-логин).
- `admin`: `*` с `sudo`, кроме `sys/seal`, `sys/step-down`, `sys/raw/*`,
  `sys/replication/*`, `sys/rotate`, `sys/rekey/*`,
  `sys/rekey-recovery-key/*`.
- TOTP: метод `human-totp` (SHA1/6/30 с, issuer `OpenBao home`), enforcement на
  `auth/userpass` включён. `configure.py` без `--enforce-totp` enforcement не
  трогает, только сообщает состояние.

Перевыпуск устройства: admin-токен → `entity_id` из
`auth/token/lookup-self` → `identity/mfa/method/totp/admin-destroy`, затем
`admin-generate` с `method_id` (`secrets/openbao/totp-method-id`) → QR из
`data.barcode` показать только локально → `configure.py --enforce-totp` →
проверить вход с кодом. Admin-токен держать до успешного входа (им снимается
enforcement: `DELETE identity/mfa/login-enforcement/userpass-totp`). Потерян
телефон — recovery keys → root → тот же перевыпуск.

## Auth-методы и политики

| Mount | Роль | Политика | Доступ |
|---|---|---|---|
| `userpass` | человек | `admin` | см. выше |
| `github` | `android-distribute` | `ci-android` | PR CI: `environment=Android`, `ref=refs/pull/*/merge`, `event_name=pull_request`, `job_workflow_ref=…/_android-release.yml@refs/pull/*/merge` |
| `github` | `android-release` | `ci-android` | ручной релиз: `ref=refs/heads/develop`, `event_name=workflow_dispatch`, `_android-release.yml@refs/heads/develop` |
| `github` | `android-cleanup` | `ci-android` | чистка App Distribution: `ref=refs/heads/develop`, `event_name` ∈ {`schedule`, `workflow_dispatch`}, `_cleanup.yml@refs/heads/develop` |
| `github` | `deploy-station` | `ci-station` | `repository`, `environment=Station`, без claim `ref` |
| `kubernetes` | `external-secrets` | `k8s-read` | SA `external-secrets/external-secrets` |
| `kubernetes` | `k8s-smoke` | `k8s-read` | smoke-Job |
| `kubernetes` | `backup` | `backup` | CronJob snapshot |

- GitHub-роли: `user_claim=job_workflow_ref`, TTL 10 мин / max 15 мин,
  `clock_skew=60`, audience — публичный URL OpenBao. OIDC-claim называет
  вызванный reusable-файл: переименование `_android-release.yml` или
  `_cleanup.yml` требует повторного `configure.py`.
- `deploy-station` без `ref`: допустимые refs ограничивает GitHub Environment
  `Station`, а не роль.
- `ci-android` — read ровно `secret/ci/android-distribute`; `ci-station` —
  read ровно `secret/ci/deploy-station`; `k8s-read` — read `secret/k8s/*`;
  `backup` — только `sys/storage/raft/snapshot`; `operator-kv` (токен
  владельца) — create/read/update `secret/{ci,k8s,infra}/*` без delete,
  `sys/*`, `auth/*`.
- Модель доверия: PR из форков не получают ни Environment-секретов, ни OIDC.
  Тот, кто может пушить ветку в репозиторий, может получить секреты Android —
  для репозитория с одним владельцем это принято.
- Deployment branch policy Environment `Android` должна разрешать PR-head-ветки
  и `develop` (`*`, `*/*`): иначе job падает до получения OIDC-токена;
  ref ограничивает роль.
- `secret/ci/test` и `secret/k8s/test` — несекретные пробы для smoke
  (`secret/ci/*` не должен читаться политикой workload'а).

Поток `deploy-station` (reusable `.github/workflows/_station-deploy.yml`,
`environment: Station`): VPN-сертификат `ci-pulsar` → VPN-группа `ci` (до узла
K3s только `tcp/443`) → ожидание `$OPENBAO_ADDR/v1/sys/health` →
`hashicorp/vault-action` (OIDC, роль `deploy-station`) читает
`ha_url`/`ha_token`/`entity_id` → `ha_firmware_deploy.py` → cleanup VPN.

## Kubernetes auth

Chart включает `authDelegator` (`system:auth-delegator` для SA `openbao`).
`mise run admin:openbao-verify -- --kubernetes` создаёт namespace
`openbao-smoke` и Job: логин, чтение `secret/k8s/test`, отказ на
`secret/ci/test` и на запись.

## Backup / restore

CronJob `openbao-raft-snapshot` (03:17 UTC, роль `backup`) пишет snapshot в
PVC `openbao-backups` (`/srv/k3s/volumes/openbao-backups`), хранит 7 файлов.
Ручной: `mise run kubectl -- -n openbao create job
--from=cronjob/openbao-raft-snapshot snapshot-manual`.

Восстановление: PVC подключён только к CronJob, поэтому файл читается временным
подом с этим PVC, копируется в `openbao-0` и применяется
`bao operator raft snapshot restore -force` (перезаписывает raft целиком;
сначала остановить потребителей). Учения на локальном стенде —
`restore_drill.py`, порядок — `docs/REBUILD.md` (§6).

## Ротация

- **Seal key**: новый ключ с новым `current_key_id` (например `static-2`),
  старый — `previous_key`; применить, дождаться unseal, убрать
  `previous_key`, перезапустить.
- **Root**: только через recovery keys, после работы отозвать.
- **CI-роли**: правка `configure.py` и повторный запуск.
- **Секреты приложений**: значение в OpenBao, затем
  `mise run kubectl -- -n <ns> annotate externalsecret <name> force-sync="$(date +%s)" --overwrite`.

## Проверки

```bash
mise run admin:openbao-verify -- --kubernetes
python3 -m unittest discover -s projects/home/kubernetes/openbao -p 'test_*.py'
```

Ожидаемо `initialized=true`, `sealed=false`, `recovery_seal=true`,
`storage_type=raft`; после удаления пода он снова Ready без человека.

## Диагностика

- `503` от Traefik: под не Ready (sealed/не инициализирован) или allow-list.
- `error parsing 'audit'`: у `audit` два label — тип и путь.
- `invalid audience (aud) claim`: у k8s-роли задан `audience`; должен быть
  пустым (подовый токен проецируется с audience API-сервера).
- Цикл `failed to setup audit table` / `unable to open … for writing`: в raft
  осталось audit-устройство с несмонтированным путём. Временно смонтировать
  `emptyDir` в старый путь, перезапустить под (декларативный конфиг отключит
  устройство), убрать `emptyDir`, перезапустить ещё раз.
- `cannot enable audit device via API`: только декларативный `audit`.
