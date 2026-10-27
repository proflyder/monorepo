# STATE — текущее состояние (канонические факты)

## Proxmox / HAOS / K3s — рабочее состояние, 2026-10-28

> LAN-адреса ниже — публичные примеры; реальные значения только в `secrets/`.
> Приватная карта сети с реальными IP/портами — `secrets/NETWORK.md`.

- Proxmox VE `9.2.24` на внутреннем NVMe; hostname `pve`, management
  `vmbr0` на Ethernet `192.0.2.26/24`, root 64 GiB, swap 8 GiB,
  `local-lvm` около 379 GiB. SSH — отдельный ключ; PVE API/TLS проверены.
- VM `100 haos`: 2 vCPU / 4 GiB / 32 GiB, UEFI, HAOS, `192.0.2.28/24`
  (static in HAOS Supervisor); VM `101 lab-k3s`: 8 vCPU / 8 GiB / 200 GiB,
  Ubuntu 24.04.5, K3s, `192.0.2.27/24` (static netplan). Обе running,
  guest agent включён, startup order 1/2; после memory apply K3s VM
  перезагрузилась и кластер/приложения восстановились.
- Terraform `projects/home/proxmox/terraform/main.tf` (provider 0.118.1,
  Terraform 1.17.0) создал обе VM; authoritative state — root-only
  `/var/lib/ricardo-terraform/` на PVE, копия — в `secrets/`.
  `terraform plan` — **No changes**; startup order задаётся отдельным
  host bootstrap `set-startup-order.sh`.
- HAOS `2026.10.4`, Supervisor healthy. HA восстановлен из cold copy,
  `internal_url=http://192.0.2.28:8123`, trusted proxy `192.0.2.27/32`.
  HAOS доступен напрямую `192.0.2.28:8123`; K3s HAProxy удалён.
- **HAOS add-ons:** Matter Server `9.2.0` (официальный `core_matter_server`,
  остановлен до миграции, затем запущен; fabric/nodes проверены) и
  Music Assistant `2.10.8` (`d5369777_music_assistant`, host network).
  MA data скопированы из прежнего PV `runtime-music`; HA integration
  автоматически переключилась на add-on endpoint, player Sendspin
  зарегистрирован. Matter endpoint для HA — `ws://127.0.0.1:5580/ws`.
- K3s `v1.35.9+k3s1`, node `lab-host` внутри VM; namespaces
  `home-assistant`/`monitoring`/`tailscale`. В K3s: GigaAM, Whisper, Piper,
  Hermes gateway+proxy, health exporter и observability. MA/Matter
  Deployments, PV, data и images удалены из K3s; rollback-копии — на Mac.
  K3s kernel после reboot — `7.0.0-38-generic`.
- Prometheus/Grafana/Alertmanager `kube-prometheus-stack` `92.14.0`;
  Loki `20.0.2`, Alloy `1.14.3`; четыре project dashboards доставляются
  Helm ConfigMap → Grafana sidecar. Prometheus/Loki не опубликованы.
- Tailscale Operator `1.104.2` в K3s; старый host Serve заменён:
  `pl-home.<tailnet>.ts.net/` → HAOS, `/grafana/` → Grafana; DNS указывает
  на operator proxy с `tag:k8s`. Host tailscale K3s VM — `home-k3s-host`;
  Proxmox host tailscaled не запущен, поэтому его аварийный доступ сейчас
  только LAN/Ethernet.
- Hermes — revision `bf0203a794a07aac3ff764972ab9087baa4e4f4e`, собственный
  image, API extras/tini/SQLite 3.53.4; state в `/srv/k3s/volumes/hermes`.
- Functional E2E: Kubernetes runtime 5/5 (5 Deployments, 7 health endpoints),
  HA read-only 3/3, Hermes read-only 3/3 + real chat 1/1, MCP HA 73 /
  MA 86 tools. HA integration Music Assistant работает через add-on,
  Matter add-on подключён; alarm media URLs — `.28:8123`.
- Cold backup 11 PVC, host recovery, EFI и custom images сохранены на Mac
  вне Git с SHA-256; HA full backup до миграции (`pre-ma-matter-addons`),
  K3s tar `runtime-music`/`runtime-matter` и полный HA backup после
  миграции с обоими add-ons (`final-cleanup-no-proxy`) сохранены. Полный
  disaster restore на запасном железе и длительный power-loss тест ещё
  не выполнялись.
