# STATE — текущее состояние

Обновлено: 2026-10-29. Публичные адреса — примеры; реальные — в
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
  - Matter Server `9.2.0`: `ws://127.0.0.1:5580/ws`; paired nodes пока 0.
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
- Pulsar Rust `v0.5.0-1-g908a027`; C++ `v0.4.1-ricardo6` — `.prev`/backup.
  ESPHome `6053`, Sendspin `8928`, HTTP control `8099` (bearer), ADB `5555`, SSH `22`.
- ESPHome: 33 entities, Noise; HA satellite работает.
- Custom wake word «Рикардо» v1: on-device microWakeWord, threshold 0.9,
  окно 10, pre-roll 0.5 с.
- HIL 2026-10-28: Rust active; live wake 0.904, HA pipeline/TTS, HAOS reboot,
  Sendspin/MA reconnect и station reboot прошли.
- BLE gateway: `Station-Gateway`, GATT provisioning, кнопки 1/3/4/5;
  4 нажатия могут временно уронить Wi-Fi — BLE остаётся аварийным каналом.
- QR/DPP provisioning удалён по решению пользователя: убраны оба автозапуска
  (onboot/rootkeeper), helper scripts, URI/key/force files и bootstrap state;
  dpp_config_processing=0. Сохранённые WPA сети не менялись, BLE работает.
  Прежние данные — только в private off-device recovery archive.
- OTA/ADB fortress активен: обновления no-op, ADB root только LAN.
- Egress guard: LAN/DHCP/NTP разрешены, публичный WAN и Яндекс — DROP.
- 2026-10-28: после перезагрузки станция перестала отвечать в LAN (ARP,
  SSH/ADB/ESPHome), хотя роутер показывал её online. BLE был доступен, но
  повторное подключение к сохранённой сети завершилось TIMEOUT и откатом.
  Снятие питания на 20–30 с восстановило IP, Rust autostart, HA и голосовой
  ответ (подтверждён пользователем); Sendspin затем переподключился сам.
  Первопричина сетевого зависания не установлена; прошивка не менялась.

## Голос / музыка / расписания

- Pipeline «Рикардо»: GigaAM v3 CTC int8 primary, Whisper `medium-int8`
  fallback; локальные intents → Hermes только для свободной речи.
- TTS: Piper `2.5.2`, `ru_puriso2-medium`; Irina сохранена для отката.
- Hermes `v0.22.5`, модель `gpt-5.6-luna` через ChatGPT/Codex OAuth;
  low reasoning + streaming; HA MCP 73 tools, MA MCP 86 tools.
- Voice proxy `v4`: ошибки upstream приведены к OpenAI error object. При
  раннем старте HA интеграция llama_cpp получает ConfigEntryNotReady и
  повторяет подключение, вместо постоянного setup_error. Агент
  `conversation.hermes_agent` восстановлен; живое обращение подтверждено
  пользователем. Проверены HA→Hermes model turn и startup fault/recovery
  на реальном коде интеграции HA. Предыдущая API-only приёмка этот сбой
  не обнаруживала; agent entity теперь обязателен в live HA tests.
- MA/Spotify: Soloist `1.3.10.13`, Spotify Connect device
  «Яндекс Станция Миди», music router `Music Assistant` / `Spotify Connect`.
- Будильники: `todo.alarms_once/daily/weekdays/weekends`; таймеры
  `timer.taimer_1…8`; напоминания `todo.reminders`; заметки `todo.notes`;
  dashboard `/assistant-schedules`. Legacy single-slot alarm отключён.
- Голосовые команды и точные entity/scripts — `projects/home-assistant/ha-voice/`.

## Планировщик, HA bridge, Android

- `solutions/scheduler`: durable на Go/SQLite (inbox/outbox, leases, ack),
  Connect/gRPC API на одном listener, legacy `AlarmService` facade. Очереди
  `home` (alarms/reminders/timers) и `tasks` (generic one-shot `EnqueueTask`);
  Kafka/внешний брокер не используются. Go- и backend-тесты проходят.
  Процесс не задеплоен на домашний сервер в рамках этой итерации.
- `solutions/ha-bridge`: executor для очереди `home`. Собирает события через
  Connect JSON, вызывает allowlisted HA scripts (`alarm_start`,
  `schedule_announce`, `alarm_stop`), подтверждает только после 2xx HA,
  at-least-once + локальный receipt-ledger. Dry-run — режим по умолчанию;
  `BRIDGE_EXECUTE=true` не включался на живой системе. Мост не слушает порт,
  не создаёт расписания и не вызывает `alarm_snooze`. Исходники и hermetic
  тесты реализованы; deploy/cutover отсутствуют.
- Действующий владелец будильников, таймеров и напоминаний — текущий HA
  (`todo.*`, `timer.*`, scripts/automations). Импорт расписаний и переключение
  исполнителя не выполнялись; живой HA остаётся источником звука.
- `projects/mobile/android`: Kotlin-клиент (Views, одна Activity, Metro DI,
  `applicationId dev.proflyder.home`, minSdk 26 / target 36, только
  `INTERNET`, debug-signed). Вкладки Alarms/Timers/Reminders/Connection;
  alarms — one-shot и recurring (weekdays/custom, IANA, snooze/stop),
  reminders — one-shot, timers — server-owned с pause/resume/cancel и
  server-clock remaining. Есть обязательный bearer token с Keystore
  AES-GCM, настраиваемые таймауты, GetStatus, SavedStateHandle и стабильные
  idempotency keys для неоднозначных retry; redirects выключены. Сборка
  `//projects/mobile/android:app` и общий gate (9 Android-таргетов, включая
  2 cold-launch smoke и 14 nav lifecycle, + 7 backend) проходят 16/16.
  Ограничения: телефонных уведомлений/FCM, offline cache, generic task/
  scenario UI и HA cutover нет; запуск на реальном устройстве/эмуляторе не
  выполнялся (только Robolectric), переключение вкладки сбрасывает её стек к
  корню. Телефон управляет серверным состоянием, расписания остаются
  server-authoritative.
- Будущая Hermes-интеграция — один общий MCP gateway с доменными группами
  инструментов; MCP runtime сегодня не разворачивается и Android-срезу не
  нужен.

## Приватное / backups

- `secrets/NETWORK.md` — реальные IP/порты; `secrets/CREDENTIALS.md` — доступы.
- HA backups до/после миграции, K3s recovery, OCI images и 11 PVC cold copy —
  на Mac, вне Git; SHA-256 проверены.
- `secrets/kubernetes-research/` — kubeconfig, private values, Grafana password,
  recovery.
- Полный disaster restore на запасном железе и offsite backup ещё не выполнены.

## Открыто

- Matter: добавить Yandex LED strip; проверить on/off, яркость, цвет.
- Xiaomi: проверить лампы on/off/brightness/color temp.
- Живая приёмка: микрофон/wake, будильник/стоп/snooze, Spotify transfer,
  phone→speaker.
- Offsite encrypted backup, power-loss тест, host Tailscale emergency access.
- Внешний Alertmanager/heartbeat и полная voice latency instrumentation.
- `quasar-server :31116` без auth — требует решения.
- Secure Boot PK `DO NOT TRUST - AMI Test PK` — не менять без плана.
- Rust Pulsar: активен; wake/TTS/HA/MA/reboot проверены. Осталось живое
  «Рикардо, стоп»/snooze, кнопки/mute/clock/volume, phone→speaker и полный откат.
- Wake word: измерить recall/FP на реальном фоне.
- Factory/recovery image Midi отсутствует; raw dump — только с согласованием.
