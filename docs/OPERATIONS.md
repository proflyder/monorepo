# OPERATIONS — runbook

Стабильные команды и откаты. Текущие факты — только в `STATE.md`.

## Доступ

```bash
adb connect 192.0.2.13:5555
adb devices
ssh root@192.0.2.13          # пароль: secrets/CREDENTIALS.md
```

ADB root без авторизации, SSH root по паролю. Только LAN; `5555/22` не
пробрасывать наружу.

## Статус

```bash
~/Desktop/yandex-station-lab/projects/station/scripts/station-status.sh
```

`alice-mitm-bootstrap` и `v3rt3p-stack` удалены с Desktop; source/архивы —
`archive/legacy/` (v3rt3p source — `archive/legacy/v3rt3p-stack/repos/`).
Старый DNS/proxy на Mac и v3rt3p-контейнеры не используются.

## Тесты

```bash
cd ~/Desktop/yandex-station-lab
make test          # hermetic, не трогает станцию/HA
make coverage      # + Go/Python coverage + reports/TEST-COVERAGE.md
make test-e2e      # live HA/BLE read-only; нужны HA_TOKEN/STATION_CONTROL_TOKEN
STATION_E2E_MUTATE=1 make test-e2e
STATION_E2E_AUDIO=1 make test-e2e   # короткий alarm + «стоп» + resume
```

Границы и флаги — `../tests/README.md`; измеренная площадь —
`../reports/TEST-COVERAGE.md`. Не коммитить токены.

## HA + MA + Matter + Whisper/Piper на неттопе

```bash
ssh labuser@192.0.2.26          # Ethernet; Wi-Fi: 192.0.2.25
cd /home/labuser/nettop
./ctl.sh status
./ctl.sh logs ha
./ctl.sh logs ma
./ctl.sh logs matter
./ctl.sh logs whisper
./ctl.sh logs piper
./ctl.sh restart ha
curl -sSL -o /dev/null -w 'HA=%{http_code}\n' http://127.0.0.1:8123/
curl -sSL -o /dev/null -w 'MA=%{http_code}\n' http://127.0.0.1:8095/
ss -lntp | grep -E ':(10200|10300|5580)\b'
```

Единая точка входа, backup/update, MA/STT/TTS и Secure Boot/MOK —
`../projects/home-assistant/stack/README.md`, `../projects/home-assistant/HA.md`,
`../projects/home-assistant/TROUBLESHOOTING.md`. HA — Container, без add-ons.

## Пользовательский голос Piper

Текущий голос/язык — [STATE.md](STATE.md). Закреплённая установка весов, SHA256, точечный откат контейнерного default, Assist и alarm options — [runbook Puriso](../projects/home-assistant/tts/puriso/README.md). Протокол внедрения — [DEPLOY-RICARDO-PURISO.md](DEPLOY-RICARDO-PURISO.md).

## Pulsar v0.4.1-ricardo6 (активный спутник)

```bash
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh status'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh log 100'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh restart'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh disable'   # вернуть сток
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh enable'    # снова Pulsar
adb -s 192.0.2.13:5555 shell '/data/pulsar/pulsar --version'
```

- HA: ESPHome `Яндекс Станция Миди`, 33 entities, API `:6053` с Noise.
- Текущее обращение и версия спутника — `STATE.md`; runbook своего wake word — `WAKE-WORD.md`.
- MA: Sendspin `:8928`, player «Яндекс Станция Миди».
- `ctl.sh disable` удаляет `/data/pulsar/enabled` и возвращает `maind`;
  `S99zpulsar` остаётся, но `boot.sh` без `enabled` ничего не делает.
- Pulsar не трогает ADB/SSH/OTA fortress и BLE gateway.
- 1/4/5 и голос проверены; HA/MA integration и Connect добавлены.
  Connect search/play работает, Spotify 429 снят. Осталось:
  Bluetooth-пара и phone→speaker transfer.

## OTA/ADB fortress

```bash
~/Desktop/yandex-station-lab/projects/station/scripts/station-status.sh
# откат OTA/ADB к штатному состоянию (осторожно)
~/Desktop/yandex-station-lab/projects/station/scripts/restore-ota-fortress.sh
```

Механизм и остаточные риски — `OTA-FORTRESS.md`.

## Egress guard