- VM `.27/.28` переведены на static внутри guests; router reservations не
  требуются для них. Открытые вопросы: переход Proxmox host на Wi-Fi
  (сейчас Ethernet), offsite backup, host Tailscale emergency access.
- Команды/пути/восстановление: [KUBERNETES-OPERATIONS.md](KUBERNETES-OPERATIONS.md);
  физическая установка — [INSTALL-PROXMOX.md](INSTALL-PROXMOX.md);
  границы IaC/Operator — [PROXMOX-HAOS-IAC.md](PROXMOX-HAOS-IAC.md).

## Kubernetes runtime — K3s VM, 2026-10-28

- Предыдущий bare-metal Ubuntu/K3s runtime выведен из эксплуатации вместе
  с системным диском; его данные восстановлены в `lab-k3s` VM. Прежние
  native/Compose описания ниже — история feature deployment.
- K3s data-dir `/srv/k3s/runtime`; application PV — Retain local PV/PVC.
  Старые каталоги `/home/labuser/nettop/...` больше не являются рабочим
  host filesystem; актуальные пути — внутри VM и в private values.
- Voice/Hermes endpoints остаются на K3s `.27`; HAOS `.28`. MA — HAOS
  add-on, HA integration использует add-on endpoint; Matter — HAOS add-on
  localhost. K3s health exporter проверяет HA `.28:8123` и MA `.28:8095`,
  voice/Hermes — `.27`; Matter из K3s больше не проверяется.
  Pulsar и BLE gateway на станции не менялись.
- `helm` releases: `runtime`, `monitoring`, `ha-observability`, `loki`,
  `alloy`, `tailscale-operator`, `home-access`; все применены и проверены.
- Host SSH/network/chrony остаются вне кластера; native Hermes units
  masked. После reboot VM/хоста нужно отдельно проверить порядок старта,
  HAOS↔K3s endpoints и operator; полный power-loss тест ещё впереди.



> Публичная копия: serial/MAC, внешний адрес и tailnet-домен заменены примерами.
> Реальные значения и исходные локальные документы — только в `secrets/`.
> LAN-адреса и remote-пути описывают стенд; перед применением адаптировать.

> Только этот файл владеет текущими фактами. Не дублировать их в README/CONTEXT.
> Команды — `OPERATIONS.md`; задачи — `PENDING.md`; хронология — `HISTORY.md`.

**Снимок:** 2026-10-28 02:35 (+05)  
**Последний статус:** Proxmox/HAOS/K3s cutover + MA/Matter HAOS add-ons;
K3s runtime 5/5, HA read-only 3/3, Hermes read-only 3/3 + real chat 1/1
(2026-10-28). Ранее: `logs/status-20261027-ricardo6.txt`; persona/music:
`logs/status-20261027-ricardo-persona.txt`; Matter:
`logs/status-20261027-matter.txt`; Xiaomi:
`logs/status-20261027-xiaomi-home.txt`; TTS:
`logs/status-20261027-puriso.txt`; wake word:
`logs/status-20261027-0308-wakeword.txt`; maintenance:
`logs/status-20261027-1410-maintenance.txt`

## Hermes — low reasoning и streaming, 2026-10-27

- Голосовой proxy v2 передаёт `model_options.reasoning_effort=low` на каждый
  chat completion, включая non-streaming. Глобальный Hermes остаётся `medium`;
  модель `gpt-5.6-luna` и права tools сохранены.
- HA `llama_cpp` entry `01M5W7PPAR84C51486PX6Q79YF`: `data.streaming=true`.
  Proxy передаёт SSE с очисткой законченных предложений, фильтрует reasoning/
  tool/status events; разделитель предложений позволяет Piper начать синтез
  до конца генерации. Piper подтвердил `supports_synthesize_streaming=true`.
- HA text → Hermes → Puriso → HTTP audio проверен: `stream_response=true`,
  ранний TTS на 2.061 с, первые 1024 аудиобайта на 2.328 с, intent-end на
  2.592 с. Это один синтетический text-запрос; не замер от конца речи до
  первого слышимого звука. Короткие ответы до порога HA 60 символов могут
  ждать завершения; новый микрофонный retest пользователем ещё не выполнен.
- Резервные копии private на lab-host:
  `~/nettop/backups/hermes-streaming-20261027/`. HA перезапущен один раз для
  смены entry data; станция/Pulsar и MA не перезапускались.
- Проверки, результаты и избирательный откат:
  [DEPLOY-HERMES-STREAMING.md](DEPLOY-HERMES-STREAMING.md).

## TTS Puriso — внедрение 2026-10-27

