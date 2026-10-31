# STATE — текущее состояние

Публичные адреса — примеры; реальные — в `secrets/NETWORK.md`. Открытая
работа — `PENDING.md`; команды — `OPERATIONS.md`.

## Host / VM

- Proxmox VE `9.2.24`, kernel `7.0.14-26-pve`, host `pve`.
- Железо: DEXP Jupiter JU01CY, i7-12650H, 16 GiB RAM, NVMe 512 GB.
- Management `vmbr0`: `192.0.2.26/24`; uplink — Wi-Fi `RTL8821CE` в
  4-address/WDS к `vmbr0`, Ethernet — резерв по carrier. NAT/PCI passthrough
  не используются; VM сохраняют L2-адреса и IPv6 multicast.
- `ricardo-uplink` держит ровно один physical uplink; полные перезагрузки PVE
  без Ethernet проходят.
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
    Yandex LED strip `YNDX-00544` («Лента», `light.spalnia_lenta`), on/off,
    яркость и цвет проверены. Дублирующий набор сущностей от повторного
    интервью остался неактивным.
- K3s HAProxy и HA Container удалены; HAOS — единственный HA runtime.

## K3s

- K3s `v1.35.9+k3s1`, node `lab-host`; data-dir `/srv/k3s/runtime`; SQLite;
  local-path PV с `Retain`.
- Namespaces: `home-assistant`, `monitoring`, `traefik`, `cert-manager`,
  `openbao`, `external-secrets`, `currency-bot`.
- Приложения: GigaAM `10305`, Whisper `10300`, Piper `10200`,
  Hermes gateway `127.0.0.1:8642` + voice proxy `8643`, health exporter `9116`,
  currency-bot + cloudflared.
- `8643` доступен только HAOS/K3s/loopback через nftables.
- MA/Matter Deployments, PV и data из K3s удалены; `runtime-ha` оставлен как
  архив старого HA Container.
- Observability: kube-prometheus-stack `92.14.0`, Loki `20.0.2`,
  Alloy `1.14.3`; четыре dashboard JSON через Helm ConfigMap.
- Grafana: LAN `http://192.0.2.27:30300/` (корень), HTTPS
  `https://grafana.home.example.com/`.
- HTTPS ingress `*.home.example.com` (Traefik `42.2.0` + cert-manager
  `v1.21.3`, ACME DNS-01 Cloudflare, wildcard Let's Encrypt prod до
  2027-01-28): `ha`, `grafana`, `alarms`, `control`, `vault` → узел `.27`,
  Cloudflare DNS only. Traefik в `hostNetwork` (`80/443`), редирект на HTTPS,
  IP allow-list LAN + pod CIDR; HA-источник остаётся `.27`
  (`trusted_proxies /32`). WebSocket HA и Grafana проверены.
  Файлы — `projects/home/kubernetes/ingress/`; приватные имена/токен —
  `secrets/home/ingress.env` и `secrets/home/vpn/cloudflare.env`.

## Секреты: OpenBao и External Secrets

- OpenBao `2.7.2` (chart `0.30.6`) в namespace `openbao`,
  `https://vault.home.example.com/` через тот же Traefik/allow-list и
  wildcard-сертификат. Один узел: `raft` + локальные PV
  (`/srv/k3s/volumes/openbao*`, `ricardo-existing`, `Retain`), декларативный
  файловый audit на PV, UI включён.
- Auto-unseal — встроенный `static` seal (ключ в Secret `openbao-seal`), после
  `kubectl delete pod` сервер распечатывается сам.
- Auth: `userpass` (человек, политика `admin`; пароль задаёт человек; обязательный
  TOTP `human-totp`, устройство зарегистрировано, enforcement включён), `jwt` `github` (GitHub OIDC,
  роль `deploy-home`, environment `Home`, ref `develop`, TTL ≤15 мин),
  `kubernetes` (TokenReview, роли `external-secrets`/`k8s-smoke`/`backup`).
