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

## Группы инструментов

Группы включаются через `MCP_GROUPS` (CSV). Неизвестная группа — ошибка
запуска. Реестр расширяемый: новая группа реализует `group.Group` и
регистрируется в `app`, без изменений транспорта и auth.

### `infra.read` (все `readOnlyHint=true`)

| Инструмент | Что делает |
|---|---|
| `infra_status()` | Один агрегированный вызов: активные алерты Alertmanager, сигналы каталога, доступность workload'ов и перезапуски подов за час. Параллельные запросы с таймаутами; общий бюджет `MCP_STATUS_TIMEOUT` (8 с). |
| `infra_service(name)` | Разрешает spoken-имя (регистр и ё/е не важны, лёгкая нечёткость) и отдаёт workload, поды, перезапуски, события, сигналы и связанные алерты. |
| `infra_alert(ref)` | Объясняет один активный алерт по fingerprint Alertmanager (полному или префиксу от 6 символов) либо по имени. Возвращает labels/annotations и detail связанного сервиса из каталога (сопоставление по namespace/deployment/pod/endpoint/job и алиасам), чтобы можно было ответить на уточнение «а что с этим алертом?». |
| `infra_logs(name, since, grep, limit)` | Логи сервиса: Loki, если он там есть, иначе `pods/log` (все контейнеры каталога или выбранные). Секреты редактируются. Размер ограничен `MCP_LOG_BYTE_CAP`. |
| `infra_query(promql)` | Один instant-запрос PromQL, только чтение, результат ограничен. |
| `infra_overview()` | Топология и каталог для контекста модели. |
| `infra_report(verbose)` | Полный отчёт по всем сервисам каталога: статус, готовые реплики, перезапуски за час, образ (digest сокращён), возраст, внешние проверки, версии HA и активные алерты. Заголовок отчёта печатается в зоне `MCP_TIMEZONE`. Возвращает короткий вердикт и компактный plain-text отчёт (≤ 3500 символов) для Telegram: проблемы первыми, затем «В норме» по группам каталога. Образ выбирается по `containers[0]`, затем по имени workload/суффиксу, затем по порядку контейнеров в поде — сайдкар не подменяет основной контейнер. `verbose: true` добавляет секцию «Версии» (образ, возраст, версии); по умолчанию их нет в тексте, но они есть в `details` JSON. |
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
Текст ограничен 2000 символами; действует rate-limit; в slog пишутся маршрут,
профиль, id треда и длина текста, **никогда** сам текст речи.

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
| `MCP_GROUPS` | нет | `infra.read` | Включённые группы: `infra.read`, `infra.fix`, `infra.report`, `bb.read`, `bb.act`. |
| `MCP_INFRA_CATALOG_FILE` | для read/fix | — | YAML-каталог сервисов. |
| `MCP_HTTP_ADDR` | нет | `:8080` | Адрес HTTP-сервера. |
| `MCP_TIMEZONE` | нет | `UTC` | IANA-зона для человекочитаемого времени в текстах инструментов (заголовок отчёта). Проверяется при загрузке; для distroless-образа IANA tzdata встроена в бинарник. |
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
| `MCP_PENDING_TTL` | нет | `90s` | Время жизни отложенного действия. |
| `MCP_RESTART_COOLDOWN` | нет | `5m` | Минимальный интервал между перезапусками одного workload'а. |
| `MCP_SELF_RESTART_DELAY` | нет | `20s` | Задержка перезапуска `self_disrupting` сервиса. |
| `MCP_QUERY_TIMEOUT` | нет | `4s` | Таймаут одного запроса к бэкенду. |
| `MCP_STATUS_TIMEOUT` | нет | `8s` | Таймаут всего `infra_status`. |
| `MCP_LOG_BYTE_CAP` | нет | `65536` | Максимальный размер логов в ответе. |
| `LOG_LEVEL` | нет | `info` | `debug`, `info`, `warn`, `error`. |

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
- Pending spawn лежит в `pending.Keyed` под ключом `bb.spawn`, restart — в
  отдельном `pending.Store`, поэтому группы не перезаписывают друг друга.

## Каталог сервисов

Схема (полный нейтральный пример — `testdata/catalog.example.yaml`):

```yaml
overview: |
  Свободный markdown о топологии: без секретов и реальных адресов.

ignore_alerts: [Watchdog, InfoInhibitor]   # всегда активные алерты, не проблема
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
- Только `mcp_gateway_*`: Go/process-коллекторов нет, как и мёртвой
  `mcp_gateway_backend_up`.

## Сборка и тесты

```bash
cargo test -p mcp-gateway
bazel test //solutions/mcp-gateway/... //projects/home/kubernetes/mcp-gateway/...
bazel build //solutions/mcp-gateway:mcp-gateway_image
```

Образ — статический `x86_64-unknown-linux-musl` бинарник на digest-pinned
distroless static; `rust_service_image` сам применяет musl-переход, поэтому
флаг `--platforms=@rules_go//...` не нужен.

Тесты герметичны: фейковые HTTP-бэкенды для Prometheus, Alertmanager, Loki,
Kubernetes REST и Telegram; сеть и кластер не используются. MCP-транспорт
проверяется через Streamable HTTP с bearer-токеном на замороженных фикстурах
`testdata/contract/{server,tools}`. bb проверяется на уровне трейта `BbApi` и
встроенного фейкового SSH-сервера (реальные key/known_hosts пути), голос — на
фейковом dispatcher'е (`testdata/contract/voice`): prepare/confirm/cancel/TTL,
маршруты utterance, выход из режима, fail-closed для work и long-poll
результата. `tests/deploy_contract.rs` проверяет tracked-каталог и
bb-allowlist настоящими валидаторами.
