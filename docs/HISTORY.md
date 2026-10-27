# HISTORY — хронология

Append-only. Не текущий статус; актуальные факты — `STATE.md`.
Полная старая хронология — `archive/2026-10-24-pre-cleanup/TIMELINE.md`.

## 2026-10-21/22 — root

- Factory reset на старую заводскую прошивку, восстановление доверия к eMudhra.
- Root ADB на Midi `cucumber`; ssh/dropbear, проверка ребутом.
- Первые попытки MITM/локального backend.

## 2026-10-23 — безопасная прошивка и карантин

- OTA на `.160` через `updates-config.yml`.
- OTA/ADB fortress: wrapper, keeper v4, no-op OTA applier, dummy key,
  `updatesd` вырезан.
- Полный реальный Яндекс проверен (Music/Lenta), затем решено уйти в карантин
  из-за риска remote reset/закрытия ADB.
- Карантин: DNS-спуфинг, iptables 10/10, unbound → Mac, chrony Google,
  `/etc/hosts`.
- Телеметрия вырезана; s3/cdn `403`; `/check_updates` → `hasUpdate:false`.
- Полный emergency-бэкап станции.
- Найден API `31116` без auth; проверены ресурсы; factory-образа Midi нет.

## 2026-10-24 — egress-guard, DPP, sound

- Поставлен `EGRESS_GUARD`: LAN/DHCP/NTP, остальной WAN DROP; проверен reboot
  и выключением Mac DNS/proxy.
- Guard расширен на private LAN.
- DPP/Wi-Fi Easy Connect: DER-ключ, стабильный QR, `dpp_config_processing=2`,
  boot/keeper-скрипт; reboot-тест.
- DPP с телефоном: QR распознаётся, но `wlan0` драйвер даёт
  `vm_p2p_support==0`; DPP не завершается.
- `p2p0` off-channel сработал, но уронил `wlan0`; восстановление reboot.
  p2p0 запрещён при рабочей сети.
- `quasar-connect-bot` проверен локально: `pairing.py` офлайновый, 15/15;
  добавлен локальный sound CLI.
- Документация разбита по владельцам; старые версии — в `archive/`.

## 2026-10-25 — USB-C питание

- Станция, подключённая к Mac USB-C, показывает `ya_pd` 5 В / 1.5 А,
  `MANUFACTURER=Not Supported`; dmesg перебирает 15/12/9 В, но остаётся 5 В.
- Голосовое сообщение: «не знакома с этим блоком питания»; нужен оригинальный
  адаптер 15 В / 1.5 А (22.5 Вт).
- UDC `fdd00000.crgudc2` есть, но `not attached`; Mac USB-устройство не видит.
  USB NCM не подтверждён.
- ADB/SSH по Wi-Fi при этом работают.

## 2026-10-25 — штатный BLE setup

- В `maind` найдены `BleSetupCapability`, `BluezBLEImpl`, `StartBleSetup`,
  `ConnectToNetwork`, `AddOrUpdateWiFiNetwork`, fallback на прошлую сеть.
- Официальный сценарий Midi: mute 5 с → белая пульсация → «Дом с Алисой»
  находит колонку по Bluetooth → SSID/PSK.
- Приложение «Дом с Алисой» колонку не видит; штатный путь не используется.
- Travel router и USB исключены; план: свой BLE GATT/sound →
  AP/captive portal → UART.

## 2026-10-25 — свой BLE Wi-Fi gateway

- Штатный app колонку не видит; сделан свой GATT-сервис на Go/BlueZ:
  `Station-Gateway`, SSID/PSK/token/commit/status, token.
- Кнопка mute: 3 коротких нажатия → окно 15 минут; `maind` не останавливается,
  event2 не захватывается; 1/4/5 работают.
- Авто-включение при отсутствии Wi-Fi >20 с; сервис стартует из `rootkeeper`.
- Проверено: реклама, GATT, запись SSID/PSK/token/commit, текущая сеть
  подключилась (`CONNECTED`), `save_config`, reboot.
- Текущая сеть id=4; старая id=0 снова enabled как fallback.
- Failure-rollback проверен: фейковый SSID → `ERROR:TIMEOUT` за 20 с,
  восстановлен id=4; после отката старые enabled-профили включаются снова.
- Авто-режим проверен: `wpa_cli disconnect` → BLE ON через 20 с →
  reconnect → BLE OFF.
- Токен сменён на 8-значный; старый отвергнут (`ERROR:BAD_TOKEN`), новый
  принят (`DRY_RUN_OK`).
- Осталось: Web UI (опционально), sound/AP.

## 2026-10-25 — диагностика зависания Алисы

- Обращение к Алисе доходит до `maind` (`Start alice request`,
  `onStartVinsRequest Vins.TextInput`), распознаётся и уходит в локальный
  `processor`; ответа нет, анимация «думает» остаётся.
- Причина: `processor` вызывает LLM, но `OPENAI_API_KEY` пустой; OpenRouter
  отвечает `401 Missing Authentication header`. Ошибка ловится и не
  отправляется в `maind`, поэтому диалог не закрывается.
- Это не Яндекс и не egress: WebSocket `/uni.ws` уже идёт на Mac (`101`).
- В `processor` добавлен fallback: при ошибке LLM отправляется короткий
  `partialResponse` и сокет закрывается; проверено синтетическим запросом.
- `PROCESSOR_MCP_SERVER_URLS` добавлен в compose (по умолчанию пусто).
- Подготовлен `RESEARCH-OWN-ASSISTANT.md`: Pulsar/HA, Spotify, MCP, wake word,
  риски и поэтапный план. Переключение Pulsar не выполнялось.

## 2026-10-25 — Wi-Fi на неттопе (RTL8821CE)

- Встроенный `rtw88_8821ce` на Ubuntu 26.04 / kernel 7.0 срывался в
  `failed to send h2c command` / `firmware failed to leave lps state`.
- Параметры `disable_lps_deep`, `disable_aspm`, `disable_msi` и blacklist
  Bluetooth убрали h2c-спам, но сканирование/ассоциация оставались нестабильными.
- Установлен DKMS-драйвер `rtl8821ce` (tomaspinho); для загрузки выключен
  Secure Boot. Встроенный `rtw88` blacklisted, Bluetooth отключён.
- Wi-Fi 5 ГГц подключён, `h2c=0`; reboot и failover Ethernet↔Wi-Fi проверены.
- IP статические: Ethernet `192.0.2.26`, Wi-Fi `192.0.2.25`.

## 2026-10-25 — UBSAN в rtl8821ce

- Вендорский DKMS-драйвер `8821ce` при старте писал UBSAN
  `array-index-out-of-bounds` для flexible-array `pIE->data`/`crypt.key`.
- Исходник пропатчен: индексация через указатели, ключи через pointer arithmetic;
  DKMS пересобран, initramfs обновлён.
- После reboot: `UBSAN=0`, `h2c=0`, Wi-Fi 5 ГГц и HA работают.

## 2026-10-25 — Secure Boot + MOK

- Для DKMS `8821ce` зарегистрирован MOK-ключ
  `lab-host Secure Boot Module Signature key`.