- KV v2 `secret/`: `secret/ci/test`, `secret/ci/deploy-home` и
  `secret/k8s/*`. `secret/ci/deploy-home` (`kubeconfig`, `scheduler_env`,
  `runtime_values`) — источник для CI; в GitHub этих значений больше нет.
  `deploy-home` входит по OIDC после VPN, читает запись, материализует файлы
  600, маскирует значения и отзывает токен; политика `ci-deploy` — read
  только `secret/ci/test` и `secret/ci/deploy-home`.
- Ночной CronJob raft snapshot в `/srv/k3s/volumes/openbao-backups`
  (7 файлов). Root-токен отозван, recovery keys — только `secrets/openbao/`.
- External Secrets Operator `2.14.0` (chart `2.14.0`, образ по digest) в
  namespace `external-secrets`: `ClusterSecretStore/openbao` через in-cluster
  Service (Kubernetes auth, роль `external-secrets`, KV v2). Источник правды
  для `secret/k8s/*`: `scheduler-auth`, `ha-bridge-ha`, `hermes-environment`,
  `grafana-admin`, `cloudflare-api-token`. Kubernetes Secrets с теми же
  именами/ключами создаёт ESO (`creationPolicy: Owner`,
  `deletionPolicy: Retain`, `refreshInterval: 1h`); deploy-скрипты и CI их не
  перезаписывают. Не переносятся: `openbao-seal`, helm release/SA token
  Secrets, TLS и служебные Secrets контроллеров. Файлы —
  `projects/home/kubernetes/openbao/` и
  `projects/home/kubernetes/external-secrets/`.

## CI/CD и образы

- GitHub-hosted runners. Карта юнитов — `platform/ci/units.yml`, запуск —
  платформенные reusable workflows (`.github/workflows/_*.yml`),
  обязательный итог — `ci-ok`. Прямой push в `develop` запрещён; `develop`
  обновляется только squash-merge PR.
- `deploy-home` (push в `develop` = merge PR) деплоит только затронутые
  юниты параллельно: входит в VPN группой `ci`, затем OIDC в OpenBao,
  kubectl → K3s API `6443`, HTTPS → Traefik/OpenBao `443`. Режимы
  (`preserve`/`execute`/`shadow`/`control-*`) — только через
  `workflow_dispatch`.
- scheduler, ha-bridge, ha-control — собственные distroless-образы,
  собираемые Bazel `rules_oci` и адресуемые по digest в
  `ghcr.io/proflyder/<name>@sha256:…`; CI пушит их job-scoped
  `GITHUB_TOKEN`, живой узел импортирует те же digest'ы в containerd.
- Hermes и voice-proxy — публичные digest-pinned образы GHCR
  (`ghcr.io/proflyder/{hermes,voice-proxy}@sha256:…`), без pull-secret.
  `deploy-home` собирает и пушит их, `hermes_rollout.py` подставляет digest'ы,
  проверяет health/Assist/логи и делает `helm rollback` при провале.
- Деплой — `converge.py`: полная замена Deployment, сохранение env/режима и
  replicas; откат восстанавливает только применённый объект целиком.
- PV/PVC применяет `apply-runtime.sh` через kubectl; Helm управляет только
  namespace-scoped workloads, поэтому `ci-deploy` не имеет cluster-scoped
  прав.
- Pulsar не собирается монорепо: релиз выходит в форке. Push тега `v0.5.100N`
  → `build.yml` публикует GitHub release → `deploy-station.yml` после approve
  в Environment `Station` входит в VPN группой `ci` (отдельный сертификат
  `ci-pulsar`) и по OIDC в OpenBao роль `deploy-station` (политика `ci-station`,
  `secret/ci/deploy-station`: `ha_url`, `ha_token`, `entity_id`), затем
  вызывает HA `update_entity` + `update.install`. Job ждёт
  `installed_version == tag` и 90 с стабильности; откат `supervise.sh` на
  `.prev` валит job. Ручной повтор — `gh workflow run deploy-station.yml
  --ref <tag> -f tag=<tag>` в форке. Откат — не через CI, а на станции
  (`.prev` + `ctl.sh reload`).

