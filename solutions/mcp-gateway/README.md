# mcp-gateway

Один расширяемый MCP-шлюз для инфраструктуры: read-only диагностика,
подтверждаемый перезапуск и отправка отчётов. Шлюз не знает, где он запущен:
все endpoint'ы, пространства имён, каталог сервисов и секреты приходят из
окружения или смонтированных файлов.

```
MCP client (Hermes, IDE, agent)          voice proxy
        │  Streamable HTTP + Bearer        │  HTTP + voice Bearer
        ▼                                  ▼
   mcp-gateway ── infra.read   → Prometheus / Alertmanager / Loki / Kubernetes REST
        │      ── infra.fix    → rollout restart (единственная мутация)
        │      ── infra.report → Telegram Bot API (фиксированный получатель)
        │      ── bb.read      → bb home-dispatcher: status тредов (личный профиль)
        │      ── bb.act       → bb home-dispatcher: prepare/confirm/cancel spawn
        │      ── /v1/voice/*  → маршрутизация речи: Hermes, рабочий тред, агент
        ▼
  /healthz  /readyz  /metrics

  mcp-gateway ── SSH (одна сессия на вызов, JSON в stdin) ──► forced command
       на bb-хосте ──► HTTP loopback ──► плагин bb home-dispatcher
```

Шлюз не знает plugin-token и HMAC-секрет bb: их держит мост на хосте bb.
Шлюз владеет только ограниченным ed25519-ключом и pinned `known_hosts`.

## Транспорт и аутентификация

- MCP Streamable HTTP: `POST /mcp` (Rust SDK `rmcp`; только серверный
  транспорт, без клиентских фич).
- `Authorization: Bearer <token>`. Токен MCP читается из `MCP_AUTH_TOKEN_FILE`,
  сравнивается в постоянное время. Без токена — `401`.
- `/v1/voice/*` защищён **отдельным** токеном `MCP_VOICE_TOKEN_FILE`: голосовой
  токен не даёт `/mcp`, а MCP-токен не даёт voice-маршруты. Оба сравниваются в
  постоянное время и обязаны различаться (проверяется при старте).
- `/healthz`, `/readyz`, `/metrics` не требуют токена и предназначены только
  для кластерной сети (ServiceMonitor и kubelet probes). Наружу не публиковать.
- `-healthcheck` — самопроверка без shell (distroless): читает конфигурацию и
  запрашивает `/healthz` по loopback-адресу.

### Уведомление об изменении списка инструментов

После инициализации каждой MCP-сессии шлюз один раз отправляет
`notifications/tools/list_changed` (`tools.listChanged` в `initialize`):
Hermes при переподключении не перезаписывает свой реестр инструментов, пока
сервер молчит, поэтому без уведомления новый набор не увидит и Hermes пришлось
бы перезапускать. Уведомление, отправленное до открытия standalone GET-потока,
буферизуется SDK и проигрывается при подключении; повторов нет, ошибка только
логируется.

## Группы инструментов

Группы включаются через `MCP_GROUPS` (CSV). Неизвестная группа — ошибка
запуска. Реестр расширяемый: новая группа реализует трейт `ToolGroup`
(`tools/mod.rs`) и регистрируется в `app`, без изменений транспорта и auth.

### `infra.read` (все `readOnlyHint=true`)

| Инструмент | Что делает |
|---|---|
| `infra_status()` | Один агрегированный вызов: активные алерты Alertmanager, сигналы каталога, доступность workload'ов и перезапуски подов за час. Параллельные запросы с таймаутами; общий бюджет `MCP_STATUS_TIMEOUT` (8 с). |
| `infra_service(name)` | Разрешает spoken-имя (регистр и ё/е не важны, лёгкая нечёткость) и отдаёт workload, поды, перезапуски, события, сигналы и связанные алерты. |
| `infra_alert(ref)` | Объясняет один активный алерт по fingerprint Alertmanager (полному или префиксу от 6 символов) либо по имени. Возвращает labels/annotations и detail связанного сервиса из каталога (сопоставление по namespace/deployment/pod/endpoint/job и алиасам), чтобы можно было ответить на уточнение «а что с этим алертом?». |
| `infra_logs(name, since, grep, limit)` | Логи сервиса: Loki, если он там есть, иначе `pods/log` (все контейнеры каталога или выбранные). Секреты редактируются. Размер ограничен `MCP_LOG_BYTE_CAP`. |
| `infra_query(promql)` | Один instant-запрос PromQL, только чтение, результат ограничен. |
| `infra_overview()` | Топология и каталог для контекста модели. |
| `infra_report(verbose)` | Полный отчёт по всем сервисам каталога: статус, готовые реплики, перезапуски за час, образ (digest сокращён), возраст, внешние проверки, версии HA и активные алерты. Заголовок отчёта печатается в зоне `MCP_TIMEZONE`. Возвращает короткий вердикт и компактный plain-text отчёт (≤ 3500 символов) для Telegram: проблемы первыми, затем «В норме» по группам каталога. Образ выбирается по `containers[0]`, затем по имени workload/суффиксу, затем по порядку контейнеров в поде — сайдкар не подменяет основной контейнер. Две кластерные проверки добавляются в группу «Платформа»: **Flux** (каждый Kustomization и HelmRelease должен быть `Ready=True`; `suspend: true` показывается числом, но не считается проблемой, а причина и сообщение условия Ready попадают в текст; `Ready=Unknown` с причиной `Progressing` — штатный reconcile, он показывается как «обновляется N» и не считается проблемой, пока не истекло окно 15 минут с `lastTransitionTime` — то же `for:`, что у алертов `FluxKustomizationNotReady`/`HelmReleaseNotReady`) и **Каталог отчёта** (любой workload вне каталога — проблема «не в каталоге отчёта», см. `ignore_workloads`). `verbose: true` добавляет секцию «Версии» (образ, возраст, версии); по умолчанию их нет в тексте, но они есть в `details` JSON. |
| ресурс `infra://catalog` | То же в виде markdown-ресурса. |

### `infra.fix` (`destructiveHint=true`)