- Puriso установлен в существующий Piper на lab-host; default контейнера и pipeline «Рикардо» — `ru_puriso2-medium`, язык `ru`. Irina сохранена для отката.
- `alarm_announce` явно выбирает Puriso и прежний ESPHome endpoint; scripts reload выполнен.
- HA config check, pipeline TTS, local intent «какой будильник» → Puriso, отправка TTS на станцию и вызов alarm_announce пройдены. Piper healthy, satellite idle, alarm_ringing off. Слуховая приёмка пользователем и новый микрофонный retest ещё впереди.
- При последней проверке играет Spotify Connect; «включи музыку» и
  «громкость пять» проверены через pipeline (0.5), затем пользователь
  тестировал другие значения. Не останавливать музыку ради косметического
  idle-check.
- Детали, ревизия/хеши, измерения и private backup: [DEPLOY-RICARDO-PURISO.md](DEPLOY-RICARDO-PURISO.md).

## Станция

| Параметр | Значение |
|---|---|
| Модель | Yandex Station Midi, `cucumber` |
| Device ID | `STATION_SERIAL` |
| MAC | `02:00:00:00:00:02` |
| IP | `192.0.2.13/24` (wlan0, DHCP) |
| Прошивка | `0.369.1.46.4778262955.20261013.160` |
| quasar | `3.21` |
| Ядро | Linux 5.4.180, aarch64; userspace armhf |
| Корень | `/dev/root`, ext4, `ro` |
| `/ota` | `/bkupgrade`, ~1.4 ГБ |
| ADB | `:5555`, root без авторизации |
| SSH | `:22`, dropbear, root; пароль в `secrets/CREDENTIALS.md` |
| Pulsar | `v0.4.1-ricardo6`; `/data/pulsar`, `/etc/init.d/S99zpulsar`, `enabled` |
| Pulsar API | `:6053`, ESPHome Noise; HA key создан |
| Pulsar Sendspin | `:8928`; MA видит «Яндекс Станция Миди» |
| Штатный стек | `maind` остановлен; Алиса/Yandex Music/навыки не работают |

OTA/ADB fortress: `activate_adb.sh` wrapper, `rootkeeper.sh` v4, no-op
`device_ota_update.sh`, dummy `ota_script.pub`, `updatesd` вырезан.
ADB-маркеры: `/data/quasar/activate_adb_quasmodrom`,
`/data/quasar/activate_network_adb`. Проверено reboot'ом.

## Pulsar / HA Assist

- Custom wake word: модель v1 включена на станции; FULL resolver встроенной libtflite,
  threshold 0.9, окно 10; буфер 0.5 с. Пользователь подтвердил wake → ответ HA.
  Проверка на голосе — функциональная; recall/FP в быту ещё не измерены.
  База исходников — точный tag v0.4.1 (15554498), с ALSA ABI shim.
  Откат: `/data/pulsar/rollback-ricardo.sh`; исходный бинарник и state —
  `/data/pulsar/backups/pre-ricardo/`; ricardo4 — `pre-ricardo5/`;
  ricardo5 — `pre-ricardo6/`.
- В `ricardo6` локальный command spotter выключен на время wake-запроса и
  арбитража: «Рикардо, стоп» доходит до HA `StationAlarmStop`, а не
  приглушает/останавливает музыку на колонке. Без wake-слова локальные
  «стоп/пауза/громче» и barge-in по ответу/объявлению сохранены.
  Duck/resume музыки на время ответа и объявления сохранён.
- «Рикардо, отмена/передумал/не надо» — `StationCancel` в HA (ответ
  «Хорошо.», без side effects). Если STT вернул `stt-no-text-recognized`,
  Pulsar трактует это как тихую отмену: без красной ошибки, музыка
  возвращается. Это покрывает «передумал обращаться» после wake.
- Штатная Алиса **не запущена**: `pidof maind`/`quasar_launcher2` пусты,
  работает только Pulsar, `state.conf` имеет `wake_word=ricardo`. Поэтому
  Алиса не может «проснуться» на будильник или своё имя. Если при ошибке
  слышен голос Алисы — это не native assistant, а earcon
  `/system/vendor/quasar/sounds/ru-RU/vins_error.wav`, который Pulsar
  проигрывает на `VaEvent::ERROR` (ошибка/неузнанный интент HA). Проверка
  ошибки — по HA pipeline/логам, а не по голосу станции.

- Установка `pulsar-install --no-enable` на `.160`, затем `ctl.sh enable`;
  `ctl.sh disable` возвращает штатный стек, reboot — autostart.
