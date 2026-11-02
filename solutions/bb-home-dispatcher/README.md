# bb-home-dispatcher

Тонкий, детерминированный исполнитель для домашнего голосового пути:

```text
голос / Hermes -> mcp-gateway (allowlist, pending, аудит) -> SSH forced command
              -> curl на loopback -> этот плагин -> bb.sdk -> треды bb
```

Плагин не содержит LLM, не открывает API bb наружу и не умеет ничего, кроме
шести операций ниже. Терминалы, удаление, произвольные команды, установка
плагинов и доступ к файлам отсутствуют.

## Установка на сервер bb

Плагин ставится как локальный path-плагин, как `profiles`:

```bash
# на машине, где живёт bb server; путь — любой checkout этого каталога
bb plugin install path:/home/bb/plugins/bb-home-dispatcher
bb plugin token home-dispatcher          # токен для x-bb-plugin-token
bb plugin config home-dispatcher set hmacSecret <random-secret>
```

`hmacSecret` хранится как secret-настройка (0600), не в коде и не в БД.
`bb plugin token` выдаётся bb и ротируется через `--rotate`.

Для работы метода `targets` должен быть установлен плагин `profiles`: профиль
проекта читается через его RPC `listProjects`, а allowlist провайдеров и машин —
через `profilePolicy({profile})`. Без него `targets` и
`validate_spawn`/`spawn` отвечают `profiles_unavailable`; если профиль исчез из
`profiles` — `profile_not_found`.

## Настройки

| Ключ | По умолчанию | Назначение |
|---|---|---|
| `hmacSecret` | нет | Секрет HMAC-SHA256; без него маршрут отвечает 503. |
| `workerLimit` | `2` | Максимум одновременно `starting`/`active`/`pending` тредов, созданных этим плагином. |
| `permissionMode` | `auto` | Режим разрешений для создаваемых тредов: только `auto` или `accept-edits`; `full` отклоняется схемой настройки. |
| `dispatcherThreadId` | `""` | id «рабочего диспетчера», в который `send` разрешён всегда. |

Отдельной настройки allowlist провайдеров больше нет: политика профиля живёт
в плагине `profiles` (`bb profiles providers`, `bb profiles machines`) и
читается через `profilePolicy`. `null` в ответе RPC означает «любой провайдер»
или «любая машина».

## HTTP-маршрут

`POST /api/v1/plugins/home-dispatcher/http/rpc`

Аутентификация двухслойная:

1. `auth: "token"` самого bb — заголовок `x-bb-plugin-token` (или `?token=`).
2. HMAC-SHA256 подписи тела:

```text
x-bb-dispatch-timestamp: <unix seconds>
x-bb-dispatch-signature: <hex hmac_sha256(secret, "<timestamp>\n" + body)>
```

Окно метки времени — ±30 секунд; подпись сравнивается за постоянное время.
Тело ограничено 16 KiB (по `content-length` и по фактическим байтам). Любая
ошибка аутентификации — 401, не-JSON `content-type` — 415, слишком большое
тело — 413, не настроенный секрет — 503.

Пример подписи:

```bash
body='{"method":"targets","params":{}}'
ts=$(date +%s)
sig=$(printf '%s\n%s' "$ts" "$body" | openssl dgst -sha256 -hmac "$SECRET" -hex | awk '{print $2}')
curl -sS -X POST \
  -H 'content-type: application/json' \
  -H "x-bb-plugin-token: $BB_TOKEN" \
  -H "x-bb-dispatch-timestamp: $ts" \
  -H "x-bb-dispatch-signature: $sig" \
  --data "$body" \
  http://127.0.0.1:38886/api/v1/plugins/home-dispatcher/http/rpc
```

## Формат запроса и ответа

Запрос: ровно `{"method": "...", "params": {...}}`, лишние поля отклоняются.

Успех:

```json
{ "ok": true, "result": { "...": "..." }, "speech": "короткая русская фраза" }
```

Отказ метода (валидация, лимит, чужой профиль):

```json
{ "ok": false, "code": "profile_mismatch", "speech": "Проект относится к профилю «personal», а не «work»." }
```

`speech` — всегда ≤160 символов, без markdown. Ошибки транспорта и схемы
возвращают HTTP 400/401/413/503; отказы методов — HTTP 200 с `ok: false`,
чтобы голосовой путь мог их озвучить.

## Методы

### `targets`

Без параметров. Отдаёт проекты из `profiles`, машину исходника, статус
машины, политику профиля и разрешённых провайдеров с моделями и уровнями
reasoning:

```json
{
  "projects": [
    {
      "projectId": "proj_...",
      "name": "Project",
      "profile": "work",
      "policy": { "providers": ["codex"], "machines": null },
      "machine": { "hostId": "host_...", "name": "Mac", "status": "connected", "online": true },
      "providers": [
        {
          "providerId": "codex",
          "displayName": "Codex",
          "models": [
            {
              "model": "gpt-5.2-codex",
              "displayName": "GPT-5.2 Codex",
              "defaultReasoningLevel": "medium",
              "reasoningLevels": ["low", "medium", "high"]
            }
          ]
        }
      ]
    }
  ]
}
```

