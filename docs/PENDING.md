# PENDING — открытые задачи

> Только задачи. Факты — `STATE.md`, хронология — `HISTORY.md`, команды —
> `OPERATIONS.md`. Закрытые этапы не переписывать здесь.

## Сделано (ссылки)

- MA/Spotify/Soloist, HA Assist base, Pulsar `v0.4.1`, ESPHome, MA
  Sendspin, TTS/reboot/rollback, кнопки 1/3/4/5, голос и shopping list,
  голосовые команды Bluetooth/gateway/volume — см. `HISTORY.md` и
  `STATE.md`.
- HA integration Music Assistant добавлена; MA-плеер Pulsar и
  `music_assistant.play_media` есть; announce проверен.
- ✅ HA lights: «выключи/включи лампу» (ночник/лампа у кровати),
  «светильник», «ленту» — локальные `StationLightOn/Off`; проверено
  pipeline «выключи лампу» → «Выключаю лампу».
- ✅ Сценарий «я сел работать» / «включи рабочий режим» →
  `StationWorkMode` → `script.work_mode`; включает все три светильника
  спальни, проверено pipeline.
- ✅ Обратный сценарий «black out» / «blackout» / «блэкаут» / «блэк аут» →
  `StationBlackout` → `script.blackout`; выключает те же три светильника,
  проверено pipeline. ASR-вариант «блок аут» добавлен после реального
  распознавания GigaAM.

## P1 — Proxmox / HAOS / K3s VM cutover

- ✅ PVE `9.2.24`; VM `100 haos` (2/4 GiB/32 GiB, `.28`) и VM `101 lab-k3s`
  (8/8 GiB/200 GiB, `.27`) running; startup order 1/2; Terraform plan — no changes.
- ✅ HAOS `2026.10.4` healthy; HA restored, integration endpoints переведены
  на `.27`, `internal_url` и trusted proxy обновлены; alarm media URLs
  исправлены на `.27:8123`.
- ✅ **Matter Server `9.2.0`** перенесён в HAOS add-on; HA integration
  использует `ws://127.0.0.1:5580/ws`; K3s Matter Deployment удалён,
  PV `runtime-matter` сохранён Retain.
- ✅ **Music Assistant `2.10.8`** перенесён в HAOS add-on; data (SQLite/
  settings/Spotify/Sendspin) скопированы из `runtime-music`; HA integration
  автоматически переключилась на add-on, station player зарегистрирован;
  Hermes MCP переключён на `.28:8095`; K3s MA Deployment удалён, PV сохранён.
- ✅ K3s runtime: GigaAM/Whisper/Piper/Hermes, observability и Tailscale
  Operator `1.104.2`; `pl-home` → HAOS, `/grafana` → Grafana; HAOS proxy
  `.27:8123` сохраняет локальные callbacks. Health exporter — 7 endpoints.
- ✅ VM `.27/.28` переведены на static в guests (netplan/HAOS Supervisor);
  Proxmox host `.26` static с установки. Router reservations для них не нужны;
  проверить, что адреса вне DHCP-пула, при следующей смене роутера.
- ⏳ Wi-Fi: Ethernet обязателен; `home-wifi` PCI mapping создан, но Wi-Fi
  не передан VM. Нужен отдельный routed/NAT или внешний Wi-Fi bridge design;
  не отключать Ethernet до приёмки.
- ⏳ Host/VM power-loss reboot, полный disaster restore на запасном железе,
  offsite backup и host Tailscale emergency access.
- ⏳ Живая приёмка после cutover: микрофон/wake, alarm/stop/resume,
  Spotify/MA transfer, локальные HA intents; read-only E2E уже pass.
- ✅ Полные HA backups до/после миграции (`final-ma-matter`, 35.6 MiB)
  созданы и скопированы на Mac; checksums записаны. ⏳ Осталось проверить
  Spotify/Sendspin после reboot HAOS; Spotify rate-limit уже снялся.

