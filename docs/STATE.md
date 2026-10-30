# STATE — текущее состояние

Обновлено: 2026-10-31. Публичные адреса — примеры; реальные — в
`secrets/NETWORK.md`.

## Host / VM

- Proxmox VE `9.2.24`, kernel `7.0.14-26-pve`, host `pve`.
- Железо: DEXP Jupiter JU01CY, i7-12650H, 16 GiB RAM, NVMe 512 GB.
- Management `vmbr0`: `192.0.2.26/24`; uplink — Wi-Fi `RTL8821CE` в
  4-address/WDS к `vmbr0`, Ethernet — резерв по carrier. NAT/PCI passthrough
  не используются; VM сохраняют L2-адреса и IPv6 multicast.
- `ricardo-uplink` держит ровно один physical uplink; две полные перезагрузки
  PVE без Ethernet прошли.
- VM `100 haos`: 2 vCPU / 4 GiB / 32 GiB, HAOS `2026.10.4`, `192.0.2.28`.
- VM `101 lab-k3s`: 8 vCPU / 8 GiB / 200 GiB, Ubuntu 24.04.5, K3s,
  `192.0.2.27`; startup order 1/2.
- CT `102 vpn`: unprivileged Debian 13, 1 vCPU / 512 MiB / 4 GiB,
  `192.0.2.29`, onboot, ocserv `443/tcp+udp`, пул `10.77.0.0/24`.
- Terraform: provider `bpg/proxmox 0.118.1`, plan — No changes; state
  root-only на PVE, копия в `secrets/`.
- Wi-Fi/Ethernet uplink runbook: `projects/home/proxmox/wifi-uplink/`.

## HAOS

- HA: `http://192.0.2.28:8123`; `internal_url` и trusted proxy обновлены.
- Add-ons:
  - Music Assistant `2.10.8`: UI/API `8095`, stream `8097`, Sendspin `8927`;
    данные перенесены из K3s `runtime-music`.
  - Matter Server `9.2.0`: `ws://127.0.0.1:5580/ws`; paired 1 узел —
    Yandex LED strip `YNDX-00544` («Лента», `light.spalnia_lenta`),
    on/off, яркость и цвет проверены 29.10. Дублирующий набор сущностей от
    повторного интервью остался неактивным.
- K3s HAProxy и HA Container удалены; HAOS — единственный HA runtime.

## K3s

- K3s `v1.35.9+k3s1`, node `lab-host`; data-dir `/srv/k3s/runtime`; SQLite;
  local-path PV с `Retain`.
- Namespaces: `home-assistant`, `monitoring`, `traefik`, `cert-manager`.
- Приложения: GigaAM `10305`, Whisper `10300`, Piper `10200`,
  Hermes gateway `127.0.0.1:8642` + voice proxy `8643`, health exporter `9116`.
- `8643` доступен только HAOS/K3s/loopback через nftables.
- MA/Matter Deployments, PV и data из K3s удалены; `runtime-ha` оставлен как
  архив старого HA Container.
- Observability: kube-prometheus-stack `92.14.0`, Loki `20.0.2`,
  Alloy `1.14.3`; четыре dashboard JSON через Helm ConfigMap.
- Grafana: LAN `http://192.0.2.27:30300/` (корень), HTTPS
  `https://grafana.home.example.com/`.
- HTTPS ingress `*.home.example.com` (Traefik `42.2.0` + cert-manager
  `v1.21.3`, ACME DNS-01 Cloudflare, wildcard Let's Encrypt prod до
  2027-01-28): `ha`, `grafana`, `alarms`, `control` → узел `.27`, Cloudflare
  DNS only. Traefik в `hostNetwork` (`80/443`), редирект на HTTPS, IP
  allow-list LAN + pod CIDR; HA-источник остаётся `.27`
  (`trusted_proxies /32`). WebSocket HA и Grafana проверены.
  Файлы — `projects/home/kubernetes/ingress/`; приватные имена/токен —
  `secrets/home/ingress.env` и `secrets/home/vpn/cloudflare.env`.

