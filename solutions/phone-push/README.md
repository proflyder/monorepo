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

Bearer — service-token (`PUSH_SERVICE_TOKEN_FILE`, обязателен, без fallback), сравнение в постоянное время. App-токены здесь не принимаются. Токены устройств
никогда не возвращаются и не логируются: ответы содержат только счётчики.

| Метод | Назначение |
|---|---|
| `POST /v1/push/find` `{reason?, deviceId?}` | data-only сообщение высокого приоритета, без `notification`: приложение включает громкий звонок (`USAGE_ALARM`, обход DND, full-screen intent), стоп по «Нашёл» или через 60 с |
| `POST /v1/push/notify` `{title, body, deepLink?, deviceId?}` | обычное уведомление (data-only): заголовок/текст/deep link; приложение рисует его в канале `home_notifications` в цвете темы |
| `GET /v1/push/devices` | идентификаторы, имена и платформы зарегистрированных телефонов — **без токенов**; gateway сопоставляет по ним имя |
| `GET /healthz`, `GET /readyz` | пробы без токена |

`deviceId` — стабильная идентичность телефона из приложения, а не FCM-токен.
Без него сообщение уходит на все телефоны; с ним — ровно на один. Ответ:
`{"sent": n, "failed": m, "devices": k}`. Мёртвый токен (`UNREGISTERED`)
удаляется из реестра, чтобы следующая отправка не тратила попытку.

После успешной доставки уведомления или «найди телефон» сервис пишет
строку истории в scheduler (`POST /v1/push/history`) на каждый доставленный
`deviceId`. `id` записи генерируется заранее и уходит в FCM `data` как
`notificationId`, поэтому повтор после таймаута не создаёт вторую строку.
Запись выполняется после fan-out, параллельно и не блокирует доставку
следующему телефону; ошибка записи только логируется и не меняет счётчики
доставки. История доступна телефону на том же scheduler-адресе
(`GET /v1/push/history`).

Сопоставление имени («Стас», «Стаса», «телефон Стаса») выполняет
`mcp-gateway` по списку из `GET /v1/push/devices`: сервис отправки остаётся
простым fan-out и не знает про падежи и уменьшительные. Токен по-прежнему не
возвращается ни в один ответ.

## FCM

`PUSH_FCM_SERVICE_ACCOUNT_FILE` — JSON сервис-аккаунта. Подпись RS256
(`ring`), обмен на access token через `oauth2.googleapis.com/token`, кэш в
памяти до истечения (минус минута). Ничего не пишется на диск.

Все сообщения — **data-only** (`android.priority=high` для «найди телефон» и
обычного уведомления): `notification`-блок не ставится. Иначе, пока
приложение в фоне, трей рисует FCM SDK с иконкой приложения и без цвета, а не
приложение — с иконкой и цветом текущей темы, каналом и deep link. Текст
(`title`, `body`, `deepLink`) приходит в `data`; все подписи и действия на
телефоне — из ресурсов приложения.

## Переменные окружения

| Переменная | Обязательна | По умолчанию | Назначение |
|---|---|---|---|
| `PUSH_LISTEN` | нет | `0.0.0.0:8095` | адрес HTTP |
| `PUSH_SERVICE_TOKEN_FILE` | да | — | service-token для `/v1/push/*` |
| `PUSH_SCHEDULER_URL` | да | — | база scheduler'а (реестр устройств) |
| `PUSH_SCHEDULER_SERVICE_TOKEN_FILE` | да | — | service-token scheduler'а (`scheduler-auth#service_token`) |
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
RSA-ключом из `crates/phone-push/testdata/` (одноразовый ключ только для тестов, см. allowlist
Gitleaks). Реальная отправка в CI не выполняется.

## Деплой

Манифесты, секреты и создание сервис-аккаунта —
[`projects/home/kubernetes/phone-push/README.md`](../../projects/home/kubernetes/phone-push/README.md).
