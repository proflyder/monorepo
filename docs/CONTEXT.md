# CONTEXT — ориентация в проекте

> **TTS уже внедрён:** [DEPLOY-RICARDO-PURISO.md](DEPLOY-RICARDO-PURISO.md) — Puriso и следующий этап своего голоса.

> **Предыдущий контекст TTS:** начать с [RICARDO-TTS-HANDOFF.md](RICARDO-TTS-HANDOFF.md). Там сохранены выбор пользователя после прослушивания, сэмплы, ограничения проверки и точка продолжения; полный ресерч и план связаны ссылками.

**Дата:** 2026-10-28 01:20 (+05)  
**Станция:** Midi `cucumber`, `STATION_SERIAL`, `192.0.2.13`  
**Сервер:** Proxmox host `192.0.2.26`; HAOS VM `192.0.2.28`;
Ubuntu/K3s VM `lab-host` `192.0.2.27`  
**Стек:** HAOS `2026.10.4` + K3s `v1.35.9+k3s1`: MA `2.10.8`,
Whisper `3.8.1`, Piper `2.5.2`, GigaAM, Matter, Hermes; Tailscale Operator

> Это короткая память, а не второй STATE. Факты — `STATE.md`, задачи —
> `PENDING.md`, хронология — `HISTORY.md`, команды — `OPERATIONS.md`.

## Где мы сейчас

- Станция на прошивке `.160`; root ADB `:5555`, SSH `:22`; OTA/ADB fortress,
  egress guard и `rootkeeper.sh` живы.
- **Pulsar `v0.4.1-ricardo6` активен**; штатный `maind` остановлен. Штатная
  Алиса/Yandex Music/навыки не работают — это осознанная замена.
- Pulsar: микрофоны/VQE, LED/часы, GStreamer, hardware mute; HA ESPHome
  (33 entities, Noise key), MA Sendspin; autostart после reboot.
- Реальный голос «Алиса» → HA Assist → TTS работает; `todo.shopping_list`
  получил aliases `список`/`покупки`.
- Голосовые команды: «включи/выключи блютуз», «включи/выключи гейтвей»,
  «громче/тише»; «пауза/продолжи/стоп» (голая «продолжи» тоже локальная),
  «включи/выключи лампу» (ночник), «светильник», «ленту»,
  «я сел работать» (все три светильника спальни), «black out»/«блэкаут»
  (выключить их; ASR «блок аут» тоже), «следующая/предыдущая», «включи музыку», «включи любимое» (+ варианты),
  «включи песню/трек/альбом/плейлист/группу …», «второй/третий», «что играет».
  При пустом Spotify-плейлисте команда честно говорит, что sync не готов.
- После wake-слова локальный command spotter Pulsar выключен: «Рикардо, стоп»
  идёт в HA, поэтому останавливает будильник и возвращает музыку. Без wake
  локальные «стоп/пауза/громче» и barge-in работают как раньше.
- «Рикардо, отмена/передумал» отменяет вызов: `StationCancel` отвечает
  «Хорошо.»; пустой STT после wake тоже тихо отменяется, без красной ошибки.
- **Hermes `v0.22.5`** на неттопе — fallback для свободной речи/веб-поиска;
  **полный MCP-доступ**: HA MCP 73 tools и Music Assistant FastMCP
  86/86 tools (плагин experimental, все permission flags), токены
  `HERMES_HA_TOKEN`/`HERMES_MA_TOKEN`; модель `gpt-5.6-luna` через
  ChatGPT/Codex OAuth, `web_search` backend `openai-native`. Pipeline
  `Рикардо`: local intents → Hermes. Hermes не владеет таймерами/будильником.
  Личность — **Рикардо, личный ассистент** (`SOUL.md`, `voice` personality,
  HA prompt); не Hermes/Гермес. Проверено: «как тебя зовут» → «Я Рикардо…».
- BLE gateway `Station-Gateway`: 1 нажатие — hardware mute, 3 — gateway,
  4 — Bluetooth, 5 с — gateway setup. GATT, auto-mode, rollback, reboot.
- **4 нажатия могут уронить Wi-Fi/HA** на combo-чипе; окно Bluetooth
  ограничено 90 с + `wpa_cli reconnect`; BLE gateway остаётся аварийным
  каналом.
- HA/MA/Whisper/Piper healthy; `ctl.sh` — единая точка входа; backup
  проверен. HA integration Music Assistant добавлена; MA-плеер Pulsar есть.
- Matter Server `matterjs-server` (`127.0.0.1:5580`, fabric в
  `data/matter-server`) подключён к HA; Yandex LED strip/Matter — pairing
  через HA Companion, 2.4 ГГц и IPv6 link-local/mDNS.
- Xiaomi Home `v0.5.0` loaded; вход Xiaomi China выполнен, импортированы
  Monitor Light Bar 1S (`light.yeelink_cn_919937191_lamp22_s_2`) и
  Mi Bedside Lamp 2 (`light.yeelink_cn_827036414_bslamp2_s_2_light`).
  Дубликат HomeKit Controller `MiBedsideLamp2-F723` удалён. Осталось
  проверить on/off, brightness, color temperature; локальный API Xiaomi
  для этих моделей закрыт, резерв — ESPHome/Tasmota с пайкой.