## P1 — Matter / Yandex LED strip

- ✅ Matter Server `9.2.0` работает как HAOS add-on `core_matter_server`;
  HA integration `Matter` подключена к `ws://127.0.0.1:5580/ws`.
  Прежние K3s data `runtime-matter` сохранены Retain как rollback.
- ⏳ Добавить светодиодную ленту Яндекса (Matter over Wi-Fi) через HA
  Companion: Matter → Add device → QR/11-значный код. Телефон на 2.4 ГГц,
  Bluetooth и location permission.
- Если лента уже в «Дом с Алисой»: использовать multi-fabric/share-код из
  приложения Яндекса; иначе сброс и повторное добавление. Проверить
  on/off, яркость, цвет и возврат в HK/HA после restart.
- IPv6 не отключать: на `wlp4s0` нужен link-local/mDNS; при проблемах
  проверить multicast/IGMP snooping и 2.4 ГГц.

## P1 — Xiaomi Home / MJGJD02YL + Mi Bedside Lamp 2

- ✅ Официальная Xiaomi Home integration `v0.5.0` установлена в HA
  (`custom_components/xiaomi_home`), component loaded; config entry
  `01M5Y1M47A9VKA6Z98BEJT9FR8` loaded.
- ✅ Вход Xiaomi China выполнен (OAuth redirect `homeassistant.local`
  обойдён заменой host на `192.0.2.25`). Импортированы:
  - Monitor Light Bar 1S `MJGJD02YL` / `yeelink.light.lamp22` —
    `light.yeelink_cn_919937191_lamp22_s_2`;
  - Mi Bedside Lamp 2 / `yeelink.light.bslamp2` —
    `light.yeelink_cn_827036414_bslamp2_s_2_light`.
- ✅ Дубликат Mi Bedside Lamp от HomeKit Controller
  (`MiBedsideLamp2-F723`, entry `01M5XYMMY3DKZPEFTBJNQJATHF`) удалён;
  backup — `~/backups/ha-duplicate-lamp-20261027-114648/`. Осталась одна
  `light.*`-сущность на каждую лампу.
- ⏳ Проверить on/off, brightness, color temperature; BLE-пульт отдельно не
  добавляется. Если какая-то MIoT-функция не появится: официального
  локального API нет; вариант — ESPHome/Tasmota с разборкой/пайкой
  (`vanzi11a/esphome-xiaomi_monitor_light_bar_s1`).

## P1 — TTS: приёмка Puriso и будущий свой голос

- Внедрение Puriso выполнено; факты — `STATE.md`, протокол и fine-tuning этапы — [DEPLOY-RICARDO-PURISO.md](DEPLOY-RICARDO-PURISO.md).
- Осталось пользователю послушать на станции и выполнить реальный микрофонный запрос «Рикардо, какой будильник?»/обычный вопрос.
- Для дообучения выбрать целевой тембр и пригодные записи/teacher; подготовить корпус и holdout, выбрать compute, провести pilot → ONNX под новым voice ID → A/B с Puriso. Целевой голос пока не указан; обучение не начато.
- Kokoro исключён; не разворачивать прежний план Silero поверх принятого Puriso. Сохранить Irina для отката, в будущем Puriso — резерв собственного голоса.

## P1 — Pulsar Rust fork (Ricardo profile, not deployed)

- ✅ Отдельный форк `OWNER/pulsar`, ветка `feat/ricardo-rust`; база v0.5.0.
- ✅ Перенесены «Рикардо»/pre-roll/тихая отмена/стоп-в-HA/локальные команды;
  Zigbee и auto-update выключены, `nearest_answers=0`.
- ✅ Cross-build aarch64/armhf, clippy обоих targets, 30/30 `pulsar`,
  20/20 `pulsar-audio`, installer Go-тесты.
- ⏳ HIL-приёмка: загрузка `libtflite.so` и модели на Midi, микрофонный wake,
  ESPHome entities, Spotify/MA, alarm stop/resume, откат. Не ставить на
  станцию без отдельного backup/подтверждения.
