# scheduler

Durable планировщик будильников, напоминаний, таймеров и generic-задач.
Один процесс, одна SQLite-база, один поток тиков. Сервис **не проигрывает
звук**: он фиксирует occurrence в durable outbox, а executor (HA bridge,
Android, будущий агент) забирает событие, выполняет действие и подтверждает
его.

Процесс — Rust-крейт `crates/scheduler`; домашний деплой —
[`projects/home/kubernetes/scheduler/`](../../projects/home/kubernetes/scheduler/README.md).
`testdata/contract`, `testdata/planner`, `testdata/ids.json` и
`testdata/go_v1.sql` (дамп схемы v1) — замороженные golden-данные.

`home.v1.AlarmService` отдаётся на том же listener и той же реализации, что и
`home.v1.SchedulerService`; это facade над теми же schedules, не второй
планировщик.

## Контракт

`packages/proto/home/v1/scheduler.proto`, package `home.v1`,
service `SchedulerService`:

- schedules: `CreateSchedule`, `GetSchedule`, `ListSchedules`,
  `UpdateSchedule`, `DeleteSchedule`, `SetScheduleEnabled`,
  `SnoozeSchedule`, `StopSchedule`, `SkipScheduleOccurrence`;
- scenarios: `home.v1.ScenarioService` в `packages/proto/home/v1/scenario.proto`
  — `CreateScenario`, `GetScenario`, `ListScenarios`, `UpdateScenario`,
  `DeleteScenario`, `SetScenarioEnabled`, `RunScenario`. Scheduler только
  хранит и валидирует определения; условия и шаги исполняет отдельный
  `ha-scenario` (`solutions/ha-bridge`). `GetStatus.scenario_execution_supported`
  — флаг `SCENARIO_EXECUTION` (default `false`), включаемый вместе с
  исполнителем;
- timers: `StartTimer`, `GetTimer`, `ListTimers`, `PauseTimer`,
  `ResumeTimer`, `CancelTimer`;
- outbox: `ListDueEvents`, `AckEvent`;
- generic queue: `EnqueueTask`, `CancelTask`;
- status: `GetStatus`.

Streaming (`WatchEvents`) отложен; unary-контракта достаточно для мобильного
клиента и HA bridge. Field names в Connect JSON — lowerCamelCase, `int64` —
decimal string (стандартный protobuf JSON). Пустые/дефолтные поля могут
отсутствовать в JSON.

### Транспорты

Один listener отдаёт Connect unary (JSON и binary) и gRPC-Web по HTTP/1.1.
Отдельного REST API нет. `Content-Type: application/json` +
`/home.v1.SchedulerService/<Method>` — это Connect JSON, а не hand-written
REST.

gRPC over h2c не отдаётся: `connectrpc` собран только с feature `json` (без
`hyper/http2`). Добавление h2c — отдельное решение с проверкой зависимостей
образа.

Auth: `Authorization: Bearer <token>` на каждом RPC. Сравнение
constant-time по SHA-256; токен не логируется. `SCHEDULER_CONSUMERS`
опционально ограничивает `consumer_id` allowlist.

## Recurrence, UTC и заголовок Time-Zone

Сервер хранит и считает **только UTC**: `at_*` — UTC time-of-day,
`weekdays` — UTC-маска ISO 1=Mon..7=Sun, `once_unix` — UTC instant.
Поля `timezone` в protobuf и в БД нет.

Клиент обязан присылать `Time-Zone: <IANA>` на **каждом** запросе
(`home.v1.SchedulerService` и `home.v1.AlarmService`). По нему API
переводит `at`/`once_date`/`weekdays` из зоны клиента в UTC при записи и
обратно при чтении; `nextDueUnix`/`nextRunUnix` — UTC epoch, клиент
рендерит его в своей зоне. Нет заголовка — времена трактуются как UTC и
пишется warning; неизвестная зона — `400 invalid Time-Zone header`.
Конверсия живёт в одном месте: `api/zone.rs` + `domain/rule.rs`
(`local_to_utc`/`utc_to_local`), включая сдвиг маски дней через полночь
UTC. Пример: Алматы «будни 03:00» хранится как вс–чт 22:00 UTC и
отдаётся Москве как пн–пт 01:00.

