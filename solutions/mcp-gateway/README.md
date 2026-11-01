# mcp-gateway

Один расширяемый MCP-шлюз для инфраструктуры: read-only диагностика,
подтверждаемый перезапуск и отправка отчётов. Шлюз не знает, где он запущен:
все endpoint'ы, пространства имён, каталог сервисов и секреты приходят из
окружения или смонтированных файлов.

```
MCP client (Hermes, IDE, agent)
        │  Streamable HTTP + Bearer
        ▼
   mcp-gateway ── infra.read   → Prometheus / Alertmanager / Loki / Kubernetes REST
        │      ── infra.fix    → rollout restart (единственная мутация)
        │      ── infra.report → Telegram Bot API (фиксированный получатель)
        ▼
  /healthz  /readyz  /metrics
```

## Транспорт и аутентификация

- MCP Streamable HTTP: `POST /mcp` (официальный Go SDK
  `github.com/modelcontextprotocol/go-sdk`).
- `Authorization: Bearer <token>`. Токен читается из `MCP_AUTH_TOKEN_FILE`,
  сравнивается в постоянное время. Без токена — `401`.
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
| `MCP_GROUPS` | нет | `infra.read` | Включённые группы: `infra.read`, `infra.fix`, `infra.report`. |
| `MCP_INFRA_CATALOG_FILE` | для read/fix | — | YAML-каталог сервисов. |
| `MCP_HTTP_ADDR` | нет | `:8080` | Адрес HTTP-сервера. |
| `MCP_PROMETHEUS_URL` | нет | — | База Prometheus API, например `http://prometheus.example:9090`. |
| `MCP_ALERTMANAGER_URL` | нет | — | База Alertmanager, без `/api/v2`. |
| `MCP_LOKI_URL` | нет | — | База Loki, например `http://loki.example:3100`. |
| `KUBERNETES_API_URL` | нет | in-cluster | Переопределение API-сервера (тесты, внешний кластер). |
| `KUBERNETES_TOKEN_FILE` | нет | `/var/run/secrets/kubernetes.io/serviceaccount/token` | ServiceAccount-токен; перечитывается на каждый запрос. |
| `KUBERNETES_CA_FILE` | нет | `/var/run/secrets/kubernetes.io/serviceaccount/ca.crt` | CA кластера. |
| `TELEGRAM_API_URL` | нет | `https://api.telegram.org` | База Bot API. |
| `TELEGRAM_BOT_TOKEN_FILE` | для report | — | Токен бота. |
| `TELEGRAM_CHAT_ID_FILE` | для report | — | Chat id получателя; если в файле список через запятую, берётся первый элемент. |
| `MCP_PENDING_TTL` | нет | `90s` | Время жизни отложенного действия. |
| `MCP_RESTART_COOLDOWN` | нет | `5m` | Минимальный интервал между перезапусками одного workload'а. |
| `MCP_SELF_RESTART_DELAY` | нет | `20s` | Задержка перезапуска `self_disrupting` сервиса. |
| `MCP_QUERY_TIMEOUT` | нет | `4s` | Таймаут одного запроса к бэкенду. |
| `MCP_STATUS_TIMEOUT` | нет | `8s` | Таймаут всего `infra_status`. |
| `MCP_LOG_BYTE_CAP` | нет | `65536` | Максимальный размер логов в ответе. |
| `LOG_LEVEL` | нет | `info` | `debug`, `info`, `warn`, `error`. |

Незаданный бэкенд не роняет процесс: инструмент честно сообщает, что бэкенд не
настроен, а `infra_status` помечает проверку как неполную.

## Каталог сервисов

Схема (полный нейтральный пример — `testdata/catalog.example.yaml`):

```yaml
overview: |
  Свободный markdown о топологии: без секретов и реальных адресов.

ignore_alerts: [Watchdog, InfoInhibitor]   # всегда активные алерты, не проблема

services:
  - id: scheduler                # стабильный идентификатор
    aliases:
      ru: [планировщик]          # spoken-имена, обязателен хотя бы один alias
      en: [scheduler]
    kind: k8s                    # k8s | external
    namespace: example-apps      # только для k8s
    workload:
      kind: Deployment           # Deployment | StatefulSet | DaemonSet
      name: scheduler
    containers: [scheduler]      # используется для pods/log
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

`ignore_alerts` — имена алертов, которые активны постоянно по замыслу
(у kube-prometheus-stack это `Watchdog` и `InfoInhibitor`). `infra_status`
выносит их в `details.ignoredAlerts`, не считает проблемой и не произносит;
`infra_alert` и `infra_service` по-прежнему показывают их явно. Сравнение
имён без учёта регистра; пустые и повторяющиеся имена — ошибка загрузки.

Логи из подов читаются максимум с трёх подов и всех контейнеров каталога
(или выбранного списка), с ограничением `MCP_LOG_BYTE_CAP`.

Правила валидации: уникальный `id`, хотя бы один alias, `kind` из двух
значений, для `k8s` обязательны namespace и workload поддерживаемого вида, для
`external` запрещены workload/restart/pods-log, для `restart.allowed` обязателен
`warning`, для `loki` обязателен selector, у каждого сигнала есть `promql` и
`meaning`.

## Безопасность

- Только чтение плюс один вид мутации: strategic-merge patch аннотации
  rollout restart. Никаких других глаголов в коде нет.
- Список целей — только каталог; имена не принимаются из свободного текста.
- Перезапуск всегда двухшаговый; подтверждение хранится на сервере, TTL 90 с,
  одно действие за раз.
- Cooldown читается из аннотации объекта `mcp-gateway/last-restart` (5 минут по
  умолчанию), поэтому он переживает перезапуск процесса.
- `self_disrupting` откладывает патч на 20 секунд: сначала озвучивается ответ.
- Логи и отчёты проходят через redaction (bearer/Authorization, `token=`,
  `password=`, JWT, длинные hex/base64).
- Секреты не логируются; аудит — структурированный slog (`event=action`,
  `action`, `target`, `outcome`) и метрика
  `mcp_gateway_actions_total{action,target,outcome}`.
- Решения о RBAC, NetworkPolicy и TLS принимает проект, который разворачивает
  шлюз. Шлюз не публикует себя наружу.

## Метрики

- `mcp_gateway_actions_total{action,target,outcome}` — попытки мутаций.
- `mcp_gateway_tool_calls_total{tool,outcome}` и
  `mcp_gateway_tool_duration_seconds{tool}` — вызовы инструментов.
- `mcp_gateway_http_requests_total{route,status}` — HTTP-запросы.
- Стандартные Go/process-метрики.

## Сборка и тесты

```bash
go test ./solutions/mcp-gateway/...
go vet ./solutions/mcp-gateway/...
bazel run //:gazelle
bazel build //solutions/mcp-gateway/...
bazel build //solutions/mcp-gateway:mcp-gateway_image \
  --platforms=@rules_go//go/toolchain:linux_amd64
```

Тесты герметичны: `httptest`-фейки для Prometheus, Alertmanager, Loki,
Kubernetes REST и Telegram; сеть и кластер не используются. MCP-транспорт
проверяется настоящим SDK-клиентом через Streamable HTTP с bearer-токеном.