- Отдельный Rust-форк `OWNER/pulsar` (ветка `feat/ricardo-rust`, база
  v0.5.0) склонирован в `projects/station/upstream/pulsar-rust/`; профиль
  Ricardo, cross-build aarch64/armhf и unit-тесты проходят, но на станцию **не
  установлен**. Активен C++ `v0.4.1-ricardo6`. Границы и HIL-gate —
  [`projects/station/PULSAR-RUST.md`](../projects/station/PULSAR-RUST.md).
- Железо: 4 микрофона/VQE, LED/кольцо/логотип, часы, GStreamer, hardware
  mute, кнопки/volume; HA ESPHome — 33 entities, API шифруется.
- Pipeline `Рикардо`: RU, `stt.onnx_asr` (GigaAM v3 CTC int8; Whisper
  `medium-int8` — fallback), `conversation.hermes_agent` — fallback,
  `prefer_local_intents: true`,
  `tts.piper`/`ru_puriso2-medium` (Puriso, TTS language `ru`); Pulsar назначен на `Рикардо`, wake word
  `Рикардо` (on-device microWakeWord). Локальные intents (таймеры, будильник, громкость, музыка, простой
  дом) идут первыми; Hermes получает только нераспознанное/свободную речь.
- Реальный голос «Алиса» → HA → TTS проверен. TTS/announce и `tts.speak`
  играют; MA Sendspin player зарегистрирован.
- `todo.shopping_list` aliases `список`/`покупки`; иначе STT иногда теряет
  «покупок». Список содержит один «Молоко».
- Голосовые команды: «включи/выключи блютуз», «включи/выключи гейтвей»,
  «громче/тише», «громкость 1…10», «пауза/продолжи/останови музыку»,
  голая «продолжи» распознаётся как `StationMusicResume` (проверено
  pipeline: «Продолжаю», playback resumed),
  «включи/выключи лампу» (ночник/лампа у кровати), «светильник», «ленту» —
  локальные `StationLightOn/Off` (проверено pipeline: «Выключаю лампу»),
  «я сел работать» / «включи рабочий режим» —
  `StationWorkMode` → `script.work_mode`, включает все три светильника
  спальни (проверено pipeline: «Включаю всё освещение в спальне»),
  «black out» / «blackout» / «блэкаут» / «блэк аут» —
  `StationBlackout` → `script.blackout`, выключает те же три светильника;
  ASR-вариант «блок аут» также локальный (проверено pipeline:
  «Выключаю всё освещение в спальне»),
  «следующая/следующий трек» и «предыдущая/предыдущий трек» (локально,
  без Hermes), «включи музыку» (локальный `StationMusicPlay` → Liked Songs),
  «включи любимое» (+ варианты), «включи песню/трек/альбом/плейлист/группу
  …», «второй/третий вариант», «что играет». Проверены текстом/пайплайном.
  При пустом Liked Songs «включи любимое» отвечает: «Любимый плейлист пока
  не синхронизирован…».
- Музыкальный каскад: `script.music_search` (Spotify Connect Search API или
  MA search), `script.music_play_uri`, `script.music_play_query` (top-1 +
  кандидаты), `script.music_play_candidate` («второй»), `script.music_status`.
  Spotify 429 снят 2026-10-27; поиск и play проверены (`Белые розы`,
  `Nirvana`). Исправлено чтение `tracks.items` в HA-шаблоне (было
  `TypeError: builtin_function_or_method`); `music_play_query` не выдумывает
  URI, использует только результаты search. MA success path проверен:
  `music_play_query "The Black Queen" artist` → `library://artist/54` → MA
  Queue; source/volume восстановлены.
- Громкость Connect: `volume_mode=sync_spotify`; Spotify app ↔ MA ↔ Pulsar
  синхронизированы. Voice absolute в Connect идёт через
  `spotify_connect_set_volume`: сначала локальный `media_player.volume_set`,
  затем необязательный Spotify Web API. Ошибка Spotify (429/токен) больше не
  мешает локальной громкости. Проверено: «громкость пять» → 0.5, ранее
  «громкость 3» → 0.3, «громкость 10» → 1.0. Spotify app остаётся
  авторитетным при ручной смене слайдера.