`REPEAT_KIND_ONCE/DAILY/WEEKDAYS/WEEKENDS/CUSTOM` (ISO 1=Mon..7=Sun).
Reminder — `SCHEDULE_KIND_REMINDER`, обязательно `REPEAT_KIND_ONCE` с
`once_date`. Для повторяющихся правил next-fire — календарный расчёт по
UTC, без tzdb и DST; DST-переходы остаются заботой клиента.

## Политика опозданий и повторов

- grace `SCHEDULER_MISSED_GRACE_SECONDS` (600 по умолчанию);
- occurrence в пределах grace эмитится как `FIRE`;
- более старый occurrence фиксируется как явный `SKIPPED` event с
  `detail=missed_grace`, после чего расписание **coalesce**-ится к первому
  occurrence после `now`. Пропущенные дни не разворачиваются в лавину
  событий и не блокируют таймеры;
- timer, просроченный на любой срок, эмитится один раз (countdown завершился,
  пока процесс лежал);
- `ONCE` после emission выключается; `snooze` для него всё равно доступен,
  пока активно ringing window.

## Ringing window, stop и snooze

Активное occurrence хранится в schedule (`active_occurrence_unix`,
`active_until_unix`) **отдельно** от transport-состояния события
(`PENDING/LEASED/DELIVERED`). Поэтому:

- `DELIVERED` ack означает «executor принял действие», но не стирает
  возможность `Stop`/`Snooze`;
- `SnoozeSchedule` ставит немедленный `STOP` control event и отложенный
  `FIRE` (тот же occurrence, `snooze_seq+1`);
- `StopSchedule` ставит немедленный `STOP` и отменяет pending triggers;
- `SkipScheduleOccurrence` сдвигает recurring schedule на следующее
  occurrence без выключения: pending `FIRE` пропускаемого occurrence
  помечается `SKIPPED` (`detail=skipped`), активное occurrence закрывается;
  one-shot и уже выключенный schedule отвергаются;
- повторный `Stop` в том же окне идемпотентен (тот же STOP event);
- `Snooze` после `Stop` возвращает `FAILED_PRECONDITION`;
- пока идёт snooze-ожидание, повторный snooze разрешён (active snooze
  semantics);
- `UpdateSchedule` и `SetScheduleEnabled(true)` во время активного occurrence
  (звонок или pending snooze) закрывают его durable STOP
  (`detail=rule_changed`) со старым label/payload и отменяют pending `FIRE`
  этого occurrence; `Stop` сразу после правки идемпотентен;
- `SetScheduleEnabled(false)` окно не закрывает: STOP придёт по истечении
  `RingWindow`;
- recurrence не сдвигается snooze/stop.

`DueEvent.action`: `FIRE` — начать occurrence, `STOP` — остановить звук.
Control events **не перезаписывают** пользовательский `payload_json`; причина
лежит в `detail`. Trigger и control имеют разные стабильные dedupe identity.

### Истечение ringing window

`RingWindow` (`SCHEDULER_RING_WINDOW_SECONDS`, 900 по умолчанию) — это
**максимальная длительность одного срабатывания**, а не grace доставки.
Scheduler владеет lifecycle occurrence, поэтому истечение окна закрывает
срабатывание, а не только чистит state:

- в той же SQLite-транзакции tick: pending/leased `FIRE` **этого же**
  occurrence помечаются `SKIPPED` (`detail=ring_window_expired`), чтобы
  поздняя доставка не начала звук после закрытия окна;
- для `SCHEDULE_KIND_ALARM` создаётся ровно одна логическая STOP-запись с
  `occurrence_unix` истёкшего occurrence и стабильным dedupe key
  `expiry:stop:<kind>:<id>:<occurrence>`; повторный tick, restart и
  lease-redelivery возвращают тот же event id. Доставка — штатная durable
  очередь с at-least-once (не exactly-once side effect), retry ограничен
  `SCHEDULER_EVENT_MAX_ATTEMPTS`; HA-сторона делает повторный STOP no-op,
  если звонок уже молчит;
- окно и snooze очищаются, revision растёт;
- для reminder/timer глобальный STOP не создаётся (у них нет безопасного
  общего stop); их pending `FIRE` также отменяется;