| Инструмент | Что делает |
|---|---|
| `infra_prepare_restart(name)` | Проверяет `restart.allowed`, наличие workload'а, отсутствие текущего rollout и cooldown. Кладёт **одно** отложенное действие (TTL 90 с, заменяет предыдущее) и возвращает короткий русский вопрос с предупреждением каталога. |
| `infra_confirm()` | Выполняет отложенное действие. Аргументов нет — работает, даже если клиент потерял историю. Rollout restart = strategic-merge patch аннотации `kubectl.kubernetes.io/restartedAt` плюс `mcp-gateway/last-restart` для cooldown. Для `self_disrupting` патч откладывается на `MCP_SELF_RESTART_DELAY` (20 с), чтобы ответ успели озвучить. |
| `infra_cancel()` | Удаляет отложенное действие. |

Единственный глагол — rollout restart. Нет delete/scale/exec/apply и нет
перезапуска внешних (не Kubernetes) сервисов.

### `infra.report`

| Инструмент | Что делает |
|---|---|
| `infra_send_report(title, text)` | Отправляет отчёт в Telegram: HTML-экранирование, разбиение по 4096 символов, редактирование секретов. Получатель фиксирован конфигурацией; модель не может его выбрать. |

### `bb.read` и `bb.act` (Hermes — только личный профиль)

| Инструмент | Что делает |
|---|---|
| `bb_status()` | Короткая сводка работающих тредов. Возвращает только треды профиля `personal`; рабочие треды не попадают ни в `speech`, ни в `details`. |
| `bb_prepare_spawn(project, model?, effort?, task)` | Резолвит проект по имени/алиасу **только среди `targets` с профилем `personal`** из allowlist `bb-targets.yaml`. Провайдер по умолчанию `claude-code`; модель и reasoning-уровень берутся из живого ответа плагина и проверяются им же. Вызывает `validate_spawn`, кладёт **один** pending на 90 с (ключ `bb.spawn`, отдельно от restart) и возвращает вопрос «Запустить агента … . Подтверждаете?» — последним предложением, чтобы колонка продолжала слушать. |
| `bb_confirm()` | Забирает pending и вызывает `spawn` с сохранённым `idempotencyKey`, затем включает режим агента на созданный тред. Аргументов нет. |
| `bb_cancel()` | Удаляет pending без запуска. |

Рабочий проект для Hermes неотличим от несуществующего: тот же ответ «не
нашёл», без кода `profile_mismatch` и без имени проекта. Запуск всегда
двухшаговый; свободный текст проекта не принимается.

### `time` (Hermes: будильники, таймеры, напоминания)

Группа говорит с durable `home.v1.SchedulerService` тем же Connect JSON API,
что Home Assistant и Android; звук по-прежнему проигрывает `ha-bridge` через
`script.alarm_start` / `schedule_announce` / `alarm_stop`. В HA не осталось
шаблонов времени: фразы маршрутизируются в Hermes, а Hermes зовёт эти
инструменты.

| Инструмент | Что делает |
|---|---|
| `time_now()` | Текущее локальное время дома (`MCP_TIMEZONE`), RFC3339 с офсетом, человеческая строка и IANA-пояс. Первый вызов для любой фразы про время. `alarm_*`/`reminder_*` пишут в scheduler с заголовком `Time-Zone: MCP_TIMEZONE`; сервер хранит UTC, поэтому шлюз сам время не пересчитывает. |
| `alarm_set(at, label?, repeat?)` | Абсолютное время RFC3339 **с офсетом**. `repeat`: `once`/`daily`/`weekdays`/`weekends`. Однократный — не дальше 7 дней, прошедшее время отклоняется без создания. Создаёт тот же `CreateSchedule`, что HA: `kind=SCHEDULE_KIND_ALARM`, `payloadJson="{}"`, метка по умолчанию «Будильник HH:MM». |
| `alarm_list(include_disabled?)` | Ближайшее срабатывание, метка, повтор; id для остальных операций. |
| `alarm_cancel(id?, all?)` | Выключить (`SetScheduleEnabled=false`), не удаляя. |
| `alarm_delete(id?, all?)` | Удалить навсегда. |
| `alarm_update(id, at)` | Перенести на новое абсолютное время, сохранив повтор и метку; `expectedRevision` защищает от гонки. |
| `alarm_snooze(id?)` / `alarm_stop(id?)` | Отложить на 9 минут / остановить; без id ищут активный звонок. |
| `timer_set(seconds, label?)` | Длительность 1…604800 с; 0, отрицательное и больше 7 дней отклоняются. |
| `timer_list(include_finished?)` | Остаток и состояние (идёт/пауза). |
| `timer_cancel(id?, all?)`, `timer_pause(id?)`, `timer_resume(id?)` | Управление таймером; без id работает, если активен ровно один. |
| `reminder_set(at, text)` / `reminder_list(include_disabled?)` | Однократные напоминания (`SCHEDULE_KIND_REMINDER`). |

Ответ всегда содержит итоговое локальное время или длительность человеческим
текстом для озвучки; ошибки планировщика озвучиваются короткой фразой, без
технических деталей. Токен — `MCP_SCHEDULER_SERVICE_TOKEN_FILE`
(`k8s/home-assistant/scheduler-auth`), URL — `MCP_SCHEDULER_URL`.

### `phone` (Hermes: найти телефон, уведомление, список телефонов)

| Инструмент | Что делает |
|---|---|
| `phone_find(reason?, name?)` | «Где телефон», «найди телефон»: `solutions/phone-push` отправляет **data-only** FCM-сообщение высокого приоритета без `notification`-блока. Приложение запускает foreground-сервис: громкий звонок через `USAGE_ALARM` в обход беззвучного режима и DND, вибрация, экран поверх блокировки с кнопкой «Нашёл», автостоп через 60 с. Поведение одинаково при выключенном экране, в фоне и в убитом приложении: high-priority FCM разрешает старт FGS, а трей не рисуется. |
| `phone_notify(title, body, deep_link?, name?)` | Обычное уведомление: заголовок, текст и необязательный `deep_link` (`home://…`), канал `home_notifications`. Подписи кнопок и каналов — из ресурсов приложения; сервер присылает только текст. |
| `phone_list()` | Имена и идентификаторы зарегистрированных телефонов. Read-only. |