```bash
# статус
adb -s 192.0.2.13:5555 shell 'iptables -S EGRESS_GUARD; ip6tables -S EGRESS_GUARD6'

# применить / снять только guard
adb -s 192.0.2.13:5555 shell /data/quasar/egress-guard.sh
adb -s 192.0.2.13:5555 shell /data/quasar/egress-guard-off.sh

# проверить: LAN/ADB живы, Яндекс/внешний WAN нет
adb -s 192.0.2.13:5555 shell \
  'ping -c 1 -W 2 192.0.2.11; ping -c 1 -W 2 77.88.8.8'
```

Guard разрешает private LAN/DHCP/Google NTP; публичный WAN и Яндекс — `DROP`.
Снятие guard не снимает Яндекс-блок/OTA.

## Яндекс-карантин и телеметрия

```bash
# повторно применить DNS/hosts/chrony/iptables/launch.json
adb -s 192.0.2.13:5555 shell /data/quasar/yandex-block.sh
```

- `launch.json` без task `telemetry`;
- `clck/report/startup/quasar_metrics` → `204`;
- s3/cdn → `403`;
- `/check_updates` → `hasUpdate:false`.

## BLE Wi-Fi gateway (свой)

Файлы: `/data/quasar/ble-gateway/`; сервис: `Station-Gateway`; GATT:

```text
a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d
SSID   …4c01   read/write
PSK    …4c02   write
Token  …4c03   write
Commit …4c04   write
Status …4c05   read/notify
```

```bash
# запуск/остановка/статус
/data/quasar/ble-gateway/start.sh
/data/quasar/ble-gateway/stop.sh
/data/quasar/ble-gateway/status.sh

# токен (не публиковать)
cat /data/quasar/ble-gateway/token

# лог
tail -40 /data/quasar/ble-gateway/ble-gateway.log
```

Кнопки (event2 читается пассивно, без `EVIOCGRAB`):

- **1 нажатие** — hardware mute (`SW_MUTE_DEVICE`), читает Pulsar;
- **3 нажатия** — открыть/закрыть окно gateway на 15 минут;
- **4 нажатия** — Bluetooth A2DP-приёмник на 90 с (discoverable/pairable, bluealsa);
  затем off + `wpa_cli reconnect`;
- **5 секунд** — открыть окно gateway (замена Yandex setup). Звука/LED
  нет: проверять `Station-Gateway` в nRF Connect или `status`; имя может
  быть BlueZ-alias, искать service UUID `a1b2…4c5d`.

При отсутствии Wi-Fi больше 20 секунд gateway включается сам. `maind`
остановлен (Pulsar), но gateway работает независимо. 1/4/5 по смыслу
сохранены; 4/5 обслуживает gateway, а не `maind`.

**Bluetooth и Wi-Fi:** на этом combo-чипе discoverable Bluetooth может
уронить Wi-Fi/HA. После 4 нажатий окно ограничено 90 с, затем gateway
выключает discoverable и делает `wpa_cli reconnect`. Если Wi-Fi всё равно
пропал, `Station-Gateway` остаётся аварийным каналом: подключиться
nRF Connect и записать текущую сеть/токен заново.

Клиент — nRF Connect: писать **UTF-8**, кнопка **SEND** (не SAVE), затем
читать `Status`. Commit = `1`. При неудаче gateway возвращает старую сеть.

Штатный setup Яндекса (mute 5 с + «Дом с Алисой») не используется: app
колонку не видит. Factory reset — удержание mute при подключении питания.

## Голосовые команды HA

«включи/выключи блютуз», «включи/выключи гейтвей», «громче/тише»,
«громкость 1…10», «пауза/продолжи/останови музыку», «включи любимое» —
custom sentences + `intent_script` + `rest_command`/MA media_player. Голая
«продолжи» без слова «музыку» входит в `StationMusicResume`; также
«продолжить»/«возобнови». Next/previous — `StationMusicNext/Previous`,
generic «включи музыку» — `StationMusicPlay`. Лампы: «включи/выключи лампу»
(ночник/лампа у кровати), «светильник», «ленту» — локальные
`StationLightOn/Off`; проверка `conversation.process: "выключи лампу"` →
«Выключаю лампу». Сценарий «я сел работать» / «включи рабочий режим» →
`StationWorkMode` → `script.work_mode`: включает ленту, лампу у кровати и
светильник на мониторе. Обратное действие: «black out» / «blackout» /
«блэкаут» / «блэк аут» → `StationBlackout` → `script.blackout`: выключает
те же три светильника. ASR-варианты «блок аут»/«блок-аут»/«блак аут»
добавлены после реального распознавания GigaAM. Любимое проверяет
`library://playlist/9` и при пустом плейлисте отвечает fallback-текстом.
HTTP control станции `:8099`, bearer token; файлы и проверка —
`../projects/home-assistant/ha-voice/README.md`.
Токен не печатать.