## Remote

- Удалённый доступ — собственный VPN (ocserv, CT `102`): `vpn.example.com`,
  профили `lan`/`full`, пароль + TOTP или клиентский сертификат.
- HTTPS-имена LAN/VPN: `https://{ha,grafana,alarms,control}.home.example.com`
  через Traefik + cert-manager.
- Публичного HA нет; `8123/8095/8099/6053/6638` наружу не проброшены.
- K3s management/metrics `6443/10250/9100/9116/8472` закрыты от обычного LAN;
  SSH — LAN, K3s API — через туннель.
- Аварийный доступ к Proxmox — LAN/Wi-Fi.

## Собственный VPN

- CT `102 vpn` (`192.0.2.29`): ocserv с паролем + TOTP и дополнительным
  входом по клиентскому сертификату, профили `lan`
  (split tunnel до домашней LAN, DNS `192.0.2.1`) и `full` (весь трафик).
  `443/tcp` + `443/udp`, nftables `drop` по умолчанию, masquerade
  VPN → LAN/WAN, SSH только из LAN, onboot.
- Проверены успешная
  аутентификация с верным TOTP, отказ при неверном TOTP и реальный туннель
  ping до `192.0.2.1` из изолированного netns.
- 2026-10-31: проброс `443/tcp+udp` ONT → AX3 → CT, DHCP reservation
  `.29` на AX3, сертификат Let's Encrypt для `vpn.example.com` (до
  2027-01-28). Вход с мобильной сети (профиль `lan`, DTLS) проверен.
- 2026-10-31: сертификат продлевается автоматически: certbot +
  `dns-cloudflare` (DNS-01), `certbot.timer`, deploy hook копирует
  fullchain/privkey в ocserv и перезапускает его. MSS clamp в forward chain
  стоит до accept. Ручные auth-хуки и TXT удалены.
- 2026-10-31: добавлен вход по клиентскому сертификату без пароля/TOTP.
  Частный CA (RSA 4096, 10 лет) и его ключ — только в
  `secrets/home/vpn/client-ca/`; в CT — `client-ca.pem` и `client-ca.crl`.
  ocserv: `auth = "certificate"`, `enable-auth = "plain[...]"` (пароль +
  TOTP остаётся), `cert-user-oid`/`cert-group-oid`. Выпущен сертификат
  `<user>` для `phone` (RSA 2048, 2 года, CN = пользователь, OU = группы).
  Управление — `vpncerts.py` (`init-ca`/`issue`/`revoke`/`list`); отзыв
  обновляет CRL и перезагружает ocserv. Профиль при входе по сертификату
  выбирается полем Group (`lan` по умолчанию или `full`); без группы
  сервер просит выбрать её при подключении.
- 2026-10-31: группа `ci` для GitHub Actions: пул `10.77.1.0/24`, маршрут
  только до K3s, без DNS; nftables `forward` разрешает только `tcp/22` на
  K3s и `drop` остальное. Сертификат `ci` (OU `ci`) выпущен, проверены
  `--authenticate` и реальный туннель из netns: открыт только `K3s:22`,
  HA/PVE/станция недоступны. CI-деплой — `ci-deploy` (SSH + helper-only
  sudo) и ServiceAccount `ci-deploy` в `home-assistant`.
- Исходники: `projects/home/proxmox/vpn/`; приватные учётные данные —
  `secrets/home/vpn/`.

## Станция