`name` — имя телефона дословно, как его произнёс владелец. Gateway сам
сопоставляет русские падежи и уменьшительные («Стас», «Стаса», «телефон
Стаса», «Стасику»): он читает `GET /v1/push/devices` у `phone-push`, находит
единственное совпадение и передаёт в отправку `deviceId`. Если имя не
названо и телефон один, вопрос не задаётся. Названное имя всегда должно
совпасть: телефон другого человека не звонит даже тогда, когда он
единственный зарегистрированный, — речь говорит, что телефона с этим именем
нет, перечисляет зарегистрированные и подсказывает указать имя телефона в
настройках уведомлений приложения. Если телефонов несколько и имя не названо
или подходит к нескольким, инструмент возвращает `details` со списком имён и
речь-вопрос; Hermes задаёт ровно один короткий вопрос и не угадывает. После
отправки речь называет телефон, который ответил.

Ответ отправки — `{device:{id,name}, sent, failed, devices}`: токены
устройств не попадают ни в речь, ни в `details`. `phone_find` и
`phone_notify` — `destructiveHint` (действуют на реальное устройство);
`phone_list` — `readOnlyHint`.

Токены хранит scheduler (`POST/GET /v1/push/devices`, схема v4, ключ —
`deviceId`), а FCM service account — единственный секрет `phone-push`;
gateway и Hermes его не видят. `MCP_GROUPS` должен включать `phone`.

### Голосовой вход (`/v1/voice/*`)

Отдельный Bearer-токен, только для voice proxy в том же namespace.

| Маршрут | Назначение |
|---|---|
| `POST /v1/voice/utterance {text, profile}` | Маршрутизация одной фразы. |
| `POST /v1/voice/result {threadId, afterSeq, waitSeconds}` | Long-poll `turn_result` (≤25 с), ответ `{status, speech}` или `{status:"running"}`. |
| `POST /v1/voice/updates {afterSeq?, waitSeconds?}` | Обновления тредa-диспетчера: завершённые ходы с `initiator` `system`/`agent`, ответ `{cursor, items:[{seq, spoken}]}`. Без `afterSeq` — только `cursor` для bootstrap. Один вызов bb ограничен 10 с (ниже SSH-таймаута); прокси продолжает с полученным `cursor`. |
| `GET /v1/voice/state` | `{agentMode, threadId?}` для отладки. |

Маршруты utterance, по порядку:

1. фраза выхода («выйди из режима агента», «выключи режим агента», «закончи
   разговор с агентом») — целиком, не подстрокой — выключает режим и отвечает
   `{route:"local", speech:"Выключил режим агента."}`; прокси не форвардит её;
2. `profile == work` → `send` в постоянный рабочий тред
   (`MCP_BB_DISPATCHER_THREAD_ID`), `speech:"Передал в рабочий тред."` — даже
   если режим агента включён; режим при этом не сбрасывается и продолжит
   действовать при возврате в личный профиль;
3. `profile == personal` и включён режим агента → `send` в активный тред,
   `{route:"bb", threadId, afterSeq, speech:"Передал агенту."}`;
4. иначе → `{route:"hermes"}` (proxy сам зовёт Hermes).

Ошибка bb всегда остаётся на `route:"bb"` с `ok:false` и короткой фразой.
Для `profile == work` возврат `hermes` при ошибке запрещён (fail-closed), как и
работа без настроенного рабочего треда. Режим агента — только возможность
личного профиля; фраза выхода из него работает в любом профиле. Состояние
режима — in-memory, один активный тред, исчезает при рестарте (это routing
hint, не конфигурация).
Текст ограничен 2000 символами; действует rate-limit; в лог пишутся маршрут,
профиль, id треда и длина текста, **никогда** сам текст речи.

## Дашборд системы (`/v1/system/*`)

Read-only агрегатор для вкладки «Система» Android-приложения. Отдельного
сервиса нет: шлюз уже читает Prometheus, Alertmanager и реестр push, а
дашборд — это ещё одно представление тех же источников. Код —
`crates/mcp-gateway/src/system/`.

- Аутентификация: `Authorization: Bearer <токен приложения>`; решение
  принимает общий `home-app-auth` по snapshot scheduler (`/v1/access/snapshot`,
  кэш 30 с). `/v1/system/*` требует модуль `system` (по умолчанию только
  owner), `/v1/chat/*` — `chat`. MCP- и voice-токены здесь отклоняются
  (`401`, realm `home-app`). Маршруты включает `MCP_APP_API=true`; snapshot
  читается service-токеном `MCP_SCHEDULER_SERVICE_TOKEN_FILE`.
- Push из чата: alert-карточки уходят с `module=alerts`, ответы ассистента —
  с `module=chat`; scheduler доставляет их только устройствам токенов с этим
  модулем (и owner).
- Внешний маршрут (только `/v1/system/` и `/v1/chat/`, без `/mcp`) — в
  [README деплоя](../../projects/home/kubernetes/mcp-gateway/README.md).
- Кэш секции 15 с (`CACHE_TTL`); параллельные запросы делят одно обновление.
  Таймаут одного запроса к Prometheus/реестру 5 с, всей секции 8 с.
- Ошибка или отсутствие источника → секция `unknown` («нет данных»), HTTP
  остаётся `200`. Уровни: `ok < unknown < warning < critical`. Общий статус —
  худший из измеренных секций и алертов (`severity` `critical`/`warning`);
  `Watchdog`, `InfoInhibitor` и `severity=none` игнорируются.

| Маршрут | Источник | Содержимое |
|---|---|---|
| `GET /v1/system/summary` | все секции + Alertmanager | `status`, `generatedAt`, `sections{host,guests,k8s,devices}`, `alerts[]`, `alertsAvailable` |
| `GET /v1/system/hosts` | node exporter PVE (`job="pve-node"`), prometheus-pve-exporter (`job="pve-guests"`) | хост: CPU, ОЗУ, диск `/`, max coretemp, аптайм; гости: VMID, имя, VM/CT, running, onboot, CPU/ОЗУ/диск/аптайм. Остановленный гость с автозапуском — `critical`, проблемные первыми |
| `GET /v1/system/k8s` | kube-state-metrics + cAdvisor | неймспейсы → поды: phase, ready/containers, рестарты (всего и за час), waiting reason, CPU, working set, возраст. `Succeeded` скрыты; проблемные неймспейсы и поды первыми |
| `GET /v1/system/devices` | экспортёр Pulsar (`job="station"`), `phone-push` | колонка: up, версия/модель из `pulsar_info`, задержка = `scrape_duration_seconds`, Wi-Fi RSSI (`pulsar_wifi_rssi_dbm`, если экспортируется), температура, аптайм; телефоны из реестра push (без токенов) |