- Будильники: **список в `local_todo`**, четыре списка по повтору:
  `todo.alarms_once`, `todo.alarms_daily`, `todo.alarms_weekdays`,
  `todo.alarms_weekends`. Элемент = название + `due_datetime`; enabled —
  `needs_action`, disabled/завершён — `completed`. Относительные фразы
  «через N минут/часов» → `StationAlarmRelative` /
  `schedule_alarm_create_relative`, однократный `todo.alarms_once`; текст
  проверен, live-голос — в приёмке. Scheduler
  `schedules_alarm_dispatcher` / `script.schedule_alarm_tick` проверяет
  списки каждые 30 с и на старте HA. Голос: создать, список, изменить,
  выключить, удалить, snooze/stop. Повторы: однократно/ежедневно/будни/
  выходные; snooze 9 минут; grace 10 минут для пропущенного при простое HA.
  Звонок: `alarm_start` → `alarm_ramp` + `alarm_loop` с непрерывным
  локальным `www/alarm_ringtone.wav` (полный трек 4:54, повтор до stop);
  `alarm_stop`/`snooze` останавливают рингтон, восстанавливают source/volume
  и Spotify. `alarm_announce` (chime + Puriso TTS) оставлен отдельно для
  announce/legacy, в цикле будильника больше не используется.
- Таймеры: **8 UI-created `timer.taimer_1…8`** с `restore: true`, label
  `assistant_timer`; названия — `input_text.taimer_N_nazvanie` (созданы без
  `initial`, чтобы состояние восстанавливалось). Голос: «поставь таймер
  пицца на 10 минут», «какие таймеры», pause/resume/cancel. `timer.finished`
  озвучивается и уведомляет телефон. Встроенные Assist-таймеры Pulsar
  больше не являются основным путём.
- Напоминания: `todo.reminders` с `due_datetime`; dispatcher раз в 30 с
  озвучивает и отправляет mobile notification, затем закрывает элемент.
  Заметки: `todo.notes` без времени; голосовой CRUD/поиск.
- Dashboard `Помощник` (`/assistant-schedules`) содержит to-do cards
  будильников/напоминаний/заметок, 8 timer cards и форму нового будильника
  (название + время + повтор); due следующего срабатывания считается
  автоматически.
- Legacy single-slot alarm (`input_datetime.alarm_next`, `alarm_enabled`,
  `alarm_repeat`, `alarm_scheduler`) **выключен и скрыт**; источник истины —
  local_todo. Старые `alarm_*` scripts оставлены для rollback.
- Текущее состояние: будильников/таймеров/напоминаний нет; `alarm_volume`
  0.45; `alarm_ringing=off`, snooze off; source Connect, музыка idle.
- ASR: **GigaAM v3 CTC int8** через `wyoming-onnx-stt` (`:10305`,
  `stt.onnx_asr`); Whisper `medium-int8` остаётся fallback. GigaAM выбран
  после реальных «фатесинтус»/«фотоэссентос» от Whisper; на сохранённом
  микрофонном WAV и TTS-файле GigaAM даёт «что такое фотосинтез».
  Hermes voice prompt восстанавливает искажённые слова («Похоже, вы имели
  в виду фотосинтез…»). Реальный микрофонный retest пройден 2026-10-26.
  Debug recording (`/config/assist_pipeline`) временно включался для
  диагностики и удалён; голосовые WAV не хранятся.
- Assist satellite: HA 2026.10.4 иногда оставляет Pulsar в `responding`
  после TTS; automation `assist_satellite_responding_watchdog` через 25 с
  перезагружает ESPHome config entry, только если будильник не звонит.
- HTTP control на станции `:8099` (bearer token, LAN-only) для gateway/
  Bluetooth; HA использует `rest_command` + `intent_script`.
- После voice pipeline HA `assist_satellite` иногда остаётся `responding`;
  reload ESPHome config entry возвращает `idle`. Косметический баг.
- На `assist_satellite.announce` HA 2026.10.4 пишет
  `KeyError: MediaPlayerState.ANNOUNCING`; звук при этом играет.

## BLE Wi-Fi gateway

- Бинарник: `/data/quasar/ble-gateway/ble-gateway` (статический armhf, Go).
- Сервис: `a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d`; имя `Station-Gateway`.
- Характеристики: SSID `…4c01`, PSK `…4c02`, Token `…4c03`, Commit `…4c04`,
  Status `…4c05`.
- Токен: `/data/quasar/ble-gateway/token` (600); текущий 8-значный; старый
  отвергается. Не публиковать.
- `/dev/input/event2` читается пассивно, без `EVIOCGRAB`:
  - 1 — hardware mute (Pulsar);
  - 3 — окно gateway 15 минут;
  - 4 — Bluetooth A2DP на 90 с, затем off + `wpa_cli reconnect`;
  - 5 с — окно gateway вместо Yandex setup.
