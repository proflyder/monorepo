# STATE — текущее состояние

Только текущие факты. Адреса — примеры (`192.0.2.x`), реальные — в
`secrets/NETWORK.md`; порты и классы доступа — `NETWORK.md`; секреты —
`SECRETS.md`; открытая работа — `PENDING.md`; команды — `OPERATIONS.md`.
Устройство сервисов — README модулей.

## Host / VM

- Proxmox VE `9.2.24`, host `pve`, `192.0.2.26`. Железо: i7-12650H,
  32 GiB RAM, NVMe 512 GB (система, `local-lvm`), SATA SSD 512 GB — только
  бэкапы (storage `backup`, `/mnt/pve/backup`, `nofail`).
- Uplink: Wi-Fi 4-address/WDS к `vmbr0`, Ethernet — резерв по carrier;
  сервис `home-uplink` держит ровно один physical uplink
  (`projects/home/proxmox/wifi-uplink/`). NAT и PCI passthrough нет.
- Ночной vzdump `nightly` 04:00: VM 100/101 и CT 102/103, zstd,
  `keep-daily=1,keep-monthly=2`, на том же узле. vzdump VM 101 содержит raft
  OpenBao и Secret `openbao-seal` — не вывозить без шифрования.
- После полного обесточивания всё поднимается само (~4 мин).

| Гость | Ресурсы | ОС | Адрес | Роль |
|---|---|---|---|---|
| VM `100 haos` | 2 vCPU / 4 GiB / 32 GiB | HAOS `18.4`, HA Core `2026.11.0` | `.28` | единственный HA runtime |
| VM `101 lab-k3s` | 8 vCPU / 12 GiB / 200 GiB | Ubuntu 24.04, K3s | `.27` | приложения, startup order 1 |
| CT `102 vpn` | 1 vCPU / 512 MiB / 4 GiB | Debian 13, unprivileged | `.29` | ocserv + WireGuard |
| CT `103 bb` | 2 vCPU / 2 GiB / 16 GiB | Debian 13, unprivileged | `.30` | сервер bb, onboot `order=3` |

- Terraform: provider `bpg/proxmox`, state root-only на PVE
  (`projects/home/proxmox/`).
- Сервер bb работает на CT 103 под пользователем `bb` (systemd user-сервис,
  данные `/home/bb/.bb-machines/<server-host>`), клиенты подключаются через
  bb Connect. Mac — машина bb: треды проектов выполняются там. Плагины
  `profiles` (`solutions/bb-profiles`) и `home-dispatcher`
  (`solutions/bb-home-dispatcher`) — path-источники, скопированные на CT
  вручную. Маршрут `home-dispatcher` через bb Connect закрыт (401); путь из
  дома — только SSH-мост `bb-dispatch`. На CT не залогинены `gh` и Codex.
  Бэкап и restore — `projects/home/proxmox/bb/README.md`.

## HAOS

- Add-ons: Music Assistant `2.10.9` (UI/API `8095`, stream `8097`, Sendspin
  `8927`), Matter Server `9.3.0` (1 узел — LED-лента `light.spalnia_lenta`).
- HA automatic encrypted backup: ежедневно 03:30, 3 копии.
- YAML голосового контура (`projects/home-assistant/ha-voice`) доставляет
  `ha-config-sync`.

## K3s

- K3s `v1.35.9+k3s1`, один узел, SQLite, data-dir `/srv/k3s/runtime`,
  local-path PV с `Retain` (`/srv/k3s/volumes/*`).