## Будильники, таймеры, напоминания, заметки

Канонический план и ограничения — `PLAN-SCHEDULES.md`. Источник истины:
`local_todo` + `timer` helpers, не старый один слот.

### Dashboard

В sidebar HA: **Помощник** (`/assistant-schedules`). Там to-do cards для
будильников, напоминаний и заметок и 8 карточек таймеров. Создание,
редактирование и удаление будильника/напоминания/заметки — обычное действие
to-do card. Старые `input_datetime.alarm_next` и `alarm_scheduler` скрыты/
выключены.

На вкладке «Будильники» есть отдельная форма: название, время и повтор
(`Однократно` / `Каждый день` / `По будням` / `По выходным`) → кнопка
«Создать будильник». Дата следующего срабатывания рассчитывается
автоматически; поле «Срок выполнения» в to-do списках — техническая дата
следующего звонка, а не дедлайн. Элемент без времени не планируется.

### Будильники

Списки:

```text
todo.alarms_once
todo.alarms_daily
todo.alarms_weekdays
todo.alarms_weekends
```

Элемент: название, `due_datetime`, статус (`needs_action` = включён,
`completed` = выключен/завершён). Scheduler:
`automation.schedules_alarm_dispatcher` → `script.schedule_alarm_tick`
каждые 30 секунд и после старта HA. Звонок — прежние
`alarm_start` → `alarm_ramp` + `alarm_loop` (непрерывный локальный
`www/alarm_ringtone.wav`, полный трек 4:54, до stop) → `alarm_stop`; snooze — 9 минут,
grace — 10 минут.

Голос:

```text
поставь будильник на 7:30
поставь будильник на 7:30 на завтра
поставь будильник на 7:30 по будням
поставь будильник работа на 8:15 по будням
поставь будильник через 20 минут
заведи будильник через 2 часа
поставь будильник работа через 30 минут
какие будильники
выключи будильник работа
включи будильник работа
измени будильник работа на 8:45
удали будильник работа
удали все будильники
отложи будильник
останови будильник
стоп
```

Проверка:

```bash
conversation.process: "поставь будильник работа на 8:15 по будням"
conversation.process: "какие будильники"
conversation.process: "удали будильник работа"
```

Фразы «через N минут/часов» создают однократный будильник в
`todo.alarms_once` со срабатыванием от текущего времени; имя можно указать
перед «через» («работа через 30 минут»).

Если будильник не сработал: проверить `todo.get_items` для четырёх списков,
`automation.schedules_alarm_dispatcher`, `input_boolean.alarm_ringing` и
`input_text.tekushchii_budilnik_*`. Для сброса — `script.alarm_stop`;
старый `script.alarm_cancel` относится к legacy-слоту.

### Таймеры

8 UI-created `timer.taimer_1…8`, `restore: true`; названия —
`input_text.taimer_N_nazvanie` (скрыты в UI, состояние восстанавливается).
Голос:

```text
поставь таймер на 5 минут
поставь таймер на одну минуту
поставь таймер пицца на 10 минут
какие таймеры
поставь таймер пицца на паузу
продолжи таймер пицца
отмени таймер пицца
```

`timer.finished` → `schedule_announce` + mobile notification. Встроенные
Assist-таймеры больше не основной путь.

### Напоминания и заметки

```text
напомни завтра в 9:00 проверить духовку
какие напоминания
запомни что надо купить молоко
покажи заметки
что я записал про молоко
удали заметку про молоко
```

`todo.reminders` с `due_datetime`; dispatcher раз в 30 с озвучивает и
отправляет notification. `todo.notes` — без времени.

### Текстовые проверки

```bash
conversation.process: "какие таймеры"
conversation.process: "поставь таймер на 30 секунд"
conversation.process: "какие напоминания"
conversation.process: "покажи заметки"
```

Конфиг:

```bash
docker exec -it homeassistant hass --script check_config -c /config
./ctl.sh logs ha | tail -100
```


## DPP / Wi-Fi Easy Connect

Ключ/QR — `secrets/dpp/`; станционные файлы — `/data/quasar/dpp-*`.