- Yandex Station Midi `cucumber`, IP `192.0.2.13`; `maind` остановлен.
- Pulsar Rust `v0.5.2-4-g9b5e86a-dirty` — установленная armhf-сборка
  (`/lib/ld-linux-armhf.so.3`; локальный форк, база upstream `v0.5.2` =
  `4cba44a`), собрана проектным Makefile `ARCH=armhf`; включает повторный
  wake, заменяющий активный диалог, и экспериментальный локальный
  endpointing. Процесс запущен с `PULSAR_LOCAL_ENDPOINTING=1`, унаследованным
  supervisor'ом от `ctl.sh reload`; это не boot-persistent: после reboot или
  нового supervisor без env feature выключен. Health 30.10: HA voice
  subscribed, Sendspin/MA переподключились без рестарта MA, crashloop нет;
  живая голосовая приёмка endpointing ещё нужна. StationCancel без речи —
  изменение HA-конфига, не бинарника.
  Установленный бинарник — SHA256 `7081ef8e…` (Ricardo pre-roll 256 ms);
  `pulsar.prev` — прежний endpointing `0b0c48e4…` (512 ms). Historical
  known-good `d2197f6a…` (`v0.5.2-3-gc57d521-dirty`) — теперь
  `pulsar.pre-endpoint-20261030`, не `pulsar.prev`;
  `pulsar.prev.pre-endpoint-20261030` — более старый pre-endpoint backup
  `69310a37…`; `v0.5.2-2-g95ce568` и `v0.5.0-1-g908a027` — только private
  backup вне Git.
  Legacy C++ checkout,
  patch и binaries — private в `secrets/local/recovery/`.
  ESPHome `6053`, Sendspin `8928`, HTTP control `8099` (bearer), ADB `5555`, SSH `22`.
- ESPHome: 33 entities, Noise; HA satellite работает.
- SSH: dropbear `v2026.94`; ключ Mac `~/.ssh/ricardo_station_ed25519` →
  `/root/.ssh/authorized_keys`. Вход по паролю оставлен как fallback
  осознанно (решение); ADB/SSH доступны только из LAN.
- Установленная модель wake word — кандидат v27
  (`v27-ambient-capacity-sel22-20261030`, SHA256 `4f645834…b0e12444`, 207 720 Б).
  Установлен 30.10 по прямому запросу пользователя model-only: HA/MA доступны,
  бинарник Pulsar не менялся; threshold 0.9, окно 10. Живая голосовая приёмка
  не выполнялась — private `active.json` фиксирует `live_voice_acceptance=pending`;
  установка не является приёмкой.
- Best-known/reference по решению пользователя остаётся v24
  (`v24-ambient-scratch-20261030`, SHA256 `83b92351…314787a`) и не заменяется
  установленным кандидатом до его живой приёмки. V24 обучена с нуля с Puriso2,
  личными примерами и двумя новыми ambient-дорожками; на обучающем ambient
  0 событий вместо 5 у v22 (не независимая оценка), на прежней ASR-assisted
  проверке 161/164 против 163/164 у v22. Designation хранится private отдельно
  от installed-записи.
- Кандидат v27 — более ёмкий MixedNet (pointwise 128×4, первая свёртка 64;
  те же MixConv-ядра, stride 3, residual, контекст 204×40), обучен с нуля,
  seed 42, 23 эпохи / 2300 шагов из запрошенных 15000 (early stop; RTX 5070,
  ~2 мин обучения). 138 113 параметров против 40 385, int8-модель ≈207 КБ,
  вход `int8[1,3,40]`, выход `uint8[1,1]`.
  Потоковый выбор по validation среди всех 23 checkpoint: эпоха 22 даёт
  32/32 по манерам речи и 2 события на validation-наборе отрицательных
  примеров (там есть и намеренно похожие слова, это не обычный ambient
  и не независимая оценка); у v22 — 5, у v24 — 2.
  Python-регрессия на 164 размеченных обращениях: 161 совпадение и 15 событий на
  сложных негативах (v22 — 163/16, v24 — 161/14): явного улучшения
  относительно v24 не показала; это регрессионный набор, а не слепой тест.
