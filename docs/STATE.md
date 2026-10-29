# STATE — текущее состояние

Обновлено: 2026-10-30. Публичные адреса — примеры; реальные — в
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
- Namespaces: `home-assistant`, `monitoring`, `tailscale`.
- Приложения: GigaAM `10305`, Whisper `10300`, Piper `10200`,
  Hermes gateway `127.0.0.1:8642` + voice proxy `8643`, health exporter `9116`.
- `8643` доступен только HAOS/K3s/loopback через nftables.
- MA/Matter Deployments, PV и data из K3s удалены; `runtime-ha` оставлен как
  архив старого HA Container.
- Observability: kube-prometheus-stack `92.14.0`, Loki `20.0.2`,
  Alloy `1.14.3`; четыре dashboard JSON через Helm ConfigMap.
- Grafana: LAN `http://192.0.2.27:30300/grafana/`; Tailscale
  `https://pl-home.<tailnet>.ts.net/grafana/`.

## Remote

- Tailscale Operator `1.104.2` в K3s; `home-access` ingress:
  `/` → HAOS, `/grafana/` → Grafana.
- `pl-home` указывает на operator proxy `tag:k8s`; host Serve не используется.
- Публичного HA нет; `8123/8095/8099/6053/6638` наружу не проброшены.
- K3s management/metrics `6443/10250/9100/9116/8472` закрыты от обычного LAN;
  SSH — LAN, K3s API — через туннель.
- Host Tailscale emergency access не настроен; Proxmox доступен по LAN/Wi-Fi.

## Станция

- Yandex Station Midi `cucumber`, IP `192.0.2.13`; `maind` остановлен.
- Pulsar Rust `v0.5.2-3-gc57d521-dirty` — установленная сборка (локальный форк,
  база upstream `v0.5.2` = `4cba44a`); собрана из рабочего дерева с повторным
  wake, заменяющим активный диалог (30.10). StationCancel без речи — изменение
  HA-конфига, не бинарника.
  `pulsar.prev` — предыдущая `v0.5.2-3-gc57d521`; `v0.5.2-2-g95ce568` и
  `v0.5.0-1-g908a027` — только private backup вне Git.
  Legacy C++ checkout,
  patch и binaries — private в `secrets/local/recovery/`.
  ESPHome `6053`, Sendspin `8928`, HTTP control `8099` (bearer), ADB `5555`, SSH `22`.
- ESPHome: 33 entities, Noise; HA satellite работает.
- SSH: dropbear `v2026.94`; ключ Mac `~/.ssh/ricardo_station_ed25519` →
  `/root/.ssh/authorized_keys`. Вход по паролю оставлен как fallback
  осознанно (решение); ADB/SSH доступны только из LAN.
- Активная модель wake word — v22 (`v22-scratch-puriso-selected-20261030`),
  обучена с нуля на исходной синтетике, Puriso2 и личных train-примерах.
  Установлена model-only: HA/MA подключены, бинарник Pulsar не менялся.
  Threshold 0.9, окно 10, pre-roll 0.5 с; SHA256 закреплён в personal-профиле.
  Нативный inference probe прошёл. Живая приёмка ожидается; offline-регрессия
  пройдена по всем манерам речи, но ошибки на сложных негативах остаются.
- Предыдущая v2, все обученные веса и кандидаты сохранены private.
  Device backup текущего переключения указан в
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
- Голосовые intents в `projects/home-assistant/ha-voice` вызывают canonical
  Connect API (`rest_command.scheduler_rpc`); endpoint —
  `input_text.scheduler_endpoint` (LAN NodePort base URL, durable via
  `!secret scheduler_endpoint`), токен — `secrets.yaml:scheduler_bearer`.
  Удалённый доступ — отдельный tailnet-only Ingress `pl-scheduler` (TLS
  оператора) на тот же Service; `pl-home` по-прежнему `/` → HAOS,
  `/grafana` → Grafana. Create/list/update/enable/disable/delete,
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
  LAN NodePort `30099`, tailnet `pl-control.<tailnet>`; pl-home/pl-scheduler
  не менялись. Доставка at-least-once, не exactly-once.
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
- Offsite encrypted backup, power-loss тест, host Tailscale emergency access.
- Внешний Alertmanager/heartbeat, регулярный backup K3s/HA и полная voice
  latency instrumentation.
- Secure Boot PK `DO NOT TRUST - AMI Test PK` — не менять без плана.
- Rust Pulsar: активен; wake/TTS/HA/MA/reboot проверены. Осталось живое
  «Рикардо, стоп»/snooze, кнопки/mute/clock/volume и phone→speaker.
- Wake word: измерить recall/FP на реальном фоне.
- Factory/recovery image Midi отсутствует; raw dump — только с согласованием.