## Remote / VPN

- Удалённый доступ — собственный VPN (ocserv, CT `102`): `vpn.example.com`,
  `443/tcp` + `443/udp` (DTLS), наружу проброшен только этот порт.
- Вход — клиентский сертификат частного CA либо пароль + TOTP; группы
  `lan` (split tunnel до домашней LAN, DNS `192.0.2.1`), `full` (весь трафик)
  и `ci`. Пул `10.77.0.0/24`, у группы `ci` — `10.77.1.0/24`.
- Приватный ключ CA (RSA 4096, 10 лет) — только на рабочей машине в
  `secrets/home/vpn/client-ca/`; в CT — `client-ca.pem` и `client-ca.crl`.
  Клиентские сертификаты — RSA 2048, 2 года, CN = пользователь, OU = группа.
  Управление — `vpncerts.py` (`init-ca`/`issue`/`revoke`/`list`).
- Группа `ci` маршрутизируется только до K3s; nftables `forward` разрешает
  `tcp/{443,6443}` и `drop` остальное, SSH из группы закрыт.
- Серверный сертификат Let's Encrypt продлевается автоматически: certbot +
  `dns-cloudflare` (DNS-01), `certbot.timer`, deploy hook копирует
  fullchain/privkey в ocserv и перезапускает его. MSS clamp в forward chain
  стоит до accept. Ручное продление и TXT-записи не нужны.
- Лимит `max-same-clients = 10`; встроенный бан неудачных попыток.
- HTTPS-имена LAN/VPN: `https://{ha,grafana,alarms,control,vault}.home.example.com`
  через Traefik + cert-manager. Публичного HA нет;
  `8123/8095/8099/6053/6638` наружу не проброшены.
- K3s management/metrics `6443/10250/9100/9116/8472` закрыты от обычного LAN;
  SSH — LAN, K3s API — через туннель. Аварийный доступ к Proxmox — LAN/Wi-Fi.
- Исходники: `projects/home/proxmox/vpn/`; приватные учётные данные —
  `secrets/home/vpn/`.

## Станция

- Yandex Station Midi `cucumber`, IP `192.0.2.13`; `maind` остановлен.
- Pulsar Rust `v0.5.1002` — установленная armhf-сборка
  (`/lib/ld-linux-armhf.so.3`; релиз форка Pulsar, origin вложенного checkout
  `projects/station/upstream/pulsar-rust`, линия `main` = Ricardo; база
  upstream `v0.5.3` = `504830d`). Включает повторный wake, заменяющий
  активный диалог, экспериментальный локальный endpointing и Ricardo
  pre-roll 256 ms; из upstream `v0.5.3` — switch часов «Clock auto
  brightness», Mini gen1 и Mini 3 (experimental), S32 capture,
  переработанные volume/mixer/player/pcm_out/sendspin и `deploy/asound.conf`.
  Процесс запущен с `PULSAR_LOCAL_ENDPOINTING=1`,
  унаследованным supervisor'ом от `ctl.sh reload`; это не boot-persistent:
  после reboot или нового supervisor без env feature выключен.
  `--version` теперь различает сборки по тегу.
- Теги форка — `v<upstream major.minor>.<1000+N>` (`v0.5.1002`, следующий
  `v0.5.1003`):
  updater демона ставит только строго большую `MAJOR.MINOR.PATCH`, а upstream
  уже имеет `v0.5.3`. Релиз — push тега → `build.yml` собирает и публикует
  GitHub release (latest) → `deploy-station.yml` после approve в Environment
  `Station` (только теги `v*`) катит на станцию через HA updater. Процедура —
  RICARDO.md «Релизы и деплой форка» в форке. Репозиторий обновлений —
  константа `pulsar_update::UPDATE_REPO` в форке (перенесена upstream),
  указывает на форк.