- C++/firmware-проверка того же замороженного файла e22 (в отчёте обозначен
  как `v25`; SHA256 `4f645834…b0e12444`) на станции, cutoff 0.90, окно 10,
  reset + прогрев 1 с, CPU-only: 162/164 на регрессионном наборе против
  159/164 у v24 и 164/164 у v22; на validation-срезах 31/32 против 29/32
  и 30/32.
  По манерам: commands 22/22, normal 35/36, quiet 34/35, relaxed 18/18,
  sharp 18/18, whisper 35/35. Похожие слова (130 с, стресс-набор):
  16 срабатываний против 17 у v22/v24; носовое дыхание 0 у всех.
  Медианная задержка 172 мс против 200 мс у v24 и 148 мс у v22.
  Модель загружается штатным `libtflite.so` как `int8[1,3,40]`, окно 10.
  Это небольшое улучшение относительно v24 на одном C++-стенде, а не общее
  превосходство: Python-стенд дал 161/164 одинаково с v24, обычный ambient
  на час не измерялся, 130-секундный набор — не бытовой фон.
  Offline-кандидат установлен для живой проверки; private registry
  `v27-ambient-capacity-sel22-20261030` (плюс `v25` — отдельный val_loss-экспорт
  и `v26` — ранний архив выбора).
- Откат на best-known v24: профиль `projects/station/configs/wakeword-v24.json`
  через `projects/station/tools/wakeword-personalize/deploy.py` (команда в
  `OPERATIONS.md`). Откат на v22 остаётся отдельным профилем
  `projects/station/configs/wakeword-v22.json`. Device backup пары v24 до
  установки v27: `/data/pulsar/backups/model-20261030T160858Z-f28772d8`;
  независимая private копия —
  `secrets/local/backups/wakeword-v24-pre-v27-20261030T160839Z/`.
  Все веса сохранены private на Mac и WSL; installed-запись и hashes —
  `secrets/model-registry/ricardo/active.json`.
- Актуальная уточнённая разметка и PCM: `secrets/datasets/ricardo-fixed-20261030/`.
- Приватный датасет: `secrets/datasets/ricardo-20261029/` — исходные WAV,
  PCM-контексты, разметка по версиям, архив обучающих признаков и SHA256.
- Деплой 30.10: бинарник с повторным wake установлен через
  backup → `pulsar.new` → `.prev` → `ctl.sh reload`; после рестарта HA
  подключён и voice subscribed, Sendspin/MA переподключились сами.
  Отдельно в HA-конфиге применён `StationCancel` без `speech` (бинарник не
  менялся) через `intent_script.reload` без рестарта HA; `ha core check`
  прошёл, тихий `conversation.process` («отмена») вернул `action_done` без
  речи. Живая голосовая приёмка ещё нужна.
  Backup: `/data/pulsar/backups/deploy-20261029T200123Z`,
  `secrets/local/backups/pulsar-pre-wake-cancel-20261030-0101/`,
  live HA — `secrets/local/backups/ha-intent-pre-deploy-20261030/`.
- Ricardo pre-roll 256 ms: установлен `v0.5.2-4-g9b5e86a-dirty` SHA256
  `7081ef8e…` (armhf, `Fixed(8)`, front cut 256 ms; три реальные команды
  распознаны точно в replay-sweep 30.10, n=3 — эмпирика, не гарантия).
  Процесс с `PULSAR_LOCAL_ENDPOINTING=1`; HA voice subscribed, Sendspin/MA
  переподключились. `pulsar.prev` — прежний endpointing-бинарник `0b0c48e4…`
  (512 ms); старый historical `d2197f6a…` (`v0.5.2-3-gc57d521-dirty`) сохранён
  в `/data/pulsar/backups/deploy-preroll8-20261030T185700Z/` и
  `pulsar.pre-endpoint-20261030`; off-device
  `secrets/local/backups/pulsar-preroll8-20261030T185157Z/`. Живая приёмка —
  `PENDING.md`.
- Проверено: live wake 0.904, HA pipeline/TTS, HAOS reboot, Sendspin/MA
  reconnect и station reboot. Supervised `reload` завершает Sendspin-сессию
  goodbye-причиной `restart`, и MA переподключается сам: измерено 3.1 с и 3.1 с
  на `v0.5.2-3-gc57d521`, без рестарта add-on MA и без потери регистрации
  player. Рестарт MA нужен только как необязательное восстановление при реальном
  сбое, а не после каждого рестарта Pulsar.