- Kubelet: логи 10Mi×3, GC неиспользуемых образов через 48 ч, параллельный
  pull до 4 образов (drop-in'ы в `agent/etc/kubelet.conf.d/`).
- Namespaces: `home-assistant`, `monitoring`, `traefik`, `cert-manager`,
  `openbao`, `external-secrets`, `currency-bot`, `vaultwarden`, `nexus`,
  `flux-system`, `hermes-work`.
- Под Flux: платформа (ESO, cert-manager, Traefik, маршруты), observability,
  Vaultwarden, Nexus и все деплой-юниты приложений. Вручную: OpenBao,
  host-firewall (nftables) и Cloudflare DNS.
- Приложения в `home-assistant`: GigaAM `10305`, Whisper `10300`, Piper
  `10200`, Hermes (`8642` loopback) + voice-proxy (`8643`, chat relay `8644`),
  health exporter `9116`, config-drift sidecar Hermes `9118`, scheduler
  (NodePort `30098`), ha-bridge, ha-control (NodePort `30099`), ha-scenario,
  intercom (NodePort `30097`), wakegrant (NodePort `30130`), mcp-gateway
  (ClusterIP `:8080`), phone-push (ClusterIP `:8095`), secrets-gateway
  (ClusterIP `:8096`), ha-config-sync
  (ClusterIP `:9119`). Отдельно: currency-bot + cloudflared, Nexus `:8081`,
  pve-exporter (`monitoring`).
- Версии платформы — `projects/home/kubernetes/versions.sh` и HelmRelease'ы:
  kube-prometheus-stack `92.14.0`, Loki `20.0.2`, Alloy `1.14.3`, Traefik
  `42.2.0`, cert-manager `v1.21.3`, OpenBao `2.7.2` (chart `0.30.6`), ESO
  `2.14.0`, Vaultwarden `1.37.5`.
- HTTPS `https://<name>.home.example.com` (Traefik + wildcard Let's Encrypt,
  DNS-01 Cloudflare, только LAN/VPN): `ha`, `grafana`, `alarms`, `control`,
  `currency`, `intercom`, `secrets`, `vault`, `passwords`, `nexus`. Grafana
  также LAN
  NodePort `30300`.
- Алерты: Alertmanager → личный чат владельца через инфра-бот Hermes
  (Secret `alertmanager-telegram`, шаблон
  `projects/home/kubernetes/observability/alertmanager/telegram.tmpl`) и
  вебхук в чат приложения. Dead-man switch: `Watchdog` → Cloudflare Worker
  `home-heartbeat` (`solutions/heartbeat`), тишина > 15 мин → «дом молчит».
  Правила: `home-backup-alerts`, `home-pod-alerts`.

## Секреты

- OpenBao — единственный источник секретов, один узел, raft на локальном PV,
  auto-unseal встроенным `static` seal (Secret `openbao-seal`), audit в stdout
  → Loki. Root-токен отозван; recovery keys и seal key — у владельца в
  Bitwarden и Vaultwarden, на диске их нет. Ночной raft snapshot в
  `/srv/k3s/volumes/openbao-backups`.
- Auth: `userpass` + обязательный TOTP (человек), `jwt` `github` (роли
  `deploy-station`, `android-distribute`, `android-release`,
  `android-cleanup`), `kubernetes` (ESO и CronJob'ы). Рабочие места — свой
  periodic token `operator-kv` на машину.
- ESO (`ClusterSecretStore/openbao`) создаёт Secrets приложений из
  `secret/k8s/*`. Пути и ключи — `SECRETS.md`; модель угроз — `SECURITY.md`.
- Vaultwarden — менеджер паролей человека (`passwords`), регистрация закрыта.

## CI/CD

- GitHub-hosted runners; карта юнитов — `platform/ci/units.mjs`; итоговый
  required check — `ci-ok`; workflow-паттерн — `TESTING.md` (CI), поток
  веток — `GIT.md`.
- Merge в `develop` → `deploy-home`: job `pins` коммитит проверенные digest'ы
  в `deploy/home-apps`, Flux применяет workload'ы из `develop`. Входящего
  доступа CI в K3s нет; откат — revert pins-коммита или `flux suspend`.
- Образы — GHCR по digest (`ghcr.io/proflyder/<name>@sha256:…`), собираются
  Bazel `rules_oci`/`rules_rust` (static musl, distroless); `hermes` —
  воспроизводимая Docker-сборка из upstream-ревизии
  (`projects/home-assistant/deploy/images/hermes/`).
- Версии native-юнитов (Pulsar, logon provider) вычисляет CI
  (`platform/ci/version.mjs`, решение 60). Инструменты — `mise.toml`.

## Remote / VPN

- CT 102: ocserv `443/tcp+udp` (`vpn.example.com`), группы `lan`/`full`/`ci`,
  пулы `10.77.0.0/24` и `10.77.1.0/24` (`ci`); WireGuard `wg0` `51820/udp`,
  пул `10.77.2.0/24`, peer'ы синхронизирует `wg-peer-sync` из scheduler.
  Проброс `51820/udp` на роутере ещё не сделан (`PENDING.md`). Устройство и
  команды — `projects/home/proxmox/vpn/README.md`, сеть — `NETWORK.md`.
- Публично наружу: VPN и Cloudflare Tunnel `home-k3s`
  (`tg.example.com/telegram/webhook`, read-only Nexus `/repository/*`).

## Станция

- Yandex Station Midi `cucumber`, `192.0.2.13`; `maind` остановлен, OTA
  заблокирован, egress guard (`projects/station/scripts/station-egress-guard.sh`).
- Состав ПО — `projects/station/manifest.yaml` и `STATION-SOFTWARE.md`,
  сверка — `mise run station:audit`.
- Pulsar `v0.5.1020` (база upstream `v0.6.2`), выкачен 2026-11-18. Первый
  релиз с вычисляемой версией (`v0.6.51`) опубликован в GHCR, на колонку не
  выкачен (`PENDING.md`). Откат — `pulsar.prev` + `ctl.sh reload`. Правила
  форка — `projects/station/pulsar/RICARDO.md`.
- Local endpointing включается только вручную
  (`PULSAR_LOCAL_ENDPOINTING=1 /data/pulsar/ctl.sh reload`), не переживает
  reboot и model-only установку через `deploy.py`.
- Wake word «Рикардо»: модель v28, cutoff 0.90 / window 10, запись
  wake-событий включена; живая приёмка v28 не пройдена. Откат —
  `projects/station/configs/wakeword-v27.json`. Подробности — `WAKE-WORD.md`.
- BLE gateway `Station-Gateway` (`/data/quasar/ble-gateway`): сборка на
  устройстве старше исправления версии и отвечает `ble-gateway 0.0.0`.
  Обновление — `STATION-SOFTWARE.md`.
- SSH — dropbear из rootfs под `/etc/init.d/S99dropbear`, ключ
  `~/.ssh/home-lab/station_ed25519`; вход по паролю оставлен как fallback.
- Порты — `NETWORK.md`. Сущности HA колонки — `projects/home-assistant/ha-voice/README.md`.
- Известная проблема: после reboot станция иногда пропадает из LAN; снятие
  питания на 20–30 с помогает (`PENDING.md`).

## Голос и музыка

- Pipeline: GigaAM v3 primary, Whisper `medium-int8` fallback, Piper
  `ru_puriso2-medium`; локальные intents в HA, свободная речь — voice-proxy →
  mcp-gateway `/v1/voice/utterance` → Hermes или bb (по профилю
  `input_select.home_voice_profile`). Маршруты — `ARCHITECTURE.md`, детали —
  `solutions/voice-proxy/README.md`, `solutions/mcp-gateway/README.md`.
- Будильники, таймеры и напоминания — только scheduler; голос ставит их через
  Hermes → mcp-gateway группа `time`. Локальные — только стоп и snooze.
- Hermes `v0.22.5`, модель через ChatGPT/Codex OAuth; MCP-серверы HA, MA и
  `infra` (mcp-gateway). Конфиг и деплой — `projects/home-assistant/hermes/personal/README.md`.
- Рабочий Hermes: namespace `hermes-work`, `HelmRelease/hermes-work`, модель
  Claude (`CLAUDE_CODE_OAUTH_TOKEN`), свой Telegram-бот, без HA. Экземпляры —
  `projects/home-assistant/hermes/README.md`.
- MA + Spotify Connect device «Яндекс Станция Миди».
- Комп (Windows-ПК): WoL, выключение/блокировка по SSH forced command,
  вход по пропуску `wakegrant` через `solutions/windows-logon-provider`. На ПК
  стоит сборка logon provider `1.3.15`; релиз `1.4.62` в GHCR ещё не
  установлен (`PENDING.md`). Настройка — `projects/home/windows-pc/README.md`.

## Планировщик и Android

- scheduler (`solutions/scheduler`): SQLite на PVC `scheduler-data`, схема v11,
  хранит только UTC, клиентская зона — заголовок `Time-Zone`; app-доступ —
  модули `base/home/scenarios/alarms_timers/intercom_call/notifications/chat/
  vpn/system/infra_reports/alerts/secrets/admin` (токены в `app_tokens`, только
  sha256; `secrets` — owner-only, как и `admin`), owner — один, остальные
  токены выпускает admin API; push-устройства привязаны к токену регистрации.
  Исполнители:
  ha-bridge (очередь `home`, execute), ha-control (`control`), ha-scenario
  (`scenario`); voice-proxy — consumer очередей озвучивания. Второго
  исполнителя на те же расписания нет.
- Контракт — `packages/proto/home/v1`; Connect JSON/binary и gRPC-Web по
  HTTP/1.1, gRPC over h2c не отдаётся.
- Нативные сценарии «Блэкаут» и «Сел работать» —
  `projects/home/kubernetes/scheduler/scenarios/`.
- Android `dev.proflyder.home` (`projects/mobile/android`): вкладки Home,
  Scenarios, Time, Чат, Система; звонок на колонку, push «найди телефон»,
  встроенный VPN. Набор вкладок, строк настроек, быстрых плиток и действий
  следует модулям токена из `GET /v1/me`; при `403 module_forbidden` клиент
  обновляет `/v1/me` и показывает локализованное «нет доступа». Экран
  «Система → Доступы» (`admin`) выпускает именованные токены, меняет их
  модули и отзывает; plaintext показывается один раз и не попадает в
  сетевые инспекторы. «Система → Секреты» (`secrets`, только owner) ходит в
  secrets-gateway через Traefik `secrets.<domain>`: биометрия/пароль
  устройства на сеанс процесса, step-up подписью ключа телефона (5 минут для
  просмотра, подпись каждой операции для update/rollback/delete), привязка
  одноразовым кодом: непривязанный телефон видит компактную карточку
  «Привязать» первой строкой каталога, привязанный — пункт «Устройство» в
  меню экрана (статус, дата, перепривязка), авто-маскировка значения через
  30 секунд, очистка
  буфера обмена через 30 секунд, `FLAG_SECURE` и исключение клиента из
  сетевых инспекторов. Адреса и таймауты — Firebase Remote Config
  `home_connection` (`schedulerBaseUrl`, `controlBaseUrl`, `intercomBaseUrl`,
  `secretsBaseUrl`); выкладка — Firebase App Distribution из CI. Живая
  приёмка большинства экранов не пройдена (`PENDING.md`).

## Прочие сервисы

- currency-bot (`solutions/currency-bot`): namespace `currency-bot`, webhook
  через Cloudflare Tunnel, API `currency.home.<domain>`, голосовой intent
  «курс валют». VPS сервисов не держит.
- mcp-gateway (`solutions/mcp-gateway`): группы `infra.*`, `bb.*`, `time`,
  `phone`, голосовые маршруты `/v1/voice/*`, API приложения `/v1/system/*` и
  `/v1/chat/*`. Мост в bb — SSH forced command на CT 103.
- phone-push (`solutions/phone-push`): FCM HTTP v1; телефоны хранит scheduler.
- secrets-gateway (`solutions/secrets-gateway`): owner-only API секретов
  приложения поверх OpenBao KV v2 (роль `secrets-gateway`, audience
  `secrets-gateway`), модуль `secrets`, step-up подписью EC P-256 ключа
  телефона (одноразовая привязка кодом с проверкой Google key attestation,
  использованные nonce на PVC), audit в
  лог и phone-push; значения не хранит и не логирует. Каталог и deny-список
  приходят из ConfigMap, `readyz` проверяет OpenBao.
- Чат приложения с Hermes — отдельная сессия `home-app` через chat relay
  voice-proxy `:8644`.

## Бэкапы

- Уровни: vzdump (на SSD узла), HA backup, offsite — K3s timer 04:45, restic →
  Google Drive (`rclone:gdrive:home-offsite-v2`, client-side шифрование).
  Состав offsite: raft snapshot OpenBao, SQLite scheduler/currency-bot,
  ledger ha-bridge, backup HA, `bb server export`. Seal key, recovery keys и
  пароль restic в архив не входят. Restore — `REBUILD.md`, `OPERATIONS.md`.

## Ограничения

- Один K3s node, без HA control plane и распределённого storage.
- Публичного HA/Grafana/OpenBao нет; доступ только LAN/VPN.
- Проверенного disaster restore с чистого железа нет.
- Станция — единственный экземпляр; factory/recovery image отсутствует.
