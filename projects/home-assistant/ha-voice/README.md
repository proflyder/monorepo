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
  `script.scenario_run` с меткой «Сел работать»: нативный сценарий scheduler
  (`home.v1.ScenarioService`) включает ленту, лампу у кровати и светильник на
  мониторе в снятых рабочих параметрах — яркость 100% у всех трёх, цветовая
  температура 2816 K у ленты и 2800 K у лампы и светильника (светильник
  поддерживает только `color_temp`), у лампы также её белый режим
  `effect: Day`. Определение — `scheduler/scenarios/work_mode.json`;
- «black out» / «blackout» / «блэкаут» / «блэк аут» — `StationBlackout` →
  `script.scenario_run` с меткой «Блэкаут»: нативный сценарий scheduler
  выключает те же три светильника. ASR-варианты «блок аут», «блок-аут»,
  «блак аут» тоже распознаются. Определение —
  `scheduler/scenarios/blackout.json`;
- «рабочий профиль» / «включи рабочий профиль» / «переключи на рабочий
  профиль» — локальный `StationVoiceProfileWork` → `Рабочий` в
  `input_select.ricardo_voice_profile`; «личный профиль» и аналоги —
  `StationVoiceProfilePersonal`; «какой профиль» / «какой сейчас профиль» /
  «какой профиль включён» — локальный `StationVoiceProfileQuery`. Профиль
  влияет только на свободную речь (Hermes или рабочий тред), локальные
  бытовые команды работают в обоих профилях. Автовозврата нет; HA
  восстанавливает последнее значение после перезапуска, первый старт —
  «Личный»;
- «Рикардо: ложное срабатывание» — сценарий `script.wake_false_alarm`
  (метка HA `scenario`, запуск из приложения): нажимает
  `button.iandeks_stantsiia_midi_wake_lozhnoe_srabatyvanie` и помечает
  последнее срабатывание Рикардо за 10 минут как ложное;
- «Рикардо: не услышала» — сценарий `script.wake_missed` (метка HA
  `scenario`, запуск из приложения): нажимает
  `button.iandeks_stantsiia_midi_wake_ne_uslyshala` и сохраняет последние
  30 с микрофона станции как пропущенное срабатывание для обучения. Жать
  в течение ~20 с после неотвеченного «Рикардо», пока момент обращения
  ещё попадает в окно;
- «включи музыку» / «поставь музыку» / «вруби музыку» — локальный
  `StationMusicPlay` → Liked Songs, без Hermes;
- «включи любимое» и варианты — Liked Songs (`library://playlist/9`);
- «включи песню/трек/альбом/плейлист/группу …» — поиск и top-1;
- «второй» / «третий» — выбрать сохранённый кандидат;
- «что играет» — текущий трек/состояние;
- «курс доллара», «курс рубля», «какой курс доллара», «сколько стоит
  доллар/рубль», «курс валют» (оба), «доллар/рубль к тенге» — локальный
  `StationCurrencyRate` → `script.currency_rates` → `GET /api/latest`
  currency-bot по LAN NodePort. Ответ в тенге: «купить» озвучивает поле
  `sell` (курс, по которому валюту покупаете вы), «продать» — поле `buy`
  (курс, по которому вы продаёте). USD — 1 знак, RUB — 2, десятичная запятая;
  если снимок старше 3 ч, добавляется «Данные от HH:MM» (Asia/Almaty).
  Ошибка API — только «Не удалось получить курс.», без деталей;
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
- «включи комп» / «включи мой комп» / «включи компьютер» — локальный
  `KompOn` → `switch.turn_on` `switch.komp` (одноразовый logon-пропуск
  wakegrant, best-effort, и Wake-on-LAN; MAC только из `!secret komp_mac`);
  «разблокируй комп» / «разблокируй мой комп» / «открой комп» — `KompUnlock`
  (unlock-пропуск для работающего заблокированного ПК, без WoL); «заблокируй
  комп» / «заблокируй мой комп» / «запри комп» — `KompLock` →
  `script.komp_lock` (SSH → задача `\Home\KompLock` блокирует экран); «выключи
  комп» и варианты — `KompOff` →
  `script.komp_shutdown` (SSH → Windows `shutdown /s /t 60`); «отмени
  выключение компа» / «не выключай комп» — `KompCancelOff`; «комп включён»,
  «включён ли комп», «комп работает» — `KompStatus` (ping-сенсор). Команды
  локальные, работают в обоих профилях и без Hermes; подробности ниже;
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
      ├── scenario_run → ScenarioService Connect JSON (native scenarios)
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

## Бытовые сценарии: нативные определения scheduler