Числа — JSON numbers; неизвестное значение опускается, а не превращается в `0`.
Эталонные ответы — `crates/mcp-gateway/src/system/testdata/contract/*.json`
(их же читает Android-тест `core/network`); пересборка эталонов —
`UPDATE_SYSTEM_GOLDEN=1 cargo test -p mcp-gateway --lib system`, затем
скопировать файлы в `projects/mobile/android/core/network/src/test/resources/fixtures/system/`.
Входные ответы Prometheus — `src/system/testdata/{hosts,guests,k8s,station}.json`.

## Чат приложения (`/v1/chat/*`)

Вкладка «Чат» Android-приложения — тот же агент Hermes, что в Telegram
(те же память, skills и MCP-инструменты), в нескольких беседах (сессиях)
приложения. Код — `crates/mcp-gateway/src/chat/`.

```
Android ──/v1/chat/*──► mcp-gateway ──► chat relay (voice-proxy) ──► Hermes /api/sessions/<session>[/chat/stream]
                            │ SQLite MCP_CHAT_DB
```

- Аутентификация — app-токен с модулем `chat`; alert-сообщения
  (`kind=alert`) в `/v1/chat/messages` и `/v1/chat/events` отдаются только
  токену с модулем `alerts`.
- Hermes API слушает только loopback пода Hermes. Gateway ходит к нему через
  chat relay в `voice-proxy` (отдельный Bearer), который пропускает только
  сессии приложения (`home-app` и `app_*`) и подставляет `API_SERVER_KEY` —
  см. [voice-proxy](../voice-proxy/README.md#app-chat-relay).
- Беседы — таблица `chat_sessions` (id, название, закреп, архив); каждое
  сообщение несёт `sessionId`. Сессия Hermes с тем же id создаётся при первом
  ходе (`409` = уже есть). Новые сессии получают id `app_<uuid>`. Прежняя
  единственная беседа (`MCP_CHAT_SESSION`, по умолчанию `home-app`) при старте
  становится сессией «Главный чат»: сообщения без `session_id` переносятся в
  неё; алерты пишутся туда же.
- Переименование зеркалируется в Hermes (`PATCH {title}`), удаление — `DELETE`
  (лучшее усилие, ошибки только в лог). Закреп и архив локальные: архив в
  Hermes запускает удаление по retention, поэтому туда не передаётся.
- Название: если не задано, после первого ответа берётся автозаголовок Hermes
  (`GET /api/sessions/<id>`), иначе первая строка первого сообщения (40
  символов). Изменение уходит SSE-событием `session`.
- Остановка: `POST /v1/chat/messages/{id}/stop` обрывает поток к Hermes
  (Hermes прерывает run при закрытии SSE); ответ становится `failed` с
  `chat_stopped` и частичным текстом, его можно повторить через `retry`.
- Инструменты: события Hermes `tool.started/completed/failed` идут в SSE как
  `tool {id, sessionId, name, done}` без сохранения; `_thinking` скрыт.
- Персона и формат — `system_message` из
  `MCP_CHAT_PROMPT_FILE`. Память и skills Hermes общие для
  всех платформ, поэтому факты из Telegram доступны и здесь.
- История — SQLite `MCP_CHAT_DB` с одним писателем (одна реплика). Hermes хранит контекст агента сам; таблица
  шлюза — то, что видит приложение: id, `clientId` для идемпотентности,
  цитата, статус, ключ ошибки. Поэтому история переживает переустановку и
  второй телефон.
- Ходы выполняются строго по одному (очередь в памяти + статус `pending` в
  базе). После рестарта `streaming` → `failed` (`chat_interrupted`), `pending`
  ставятся в очередь заново. Поток Hermes держит шлюз, а не телефон: закрытое
  приложение не прерывает ответ.

| Маршрут | Назначение |
|---|---|
| `GET /v1/chat/sessions?archived=<bool>` | Сессии: закреплённые, затем по последней активности; `preview` — последнее сообщение (роль, вид, текст до 120 символов, статус). |
| `POST /v1/chat/sessions {title?}` | Новая сессия `app_<uuid>`. |
| `PATCH /v1/chat/sessions/{id} {title?, pinned?, archived?}` | Название 1–80 символов, иначе `400 chat_title_invalid`. |
| `DELETE /v1/chat/sessions/{id}` | Сессия, её сообщения и файлы вложений; идущий ход останавливается. SSE `session_removed`. |
| `POST /v1/chat/messages/{id}/stop` | Остановить `pending`/`streaming` ответ, иначе `409 chat_not_stoppable`. |
| `GET /v1/chat/messages?session=<id>&before=<seq>&limit=<n>` | Без `session` — `MCP_CHAT_SESSION`; неизвестная — `404 chat_session_not_found`. | Страница истории (по умолчанию 50, максимум 200) в порядке возрастания `seq`; `hasMore`, `cursor` — текущая версия для `events`. |
| `POST /v1/chat/messages {clientId, sessionId?, text, replyTo?}` | Сообщение и плейсхолдер ответа `pending`. Повтор с тем же `clientId` возвращает ту же пару без нового хода — офлайн-очередь телефона повторяет безопасно. `replyTo` — id цитируемого сообщения; Hermes получает цитату строкой «[Ответ на сообщение (role): «…»]». |
| `POST /v1/chat/messages/{id}/retry` | Повтор упавшего ответа (`assistant` + `failed`), иначе `409 chat_not_retryable`. |
| `GET /v1/chat/events?after=<version>&session=<id>` | SSE; с `session` — только события этой сессии, без — все (экран списка слушает `message`, `session`, `session_removed`). Сначала все сообщения с `version > after`, затем живые события: `message` (полная версия сообщения, `id:` = `version`) и `delta` `{id, offset, text}`. `offset` — длина уже показанного текста в UTF-16 (как `String.length` в Kotlin): клиент применяет дельту, только если она продолжает его текст, иначе ждёт следующего `message`. Keepalive 15 с; при отставании подписчика поток закрывается, клиент переподключается со своим `after`. |

### Голос и вложения

| Маршрут | Назначение |
|---|---|
| `POST /v1/chat/attachments?name=<имя>` | Тело — сырые байты, тип — `Content-Type`. Ответ — `{id, kind, mime, size, name, durationMs?}`. Лимит тела 6 МБ + запас. |
| `GET /v1/chat/attachments/{id}` | Байты вложения с исходным `Content-Type` (кэшируется телефоном). |

- `kind=voice` — только `audio/wav` PCM 16 кГц, моно, 16 бит (приложение пишет
  его само), до 4 МБ (~2 мин); иначе `chat_voice_format`. Шлюз не
  перекодирует звук: те же байты уходят в Wyoming.
- `kind=image` — `image/jpeg|png|webp` (проверяется сигнатура), до 6 МБ;
  Hermes получает их тем же путём, что фото из Telegram: частью
  `image_url` (data URL) мультимодального сообщения, подпись — текстом.
- `kind=file` — UTF-8 текст (`text/plain|markdown|csv|x-log`,
  `application/json`) до 256 КБ; встраивается в сообщение блоком
  «[Файл имя]» с кодом. Остальное — `chat_attachment_unsupported`
  (sessions API Hermes не принимает файловые части).
- В `POST /v1/chat/messages` вложения передаются `attachments: [id]` (до 4;
  голосовое — только одно и без других вложений); текст при вложениях может
  быть пустым. `kind` сообщения: `voice`, `image`, `file` или `text`.
- Голосовое распознаёт Wyoming STT из `MCP_CHAT_STT_ADDRS` (по очереди,
  таймаут 60 с). Расшифровка сохраняется в поле `transcript`
  сообщения пользователя (событие `message` с новой версией, приложение
  показывает её под голосовым), Hermes получает «[Голосовое сообщение]:
  <текст>». Ошибка/пустая речь — ответ `failed` с `chat_stt_failed`
  (повтор перезапускает распознавание).
- Файлы — в `MCP_CHAT_MEDIA_DIR` (`<id>.bin` + `<id>.json`).

### Алерты и push

- Alertmanager шлёт тот же набор алертов, что в Telegram (маршрут `telegram`,
  `warning|critical`), ещё и вебхуком `POST /v1/chat/alerts` (`send_resolved`,
  отдельный Bearer `MCP_CHAT_ALERTS_TOKEN_FILE`, realm `home-alerts`;
  OpenBao `k8s/home-assistant/chat-alerts`). Каждый алерт — сообщение
  `role=system`, `kind=alert` с полем `alert` (`fingerprint`, `name`,
  `severity`, `status` `firing|resolved`, `summary`, `description`,
  `namespace`, `object`, `startsAt`, `endsAt?`, `runbookUrl`) и текстом как в
  Telegram, включая «🔎 <12 символов отпечатка>». Ключ — отпечаток + `startsAt`:
  повтор группы не плодит карточки, `resolved` обновляет ту же карточку новой
  версией, повторное срабатывание после решения — новая карточка.
- Ответ на алерт в приложении — обычный `replyTo`: Hermes получает цитату с
  отпечатком и, как в Telegram, разбирает его через `infra_alert`.
- Push через `phone-push` (`MCP_PUSH_URL`, группа `phone`), deep link
  `home://chat/message/<id>`: один раз на новый алерт (`firing`) и один раз на
  готовый ответ Рикардо. Решение алерта и сообщения пользователя push не
  шлют; приложение само не показывает уведомление чата, пока вкладка «Чат» открыта.

Сообщение: `id`, `seq` (порядок в ленте), `version` (монотонная версия
изменения), `clientId?`, `role` (`user`/`assistant`/`system`), `kind`
(`text`/`voice`/`image`/`file`/`alert`), `text` (Markdown), `replyTo? {id, role, text}`
(текст цитаты обрезан до 200 символов), `status`
(`pending`/`streaming`/`done`/`failed`), `error?` — ключ ресурса приложения
(`chat_text_empty`, `chat_text_too_long`, `chat_client_id_invalid`,
`chat_reply_not_found`, `chat_not_found`, `chat_not_retryable`,
`chat_upstream_unavailable`, `chat_upstream_failed`, `chat_interrupted`,
`chat_storage_failed`), `createdAt`/`updatedAt` — unix ms. Ошибки запросов —
`{"code": "<ключ>"}` с `400/404/409/503` (поле `code` — как у Connect-ошибок,
поэтому общий парсер приложения отдаёт ключ без отдельного формата). Сервер не шлёт пользовательских
строк: тексты ошибок — ресурсы приложения.

Текст пользователя ≤ 4000 символов, `clientId` — 1–64 символа
`[A-Za-z0-9_-]`. Во время генерации текст сохраняется в базу каждые 2 с, а
снимок для нового подписчика берёт актуальный текст из памяти.

Эталоны — `crates/mcp-gateway/src/chat/testdata/contract/*.json`
(`send`, `page`, `events`, `error`, `attachment`, `voice`, `alert`); их же читает Android-тест
`core/network` (`ChatClientTest`: `events.json` собирается в SSE-поток). Пересборка: `UPDATE_CHAT_GOLDEN=1 cargo test -p mcp-gateway
--lib chat`, затем скопировать в
`projects/mobile/android/core/network/src/test/resources/fixtures/chat/`.

## Панель Hermes (`/v1/hermes/*`)

Read-only экран «Hermes» Android-приложения (вход строкой на вкладке
«Система»). Модуль `chat` и owner, как у чата; маршруты включаются вместе с
чатом (`MCP_APP_API=true` и `MCP_CHAT_HERMES_URL`). Код —
`crates/mcp-gateway/src/hermes/`. Мутаций нет, поэтому аудита нет; ошибка
источника не даёт `5xx`, а выставляет `available=false`.

| Маршрут | Источник | Содержимое |
|---|---|---|
| `GET /v1/hermes/agents` | bb-диспетчер, метод `status` (limit 20) | треды личного профиля: `id`, `title`, `project`, `status` (как в bb), `runtimeSeconds`; `available` |
| `GET /v1/hermes/reports` | relay → Hermes `GET /api/sessions?source=telegram`, 5 последних сессий → `GET /api/sessions/<id>/messages` | ответы на `/report`: сообщение пользователя, которое начинается с маркера вызова скилла `report` (или `/report`), и последний непустой ответ ассистента до следующего сообщения; `requestedAt`, `completedAt`, `text` (через `redact`), `failed` |
| `GET /v1/hermes/voice` | relay `GET /v1/health`; сессии `source=api_server` с id `api-…` (голосовые ходы HA без `X-Hermes-Session-Id`), 10 последних | `relayUp` (ответил voice-proxy), `hermesUp`/`hermesVersion`, `agentMode` (голосовой режим агента шлюза), `requests[]`: `at`, `text`, `reply`, `latencyMs` (между временем сообщения пользователя и ответа), `failed` |

Чего нет и почему:

- **Отмена агента.** У bb-диспетчера нет метода остановки треда (семь
  операций, см. `solutions/bb-home-dispatcher/README.md`). Нужен новый метод
  `cancel` в плагине и в контракте `contract/params.json`, плюс аудит в шлюзе.
- **Отчёт bb-агента.** `turn_result` требует `afterSeq` конкретного хода, а
  `status` его не отдаёт. Нужен метод «последний завершённый ход треда».
- **Ошибки голосового пути до Hermes.** Отказы роутера voice-proxy (профиль,
  шлюз, отмена) не сохраняются; видны только ходы, дошедшие до Hermes. Нужен
  журнал событий voice-proxy или метрики с разбивкой по исходу.

## Контракт ответа

Каждый инструмент возвращает JSON:

```json
{
  "speech": "Планировщик работает: готовы 1 из 1, подов 1.",
  "details": { }
}
```

- `speech` — одна строка на русском, без markdown и URL, не длиннее 160
  символов. Это то, что озвучивает голосовой путь.
- `details` — структурированные данные для модели.

## Переменные окружения

Секреты — только через файлы (`*_FILE`), значения в env не читаются.

| Переменная | Обязательна | По умолчанию | Назначение |
|---|---|---|---|
| `MCP_AUTH_TOKEN_FILE` | да | — | Bearer-токен для `/mcp`. |
| `MCP_GROUPS` | нет | `infra.read` | Включённые группы: `infra.read`, `infra.fix`, `infra.report`, `bb.read`, `bb.act`, `time`, `phone`. |
| `MCP_INFRA_CATALOG_FILE` | для read/fix | — | YAML-каталог сервисов. |
| `MCP_HTTP_ADDR` | нет | `:8080` | Адрес HTTP-сервера. |
| `MCP_TIMEZONE` | нет | `Asia/Almaty` | IANA-зона дома: заголовок `Time-Zone` для scheduler и человекочитаемое время в текстах инструментов. Проверяется при загрузке; для distroless-образа IANA tzdata встроена в бинарник. |
| `MCP_PROMETHEUS_URL` | нет | — | База Prometheus API, например `http://prometheus.example:9090`. |
| `MCP_ALERTMANAGER_URL` | нет | — | База Alertmanager, без `/api/v2`. |
| `MCP_LOKI_URL` | нет | — | База Loki, например `http://loki.example:3100`. |
| `MCP_HA_URL` | нет | — | База Home Assistant REST для `ha_*`-проверок каталога. Вместе с токеном; если не задана, проверки честно сообщают «не настроено». |
| `MCP_HA_TOKEN_FILE` | нет | — | Файл long-lived токена Home Assistant (Authorization: Bearer). |
| `MCP_REPORT_TIMEOUT` | нет | `9s` | Общий бюджет `infra_report` (все проверки параллельно). |
| `KUBERNETES_API_URL` | нет | in-cluster | Переопределение API-сервера (тесты, внешний кластер). |
| `KUBERNETES_TOKEN_FILE` | нет | `/var/run/secrets/kubernetes.io/serviceaccount/token` | ServiceAccount-токен; перечитывается на каждый запрос. |
| `KUBERNETES_CA_FILE` | нет | `/var/run/secrets/kubernetes.io/serviceaccount/ca.crt` | CA кластера. |
| `TELEGRAM_API_URL` | нет | `https://api.telegram.org` | База Bot API. |
| `TELEGRAM_BOT_TOKEN_FILE` | для report | — | Токен бота. |
| `TELEGRAM_CHAT_ID_FILE` | для report | — | Chat id получателя; если в файле список через запятую, берётся первый элемент. |
| `MCP_BB_SSH_ADDR` | для bb/voice | — | `host:port` моста bb (forced command). |
| `MCP_BB_SSH_USER` | нет | `bb` | Пользователь ограниченного ключа. |
| `MCP_BB_SSH_KEY_FILE` | для bb/voice | — | Файл ed25519-ключа; ключ принимается только forced command'ом. |
| `MCP_BB_KNOWN_HOSTS_FILE` | для bb/voice | — | Pinned host key bb-хоста. |
| `MCP_BB_TIMEOUT` | нет | `15s` | Таймаут одного SSH-вызова (1s–60s). |
| `MCP_BB_TARGETS_FILE` | для `bb.act` | — | Allowlist личных проектов и spoken-алиасов. |
| `MCP_BB_DISPATCHER_THREAD_ID` | для voice | — | Постоянный рабочий тред для `profile=work`. |
| `MCP_VOICE_TOKEN_FILE` | нет | — | Включает `/v1/voice/*`; токен обязан отличаться от MCP-токена. |
| `MCP_SCHEDULER_URL` | для `time` | — | База `home.v1.SchedulerService`, например `http://scheduler.home-assistant.svc.cluster.local:8098`. |
| `MCP_SCHEDULER_SERVICE_TOKEN_FILE` | для `time`/app | — | обязательный service-token планировщика (`scheduler-auth#service_token`). |
| `MCP_PUSH_URL` | для `phone` | — | База `solutions/phone-push`, например `http://phone-push.home-assistant.svc.cluster.local:8095`. |
| `MCP_PUSH_SERVICE_TOKEN_FILE` | для `phone` | — | обязательный service-token phone-push. |
| `MCP_APP_API` | нет | `false` | `true` включает `/v1/system/*` и `/v1/chat/*`; требует `MCP_SCHEDULER_URL` и `MCP_SCHEDULER_SERVICE_TOKEN_FILE`. |
| `MCP_CHAT_HERMES_URL` | нет | — | Включает `/v1/chat/*`: база chat relay. Требует `MCP_APP_API=true`. |
| `MCP_CHAT_HERMES_TOKEN_FILE` | для чата | — | Bearer chat relay (`secret/k8s/home-assistant/hermes-chat-relay`, ключ `token`). |
| `MCP_CHAT_PROMPT_FILE` | для чата | — | Системный промпт беседы приложения (`chat-prompt.txt` из ConfigMap). |
| `MCP_CHAT_SESSION` | нет | `home-app` | Id сессии Hermes; должен совпадать с `HERMES_CHAT_RELAY_SESSION`. |
| `MCP_CHAT_DB` | нет | `/var/lib/mcp-gateway/chat.db` | SQLite истории чата. |
| `MCP_CHAT_MEDIA_DIR` | нет | `/var/lib/mcp-gateway/media` | Каталог вложений. |
| `MCP_CHAT_ALERTS_TOKEN_FILE` | нет | — | Включает `POST /v1/chat/alerts` (вебхук Alertmanager); токен отличается от остальных. |
| `MCP_CHAT_STT_ADDRS` | нет | — | Wyoming STT `host:port` через запятую, по порядку (GigaAM, затем Whisper). Без него голосовые получают `chat_stt_failed`. |
| `MCP_PENDING_TTL` | нет | `90s` | Время жизни отложенного действия. |
| `MCP_RESTART_COOLDOWN` | нет | `5m` | Минимальный интервал между перезапусками одного workload'а. |
| `MCP_SELF_RESTART_DELAY` | нет | `20s` | Задержка перезапуска `self_disrupting` сервиса. |
| `MCP_QUERY_TIMEOUT` | нет | `4s` | Таймаут одного запроса к бэкенду. |
| `MCP_STATUS_TIMEOUT` | нет | `8s` | Таймаут всего `infra_status`. |
| `MCP_LOG_BYTE_CAP` | нет | `65536` | Максимальный размер логов в ответе. |

Незаданный бэкенд не роняет процесс: инструмент честно сообщает, что бэкенд не
настроен, а `infra_status` помечает проверку как неполную.

## Allowlist личных проектов bb

`MCP_BB_TARGETS_FILE` — deployment data, по образцу каталога:

```yaml
targets:
  - project: proj_example        # projectId или точное имя проекта из плагина
    aliases: ["стан лэб", "station lab"]
    provider: claude-code        # необязательный pin
    model: ""                    # пусто = первая разрешённая модель
    effort: ""                   # пусто = defaultReasoningLevel модели
    title_prefix: "Голос"
```

- Резолвинг идёт только по проектам с `profile: personal` из живого ответа
  `targets`; проект из другого профиля даёт тот же отказ, что неизвестное имя.
- Явно заданные `model`/`effort` всё равно проверяются по списку моделей и
  reasoning-уровней плагина; неподдерживаемое значение — отказ до pending.
- Pending spawn лежит в `KeyedStore` под ключом `bb.spawn`, restart — в
  отдельном `PendingStore`, поэтому группы не перезаписывают друг друга.

## Каталог сервисов

Схема (полный нейтральный пример — `testdata/catalog.example.yaml`):

```yaml
overview: |
  Свободный markdown о топологии: без секретов и реальных адресов.

ignore_alerts: [Watchdog, InfoInhibitor]   # всегда активные алерты, не проблема
ignore_workloads:                          # workload'ы вне каталога по замыслу
  - kube-system/DaemonSet/svclb-traefik    # namespace/Kind/name
group_order: [Apps, External]              # фиксированный порядок групп в отчёте

services:
  - id: scheduler                # стабильный идентификатор
    title: Scheduler             # необязательное отображаемое имя (отчёт, статус, речь)
    group: Apps                  # необязательная группа отчёта
    host_title: HAOS VM          # необязательное имя хоста для external-сервисов
    aliases:
      ru: [планировщик]          # spoken-имена, обязателен хотя бы один alias
      en: [scheduler]
    kind: k8s                    # k8s | external
    namespace: example-apps      # только для k8s
    workload:
      kind: Deployment           # Deployment | StatefulSet | DaemonSet
      name: scheduler
    containers: [scheduler]      # containers[0] задаёт образ в отчёте и pods/log
    signals:
      - promql: up{job="scheduler"} == 0
        meaning: планировщик недоступен
        status: true             # включать в infra_status
    logs:
      source: pods               # loki | pods | none
      selector: '{namespace="example-apps"}'   # обязателен для loki
      containers: [scheduler]    # необязательное сужение для pods
    restart:
      allowed: true              # разрешает infra.fix
      self_disrupting: false     # отложить патч, чтобы ответ успели озвучить
      warning: Будильники в ближайшие минуты могут не сработать.
    description: Планировщик фоновых задач.
```

Сигнал с `status: true` попадает в `infra_status`; сам PromQL должен
описывать *плохое* состояние (например `up{job="x"} == 0`), потому что любой
непустой результат считается срабатыванием. Сигналы без `status` доступны
только в `infra_service`.

`title`, `group` и `host_title` необязательны, но если заданы — не могут быть
пустыми. `title` заменяет первый alias во всех человекочитаемых местах
(отчёт, статус, речь); `group` задаёт секцию отчёта, порядок секций —
`group_order`; `host_title` не даёт печатать сырой адрес внешнего хоста.

`ignore_workloads` — `namespace/Kind/name` workload'ов, которые существуют
намеренно, но не являются сервисами каталога (например, служебные
DaemonSet'ы K3s). `infra_report` перечисляет кластерные
Deployment/StatefulSet/DaemonSet и показывает каждый workload, которого нет
ни в `services`, ни здесь, отдельной проблемой «не в каталоге отчёта:
ns/Kind/name» — поэтому новый сервис нельзя забыть внести в каталог. Ту же
сверку заранее делает статическая проверка
`KubernetesCatalogCoverageTests` в `platform/quality/test_static.py`
(`mise run test:static`, quality-пайплайн): она читает все Deployment/
StatefulSet/DaemonSet из `projects/home/kubernetes/**/*.yaml` и падает,
перечисляя сразу все workload'ы, которых нет ни в `services`, ни в
`ignore_workloads`. Формат записи — ровно три части
`namespace/Kind/name` с известным видом; дубликаты — ошибка.

`ignore_alerts` — имена алертов, которые активны постоянно по замыслу
(у kube-prometheus-stack это `Watchdog` и `InfoInhibitor`). `infra_status`
выносит их в `details.ignoredAlerts`, не считает проблемой и не произносит;
`infra_alert` и `infra_service` по-прежнему показывают их явно. Сравнение
имён без учёта регистра; пустые и повторяющиеся имена — ошибка загрузки.

Логи из подов читаются максимум с трёх подов и всех контейнеров каталога
(или выбранного списка), с ограничением `MCP_LOG_BYTE_CAP`.

Проверки внешних (и любых других) сервисов объявляются в каталоге списком
`checks:`; решение ничего не знает о конкретных адресах, host/URL приходят из
каталога (в проекте — через плейсхолдеры, рендерятся при деплое):

```yaml
checks:
  - {type: tcp, label: API, host: 192.0.2.10, port: 8123}
  - {type: http, label: REST, url: "http://192.0.2.10:8123/api/",
     expect_status: 200, auth: ha}
  - {type: ha_state, label: спутник, entity_id: assist_satellite.example,
     bad_states: [unavailable, unknown]}          # bad_states по умолчанию
  - {type: ha_template, label: версия,
     template: "{{ device_attr(device_id('assist_satellite.example'), 'sw_version') }}",
     required: false}                              # справочно
  - {type: ha_unavailable_summary, label: недоступные,
     domains: [media_player]}                      # до 10 сущностей в details
```

Правила проверок: `label` обязателен; `tcp` требует host и port 1..65535;
`http` — url, `auth` из `none|ha`, `expect_status` 100..599 (по умолчанию 200),
тело не возвращается; `ha_state` — entity_id; `ha_template` — template
(пустой результат не ошибка, если не `required: true`);
`ha_unavailable_summary` считает сущности в состоянии `unavailable`
(опционально только указанные домены). Все проверки идут параллельно с
таймаутом `MCP_QUERY_TIMEOUT`; при незаданных `MCP_HA_URL`/токене `ha_*`
помечаются `skipped`, а не падают.

Правила валидации: уникальный `id`, хотя бы один alias, `kind` из двух
значений, для `k8s` обязательны namespace и workload поддерживаемого вида, для
`external` запрещены workload/restart/pods-log, для `restart.allowed` обязателен
`warning`, для `loki` обязателен selector, у каждого сигнала есть `promql` и
`meaning`, у каждой проверки — известный тип и обязательные для типа поля.

## Безопасность

- Только чтение плюс один вид мутации: strategic-merge patch аннотации
  rollout restart. Никаких других глаголов в коде нет.
- Список целей — только каталог; имена не принимаются из свободного текста.
- Перезапуск всегда двухшаговый; подтверждение хранится на сервере, TTL 90 с,
  одно действие за раз.
- Cooldown читается из аннотации объекта `mcp-gateway/last-restart` (5 минут по
  умолчанию), поэтому он переживает перезапуск процесса.
- `self_disrupting` откладывает патч на 20 секунд: сначала озвучивается ответ.
- bb: SSH только ключом к forced command, один JSON в stdin, без shell и без
  произвольных методов. Plugin-token и HMAC-секрет на хосте bb шлюзу не видны.
  Список проектов — только `bb-targets.yaml` и только `personal`; рабочие
  проекты неотличимы от несуществующих.
- voice: отдельный токен, только `/v1/voice/*`; ошибки bb для `work` не
  откатываются в Hermes; текст речи никогда не пишется в лог.
- Логи и отчёты проходят через redaction (bearer/Authorization, `token=`,
  `password=`, JWT, длинные hex/base64).
- Секреты не логируются; аудит — структурированные JSON-логи
  (`event=action`, `action`, `target`, `outcome`) и метрика
  `mcp_gateway_actions_total{action,target,outcome}`.
- TLS: workspace `reqwest` собран без встроенных корней, поэтому внешние
  HTTPS-клиенты (Prometheus/Alertmanager/Loki, Telegram, HA и внешние `http`
  проверки) берут системный PEM-бандл через `packages/rust/home-tls`
  (`SSL_CERT_FILE`, иначе `/etc/ssl/certs/ca-certificates.crt`). Клиент
  Kubernetes API доверяет **только** CA сервис-аккаунта из
  `/var/run/secrets/kubernetes.io/serviceaccount/ca.crt`
  (`tls_built_in_root_certs(false)`), а не системным корням.
- Решения о RBAC, NetworkPolicy и TLS принимает проект, который разворачивает
  шлюз. Шлюз не публикует себя наружу.

## Метрики

- `mcp_gateway_actions_total{action,target,outcome}` — попытки мутаций.
- `mcp_gateway_tool_calls_total{tool,outcome}` и
  `mcp_gateway_tool_duration_seconds{tool}` — вызовы инструментов.
- `mcp_gateway_http_requests_total{route,status}` — HTTP-запросы.
- `mcp_gateway_voice_utterances_total{route,outcome}` — маршруты голосовых фраз.
- `mcp_gateway_tool_list_changed_total{outcome}` — попытки объявить новый
  список инструментов MCP-сессии (`sent`/`error`).

## Сборка и тесты

```bash
cargo test -p mcp-gateway
bazel test //solutions/mcp-gateway/... //projects/home/kubernetes/mcp-gateway/...
bazel build //solutions/mcp-gateway:mcp-gateway_image
```

Образ — статический `x86_64-unknown-linux-musl` бинарник на digest-pinned
distroless static; `rust_service_image` сам применяет musl-переход, поэтому
отдельный флаг `--platforms` не нужен.

Тесты герметичны: фейковые HTTP-бэкенды для Prometheus, Alertmanager, Loki,
Kubernetes REST и Telegram; сеть и кластер не используются. MCP-транспорт
проверяется через Streamable HTTP с bearer-токеном на замороженных фикстурах
`testdata/contract/{server,tools}`. bb проверяется на уровне трейта `BbApi` и
встроенного фейкового SSH-сервера (реальные key/known_hosts пути), голос — на
фейковом dispatcher'е (`testdata/contract/voice`): prepare/confirm/cancel/TTL,
маршруты utterance, выход из режима, fail-closed для work и long-poll
результата. `tests/deploy_contract.rs` проверяет tracked-каталог и
bb-allowlist настоящими валидаторами.
