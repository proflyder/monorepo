# ha-bridge

Исполнитель (executor) durable-планировщика `ricardo.v1.SchedulerService` для
Home Assistant. Забирает события из очереди `home`, вызывает allowlisted HA
script и подтверждает доставку только после HTTP 2xx от Home Assistant.

Статус: **исходники и тесты реализованы; мост не задеплоен, cutover не
выполнен**. Действующим владельцем расписаний остаётся текущий HA
(`input_datetime` + `script.schedule_alarm_*` / `alarm_*` ticks). Мост не
создаёт, не включает и не импортирует расписания.

## Границы

- Планировщик владеет расписаниями, таймерами, occurrence state, lease/ack и
  очередями. Мост не хранит расписание и не имеет собственной БД.
- Home Assistant выполняет только side effect: звонок, chime, TTS, stop.
  Громкость, restore source, media player и TTS остаются в HA/Pulsar.
- Телефон — только управление состоянием на сервере; он не является
  исполнителем и не владеет будильниками.
- Мост говорит только с доменным scheduler API. В нём нет MCP, LLM,
  Hermes-интеграции и входящего HTTP. Будущая интеграция Hermes и других
  проектов — **один** расширяемый MCP gateway с доменными группами
  инструментов, а не MCP внутри моста. Сегодня MCP runtime не реализуется и
  не деплоится; Android-срезу он не нужен.
- Нет слушающего порта и нет неаутентифицированной входящей точки.

## Схема

```text
scheduler (SQLite durable outbox)
  │  Connect JSON POST /ricardo.v1.SchedulerService/ListDueEvents  (queue="home")
  │  Authorization: Bearer <scheduler token>
  ▼
ha-bridge (этот solution)  ──►  Home Assistant REST
  │  POST /api/services/script/turn_on          (Bearer <HA token>)
  │  entity_id: script.<configured>
  │  variables: label/kind/message/chime/event_id/source_id
  ▼
Connect JSON POST /ricardo.v1.SchedulerService/AckEvent
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
конфигурации. `payload_json` не читается (в proto поле объявлено `int64` —
дефект контракта) и не влияет на исполнение.

`alarm_stop` — глобальный stop для активного звонка; это согласуется с моделью
«одна активная occurrence на исполнителя». `schedule_announce` имеет
`mode: restart`, поэтому новый announce заменяет предыдущий.

## Конфигурация

Все значения читаются из окружения. Токены — только через env или файл; файл
имеет приоритет. Значения токенов никогда не логируются.

| Переменная | Обяз. | По умолчанию | Смысл |
| --- | --- | --- | --- |
| `HA_URL` | да | — | База HA, `http(s)://host:port`, без credentials/query |
| `SCHEDULER_ENDPOINT` | да | — | База scheduler; допускается суффикс `/ricardo.v1.SchedulerService` |
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
go run ./solutions/ha-bridge/cmd/ha-bridge
```

Execute:

```bash
BRIDGE_EXECUTE=true \
BRIDGE_LEDGER_PATH=/var/lib/ha-bridge/ledger.json \
go run ./solutions/ha-bridge/cmd/ha-bridge
```

Bazel:

```bash
bazel build //solutions/ha-bridge/cmd/ha-bridge
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

`go test ./solutions/ha-bridge/...` (и `-race`) покрывает:

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

Тесты не обращаются к живому scheduler, HA, Tailscale или устройству.
Процессный E2E (реальные бинарники, fake HA на loopback, временные SQLite и
ledger) — `projects/home/e2e/scheduler_smoke.py`; живой звук и cutover он не
проверяет.

## Cutover, shadow и rollback

Ни один шаг не выполняется автоматически и не заявляется как выполненный.

1. **Dry-run на целевом хосте**: мост читает `GetStatus`, проверяет HA
   reachability и имена script. Убедиться, что `pending`/`leased` не растут из-за
   другого исполнителя.
2. **Изолированная репетиция**: отдельный тестовый scheduler instance или
   тестовая очередь + отдельный `BRIDGE_CONSUMER_ID`, один тестовый timer.
   Живые расписания не участвуют. Проверить ACK DELIVERED и звук.
3. **Shadow**: полноценного shadow-режима у моста нет (dry-run не лизирует
   события). Сравнение поведение — только через тестовую очередь/инстанс и
   логи; не выдавать dry-run за теневой прогон.
4. **Cutover** — только осознанно: остановить старые HA ticks/automations,
   которые звонят по расписанию, убедиться, что остался **один** исполнитель,
   запустить мост с `BRIDGE_EXECUTE=true`, проверить первые DELIVERED. Нельзя
   держать два исполнителя на одних расписаниях.
5. **Rollback**: остановить мост (lease истечёт, неподтверждённые события
   вернутся), вернуть прежние HA automations/ticks, при необходимости остановить
   текущий звук. Ledger сохранить: он лишь подавляет повторы и не блокирует
   возврат к HA.

Импорт существующих HA-расписаний и включение/выключение живых ticks — отдельная
задача; в этом solution их нет.

## Ограничения

- `payload_json` в `DueEvent` объявлен `int64`; мост его не использует.
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