- `StopSchedule` в пределах `RingWindow` после expiry идемпотентен (тот же
  STOP replay), `SnoozeSchedule` остаётся `FAILED_PRECONDITION`;
- STOP истёкшего occurrence не трогает следующий occurrence того же
  расписания: bridge сверяет source+occurrence, HA — source+FIRE event id.


## Сценарии

`home.v1.ScenarioService` хранит определения сценариев. scheduler — их
единственный владелец: он не исполняет шаги и не вычисляет условия.

- `Scenario`: `id`, `label`, `description`, `enabled`, `revision`,
  timestamps, `triggers`, `conditions`, `steps`, `continue_on_error` и
  вычисляемый `next_due_unix` (ближайший time-триггер; 0 — disabled/manual/
  exhausted).
- Триггеры v1: `manual` и `time` (IANA timezone, `WallTime`, повтор
  once/daily/weekdays/weekends/custom — та же политика DST, что у Schedule).
  Модель — `oneof`, новые виды триггеров добавляются без ломки.
- Условия v1: `entity_state{entity_id,state}` и
  `time_window{start,end,weekdays}`. Их вычисляет исполнитель.
- Шаги v1 (`oneof`): `catalog{entry_id,action,params_json}` (формат
  `home.control`), `speak{text}` (1..500 символов, без control-символов),
  `delay{seconds}` (1..3600).
- Лимиты: label ≤ 80 байт, ≤ 8 триггеров, ≤ 8 условий, ≤ 32 шагов, тело
  определения ≤ `SCHEDULER_MAX_PAYLOAD_BYTES` (16 KiB).

Time-триггеры не живут отдельной очередью: при create/update/enable каждый
time-триггер материализуется строкой `schedules` с
`kind=SCHEDULE_KIND_SCENARIO` и payload
`{"scenario_id":"…","trigger_index":n}`. Tick, DST, missed grace, dedupe и
outbox переиспользуются. Событие уходит в очередь `scenario` с
`task_type=scenario.fire`. Update/Delete/SetEnabled пересобирают эти строки в
одной транзакции с определением: активный occurrence закрывается как при
`rule_changed`, pending FIRE отменяются. У disabled-сценария строки остаются
с `enabled=0` и `next_due=0`; удаление сценария удаляет их. `RunScenario`
кладёт manual-событие
(`payload={"scenario_id":…,"trigger":"manual"}`) в ту же очередь; непустой
`idempotency_key` возвращает то же событие. Manual-запуск разрешён и для
disabled-сценария: `enabled` отключает только автотриггеры.

`GetStatus.scenario_store_supported=true`; `scenario_execution_supported`
отражает deployment-флаг `SCENARIO_EXECUTION` (очередь `scenario` исполняет
отдельный `ha-scenario`).
`ListSchedules` без явного `kind` (UNSPECIFIED) исключает scenario-строки;
только `kind=SCENARIO` их возвращает, поэтому Android-будильники и голосовые
скрипты не видят внутренние триггеры. Прямые
`UpdateSchedule`/`DeleteSchedule`/`SetScheduleEnabled`/`SnoozeSchedule`/
`StopSchedule` по scenario-строке отвечают `FAILED_PRECONDITION`
(`managed by scenario <id>`), а `CreateSchedule` с `kind=SCENARIO` —
`INVALID_ARGUMENT`: такие строки создаются только `ScenarioService`.

## Durable outbox, leases, tasks

- `events.dedupe_key` уникален; event id = UUIDv5(dedupe identity), поэтому
  повторный tick, redelivery и restart возвращают тот же id;
- `ListDueEvents` арендует события (`lease_seconds`, default 60) и повышает
  `attempts`; expired lease возвращается в `PENDING`, после
  `SCHEDULER_EVENT_MAX_ATTEMPTS` — `FAILED`;
- первый `AckEvent` требует `LEASED`, точного `consumer_id` и неистёкшего
  lease; невалидный ack не создаёт consumer heartbeat. Terminal replay
  идемпотентен;
- `EnqueueTask` — реальная durable generic-очередь: `task_type`, JSON payload
  (≤16 KiB), `run_at_unix`, `queue`, idempotency key. Один shot; recurring
  generic task schedules пока не реализованы (`SCHEDULE_KIND_TASK` не
  добавлен);