«Блэкаут» и «Сел работать» — не HA-скрипты, а определения
`home.v1.ScenarioService`: scheduler хранит их, `ha-scenario` исполняет шаги
`catalog` из живого каталога HA. Голос не выбирает светильники и не пишет
определения: `script.scenario_run` вызывает `ListScenarios`, находит сценарий
по точной метке (без учёта регистра) и ставит один `RunScenario` с
idempotency key от `context.id`. Метка не найдена или scheduler недоступен —
только `system_log` warning, без угадывания id и без действий в HA.

```text
«блэкаут» → StationBlackout → script.scenario_run(label="Блэкаут")
  → ListScenarios → RunScenario → scenario.fire (очередь scenario)
  → ha-scenario → catalog turn_off ×3
```

Определения и идемпотентный сид — `projects/home/kubernetes/scheduler/scenarios/`
(`blackout.json`, `work_mode.json`, `seed.mjs`). Сид создаёт только
отсутствующие по метке сценарии и никогда не перезаписывает существующие:
владелец может менять их в приложении. Эти нативные сценарии не попадают в
каталог `ha-control` (`GET /v1/catalog` строится только из HA), поэтому в
разделе «Сценарии» приложения они появятся только после клиента, читающего
`ScenarioService`; там сейчас остаются HA-скрипты с меткой `scenario`
(`wake_false_alarm`, `wake_missed`).

## Файлы

- `rest_commands.yaml` — station HTTP, `scheduler_rpc` Connect JSON,
  `currency_latest` (Bearer)
- `shell_commands.yaml` — `komp_shutdown` / `komp_cancel_shutdown` /
  `komp_lock` (ограниченный SSH на Windows-ПК; адрес из
  `input_text.komp_ssh_target`)
- `command_line.yaml` — `binary_sensor.komp_v_seti` (ping компа каждые 10 с)
- `intent_scripts.yaml` — station, music, schedule, currency intents
- `scripts.yaml` — Music, audio executor (`alarm_start` /
  `alarm_announce` / `alarm_loop` / `alarm_ramp` / `alarm_stop` /
  `alarm_restore` / `alarm_resume`), canonical `scheduler_*` /
  `schedule_*` client, notes, `scenario_run` (запуск нативного сценария
  scheduler по метке), HA-сценарии обучения Рикардо (`wake_false_alarm`,
  `wake_missed`; они остаются скриптами с меткой `scenario`)
- `automations.yaml` — music route follows source, action-button stop,
  assist-satellite watchdog
- `schedules_dashboard.yaml` — dashboard «Помощник» (read-only pointers;
  notes remain editable; отдельный view «Голос» с профилем)
- `test_ha_config.py` — static audio/entity/template invariants
- `test_scheduler_contract.py` — Connect JSON request/response, resolution and
  truthful-speech contract tests (fake clock/timezone, no live calls)
- `custom_sentences/ru/station_control.yaml`
- `custom_sentences/ru/station_alarm.yaml`
- `custom_sentences/ru/station_schedules.yaml` — timers/reminders/notes
- `custom_sentences/ru/station_currency.yaml` — курсы валют
- `custom_sentences/ru/station_komp.yaml` — включение/выключение/блокировка/разблокировка/статус компа
- `input_select.yaml` — `music_route`, `ricardo_voice_profile`
- `templates.yaml` — `switch.ricardo_work_profile` (обёртка над
  `input_select.ricardo_voice_profile`: on = Рабочий, off = Личный; источник
  истины — `input_select`), `switch.komp` и кнопки `button.komp_lock` /
  `button.komp_unlock` группы «Комп»
- `input_datetime.yaml` — `komp_off_requested` (когда запрошено выключение компа)
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

В репозитории нет копии `configuration.yaml`: он живёт только внутри HAOS,
поэтому includes ниже добавляются вручную один раз.

В `configuration.yaml`:

```yaml
intent_script: !include intent_scripts.yaml
rest_command: !include rest_commands.yaml
input_select: !include input_select.yaml
input_datetime: !include input_datetime.yaml
input_boolean: !include input_boolean.yaml
input_number: !include input_number.yaml
input_text: !include input_text.yaml
template: !include templates.yaml
# Комп (Wake-on-LAN + ping + ограниченный SSH):
wake_on_lan:
command_line: !include command_line.yaml
shell_command: !include shell_commands.yaml
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

Цвет кольца Pulsar по профилю (Violet = личный, Blue = рабочий) задаёт
автоматизация `voice_profile_station_color` в `automations.yaml`: при смене
`input_select.ricardo_voice_profile`, при старте HA и когда select станции
появляется после недоступности она выставляет
`select.iandeks_stantsiia_midi_voice_color`. Select «Voice color» приходит с
релизом Pulsar; по умолчанию станция показывает прежний фиолетовый, а пока
сущности нет, автоматизация ничего не делает. Менять `light.ring`/`led_show`
для этого бессмысленно — во время LISTENING их перекрывает voice state
прошивки.

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
conversation.process: "курс доллара"
conversation.process: "курс валют"
conversation.process: "включи комп"
conversation.process: "комп включён"
conversation.process: "выключи комп"
conversation.process: "отмени выключение компа"
conversation.process: "заблокируй комп"
conversation.process: "разблокируй комп"
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



## Комп (Windows-ПК)

Проводной Windows-десктоп «комп» (алиас «мой комп»): включение Wake-on-LAN,
выключение через ограниченный SSH, состояние по ping. Настройка Windows и
проверка — `projects/home/windows-pc/README.md`.

```text
binary_sensor.komp_v_seti   command_line: busybox ping -c 1 -W 1, 10 с
switch.komp                 template switch, state = binary_sensor
  turn_on  → если ПК выключен/выключается:
               rest_command.wakegrant_issue (logon, best-effort)
             → wake_on_lan.send_magic_packet (mac из !secret komp_mac)
  turn_off → script.komp_shutdown
script.komp_shutdown        → input_datetime.komp_off_requested = now()
                            → shell_command.komp_shutdown  (SSH: shutdown)
script.komp_cancel_shutdown → shell_command.komp_cancel_shutdown (SSH: cancel)
script.komp_lock            → sensor on и адрес SSH задан
                            → shell_command.komp_lock (SSH: lock)
script.komp_unlock          → sensor on → rest_command.wakegrant_issue_unlock
button.komp_lock            → script.komp_lock    (кнопка группы «Комп»)
button.komp_unlock          → script.komp_unlock  (кнопка группы «Комп»)
```

```text
KompOn      sensor off или выключение запрошено < 5 мин
            → switch.turn_on switch.komp, «Включаю комп. Он загрузится примерно за минуту.»
            иначе «Комп уже включён.», без действий
KompOff     выключение уже запрошено < 2 мин → «Комп уже выключается.», без повторного SSH
            иначе sensor on → script.komp_shutdown; sensor off → «Комп уже выключен.»
KompUnlock  sensor on → script.komp_unlock → rest_command.wakegrant_issue_unlock, «Разблокирую.»
            sensor off → «Комп выключен.», ничего не выдаётся и WoL не отправляется
KompLock    sensor on → script.komp_lock → shell_command.komp_lock (SSH: lock), «Блокирую.»
            sensor off → «Комп выключен.», SSH не выполняется
