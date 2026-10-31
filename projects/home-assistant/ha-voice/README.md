# HA voice control for Midi

Голосовые команды, которые работают через HA Assist и Pulsar:

- «включи блютуз» / «выключи блютуз»;
- «включи гейтвей» / «выключи гейтвей»;
- «громче» / «тише»;
- «пауза», «продолжи» / «продолжи музыку», «останови/выключи музыку» — MA-плеер Pulsar;
- «следующая/следующая песня/следующий трек» и «предыдущая/предыдущий трек»
  — локальный next/previous, без Hermes;
- «громкость 1…10» — абсолютная громкость (3 → 0.3, 10 → 1.0);
- «включи/выключи лампу» (ночник, лампа у кровати), «светильник», «ленту»
  — локальные `StationLightOn/Off`, без Hermes;
- «я сел работать» / «включи рабочий режим» — `StationWorkMode` →
  `script.work_mode`: включает ленту, лампу у кровати и светильник на мониторе
  в снятых рабочих параметрах — яркость 255 (100%) у всех трёх, цветовая
  температура 2816 K у ленты и 2800 K у лампы и светильника (светильник
  поддерживает только `color_temp`), у лампы также её белый режим
  `effect: Day`;
- «black out» / «blackout» / «блэкаут» / «блэк аут» — `StationBlackout` →
  `script.blackout`: выключает те же три светильника. ASR-варианты
  «блок аут», «блок-аут», «блак аут» тоже распознаются;
- «включи музыку» / «поставь музыку» / «вруби музыку» — локальный
  `StationMusicPlay` → Liked Songs, без Hermes;
- «включи любимое» и варианты — Liked Songs (`library://playlist/9`);
- «включи песню/трек/альбом/плейлист/группу …» — поиск и top-1;
- «второй» / «третий» — выбрать сохранённый кандидат;
- «что играет» — текущий трек/состояние;
- «поставь таймер на 5 минут», «поставь таймер пицца на 10 минут»,
  «какие таймеры», «поставь на паузу», «продолжи», «отмени» — durable
  таймеры `SchedulerService`; выполнение — ha-bridge;
- «поставь будильник на 7:30», «на завтра», «по будням», «какие будильники»,
  «выключи/удали будильник …», «отложи/останови/стоп» — durable расписания
  `SchedulerService`; snooze и stop управляют серверным occurrence, звук
  останавливает executor. Одно срабатывание звучит не дольше
  `RingWindow` (900 с): по истечении scheduler ставит `STOP` и звонок
  замолкает сам, без участия HA-таймера;
- «напомни завтра в 9:00 …», «какие напоминания» — one-shot расписания
  `SCHEDULE_KIND_REMINDER` в `SchedulerService`;
- «запомни …», «покажи заметки», «что я записал про …» — `todo.notes`
  (заметки остаются в Home Assistant без изменений);
- «отмена» / «отмени» / «передумал» — локальный `StationCancel`: распознанный
  запрос завершается без озвучки и без Hermes. Слова «отмена» нет в штатной
  command-модели станции, поэтому посреди уже идущего вызова без повторного
  wake word она локально не распознаётся.
  Отдельный случай — продолжение диалога: если ответ Hermes заканчивается
  вопросом, HA выставляет `continue_conversation=true` и следующий run
  намеренно обходит `prefer_local_intents` (`_intent_agent_only`), отправляя
  «отмена» сразу агенту. Это был live-дефект; теперь
  `hermes-voice-proxy` (v6) перехватывает точную нормализованную фразу
  последнего пользовательского сообщения до вызова Hermes и возвращает пустой
  SSE/JSON completion. HA отвечает `action_done` без TTS; `processed_locally`
  остаётся `false`, потому что ответ формально пришёл от conversation-агента,
  а не от локального intent. Свободные запросы и follow-up после отмены
  по-прежнему идут в Hermes.

