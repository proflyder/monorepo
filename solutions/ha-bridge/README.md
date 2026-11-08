# ha-bridge

Исполнитель (executor) durable-планировщика `home.v1.SchedulerService` для
Home Assistant. Забирает события из очереди `home`, вызывает allowlisted HA
script и подтверждает доставку только после HTTP 2xx от Home Assistant.

Владелец расписаний — scheduler; HA выполняет только звук (`alarm_start`,
`schedule_announce`, `alarm_stop`). Мост не создаёт, не включает и не
импортирует расписания. Домашний инстанс — `projects/home/kubernetes/scheduler/`.

## Реализация

Развёрнутый бинарь — Rust-крейт
`solutions/ha-bridge/crates/ha-bridge` (`ha-bridge`): домен
`plan/order/stop/announce`, ledger JSON v1 с атомарной записью, `Handler`
поверх общего `home-worker` на очереди `home`, клиенты `home-ha` и
`home-proto`, конфиг и те же `-healthcheck`. Образ собирается
`rust_service_image` (static musl, uid 65534, entrypoint
`/usr/local/bin/ha-bridge`), путь ledger на PVC не изменился
(`/data/ledger.json`).

Go-код удалён. `solutions/ha-bridge/testdata/events/*.json` и
`testdata/ledger/v1.json` — замороженные golden-векторы, снятые с прежней
Go-реализации; Rust-тесты читают их без изменений.

`ha-control` — второй `[[bin]]` того же крейта (`src/ha_control.rs`): каталог
`GET /v1/catalog`, discovery с TTL-кэшем, `Handler` на очереди `control` и
`-healthcheck`. Векторы каталога (`testdata/catalog/states.json`,
`templates/*.txt`, `catalog.json`) — замороженные golden-данные и читаются
Rust-тестом.

## Границы

- Планировщик владеет расписаниями, таймерами, occurrence state, lease/ack и
  очередями. Мост не хранит расписание и не имеет собственной БД.
- Home Assistant выполняет только side effect: звонок, chime, TTS, stop.
  Громкость, restore source, media player и TTS остаются в HA/Pulsar.
- Телефон — только управление состоянием на сервере; он не является
  исполнителем и не владеет будильниками.
- Мост говорит только с доменным scheduler API. В нём нет MCP, LLM,
  Hermes-интеграции и входящего HTTP. MCP для Hermes и других проектов —
  общий `solutions/mcp-gateway`, а не MCP внутри моста.
- Нет слушающего порта и нет неаутентифицированной входящей точки.

## Схема

```text
scheduler (SQLite durable outbox)
  │  Connect JSON POST /home.v1.SchedulerService/ListDueEvents  (queue="home")
  │  Authorization: Bearer <scheduler token>
  ▼
ha-bridge (этот solution)  ──►  Home Assistant REST
  │  POST /api/services/script/turn_on          (Bearer <HA token>)
  │  entity_id: script.<configured>
  │  variables: label/kind/message/chime/event_id/source_id
  ▼
Connect JSON POST /home.v1.SchedulerService/AckEvent
  DELIVERED только после 2xx от HA; FAILED только для неисполнимых событий
```

Цикл execute-режима:

1. `ListDueEvents(consumer_id, queue, lease_seconds, limit)` — ровно одна
   ограниченная пачка; очередь всегда задана явно.
2. События одной пачки упорядочиваются по source, затем по `due_unix`; при
   одинаковом due `STOP` выполняется раньше `FIRE`. Обработка последовательная.
3. Для каждого события: проверка allowlist, опциональная проверка локального
   receipt-ledger, вызов HA, запись ledger, затем `AckEvent`.
4. `BRIDGE_POLL_INTERVAL` (по умолчанию 1 s) между пачками; при ошибке
   лизинга — экспоненциальный backoff до `BRIDGE_MAX_BACKOFF`.

Snooze создаёт на сервере отложенный `FIRE`; STOP приходит как control event с
`action=STOP`. Мост **не вызывает** `alarm_snooze`: snooze полностью принадлежит
планировщику, иначе появилось бы второе расписание в HA.