- 1/3/4/5 проверены физически. У 5 с нет звука/LED: смотреть
  `Station-Gateway`/status; BlueZ может показывать alias, искать service UUID.
- **4 нажатия могут уронить Wi-Fi/HA** на combo-чипе; после теста
  восстановление шло через BLE GATT (SSID/PSK/token/commit → `CONNECTED`).
  BLE gateway остаётся аварийным каналом.
- GATT/auto-mode/failure-rollback/reboot проверены; `rootkeeper.sh` поднимает
  gateway. Текущий Wi-Fi: id=5, `Stanislav1_Wi-Fi5`, `COMPLETED`; id=4 —
  fallback, id=0 — enabled; 1–3 disabled.

## Сервер / VM `lab-host`

- DEXP Jupiter JU01CY: i7-12650H, 16 GiB RAM, NVMe; Proxmox VE `9.2.24`
  на host, kernel `7.0.14-26-pve`; management Ethernet `192.0.2.26`.
  K3s VM `lab-k3s` — Ubuntu 24.04.5, kernel `7.0.0-38-generic`,
  8 GiB, `192.0.2.27`; HAOS VM — 4 GiB, `192.0.2.28`.
- Wi-Fi RTL8821CE на Proxmox host сейчас DOWN; PCI mapping `home-wifi`
  создан, но не передан VM. Ethernet остаётся обязательным до отдельного
  Wi-Fi/routed-проекта. Старые MOK/DKMS-заметки относятся к прежней
  Ubuntu; на PVE host не переносились.
- 2026-10-27 maintenance: systemd `259.9`; Secure Boot KEK/UEFI CA `2023`,
  dbx `20260402`; reboot OK; `UEFI db: Valid`; full backup —
  `backups/maintenance-20261027/` (1.5 ГБ, 600).
- Secure Boot PK остаётся `DO NOT TRUST - AMI Test PK`; HSI 0; отдельная
  задача, не менять без плана/физдоступа.
- K3s `lab-k3s`: HAOS `2026.10.4` отдельно в VM; MA `2.10.8`,
  Whisper `3.8.1`, Piper `2.5.2`, GigaAM в K3s; данные — Retain PV
  (`/srv/k3s/volumes/*`), доступ через kubeconfig/Helm.
- Xiaomi Home custom integration `v0.5.0` установлена и loaded
  (`custom_components/xiaomi_home`, source — HAOS `/config/custom_components`,
  commit `75b3acb`), config entry `01M5Y1M47A9VKA6Z98BEJT9FR8`
  (`Proflyder: 1629957439 [中国大陆]`). Вход Xiaomi China выполнен; импортированы:
  - `MJGJD02YL` / `yeelink.light.lamp22` — `light.yeelink_cn_919937191_lamp22_s_2`
    (Monitor Light Bar 1S; `color_temp`);
  - Mi Bedside Lamp 2 / `yeelink.light.bslamp2` —
    `light.yeelink_cn_827036414_bslamp2_s_2_light` (`color_temp`, `rgb`).
  Дубликат HomeKit Controller `MiBedsideLamp2-F723` удалён; backup
  `~/backups/ha-duplicate-lamp-20261027-114648/`. Для каждой лампы ровно одна
  `light.*`-сущность Xiaomi Home.
- Matter Server `9.2.0` — HAOS add-on `core_matter_server`, host network,
  WebSocket `ws://127.0.0.1:5580/ws`. HA integration `Matter`
  (`01M5Y0758DM9RS34XGESD2EKXG`) подключена; nodes пока 0.
  Прежний K3s `runtime-matter` удалён; rollback-копия есть на Mac.
  Matter-устройства ещё не добавлены; лента Яндекса ожидает pairing.
- STT primary: `wyoming-onnx-stt` `:10305` (GigaAM v3 CTC int8, CPU, ru);
  Whisper `medium-int8` `:10300` — fallback. HA pipeline `stt.onnx_asr`.