- MOK Manager не появлялся через обычный shim; добавлен/использован
  одноразовый boot entry `MOK Manager` (`mmx64.efi`), затем удалён.
- Secure Boot включён; `8821ce` загружается, Wi-Fi 5 ГГц и HA работают.

## 2026-10-25 — неттоп lab-host и HA

- Ноутбучный сервер DEXP Jupiter JU01CY: Ubuntu 26.04.1, Docker 29.1.3,
  HA Container `2026.10.4`, onboarding пройден.
- Статические IP: Ethernet `192.0.2.26`, Wi-Fi `192.0.2.25`; failover
  Ethernet↔Wi-Fi проверен.
- Wi-Fi RTL8821CE: mainline blacklisted, DKMS `8821ce`, UBSAN-патч,
  Secure Boot + MOK.
- Воспроизводимый сетап вынесен в `projects/home-assistant/`; HA-runbook — `projects/home-assistant/HA.md`.
- Boot с вынутым Ethernet: `systemd-networkd-wait-online` ждал `enp3s0`
  120 с. Добавлен drop-in `--any --timeout=30 -i enp3s0 -i wlp4s0`;
  reboot с вынутым кабелем — SSH через ~20 с, система `running`, HA `200`.
- Shell пользователя `labuser`: установлен Oh My Zsh, `/usr/bin/zsh`
  сделан default shell; тема `robbyrussell`, плагин `git`.
- Единый стек `/home/labuser/nettop`: HA config перенесён внутрь,
  добавлен Music Assistant `2.10.8`, `ctl.sh` как единая точка входа.
  HA/MA подняты, HA `200`, MA `200`. Старый compose заархивирован,
  старый `.disabled`.
- MA setup завершён: admin, Spotify provider, Soloist `1.3.10.13`, web player.
  Spotify playback проверен через Web (Chrome on Mac): трек играл, прогресс
  шёл, пауза сработала. HA integration и Pulsar не добавлены; MA-этап закрыт.
- HA Assist base: Whisper `3.8.1` (`small-int8`, ru) + Piper `2.5.2`
  (`ru_RU-irina-medium`); pipeline `Рикардо` (HA intents). Проверены text
  intent, TTS mp3 и прямой STT-тест.

## 2026-10-26 — Pulsar, HA Assist, кнопки, голос

- Backup `/data` и `/etc`; Pulsar `v0.4.1` (armhf userspace на `.160`),
  `--no-enable`, затем `ctl.sh enable`.
- Pulsar поднял микрофоны/VQE, LED/часы, GStreamer, hardware mute; HA ESPHome
  integration добавилась по IP, 33 entities, Noise key создан.
- Pipeline `Рикардо` назначен satellite; wake word `Алиса`; TTS/announce и
  `tts.speak` играют, после reboot HA/MA переподключаются; `ctl.sh disable`
  возвращает штатный стек.
- MA автоматически увидела Sendspin-плеер «Яндекс Станция Миди».
- BLE gateway: 1 — hardware mute, 3 — gateway, 4 — Bluetooth, 5 с — gateway.
  1/3/4/5 проверены физически; 5 с открывает окно без звука/LED.
- После 4 нажатий discoverable Bluetooth уронил Wi-Fi/HA; восстановили через
  BLE GATT (SSID/PSK/token/commit → `CONNECTED`). Окно 4 нажатия ограничено
  90 с + `wpa_cli reconnect`; BLE gateway — аварийный канал.
- В BLE-сканере устройство может называться `My Favourite Yandex Station
  No. 18921` (BlueZ alias), но service UUID — наш `a1b2…4c5d`.
- Pulsar услышал «Алиса», поднял pipeline, Whisper/HA/TTS отработали;
  первый intent не нашёл «список покупок», потому что STT услышал «список».
  У `todo.shopping_list` добавлены aliases `список`/`покупки`; retest —
  «Добавил молоко».
- Добавлены голосовые команды «включи/выключи блютуз»,
  «включи/выключи гейтвей», «громче/тише»: HA custom sentences +
  `intent_script` + `rest_command`; BLE gateway HTTP `:8099` с bearer token.
- Текстовые intent-тесты прошли; громкость 0.5 → 0.55 → 0.6 → 0.55;
  пользователь подтвердил работу голосом.
- HA integration Music Assistant добавлена: `music_assistant.play_media`,
  `media_player.iandeks_stantsiia_midi`; announce на Pulsar прошёл.
- Spotify API вернул 429 `Retry-After` ~18.5 ч; Liked Songs
  (`library://playlist/9`) есть, но треки пусты, `play_media` —
  `There is nothing to play here`.
- Добавлены голосовые «пауза/продолжи/останови/выключи музыку»; текстовые
  тесты прошли. «Продолжи» на idle теперь не роняет intent.
- Добавлена «включи любимое» (+ варианты). Скрипт сначала проверяет
  `library://playlist/9` через `browse_media`; при пустом Liked Songs отвечает
  «Любимый плейлист пока не синхронизирован…», а не молчит/падает. После
  Spotify sync эта же фраза запускает `music_assistant.play_media`.
- Spotify Connect plugin добавлен (Soloist, target `up77eb9a80`, name
  «Яндекс Станция Миди»); mDNS `_spotify-connect._tcp` и `getInfo` проверены.
  На тот момент phone pairing ещё не был выполнен; source list MA уже
  содержал `Spotify Connect (Яндекс Станция Миди)`.
- Добавлен HA-роутер музыки: `input_select.music_route`, voice-команды
  режимов и `script.music_play_favorites`. В Connect-режиме «включи любимое»
  идёт через Spotify Web API.
- Spotify Connect device выбран на телефоне; Web API `play` Liked Songs
  работает, metadata на Pulsar идёт.
- `volume_mode` переключён на `sync_spotify`: Spotify app 20% ↔ MA/Pulsar 0.2;
  добавлена абсолютная команда «громкость 1…10» (3 → 0.3, 10 → 1.0).
- Pipeline debug показал причину «Обращение не распознано»: STT слышал
  «Грокость 3» / «громкая стрима» вместо «громкость 3/три». Добавлены
  Whisper `--initial-prompt`, ASR-алиасы `грокость`/`громкасть` и fallback
  `громкая стрима` → 3. Voice-громкость в Connect теперь идёт через Web API
  volume + MA, чтобы app/Pulsar не расходились. Нужен голосовой retest.
- Известные косметические баги HA 2026.10.4: `KeyError:
  MediaPlayerState.ANNOUNCING` на announce; `assist_satellite` иногда
  остаётся `responding` до reload ESPHome config entry.

## 2026-10-26 — таймеры и будильник

- Проверены встроенные Assist-таймеры Pulsar через HA conversation API с
  `device_id`: «поставь таймер на 30 секунд» → action_done, «что с таймером»
  → 30 секунд, «отмени таймер» → выключен. «какие таймеры» не распознаётся;
  использовать «что с таймером». Таймеры не являются `timer.*` entities и
  не сохраняются после restart HA.
- Добавлен один слот будильника: `input_datetime.alarm_next`,
  `input_boolean.alarm_enabled/alarm_ringing/alarm_snooze_active`,
  `input_select.alarm_repeat`, `input_number.alarm_volume/
  alarm_previous_volume`, `input_text.alarm_previous_source`.