Громкость: `volume_mode=sync_spotify` + Web API volume. Слайдер Spotify
меняет MA/Pulsar; voice «громкость 1…10» меняет MA/Pulsar и Spotify app.
`spotify_connect_set_volume` сначала делает локальный
`media_player.volume_set`, затем необязательный Spotify Web API; ошибка
Spotify (429/токен) больше не мешает локальной громкости. Проверено 5 → 0.5,
20% ↔ 0.2, 30% ↔ 0.3. ASR prompt и aliases `грокость`/`громкасть`;
observed fallback `громкая стрима` → 3.

Музыкальный каскад:

```text
включи песню X
  → custom sentence {media_type} {query}
  → script.music_play_query
      → script.music_search
          → Spotify Connect: Spotify Search API
          → Music Assistant: music_assistant.search
      → top-1 → script.music_play_uri
      → input_text.music_candidates (top-2 для «второй»)
  → ответ: «Включаю …» / 429 / auth / device / no_results
```

Spotify 429 обрабатывается голосовым ответом; поиск не выдумывает URI.
После снятия 429 проверены поиск и play (`Белые розы`, `Nirvana`).
В Spotify-ветке `music_search` поля `tracks/albums/playlists/artists`
читаются через `content.get(...).get('items')`; старый `.items` давал
`TypeError: builtin_function_or_method`. MA-ветка использует
`music_assistant.play_media`; Connect-ветка — Spotify Web API. Не смешивать:
Connect сохраняет source, MA переключает на MA Queue.

Pipeline `Рикардо` теперь использует `conversation.hermes_agent` с
`prefer_local_intents: true`: локальные команды (таймеры, будильник,
громкость, музыка, простой дом) идут первыми; свободная речь/веб-поиск —
в Hermes. Если Hermes недоступен, локальные команды должны продолжать
работать.

## STT и устойчивость

- Primary STT — **GigaAM v3 CTC int8** (`wyoming-onnx-stt`, `:10305`,
  `stt.onnx_asr`, CPU, ru). Whisper `medium-int8` (`:10300`) — fallback.
- GigaAM выбран после реальных ошибок Whisper («фатесинтус»,
  «фотоэссентос»); на сохранённых WAV он даёт «что такое фотосинтез».
- Hermes voice prompt сокращает ответ до 1–2 предложений/180 символов и
  восстанавливает искажённые слова: «Похоже, вы имели в виду фотосинтез…».
- `hermes-voice-proxy` (`:8643`) убирает URL/Markdown/цитаты перед Piper.
- Automation `assist_satellite_responding_watchdog` через 25 с
  перезагружает ESPHome config entry, если сателлит завис в `responding`
  (только при выключенном будильнике).

## Как устроено

```text
Pulsar (Алиса)
  → HA Assist pipeline «Рикардо»
  → custom_sentences/ru/station_control.yaml
  → custom_sentences/ru/station_alarm.yaml
  → custom_sentences/ru/station_schedules.yaml
  → intent_scripts.yaml
      ├── rest_command → station HTTP :8099 (gateway/bluetooth)
      ├── media_player.volume_set → Pulsar
      ├── media_player.media_pause/play/stop → MA Pulsar
      ├── browse_media → play_media Liked Songs
      ├── scheduler_* → SchedulerService Connect JSON (canonical state)
      └── alarm_* → audio executor path (start/announce/ramp/stop/restore)
```

Canonical schedule/timer/reminder state lives in `SchedulerService`, not in
HA helpers. `ha-bridge` leases `home` events and calls the HA audio scripts:
`alarm_start` for alarms, `schedule_announce` for timers/reminders, and
`alarm_stop` for scheduler STOP. HA remains the audio executor; the scheduler
never plays sound. The bridge passes `source_id`, `event_id`, `label` and
`kind` to `alarm_start`; HA stores them in `input_text.scheduler_active_*`
so voice snooze/stop target the correct schedule id without guessing.

- HTTP control слушает `0.0.0.0:8099` на станции.
- Авторизация: `Authorization: Bearer <control-token>`.
- Токен: `/data/quasar/ble-gateway/control-token` (0600) и
  `secrets.yaml` → `station_control_bearer` (полный `Bearer ...`).