- Hermes Agent `v0.22.5` в K3s Pod `hermes` (gateway+voice-proxy,
  `hostNetwork`, state `/srv/k3s/volumes/runtime-hermes`), не на станции;
  OpenAI-compatible API `127.0.0.1:8642` внутри Pod, HA использует
  `192.0.2.27:8643` напрямую (nftables: только HAOS/K3s/loopback), модель `gpt-5.6-luna` через
  ChatGPT/Codex OAuth. MCP:
  - HA MCP Server `http://192.0.2.28:8123/api/mcp` — **73 tools**,
    resources и prompts (без `tools.include`), token `HERMES_HA_TOKEN`;
  - Music Assistant FastMCP plugin `http://192.0.2.28:8095/mcp/v1` — **86
    tools**, token `HERMES_MA_TOKEN`; плагин `fastmcp_server` loaded, все
    permission flags (query/control/edit/delete/debug/config/resources)
    включены, `require_auth=true`, `require_confirmation=false`;
  - `platform_toolsets.api_server=[mcp-homeassistant, mcp-music-assistant,
    web]`; `web_search` backend `openai-native`.
  Проверены `hermes mcp test` для обоих серверов; MA-запрос «перечисли
  плееры» вернул `Яндекс Станция Миди; Web (Chrome on Mac)`. Полный доступ
  осознанный; MA MCP experimental.
- Личность Hermes: `SOUL.md` и `agent.personalities.voice` в Hermes home
  называют его **Рикардо, личным ассистентом**, а не Hermes/Гермесом;
  HA `llama_cpp` prompt обновлён тем же текстом, subentry/entity title —
  `Рикардо`. Проверено: «как тебя зовут» → «Я Рикардо, ваш личный ассистент».
- HA `llama_cpp` integration добавлена как `conversation.hermes_agent`;
  base URL `http://192.0.2.27:8643/v1`, прямой вызов и веб-поиск проверены.
  Секреты — Kubernetes Secret `hermes-environment` и writable Hermes home;
  HA token — в закрытом runtime secret. Pipeline `Рикардо` проверен:
  local intent работает при остановленном Hermes; свободный вопрос уходит
  в Hermes. HA `llama_cpp` смотрит в `hermes-voice-proxy` (`:8643`): он
  убирает URL/Markdown/цитаты перед Piper TTS.
- Тесты после cutover: Kubernetes runtime 5/5, HA read-only 3/3/Hermes
  read-only 3/3 + real chat 1/1; MCP HA 73 / MA 86 tools; HA config check
  `RC=0`. Полные live audio/microphone тесты остаются отдельными.
- HAOS `:8123` напрямую, MA add-on `.28:8095`/stream `8097`/
  Sendspin `8927`; Whisper `.27:10300`, Piper `.27:10200`, GigaAM
  `.27:10305`; HA/MA/Whisper/Piper — healthy.
- **Remote access:** Tailscale Operator `1.104.2` в K3s;
  `https://pl-home.example-tailnet.ts.net/` → HAOS, `/grafana/` → Grafana.
  LAN Grafana — `http://192.0.2.27:30300/grafana/` (NodePort, login required).
  `pl-home` DNS указывает на operator proxy с `tag:k8s`; старый host Serve
  удалён из маршрута. HA MFA, external URL, trusted proxy `192.0.2.27/32`;
  `xiaomi-17` в tailnet. Публичного HA нет, `8123` не проброшен,
  ONT/AX3 не менялись. Проверено из operator tailnet: `/` и
  `/grafana/api/health` — HTTP 200; HA read-only 3/3.
  Открыты: host Tailscale emergency access, ONT WAN `22/443`, offsite backup.
- MA setup завершён: Spotify Soloist `1.3.10.13`, web player, playback
  проверен.
- HA integration Music Assistant **добавлена** (`entry_id`
  `01M5TR8D68V6CVPP45ZN0RZ46Q`): `music_assistant.play_media`,
  `media_player.iandeks_stantsiia_midi`; announce на Pulsar прошёл.
- **Spotify Connect plugin включён** (`spotify_connect`, Soloist): устройство
  `Яндекс Станция Миди` анонсируется по `_spotify-connect._tcp`;
  `getInfo` возвращает `remoteName` станции. В MA source list два источника:
  `Music Assistant Queue` и `Spotify Connect (Яндекс Станция Миди)`.
- Добавлен HA-роутер музыки: `input_select.music_route` (`Music Assistant` /
  `Spotify Connect`), voice-команды переключения, `script.music_play_favorites`.
  В режиме Connect «включи любимое» идёт через Spotify Web API
  (`PUT /me/player/play`, Liked Songs); Connect device уже paired, playback
  Liked Songs проверен. Текущая громкость `0.3` (2026-10-26 20:45).
- Connect — отдельный live-source: голосовой `play_media` вытесняет Spotify,
  а при одном аккаунте Connect и music provider делят один stream-slot.
  Connect не умеет выбирать трек/плейлист; для голосового контента нужен
  Spotify Web API или выбор в приложении.