```bash
# статус
adb -s 192.0.2.13:5555 shell \
  'wpa_cli -i wlan0 get dpp_config_processing; wpa_cli -i wlan0 dpp_bootstrap_info 1'

# force-listen для теста (только когда станция не критична)
adb -s 192.0.2.13:5555 shell 'touch /data/quasar/dpp-force-listen'
adb -s 192.0.2.13:5555 shell 'rm -f /data/quasar/dpp-force-listen'
adb -s 192.0.2.13:5555 shell 'wpa_cli -i wlan0 dpp_stop_listen'
```

**Ограничение:** DPP на `wlan0` не работает при активном Wi-Fi
(`vm_p2p_support==0`); `p2p0`-обход роняет `wlan0`. Не запускать p2p0 при
рабочей сети; DPP тестировать только unassociated после BLE/AP.
Детали и дизайн — `PROVISIONING.md`.

## Sound pairing

```bash
python3 projects/station/scripts/station-pairing-sound.py \
  --ssid 'MyWiFi' --password 'secret' --format wav --out /tmp/pair.wav
```

Локально, без Telegram/облака. Играть рядом с колонкой в режиме сопряжения;
случайный токен — только Wi-Fi, не аккаунт. Пароль звучит в эфире.

## STT: GigaAM / Whisper fallback

```bash
docker ps --filter name=wyoming-onnx-stt --filter name=wyoming-whisper
docker logs wyoming-onnx-stt --tail 50
docker logs wyoming-whisper --tail 50

# проверка GigaAM через HA STT API (WAV 16 kHz mono)
TOKEN=$(cat ~/.config/station/hermes-ha-token)
curl -sS -X POST http://127.0.0.1:8123/api/stt/stt.onnx_asr \
  -H "Authorization: Bearer $TOKEN" \
  -H 'X-Speech-Content: format=wav; codec=pcm; sample_rate=16000; bit_rate=16; channel=1; language=ru' \
  -H 'Content-Type: audio/wav' --data-binary @/tmp/test.wav
```

Pipeline `Рикардо` должен иметь `stt_engine=stt.onnx_asr` и
`conversation_engine=conversation.hermes_agent`. Если сателлит завис в
`responding`, watchdog перезагрузит ESPHome entry через 25 с; вручную:

```bash
curl -sS -X POST http://127.0.0.1:8123/api/services/homeassistant/reload_config_entry \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"entry_id":"01M5TAMM4T0CH5RASFJR8MSKT7"}'
```

## Xiaomi Home (неттоп/HA)

Официальная Xiaomi Home integration установлена как custom component
`v0.5.0` в `/home/labuser/nettop/homeassistant/config/custom_components/xiaomi_home`.
Source checkout: `/home/labuser/nettop/ha_xiaomi_home` (tag `v0.5.0`,
commit `75b3acb`). HACS не установлен; обновление — вручную.

```bash
cd /home/labuser/nettop/ha_xiaomi_home
git fetch --tags
git checkout v0.5.0
sudo ./install.sh /home/labuser/nettop/homeassistant/config
cd /home/labuser/nettop && ./ctl.sh restart ha
```

В HA: Settings → Devices & services → Add integration → Xiaomi Home →
EULA → вход Xiaomi → регион **China mainland** → дом/устройство.
Для `MJGJD02YL` (Mi Computer Monitor Light Bar 1S, Wi-Fi + BLE dimmer)
это первый путь. Интеграция облачная; локальный режим требует Xiaomi
Central Hub Gateway. Если модель не появляется, официального локального
API нет — остаётся ESPHome/Tasmota с разборкой/пайкой
(`vanzi11a/esphome-xiaomi_monitor_light_bar_s1`). BLE-пульт отдельно в HA не
добавляется; ESPHome-проект умеет его слушать.

**OAuth redirect:** Xiaomi Home hardcodes `http://homeassistant.local:8123`
as the registered redirect. On HA Container this name usually does not resolve
on a phone. After Xiaomi shows «Agree and link», the browser may land on
`DNS_PROBE_FINISHED_NXDOMAIN`. Tap the address bar, replace
`homeassistant.local` with `192.0.2.25` (keep `http://`, port and the full
path/query after `8123`), press Enter. The HA webhook receives the code and
the flow completes. Setting a permanent DNS/mDNS alias for
`homeassistant.local` is optional; do not change the hardcoded constant.

**Текущее состояние:** config entry `01M5Y1M47A9VKA6Z98BEJT9FR8` loaded.
Импортированы `light.yeelink_cn_919937191_lamp22_s_2` (Monitor Light Bar 1S)
и `light.yeelink_cn_827036414_bslamp2_s_2_light` (Mi Bedside Lamp 2).
Дубликат HomeKit Controller `MiBedsideLamp2-F723` удалён; backup
`~/backups/ha-duplicate-lamp-20261027-114648/`. Проверить on/off,
brightness и color temperature; BLE-пульт отдельно не добавляется.

