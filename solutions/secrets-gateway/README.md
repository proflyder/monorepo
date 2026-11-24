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
| `GET /v1/secrets` | каталог: `manifests/catalog.json` (`purpose`/`consumer`) + live-листинг KV; core-пути скрыты, ответ содержит `truncated` |
| `GET /v1/secrets/meta/{path}` | метаданные: `currentVersion`, `oldestVersion`, версии (`createdTime`, `deletionTime`, `destroyed`) и имена ключей текущей версии — **без значений** |
| `POST /v1/secrets/reveal/{path}` `{key,version}` | значение одного ключа одной версии; нужен step-up session |
| `POST /v1/secrets/update/{path}` `{key,value,cas}` | CAS-запись одного ключа в новую версию (остальные ключи сохраняются); soft-deleted текущая версия → `409 version_unavailable`; нужен свежий TOTP |
| `POST /v1/secrets/rollback/{path}` `{version,cas}` | читает старую версию и пишет её как новую (CAS); нужен свежий TOTP |
| `DELETE /v1/secrets/{path}?cas=N&version=N` | soft-delete последней версии (`DELETE secret/data/…`); `cas`/`version` обязаны совпасть с текущей; нужен свежий TOTP; **destroy/metadata-delete/undelete недоступны** |
| `POST /v1/step-up` `{totp}` | открывает 5-минутную сессию для reveal; `Cache-Control: no-store` |
| `GET /healthz`, `GET /readyz` | пробы без токена; `readyz` проверяет свежесть snapshot и доступность OpenBao |

`cas` — ожидаемая текущая версия. Несовпадение с OpenBao — `409
{"error":"conflict","currentVersion":N}`; гонка на стороне OpenBao с
`check-and-set` тоже `409`. `key` — до 128 ASCII-символов, значение — до 32
KiB. Ответ reveal `Cache-Control: no-store`; значения никогда не попадают в
логи, audit-push и тексты ошибок. Request timeout — 15 секунд, body limit —
128 KiB, live-листинг ограничен 200 записями и 64 каталогами.

## Step-up

TOTP: RFC 6238, SHA-1, 30 с, 6 цифр, окно ±1 шаг. Секрет (`totp_secret`,
base32) хранится в OpenBao `k8s/home-assistant/secrets-gateway` и приходит в
под через ESO. Enrollment — `mise run admin:secrets-totp-init` (otpauth URI
пишется в файл 0600, не в stdout).

- reveal требует `X-Step-Up-Session`, полученный из `POST /v1/step-up`;
  сессия живёт 5 минут, привязана к `token_id` и не переживает рестарт пода.
- update/rollback/delete требуют `X-Step-Up-Totp` на каждом запросе; повтор
  того же шага отклоняется. Успешный шаг пишется в `/state/last-totp-step`
  (PVC), поэтому рестарт пода не позволяет повторно использовать код.
  Код, использованный для сессии, можно один раз использовать для мутации в
  том же окне.
- 5 ошибок включают экспоненциальную блокировку `60s, 120s, 240s, …` до
  часа; после 10 ошибок за 60 с глобальный лимитер блокирует все токены на
  5 минут. Ответ — `429` c `Retry-After`.

## Denylist

Владелец решил открыть из телефона **весь** каталог, кроме ядра. Сервис
отказывает (`403 {"error":"path_forbidden"}`) до обращения в OpenBao только
для:

- `openbao/*` — operator token, unseal и внутренние данные OpenBao;
- `infra/vpn/client-ca` — приватный ключ клиентского CA;
- `k8s/flux-system/*` — GitOps deploy key и служебные данные Flux;
- `k8s/home-assistant/scheduler-auth` — app-токены и service-token;
- `k8s/home-assistant/secrets-gateway` — собственный TOTP.

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
| `SECRETS_GATEWAY_SA_TOKEN_FILE` | нет | `/var/run/secrets/tokens/token` | projected JWT пода с audience `secrets-gateway` |
| `SECRETS_GATEWAY_TOTP_FILE` | да | — | base32 TOTP-секрет |
| `SECRETS_GATEWAY_CATALOG_FILE` | да | — | `catalog.json` из ConfigMap |
| `SECRETS_GATEWAY_DENY_FILE` | да | — | `deny.json` из ConfigMap |
| `SECRETS_GATEWAY_STATE_FILE` | нет | `/state/last-totp-step` | durable последний TOTP-шаг на PVC |
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
CAS, soft delete) и fake phone-push. Fake OpenBao проверяет capability по
`policy.hcl` для каждого запроса, поэтому пропавший `delete`/`read`/`list`
ломает тест. Покрыты deny-list, owner-only на всех ручках, истечение сессии,
persisted TOTP replay, CAS conflict, soft-deleted update, отсутствие destroy,
traversal/percent-encoding и audit-push без значений. Реальный OpenBao и
кластер не нужны.

## Заметки к коду

- `openbao.rs` держит Kubernetes-auth токен в памяти до `lease_duration - 60s`;
  на `403` один раз логинится заново и повторяет запрос. Мьютекс удерживается
  во время login, чтобы параллельные запросы не открывали несколько сессий.
- `stepup.rs` держит сессии и счётчики ошибок в памяти; последний успешный
  TOTP-шаг пишется в `/state/last-totp-step` до мутации, поэтому рестарт не
  позволяет replay. Нет состояния — сервис не стартует.
- `audit.rs` создаёт `PushBody` только из action/path; значения не входят в
  тип события, поэтому не могут утечь в push по ошибке.
- `catalog.rs` и `deny.rs` читают файлы из ConfigMap (`SECRETS_GATEWAY_*_FILE`),
  поэтому solution не вшивает project-пути; `deny.json` — источник и для кода,
  и для сгенерированной `policy.hcl`.
- `health.rs` проверяет свежесть snapshot и `/v1/sys/health` OpenBao; liveness
  остаётся статической.
- `openbao::list_all` ограничен 200 записями, 64 каталогами и 5 уровнями;
  `truncated` возвращается клиенту. HTTP-слой — 15-секундный timeout и
  128-KiB body limit.

## Деплой

Манифесты, роль OpenBao, политика и TOTP enrollment —
[`projects/home/kubernetes/secrets-gateway/README.md`](../../projects/home/kubernetes/secrets-gateway/README.md)
и `docs/OPERATIONS.md`.