- queues: convenience alarm/reminder/timer события идут в `home` с
  `task_type=alarm.fire|reminder.fire|timer.fire` (плюс `.stop` для control);
  generic tasks по умолчанию в `tasks`. Executor арендует только свою
  очередь; `ListDueEvents.queue=""` — admin/all;
- `CancelTask` отменяет только `PENDING` task event; leased → precondition;
- `GetStatus` показывает server clock, backlog, known queues, consumer
  liveness, `ready` и last tick error. Он не утверждает, что звук был слышен.
- `SCHEDULER_EVENT_RETENTION_SECONDS` чистит только terminal events и старые
  idempotency keys.

## Broker / MCP boundary

Внешнего broker нет: SQLite outbox достаточен для одного home-сервиса. Broker,
если понадобится, станет ещё одним consumer/producer, а `EnqueueTask`/
`ListDueEvents`/`AckEvent` останутся контрактом. Scheduler не импортирует
Hermes/HA; MCP — общий `solutions/mcp-gateway`.

## Хранилище

SQLite через `home-sqlite` (`rusqlite`, bundled), WAL, `busy_timeout`,
одно соединение => один writer. Tick выполняет
event-insert и advance источника в одной транзакции. `BEGIN IMMEDIATE`
не нужен, пока процесс один; второй процесс на той же БД не поддерживается.
Schema version проверяется: БД новее бинарника отвергается.

Схема v2 добавляет таблицу `scenarios(id, body_json, enabled, revision,
created_unix, updated_unix)`, схема v3 — реестр `push_devices`, схема v4
переключает его первичный ключ с FCM-токена на стабильный `device_id`
(`push_devices(device_id PRIMARY KEY, token UNIQUE, platform, app_version,
device_name, created_unix, updated_unix)`), схема v6 — VPN-пиры, схема v7 —
`notifications(id, device_id, type, title, body, deep_link, created_unix,
read_unix)` с индексом `(device_id, created_unix DESC)`. База v1/v2/v3 мигрирует на месте при старте без
изменения schedules, timers, events и idempotency. Записи v3, ключённые по
токену, сохраняются с токеном в роли provisional `device_id`; первая
регистрация обновлённого приложения присылает настоящий `device_id` и
схлопывает старую строку того же токена, а не создаёт вторую. Обратный откат
бинарника после миграции требует восстановить копию БД предыдущей версии:
старый бинарник отвергает более новую схему. `backup.sh` копирует файл
целиком и schema-проверок не делает, поэтому менять его не требуется.

### Push-устройства (`/v1/push/devices`)

Телефон регистрирует FCM-токен на том же endpoint'е и тем же bearer'ом, что и
Connect RPC: `POST /v1/push/devices` (upsert по стабильному `deviceId` от
приложения, ≤16 устройств), `POST /v1/push/devices/rename` (имя телефона без
перерегистрации), `POST /v1/push/devices/remove`, `GET /v1/push/devices`
(читает `solutions/phone-push`). Это не часть `SchedulerService`: регистрация
— plain JSON рядом с Connect-fallback'ом, чтобы телефону не понадобился
второй адрес или второй токен. Scheduler хранит токены и ничего не
отправляет; FCM service account и сама отправка — в `phone-push`, поэтому
доставка не попадает в планировщик. Ротация FCM-токена обновляет строку
того же `device_id`; токен уникален в реестре. Ответы содержат token (нужен
реестру), но токен никогда не пишется в лог и не попадает в речь
MCP-инструментов.

### История уведомлений (`/v1/push/history`)

История — per-device состояние scheduler'а, а не телефона. Телефон берётся из
заголовка `Home-Device-Id` на каждом запросе; без него — `400` с телом
`{"error":"device_required"}` (клиент распознаёт только этот код). Записи
создаёт `phone-push` после успешной доставки (по одной на доставленный
`deviceId`), поэтому телефон ничего не загружает в историю сам.

- `GET /v1/push/history?cursor=&limit=20` — новые сверху, keyset-курсор
  `created_unix:id`, `nextCursor` есть только у полной страницы; предел
  `limit` — 50;