## Matter Server (неттоп)

Matter-контроллер — отдельный контейнер `matterjs-server` в
`/home/labuser/nettop`, host network, WebSocket только на loopback.

```bash
cd /home/labuser/nettop
./ctl.sh status
./ctl.sh logs matter
curl -I http://127.0.0.1:5580/
docker compose restart matter-server
```

- URL для HA: `ws://127.0.0.1:5580/ws` (HA Container тоже host-network).
- Данные/fabric: `/home/labuser/nettop/data/matter-server`; входит в
  `./ctl.sh backup`.
- `PRIMARY_INTERFACE=wlp4s0`, `LISTEN_ADDRESS=127.0.0.1`; порт `5580`
  не публиковать в LAN/WAN. Dashboard — только локально или через SSH tunnel.
- Проверка: `docker inspect --format '{{.State.Health.Status}}' matterjs-server`
  → `healthy`; Matter Server log `Webserver listening on http://127.0.0.1:5580`.
- HA integration: Settings → Devices & services → Add integration → Matter,
  URL выше. После этого устройства добавляются через HA Companion:
  Matter → Add device → QR/11-значный код, телефон на 2.4 ГГц, Bluetooth,
  на Android — location permission. Fabric сохраняется в data volume.
- Yandex LED strip: Matter over Wi-Fi; если уже в «Дом с Алисой», нужен
  multi-fabric/share-код из приложения Яндекса либо сброс и повторное
  добавление в HA.

## Hermes / MCP (неттоп)

Всё на `lab-host`, не на станции. Секреты — в `~/.hermes/.env` (600:
`HERMES_HA_TOKEN`, `HERMES_MA_TOKEN`) и `~/.config/station/hermes-ha-token`
(600); не печатать. Hermes имеет полный доступ к HA MCP (57 tools) и MA MCP
(86 tools); оба сервера проверяются отдельно.

```bash
ssh labuser@192.0.2.25
set -a; . ~/.hermes/.env; set +a

~/.local/bin/hermes auth status openai-codex
~/.local/bin/hermes gateway status
~/.local/bin/hermes mcp list
~/.local/bin/hermes mcp test homeassistant
~/.local/bin/hermes mcp test music-assistant
~/.local/bin/hermes config get model
~/.local/bin/hermes config get web

# Persona: canonical identity, not "Hermes"
sed -n '1,20p' ~/.hermes/SOUL.md
~/.local/bin/hermes config get agent.personalities.voice
# Apply repo persona after editing projects/home-assistant/hermes/SOUL.md:
# cp projects/home-assistant/hermes/SOUL.md ~/.hermes/SOUL.md
# ~/.local/bin/hermes config set --force agent.personalities.voice \
#   "$(cat projects/home-assistant/hermes/voice_prompt.txt)"
# ~/.local/bin/hermes gateway restart

curl -sS http://127.0.0.1:8642/health
curl -sS -H "Authorization: Bearer $API_SERVER_KEY" \
  http://127.0.0.1:8642/v1/models

# HA uses the sanitizer proxy, not Hermes directly:
systemctl --user status hermes-voice-proxy
journalctl --user -u hermes-voice-proxy -n 50 --no-pager
curl -sS -H "Authorization: Bearer $API_SERVER_KEY" \
  http://127.0.0.1:8643/v1/models
```

If voice replies contain URLs/Markdown, check `hermes-voice-proxy` first.  If
HA hangs, check that `llama_cpp` base URL is
`http://127.0.0.1:8643/v1` and `streaming=true` in the **entry data**
(not just subentry options). Voice reasoning is injected by the proxy as
`model_options.reasoning_effort=low`; the global Hermes config may still show
`medium`. Streaming/rollback runbook: [DEPLOY-HERMES-STREAMING.md](DEPLOY-HERMES-STREAMING.md).
For SSH from this Mac, use `ssh -i ~/.ssh/labuser_ha labuser@192.0.2.25`.

Проверка свободной речи/веб-поиска через HA:

```bash
curl -sS -X POST http://127.0.0.1:8123/api/conversation/process \
  -H "Authorization: Bearer $(cat ~/.config/station/hermes-ha-token)" \
  -H 'Content-Type: application/json' \
  -d '{"text":"что такое фотосинтез?","language":"ru", \
       "agent_id":"conversation.hermes_agent"}'
```