- Установленный бинарник — SHA256 `3a544979…` (поставлен этим flow
  2026-10-31); `pulsar.prev` — предыдущая сборка `80aea2ec…`
  (`v0.5.1001`, база upstream `v0.5.2`, тот же Ricardo-код: pre-roll 256 ms +
  endpointing). Historical старше: `7081ef8e…`
  (`v0.5.2-4-g9b5e86a-dirty`), `0b0c48e4…` (endpointing 512 ms), `d2197f6a…`
  (`v0.5.2-3-gc57d521-dirty`, `pulsar.pre-endpoint-20261030`),
  `pulsar.prev.pre-endpoint-20261030` — `69310a37…`; `v0.5.2-2-g95ce568` и
  `v0.5.0-1-g908a027` — только private backup вне Git. Legacy C++ checkout,
  patch и binaries — private в `secrets/local/recovery/`.
- ESPHome `6053`, Sendspin `8928`, HTTP control `8099` (bearer), ADB `5555`,
  SSH `22`; 33 ESPHome entities на `v0.5.1001`, Noise; HA satellite работает.
  `v0.5.1002` добавляет switch «Clock auto brightness» (включён): автояркость
  часов по датчику освещённости, при выключении — уровень «Clock brightness»;
  число сущностей после обновления не пересчитывалось.
- После `v0.5.1002` живая проверка: модель Ricardo загружена, Sendspin
  подключён к MA, HA satellite в `idle`.
- SSH: dropbear `v2026.94`; ключ Mac `~/.ssh/ricardo_station_ed25519` →
  `/root/.ssh/authorized_keys`. Вход по паролю оставлен как fallback
  осознанно (решение); ADB/SSH доступны только из LAN.
- Установленная модель wake word — кандидат v27
  (`v27-ambient-capacity-sel22-20261030`, SHA256 `4f645834…b0e12444`,
  207 720 Б); threshold 0.9, окно 10. Установка model-only, бинарник Pulsar
  не менялся; живая голосовая приёмка не выполнялась — private `active.json`
  фиксирует `live_voice_acceptance=pending`.
- Best-known/reference по решению пользователя остаётся v24
  (`v24-ambient-scratch-20261030`, SHA256 `83b92351…314787a`) и не заменяется
  установленным кандидатом до его живой приёмки. Откат на v24 — профиль
  `projects/station/configs/wakeword-v24.json` через
  `projects/station/tools/wakeword-personalize/deploy.py`; откат на v22 —
  `wakeword-v22.json`.
- Кандидат v27 — более ёмкий MixedNet (pointwise 128×4, первая свёртка 64;
  те же MixConv-ядра, stride 3, residual, контекст 204×40), 138 113
  параметров против 40 385 у v24, вход `int8[1,3,40]`. C++/firmware-проверка
  замороженного файла e22 на станции: 162/164 на регрессионном наборе против
  159/164 у v24 и 164/164 у v22; Python-стенд дал 161/164 одинаково с v24.
  Это небольшое улучшение на одном C++-стенде, а не общее превосходство:
  независимый ambient и живая приёмка ещё нужны.
- Device backup пары v24 до установки v27:
  `/data/pulsar/backups/model-20261030T160858Z-f28772d8`; независимая private
  копия — `secrets/local/backups/wakeword-v24-pre-v27-20261030T160839Z/`.
  Все веса сохранены private на Mac и WSL; installed-запись и hashes —
  `secrets/model-registry/ricardo/active.json`.
- Актуальная уточнённая разметка и PCM: `secrets/datasets/ricardo-fixed-20261030/`;
  исходный приватный датасет — `secrets/datasets/ricardo-20261029/`
  (WAV, PCM-контексты, разметка по версиям, SHA256).
- Проверено: live wake 0.904, HA pipeline/TTS, HAOS reboot, Sendspin/MA
  reconnect и station reboot. Supervised `reload` завершает Sendspin-сессию
  goodbye-причиной `restart`, и MA переподключается сам (~3 с) без рестарта
  add-on MA и без потери регистрации player. Рестарт MA нужен только как
  необязательное восстановление при реальном сбое.