- Пользователь подтвердил живую приёмку `v0.5.2-2` (wake+команда сразу после
  имени). На `v0.5.2-3` проверены wake 0.919/0.923, будильник: звонок, «стоп»
  с wake и без имени, snooze +9 минут с повторным звонком, resume Spotify
  Connect с исходным source и громкостью 0.3. Mute и кнопки не проверялись.
  Wake-free «стоп» Pulsar обрабатывает локально как паузу плеера; HA-автоматизация
  `alarm_local_stop_command_ends_the_occurrence` закрывает occurrence.
- BLE gateway: `Station-Gateway`, GATT provisioning, кнопки 1/3/4/5;
  4 нажатия могут временно уронить Wi-Fi — BLE остаётся аварийным каналом.
- QR/DPP provisioning удалён по решению пользователя: убраны оба автозапуска
  (onboot/rootkeeper), helper scripts, URI/key/force files и bootstrap state;
  dpp_config_processing=0. Сохранённые WPA сети не менялись, BLE работает.
  Прежние данные — только в private off-device recovery archive.
- OTA/ADB fortress активен: обновления no-op, ADB root только LAN.
- Egress guard: LAN/DHCP/NTP разрешены, публичный WAN и Яндекс — DROP.
- Известная проблема: после reboot станция иногда не отвечает в LAN при
  online-статусе роутера; BLE остаётся доступен, снятие питания на 20–30 с
  восстанавливает IP/HA/голос. Первопричина не установлена (см. `PENDING.md`).

## Голос / музыка / расписания

- Pipeline «Рикардо»: GigaAM v3 CTC int8 primary, Whisper `medium-int8`
  fallback; локальные intents → Hermes только для свободной речи.
- Локальный endpointing в Rust Pulsar (30.10): после VQE детектор конца фразы
  по энергии чанков (порог относительно уровня wake-слова и тихой части его
  окна, без абсолютных dBFS) шлёт ровно один graceful
  `VoiceAssistantAudio.end=true` на активный LISTENING-диалог после ~1 с
  тишины. Abort/отмена/повторный wake по-прежнему идут через
  `VoiceAssistantRequest.start=false`; HA VAD остаётся главным и может
  завершить раньше. Это экспериментальная эвристика: включается только
  `PULSAR_LOCAL_ENDPOINTING=1`, по умолчанию выключена; 30.10 включена в
  текущем supervised-процессе на Midi (не boot-persistent); живых измерений
  порогов нет, коэффициенты не откалиброваны. При нехватке окна/SNR wake
  или для turn без wake (кнопка, continued conversation, start_conversation)
  детектор не запускается. Pre-roll считается только pre-wake evidence:
  одного pre-roll недостаточно для завершения фразы, а команда, найденная в
  arbitration-буфере, помечается как live. Известные ограничения:
  команда целиком в pre-roll не завершается локально (неотличима от wake
  tail); хвост wake word в live и громкий earcon по энергии не гарантированно
  отличимы от речи — они полагаются на AEC и порог длительности.
- TTS: Piper `2.5.2`, `ru_puriso2-medium`.
- Hermes `v0.22.5`, модель `gpt-5.6-luna` через ChatGPT/Codex OAuth;
  low reasoning + streaming; HA MCP 73 tools, MA MCP 86 tools.
- Voice proxy `v6`: к v4-санитайзеру добавлен exact cancel guard. Если
  предыдущий ответ Hermes закончился вопросом, HA включает
  `continue_conversation` и обходит `prefer_local_intents`; распознанная
  «отмена» уходила в Hermes. Proxy перехватывает только точную
  нормализованную фразу последнего user-сообщения, отвечает пустым
  completion без upstream Hermes и без TTS (`action_done`, `processed_locally=false`,
  `continue_conversation=false`). Живой continued-run 30.10: cancel 0.02 с,
  `tts` events нет; свободный follow-up после отмены по-прежнему отвечает
  Hermes (2.5 с). SHA-256 `voice_proxy.py` в запущенном контейнере совпадает
  с исходником `projects/home-assistant/hermes/voice_proxy.py`; targeted
  `kubectl set image` изменил only `hermes` Deployment, остальные workloads
  сохранили resourceVersion. Helm release `runtime` v13 хранит values с v4:
  drift зафиксирован, канонические values и `versions.sh` обновлены на v6;
  следующий helm upgrade применит их. Ошибки upstream по-прежнему
  нормализуются в OpenAI error object. При
  раннем старте HA интеграция llama_cpp получает ConfigEntryNotReady и
  повторяет подключение, вместо постоянного setup_error. Агент
  `conversation.hermes_agent` восстановлен; живое обращение подтверждено
  пользователем. Проверены HA→Hermes model turn и startup fault/recovery
  на реальном коде интеграции HA. Предыдущая API-only приёмка этот сбой
  не обнаруживала; agent entity теперь обязателен в live HA tests.