- Добавлены scripts `alarm_set/schedule_next/start/announce/loop/ramp/
  snooze/stop/cancel/restore/resume/startup_reconcile`, automation
  `alarm_scheduler`,
  custom sentences `custom_sentences/ru/station_alarm.yaml`, intent scripts
  `StationAlarm*`.
- Голосовое время: 7:30, 7 30, «семь тридцать», «на завтра», «по будням»;
  status/cancel; trigger в +25 с и snooze trigger в +10 с проверены.
- Звук: локальный `www/alarm_chime.wav` + Piper TTS; source и volume
  восстановлены (0.06) после stop. ASR prompt расширен словами
  «будильник/таймер/на завтра/по будням»; `small-int8` оставлен.
- Текущее состояние: будильник выключен, target 0.45, source Connect,
  громкость 0.06; музыка может играть/быть idle. Фактический голосовой
  retest — открыт.
- Музыка: `alarm_music_was_playing` сохраняет факт playback; `alarm_resume`
  продолжает трек после stop (проверено с Spotify Connect), при snooze
  остаётся на паузе. Нормальные voice-команды Pulsar duck/resume; «пауза»/
  «стоп музыка» останавливают намеренно.
- Остановка будильника: «стоп»/«останови будильник» и короткое action-
  нажатие Pulsar (automation `alarm_button_stop`, event_type=action,
  `alarm_ringing=on`). При остановке музыка, игравшая до будильника,
  продолжается с того же места. Проверено text/API и ручным event.

## 2026-10-26 — тестовый контур

- Добавлен hermetic test suite: HA YAML/entity/service/Jinja/intent checks,
  `ctl.sh` с fake docker/curl/ss/systemd, egress-guard rules, shell/Python
  syntax, secret-pattern gate, pairing WAV/OGG CLI.
- BLE gateway Go tests: token/Wi-Fi validation, WPA parsing, GATT read/write/
  offset/commit, HTTP auth/status, event parser; coverage 45.5% statements
  (package 47.0%). Pulsar installer upstream tests остаются отдельным gate.
- Live E2E: read-only HA API/entities/intents, BLE gateway auth/status/method
  guards, opt-in volume и alarm set/cancel, audio alarm → текст «стоп» →
  resume музыки. Тесты не запускают мутации без явных флагов.
- `reports/TEST-COVERAGE.md`: 90 тестовых функций; pairing 88% lines;
  структурно проверены 11 HA YAML, 17 scripts, 19 intents, 3 automations,
  101 фраза; взвешенная оценка ~46%.

## 2026-10-26 — Hermes, MCP и музыкальный каскад

- Hermes Agent `v0.22.5` установлен на неттоп (не на станцию); официальный
  installer, ChatGPT/Codex OAuth, модель `gpt-5.6-luna`, пользовательский
  `hermes-gateway.service`, API `127.0.0.1:8642`.
- HA MCP Server создан; Hermes MCP `homeassistant` подключается по
  loopback с long-lived HA token. Allowlist: music search/play/URI/candidates/
  status/favorites, базовый дом и media_player; без shell/root/reboot/
  Bluetooth/Wi-Fi/alarm/timer.
- Добавлен музыкальный каскад HA: `music_search` (Spotify Search или MA),
  `music_play_uri`, `music_play_query` (top-1 + кандидаты),
  `music_play_candidate`, `music_status`; wildcard `{query}`, ASR-фразы
  «включи песню/трек/альбом/плейлист/группу», «второй/третий», «что играет».
- Spotify 429 корректно отображается голосом; URI никогда не выдумывается,
  поиск возвращает реальные кандидаты. MA search проверен.
- Pipeline `Рикардо` переключён на `conversation.hermes_agent` с
  `prefer_local_intents: true`: «какой будильник» остаётся локальным,
  «что такое фотосинтез» уходит в Hermes; `web_search` backend
  `openai-native` проверен через HA conversation.
- Тесты: 65 Python + 15 pairing + 22 Go + 10 installer = 112; HA config
  check RC=0; weighted area ~52%.

## 2026-10-26 — voice-профиль Hermes

- HA `llama_cpp` subentry получил voice prompt: кратко, без Markdown/URL,
  web_search для актуальных фактов, music tools для музыки.
- Добавлен loopback `hermes-voice-proxy` (`127.0.0.1:8643`): форвардит
  OpenAI-compatible API Hermes и вычищает URL, Markdown-ссылки, цитаты
  `([domain](url))`, списки и лишние пробелы перед Piper TTS.
- HA conversation agent переключён на proxy; `streaming=false`.
- Проверено: новость про OpenAI голосом без URL, источник остаётся словом
  `investing.com`; локальный «какой будильник» продолжает работать.
- Добавлены 8 unit-тестов sanitizer; всего 123 hermetic-теста
  (76 Python + 15 pairing + 22 Go + 10 installer).

## 2026-10-26 — STT-фикс после живого «фотосинтеза»

- Реальный голосовой запрос «Что такое фотосинтез?» Whisper `small-int8`
  распознал как «Что такое фатесинтус?»; Hermes ответил по бессмысленному
  слову. Логи: `wyoming-whisper` transcript, Hermes agent log.
- Whisper переключён на `medium-int8` (образ `wyoming-whisper:3.8.1`,
  модель ~1.5 ГБ, CPU/int8). HA перезапущен; Wyoming переподключён.
- Hermes voice prompt усилен: при искажённом/несуществующем слове не
  придумывать значение, а сразу отвечать по наиболее вероятному термину с
  оговоркой «Похоже, вы имели в виду …». Проверено: «фатесинтус» →
  «Похоже, вы имели в виду фотосинтез…».
- Следующий шаг — реальный микрофонный retest «Алиса, что такое фотосинтез?».

## 2026-10-26 — GigaAM v3 и watchdog Assist satellite

- После повторного «опять лажа» проверен сохранённый debug WAV: Whisper
  medium слышал «фотоэссентос», GigaAM v3 — «что такое фотосинтез».
- Поднят `wyoming-onnx-stt` (`lmo3/wyoming-onnx-stt:latest`) на `:10305`,
  модель `gigaam-v3-ctc` int8, CPU, ru; HA Wyoming integration; pipeline
  `Рикардо` переключён на `stt.onnx_asr`. Whisper `medium-int8` оставлен
  fallback.
- GigaAM проверен на реальном debug WAV и на TTS-файле ответа: текст
  «что такое фотосинтез»; HA STT API возвращает success.
- Найдена вторая проблема: `assist_satellite` остаётся `responding` после
  TTS (HA 2026.10.4/ESPHome `ANNOUNCING`). Добавлен automation
  `assist_satellite_responding_watchdog`: через 25 с перезагружает ESPHome
  config entry, если будильник не звонит.
- Voice prompt сокращён до 1–2 предложений/180 символов, URL/Markdown
  вычищаются proxy; HA config check RC=0; 123 hermetic-теста.

## 2026-10-26 — голосовой retest принят

- Реальные микрофонные тесты GigaAM v3: «что такое фотосинтез»,
  «громкость три», «кто такой робби уильямс», «кто такой эльдар автоподбор».
  Сохранённые WAV подтверждают транскрипты; пользователь подтвердил:
  «теперь вроде все супер».