- BLE gateway: `Station-Gateway`, GATT provisioning, кнопки 1/3/4/5;
  4 нажатия могут временно уронить Wi-Fi — BLE остаётся аварийным каналом.
- QR/DPP provisioning удалён по решению пользователя: убраны оба автозапуска
  (onboot/rootkeeper), helper scripts, URI/key/force files и bootstrap state;
  `dpp_config_processing=0`. Сохранённые WPA сети не менялись, BLE работает.
  Прежние данные — только в private off-device recovery archive.
- OTA/ADB fortress активен: обновления no-op, ADB root только LAN.
- Egress guard: LAN/DHCP/NTP разрешены, публичный WAN и Яндекс — DROP.
  `ALLOW_GITHUB=1` (по умолчанию) дополнительно разрешает `tcp/443` только к
  публичным IPv4-диапазонам GitHub (`140.82.112.0/20`, `143.55.64.0/20`,
  `185.199.108.0/22`, `192.30.252.0/22`, по `api.github.com/meta`) — для
  updater'а; при смене диапазонов обновить `GITHUB_NETS`. Прежний скрипт —
  `egress-guard.sh.bak-20261031-github`.
- Известная проблема: после reboot станция иногда не отвечает в LAN при
  online-статусе роутера; BLE остаётся доступен, снятие питания на 20–30 с
  восстанавливает IP/HA/голос. Первопричина не установлена (`PENDING.md`).

## Голос / музыка / расписания

- Pipeline «Рикардо»: GigaAM v3 CTC int8 primary, Whisper `medium-int8`
  fallback; локальные intents → Hermes только для свободной речи.
- Локальный endpointing в Rust Pulsar: после VQE детектор конца фразы по
  энергии чанков (порог относительно уровня wake-слова и тихой части его
  окна, без абсолютных dBFS) шлёт ровно один graceful
  `VoiceAssistantAudio.end=true` на активный LISTENING-диалог после ~1 с
  тишины. Abort/отмена/повторный wake по-прежнему идут через
  `VoiceAssistantRequest.start=false`; HA VAD остаётся главным и может
  завершить раньше. Это экспериментальная эвристика: включается только
  `PULSAR_LOCAL_ENDPOINTING=1`, по умолчанию выключена; живых измерений
  порогов нет, коэффициенты не откалиброваны. При нехватке окна/SNR wake
  или для turn без wake (кнопка, continued conversation, start_conversation)
  детектор не запускается. Pre-roll считается только pre-wake evidence:
  одного pre-roll недостаточно для завершения фразы, а команда, найденная в
  arbitration-буфере, помечается как live. Известные ограничения: команда
  целиком в pre-roll не завершается локально; хвост wake word в live и
  громкий earcon по энергии не гарантированно отличимы от речи.
- TTS: Piper `2.5.2`, `ru_puriso2-medium`.
- Hermes `v0.22.5`, модель `gpt-5.6-luna` через ChatGPT/Codex OAuth; low
  reasoning + streaming; HA MCP 73 tools, MA MCP 86 tools.
- Voice proxy `v6`: к v4-санитайзеру добавлен exact cancel guard. Если
  предыдущий ответ Hermes закончился вопросом, HA включает
  `continue_conversation` и обходит `prefer_local_intents`; распознанная
  «отмена» уходила в Hermes. Proxy перехватывает только точную
  нормализованную фразу последнего user-сообщения, отвечает пустым
  completion без upstream Hermes и без TTS (`action_done`,
  `processed_locally=false`, `continue_conversation=false`). Свободный
  follow-up после отмены по-прежнему отвечает Hermes. Ошибки upstream
  нормализуются в OpenAI error object. При раннем старте HA интеграция
  llama_cpp получает ConfigEntryNotReady и повторяет подключение вместо
  постоянного setup_error. Агент `conversation.hermes_agent` обязателен в
  live HA tests.
- MA/Spotify: Soloist `1.3.10.13`, Spotify Connect device
  «Яндекс Станция Миди», music router `Music Assistant` / `Spotify Connect`.