`alarm_stop` в HA глобальный, поэтому STOP разрешён только для alarm-события,
чей `source_id`, occurrence и due-порядок совпадают с активным alarm, который
запустил этот мост. Активный source вместе с `occurrence_unix` и `due_unix`
FIRE хранится в receipt-ledger (`activeAlarm`) и восстанавливается после
рестарта. Поздний STOP от старой occurrence или от времени раньше активного
FIRE (например, после snooze re-fire) не останавливает новый звонок; сравнение
идёт только по значениям scheduler, не по wall-clock моста. Timer/reminder STOP
не вызывает HA: announce — one-shot, и глобальный stop мог бы оборвать чужой
будильник. Такие события подтверждаются `ACK_SKIPPED` с безопасной причиной.

### Очереди

- `BRIDGE_QUEUE` по умолчанию `home`: будильники, напоминания, таймеры.
- События `SOURCE_KIND_TASK` (generic tasks, `EnqueueTask`) не являются
  действием HA. Мост не запрашивает очередь `tasks`; значение `BRIDGE_QUEUE=tasks`
  отклоняется при старте. Если task-событие всё же придёт в `home`, оно
  завершается `ACK_OUTCOME_FAILED` без вызова HA.
- Пустая очередь в `ListDueEventsRequest` означает «все очереди», поэтому мост
  никогда не отправляет пустое значение.

### Маппинг событий

| Событие | Script (по умолчанию) | Variables |
| --- | --- | --- |
| `FIRE`, `SOURCE_KIND_ALARM` | `alarm_start` | `label`, `kind`, `event_id`, `source_id` |
| `FIRE`, `SOURCE_KIND_TIMER` / `REMINDER` | `schedule_announce` | `message`, `chime=true`, `label`, `kind`, `event_id`, `source_id` |
| `STOP`, alarm, source = активный alarm этого моста | `alarm_stop` | `reason=scheduler_stop`, `label`, `kind`, `event_id`, `source_id` |
| `STOP`, alarm, другой/неизвестный source | — | `ACK_SKIPPED`, HA не вызывается |
| `STOP`, timer/reminder | — | `ACK_SKIPPED`: one-shot announce нельзя безопасно остановить глобальным stop |
| `TASK`, `SCENARIO`, неизвестный kind/action, чужая queue | — | `ACK_FAILED`, HA не вызывается |

`schedule_announce` — существующий HA script с полями `message` и `chime`
(`projects/home-assistant/ha-voice/scripts.yaml`). `alarm_start`/`alarm_stop`
там же. Имена переопределяются; никакой runtime-контракт, кроме этих трёх
полей, не подразумевается.

Событие никогда не выбирает service/entity: script берётся только из
конфигурации. `payload_json` (proto `string`, ≤16 KiB) сегодня не читается и
не влияет на исполнение; любой будущий executor обязан трактовать его как
данные, а не как выбор service/entity.

`alarm_stop` — глобальный stop для активного звонка; это согласуется с моделью
«одна активная occurrence на исполнителя». `schedule_announce` имеет
`mode: restart`, поэтому новый announce заменяет предыдущий.

## Конфигурация

Все значения читаются из окружения. Токены — только через env или файл; файл
имеет приоритет. Значения токенов никогда не логируются.

| Переменная | Обяз. | По умолчанию | Смысл |
| --- | --- | --- | --- |
| `HA_URL` | да | — | База HA, `http(s)://host:port`, без credentials/query |
| `SCHEDULER_ENDPOINT` | да | — | База scheduler; допускается суффикс `/home.v1.SchedulerService` |
| `SCHEDULER_TOKEN` / `SCHEDULER_TOKEN_FILE` | да | — | Bearer для scheduler |
| `HA_TOKEN` / `HA_TOKEN_FILE` | в execute | — | Bearer для HA; в dry-run опционален |
| `BRIDGE_CONSUMER_ID` | да | — | Стабильный id `[A-Za-z0-9._-]{1,64}` |
| `BRIDGE_QUEUE` | нет | `home` | Очередь; `tasks` запрещена |
| `BRIDGE_EXECUTE` | нет | `false` | `true` включает lease/execute/ack |
| `BRIDGE_BATCH_SIZE` | нет | `10` | 1..200 |
| `BRIDGE_LEASE_SECONDS` | нет | `300` | 1..3600; должен покрывать бюджет пачки |
| `BRIDGE_POLL_INTERVAL` | нет | `1s` | Пауза между пачками (быстрый STOP для интерактивного управления) |
| `BRIDGE_MAX_BACKOFF` | нет | `5m` | Потолок backoff |
| `BRIDGE_HA_TIMEOUT` | нет | `10s` | Таймаут одного вызова HA |
| `BRIDGE_SCHEDULER_TIMEOUT` | нет | `10s` | Таймаут одного RPC |
| `BRIDGE_MAX_ATTEMPTS` | нет | `10` | Предел попыток для 401/403 до `ACK_FAILED` |
| `BRIDGE_ALARM_SCRIPT` | нет | `alarm_start` | Script для FIRE alarm |
| `BRIDGE_ANNOUNCE_SCRIPT` | нет | `schedule_announce` | Script для FIRE timer/reminder |
| `BRIDGE_STOP_SCRIPT` | нет | `alarm_stop` | Script для STOP |
| `BRIDGE_LEDGER_PATH` | нет | — | JSON-ledger; без него dedupe только в памяти процесса |
| `HA_CA_FILE` | нет | — | Дополнительный CA для приватного TLS HA |
| `SCHEDULER_CA_FILE` | нет | — | Дополнительный CA для приватного TLS scheduler |