- MA/Spotify: Soloist `1.3.10.13`, Spotify Connect device
  «Яндекс Станция Миди», music router `Music Assistant` / `Spotify Connect`.
- Будильники, таймеры и напоминания хранит scheduler; голосовые intents и
  Android работают через Connect API. Legacy helper-ы `todo.alarms_*` и
  `todo.reminders` удалены, `timer.taimer_1…8` отключены в registry
  (YAML-определения остаются в живом конфиге HA). Заметки — `todo.notes`;
  dashboard `/assistant-schedules` только для чтения. Отдельно добавлены
  Xiaomi-лампы `xiaomi_home` (`Lamp` bsLamp2, `Lighter` lamp22): on/off,
  яркость и цвет/CT проверены 29.10.
- Голосовые команды и точные entity/scripts — `projects/home-assistant/ha-voice/`.

## Планировщик, HA bridge, Android

- `solutions/scheduler` задеплоен в K3s `home-assistant`: один под, SQLite на
  PVC `scheduler-data` (`local-retain`), NodePort `30098`, bearer обязателен,
  readiness/liveness — аутентифицированный `-healthcheck`. Состояние
  переживает рестарт пода (проверено).
- `solutions/ha-bridge` работает в режиме execute на очереди `home`
  (`BRIDGE_EXECUTE=true`, 1 реплика, ledger на PVC `ha-bridge-data`), зовёт
  allowlisted HA scripts; второй исполнитель не запущен. Dry-run Job
  (`ha-bridge-dryrun`) остаётся одноразовой проверкой.
- Действующий владелец будильников, таймеров и напоминаний — scheduler.
  Старые HA-планировщики и их вспомогательные поля удалены из исходников и
  живой конфигурации; оставшиеся недоступные записи реестра очищены.
  Повторного исполнителя нет. HA остаётся исполнителем звука
  (`alarm_start`, `schedule_announce`, `alarm_stop`).
- `RingWindow` = максимальная длительность срабатывания (900 с по
  умолчанию). Истечение окна закрывает occurrence: tick в одной транзакции
  отменяет pending `FIRE` того же occurrence и ставит STOP в durable очередь
  (одна логическая запись, dedupe `expiry:stop:<kind>:<id>:<occurrence>`;
  доставка at-least-once, HA повторный STOP игнорирует). Bridge доставляет
  STOP, HA проверяет source+FIRE event id и не трогает более новый звонок.
  Причина: 30.10 будильник «Работа» звонил 10:00–11:09, при этом scheduler
  очистил active occurrence в 10:15 без события, а HA продолжала loop;
  `StopSchedule` в 11:09 был `FAILED_PRECONDITION`. Правка правила во время
  звонка (`UpdateSchedule`/enable) пока не эмитит STOP — отдельная задача в
  `PENDING.md`.
- Задеплоено 30.10 (build `d3b9f0b-05c1df84a9d1`): scheduler + ha-bridge из
  `solutions/`, HA `scripts.yaml` обновлён (`ha core check` +
  `script.reload`). `ha-control` не менялся (`5fb9800-84d66babca30`).
  Бэкапы: `secrets/local/backups/scheduler/20261030-113927`,
  `secrets/local/backups/ha-scripts-pre-ringwindow-20261030T064220Z`.
  Живой громкий 15-минутный expiry-тест не запускался; проверены unit/store/
  bridge/HA-template тесты, live no-op guard при молчащем звонке, health,
  `GetStatus.ready`, оба consumer-а и неизменный `ListSchedules`.
