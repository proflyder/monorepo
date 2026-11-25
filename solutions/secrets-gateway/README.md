# secrets-gateway

Owner-only серверный API для просмотра и изменения секретов приложения из
`home-app-auth`-модуля `secrets`. Сервис не хранит значения: источник правды —
OpenBao KV v2 (`secret/`), доступ — Kubernetes auth ролью `secrets-gateway`, а
не operator-токеном. Телефон/клиент получает app-токен; `secrets` — owner-only
модуль, поэтому gateway отвечает только владельцу.

```
app token ──▶ secrets-gateway ──Kubernetes auth──▶ OpenBao KV v2
                   │  list / meta / reveal / update / rollback / delete
                   │  device enroll / step-up challenge
                   ├──▶ scheduler  GET /v1/access/snapshot (service-token)
                   ├──▶ phone-push POST /v1/push/notify  module=secrets
                   └──▶ structured audit log (без значений)
```

## Контракт

Bearer — app-токен из snapshot scheduler'а, модуль `secrets`. Без модуля —
`403 {"error":"module_forbidden","module":"secrets"}`; без токена — `401`.
Пути — логические (без `secret/`): `k8s/test`, `k8s/home-assistant/phone-push`.
Принимаются также `secret/data/<path>`, `secret/metadata/<path>` и
`secret/<path>`; `..`, пустые сегменты, не-ASCII и слишком длинные пути
отклоняются.

| Метод | Назначение |
|---|---|
| `GET /v1/secrets` | каталог: `manifests/catalog.json` (`purpose`/`consumer`) + live-листинг KV; core-пути и состояние устройства скрыты, ответ содержит `truncated` |
| `GET /v1/secrets/meta/{path}` | метаданные: `currentVersion`, `oldestVersion`, версии (`createdTime`, `deletionTime`, `destroyed`) и имена ключей текущей версии — **без значений** |
| `POST /v1/secrets/reveal/{path}` `{key,version}` | значение одного ключа одной версии; нужен step-up session |
| `POST /v1/secrets/update/{path}` `{key,value,cas}` | CAS-запись одного ключа в новую версию (остальные ключи сохраняются); soft-deleted текущая версия → `409 version_unavailable`; нужна подпись операции |
| `POST /v1/secrets/rollback/{path}` `{version,cas}` | читает старую версию и пишет её как новую (CAS); нужна подпись операции |
| `DELETE /v1/secrets/{path}?cas=N&version=N` | soft-delete последней версии (`DELETE secret/data/…`); `cas`/`version` обязаны совпасть с текущей, перед удалением метаданные перечитываются; нужна подпись операции; **destroy/metadata-delete/undelete недоступны** |
| `GET /v1/device` | привязан ли телефон: `{enrolled, deviceId, enrolledAt}` |
| `POST /v1/device/challenge` | одноразовый nonce для key attestation: `{challenge, expiresIn}` (15 минут) |
| `POST /v1/device/enroll` `{code,publicKey,attestationChallenge,attestationChain}` | одноразовый код привязки + публичный ключ P-256; сервер сохраняет ключ и device id, старый ключ перестаёт работать |
| `POST /v1/step-up/challenge` `{action,path?,key?,version?,cas?}` | nonce, привязанный к token id, действию, пути/ключу/версии и сроку 60 секунд: `{challenge,payload,expiresIn}` |
| `POST /v1/step-up` `{challenge,signature}` | проверяет подпись `action=session` и открывает 5-минутную сессию; `Cache-Control: no-store` |
| `GET /healthz`, `GET /readyz` | пробы без токена; `readyz` проверяет свежесть snapshot и доступность OpenBao |

`cas` — ожидаемая текущая версия. Несовпадение с OpenBao — `409
{"error":"conflict","currentVersion":N}`; гонка на стороне OpenBao с
`check-and-set` тоже `409`. `key` — до 128 ASCII-символов, значение — до 32
KiB. Ответ reveal `Cache-Control: no-store`; значения никогда не попадают в
логи, audit-push и тексты ошибок. Request timeout — 15 секунд, body limit —
128 KiB, live-листинг ограничен 200 записями и 64 каталогами.