- `GET /v1/push/history/unread-count` — `{"unread":n}` для бейджа;
- `POST /v1/push/history/read` `{"id","read"}` — прочитано/не прочитано;
- `POST /v1/push/history/read-all` — `{"updated":n}`;
- `POST /v1/push/history/remove` `{"id"}` и `POST /v1/push/history/clear`;
- `POST /v1/push/history` — внутренняя запись от `phone-push`
  (`{"deviceId","id","type","title","body","deepLink"}`); явный `id` делает
  повтор идемпотентным.

`type` — `notify|find`; заголовок ≤120, текст ≤500, deep link ≤512 символов.
На телефон хранится не больше 200 записей, старые вытесняются. Удаление
`push_devices`-строки (включая схлопывание по токену) удаляет историю этого
устройства. Каждое изменение истории публикует sync-пуш
`entity=notifications` только владельцу истории (`targetDeviceId`); если
изменение пришло с этого же телефона, пуш не отправляется, потому что у него
уже есть оптимистичное обновление. Устройство берётся из `Home-Device-Id`.

### VPN-профили (`/v1/vpn/*`)

Выдача WireGuard-доступа приложению — тот же base URL и bearer, что
`/v1/push/devices`. Телефон сам генерирует пару ключей и присылает только
публичный ключ; приватный ключ не покидает телефон.

- `POST /v1/vpn/profile` `{"deviceId","publicKey","deviceName"}` —
  идемпотентный upsert по `deviceId`: адрес из пула сохраняется, ротация
  ключа обновляет ту же строку; ключ другого устройства — `409`, неверный
  ключ/`deviceId` — `400`, нет настроек — `503`. Ответ — `{"profile":{...}}`
  с `wireguard.interface` (`address`, `dns`, `mtu`), `wireguard.peer`
  (`publicKey` сервера, `endpoint`, `lanEndpoint`, `allowedIps`,
  `persistentKeepalive`),
  `wireguard.splitAllowedIps` и запасным `ocserv` (`url`, `group`,
  `splitGroup`, `authentication`).
- `POST /v1/vpn/profile/remove` `{"deviceId"}` → `{"removed":bool}`.
- `GET /v1/vpn/peers` — для синхронизатора пиров на VPN-хосте, отдельный
  bearer `SCHEDULER_VPN_SYNC_TOKEN_FILE`; токены не взаимозаменяемы.

Настройки — JSON `SCHEDULER_VPN_SETTINGS_FILE`. Без файла или с невалидным файлом сервис стартует, а VPN-API
отвечает `503`. Необязательное поле `lanEndpoint` (`host:port` в домашней
сети) отдаётся телефону как адрес WireGuard внутри дома: роутер не делает
hairpin NAT для UDP, поэтому публичный `endpoint` из домашней Wi-Fi не
отвечает. Клиентский сертификат ocserv сервер не выпускает: приватный ключ
CA хранится только у владельца.

## Доступ приложения (модули)

Источник правды — SQLite scheduler (schema v10): `app_tokens` хранит только
sha256, имя, набор модулей и флаг owner; `access_meta` — монотонную ревизию.
Токен создаётся только через admin API (значение показывается один раз), а
старый общий токен при миграции импортируется как «Мама» (id `mama`) со
всеми модулями, кроме `admin`: решение владельца на время миграции, чтобы до
UI выдачи именованных токенов у существующих телефонов, HA и APK не было
регрессий. Owner-токен один (id `owner`), его нельзя отозвать и нельзя лишить
`admin`. Имена уникальны без учёта регистра (включая кириллицу); имя
отозванного токена освобождается. Активных токенов не больше 64 (`429`).