- Будильники, таймеры и напоминания хранит scheduler; голосовые intents и
  Android работают через Connect API. Legacy helper-ы `todo.alarms_*` и
  `todo.reminders` удалены, `timer.taimer_1…8` отключены в registry
  (YAML-определения остаются в живом конфиге HA). Заметки — `todo.notes`;
  dashboard `/assistant-schedules` только для чтения. Xiaomi-лампы
  `xiaomi_home` (`Lamp` bsLamp2, `Lighter` lamp22): on/off, яркость и
  цвет/CT проверены.
- Голосовые команды и точные entity/scripts — `projects/home-assistant/ha-voice/`.

## Планировщик, HA bridge, Android

- `solutions/scheduler` задеплоен в K3s `home-assistant`: один под, SQLite на
  PVC `scheduler-data` (`local-retain`), NodePort `30098`, bearer обязателен,
  readiness/liveness — аутентифицированный `-healthcheck`. Состояние
  переживает рестарт пода.
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
  Правка правила во время звонка (`UpdateSchedule`/enable) пока не эмитит
  STOP — отдельная задача в `PENDING.md`.
- Голосовые intents в `projects/home-assistant/ha-voice` вызывают canonical
  Connect API (`rest_command.scheduler_rpc`); endpoint —
  `input_text.scheduler_endpoint` (LAN NodePort base URL, durable via
  `!secret scheduler_endpoint`), токен — `secrets.yaml:scheduler_bearer`.
  Удалённый доступ — `https://alarms.home.example.com` (LAN/VPN) на тот же
  Service. Create/list/update/enable/disable/delete, timer
  start/pause/resume/cancel и reminder create проверены тихими
  `conversation.process` через builtin-агент.
- Deploy/rollback: `projects/home/kubernetes/scheduler/` (manifest templates,
  render, preflight, backup). Приватные значения — `secrets/home/scheduler.env`;
  rendered manifests — `secrets/local/rendered/`.
- `ha-control` (тот же solution) развёрнут отдельным Deployment: каталог
  `GET /v1/catalog` из live HA, очередь `control`; `CONTROL_EXECUTE=true`,
  consumer `ha-control-home` в allowlist планировщика рядом с
  `ha-bridge-home`. Скрипты попадают в каталог только с нативной меткой HA
  `scenario` (`CONTROL_SCENARIO_LABEL`); иконки — только отображение. Метка
  назначена `script.work_mode` и `script.blackout`. Живой каталог: 2 сценария
  + 15 light/switch, служебные скрипты отсутствуют. LAN NodePort `30099`,
  HTTPS `https://control.home.example.com` (LAN/VPN). Доставка
  at-least-once, не exactly-once.
- `projects/mobile/android`: Kotlin-клиент (Views, одна Activity, Metro DI,
  `applicationId dev.proflyder.home`, minSdk 26 / target 36, только
  `INTERNET`, debug-signed). Вкладки **Home**, Alarms, Timers, Reminders,
  Connection; Home читает `GET /v1/catalog`, команды идут через
  `home.control`/очередь `control`. Bearer token в Keystore AES-GCM,
  настройки и фильтр Home — Jetpack DataStore с миграцией из
  SharedPreferences; idempotency keys, redirects выключены. Debug APK
  установлен на телефон; холодный запуск и каталог проверены, интерактивные
  фильтры/сценарии/toggle ещё не приняты.

## Currency bot

- `solutions/currency-bot` (Go) заменил Kotlin `tg-bot-currency` с тем же
  поведением: парсер kurs.kz, часовой job в :00 UTC, алерты только при
  порогах HOUR/DAY/WEEK/MONTH с dedup в `sent_alerts`, retention 35 дней,
  команды `/start` `/help` `/trigger`.