- Детали: `projects/station/PULSAR-RUST.md`.

## P0 — Pulsar finalization

- **Spotify на Pulsar:** Connect-плеер играет, pause/next/volume
  проксируются; Spotify 429 снят, search/play проверены. MA Liked Songs sync
  отдельно не перепроверялся.
- **Bluetooth-пара:** 4 нажатия или «включи блютуз» включают A2DP на 90 с;
  проверить телефон, звук и восстановление Wi-Fi после окна. Помнить:
  discoverable может уронить Wi-Fi/HA; `Station-Gateway` — аварийный канал.
- **5 с / gateway:** физически проверено; проверить 3 нажатия и голосовые
  «включи/выключи гейтвей» без nRF Connect (status `READY`/`IDLE`).
- **`assist_satellite` stuck:** после voice иногда `responding`; reload
  ESPHome config entry возвращает `idle`. Проверить, не мешает ли
  следующему wake; при повторе — отдельный баг Pulsar/HA.

## P0 — Spotify favorites / voice

- HA/MA integration готова; `media_player.iandeks_stantsiia_midi` и
  `music_assistant.play_media` доступны.
- Spotify Connect plugin включён (Soloist, `up77eb9a80`); устройство
  анонсируется по mDNS, phone pairing и Web API playback проверены.
- Voice-роутер добавлен: `input_select.music_route`, команды режима,
  `script.music_play_favorites`; Connect-ветка использует Spotify Web API
  и уже запускает Liked Songs.
- ✅ Spotify 429 снят 2026-10-27; Connect search/play проверены на
  `Белые розы`/`Nirvana`. Liked Songs (`library://playlist/9`) в MA отдельно
  не перепроверялся; fallback-ответ сохранён.
- «включи любимое» и варианты добавлены; при пустом плейлисте команда
  отвечает «Любимый плейлист пока не синхронизирован…», без ошибки.
- ✅ Generic «включи музыку» теперь локальный `StationMusicPlay` →
  Liked Songs, без ухода в Hermes.
- ✅ «Следующая/следующая песня/следующий трек» и «предыдущая/предыдущий
  трек» — локальные `StationMusicNext/Previous`, без Hermes.
- ✅ Голая «продолжи» (без «музыку») добавлена в `StationMusicResume` и
  проверена pipeline: ответ «Продолжаю», playback resumed.
- После retry проверить успешный `play_media`; «пауза/стоп» — на реальном
  playback.
- ✅ **Gate:** «включи любимое» в Connect реально играет Liked Songs на Pulsar.
- ✅ **Source router:** проверен; Connect branch запускает Liked Songs.
- **Volume:** `sync_spotify`; `spotify_connect_set_volume` сначала ставит
  локальный `media_player.volume_set`, затем необязательный Spotify Web API.
  ✅ «громкость пять» → 0.5 проверена pipeline; ошибка Spotify больше не
  блокирует локальную громкость. ✅ Голосовой retest «громкость три» пройден
  2026-10-26 на GigaAM v3.
- ✅ **Gate:** телефон видит Connect device; Web API `play` — 204; metadata идёт.
- **Осталось:** phone→speaker/transfer.

## P0 — Schedules: alarms / timers / reminders / notes

План и границы: `PLAN-SCHEDULES.md`. Реализовано 2026-10-27.

- ✅ Будильники: списки `todo.alarms_once/daily/weekdays/weekends`;
  scheduler `schedules_alarm_dispatcher`; голос create/list/delete/disable/
  update/enable; повтор, snooze 9 мин, grace 10 мин; звонок через прежний
  alarm path; startup-срабатывание проверено.
- ✅ Таймеры: 8 `timer.taimer_*` с restore; голос start/status/pause/resume/
  cancel; `timer.finished` → TTS + phone; восстановление active timer и
  названия после restart проверено.