`BRIDGE_LEASE_SECONDS` проверяется при старте: он должен быть больше
`batch_size × (HA timeout + scheduler timeout + 1s) + 15s`. Иначе мост
завершается с ошибкой, не начиная лизинг.

Пример (LAN):

```bash
export HA_URL=http://192.0.2.28:8123
export SCHEDULER_ENDPOINT=http://192.0.2.27:8098
export SCHEDULER_TOKEN_FILE=/etc/ha-bridge/scheduler.token
export HA_TOKEN_FILE=/etc/ha-bridge/ha.token
export BRIDGE_CONSUMER_ID=ha-bridge-home
```

TLS-проверка всегда включена. URL с `user:pass@` отклоняется, редиректы
отключены (`CheckRedirect` → `ErrUseLastResponse`), поэтому Bearer не уходит на
чужой хост, а 3xx считается ошибкой.

## Запуск

Dry-run (по умолчанию; только `GetStatus` и, при наличии токена, `GET /api/`):

```bash
cargo run -p ha-bridge
```

Execute:

```bash
BRIDGE_EXECUTE=true \
BRIDGE_LEDGER_PATH=/var/lib/ha-bridge/ledger.json \
cargo run -p ha-bridge
```

Проверки и golden-векторы:

```bash
cargo test -p ha-bridge
cargo clippy -p ha-bridge --all-targets -- -D warnings
```

Bazel:

```bash
bazel build //solutions/ha-bridge/...
bazel test //solutions/ha-bridge/...
```

Остановка — `SIGINT`/`SIGTERM`: мост прекращает лизинг, не подтверждает
прерванные события, и они возвращаются после истечения lease.

## Семантика доставки

- Доставка **at-least-once**. `ACK_DELIVERED` отправляется только после 2xx от
  HA. Ответ scheduler ACK не очищает активную occurrence — STOP/snooze остаются
  доступны, пока окно активно.
- Транзиентные ошибки (сеть, timeout, 5xx, 429, 408) не подтверждаются: lease
  истекает, событие приходит снова. Backoff применяется к ошибкам лизинга.
- Структурные 4xx HA (400/404/405/409/422) → `ACK_FAILED` с коротким detail:
  повтор не поможет до изменения конфигурации.
- 401/403 → повтор до `BRIDGE_MAX_ATTEMPTS`, затем `ACK_FAILED` (граница, чтобы
  не крутиться бесконечно при неверном токене).
- Локальный receipt-ledger (`BRIDGE_LEDGER_PATH`) уменьшает дубли: повторно
  доставленное событие с записью в ledger подтверждается без вызова HA. Тот же
  файл атомарно хранит `activeAlarm`, поэтому STOP после рестарта не теряет
  активный source. Без файла (in-memory fallback) marker живёт только в
  процессе.
  Запись — атомарная (`temp` + `fsync` + `rename`). Это **не** exactly-once:
  падение между side effect в HA и записью ledger или между ledger и ACK даёт
  повтор. Ledger не является источником истины.
- Один потребитель на очередь: мост не координируется с другими экземплярами.
  Параллельные исполнители для одних и тех же событий не поддерживаются.