- Ответы Hermes сокращены до 1–2 предложений/180 символов; URL/Markdown
  вычищает proxy; watchdog Assist satellite работает.
- Debug recording отключён, сохранённые голосовые WAV удалены.
- Текущее состояние: Assist satellite idle, музыка idle, Spotify Connect,
  громкость 0.3, будильник выключен.
- Осталось: голосом будильник/таймер и музыкальный каскад, phone transfer,
  Spotify 429.

## 2026-10-26 — research rev 2: wake word «Рикардо»

- Проверены станция, `libtflite.so`, формат microWakeWord v2 и код Pulsar
  v0.4.1; подтверждено, что «Рикардо» нельзя включить настройкой.
- Рекомендация: MVP server-side microWakeWord на `lab-host` + патч Pulsar
  v0.4.1 C++; on-device — после gate по recall/FP. Обновление до v0.5.0
  Rust — отдельная миграция, не блокер.
- Полный отчёт и план: `docs/RESEARCH-RICARDO-WAKE-WORD.md`. Live-контур
  не менялся; `wake_word=alisa`.
- Изучен сторонний `jarvis.zip` (Go, Midi + штатная Алиса): on-device
  microWakeWord 84 КБ работает, ~1% CPU в тишине, ~20% при звуке,
  22–32 МБ RSS. Ключевая деталь — Yandex `libtflite.so` требует
  `TfLiteCustomOpResolverCreate("FULL")`; обычный `TfLiteInterpreterCreate`
  падает. Аудио-путь Jarvis (`yio_audio`) не совместим с Pulsar; берём
  frontend/runtime как reference, модель «Рикардо» обучаем отдельно.
- Desktop cleanup: удалены точные дубликаты (`airplay2-station-v1.0.zip`,
  `spotify.sh`, `yandex-station-airplay-fix.sh`,
  `yandex-station-privacy.sh`), `TTS-voices/`; `jarvis.zip` и
  `TTS-voices.zip` перенесены в `artifacts/`. Старый
  `alice-mitm-bootstrap` остановлен (DNS на Mac больше не используется)
  и удалён; исходники — `archive/legacy/alice-mitm-bootstrap/`, архив —
  `artifacts/desktop-originals/alice-mitm-bootstrap-20261026.tar.gz`.
- `v3rt3p-stack` остановлен (`docker compose down`) и удалён; compose/
  скрипты — `archive/legacy/v3rt3p-stack/`, upstream —
  `https://github.com/v3rt3p`. Colima оставлен для других Docker-проектов.
- Docker cleanup 2026-10-27: удалены 5 v3rt3p-образов (~8.36 ГБ) и
  dangling-образы (4.19 ГБ); Docker images 81.5 → 63.2 ГБ. Образы других
  проектов не трогали; build cache 7.48 ГБ остался.


## 2026-10-26 — TTS research и выбор Eugene, передача сессии

- Сохранены полный TTS-ресерч, исправления прежних утверждений, план реализации и корпус из 36 реплик + 6 сценариев отказа.
- После прослушивания пользователь исключил Kokoro («битые» сэмплы), предпочёл Silero Eugene и запросил обработку ударений как у Alexandr. Показан готовый Eugene + RUAccent; итоговая приёмка на станции ещё впереди.
- Полный контекст, точные файлы аудио и границы проверок: `docs/RICARDO-TTS-HANDOFF.md`; ресерч: `docs/RESEARCH-RICARDO-TTS.md`; план: `docs/PLAN-RICARDO-TTS.md`.
- Снимки источников и сэмплы: `artifacts/tts-research-2026-10-26/`; корпус: `projects/home-assistant/tools/tts-lab/ricardo-tts-eval.json`.
- Live-контур не менялся; SSH .26 дал timeout, .25 ответил без успешной BatchMode-аутентификации. Замеров TTS на lab-host и новой генерации в этой сессии не было.


## 2026-10-26 — remote access: Tailscale + Serve

- Выбран Tailscale вместо публичного HA/port-forward: `lab-host` + `tailscale serve`, HA остаётся непубличным.
- Backup стека `nettop-20261026-224336.tar.gz` (1.5 ГБ) + копия на Mac; HA config check RC=0.
- HA: TOTP MFA, trusted proxy `127.0.0.1`, XFF, login threshold 5, internal `http://192.0.2.25:8123`, external `https://lab-host.example-tailnet.ts.net`.
- Tailscale `1.104.2`, `lab-host` `100.114.158.93`; Serve `443 → 127.0.0.1:8123`, Let's Encrypt cert; Funnel off.
- ACL: `tag:homeassistant` + grants только `tcp:443`; netmap подтвердил; `xiaomi-17` в tailnet; remote с cellular работает.
- `verify-remote-access.sh` — 0 FAIL/0 WARN; `tailscaled` restart переживает Serve. Полный reboot, HA app/push, ONT WAN 22/443 — открыты.
- ACL snapshot/rollback: `backups/tailscale-acl-20261026/`.

## 2026-10-26 — Tailscale remote access принят

- Reboot `lab-host` 23:46 прошёл: HA/контейнеры/Tailscale/Serve/ACL поднялись автоматически.
- `xiaomi-17` после ребута переподключился: active, direct `203.0.113.11`, traffic tx/rx.
- `verify-remote-access.sh`: 0 FAIL/0 WARN; HTTPS 200, cert valid; Funnel off.
- Remote HA через cellular и HA app подтверждены; публичного HA нет.
- Осталось: ONT WAN `22/443` hardening, offsite backup.

## 2026-10-27 — cleanup и recovery kit

- Удалён generated junk, битые `services/` symlink’и, точные дубли artifacts;
  Mini 2 guide/rollback — `archive/legacy/mini2/`; старые логи —
  `archive/logs/2026-10/`.
- v3rt3p source repos (5) архивированы в
  `archive/legacy/v3rt3p-stack/repos/`; commits — `UPSTREAM.md`.
- Добавлены `docs/REBUILD.md`, `docs/DECISIONS.md`,
  `docs/SENSITIVE-DATA.md`; обновлены индексы/READMEs.
- Зафиксирован factory-reset recovery path: Mac/Quasar stage 1 → ADB →
  CA/root → OTA/egress → Pulsar/BLE.
- Секреты не менялись; реестр — без значений. Offsite backup/factory image
  остаются gap’ами.



## 2026-10-27 — on-device «Рикардо»

- Обученная модель v1 интегрирована в C++ Pulsar на точном tag v0.4.1;
  сохранён ALSA ABI shim armhf. Установлен v0.4.1-ricardo4.
- Runtime проверен на Midi: 12/12 синтетических позитивов, 0/24 негативов;
  пользователь подтвердил ответ на живое «Рикардо». Recall/FP в быту не измерены.
- Добавлены 0.5 с pre-roll и выбор обращения; HA/MA переподключились.
- Оригинал сохранён, добавлен rollback-ricardo.sh; временная диагностика удалена.
- Артефакты: `artifacts/wakeword/ricardo-station/`; статус: `logs/status-20261027-0308-wakeword.txt`.