- `rest_commands.yaml` использует `!secret station_control_bearer`.
- Endpoint: `GET /status`, `POST /gateway/on|off`,
  `POST /bluetooth/on|off`.

## Файлы

- `rest_commands.yaml` — station HTTP и `scheduler_rpc` Connect JSON
- `intent_scripts.yaml` — station, music, schedule intents
- `scripts.yaml` — Music, audio executor (`alarm_start` /
  `alarm_announce` / `alarm_loop` / `alarm_ramp` / `alarm_stop` /
  `alarm_restore` / `alarm_resume`), canonical `scheduler_*` /
  `schedule_*` client, notes
- `automations.yaml` — music route follows source, action-button stop,
  assist-satellite watchdog
- `schedules_dashboard.yaml` — dashboard «Помощник» (read-only pointers;
  notes remain editable)
- `test_ha_config.py` — static audio/entity/template invariants
- `test_scheduler_contract.py` — Connect JSON request/response, resolution and
  truthful-speech contract tests (fake clock/timezone, no live calls)
- `custom_sentences/ru/station_control.yaml`
- `custom_sentences/ru/station_alarm.yaml`
- `custom_sentences/ru/station_schedules.yaml` — timers/reminders/notes
- `input_select.yaml` — `music_route`
- `input_datetime.yaml` — empty mapping (kept so an existing include stays valid)
- `input_boolean.yaml` — `alarm_ringing`, `alarm_music_was_playing`
- `input_number.yaml` — `alarm_volume`, `alarm_previous_volume`
- `input_text.yaml` — `alarm_previous_source`, scheduler endpoint/active-*
- `www/alarm_chime.wav` — локальный chime для announce/таймеров
- `www/alarm_ringtone.wav` — приватный пользовательский трек (вне Git;
  перед deploy предоставить свой файл или восстановить из `secrets/local/`), повторяется
  `alarm_loop` до stop
  Источник — пользовательский MP3 `HONEYMOAN - Too Much` (4:54), сконвертирован
  целиком; файл локальный, не для публикации.

Notes stay in Home Assistant:

```text
todo.notes    # canonical notes list, editable from the dashboard
```

Установка:

HA config живёт внутри HAOS в `/config`. Копировать файлы через HA File
Editor, Samba или HAOS SSH add-on; не писать в HAOS из host. После копирования
проверить `configuration.yaml` includes и выполнить **Developer Tools → YAML →
Check configuration → Restart**.

В `configuration.yaml`:

```yaml
intent_script: !include intent_scripts.yaml
rest_command: !include rest_commands.yaml
input_select: !include input_select.yaml
input_datetime: !include input_datetime.yaml
input_boolean: !include input_boolean.yaml
input_number: !include input_number.yaml
input_text: !include input_text.yaml
```

Для dashboard добавьте в `configuration.yaml`:

```yaml
lovelace:
  mode: storage
  dashboards:
    assistant-schedules:
      mode: yaml
      title: Помощник
      icon: mdi:calendar-clock
      show_in_sidebar: true
      filename: schedules_dashboard.yaml
```

Скопируйте `schedules_dashboard.yaml` в config; после restart появится раздел
«Помощник».

Секрет (не коммитить):

```bash
# не печатать токен; сразу записать в HA secrets:
TOKEN=$(adb -s 192.0.2.13:5555 shell   cat /data/quasar/ble-gateway/control-token)
# secrets.yaml: station_control_bearer: "Bearer $TOKEN"
```

Затем в HA: Developer Tools → YAML → Restart (или перезапустить HAOS VM).

## Проверка

```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://192.0.2.13:8099/status
```

Текстовый тест через HA:

```bash
conversation.process: "включи гейтвей"
conversation.process: "выключи гейтвей"
conversation.process: "включи блютуз"
conversation.process: "выключи блютуз"
conversation.process: "громче"
conversation.process: "тише"
conversation.process: "громкость 3"
conversation.process: "громкость три"
conversation.process: "пауза"
conversation.process: "продолжи"
conversation.process: "продолжи музыку"
conversation.process: "выключи лампу"
conversation.process: "включи лампу"
conversation.process: "я сел работать"
conversation.process: "black out"
conversation.process: "включи любимое"
conversation.process: "поставь таймер на 30 секунд"
conversation.process: "что с таймером"
conversation.process: "отмени таймер"
conversation.process: "поставь будильник на 7:30 по будням"
conversation.process: "какой будильник"
conversation.process: "отмени будильник"
conversation.process: "стоп"
```