- Порядок STOP/FIRE гарантируется только внутри одной пачки и одного source.
  Между пачками возможна гонка; для интерактивного STOP важен `BRIDGE_POLL_INTERVAL`.
  STOP для ещё не запущенного alarm пропускается и не глушит последующий FIRE.
  Для той же recurring occurrence STOP с due раньше активного FIRE (после snooze)
  пропускается; актуальный STOP с более поздним due разрешён.
- Один глобальный аудиоканал: одновременно отслеживается один активный alarm.
  Второй alarm перезаписывает marker; STOP первого после этого безопасно
  пропускается. После падения между вызовом HA и записью ledger marker может
  отсутствовать — STOP тогда пропускается, звук останавливается вручную в HA.

## Тесты

`cargo test -p ha-bridge` покрывает:

- конфигурацию: очередь/tasks, токен-файлы, запрет URL credentials, бюджет lease;
- Connect JSON: путь, заголовки, `queue`, int64-строки и enum-name/number,
  legacy action → FIRE, Connect-ошибки, отсутствие утечки токена/URL;
- отключённые редиректы и сохранённую TLS-проверку (self-signed отклоняется,
  приватный CA принимается);
- HA REST: `entity_id`, variables, 2xx/4xx/5xx классификацию, отсутствие тела в
  логах;
- dry-run: только `GetStatus`/`GET /api/`, без lease, HA call и ack;
- execute: маппинг alarm/timer/reminder/STOP, STOP только для активного alarm
  (A→B: STOP A не глушит B), timer STOP без HA, запрет snooze, task/unknown
  fail-closed, HA-ошибка не превращается в success, дубликаты, порядок
  `HA → ledger → ACK`, STOP раньше FIRE, backoff и graceful shutdown.

Тесты не обращаются к живому scheduler, HA или устройству.
Процессный E2E (реальные бинарники, fake HA на loopback, временные SQLite и
ledger) — `projects/home/e2e/scheduler_smoke.py`; живой звук и cutover он не
проверяет.

## Rollback

Второго исполнителя на одних расписаниях держать нельзя. Откат: остановить
мост (lease истечёт, неподтверждённые события вернутся), вернуть прежний
исполнитель, при необходимости остановить текущий звук. Ledger сохранить: он
лишь подавляет повторы.

## Ограничения

- `payload_json` в `DueEvent` — `string`; мост его не использует.
- Нет exactly-once и нет распределённой блокировки между экземплярами.
- Глобальный `alarm_stop` останавливает текущий звонок целиком; отдельного
  per-source stop в HA нет. Мост ограничивает его активным alarm-marker, но
  второй одновременный alarm не поддерживается. `schedule_announce` имеет
  `mode: restart`, поэтому новый announce заменяет предыдущий.
- STOP timer/reminder не останавливает уже проигранный one-shot announce;
  событие закрывается как `ACK_SKIPPED`, backend-состояние при этом
  отменяется.
- Ledger хранит только event id и метаданные; ретенция 30 дней, предел 20 000
  записей.
- Generic tasks (`EnqueueTask`) и recurring task schedules не исполняются.
- Нет MCP, LLM, уведомлений на телефон и импорта расписаний.

## ha-control: каталог устройств и сценариев для телефона

`ha-control` — **отдельный** бинарь того же крейта (`cargo run -p ha-bridge --
--bin ha-control`, Bazel `//solutions/ha-bridge/crates/ha-bridge:ha-control`). Алармовый `ha-bridge`
остаётся без listener и без изменений поведения; `ha-control` решает другую
задачу: дать телефону список управляемых сценариев и устройств и исполнить
команды, которые телефон кладёт в планировщик.

```text
phone ── GET  /v1/catalog ─────────────► ha-control ── /api/states, /api/template ─► HA   (только чтение)
phone ── EnqueueTask(queue=control) ───► scheduler ◄── ListDueEvents ──── ha-control (execute)
                                                       └─ POST /api/services/<domain>/<service> ─► HA
```

### Каталог собирается по факту из Home Assistant

Никакого списка сущностей на сервере нет. При каждом запросе (кэш `CONTROL_CACHE_TTL`,
3 s) `ha-control` читает из HA:

- сущности домена `light`, `switch`, `scene`, `script` с `friendly_name`, `icon` и
  состоянием (`/api/states`);
- **комнату** — HA area сущности (`area_name()` через `/api/template`);
- **скрытые** сущности (`is_hidden_entity()`): всё, что скрыто в HA, не попадает в каталог;
- **метки** — какие скрипты помечены ролью «сценарий» и какая сущность помечена
  ролью «профиль» (`label_entities()`);