## 2026-10-27 — поиск другого тембра с сохранением достоинств Piper

- Пользователь предпочёл качество речи Piper, но не понравились штатные тембры; выбор Eugene снова предварительный.
- Найдены Puriso для Piper и UtrobinTTS male; локально синтезировано по 3 одинаковых реплики на Mac, сохранены параметры и хеши.
- Подробности: `docs/RESEARCH-RICARDO-TTS-ALTERNATIVES.md`; live-сервисы не менялись. Старый ZIP не обновлялся.


## 2026-10-27 — Puriso установлен на lab-host и выбран в «Рикардо»

- По прямому запросу пользователя установлен Piper Puriso, voice `ru_puriso2-medium`, source revision `4f826cf39157118a2854f0b825917c1f83a342e7`; веса/config проверены по SHA256.
- Изменены default существующего Piper через .env, TTS language/voice pipeline через HA WebSocket API, явный голос alarm_announce. Пересоздан только Piper; HA/STT/Pulsar не перезапускались.
- Модель успешно синтезировала на i7; HA config check, TTS pipeline и локальный intent→TTS пройдены. Вызваны тестовое tts.speak и alarm_announce; слуховая приёмка пользователем впереди.
- Irina сохранена, private backup на сервере. Подробно: `docs/DEPLOY-RICARDO-PURISO.md`, воспроизведение/откат: `projects/home-assistant/tts/puriso/README.md`, installer: `projects/home-assistant/tools/tts-lab/install_puriso.py`.
- Следующий этап — выбор референса/корпуса и обучение собственного Piper-голоса. Конкретный тембр ещё не указан, обучение не начиналось.


## 2026-10-27 — ricardo5: «Рикардо, стоп» и возврат музыки

- Пользователь сообщил, что после смены TTS-голоса «Рикардо, стоп» во время
  будильника больше не останавливает звонок/не возвращает музыку.
- Проверка: HA `alarm_start → conversation("стоп") → alarm_stop → resume`
  проходит; duck/resume Pulsar в логах 0.15 → 1.00. Потерян не duck, а путь
  команды: локальный command spotter Pulsar перехватывал «стоп» и ставил
  музыку на паузу, отменяя HA pipeline.
- Исправлено в `projects/station/upstream/pulsar`: command spotter выключен на время
  wake-запроса/арбитража; «Рикардо, стоп» уходит в HA, а локальные команды
  без wake и barge-in по ответу/объявлению сохранены.
- Собрано в Linux-контейнере с Zig 0.14.1; установлен `v0.4.1-ricardo5`
  (SHA256 `52e5163e…`). HA/MA переподключились; текстовый alarm
  start/stop/resume проверен на Spotify Connect.
- Артефакты: `pulsar-v0.4.1-ricardo5-armhf.bin`,
  `pulsar-ricardo5-source.tar.gz`; откат — `/data/pulsar/backups/pre-ricardo5/`.
- **Осталось:** живой микрофонный тест «Рикардо, стоп» во время звонка.

## 2026-10-27 — сверка ricardo5 после исправления command spotter

- Сверены live-бинарник, модель и исходники с архивом ricardo4.
- Модель, microWakeWord, frontend, ALSA ABI shim и Makefile не изменены;
  pre-roll, mute и HA wake-путь сохранены. В проверенной части регрессий нет.
- Runtime probe на станции прошёл; звукозахват 96 kHz/8 ch/3072 и подписка HA
  подтверждены. Колонка не перезапускалась, музыка не изменялась.
- Отчёт: `artifacts/wakeword/ricardo-station/COMPATIBILITY-RICARDO5.md`;
  протокол: `logs/status-20261027-0418-ricardo5-compatibility.txt`.
- Живой «Рикардо, стоп» во время будильника остаётся к проверке;
  модель шёпота пока не менялась.

## 2026-10-27 — локальный Git лаборатории

- Создан корневой Git без remote для локального версионирования.
- В AGENTS.md закреплено: локальный-only — штатная конфигурация, push/remote
  без прямого запроса не добавлять; сохранять общий функционал всех сессий.
- Вложенные upstream checkout сохранены. Полный патч Pulsar и его база
  версионируются в projects/station/patches/pulsar/, снимок обновляет projects/station/scripts/snapshot-pulsar.py.
- Приватные и тяжёлые данные исключены через .gitignore; компактная модель
  «Рикардо» и метаданные входят в историю. Колонка не менялась.


## 2026-10-27 — ricardo6: тихая отмена вызова

- После «Рикардо» пользователь сказал «отмена», чтобы передумать. GigaAM
  вернул пустой текст (`stt-no-text-recognized`), Pulsar показал штатную
  ошибку тремя красными вспышками; музыка продолжилась.
- `stt-no-text-recognized` после wake теперь считается тихой отменой: без
  красной ошибки, музыка возвращается.
- Добавлен HA `StationCancel` для «отмена/отмени/передумал/не надо/ничего/
  отбой/забудь/неважно»; ответ «Хорошо.», без side effects. Файлы
  `station_control.yaml` и `intent_scripts.yaml` разложены на lab-host,
  backup — `~/nettop/backups/cancel-20261027-0428/`.
- Собран и установлен `v0.4.1-ricardo6` (SHA256 `ebe3e47e…`); HA/MA
  переподключились, музыка восстановлена на Spotify Connect 0.06.
- Текстовая отмена проверена. **Осталось:** живой микрофонный тест
  «Рикардо, отмена» и «Рикардо, стоп».

## 2026-10-27 — музыка, громкость и личность «Рикардо»

- Пользователь: «включи музыку» → ответ про ошибку музыкального сервиса;
  «громкость пять» не применилась; на «как тебя зовут» Hermes ответил
  «Я — Hermes».
- Причина музыки: в HA `music_search` поля Spotify читались как
  `spotify_search.content.tracks.items`; Jinja получала bound method
  `.items` и падала с `TypeError: builtin_function_or_method`.
  Исправлено на `content.get('tracks'/'albums'/'playlists'/'artists', {}).
  get('items', [])`. Search и play проверены (`Белые розы`, `Nirvana`).
- Spotify 429 снят; `music_play_query` играет top-1, `music_play_uri`
  сохраняет source Spotify Connect.
- Generic «включи музыку/поставь музыку/вруби музыку» добавлен как локальный
  `StationMusicPlay` → `music_play_favorites`; он не уходит в Hermes.
- `spotify_connect_set_volume` переделан: сначала локальный
  `media_player.volume_set`, затем необязательный Spotify Web API с
  `continue_on_error`; «громкость пять» → 0.5 проверена pipeline.
- Личность: `~/.hermes/SOUL.md` и `agent.personalities.voice` переписаны на
  «Рикардо, личный ассистент», HA `llama_cpp` prompt обновлён тем же
  текстом, subentry/entity title — `Рикардо`. «как тебя зовут» →
  «Я Рикардо, ваш личный ассистент».
- Проверки: HA reload/config clean, 81 hermetic test, живой
  `music_search`/`music_play_query`, pipeline «громкость пять», «включи
  музыку», identity. **Осталось:** живой микрофонный retest этих фраз и
  «Рикардо, стоп/отмена».