`включи блютуз` включает A2DP-приёмник на 90 секунд; discoverable
Bluetooth может уронить Wi-Fi/HA на combo-чипе. `Station-Gateway`
остаётся аварийным каналом.

`включи/выключи гейтвей` управляет ручным окном BLE на 15 минут.

Музыкальные команды используют `media_player.iandeks_stantsiia_midi` из
HA integration Music Assistant. Есть режим `Music Assistant` / `Spotify Connect`:

- «включи музыкальный ассистент» — MA-режим, Liked Songs через `play_media`;
- «включи спотифай коннект» — Connect-режим;
- «включи любимое» — MA-ветка проверяет Liked Songs и при 429 отвечает
  fallback-текстом; Connect-ветка вызывает Spotify Web API `play`.

Connect-устройство уже paired: Web API возвращает 204 и запускает Liked Songs
через Connect → Sendspin → Pulsar. Громкость в Connect идёт через Web API +
MA (`sync_spotify`), поэтому app/Pulsar не расходятся. Токен/URL/context лежат
в `secrets.yaml`, не в документации.



## Будильники, таймеры, напоминания, заметки

Текущее состояние — [STATE.md](../../../docs/STATE.md); команды — [OPERATIONS.md](../../../docs/OPERATIONS.md).
Источник истины — `ricardo.v1.SchedulerService`, не локальные todo/timer
helpers. Home Assistant остаётся звуковым исполнителем.

### Канонический путь

```text
голос / Android
  → ricardo.v1.SchedulerService (SQLite, durable outbox)
  → событие очереди home
  → ha-bridge (Connect JSON, lease/ack)
  → script.alarm_start | script.schedule_announce | script.alarm_stop
  → Pulsar / MA / TTS
```

HA-клиент использует Connect JSON:

```text
POST {scheduler_endpoint}/ricardo.v1.SchedulerService/{RPC}
Authorization: Bearer <token>
Content-Type: application/json
Connect-Protocol-Version: 1
```

Поля — protobuf JSON: lowerCamelCase, `int64` — decimal string, enum — имя
(`SCHEDULE_KIND_ALARM`, `REPEAT_KIND_WEEKDAYS`, ...). Запросы собираются
через `to_json`, ответы читаются из `content`/`status`; ошибка Connect
(`code`/`message`) никогда не превращается в успешную речь.

### Конфигурация HA

В `input_text.yaml`:

```text
input_text.scheduler_endpoint              # initial: !secret scheduler_endpoint
input_text.scheduler_active_schedule_id    # заполняет alarm_start из ha-bridge
input_text.scheduler_active_event_id
input_text.scheduler_active_label
input_text.scheduler_active_kind
```

`scheduler_endpoint` намеренно задан через `!secret`, а не литералом: `input_text`
не является restore-entity и после restart HA иначе вернулся бы к
TEST-NET-заглушке. Live-значение — реальный scheduler NodePort/LAN base URL
(в текущем deployment NodePort `30098`, например
`http://<scheduler-node>:30098`), без `/ricardo.v1...` и без завершающего
слэша.

В `secrets.yaml` (не коммитить):

```yaml
scheduler_endpoint: "http://<scheduler-node>:30098"
scheduler_bearer: "Bearer <scheduler-token>"
```

`rest_commands.yaml` → `scheduler_rpc` подставляет endpoint из input_text и
берёт токен только из `!secret scheduler_bearer`. URL/токен не попадают в
речь, шаблоны ответов и документацию.

`alarm_start` принимает от bridge `source_id`, `event_id`, `label`, `kind`;
`alarm_stop` очищает эти поля. Если bridge ещё не передал id, snooze/stop
ищет активное окно (`activeOccurrenceUnix`/`activeUntilUnix`) и при
неоднозначности не угадывает id.