Политика — эффективная, из `profiles`: `providers: null` или `machines: null`
означает отсутствие ограничения.

### `status`

`{ "limit": 10 }` (1–50). Возвращает работающие и недавние треды: id, title,
project, profile, machine, status, `runtimeSeconds` (для активных — от
последнего `turn/started`, иначе `null`). Работающие идут первыми.

### `validate_spawn`

`{ profile, projectId, providerId, model, reasoningLevel }`.

Проверяет: проект принадлежит профилю; провайдер разрешён профилем
(`profilePolicy.providers`); машина проекта разрешена профилем
(`profilePolicy.machines`, `null` = любая); провайдер доступен на машине;
модель есть в каталоге; уровень reasoning поддерживается моделью; машина
online; число активных тредов плагина меньше `workerLimit`. Возвращает
`ok: true` и `result`, либо `ok: false` с кодом и `speech`.

Коды: `profiles_unavailable`, `profile_not_found`, `project_not_found`,
`profile_mismatch`, `provider_not_allowed`, `machine_not_allowed`,
`machine_missing`, `machine_offline`, `provider_unavailable`,
`model_not_found`, `reasoning_not_supported`, `thread_limit`, `bad_settings`.

### `spawn`

Те же поля плюс `title`, `prompt`, `idempotencyKey`.

Повторяет `validate_spawn`, создаёт **видимый** тред с явным title,
`permissionMode` (всегда передаётся явно; по умолчанию `auto`, `full`
запрещён настройкой) и `pluginMetadata` (`dispatcher`, `idempotencyKey`,
`profile`, `projectId`), помечая его как созданный плагином. В конец prompt
добавляется:

```text
---
В конце каждого ответа добавляй отдельной строкой:
🔊 <1–2 фразы для озвучивания по-русски>.
```

Идемпотентность: повтор с тем же ключом и теми же параметрами возвращает тот
же `threadId` и `idempotent: true`; тот же ключ с другими параметрами —
`idempotency_conflict`. Одновременные запросы с одним ключом сериализуются
внутри процесса.

### `send`

`{ threadId, text, idempotencyKey }`.

Разрешён только в треды, созданные плагином, и в `dispatcherThreadId`.
Возвращает `afterSeq` (последний seq перед отправкой), `requestSeq`/`requestId`
события `client/turn/requested` (если ход уже начался) и `delivery`.
`afterSeq` — курсор для `turn_result`.

### `turn_result`

`{ threadId, afterSeq }`.

Читает события **после** `afterSeq`, находит первый `client/turn/requested`,
затем `turn/completed` после него. Это корреляция именно этого хода, а не
«предыдущего ответа»: ответы параллельных/прошлых ходов не подмешиваются.

Статусы: `running`, `idle`, `failed`, `waiting_input`. Для завершённого хода
возвращает `spoken` — строку после `🔊` (последнюю в ответе), иначе `null`;
`text` — первые ~200 символов без markdown; `requestSeq`, `completedSeq`,
`threadId`, `ref`. Если `🔊` нет, `spoken: null`, а `speech` = «Готово.».
Если агент ждёт ответа на interaction — `waiting_input`.

## Безопасность и границы

- Наружу открыт только этот маршрут с token+HMAC; API bb не публикуется.
- Нет методов для терминалов, файлов, удаления, архивации, установки
  плагинов, произвольных `bb`-команд.
- `send` не может писать в произвольный тред; `spawn` не принимает
  произвольную машину — только машину исходника проекта, и только если она
  разрешена профилю в `profiles`.
- Лимит одновременных тредов и лимит тела защищают от случайного «размножения»
  и раздутых запросов.
- Prompt и текст сообщений не пишутся в лог; логируются только method,
  project/thread id, idempotency key и исход.
- Секрет HMAC и токен bb не хранятся в репозитории.

## Разработка и проверки

```bash
node --test solutions/bb-home-dispatcher/tests/*.test.js
bb plugin build solutions/bb-home-dispatcher
```

CI-юнит `bb-home-dispatcher` в `platform/ci/units.yml` запускает тесты на
pinned Node без npm-зависимостей. Сборка плагина выполняется `bb plugin build`
(сборщик bb), `dist/` в Git не коммитится.

## Открытые вопросы к SDK

- `turn/completed` не содержит `turnId`/`requestId`, поэтому корреляция идёт по
  seq (`afterSeq` → `client/turn/requested` → `turn/completed`). При
  параллельных ходах в одном треде это работает для последовательных
  отправок; для одновременных steer'ов нужен `turnId` в событии.
- У `threads.spawn` нет отдельного параметра «машина» для
  `project-default`; машина определяется источником проекта. Это совпадает с
  проверкой `machine online`, но не позволяет выбрать другую машину.