## 2026-10-27 — «следующая» и «предыдущая» без Hermes

- Пользователь сказал «следующая»; фраза не имела локального custom intent
  и уходила в Hermes, поэтому ответ был медленным.
- Добавлены `StationMusicNext` / `StationMusicPrevious`: «следующая»,
  «следующая песня/композиция», «следующий трек», «предыдущая»,
  «предыдущий трек» и варианты. Оба вызывают
  `media_player.media_next_track` / `media_previous_track` напрямую.
- Ответы: «Следующая» / «Предыдущая»; source/маршрут не меняются.
- Проверки: HA reload, pipeline «следующая», 55 HA/Hermes test. Живой
  микрофонный retest остаётся.

## 2026-10-27 — Matter Server и HA Matter

- Пользователь хотел добавить Yandex LED strip (Matter over Wi-Fi) в HA;
  в HA Matter integration поле URL `ws://localhost:5580/ws` — это адрес
  Matter Server, а не ленты.
- В stack добавлен `matterjs-server` (`ghcr.io/matter-js/matterjs-server:stable`,
  1.5.5/matter.js 0.18) с host network, `PRIMARY_INTERFACE=wlp4s0`,
  `LISTEN_ADDRESS=127.0.0.1`, данными `data/matter-server`.
- Контейнер healthy; `127.0.0.1:5580` слушает, dashboard 200, WebSocket
  `ws://127.0.0.1:5580/ws`; HA integration `Matter` создана
  (`use_addon=false`) и подключена, fabric пустой.
- `ctl.sh status/logs/shell/backup/doctor` обновлены под Matter; backup
  перед изменением — `~/backups/nettop-matter-20261027-111827/`.
- **Осталось:** добавить ленту через HA Companion (QR/11-значный код,
  2.4 ГГц, Bluetooth/location); если она уже в «Дом с Алисой» — share/
  multi-fabric или сброс. Проверить on/off, яркость, цвет.

## 2026-10-27 — Xiaomi Home integration для MJGJD02YL

- Пользователь хочет подключить Mi Computer Monitor Light Bar 1S
  (`MJGJD02YL`): Wi-Fi + BLE dimmer, Yeelight inside, China server, не Matter.
- Установлена официальная Xiaomi Home integration `v0.5.0`
  (`XiaoMi/ha_xiaomi_home`, tag `v0.5.0`, commit `75b3acb`) в
  `custom_components/xiaomi_home`; HA перезапущен, component loaded,
  config flow EULA доступен.
- Backup config entry перед установкой:
  `~/backups/ha-xiaomi-home-20261027-113338/`.
- **Осталось:** вход в Xiaomi China и импорт светильника; если MIoT-модель
  не появится — ESPHome/Tasmota с пайкой (`vanzi11a/esphome-xiaomi_monitor_light_bar_s1`),
  официальный локальный API для неё закрыт.

## 2026-10-27 — голая «продолжи» как resume музыки

- Пользователь сказал «продолжи»; команда уходила в Hermes, потому что в
  `StationMusicResume` были только «продолжи музыку» и «продолжай».
- Добавлены «продолжи», «продолжить», «продолжай музыку»,
  «продолжить музыку», «продолжи воспроизведение», «продолжи песню»,
  «возобнови», «возобнови музыку». Локальный intent теперь срабатывает до
  Hermes.
- Проверка pipeline: `processed_locally: true`, ответ «Продолжаю», playback
  resumed. `make test` — 83 OK; HA config check чистый. Коммит `b1345f7`.

## 2026-10-27 — Xiaomi China login, импорт ламп и удаление дубля

- OAuth Xiaomi завершён; браузер упирался в жёсткий redirect
  `http://homeassistant.local:8123` (`DNS_PROBE_FINISHED_NXDOMAIN`).
  Помог официальный workaround: заменить host на `192.0.2.25`, сохранив
  путь/query. Config entry `01M5Y1M47A9VKA6Z98BEJT9FR8`
  (`Proflyder: 1629957439 [中国大陆]`) загружен.
- Импортированы:
  - Monitor Light Bar 1S `MJGJD02YL` / `yeelink.light.lamp22` —
    `light.yeelink_cn_919937191_lamp22_s_2` (`color_temp`);
  - Mi Bedside Lamp 2 / `yeelink.light.bslamp2` —
    `light.yeelink_cn_827036414_bslamp2_s_2_light` (`color_temp`, `rgb`).
- Mi Bedside Lamp была добавлена дважды: сначала HomeKit Controller
  (`MiBedsideLamp2-F723`, entry `01M5XYMMY3DKZPEFTBJNQJATHF`), затем
  Xiaomi Home. Дубликат HomeKit Controller удалён; backup
  `~/backups/ha-duplicate-lamp-20261027-114648/`. Ссылок на старую
  entity в automations/конфиге не найдено.
- **Осталось:** проверить on/off, brightness и color temperature обеих ламп;
  BLE-пульт отдельно в HA не добавляется.

## 2026-10-27 — локальное управление лампами

- Пользователь сказал «Рикардо, выключи лампу»; local intent не находил
  объект «лампу», запрос уходил в Hermes, тот переспрашивал имя и не выключал.
- Добавлены `StationLightOn` / `StationLightOff` с локальным списком
  `light_target`: «лампу»/«лампочку»/«ночник»/«лампу у кровати» →
  `light.yeelink_cn_827036414_bslamp2_s_2_light`; «светильник» →
  `light.yeelink_cn_919937191_lamp22_s_2`; «ленту» → `light.spalnia_lenta`.
  Фразы «выключи/погаси/отключи/выруби» и «включи/зажги/вруби».
- HA `intent_script` и custom sentences разложены, `intent_script.reload` и
  `conversation.reload` выполнены. Проверка pipeline: `processed_locally:
  true`, «Выключаю лампу», выключилась только bedside-лампа; светильник и
  лента остались включены. On/off для лампы и светильника проверены.
- `make test` — 92 OK; HA config check чистый.
- Backup: `~/backups/ha-voice-light-20261027-131707/`.

## 2026-10-27 — сценарий «я сел работать»

- Пользователь попросил сценарий «я сел работать», который включает всё
  освещение в спальне.
- Добавлен `script.work_mode`: `light.turn_on` для `light.spalnia_lenta`,
  `light.yeelink_cn_827036414_bslamp2_s_2_light` и
  `light.yeelink_cn_919937191_lamp22_s_2`. Станционное кольцо не входит.
- Добавлен локальный `StationWorkMode` с фразами «я сел работать»,
  «я села работать», «сел работать», «включи рабочий режим»,
  «включи режим работы» и вариантами; ответ «Включаю всё освещение в
  спальне.»
- HA `scripts.yaml`, `intent_scripts.yaml` и custom sentences разложены;
  `script.reload`, `intent_script.reload`, `conversation.reload` выполнены.
  Pipeline: `processed_locally: true`, все три `light.*` → `on`.
- `make test` — 93 OK; HA config check чистый. Backup:
  `~/backups/ha-voice-work-20261027-132428/`.

## 2026-10-27 — обратный сценарий black out

- Пользователь попросил обратное действие для рабочего света: «Рикардо,
  black out» с русской транслитерацией.