## Step-up: ключ, привязанный к телефону

TOTP удалён. Приложение создаёт неэкспортируемый EC P-256 ключ в Android
Keystore (StrongBox при наличии), с обязательной аутентификацией на каждое
использование (`timeout 0`, `BIOMETRIC_STRONG | DEVICE_CREDENTIAL` на Android
11+), и подписывает серверные challenge через `BiometricPrompt.CryptoObject`.
Публичный ключ и device id хранятся в OpenBao
`k8s/home-assistant/secrets-gateway-device`; путь скрыт из HTTP API только
проверкой в коде (reserved path): `deny.json` его не покрывает, потому что из
того же файла генерируется `policy.hcl`, а поду нужен доступ. ESO этот путь не
синхронизирует.

- **Привязка** — `mise run admin:secrets-device-enroll`: генерирует 16 байт
  (base32, 26 символов), пишет хеш SHA-256 и срок 15 минут в OpenBao, а сам код
  — в файл 0600 (stdout печатает путь). Код одноразовый, сравнение
  constant-time, ошибки дают backoff; nonce расходуется только после успешной
  проверки кода. `--revoke` удаляет ключ и код.
- **Attestation** — цепочка из `KeyStore.getCertificateChain` (leaf → … →
  корень, 2–8 сертификатов, base64url или стандартный base64, с паддингом или
  без) проверяется до закреплённых в коде корней Google Hardware Attestation
  (RSA и ECDSA). Пиннинг — по DER верхнего сертификата или по подписи его ключом
  закреплённого корня; срок действия к верхнему (anchor) не применяется, у листа
  и промежуточных проверяется. `attestationChallenge` должен совпадать с
  выданным nonce, `securityLevel` — TEE или StrongBox, публичный ключ листа —
  с привязываемым ключом. `verified boot` обязателен: `VERIFIED` и
  `deviceLocked=true`; иначе привязка отклоняется, а в лог уходят причина
  (`length`, `parse`, `expired`, `link`, `authority`, `anchor`, `clock`), длина
  цепочки и для каждого сертификата subject, issuer и notAfter.
- **Challenge** — 32 случайных байта; payload
  `home-secrets-step-up/1` содержит id, `principal`, действие, путь, ключ,
  версию, `cas`, хеш нового значения (`valueSha256`, только update) и
  `expires`. Клиент подписывает payload как есть
  (`SHA256withECDSA`, DER), сервер проверяет ECDSA-P256 (`p256`) и сверяет
  операцию с запросом. Подпись нельзя переиспользовать на другом пути,
  действии, версии или после рестарта: использованные nonce пишутся на PVC
  (`/state/step-up.json`) на 60 секунд.
- reveal требует `X-Step-Up-Session`, полученный обменом подписи
  `action=session` на `POST /v1/step-up`; сессия живёт 5 минут, привязана к
  `token_id` и отпечатку ключа, не переживает рестарт.
- update/rollback/delete требуют `X-Step-Up-Challenge` и
  `X-Step-Up-Signature`; nonce одноразовый и связан с точной операцией.
- 5 ошибок подписи/кода включают экспоненциальную блокировку
  `60s, 120s, 240s, …` до часа; после 10 ошибок за 60 с глобальный лимитер
  блокирует все токены на 5 минут. Ответ — `429` c `Retry-After`.
- Ошибки: `step_up_required`, `step_up_invalid`, `step_up_expired`,
  `device_not_enrolled`, `device_unavailable`, `enrollment_invalid` (общий
  ответ на неверный/просроченный код, nonce или аттестацию; точная причина —
  только в audit-логе), `enrollment_conflict`, `rate_limited`.

## Denylist

Владелец решил открыть из телефона **весь** каталог, кроме ядра. Сервис
отказывает (`403 {"error":"path_forbidden"}`) до обращения в OpenBao только
для:

- `openbao/*` — operator token, unseal и внутренние данные OpenBao;
- `infra/vpn/client-ca` — приватный ключ клиентского CA;
- `k8s/flux-system/*` — GitOps deploy key и служебные данные Flux;
- `k8s/home-assistant/scheduler-auth` — app-токены и service-token;
- `k8s/home-assistant/secrets-gateway` — legacy TOTP-секрет.

Дополнительно код скрывает и запрещает собственный путь состояния
`k8s/home-assistant/secrets-gateway-device`, даже если он появится в
live-листинге: публичный ключ и хеш кода не должен отдавать HTTP API.
`infra/*` и `ci/*` вне этих пяти префиксов доступны (например,
`infra/backup/offsite`, `ci/test`). Список задан в
[`manifests/deny.json`](../../projects/home/kubernetes/secrets-gateway/manifests/deny.json),
монтируется ConfigMap и читается сервисом. `policy.hcl` генерируется из того
же файла в `openbao/configure.py`; тест сравнивает политику с `deny.json`,
поэтому код и policy не расходятся. Политика дополнительно запрещает
`sys/*`, `auth/*`, `identity/*`, а также endpoint'ы destroy/delete/undelete/
subkeys; metadata-delete не выдан. Capability `delete` есть только на
`secret/data/*` — это soft-delete версии, не destroy.

## Audit

Каждый запрос пишет structured-строку `event=secrets.access` (`action`,
`principal`, `path`, `key`, `version`, `outcome`) без значений; привязка
телефона — `action=device_enroll`, отзыв — `action=device_revoke`.

Push в `phone-push` (`module=secrets`, только owner-устройства) уходит не на
каждый запрос: `list` и `metadata` только логируются. Push получают reveal,
update, rollback и delete, привязка и отзыв телефона, неудачный/запрещённый
step-up и rate-limit. Каждый push несёт `data {entity:"secrets", path?}` и
`deepLink home://secrets/path/<path с %2F вместо />` (без пути —
`home://secrets`, для привязки — `home://secrets/device`), поэтому
`NotificationTargets` в приложении открывает нужный экран по тапу.
Текст содержит действие и путь, но не значение. Push отправляется
асинхронно и не ломает запрос при недоступном phone-push; `module=secrets`
уходит в FCM `data`, и приложение показывает его с `VISIBILITY_SECRET`, чтобы
пути не появлялись на экране блокировки. Отзыв, сделанный
`admin:secrets-device-enroll -- --revoke`, сервис замечает опросом OpenBao
(не чаще раза в 30 секунд) и присылает один push.

## Переменные окружения

| Переменная | Обязательна | По умолчанию | Назначение |
|---|---|---|---|
| `SECRETS_GATEWAY_LISTEN` | нет | `0.0.0.0:8096` | адрес HTTP |
| `SECRETS_GATEWAY_SCHEDULER_URL` | да | — | база scheduler'а (snapshot доступа) |
| `SECRETS_GATEWAY_SERVICE_TOKEN_FILE` | да | — | service-токен scheduler'а |
| `SECRETS_GATEWAY_OPENBAO_URL` | нет | `http://openbao.openbao.svc:8200` | OpenBao |
| `SECRETS_GATEWAY_OPENBAO_ROLE` | нет | `secrets-gateway` | Kubernetes auth роль |
| `SECRETS_GATEWAY_SA_TOKEN_FILE` | нет | `/var/run/secrets/tokens/token` | projected JWT пода с audience `secrets-gateway` |
| `SECRETS_GATEWAY_CATALOG_FILE` | да | — | `catalog.json` из ConfigMap |
| `SECRETS_GATEWAY_DENY_FILE` | да | — | `deny.json` из ConfigMap |
| `SECRETS_GATEWAY_STATE_FILE` | нет | `/state/step-up.json` | durable использованные nonce на PVC |
| `SECRETS_GATEWAY_DEVICE_PATH` | нет | `k8s/home-assistant/secrets-gateway-device` | путь ключа устройства в OpenBao |
| `SECRETS_GATEWAY_PHONE_PUSH_URL` | да | — | база phone-push |
| `SECRETS_GATEWAY_PHONE_PUSH_TOKEN_FILE` | нет | service-token | bearer для phone-push |
| `SECRETS_GATEWAY_TIMEOUT` | нет | `10s` | таймаут OpenBao (0.5–60 с) |
| `SECRETS_GATEWAY_MAX_LIST_ENTRIES` | нет | `200` | предел live-листинга (1–1000) |