| Модуль | Эндпоинты |
|---|---|
| `base` | `SchedulerService/GetStatus`, `GET /v1/me`, `POST /v1/push/devices` (регистрация устройства) |
| `home` | `GET /v1/catalog` (ha-control); `EnqueueTask/CancelTask` только `queue=control`, `taskType=home.control` (остальное — owner/service) |
| `scenarios` | `ScenarioService/*` |
| `alarms_timers` | `AlarmService/*`, `SchedulerService/*` по расписаниям и таймерам |
| `intercom_call` | `IntercomService/*` |
| `notifications` | `GET /v1/push/devices`, `/v1/push/devices/{remove,rename}`, `/v1/push/history*` — только устройства своего токена; admin видит все |
| `chat` | `mcp-gateway /v1/chat/*` (кроме сервисного `/v1/chat/alerts`) |
| `vpn` | `POST /v1/vpn/profile`, `/v1/vpn/profile/remove` |
| `system` | `mcp-gateway /v1/system/summary|hosts|k8s|devices` |
| `infra_reports` | зарезервирован: app-эндпоинтов в PR1 нет |
| `alerts` | фильтр alert-сообщений в `/v1/chat/messages` и `/v1/chat/events` и push alert-карточек; `/v1/chat/alerts` закрыт отдельным токеном Alertmanager (`chat-alerts`) |
| `admin` | `GET /v1/access/modules`, `GET/POST /v1/access/tokens`, `PATCH/DELETE /v1/access/tokens/{id}` |

Любой действующий токен получает `base` неявно. Набор legacy-токена «Мама» —
все модули, кроме `admin` (см. выше). Owner — все модули. Запрос без модуля получает `403
{"error":"module_forbidden","module":"<id>"}`; без/с неверным токеном — `401`.

Admin API:

- `GET /v1/access/modules` — каталог `{id,title}`;
- `GET /v1/access/tokens` — активные токены (без хэшей);
- `POST /v1/access/tokens` `{name,modules}` → `{token,access}` (токен один раз);
- `PATCH /v1/access/tokens/{id}` `{name?,modules?}`;
- `DELETE /v1/access/tokens/{id}` — отзыв (owner запрещён);
- `GET /v1/me` — `{name,modules,owner}` для текущего токена.

Неизвестный id модуля в admin API — `400`; занятое имя и попытка выдать
`admin` — `409`; лимит токенов — `429`. `PATCH` применяет имя и модули в одной
транзакции. Отзыв удаляет push-устройства, их историю и VPN-peer'ы токена.
Модуль `scenarios` даёт создавать и запускать сценарии с любыми HA-действиями,
то есть по силе равен управлению домом; выдавать его стоит тем же, кому `home`.

Push-устройства привязаны к `token_id` регистрации (schema v9), VPN-peer'ы —
тоже (v10): list/rename/remove/history фильтруются по токену, чужой
`Home-Device-Id` → `404`, owner и service-token видят все. При старте
scheduler строки без владельца (созданные до v9/v10) идемпотентно
назначаются legacy-токену «Мама», поэтому текущие телефоны продолжают
обновлять FCM-токен без `409`. Регистрация атомарна (одна транзакция).
Устройство чужого токена можно перерегистрировать, только предъявив тот же
FCM-токен (та же установка приложения, например переход с «Мамы» на
именованный токен) или owner-токеном; иначе `409`. VPN-peer чужого токена
перехватывает только owner: публичный ключ WireGuard не секрет и владения не
доказывает. `GET /v1/push/devices?module=<id>` с неизвестным модулем —
`400`.

Внутренний `GET /v1/access/snapshot` (service-token) отдаёт
`{revision,tokens:[{id,name,sha256,owner,modules}]}`; без заголовка — `401`,
app-токеном — `403 service_only`. Потребители (`AccessClient` из
`home-app-auth`) обновляют snapshot раз в 30 с и по требованию при
неизвестном токене (не чаще раза в секунду, одновременные запросы ждут одно
обновление). Если успешного обновления не было час, кэш считается протухшим и
все app-запросы получают `401` (fail-closed): час переживает долгую раскатку
или перенос scheduler без отказа звонков и управления, а отзыв при живом
scheduler доходит за 30 с. Неизвестные модули в
snapshot игнорируются с `warn`. Scheduler публикует sync `entity=access`, но
Android пока его не обрабатывает (приходит во втором PR), поэтому телефон
увидит изменения модулей при следующем чтении `/v1/me`. `service_token` живёт в том же секрете; scheduler
не стартует, если service-токен (или `service_token_previous`) совпадает с
legacy app-токеном. Ротация — `admin:app-token-bootstrap --rotate-service`
(`docs/OPERATIONS.md`).