- ✅ Напоминания `todo.reminders` и заметки `todo.notes`: голосовой CRUD,
  due datetime, TTS + mobile notification.
- ✅ Dashboard «Помощник» `/assistant-schedules`.
- ✅ Старый один слот `alarm_next`/`alarm_scheduler` выключен и скрыт;
  файлы оставлены для rollback.
- **Осталось:** живой микрофонный retest «Рикардо, поставь будильник …»,
  «какие будильники», «удали будильник работа», «поставь таймер пицца»,
  «напомни …», «запомни …» через GigaAM/Pulsar (текстовые проверки пройдены).
- ✅ Звонок: `alarm_loop` проигрывает локальный 8-секундный
  `www/alarm_ringtone.wav` на непрерывном повторе, пока
  `alarm_ringing=on`; `alarm_stop`/snooze прерывают его. `alarm_announce`
  с chime+TTS оставлен отдельно. Текстовый/API smoke-тест пройден.
- ✅ Stop при звонке проверен: `alarm_stop` переведён в `mode: restart`,
  `alarm_ramp` проверяет `alarm_ringing` перед первым и каждым шагом; после
  stop громкость не растёт (проверено с задержкой 0 и 1 с, 12 с наблюдения).
  **Осталось:** живой микрофонный stop/snooze.
- **Осталось:** после недели приёмки удалить legacy-хелперы/automation из
  YAML, обновить `REBUILD`/`CONTEXT`/README, убрать скрытые сущности.
- **Осталось:** решить, нужен ли динамический `timer_list` (без лимита 8)
  или HACS Wakey pilot; сейчас не выбран.
- Нативные будильники Яндекса по-прежнему не синхронизируются.
- Наблюдение: во время TTS ESPHome media player иногда пишет
  `KeyError: <MediaPlayerState.ANNOUNCING: 4>` — это не schedule-логика, а
  несовпадение enum HA 2026.10.4/ESPHome; проверить отдельно, если мешает.

## P1 — custom wake word «Рикардо»: оценка качества

- Обучение и on-device интеграция завершены; текущие факты — `STATE.md`.
- Измерить recall в тишине/на фоне, ложные активации в час/день и задержку.
- Цели прежнего плана: recall ≥90% clean / ≥80% noisy, FP ≤1/день, latency <500 мс.
- Сохранить «Алиса»/«Яндекс» и полный откат. Текущий запуск — по явному запросу пользователя для испытаний.
- При пропусках/FP — независимые реальные записи и следующая итерация обучения.
- Лицензии голосов проверить до публикации модели.

## P1 — Hermes / свободная речь

- ✅ Low reasoning для voice через proxy + streaming HA/Hermes/Piper
  внедрены 2026-10-27; ранние реальные аудиобайты до intent-end проверены
  без воспроизведения на колонке. См. `DEPLOY-HERMES-STREAMING.md`.
- После streaming: микрофонный retest обычного вопроса и веб-поиска,
  слуховая проверка пауз/duck/resume; акустический baseline/p50/p95 ещё впереди.

- ✅ Hermes Agent `v0.22.5` в K3s Pod `hermes`; ChatGPT/Codex OAuth, модель
  `gpt-5.6-luna`; gateway `127.0.0.1:8642`, HA proxy/API `.27:8643`.
- ✅ MCP full access: HA MCP 73 tools; Music Assistant FastMCP 86/86 tools;
  `platform_toolsets.api_server` = `mcp-homeassistant`, `mcp-music-assistant`,
  `web`. MA plugin experimental; all permission flags enabled. Tokens —
  `HERMES_HA_TOKEN`/`HERMES_MA_TOKEN` in `.env` (600). Проверено `mcp test`
  обоих серверов и реальным MA-запросом списка плееров.
- ✅ Pipeline `Рикардо` переключён на `conversation.hermes_agent` с
  `prefer_local_intents: true`; локальные intents проверены первыми.