- Spotify API 429 (Retry-After ~18.5 ч) снят 2026-10-27; Connect search/play
  проверены. Liked Songs playlist есть (`library://playlist/9`); MA sync
  отдельно не перепроверялся, поэтому «включи любимое» остаётся с fallback-
  ответом при пустом плейлисте.
- Backup проверен: compose + `.env` + HA config + MA/Whisper/Piper data.
- Детали: `projects/home-assistant/INVENTORY.md`, `projects/home-assistant/SETUP.md`, `projects/home-assistant/HA.md`,
  `projects/home-assistant/stack/README.md`.

## Сеть / карантин

- `EGRESS_GUARD`/`EGRESS_GUARD6`: loopback, private LAN, link-local,
  multicast/broadcast, DHCP, Google NTP; остальной WAN `DROP`.
- `INPUT` не ограничен; ADB/SSH/LAN работают. Яндекс-сети 10 IPv4 +
  `2a02:6b8::/32` дополнительно `REJECT`/`DROP`; `unbound` → Mac,
  `/etc/hosts` YANDEX-BLOCK, chrony → Google.
- Router DNS на Mac **не указывает** (подтверждено 2026-10-26); старый
  Mac DNS/proxy не используется и удалён с Desktop. Source —
  `archive/legacy/alice-mitm-bootstrap/`; v3rt3p source/архив —
  `archive/legacy/v3rt3p-stack/` + `repos/`; recovery — `REBUILD.md` §4.
- `:8099` (gateway control), `:6053` (Pulsar API), `:10305` (GigaAM),
  `:5555`/`:22` — только LAN, не публиковать; `:8642` — loopback; `:8643` —
  только HAOS/K3s/loopback через nftables.

## Открыто / ограничения

- Spotify MA-режим: Liked Songs sync не перепроверялся; Connect search/play
  и generic «включи музыку» работают.
- Bluetooth-пара с телефоном (4 нажатия могут уронить Wi-Fi/HA).
- Matter Server и HA integration готовы, fabric пустой; добавить Yandex LED
  strip через HA Companion (QR/код, 2.4 ГГц, Bluetooth/location).
- Custom wake word «Рикардо» включён; осталось измерить recall/FP на реальном фоне.
  Исторический план — `docs/RESEARCH-RICARDO-WAKE-WORD.md`; Spotify sync для Liked Songs.
- Connect device виден/активен; Web API play Liked Songs и volume sync
  проверены. Голосовой retest громкости/свободной речи пройден; осталось
  проверить будильник/таймер голосом и phone→speaker/transfer.
- Проверить будильник голосом на реальном Pulsar; «стоп»/stop, snooze,
  слышимость непрерывного рингтона и resume музыки.
- HA MFA/Tailscale remote access работает (не публичный); offsite backup,
  ONT WAN `22/443` hardening, API `:31116` без auth.
- Hermes: голосовой fallback, web_search и общие вопросы проверены реальным
  микрофоном 2026-10-26. ChatGPT OAuth — аккаунт пользователя; токены не
  публиковать. Личность Рикардо и музыкальный каскад проверены текстом/
  пайплайном; осталось живым микрофоном: имя, «включи музыку», «громкость
  пять», будильник/таймер, phone transfer.
- Factory reset/wipe `/data` снесёт guard/root/SSH/OTA — только с
  согласованием.
- «Рикардо» — первая on-device модель, бытовые метрики не измерены; MA требует host network/flat L2; Spotify Premium/Soloist
  имеет 429/ToS-риски.

## Ссылки

- Команды/откат — `OPERATIONS.md`; задачи — `PENDING.md`; хронология — `HISTORY.md`.
- Архитектура — `ARCHITECTURE.md`; политика — `SECURITY.md`.
- Provisioning — `PROVISIONING.md`; OTA — `OTA-FORTRESS.md`; внешние
  ресурсы — `RESOURCES.md`; исследование — `RESEARCH-OWN-ASSISTANT.md`.
- BLE gateway — `../projects/station/ble-gateway/README.md`; голосовые команды —
  `../projects/home-assistant/ha-voice/README.md`.
- Тесты — `../docs/TESTING.md`; площадь/результаты — `../reports/TEST-COVERAGE.md`.
- Hermes — `../projects/home-assistant/hermes/README.md`; MCP overlay — `../projects/home-assistant/hermes/config.fragment.yaml`.