- **устройство** — `device_id()` и имя устройства (`device_attr(..., 'name_by_user')`
  или `name`) через `/api/template`; **категория сущности** — из WebSocket
  реестра сущностей (`config/entity_registry/list`, см. ниже).

Правила, а не списки:

| Что | Правило |
| --- | --- |
| `light`, `switch` | показываются все; скрытые в HA (`hidden`) и попавшие под `CONTROL_EXCLUDE` — нет |
| `switch` с меткой профиля | вид `profile`: селектор профиля ассистента, всегда первым в каталоге |
| `scene` | показываются все (сцена — пользовательский пресет по определению); `hidden`/`CONTROL_EXCLUDE` действуют |
| `script` | **только с меткой роли «сценарий»**: HA label с id `scenario` (`CONTROL_SCENARIO_LABEL`) |
| действие `speak` | синтетическая запись `action.station_speak` (`kind: "action"`) появляется, только если в HA есть `script.speak_text`; device/room — от сущности `assist_satellite` |
| иконка, area, имя | только отображение; на то, что показывать, не влияют |
| `CONTROL_EXCLUDE=glob,glob` | `path.Match`-маски по entity id, например `switch.*_child_lock` |

Что значит метка `scenario`: «сценарий, который я запускаю лично». Это доменная роль,
а не имя клиента: тот же смысл может использовать приложение, голос или MCP-gateway. Почему именно метка. Скрипты в HA — смесь пользовательских сценариев и
инфраструктуры (будильники, RPC-обёртки, музыкальные действия), а нативного признака
«это моё» у скрипта нет: *Expose to Assist* — голосовой концепт (в нём уже есть
музыкальные действия), иконка и категория — оформление, `fields`/`mode` — техническое.
Единственное, что выражает намерение пользователя, — явная метка. Метки в HA
поддерживаются нативно у любой сущности с записью в реестре, в том числе у скриптов
(у них есть `unique_id` и реестровая запись, проверено на HA 2026.10); проставляются в UI (Настройки → Области, метки и
зоны → Метки → создать метку **Scenario**, затем *Метки* в настройках скрипта) или
через entity registry API. В YAML метку не задать — она хранится в реестре.

Что значит метка `voice_profile`: «эта `switch`-сущность задаёт именованный профиль
ассистента». Каталог отдаёт её как `kind: "profile"` — не как устройство: приложение
закрепляет её над сценариями, не показывает в фильтрах и счётчиках устройств и
рисует сегментированным переключателем вместо строки-выключателя. Состояние остаётся
обычным `on`/`off` HA-switch: **`on` = второй (рабочий) профиль, `off` = профиль по
умолчанию (личный)**, команды — те же `turn_on`/`turn_off`, никаких новых RPC. Источник
истины — сама сущность в HA (например template switch над `input_select`); `ha-control`
её не дублирует. Метка ставится так же, как `scenario` (создать метку **Voice
profile** с id `voice_profile` и назначить её переключателю), либо задаётся другой id
через `CONTROL_PROFILE_LABEL`. Без метки сущность остаётся обычным устройством —
поведение каталога не меняется.

### Устройства, заголовки и служебные сущности

Записи каталога остаются плоскими (одна сущность HA — одна запись), но несут
`device_id` и `device`, поэтому клиент группирует их по устройству сам.
`friendly_name` в HA — это `<имя устройства> <имя сущности>`; заголовок каталога
отрезает префикс устройства, затем ведущие разделители (`*`, `-`, `:` и т. п.) и
лишние пробелы. Если после этого ничего не осталось, заголовком становится имя
устройства; сущность без устройства или без префикса сохраняет своё имя.

`role` различает основные сущности и служебные настройки устройства:

- `setting` — HA `entity_category` `config` или `diagnostic` (например
  переключатели колонки `Clock`/`Wake`/`Microphone`);
- `setting` — также `switch`-сущность без категории (`entity_category: null`),
  если у того же устройства есть сущность домена `light` **и** интеграция не
  размечает на этом устройстве ни одной категории `config`/`diagnostic`:
  у Yeelight лампа — это управление, а `delay-off`/`pomodoro`/`save-state`/
  `*_enable` — её настройки, хотя HA отдаёт для них `null`. Если интеграция
  категории размечает (как колонка), реестр авторитетен: у «Яндекс Станция
  Миди» есть light `Ring`, но `Microphones` и `Clock display` с `null`
  остаются основными;