Проверка личности:

```bash
curl -sS -X POST http://127.0.0.1:8123/api/conversation/process \
  -H "Authorization: Bearer $(cat ~/.config/station/hermes-ha-token)" \
  -H 'Content-Type: application/json' \
  -d '{"text":"как тебя зовут?","language":"ru", \
       "agent_id":"conversation.hermes_agent"}'
# Ожидается: «Я Рикардо, ваш личный ассистент».
```

Pipeline `Рикардо` использует `conversation.hermes_agent` и
`prefer_local_intents: true`; локальные таймеры/будильник/громкость идут
первыми. Если Hermes недоступен, эти локальные команды не должны зависеть
от него. MCP-серверы и настройки — `projects/home-assistant/hermes/config.fragment.yaml`.

MA MCP включён как плагин Music Assistant `MCP Server`
(`Settings → Providers`): endpoint `http://127.0.0.1:8095/mcp/v1`, все
permission flags, token `MCP — Hermes` в `HERMES_MA_TOKEN`. Плагин
experimental; откат — удалить `mcp_servers.music-assistant` из Hermes и
выключить плагин. Проверка:

```bash
set -a; . ~/.hermes/.env; set +a
hermes mcp test music-assistant     # 86 tools, all
hermes mcp test homeassistant       # 57 tools, all
```

```bash
# после изменения конфига Hermes
~/.local/bin/hermes gateway restart
# после изменения HA scripts/intents
cd /home/labuser/nettop && ./ctl.sh restart ha
```

Если музыкальный сервис отвечает ошибкой, сначала проверить
`script.music_search` с `return_response`: в Spotify-ветке поля
`tracks/albums/playlists/artists` должны читаться через
`content.get(...).get('items')`; старый `.items` даёт
`TypeError: builtin_function_or_method`. Локальное «включи музыку» идёт в
`StationMusicPlay` и не зависит от Hermes. Громкость `spotify_connect_set_volume`
сначала меняет Pulsar, затем необязательно Spotify Web API.

## Голос / LLM (исторический emulator)

Локальный `v3rt3p-stack` остановлен и удалён 2026-10-26. Его compose,
prompt-шаблоны и switch-скрипты — в `archive/legacy/v3rt3p-stack/`;
source repos — `archive/legacy/v3rt3p-stack/repos/` (см. `UPSTREAM.md`);
upstream — `https://github.com/v3rt3p`. Не запускать без отдельной задачи
и свежего review.

## Стек Docker

Локальный v3rt3p больше не поднимается. Если понадобится другой
Docker-проект — `colima start`, затем `docker compose` в его каталоге.
Старые образы v3rt3p могут оставаться в Colima; это не влияет на Desktop.

## DNS / локальный прокси на Mac (архив)

Старый `alice-mitm-bootstrap` удалён с Desktop 2026-10-26; DNS на Mac
больше не используется. Исходники/конфиги сохранены в
`archive/legacy/alice-mitm-bootstrap/` и
`artifacts/desktop-originals/alice-mitm-bootstrap-20261026.tar.gz`
(`.env` — в `secrets/legacy-alice-mitm/`, не публиковать). Не запускать
без отдельной задачи.

## После ребута Mac

1. `adb connect 192.0.2.13:5555`.
2. При выключенном Mac локальной Алисы нет; станция доступна по IP из LAN.
3. v3rt3p/DNS на Mac не запускать; это исторический стенд.

## Роутер и откат

- DNS на роутере на Mac больше не указывает; старый Mac DNS/proxy не
  используется. Возврат DNS на `192.0.2.1` сам по себе **не** возвращает
  станцию в Яндекс: есть guard и `yandex-block`.
- Полный возврат к Яндексу: снять `egress-guard-off.sh`, затем
  `yandex-block.sh`/iptables/hosts/unbound — только осознанно.
- Factory reset — не способ отката: см. `PENDING.md` и `OTA-FORTRESS.md`.

## Логи

```bash
adb -s 192.0.2.13:5555 shell 'logread | tail -50'
adb -s 192.0.2.13:5555 shell 'iptables -L EGRESS_GUARD -v -n'
```

## «Рикардо»: возврат к исходному Pulsar

```bash
adb -s 192.0.2.13:5555 shell /data/pulsar/rollback-ricardo.sh
```

Скрипт возвращает оригинальный бинарник из `/data/pulsar/backups/pre-ricardo/`,
ставит `wake_word=alisa` и перезапускает Pulsar. Для обычной смены обращения
использовать select wake word в HA.