- ✅ `web_search` (backend `openai-native`) проверен через HA conversation;
  свободный вопрос «что такое фотосинтез» отвечен Hermes.
- ✅ Музыкальный каскад: search → top-1 → `music_play_uri`; кандидаты
  «второй/третий»; ошибки 429/auth/device/premium.
- ✅ Исправлен HA `music_search`: Spotify `tracks/albums/playlists/artists`
  читаются как mapping (`content.get(...).get('items')`), без
  `TypeError: builtin_function_or_method`. Search/play проверены.
- ✅ Личность Hermes: `SOUL.md`, `agent.personalities.voice` и HA prompt
  говорят «Рикардо, личный ассистент»; проверено «как тебя зовут» →
  «Я Рикардо, ваш личный ассистент».
- ✅ Реальный микрофонный retest общего вопроса пройден 2026-10-26:
  «что такое фотосинтез», «кто такой робби уильямс», «кто такой эльдар
  автоподбор». STT — GigaAM v3; Hermes prompt восстанавливает искажённые
  слова. Осталось голосом: идентичность, «включи музыку», «громкость пять»,
  будильник/таймер.
- Проверить, что при падении Hermes локальные таймеры/будильник/громкость
  продолжают работать; Hermes не владеет alarm/timer state.
- Spotify 429 снят; Connect search/play проверены. MA Liked Songs sync
  отдельно не перепроверялся.
- Старый emulator `LLM_API_KEY` — исторический путь, не используется.

## P1 — remote access / HA hardening

- План и исследование: `REMOTE-ACCESS.md`; старый host Serve заменён
  Tailscale Operator; манифесты — `projects/home/kubernetes/tailscale/`.
- ✅ **Реализовано 2026-10-28:** Operator `1.104.2` в K3s;
  `https://pl-home.example-tailnet.ts.net/` → HAOS, `/grafana/` → Grafana;
  DNS `pl-home` указывает на operator proxy `tag:k8s`; HA MFA, external URL,
  trusted proxy `.27`; `xiaomi-17` в tailnet.
- ✅ Проверено из tailnet: `/` HTTP 200, `/grafana/api/health` HTTP 200;
  `tailscale/verify-tailnet.sh` PASS; старый host Serve route не используется.
- Публичного HA нет; `8123/8095/8099/6053/6638` не проброшены; ONT/AX3
  не менялись. Статический IP/WireGuard — альтернатива, не выбрана.
- ⏳ Host Tailscale emergency access: на Proxmox host `tailscaled` не
  запущен; при падении K3s VM/operator останется только LAN/Ethernet.
- ⏳ Проверить power-loss/reboot обеих VM и хоста; ONT WAN `22/443`;
  offsite backup. Не выставлять сервисные порты в интернет.

## P1 — API 31116 без авторизации

- `quasar-server` публикует `0.0.0.0:31116`; `POST /devices/...` и
  `PATCH /devices/...` без auth.
- Предложено: `ports: "127.0.0.1:31116:31116"` или auth.
- Нужно согласование; станция использует внутренний `:31115`.

## P1 — network provisioning

- BLE gateway работает; 1/3/4/5 и HTTP `:8099` — см. `PROVISIONING.md` и
  `projects/station/ble-gateway/README.md`.
- Осталось при необходимости:
  1. Web Bluetooth-страница (необязательно; nRF Connect работает).
  2. Sound pairing как офлайн-запас; нужен режим сопряжения Midi.
  3. AP + captive portal; только если BLE окажется недостаточно.
  4. DPP — только unassociated; `p2p0` при рабочей сети не запускать.
  5. Token — 8-значный; старый отвергается. Не публиковать.

## P1 — factory/recovery

- Factory-образа Midi `cucumber` нет; `yandexmidi` `.298` — Station 2.
- Raw-дампа `bootloader`/`boot`/`dtb`/`recovery` нет.
- Нужно согласование на read-only дамп; тест только на запасной.
- Factory reset не считать unbrick: сносит `/data`, root/SSH/OTA/guard.