## Сборка и тесты

```bash
cargo test -p secrets-gateway
bazel test //solutions/secrets-gateway/...
bazel build //solutions/secrets-gateway:secrets-gateway_image
```

Тесты герметичны: fake OpenBao HTTP server (auth login, list, metadata, data,
CAS, soft delete, состояние устройства) и fake phone-push. Fake OpenBao
проверяет capability по `policy.hcl` для каждого запроса, поэтому пропавший
`delete`/`read`/`list` ломает тест. Покрыты deny-list, скрытие пути
устройства, owner-only на всех ручках, истечение сессии, привязка challenge к
операции, replay, чужая подпись, отзыв ключа, одноразовость/срок/лимит кода
привязки, CAS conflict, soft-deleted update, отсутствие destroy,
traversal/percent-encoding и audit-push без значений. Реальный OpenBao и
кластер не нужны.

## Заметки к коду

- `openbao.rs` держит Kubernetes-auth **токен OpenBao** в памяти до
  `lease_duration - 60s`; путь к projected SA JWT хранится в конфигурации, а
  сам файл перечитывается на каждом login, поэтому ротация kubelet не ломает
  вход. На `403` кэш сбрасывается и выполняется один повторный login. Мьютекс
  удерживается во время login, чтобы параллельные запросы не открывали
  несколько сессий.
- `stepup.rs` держит сессии в памяти; выданные challenge — 60 секунд, а
  отметки об использованных nonce пишутся в `/state/step-up.json` до ответа,
  поэтому рестарт не позволяет replay. Нет состояния — сервис не стартует.
- `device.rs` читает публичный ключ из OpenBao на каждый challenge/проверку
  сессии, поэтому `--revoke` действует сразу. Код привязки сравнивается
  constant-time по SHA-256, attestation проверяется по Google-корням, а в
  OpenBao сохраняется только её хеш и итоговое состояние.
- `audit.rs` создаёт `PushBody` только из action/path; значения не входят в
  тип события, поэтому не могут утечь в push по ошибке.
- `catalog.rs` и `deny.rs` читают файлы из ConfigMap (`SECRETS_GATEWAY_*_FILE`),
  поэтому solution не вшивает project-пути; `deny.json` — источник и для кода,
  и для сгенерированной `policy.hcl`.
- `api/` разделён по ответственности: `mod.rs` (состояние и роутер),
  `catalog.rs` (список, метаданные), `secret.rs` (reveal/update/rollback/delete),
  `stepup.rs`, `device.rs`, `guard.rs` (аутентификация, нормализация пути,
  проверка step-up подписи), `reply.rs` (ответы), `validate.rs`, `request.rs`.
  Хендлеры возвращают `Result<Response, Rejection>`; `Rejection` хранит
  `Box<Response>`, поэтому `?` работает без `clippy::result_large_err`.
  `AuditScope` фиксирует action/principal/path/key один раз на запрос, и
  каждый отказ проходит через `scope.reject(...)`: аудит и ответ не
  расходятся.
- `health.rs` проверяет свежесть snapshot и `/v1/sys/health` OpenBao; liveness
  остаётся статической.
- `openbao::list_all` ограничен 200 записями, 64 каталогами и 5 уровнями;
  `truncated` возвращается клиенту. HTTP-слой — 15-секундный timeout и
  128-KiB body limit.

## Деплой

Манифесты, роль OpenBao, политика и привязка телефона —
[`projects/home/kubernetes/secrets-gateway/README.md`](../../projects/home/kubernetes/secrets-gateway/README.md)
и `docs/OPERATIONS.md`.