- Spotify Connect работает; `volume_mode=sync_spotify`: громкость из Spotify
  приложения меняет Pulsar (20% ↔ 0.2), а «громкость 1…10» — абсолютно.
  `spotify_connect_set_volume` сначала ставит локальную громкость, затем
  необязательный Web API; «громкость пять» → 0.5 проверена.
- Spotify API 429 снят 2026-10-27; Connect search/play и generic «включи
  музыку» проверены. Liked Songs в MA отдельно не перепроверялся.
- Spotify Connect plugin включён: станция анонсируется как Connect-устройство;
  источник выбирается в MA (`Music Assistant Queue` / `Spotify Connect`).
- Добавлен роутер `input_select.music_route` и voice-переключение; «включи
  любимое» в Connect идёт через Spotify Web API, устройство уже paired.
  Громкость Connect — Web API + MA; текущая `0.3`.
- Будильники — списки `local_todo` (однократные/ежедневные/будни/выходные);
  таймеры — 8 `timer.taimer_*` с restore; напоминания/заметки — `local_todo`.
  Голос и dashboard «Помощник»; scheduler `schedules_*`. Старый один слот
  выключен/скрыт. Детали — `OPERATIONS.md`, `PLAN-SCHEDULES.md`.

## Ключевые решения

- Pulsar — сателлит HA, а не замена хаба; HA — центр, MA — музыка.
- Штатный стек станции не возвращаем без причины; откат — `ctl.sh disable`.
- 1/4/5 обрабатывает BLE gateway, а не `maind`; 1 читает Pulsar.
- Секреты не публикуются: BLE token, HA control token, MOK, HA config.
- Factory reset / wipe `/data` — только с отдельным согласованием.

## Открыто

- Spotify Connect: голосовой `громкость 3` пройден; остались таймер/будильник
  голосом, phone→speaker/transfer. Spotify music provider ждёт sync для MA-режима.
- Будильник: проверить голосом «стоп», громкость/слышимость непрерывного
  рингтона и resume музыки; таймеры после restart HA не сохраняются.
- Bluetooth-пара с телефоном; 4 нажатия/голосовая команда могут уронить Wi-Fi.
- Custom wake word «Рикардо»: research/план —
  [`RESEARCH-RICARDO-WAKE-WORD.md`](RESEARCH-RICARDO-WAKE-WORD.md);
  рекомендация — server-side MVP + патч Pulsar v0.4.1, on-device после
  gate; v0.5.0 Rust — отдельная миграция. `jarvis.zip` подтвердил
  on-device runtime (`FULL` resolver, 84 КБ модель, ~1% CPU).
- HA MFA/Tailscale remote access работает (HA не публичный); offsite backup.
- Hermes: реальный микрофонный retest свободной речи пройден; voice-proxy убирает URL/Markdown из TTS; старый emulator LLM-ключ не используется.
- API `quasar-server :31116` без auth; закрыть после согласования.
- После voice pipeline HA `assist_satellite` иногда остаётся `responding`;
  automation `assist_satellite_responding_watchdog` перезагружает ESPHome
  entry через 25 с (кроме alarm_ringing=on).

## Карта

- Факты и ограничения — [`STATE.md`](STATE.md)
- **Восстановление — [`REBUILD.md`](REBUILD.md); решения — [`DECISIONS.md`](DECISIONS.md); секреты — [`SENSITIVE-DATA.md`](SENSITIVE-DATA.md)**
- Задачи и этапы — [`PENDING.md`](PENDING.md)
- Хронология — [`HISTORY.md`](HISTORY.md)
- Команды и откат — [`OPERATIONS.md`](OPERATIONS.md)
- Архитектура — [`ARCHITECTURE.md`](ARCHITECTURE.md)
- Политики/риски — [`SECURITY.md`](SECURITY.md)
- Station provisioning — [`PROVISIONING.md`](PROVISIONING.md)
- OTA/ADB fortress — [`OTA-FORTRESS.md`](OTA-FORTRESS.md)
- Сервер и HA — [`../projects/home-assistant/NETTOP.md`](../projects/home-assistant/NETTOP.md)
- HA runbook — [`../projects/home-assistant/HA.md`](../projects/home-assistant/HA.md)
- Стек/`ctl.sh` — [`../projects/home-assistant/stack/README.md`](../projects/home-assistant/stack/README.md)
- Голосовые команды — [`../projects/home-assistant/ha-voice/README.md`](../projects/home-assistant/ha-voice/README.md)
- Hermes fallback — [`../projects/home-assistant/hermes/README.md`](../projects/home-assistant/hermes/README.md)
- BLE gateway — [`../projects/station/ble-gateway/README.md`](../projects/station/ble-gateway/README.md)