- K3s namespace `currency-bot`: Deployment `currency-bot` (SQLite на PVC
  `currency-bot-data` 1 GiB `local-retain`, образ
  `ghcr.io/<owner>/currency-bot@sha256:…`) и Deployment `cloudflared`
  (remotely-managed Cloudflare Tunnel `home-k3s`). Публичный хост
  `tg.example.com` принимает только `^/telegram/webhook$` → Service
  `currency-bot-webhook:8081`; остальные пути — 404. Webhook проверяет
  Telegram `secret_token` и allowlist `CHAT_ID`: без секрета — 401, с
  секретом — 200. Telegram `getWebhookInfo` указывает на туннель,
  `allowed_updates=["message"]`.
- API и `/metrics` — ClusterIP :8080; снаружи только LAN/VPN через
  IngressRoute `currency.home.<domain>` (`home-allowlist`) с Bearer-токеном.
  `ServiceMonitor` скрейпит `/metrics` (up=1).
- Secrets — ESO из OpenBao `k8s/currency-bot` (`BOT_TOKEN`, `CHAT_ID`,
  `WEBHOOK_SECRET`, `API_TOKEN`) и `k8s/cloudflared` (`TUNNEL_TOKEN`).
  `CHAT_ID` — два чата: первый (группа/канал) получает алерты, второй — личный
  чат владельца для команд; в отличие от Kotlin-бота команды из других чатов
  игнорируются.
  NetworkPolicies: 8081 только от cloudflared; egress cloudflared к Cloudflare
  `7844/udp` + `443/tcp` и DNS. Пути OpenBao пока не по конвенции
  `k8s/<namespace>/<name>` (`PENDING.md`).
- История перенесена из H2 2026-10-31: 990 записей (2026-09-20 … 2026-10-31)
  и 2 `sent_alerts`; PVC заполнен до первого старта. `/trigger` и
  `POST /api/trigger` по-прежнему всегда отправляют.
- CI: юниты `currency-bot` (go + образ + deploy) и `currency-bot-deploy`
  (contract-тест `deploy.sh`); deploy-env — `secret/ci/deploy-home`,
  ключ `currency_bot_env` (`WEBHOOK_URL`, `LAN_CIDR`, `NODE_LAN_IP`).
  Деплой `currency-bot` зелёный.
- Наблюдаемость: Alloy шипит логи namespace `currency-bot` (редакция
  Telegram bot tokens, Bearer и key=value/JSON секретов); дашборд Grafana
  `currency-bot` (uid `currency-bot`). VPS-дашборд node не переносился.
- Старые `tg-bot-currency`, `infra-monitoring`, `infra-ci` заменены монорепо,
  их GitHub secrets удалены, репозитории в archive (2026-10-31). На VPS
  снесены бот, Prometheus/Grafana/Loki/node-exporter/promtail (контейнеры,
  volumes, образы, `/app/currency-bot`, `/opt/monitoring`); nginx на домене
  отдаёт только 404 (certbot продлевает сертификат), наружу открыты лишь
  22/80/443. Backup конфигов и данных — `secrets/currency-bot/vps-backup/`.
  Приватные материалы — `secrets/currency-bot/` (VPS ssh
  key/config, CSV/DB, tunnel token, Cloudflare tunnel API token, `k8s.env`) и
  `secrets/home/currency-bot.env`.

## Приватное / backups

- `secrets/NETWORK.md` — реальные IP/порты; `secrets/CREDENTIALS.md` — доступы.
- HA backups, K3s recovery, OCI images и PVC cold copy — на Mac, вне Git;
  SHA-256 проверены.
- `secrets/kubernetes-research/` — kubeconfig, private values, Grafana
  password, recovery.
- HA automatic backup не настроен: `sensor.backup_next_scheduled_automatic_backup`
  и `last_successful` = `unknown`; CronJob/Velero для K3s нет.
- Offsite encrypted backup, восстановление на запасном железе и power-loss
  тест не выполнены (`PENDING.md`).

## Ограничения

- Один K3s node, без HA control plane и распределённого storage.
- Публичного HA/Grafana нет; доступ только LAN/VPN. Единственный публичный
  путь — Telegram webhook `tg.example.com` через Cloudflare Tunnel.
- Нет offsite backup, внешнего alerting и проверенного disaster restore.
- Станция — единственный экземпляр; factory/recovery image отсутствует.
