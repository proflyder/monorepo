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
  `script.work_mode`: включает ленту, лампу у кровати и светильник на мониторе;
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
  «какие таймеры», «поставь на паузу», «продолжи», «отмени» — 8 UI
  `timer.taimer_*` с restore; видны в dashboard «Помощник»;
- «поставь будильник на 7:30», «на завтра», «по будням», «какие будильники»,
  «выключи/удали будильник …», «отложи/останови/стоп» — списки `local_todo`,
  snooze 9 минут, непрерывный рингтон;
- «напомни завтра в 9:00 …», «какие напоминания» — `todo.reminders`;
- «запомни …», «покажи заметки», «что я записал про …» — `todo.notes`.

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
После снятия 429 2026-10-27 проверены поиск и play (`Белые розы`, `Nirvana`).
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
      ├── schedule_* → local_todo / timer helpers
      └── alarm_* → legacy ring path (start/announce/ramp/stop/restore)
```

- HTTP control слушает `0.0.0.0:8099` на станции.
- Авторизация: `Authorization: Bearer <control-token>`.
- Токен: `/data/quasar/ble-gateway/control-token` (0600) и
  `secrets.yaml` → `station_control_bearer` (полный `Bearer ...`).
- `rest_commands.yaml` использует `!secret station_control_bearer`.
- Endpoint: `GET /status`, `POST /gateway/on|off`,
  `POST /bluetooth/on|off`.

## Файлы

- `rest_commands.yaml`
- `intent_scripts.yaml` — station, music, schedule intents
- `scripts.yaml` — Music + legacy `alarm_*` ring path + `schedule_*`
- `automations.yaml` — route follows source + `schedules_*`
- `schedules_dashboard.yaml` — dashboard «Помощник»
- `custom_sentences/ru/station_control.yaml`
- `custom_sentences/ru/station_alarm.yaml`
- `custom_sentences/ru/station_schedules.yaml` — timers/reminders/notes
- `input_select.yaml` — `music_route` (legacy `alarm_repeat` скрыт)
- `input_datetime.yaml` — legacy `alarm_next`, `alarm_snooze_until`
- `input_boolean.yaml` — `alarm_ringing`, `alarm_snooze_active`,
  `alarm_music_was_playing` (legacy `alarm_enabled` скрыт)
- `input_number.yaml` — `alarm_volume`, `alarm_previous_volume`
- `input_text.yaml` — `alarm_previous_source` (внутренний; UI-created labels
  создаются отдельно)
- `www/alarm_chime.wav` — локальный chime для announce/таймеров
- `www/alarm_ringtone.wav` — приватный пользовательский трек (вне Git;
  перед deploy предоставить свой файл или восстановить из `secrets/local/`), повторяется
  `alarm_loop` до stop
  Источник — пользовательский MP3 `HONEYMOAN - Too Much` (4:54), сконвертирован
  целиком; файл локальный, не для публикации.

UI-created (не в `input_*.yaml`):

```text
todo.alarms_once / alarms_daily / alarms_weekdays / alarms_weekends
todo.reminders / todo.notes
timer.taimer_1 … timer.taimer_8
input_text.taimer_N_nazvanie  (скрыты)
input_text.tekushchii_budilnik_* (скрыты)
```

Установка на сервер:

```bash
scp -r projects/home-assistant/ha-voice/* labuser@192.0.2.25:/tmp/ha-voice/
ssh labuser@192.0.2.25
sudo cp /tmp/ha-voice/rest_commands.yaml \
  /home/labuser/nettop/homeassistant/config/
sudo cp /tmp/ha-voice/intent_scripts.yaml \
  /home/labuser/nettop/homeassistant/config/
sudo cp /tmp/ha-voice/scripts.yaml \
  /home/labuser/nettop/homeassistant/config/
sudo cp /tmp/ha-voice/input_select.yaml \
  /home/labuser/nettop/homeassistant/config/
sudo cp /tmp/ha-voice/input_datetime.yaml \
  /home/labuser/nettop/homeassistant/config/
sudo cp /tmp/ha-voice/input_boolean.yaml \
  /home/labuser/nettop/homeassistant/config/
sudo cp /tmp/ha-voice/input_number.yaml \
  /home/labuser/nettop/homeassistant/config/
sudo cp /tmp/ha-voice/input_text.yaml \
  /home/labuser/nettop/homeassistant/config/
sudo cp /tmp/ha-voice/automations.yaml \
  /home/labuser/nettop/homeassistant/config/
sudo mkdir -p /home/labuser/nettop/homeassistant/config/custom_sentences/ru
sudo mkdir -p /home/labuser/nettop/homeassistant/config/www
sudo cp /tmp/ha-voice/custom_sentences/ru/station_control.yaml \
  /home/labuser/nettop/homeassistant/config/custom_sentences/ru/
sudo cp /tmp/ha-voice/custom_sentences/ru/station_alarm.yaml \
  /home/labuser/nettop/homeassistant/config/custom_sentences/ru/
sudo cp /tmp/ha-voice/www/alarm_chime.wav \
  /home/labuser/nettop/homeassistant/config/www/
sudo cp /tmp/ha-voice/www/alarm_ringtone.wav \
  /home/labuser/nettop/homeassistant/config/www/
```

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

Затем `./ctl.sh restart ha`.

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

Канонический план — [PLAN-SCHEDULES.md](../../../docs/PLAN-SCHEDULES.md).
Источник истины — `local_todo` и `timer` helpers, не старый один слот.

- Будильники: `todo.alarms_once/daily/weekdays/weekends`; scheduler
  `automation.schedules_alarm_dispatcher` → `script.schedule_alarm_tick`
  каждые 30 s и на старте HA. Элемент = название + `due_datetime`; статус
  `needs_action` = включён, `completed` = выключен/завершён.
- Таймеры: `timer.taimer_1…8` с `restore: true`; название —
  `input_text.taimer_N_nazvanie`; `timer.finished` → TTS + phone.
- Напоминания/заметки: `todo.reminders` / `todo.notes`.
- Dashboard: `/assistant-schedules`, «Помощник».
- Звонок: `alarm_start` → `alarm_ramp` + `alarm_loop`; непрерывный
  `alarm_ringtone.wav` идёт на ESPHome endpoint, чтобы не менять source
  universal MA player и не ломать resume. `alarm_stop`/snooze прерывают
  рингтон; `alarm_announce` (chime + TTS) оставлен отдельно.
- Старый `alarm_scheduler`/`alarm_next`/`alarm_enabled`/`alarm_repeat`
  выключен и скрыт; legacy `alarm_*` scripts оставлены для rollback.
- Snooze 9 минут, grace 10 минут, target `alarm_volume=0.45`.

Голосовые примеры:

```text
поставь будильник на 7:30 по будням
поставь будильник работа на 8:15 по будням
какие будильники
измени будильник работа на 8:45
включи будильник работа
выключи будильник работа
удали будильник работа
удали все будильники
поставь таймер пицца на 10 минут
какие таймеры
отмени таймер пицца
напомни завтра в 9:00 проверить духовку
запомни что надо купить молоко
покажи заметки
```