## P1 — Mac DNS/proxy (закрыто)

- Router DNS на Mac больше не указывает; старый DNS/`https-proxy` не
  используется.
- `alice-mitm-bootstrap` остановлен и удалён с Desktop; source —
  `archive/legacy/alice-mitm-bootstrap/`, архив —
  `artifacts/desktop-originals/alice-mitm-bootstrap-20261026.tar.gz`.
- `v3rt3p-stack` остановлен и удалён; source/compose — 
  `archive/legacy/v3rt3p-stack/` + `repos/` (commits: `UPSTREAM.md`);
  upstream — `https://github.com/v3rt3p`. Не запускать без review.

## P1 — cleanup / recovery

- ✅ 2026-10-27: удалены generated junk (`.DS_Store`, `__pycache__`, `.pyc`),
  битые symlink’и `services/`; точные дубли в `artifacts/desktop-originals`
  убраны; Mini 2 guide/rollback вынесены в `archive/legacy/mini2/`;
  старые логи — `archive/logs/2026-10/`.
- ✅ v3rt3p source repos архивированы в
  `archive/legacy/v3rt3p-stack/repos/` (commits — `UPSTREAM.md`).
- ✅ Добавлены `REBUILD.md`, `DECISIONS.md`, `SENSITIVE-DATA.md`;
  обновлены `README`, `docs/README`, `projects/station/configs/README`, `artifacts/README`,
  `backups/README`, `logs/README`, `resources/README`.
- **Осталось:** offsite encrypted backup; `chmod 600` для
  `projects/station/configs/stack.env`, `backups/remote-access-*`, `secrets/dpp/*`;
  вынести/архивировать remote-access research; v3rt3p models/assets;
  factory/recovery image Midi (нет); ONT WAN `22/443`.
- **Не удалять:** `secrets/`, `backups/emergency`, `backups/remote-access`,
  `archive/legacy/alice-mitm-bootstrap`, `archive/legacy/v3rt3p-stack/repos`.

## P2

- SSH по ключам, отключить password auth; ADB/SSH только LAN.
- MOK/`/etc`/HA config/MA/STT/TTS backup вне Mac, зашифрованно.
- `8123/8095/8099/6053` не в интернет; ONT WAN `22/443` — закрыть.
- Factory/recovery image Midi и raw-разделы — нет; только с согласованием.
- Secure Boot PK `DO NOT TRUST - AMI Test PK` (HSI 0): решить отдельно;
  замена PK/KEK на AMI-прошивке рискованна, только с физдоступом.
- Ресурсы (`pulsar`, `apkg`, `yaff`, AirPlay) — только на запасной Midi.
- `yse` — нужен исходник/ссылка.

## Kubernetes: дальнейшая эксплуатация

Миграция приложений и observability выполнена 2026-10-27; текущий runbook —
[KUBERNETES-OPERATIONS.md](KUBERNETES-OPERATIONS.md). Отдельно остаются:

- длительное наблюдение 48–72 часа и микрофонный retest пользователем;
- регулярный encrypted/offsite backup с проверкой полного restore на запасном хосте;
- внешний канал Alertmanager/внешний heartbeat;
- полная latency instrumentation voice pipeline;
- private registry/build pipeline для собственных images (сейчас recovery OCI archive);
- отдельная проверка Ethernet/failover, если будет подключён кабель.

## Proxmox / HAOS / Terraform / Tailscale Operator

Выбран Proxmox; ISO и внешняя recovery-копия подготовлены, гипервизор не менялся.
Границы и варианты — [PROXMOX-HAOS-IAC.md](PROXMOX-HAOS-IAC.md).
Физическая установка — [INSTALL-PROXMOX.md](INSTALL-PROXMOX.md).
Далее нужны VM/IaC и восстановление; operator также требует отсутствующие сейчас OAuth/WIF
credentials и tag policy. Не считать подготовленный K3s автоматически Proxmox/HAOS.
