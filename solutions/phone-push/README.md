# phone-push

Сервис доставки на телефон владельца через **FCM HTTP v1**: «найди телефон» и
обычные уведомления. Сервис без состояния: токены устройств живут в
scheduler'е, FCM-сервис-аккаунт — единственный секрет этого процесса.

```
Hermes ──MCP── mcp-gateway ──phone_find / phone_notify──▶ phone-push
                                                             │  GET /v1/push/devices
                                                             ▼
                                                        scheduler (реестр токенов)
                                                             │
                                                     OAuth2 + FCM HTTP v1
                                                             ▼
                                                fcm.googleapis.com ──▶ телефон
```

Почему реестр в scheduler'е, а отправка здесь:

- телефон уже аутентифицируется **одним** endpoint'ом и **одним** bearer-токеном
  (scheduler + control + intercom); отдельный push-endpoint потребовал бы ещё
  один адрес в Remote Config и ещё один credential на телефоне;
- scheduler — durable-владелец домашнего состояния (SQLite на PVC), поэтому
  реестр переживает рестарт; он **не отправляет** ничего, то есть доставка
  остаётся вне планировщика;
- процесс отправки stateless, его можно катить и перезапускать без миграций, и
  именно он владеет Google-credential'ом (ни gateway, ни Hermes его не видят).

## Контракт

Bearer — `PUSH_TOKEN_FILE`, сравнение в постоянное время. Токены устройств
никогда не возвращаются и не логируются: ответы содержат только счётчики.

| Метод | Назначение |
|---|---|
| `POST /v1/push/find` `{reason?}` | data-only сообщение высокого приоритета, без `notification`: приложение включает громкий звонок (`USAGE_ALARM`, обход DND, full-screen intent), стоп по «Нашёл» или через 60 с |
| `POST /v1/push/notify` `{title, body, deepLink?}` | обычное уведомление: заголовок/текст/deep link, канал `home_notifications` |
| `GET /healthz`, `GET /readyz` | пробы без токена |

Ответ: `{"sent": n, "failed": m, "devices": k}`. Мёртвый токен
(`UNREGISTERED`) удаляется из реестра, чтобы следующая отправка не тратила
попытку.

## FCM

`PUSH_FCM_SERVICE_ACCOUNT_FILE` — JSON сервис-аккаунта. Подпись RS256
(`ring`), обмен на access token через `oauth2.googleapis.com/token`, кэш в
памяти до истечения (минус минута). Ничего не пишется на диск.

Сообщение «найди телефон» — **data-only** и `android.priority=high`:
`notification`-блок не ставится, иначе система нарисует трей и не станет
будить приложение, когда оно убито. Обычное уведомление, наоборот, несёт
`notification` и имя канала; текст приходит с сервера, все подписи и действия
на телефоне — из ресурсов приложения.

## Переменные окружения

| Переменная | Обязательна | По умолчанию | Назначение |
|---|---|---|---|
| `PUSH_LISTEN` | нет | `0.0.0.0:8095` | адрес HTTP |
| `PUSH_TOKEN_FILE` | да | — | bearer для `/v1/push/*` |
| `PUSH_SCHEDULER_URL` | да | — | база scheduler'а (реестр устройств) |
| `PUSH_SCHEDULER_TOKEN_FILE` | да | — | bearer scheduler'а (`scheduler-auth#token`) |
| `PUSH_FCM_SERVICE_ACCOUNT_FILE` | да | — | JSON сервис-аккаунта FCM |
| `PUSH_TOKEN_URL` | нет | `https://oauth2.googleapis.com/token` | OAuth endpoint (тесты) |
| `PUSH_FCM_URL` | нет | `https://fcm.googleapis.com` | база FCM API (тесты) |
| `PUSH_TIMEOUT` | нет | `10s` | таймаут одного запроса (0.5–60 с) |
| `LOG_LEVEL` | нет | `info` | уровень логов |

## Сборка и тесты

```bash
cargo test -p phone-push
bazel test //solutions/phone-push/...
bazel build //solutions/phone-push:phone-push_image
```

Тесты герметичны: фейковые OAuth/FCM-серверы, подпись проверяется настоящим
RSA-ключом из `testdata/` (одноразовый ключ только для тестов, см. allowlist
Gitleaks). Реальная отправка в CI не выполняется.

## Что нужно от владельца

Создание сервис-аккаунта и роль — в
`projects/home/kubernetes/phone-push/README.md`.