## Конфигурация

`SCHEDULER_*` и legacy fallback `ALARM_*`:

```text
SCHEDULER_DB_PATH=scheduler.db              (ALARM_DB_PATH)
SCHEDULER_LISTEN=127.0.0.1:8098             (ALARM_LISTEN; loopback default)
SCHEDULER_TOKEN=...                         (или SCHEDULER_TOKEN_FILE; legacy app token «Мама»)
SCHEDULER_APP_TOKENS_FILE=...               (optional seed owner-токена из OpenBao)
SCHEDULER_SERVICE_TOKEN_FILE=...            (обязателен, без fallback на legacy)
SCHEDULER_SERVICE_TOKEN_PREVIOUS_FILE=...   (optional, второй service-token на время ротации)
SCHEDULER_MISSED_GRACE_SECONDS=600
SCHEDULER_RING_WINDOW_SECONDS=900
SCHEDULER_EVENT_RETENTION_SECONDS=2592000
SCHEDULER_MAX_TIMERS=64
SCHEDULER_MAX_TIMER_SECONDS=604800
SCHEDULER_MAX_PAYLOAD_BYTES=16384
SCHEDULER_EVENT_MAX_ATTEMPTS=5
SCHEDULER_TICK_MS=1000
SCHEDULER_CONSUMERS=ha-bridge,android       (optional allowlist)
SCENARIO_EXECUTION=false                    (GetStatus.scenario_execution_supported)
SCHEDULER_VPN_SETTINGS_FILE=...             (optional, без него /v1/vpn/* → 503)
SCHEDULER_VPN_SYNC_TOKEN_FILE=...           (optional, bearer /v1/vpn/peers)
SCHEDULER_PUSH_URL=http://phone-push:8095   (optional, sync-пуши)
```

Sync-пуши: если задан `SCHEDULER_PUSH_URL`, после каждой мутации будильника,
напоминания, таймера и сценария (RPC и тик движка) scheduler фоном шлёт
`POST {url}/v1/push/sync` `{entity, op, id, originDeviceId}` с тем же
bearer-токеном. `entity` — `alarms|reminders|timers|scenarios` (по `kind`
расписания), `notifications` (после изменения истории) и `access` (после
изменения токенов), `op` —
`upsert|delete`. Необязательный `targetDeviceId` адресует пуш одному
устройству (для истории); `originDeviceId` по-прежнему исключает инициатора. `originDeviceId` берётся из заголовка
`Home-Device-Id` запроса; phone-push этому устройству пуш не шлёт. Ошибка
доставки только логируется и не влияет на ответ RPC.

Токен обязателен: без него процесс не стартует; `SCHEDULER_TOKEN` побеждает
`SCHEDULER_TOKEN_FILE`. Временная зона из env не читается — она приходит в
заголовке `Time-Zone` каждого запроса. `-healthcheck` и `-status` читают токен
из файла и не кладут его в аргументы. Значения секретов не
логируются. Для минимальных хостов IANA tzdata встроена в бинарник.

## Сборка и проверки

```bash
cargo test -p scheduler
cargo clippy -p scheduler --all-targets -- -D warnings

bazel test //solutions/scheduler/... //packages/proto/...
```

`proto_library` в `packages/proto/home/v1` остаётся канонической схемой;
Rust-стабы перегенерируются `packages/rust/home-proto/generate.sh`.

## AlarmService facade

Все 8 RPC `AlarmService` реализованы поверх того же store
(`kind=SCHEDULE_KIND_ALARM`). `Alarm.revision` (поле 12, additive) даёт
optimistic concurrency для `UpdateAlarm`. `last_ring_unix` означает
«последний emission», а не «прозвучал». Audibility доказывается только
executor-ом и не гарантируется этим сервисом.

## Ограничения

- `ha-scenario` исполняет очередь `scenario`; event-триггеры сценариев
  (кроме manual/time) не поддерживаются;
- нет recurring generic tasks (`SCHEDULE_KIND_TASK`);
- нет streaming `WatchEvents`;
- SQLite рассчитан на один процесс; горизонтальное масштабирование
  потребует другого storage/broker;
- звук, громкость, рингтон и restore source остаются у HA/Pulsar.