- Голосовые intents в `projects/home-assistant/ha-voice` вызывают canonical
  Connect API (`rest_command.scheduler_rpc`); endpoint —
  `input_text.scheduler_endpoint` (LAN NodePort base URL, durable via
  `!secret scheduler_endpoint`), токен — `secrets.yaml:scheduler_bearer`.
  Удалённый доступ — `https://alarms.home.example.com` (LAN/VPN) на тот же
  Service. Create/list/update/enable/disable/delete,
  timer start/pause/resume/cancel и reminder create проверены тихими
  `conversation.process` через builtin-агент; живая звуковая приёмка
  (audible) не выполнялась.
- Deploy/rollback: `projects/home/kubernetes/scheduler/` (manifest templates,
  render, preflight, backup). Реальные адреса — `secrets/home/scheduler.env`;
  rendered manifests — `secrets/local/rendered/`.
- `ha-control` (тот же solution) развёрнут отдельным Deployment: каталог
  `GET /v1/catalog` из live HA, очередь `control`; `CONTROL_EXECUTE=true`,
  consumer `ha-control-home` в allowlist планировщика рядом с `ha-bridge-home`.
  Скрипты попадают в каталог только с нативной меткой HA `scenario`
  (`CONTROL_SCENARIO_LABEL`); иконки — только отображение. Метка создана и
  назначена `script.work_mode` и `script.blackout` (registry merge, другие
  метки сохранены). Живой каталог: 2 сценария + 15 light/switch, служебные
  скрипты отсутствуют.
  LAN NodePort `30099`, HTTPS `https://control.home.example.com` (LAN/VPN).
  Доставка at-least-once, не exactly-once.
- `projects/mobile/android`: Kotlin-клиент (Views, одна Activity, Metro DI,
  `applicationId dev.proflyder.home`, minSdk 26 / target 36, только
  `INTERNET`, debug-signed). Вкладки **Home**, Alarms, Timers, Reminders,
  Connection; Home читает `GET /v1/catalog` (2 сценария + 15 устройств),
  команды идут через `home.control`/очередь `control`. Bearer token в
  Keystore AES-GCM, настройки и фильтр Home — Jetpack DataStore с миграцией
  из SharedPreferences; idempotency keys, redirects выключены. Debug APK
  установлен на телефон; холодный запуск и каталог проверены, интерактивные
  фильтры/сценарии/toggle ещё не приняты.

## Приватное / backups

- `secrets/NETWORK.md` — реальные IP/порты; `secrets/CREDENTIALS.md` — доступы.
- HA backups до/после миграции, K3s recovery, OCI images и 11 PVC cold copy —
  на Mac, вне Git; SHA-256 проверены.
- `secrets/kubernetes-research/` — kubeconfig, private values, Grafana password,
  recovery.
- HA automatic backup не настроен: `sensor.backup_next_scheduled_automatic_backup`
  и `last_successful` = `unknown`; CronJob/Velero для K3s нет.
- Полный disaster restore на запасном железе и offsite backup ещё не выполнены.

## Открыто

- Живая приёмка: будильник/стоп/snooze, Spotify transfer, phone→speaker,
  MA Liked Songs.
- Offsite encrypted backup, power-loss тест.
- Внешний Alertmanager/heartbeat, регулярный backup K3s/HA и полная voice
  latency instrumentation.
- Secure Boot PK `DO NOT TRUST - AMI Test PK` — не менять без плана.
- Rust Pulsar: активен; wake/TTS/HA/MA/reboot проверены. Осталось живое
  «Рикардо, стоп»/snooze, кнопки/mute/clock/volume и phone→speaker.
- Wake word: измерить recall/FP на реальном фоне.
- Factory/recovery image Midi отсутствует; raw dump — только с согласованием.
