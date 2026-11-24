# secrets-gateway

Owner-only серверный API для просмотра и изменения секретов приложения из
`home-app-auth`-модуля `secrets`. Сервис не хранит значения: источник правды —
OpenBao KV v2 (`secret/`), доступ — Kubernetes auth ролью `secrets-gateway`, а
не operator-токеном. Телефон/клиент получает app-токен; `secrets` — owner-only
модуль, поэтому gateway отвечает только владельцу.

```
app token ──▶ secrets-gateway ──Kubernetes auth──▶ OpenBao KV v2
                   │  list / meta / reveal / update / rollback / delete
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
| `GET /v1/secrets` | каталог: пути из `platform/workstation/catalog.json` (`purpose`/`consumer`) + live-листинг KV; deny-пути скрыты |
| `GET /v1/secrets/meta/{path}` | метаданные: `currentVersion`, `oldestVersion`, версии (`createdTime`, `deletionTime`, `destroyed`) и имена ключей текущей версии — **без значений** |
| `POST /v1/secrets/reveal/{path}` `{key,version}` | значение одного ключа одной версии; нужен step-up session |
| `POST /v1/secrets/update/{path}` `{key,value,cas}` | CAS-запись одного ключа в новую версию (остальные ключи сохраняются); нужен свежий TOTP |
| `POST /v1/secrets/rollback/{path}` `{version,cas}` | читает старую версию и пишет её как новую (CAS); нужен свежий TOTP |
| `DELETE /v1/secrets/{path}` | soft-delete последней версии (`DELETE secret/data/…`); нужен свежий TOTP; **destroy недоступен** |
| `POST /v1/step-up` `{totp}` | открывает 5-минутную сессию для reveal |
| `GET /healthz`, `GET /readyz` | пробы без токена |

`cas` — ожидаемая текущая версия. Несовпадение с OpenBao — `409
{"error":"conflict","currentVersion":N}`; гонка на стороне OpenBao с
`check-and-set` тоже `409`. `key` — до 128 ASCII-символов, значение — до 32
KiB. Ответ reveal `Cache-Control: no-store`; значения никогда не попадают в
логи, audit-push и тексты ошибок.

## Step-up

TOTP: RFC 6238, SHA-1, 30 с, 6 цифр, окно ±1 шаг. Секрет (`totp_secret`,
base32) хранится в OpenBao `k8s/home-assistant/secrets-gateway` и приходит в
под через ESO. Enrollment — `mise run admin:secrets-totp-init` (otpauth URI
пишется в файл 0600, не в stdout).

- reveal требует `X-Step-Up-Session`, полученный из `POST /v1/step-up`;
  сессия живёт 5 минут, привязана к `token_id` и не переживает рестарт пода.
- update/rollback/delete требуют `X-Step-Up-Totp` на каждом запросе; повтор
  того же шага отклоняется. Код, использованный для сессии, можно один раз
  использовать для мутации в том же окне.
- 5 ошибок за 60 с блокируют `token_id` на 60 с: `429` c `Retry-After`.

## Denylist

Разрешены только KV v2 `secret/data|metadata/*`; сервис дополнительно
отказывает (`403 {"error":"path_forbidden"}`) до обращения в OpenBao:

- `infra/*` и `ci/*` — доступ владельца, VPN client CA, SSH, kubeconfigs,
  backup, Cloudflare, CI OIDC;
- `k8s/flux-system/*`, `k8s/home-assistant/scheduler-auth` — GitOps и
  app-доступ;
- `k8s/home-assistant/secrets-gateway` — собственный TOTP;
- `openbao/*`, `sys/*`, `auth/*`, `identity/*`, `destroy|delete|undelete|subkeys|config`;
- сегменты `seal`, `unseal`, `unseal-key`, `operator`, `recovery`,
  `recovery-key`.

Та же граница записана в
[`projects/home/kubernetes/secrets-gateway/policy.hcl`](../../projects/home/kubernetes/secrets-gateway/policy.hcl)
для роли OpenBao; код проверяет её независимо, потому что policy не защищает
от логической ошибки клиента. Unseal material в KV не лежит (это Kubernetes
Secret `openbao-seal`), поэтому deny-сегменты — дополнительный барьер.

## Audit

Каждый запрос пишет structured-строку `event=secrets.access` (`action`,
`principal`, `path`, `key`, `version`, `outcome`) без значений. Дополнительно
`phone-push` получает уведомление `module=secrets` (только owner-устройства);
текст содержит действие и путь, но не значение. Push отправляется
асинхронно и не ломает запрос при недоступном phone-push.

## Переменные окружения

| Переменная | Обязательна | По умолчанию | Назначение |
|---|---|---|---|
| `SECRETS_GATEWAY_LISTEN` | нет | `0.0.0.0:8096` | адрес HTTP |
| `SECRETS_GATEWAY_SCHEDULER_URL` | да | — | база scheduler'а (snapshot доступа) |
| `SECRETS_GATEWAY_SERVICE_TOKEN_FILE` | да | — | service-токен scheduler'а |
| `SECRETS_GATEWAY_OPENBAO_URL` | нет | `http://openbao.openbao.svc:8200` | OpenBao |
| `SECRETS_GATEWAY_OPENBAO_ROLE` | нет | `secrets-gateway` | Kubernetes auth роль |
| `SECRETS_GATEWAY_SA_TOKEN_FILE` | нет | `/var/run/secrets/kubernetes.io/serviceaccount/token` | JWT пода |
| `SECRETS_GATEWAY_TOTP_FILE` | да | — | base32 TOTP-секрет |
| `SECRETS_GATEWAY_PHONE_PUSH_URL` | да | — | база phone-push |
| `SECRETS_GATEWAY_PHONE_PUSH_TOKEN_FILE` | нет | service-token | bearer для phone-push |
| `SECRETS_GATEWAY_TIMEOUT` | нет | `10s` | таймаут OpenBao (0.5–60 с) |
| `SECRETS_GATEWAY_MAX_LIST_ENTRIES` | нет | `500` | предел live-листинга (1–2000) |

## Сборка и тесты

```bash
cargo test -p secrets-gateway
bazel test //solutions/secrets-gateway/...
bazel build //solutions/secrets-gateway:secrets-gateway_image
```

Тесты герметичны: fake OpenBao HTTP server (auth login, list, metadata, data,
CAS, soft delete) и fake phone-push; покрыты deny-list, owner-only модуль,
истечение step-up, CAS conflict, отсутствие destroy и audit-push без значений.
Реальный OpenBao и кластер не нужны.

## Заметки к коду

- `openbao.rs` держит Kubernetes-auth токен в памяти до `lease_duration - 60s`;
  на `403` один раз логинится заново и повторяет запрос. Мьютекс удерживается
  во время login, чтобы параллельные запросы не открывали несколько сессий.
- `stepup.rs` держит сессии и счётчики ошибок в памяти: один replica,
  рестарт сбрасывает сессии (это желаемое поведение для 5-минутного окна).
- `audit.rs` создаёт `PushBody` только из action/path; значения не входят в
  тип события, поэтому не могут утечь в push по ошибке.
- `catalog.rs` вшивает `platform/workstation/catalog.json` на этапе сборки
  (`compile_data`), поэтому каталог — один источник правды и в образе, и в
  workstation-скриптах.

## Деплой

Манифесты, роль OpenBao, политика и TOTP enrollment —
[`projects/home/kubernetes/secrets-gateway/README.md`](../../projects/home/kubernetes/secrets-gateway/README.md)
и `docs/OPERATIONS.md`.