- Добавлен `script.blackout`: `light.turn_off` для `light.spalnia_lenta`,
  `light.yeelink_cn_827036414_bslamp2_s_2_light` и
  `light.yeelink_cn_919937191_lamp22_s_2`; станционное кольцо не входит.
- Добавлен локальный `StationBlackout` с фразами «black out», «blackout»,
  «black-out», «блэкаут», «блэк аут», «блэк-аут», «блекаут», «блек аут»,
  «блек-аут», а также «выключи всё освещение», «погаси всё освещение»,
  «выключи весь свет», «выключи/погаси свет в спальне».
- HA config разложен; `script.reload`, `intent_script.reload`,
  `conversation.reload` выполнены. Проверены оба варианта pipeline:
  `black out` и `блэкаут` → `processed_locally: true`, ответ «Выключаю всё
  освещение в спальне», все три `light.*` → `off`. После проверки рабочий
  свет восстановлен через `script.work_mode`.
- `make test` — 94 OK; HA config check чистый. Backup:
  `~/backups/ha-voice-blackout-20261027-133454/`.

## 2026-10-27 — ASR-вариант «блок аут» для black out

- Пользователь сказал «блек аут», но GigaAM распознал «блок аут». Такой
  фразы в `StationBlackout` не было, поэтому запрос ушёл в Hermes; тот
  ответил «Похоже, вы имеете в виду блэкаут…» и ничего не выключил.
- Pipeline debug зафиксировал: `stt-end.text = "блок аут"`,
  `processed_locally: false`, `intent-end` от Hermes.
- Добавлены ASR-варианты «блок аут», «блок-аут», «блак аут», «блак-аут»,
  «блакаут». После `conversation.reload` проверен pipeline «блок аут»:
  `processed_locally: true`, ответ «Выключаю всё освещение в спальне»,
  все три `light.*` → `off`; рабочий свет затем восстановлен.
- `make test` — 94 OK. Backup:
  `~/backups/ha-voice-blackout-asr-20261027-135128/`.


## 2026-10-27 — ускорение Hermes voice: low reasoning и streaming

- Подтверждены runtime `medium` и HA `streaming=false`; SSH доступен с
  `~/.ssh/labuser_ha`, Wi-Fi адрес неттопа `.25`.
- Voice proxy v2 задаёт per-request `model_options.reasoning_effort=low`,
  очищает SSE по законченным предложениям и не передаёт reasoning/tools/status
  в озвучивание. Non-streaming JSON сохранён. HA включён streaming в entry data.
- При live-проверке найдено дополнительное ожидание Piper следующего слова;
  paragraph boundary после полного предложения устраняет его без правок Piper.
  В HA 60-символьный порог раннего TTS оставлен штатным.
- Text → Hermes → Puriso → HTTP: первые 1024 байта аудио 2.328 с, intent-end
  2.592 с; аудио действительно доступно до завершения модели. На колонке
  этот тест не воспроизводился. Акустическая приёмка остаётся отдельной.
- 91 Python test passed; профиль Hermes — 25. Private backup и rollback —
  `DEPLOY-HERMES-STREAMING.md`. HA перезапущен; Pulsar/MA не менялись.

## 2026-10-27 — systemd 259.9 + Secure Boot firmware update

- Полный pre-update backup: `~/nettop/backups/nettop-20261027-140338.tar.gz`
  (1.6 ГБ, 600), копия `backups/maintenance-20261027/` + `/etc` и metadata;
  SHA256 совпал на сервере и Mac.
- systemd 259.5 → 259.9 (9 пакетов), принудительно после phasing; initramfs
  пересобран; reboot. HA/MA 200, failed units нет.
- fwupd: KEK CA 2011→2023, UEFI CA 2011→2023, dbx 20250902→20260402;
  reboot; Secure Boot enabled, MOK сохранён, `UEFI db: Valid`. PK остался
  `DO NOT TRUST - AMI Test PK` (HSI 0).
- Tailscale Serve/remote проверены: `verify-remote-access.sh` — 0 FAIL/0 WARN.
- Zombie `soloist` ушёл после reboot; был один transient PAM denial на раннем
  boot, последующие SSH-логины успешны.

## 2026-10-27 — Hermes: полный HA и MA MCP

- Пользователь потребовал, чтобы fallback-Hermes умел всё, а не только
  узкий allowlist. Из HA MCP убран `tools.include`: Hermes видит **57/57**
  HA tools вместо 15; resources и prompts включены. `hermes mcp list`
  показывает `homeassistant → all`; `mcp test` подтверждает 57.
- В Music Assistant включён экспериментальный плагин `MCP Server`
  (`fastmcp_server`) на `http://127.0.0.1:8095/mcp/v1`. Setup-flow завершён
  через callback; выставлены все permission flags (query/control/edit/delete/
  debug/config/resources), `require_auth=true`, `require_confirmation=false`.
- Создан отдельный long-lived token `MCP — Hermes`, сохранён только в
  `~/.hermes/.env` как `HERMES_MA_TOKEN` (600). Hermes config:
  `music-assistant` MCP, `tools.resources/prompts=true`, без include;
  `platform_toolsets.api_server=[mcp-homeassistant, mcp-music-assistant, web]`.
  `hermes mcp list`: `music-assistant → all`; `mcp test`: **86 tools**.
- Проверка через gateway: запрос «перечисли плееры Music Assistant» вернул
  `Яндекс Станция Миди; Web (Chrome on Mac)` — MA MCP tool реально вызван.
- Gateway перезапущен; HA/MA/Hermes healthy. Backups:
  `~/.hermes/backups/config-before-all-mcp-20261027-140031/`,
  `config-before-ma-mcp-20261027-140254/`,
  `~/backups/ma-fastmcp-20261027-140059/`.
- Риск: Hermes теперь может вызывать destructive/admin MA tools и все HA
  intent scripts; это осознанное решение пользователя. MA MCP experimental;
  откат — restore config/MA backup, disable plugin.

## 2026-10-27 — schedules: списки будильников, таймеры, напоминания, заметки

- Реализован `docs/PLAN-SCHEDULES.md`: будильники в `local_todo`
  (`alarms_once/daily/weekdays/weekends`), scheduler `schedule_alarm_tick`,
  голосовые create/list/disable/delete, snooze/stop через существующий
  alarm-путь.
- Добавлены 8 UI `timer.taimer_*` с restore и названиями в
  `input_text.taimer_N_nazvanie`; голос start/status/pause/resume/cancel;
  `timer.finished` озвучивается и уведомляет телефон.
- Добавлены `todo.reminders` и `todo.notes`, голосовые напоминания/заметки,
  dispatcher и mobile notification.
- Dashboard «Помощник» `/assistant-schedules`.
- Старый `alarm_scheduler`/`alarm_next`/`alarm_enabled`/`alarm_repeat`
  выключены и скрыты; legacy `alarm_*` scripts сохранены для rollback.
- Проверены text/API: create/status/delete именованного будильника,
  trigger/stop/snooze, startup trigger, timer start/finish/restart restore,
  reminder/notes CRUD. Требуется микрофонный retest.