### Голосовые операции

- Создание будильника: `CreateSchedule`, `kind=SCHEDULE_KIND_ALARM`,
  IANA timezone из `now().tzinfo`, локальное `WallTime`, `REPEAT_KIND_ONCE`
  + `onceDate` для однократного, `DAILY`/`WEEKDAYS`/`WEEKENDS` для повторов.
  «на завтра» и переход через полночь сохраняются; прошедшее время
  однократного переносится на следующий день.
- Создание напоминания: `SCHEDULE_KIND_REMINDER`, только `REPEAT_KIND_ONCE`.
- Таймер: `StartTimer`, `durationSeconds` — строка; 1…604800 с. Значение
  ≤ 0 по-прежнему означает 60 с; больше семи дней отклоняется с честной
  речью.
- Список: `ListSchedules`/`ListTimers`; пустой ответ — нормальный
  «Будильников нет» / «Активных таймеров нет», а не ошибка.
- Включение/выключение/удаление/изменение: id находится по точному или
  частичному label либо времени следующего срабатывания. Несколько
  совпадений → просьба уточнить, без массового удаления. Массовое действие
  выполняется только при явном «все» (отдельный slot `alarm_all`).
- Snooze: `SnoozeSchedule` (9 минут). Планировщик ставит STOP и отложенный
  FIRE. После 200 скрипт ждёт до 3 с, пока `alarm_ringing` выключится; если
  звук всё ещё идёт от **того же** captured schedule/event, вызывается только
  локальный `alarm_stop` и речь говорит «звонок остановлен локально,
  исполнитель не подтвердил». Чужой/новый звонок не глушится. При ошибке API
  речь говорит «перенос не подтверждён»: при потерянном ответе сервер мог
  успеть зафиксировать snooze.
- Stop: `StopSchedule` + такая же bounded-проверка до 3 с. Локальный
  `alarm_stop` вызывается только для того же captured schedule/event; при
  новом звонке речь говорит, что активен другой звонок. Если звонка не было
  и планировщик вернул «не найдено», останавливается только игравшая музыка
  и речь говорит «Останавливаю музыку». Речь отличает успех, локальную
  остановку, «уже не активен» и ошибку API.
- Update: `Get`-данные из `ListSchedules` → `UpdateSchedule` с
  `expectedRevision`; при `ABORTED` речь просит повторить, состояние не
  перезаписывается вслепую.
- Idempotency: `CreateSchedule`/`StartTimer` получают ключ, зафиксированный
  один раз в `variables` (`ha:alarm|timer|reminder:<timestamp с микросекундами>-<random>`),
  и повторяют его только для того же запроса. HA не предоставляет `context`
  в `variables` скрипта, поэтому ключ не строится на `context.id`.

### Dashboard

`/assistant-schedules` не содержит форм и todo/timer-редакторов для
будильников, таймеров и напоминаний: это были бы параллельные писатели.
Разделы «Будильники», «Таймеры», «Напоминания» — read-only указатели на
Android-приложение и голос. «Заметки» остаются `todo.notes` и редактируются
как раньше.

### Ограничения

- Нет recurring reminders (контракт разрешает только one-shot).
- Голос не создаёт `REPEAT_KIND_CUSTOM` с произвольным набором дней; это
  доступно Android/API. Неподдерживаемый набор не выдумывается.
- Телефонные уведомления — отдельный канал доставки, не часть scheduler.
- `schedule_announce` — one-shot: STOP таймера/напоминания не обрывает уже
  проигранный announce; bridge закрывает такое событие как skipped.
- Громкость, restore source, рингтон и TTS остаются в HA/Pulsar.

### Проверка

```bash
python3 -m unittest discover -s projects/home-assistant/ha-voice -p 'test_*.py' -v
```

Тесты не обращаются к живому HA/scheduler: Jinja рендерится с фиксированным
`Asia/Almaty`, ответы Connect подменяются словарями. Live-проверки и
включение executor — задача deployment worker.