- `primary` — всё остальное (у колонки `mute_microphones`, `clock_display` и
  подобные остаются основными);
- пусто — реестр не прочитался: роль неизвестна, сущности не теряются.

Служебные сущности не удаляются из каталога: клиент решает, показывать ли их
отдельной группой.

Категория читается best effort по WebSocket: `/api/states` её не отдаёт, а у
entity registry нет REST-эндпоинта. `home-ha` открывает одно короткое
соединение `ws(s)://…/api/websocket` на каждое обновление каталога (кэш
`CONTROL_CACHE_TTL`), аутентифицируется тем же токеном и запрашивает
`config/entity_registry/list`; для `https` используется `wss` и тот же
`HA_CA_FILE`. Всё ограничено таймаутом клиента. При ошибке каталог отдаётся без
ролей (и без device-эвристики), в лог идёт предупреждение, следующий TTL
повторяет попытку.

Новое устройство, переименование, перенос в другую комнату, скрытие, снятие/постановка
метки в HA подхватываются сами, без правок на сервере и в приложении.

### Действие «сказать на колонке»

`action.station_speak` — единственная сегодня запись `kind: "action"`. Она
появляется в каталоге, пока в Home Assistant есть `script.speak_text`, и
исчезает вместе со скриптом (скрипт скрыт в HA — тоже исчезает). `device_id`,
`device` и `room` берутся у сущности `assist_satellite`; без неё действие
остаётся доступным, но без группировки. Действие одно: `speak`.

Задача: `{"entry":"action.station_speak","action":"speak","text":"..."}`.
Текст обязателен, обрезается по краям, 1–500 символов, без управляющих
символов; иначе `ACK_FAILED` с detail без текста. Исполнитель вызывает
`script.turn_on` для `script.speak_text` с `variables: {message: <текст>}`.
Текст — пользовательские данные: он не попадает в логи, detail, ledger или
отчёты, а service/entity из payload не выбираются. Громкость пока не
передаётся: неизвестные поля payload игнорируются, как и раньше.

Сбои: если HA недоступен, отдаётся последний известный каталог с
`statesOk=false` и `available=false` у всех записей; если каталога ещё не было —
`502`. Если метка сценария не читается (старый HA без labels), скрипты не показываются
(**закрыто**), устройства и сцены работают. Области и hidden — best effort: без них
записи остаются без комнаты / без фильтра скрытых (в лог идёт предупреждение).
Непрочитанная метка профиля только теряет роль: `switch` остаётся обычным устройством
и продолжает работать.

### Исполнение — через планировщик

Телефон делает `EnqueueTask(task_type="home.control", queue="control",
payload={"entry": "<id>", "action": "run|turn_on|turn_off|set|speak"})`. Исполнитель
**заново** обнаруживает каталог и принимает команду только если `entry` есть в нём
сейчас: id вне правил (например `script.alarm_stop`) отклоняется с `ACK_FAILED`
без вызова HA. `run` → `<domain>.turn_on` для script/scene; `turn_on`/`turn_off` —
для light/switch и `profile`; `speak` — только для `action.station_speak` и только
с `text`. Service и entity из payload никогда не берутся. `toggle` нет
намеренно (доставка at-least-once не должна переключать дважды). Если HA недоступен
в момент исполнения — событие остаётся без ack и придёт повторно (до `MaxAttempts`).

Планировщик ничего не знает о каталоге: он лишь хранит очередь `control`
(очереди `home` и `tasks` исполнителю запрещены).

### API и безопасность

`GET /v1/catalog`, `Authorization: Bearer <CONTROL_TOKEN>` (константное время,
пустой токен не авторизует никого). Ответ: `{"statesOk": bool, "entries": [{id,
kind, title, room, icon, device_id, device, role, actions[], state("on"/"off",
устройства и профиль), available, light}]}`; `kind` — `scenario`, `light`,
`switch`, `profile` или `action`; порядок: профиль, сценарии, действия, устройства
по комнатам. `device_id`, `device` и `role` (`primary`/`setting`) появились
аддитивно и опускаются, пока пусты; старые клиенты их игнорируют. `light.effects`
(список `effect_list` лампы) — тоже аддитивное поле, опускается, если HA его не
отдаёт. Действия
уровня устройства приходят отдельными записями с `kind: "action"`, тем же
`device_id` и своей разрешённой командой, не меняя формат задачи
`{"entry", "action"}`; сейчас такое действие одно — `speak`. Старый клиент,
не знающий `action`, показывает запись как неизвестный kind и не действует
ею. Сервер только читает. Телефон шлёт
тот же bearer, что и планировщику (можно задать общий). Слушает `CONTROL_LISTEN`
(по умолчанию `127.0.0.1:8099`; для LAN/VPN — адрес интерфейса). TLS на
уровне процесса нет, как у scheduler: доверенная сеть LAN/VPN.