- Исправлен ramp после stop: `alarm_stop` → `mode: restart`, `alarm_ramp`
  проверяет `alarm_ringing` до первого и перед каждым шагом. Проверено:
  после stop громкость не растёт (0/1 с задержка, 12 с наблюдения).

## 2026-10-27 — Hermes MCP smart-home repair

- HA MCP was reachable (`hermes mcp test homeassistant` → 72 tools), but
  `StationLightOff` returned `Error handling StationLightOff`: the MCP-exposed
  intent schema has generic `name`/`area`, not `light_target`.
- `StationLightOn/Off` now resolve `name`/`area` and map `лента`/`лету`,
  `лампа`/`ночник`, `светильник`/`монитор`; fallback for plain «свет» is all
  three bedroom lights.
- Added aliases and Assist exposure for the three lights; HA restarted to
  refresh the MCP/assistant view.
- Verified Hermes: «выключи лету» → already off; «выключи лампу» → lamp off
  (HA history confirmed), then lamp restored on. Main chat/tools work.
- Title-generation OAuth warning remains separate from MCP.

## 2026-10-27 — Timer voice error: built-in HassStartTimer

- Live Assist returned a generic error because the built-in HA timer intent
  matched phrases with word-number hours (e.g. «пять часов вечера») and its
  response template tried `int('пять')`.
- Extended `StationTimerStart` custom sentences for word-number hours,
  optional time-of-day words, named timers without a unit, and bare
  «поставь таймер на пять». Reloaded conversation sentences.
- Verified text: «пять минут», «пять часов», «пять часов вечера/утра»,
  named timer variants and bare «пять» all return «Таймер установлен» with no
  template error. Test timers were cancelled afterwards.
- HA satellite watchdog still recovers `responding` after TTS; unrelated
  `ANNOUNCING` KeyError remains.

## 2026-10-27 — «Голос Алисы» при ошибке — это vins_error.wav

- Пользователь услышал голос Алисы после запроса будильника/таймера.
  Проверено: native `maind`/`quasar_launcher2` не запущены; на станции
  только Pulsar `v0.4.1-ricardo6`, `wake_word=ricardo`.
- Голос Алисы — штатный Yandex earcon
  `/system/vendor/quasar/sounds/ru-RU/vins_error.wav`; Pulsar играет его
  при `VaEvent::ERROR`, в том числе при ошибке HA pipeline. Это не запуск
  native-ассистента и не реакция на слово «Алиса».
- Причина конкретного срабатывания — built-in `HassStartTimer` с
  `hours='пять'`; custom timer/alarm phrases расширены, проверены текстом.

## 2026-10-27 — Относительные будильники «через N минут/часов»

- Добавлен `StationAlarmRelative` → `script.schedule_alarm_create_relative`:
  «поставь/заведи будильник через N минут/часов», именованные варианты и
  комбинация «через 1 час 30 минут». Создаётся однократный элемент в
  `todo.alarms_once` со сроком от текущего времени.
- Проверено текстом: 2 минуты, 1 час, 30 минут с именем «работа», 1 час
  30 минут; все создаются с корректным due, после проверки удалены.
- Это закрывает относительные фразы; абсолютные будильники и повторяющиеся
  списки не менялись.

## 2026-10-27 — Таймер: форма «на одну минуту»

- Живой запрос «поставь таймер на одну минуту» ушёл в Hermes: в списке
  `timer_minutes` не было винительной формы «одну», поэтому локальный
  `StationTimerStart` не совпал и Hermes ответил, что не может запустить
  таймер.
- Добавлены формы «одну»/«две» для минут таймера и «одну» для минут
  относительного будильника; custom sentences перезагружены.
- Проверено текстом: «поставь таймер на одну минуту» и «на две минуты» →
  «Таймер установлен»; тестовые таймеры отменены. Пользовательский
  будильник на 17:49 не тронут.

## 2026-10-27 — Непрерывный рингтон будильника

- Пользователь попросил заменить повторяющийся chime+TTS на непрерывный
  рингтон до остановки. `alarm_loop` теперь играет локальный
  `www/alarm_ringtone.wav` (полный трек HONEYMOAN — Too Much, 4:54) через ESPHome
  endpoint и повторяет его с паузой 0.2 с, пока `alarm_ringing=on`.
- `alarm_stop` и snooze останавливают `alarm_loop` и media; source/volume и
  Spotify resume сохранены. `alarm_announce` (chime + Puriso TTS) больше
  не участвует в цикле, оставлен отдельно для announce/legacy.
- Проверка: `play_media` ringtone → HTTP 200 за 0.003 с, player `playing`,
  `media_stop` → `idle`; loop smoke-тест на низкой громкости 2 с без ошибок;
  `alarm_ringing=off`, volume 0.1 восстановлен. Полная живая приёмка на
  реальном звонке ещё нужна.

## 2026-10-27 — Форма будильника без «срока выполнения»

- Пользователь указал на плохой UX: в `local_todo` поле «Срок выполнения»
  выглядит как дедлайн, хотя для повторяющегося будильника это следующая
  дата звонка.
- Добавлены `input_text.alarm_form_name`, `input_datetime.alarm_form_time`,
  `input_select.alarm_form_repeat` и `script.alarm_ui_create`; на вкладке
  «Будильники» появилась форма «название + время + повтор» и кнопка
  создания. Дата следующего звонка рассчитывается существующим
  `schedule_alarm_create`; после звонка scheduler переносит её.
- Проверено API: форма создаёт элемент в `todo.alarms_weekdays` с
  корректным `due` (следующий будний день), тестовый элемент удалён.
  В старом списке остался недонастроенный «Работа» без времени — он не
  планируется, пока не задать время/не пересоздать через форму.

## 2026-10-27 — аудит реорганизации и приватных данных

Проверены переносы, Bazel/Go и локальные hermetic-тесты. Исправлены ссылки,
TTS output path и зависимость go.work от игнорируемого installer. Приватные
данные собраны под secrets с совместимыми локальными ссылками; личные
идентификаторы в публичной копии заменены примерами. Аудиотрек исключён из
текущего Git-дерева. Устройство и живой HA не менялись; история не переписывалась.
Подробности и ограничения — REPOSITORY-AUDIT.md.

## 2026-10-27 — тесты у владельцев компонентов

Корневая tests/ упразднена: тесты HA, Hermes, stack и station scripts
перенесены рядом с кодом. Live E2E принадлежат projects/home-assistant/e2e
и projects/station/e2e, у каждого проекта свой скрипт запуска. Общие
syntax/security-проверки и агрегаторы находятся в platform/quality.
Обновлены Makefile, coverage, документация и pre-push audit path.
Live-устройства в рамках переноса не проверялись и не менялись.

## 2026-10-27 — исследование Kubernetes/observability

Read-only SSH-инвентаризация неттопа: шесть host-network контейнеров, Hermes
gateway/proxy в user systemd, ресурсы/LVM/swap/сеть и зависимости HA. Подготовлен
PLAN-KUBERNETES-MIGRATION.md с K3s/containerd, поэтапным cutover/rollback,
Prometheus/Loki/Grafana/Alloy и проектными dashboard Helm charts. Работающие
сервисы, сеть и диски не изменялись; кластер не устанавливался.