KompStatus  sensor on и выключение запрошено < 5 мин → «Комп выключается.»
```

Ping остаётся `on` ещё 1–2 минуты после `shutdown /s /t 60`, поэтому одного
сенсора недостаточно: `script.komp_shutdown` пишет метку времени в
`input_datetime.komp_off_requested` до SSH, а intents считают недавний запрос
признаком «выключается». Так «выключи комп», а сразу за ним «включи комп»
снова включают ПК, а не отвечают «уже включён». Голосовое включение идёт
только через `switch.komp.turn_on`, чтобы выдавался пропуск wakegrant;
прямой `wake_on_lan` из intent этого не делал.

`command_line` не умеет склеивать `!secret` с текстом, поэтому целиком
команда ping (с адресом) лежит в `secrets.yaml` как `komp_ping_command`:
`ping -c 1 -W 1 <host> >/dev/null 2>&1 && echo 1 || echo 0`. Обёртка нужна,
иначе при недоступном хосте ping возвращает ненулевой код и сенсор остался бы `unknown`.
`shell_command`-шаблоны секретов не читают, поэтому `user@host` приходит из
`input_text.komp_ssh_target` (`initial: !secret komp_ssh_target`) — та же
схема, что у `scheduler_endpoint`. SSH-ключ и known_hosts лежат вне Git в
`/config/.ssh/komp_ed25519` и `/config/.ssh/komp_known_hosts`.

Секреты (`secrets.yaml`, не коммитить):

```yaml
komp_mac: "AA:BB:CC:DD:EE:FF"
komp_ssh_target: "<user>@<host>"
komp_ping_command: "ping -c 1 -W 1 <host> >/dev/null 2>&1 && echo 1 || echo 0"
wakegrant_endpoint: "http://<k3s-node>:30130"
wakegrant_bearer: "Bearer <issuer_token>"
```

Все ключи обязательны: `!secret` с отсутствующим ключом не даёт HA
загрузить конфигурацию, поэтому сначала добавьте секреты, потом includes.

### Вход по одноразовому пропуску (wakegrant)

`switch.komp.turn_on` сначала (только если ПК выключен или уже выключается)
вызывает `rest_command.wakegrant_issue` (`POST {wakegrant_endpoint}/v1/grants`,
тело `{"machine":"komp","purpose":"logon","ttlSeconds":180}`), затем
отправляет Wake-on-LAN. Шаг пропуска помечен `continue_on_error: true`: если
сервис пропусков недоступен, ПК всё равно включается и показывает обычный
экран входа (fail closed). `purpose` разделяет сценарии: logon-пропуск живёт
180 с и подходит только для холодного входа, unlock-пропуск (`KompUnlock`,
`ttlSeconds` 60) — только для разблокировки уже работающего ПК. Новый пропуск
заменяет старый только в пределах своей пары (machine, purpose).

`KompUnlock` вызывается только при `binary_sensor.komp_v_seti = on` и не
отправляет Wake-on-LAN. `KompLock` и `script.komp_lock` тоже работают только
при `on` и не будят ПК. SSH уходит в служебную сессию OpenSSH, поэтому
`komp-power.ps1 lock` не вызывает `LockWorkStation` напрямую, а запускает
задачу `\Home\KompLock`, созданную `projects/home/windows-pc/install-lock-task.ps1`;
задача выполняет `rundll32.exe user32.dll,LockWorkStation` в интерактивной
сессии пользователя. Логин-компонент на ПК обменивает пропуск через
`/v1/challenge` и `/v1/claim` по `packages/contracts/wakegrant/PROTOCOL.md`;
для разблокировки используется long-poll `/v1/claim` с
`{"purpose":"unlock","waitSeconds":…}`, чтобы команда «разблокируй комп»
доходила до ПК, который уже держит запрос.

`wakegrant_endpoint` — LAN base URL NodePort без завершающего слэша
(в текущем deployment `30130`), токен-издатель — только из
`!secret wakegrant_bearer` (значение с префиксом `Bearer `). Endpoint
задан через `input_text` с `initial: !secret`, как `scheduler_endpoint`,
поэтому после restart HA не возвращается к заглушке и в трекнутых файлах
нет реального адреса.

`switch.komp` — обычная `switch`-сущность без ролевых меток. Каталог
ha-control берёт все не скрытые switch'и и button'ы
(`solutions/ha-bridge/crates/ha-bridge/src/catalog/`), поэтому «Комп»
появляется в каталоге ha-control и на вкладке «Дом» в Android как штатный
on/off. Кнопки `button.komp_lock`/`button.komp_unlock` не имеют
entity-registry device, поэтому приложение группирует их вместе с
`switch.komp` по общему префиксу entity_id `komp` и показывает в настройках
устройства «Комп». `on` = сенсор ping'а `on`. В шаблонных сущностях задан
`default_entity_id`: кириллическое имя не даёт стабильный slug.

Отмена выключения должна успеть за 60 секунд (`shutdown /s /t 60` на Windows);
`shutdown /a` при отсутствии запланированного выключения просто возвращает
ошибку, HA это не озвучивает.

## Будильники, таймеры, напоминания, заметки

Текущее состояние — [STATE.md](../../../docs/STATE.md); команды — [OPERATIONS.md](../../../docs/OPERATIONS.md).
Источник истины — `home.v1.SchedulerService`, не локальные todo/timer
helpers. Home Assistant остаётся звуковым исполнителем.

### Канонический путь

```text
голос / Android
  → home.v1.SchedulerService (SQLite, durable outbox)
  → событие очереди home
  → ha-bridge (Connect JSON, lease/ack)
  → script.alarm_start | script.schedule_announce | script.alarm_stop
  → Pulsar / MA / TTS
```

HA-клиент использует Connect JSON:

```text
POST {scheduler_endpoint}/home.v1.SchedulerService/{RPC}
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
`http://<scheduler-node>:30098`), без `/home.v1...` и без завершающего
слэша.

В `secrets.yaml` (не коммитить):

```yaml
scheduler_endpoint: "http://<scheduler-node>:30098"
scheduler_bearer: "Bearer <scheduler-token>"
```

`rest_commands.yaml` → `scheduler_rpc` подставляет endpoint из input_text и
берёт токен только из `!secret scheduler_bearer`. URL/токен не попадают в
речь, шаблоны ответов и документацию.

Курс валют использует ту же схему: `input_text.currency_bot_endpoint` —
LAN base URL currency-bot NodePort без `/api`, токен — только из `!secret`.
В `secrets.yaml` (не коммитить):

```yaml
currency_bot_endpoint: "http://<k3s-node>:<CURRENCY_BOT_NODE_PORT>"
currency_bot_bearer: "Bearer <currency-bot API_TOKEN>"
```

`script.currency_rates` парсит JSON `GET /api/latest`, форматирует речь и не
показывает технические ошибки. Порядок полей и число знаков закреплены
тестами `test_ha_config.py`; NodePort и nft-фильтр — в
`projects/home/kubernetes/currency-bot/`.

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