| Переменная | Обяз. | По умолчанию | Смысл |
| --- | --- | --- | --- |
| `HA_URL`, `HA_TOKEN[_FILE]`, `HA_CA_FILE` | да | — | Как у ha-bridge |
| `SCHEDULER_ENDPOINT`, `SCHEDULER_TOKEN[_FILE]`, `SCHEDULER_CA_FILE` | да | — | Как у ha-bridge |
| `CONTROL_TOKEN[_FILE]` | да | — | Bearer каталога |
| `CONTROL_CONSUMER_ID` | да | — | Стабильный id потребителя |
| `CONTROL_SCENARIO_LABEL` | нет | `scenario` | HA label id роли «сценарий» (`CONTROL_LABEL` удалена: при её наличии старт отклоняется) |
| `CONTROL_PROFILE_LABEL` | нет | `voice_profile` | HA label id роли «профиль» (вид `profile` для `switch`) |
| `CONTROL_EXCLUDE` | нет | — | Маски entity id через запятую |
| `CONTROL_LISTEN` | нет | `127.0.0.1:8099` | Адрес каталога |
| `CONTROL_QUEUE` | нет | `control` | Очередь команд (не `home`/`tasks`) |
| `CONTROL_EXECUTE` | нет | `false` | `true` включает исполнение очереди; без него только каталог |
| `CONTROL_CACHE_TTL`, `CONTROL_POLL_INTERVAL`, `CONTROL_MAX_BACKOFF`, `CONTROL_HA_TIMEOUT`, `CONTROL_SCHEDULER_TIMEOUT` | нет | `3s`, `500ms`, `2m`, `10s`, `10s` | Тайминги |

Ограничения: телефон видит только «поставлено в очередь» (для устройств приложение
через ~1.5 с перечитывает реальное состояние); ledger для дедупа нет, поэтому
повторная доставка может повторить `turn_on`/сценарий. В домашнем проекте
развёрнут только каталог (`CONTROL_EXECUTE=false`).

```bash
CONTROL_EXECUTE=true cargo run -p ha-bridge --bin ha-control
bazel test //solutions/ha-bridge/...
```

HTTP-сервер ограничен таймаутом запроса (30 s), а каталог режет Title/Room по
символам, не по байтам: кириллица больше не превращается в U+FFFD. `Light.Mode`
определяется до текущих значений, поэтому лампа в режиме `color` не отдаёт
устаревший `kelvin`.

### Настройка света

Возможности светильника не перечислены вручную: берутся из атрибутов HA
(`supported_color_modes`, `min/max_color_temp_kelvin`, текущие `brightness`,
`color_temp_kelvin`, `hs_color`, `effect_list`). Каталог отдаёт `light` только для
регулируемых ламп: `brightness`, `colorTemp` (+ диапазон Кельвинов), `color`,
`effects` (список `effect_list`, если интеграция его сообщает) и текущие
значения. Лампа без цвета получает только температуру, лампа on/off — ничего.

Команда `set` приходит в payload задачи: `{"entry","action":"set","brightness":1-100,
"kelvin":N,"hue":0-360,"saturation":0-100,"effect":"Day","transition":0.5}`.
`turn_on` принимает те же необязательные поля, `turn_off` — только `transition`
(0–300 секунд, float). `effect` должен совпасть с одним из `light.effects`; если
лампа `effect_list` не отдаёт, непустая строка передаётся как есть. Исполнитель
проверяет команду по возможностям лампы из текущего каталога (температура в
диапазоне, цвет только парой hue+saturation, температура и цвет вместе нельзя,
`turn_off` не принимает регулировки) и вызывает `light.turn_on`/`light.turn_off`;
произвольные сервисы и данные недоступны.
