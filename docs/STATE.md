# STATE — текущее состояние

Публичные адреса — примеры; реальные — в `secrets/NETWORK.md`. Открытая
работа — `PENDING.md`; команды — `OPERATIONS.md`.

## Host / VM

- Proxmox VE `9.2.24`, kernel `7.0.14-26-pve`, host `pve`.
- Железо: DEXP Jupiter JU01CY, i7-12650H, 32 GiB RAM (2×16 GiB DDR4-3200),
  NVMe 512 GB (система, `local-lvm`), SATA SSD Apacer AS350 512 GB — только
  бэкапы (storage `backup`, `/mnt/pve/backup`, ext4, `nofail`).
- Ночной vzdump `nightly` 04:00: VM 100/101 и CT 102/103, snapshot + zstd,
  retention `keep-daily=1,keep-monthly=2` (свежий + последний
  прошлого месяца). Копия на том же узле; vzdump
  VM 101 содержит и raft OpenBao, и Secret `openbao-seal` — не вывозить
  без шифрования.
- Management `vmbr0`: `192.0.2.26/24`; uplink — Wi-Fi `RTL8821CE` в
  4-address/WDS к `vmbr0`, Ethernet — резерв по carrier. NAT/PCI passthrough
  не используются; VM сохраняют L2-адреса и IPv6 multicast.
- `ricardo-uplink` держит ровно один physical uplink; полные перезагрузки PVE
  без Ethernet проходят.
- Power-loss тест 2026-11-02 (выдернуто питание): PVE, HAOS, K3s (30/30 подов),
  VPN, OpenBao auto-unseal, scheduler со всеми consumer, backup SSD и таймеры
  поднялись сами за ~4 минуты; станция видна в HA.
- VM `100 haos`: 2 vCPU / 4 GiB / 32 GiB, HAOS `2026.10.4`, `192.0.2.28`.
- VM `101 lab-k3s`: 8 vCPU / 12 GiB / 200 GiB, Ubuntu 24.04.5, K3s,
  `192.0.2.27`; startup order 1/2.
- CT `102 vpn`: unprivileged Debian 13, 1 vCPU / 512 MiB / 4 GiB,
  `192.0.2.29`, onboot, ocserv `443/tcp+udp`, пул `10.77.0.0/24`.
- CT `103 bb`: unprivileged Debian 13, 2 vCPU / 2048 MiB / swap 1024 / 16 GiB,
  onboot `order=3`, Node.js 26 из tarball; `install-bb.sh` прошёл живьём —
  linger и systemd user bus под `bb` работают, контейнер переживает reboot,
  добавлен в vzdump `nightly` (04:00, согласованная SQLite-копия в 03:30).
- Сервер bb `0.48.1` перенесён на CT 103 2026-11-03 (`bb server move --to bb`,
  проверено primary): работает под пользователем `bb` как systemd
  user-сервис, данные — `/home/bb/.bb-machines/<server-host>`, клиенты
  подключаются через bb Connect (адрес не менялся). Mac остаётся машиной bb
  (launchd-агент, Node из Homebrew): проекты и их треды выполняются там.
  Перенесены все 125 тредов и 43 плагина (37 включены). Два плагина работают
  на CT из локальных path-источников, скопированных вручную:
  `profiles` (`solutions/bb-profiles`; изоляция work/personal, политика
  work = только `claude-code` и только рабочий Mac, `hostHomes` для путей
  `~/.bb-profiles`) и `home-dispatcher` (`solutions/bb-home-dispatcher`;
  шесть методов, `workerLimit=2`, `permissionMode=auto`, автосжатие
  диспетчера 04:30 и по 70% контекста). Маршрут плагина `home-dispatcher`
  снаружи через bb Connect закрыт (401); единственный путь — SSH-мост
  `bb-dispatch` (см. «MCP gateway»). На CT не залогинены `gh` и Codex — для
  тредов на Mac это не важно (PR-статус считается на машине окружения,
  ИИ-сервисы bb идут через bb cloud), для агентов на самом CT нужно позже.
  Бэкап: 03:30 в CT — `sqlite3 .backup` и полный `bb server export`
  (`/var/backups/bb`), offsite забирает экспорт по SSH-ключу с forced command
  (только чтение архива, без проброса портов); restore через
  `bb server import` из Drive проверен 2026-11-03. Runbook — `projects/home/proxmox/bb/`.
- Terraform: provider `bpg/proxmox 0.118.1`, plan — No changes; state
  root-only на PVE, копия в `secrets/`.
- Wi-Fi/Ethernet uplink runbook: `projects/home/proxmox/wifi-uplink/`.

## HAOS

- HA: `http://192.0.2.28:8123`; `internal_url` и trusted proxy обновлены.
- Add-ons:
  - Music Assistant `2.10.8`: UI/API `8095`, stream `8097`, Sendspin `8927`.
  - Matter Server `9.2.0`: `ws://127.0.0.1:5580/ws`; paired 1 узел —
    Yandex LED strip `YNDX-00544` («Лента», `light.spalnia_lenta`), on/off,
    яркость и цвет проверены. Дублирующий набор сущностей от повторного
    интервью остался неактивным.
- HAOS — единственный HA runtime.

## K3s

- K3s `v1.35.9+k3s1`, node `lab-host`; data-dir `/srv/k3s/runtime`; SQLite;
  local-path PV с `Retain`.
- Kubelet: логи контейнеров 10Mi×3; неиспользуемые образы удаляются через
  48 ч (`imageMaximumGCAge`, drop-in `agent/etc/kubelet.conf.d/50-image-gc.conf`
  в data-dir), иначе — при 85% диска. Откат берёт образ по digest из GHCR.
  Образы тянутся параллельно, до 4 одновременно (`serializeImagePulls: false`,
  drop-in `50-image-pull.conf`); таймауты выката — 600 с.
- Namespaces: `home-assistant`, `monitoring`, `traefik`, `cert-manager`,
  `openbao`, `external-secrets`, `currency-bot`.
- Приложения: GigaAM `10305`, Whisper `10300`, Piper `10200`,
  Hermes gateway `127.0.0.1:8642` + voice proxy `8643`, health exporter `9116`,
  currency-bot + cloudflared, mcp-gateway (ClusterIP `:8080`),
  intercom (signaling, NodePort `30097`).
- `8643` доступен только HAOS/K3s/loopback через nftables.
- Observability: kube-prometheus-stack `92.14.0`, Loki `20.0.2`,
  Alloy `1.14.3`; четыре dashboard JSON через Helm ConfigMap.
- Alertmanager шлёт warning/critical в личный чат владельца через инфра-бот
  Hermes (`telegram_configs`, resolved тоже). Конфиг — Secret
  `alertmanager-telegram`, его рендерит ESO из
  `k8s/home-assistant/hermes-environment` (`TELEGRAM_BOT_TOKEN`, первый
  `TELEGRAM_ALLOWED_USERS`); Watchdog и info не отправляются. В каждом алерте
  — отпечаток для `infra_alert`.
  Оформление — HTML-шаблон `projects/home/kubernetes/observability/alertmanager/telegram.tmpl`
  (ConfigMap `alertmanager-templates`, время Asia/Almaty).
  Доставка проверена тестовым алертом 2026-11-01.
  Внешний dead-man switch: `Watchdog` раз в 5 минут уходит webhook-ом на
  Cloudflare Worker `home-heartbeat` (`solutions/heartbeat`, деплой
  `projects/home/cloudflare/heartbeat`, URL/token — OpenBao
  `k8s/monitoring/heartbeat`). Тишина > 15 минут → «дом молчит» через тот же
  инфра-бот, повтор раз в 6 ч, затем recovery. Outage-тест 2026-11-02
  (silence на Watchdog): «молчит» через ~16 минут, recovery через 5 с после
  первого пинга.
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
  audit `stdout/` (ротация kubelet 10Mi×3, Alloy → Loki 7 дней), UI включён.
- Auto-unseal — встроенный `static` seal (ключ в Secret `openbao-seal`), после
  `kubectl delete pod` сервер распечатывается сам.
- Auth: `userpass` (человек, политика `admin`; пароль задаёт человек; обязательный
  TOTP `human-totp`, устройство зарегистрировано, enforcement включён), `jwt` `github` (GitHub OIDC,
  роль `deploy-home`, environment `Home`, ref `develop`, TTL ≤15 мин),
  `kubernetes` (TokenReview, роли `external-secrets`/`k8s-smoke`/`backup`).
- KV v2 `secret/`: `secret/ci/test`, `secret/ci/deploy-home` и
  `secret/k8s/*`. `secret/ci/deploy-home` (`kubeconfig`, `scheduler_env`,
  `runtime_values`) — источник для CI; в GitHub этих значений больше нет.
  `deploy-home` входит по OIDC после VPN, читает запись через
  `hashicorp/vault-action`, материализует файлы 600 и маскирует значения;
  токен живёт только до конца job (TTL роли), отдельного revoke нет; политика
  `ci-deploy` — read только `secret/ci/test` и `secret/ci/deploy-home`.
- Ночной CronJob raft snapshot в `/srv/k3s/volumes/openbao-backups`
  (7 файлов). Root-токен отозван, recovery keys — только `secrets/openbao/`.
- Restore OpenBao snapshot и SQLite scheduler проверен на локальном стенде
  2026-11-02 (`REBUILD.md` §6, `openbao/restore_drill.py`).
- External Secrets Operator `2.14.0` (chart `2.14.0`, образ по digest) в
  namespace `external-secrets`: `ClusterSecretStore/openbao` через in-cluster
  Service (Kubernetes auth, роль `external-secrets`, KV v2). Источник правды
  для `secret/k8s/*`: `scheduler-auth`, `ha-bridge-ha`, `hermes-environment`,
  `intercom`, `grafana-admin`, `cloudflare-api-token`, `wakegrant`. Kubernetes Secrets с теми же
  именами/ключами создаёт ESO (`creationPolicy: Owner`,
  `deletionPolicy: Retain`, `refreshInterval: 1h`); deploy-скрипты и CI их не
  перезаписывают. Не переносятся: `openbao-seal`, helm release/SA token
  Secrets, TLS и служебные Secrets контроллеров. Файлы —
  `projects/home/kubernetes/openbao/` и
  `projects/home/kubernetes/external-secrets/`.
- Vaultwarden `1.37.5` (образ по digest) в namespace `vaultwarden`,
  `https://passwords.home.example.com/` (LAN/VPN): менеджер паролей для
  человека, клиенты Bitwarden. Регистрация закрыта, `/admin` включён (хэш
  токена — `secret/k8s/vaultwarden/vaultwarden-admin` через ESO), egress только DNS,
  SQLite на PVC `vaultwarden-data` (`local-retain`), ежедневно в offsite.
  Файлы — `projects/home/kubernetes/vaultwarden/`.
- Доступ с рабочих мест: у каждой машины свой periodic operator token
  (`workstation-<host>`, политика `operator-kv`, `mise run secrets:login`);
  каталог путей — `docs/SECRETS.md` (52 пути, `mise run secrets:check`).
  Recovery keys и seal key на диске не хранятся — у владельца в Bitwarden и
  Vaultwarden (`openbao:bootstrap`).

## CI/CD и образы

- GitHub-hosted runners. Карта юнитов — `platform/ci/units.mjs`, запуск —
  платформенные reusable workflows (`.github/workflows/_*.yml`),
  обязательный итог — `ci-ok`. Прямой push в `develop` запрещён; `develop`
  обновляется только squash-merge PR.
- Версии инструментов CI и рабочего места — один `mise.toml` (+ `mise.lock`
  с sha256 linux-x64/macos-arm64): Python 3.13, Node 22, Bazelisk,
  gitleaks, actionlint, shellcheck, kubectl, helm, oras, bao, gh, protoc, bw.
  CI ставит подмножество через `jdx/mise-action`; Bazel — `setup-bazel`.
  Рабочее место — `mise run setup` (`docs/WORKSTATION.md`).
- Локальные скрипты с секретами — только `mise run admin:*`: fnox 1.47
  (`fnox.toml`, профиль на скрипт, только ссылки на OpenBao) передаёт секреты
  во временных файлах; туннель к API K3s поднимается на время команды.
- `deploy-home` (push в `develop` = merge PR) деплоит только затронутые
  юниты параллельно: входит в VPN группой `ci`, затем OIDC в OpenBao,
  kubectl → K3s API `6443`, HTTPS → Traefik/OpenBao `443`. Режимы
  (`preserve`/`execute`/`shadow`/`control-*`) — только через
  `workflow_dispatch`.
- scheduler (Rust, static musl), ha-bridge, ha-control, ha-scenario, intercom —
  собственные distroless-образы, собираемые Bazel `rules_oci`/`rules_rust` и адресуемые по
  digest в `ghcr.io/proflyder/<name>@sha256:…`; CI пушит их job-scoped
  `GITHUB_TOKEN`, живой узел импортирует те же digest'ы в containerd.
- Hermes и voice-proxy — публичные digest-pinned образы GHCR
  (`ghcr.io/proflyder/{hermes,voice-proxy}@sha256:…`), без pull-secret.
  `hermes` собирается Docker из upstream-ревизии
  (`projects/home-assistant/deploy/images/hermes/revision`) воспроизводимо:
  базовые образы закреплены по digest, apt — из snapshot Debian,
  `SOURCE_DATE_EPOCH` = время upstream-коммита, сборка через Buildx с
  `rewrite-timestamp` (общий `hermes/build.sh`, проверка —
  `verify-reproducible.sh`); повторная сборка той же ревизии не меняет digest и
  не вызывает rollout. Слои кэшируются в GHCR
  (`hermes:buildcache`, запись только у job'ов с правом записи), apt повторяет
  запросы к snapshot и установку пакетов — кэш и повторы не влияют на digest.
  `voice-proxy` — Rust-сервис `solutions/voice-proxy/crates/voice-proxy`,
  собираемый Bazel `rules_rust` + `rules_oci` (static musl, distroless) и
  пушимый байт-в-байт через `oci_push`. Health exporter — Rust-сервис
  `projects/home-assistant/deploy/health-exporter/crates/health-exporter`,
  собранный тем же `rust_service_image` (static musl, distroless) в отдельный
  `ghcr.io/proflyder/health-exporter`. PR CI (`_images.yml`, builder `mixed`,
  карта `deploy.bazel_images`) собирает и публикует gateway, proxy, exporter и
  `home-deploy` под `tree-<hash>`; `deploy-home` продвигает digest'ы, затем
  `home-deploy hermes rollout` подставляет digest'ы,
  проверяет `-healthcheck`/health/Assist/логи и делает `helm rollback` при
  провале. Assist smoke Job работает на `home-deploy` (`hermes assist-smoke`).
  Voice-proxy принят вживую на станции: оба профиля, подтверждение с
  открытым микрофоном, отмена, поздние ответы.
- Деплой — `home-deploy converge` (Rust-крейт `platform/deploy-rs`): полная замена Deployment, сохранение env/режима и
  replicas; откат восстанавливает только применённый объект целиком.
- PV/PVC применяет `apply-runtime.sh` через kubectl; Helm управляет только
  namespace-scoped workloads, поэтому `ci-deploy` не имеет cluster-scoped
  прав.
- Pulsar собирается в монорепо: исходники лежат в `projects/station/pulsar`,
  сборка — Bazel (Rust + герметичный zig cc-toolchain + sendspin-cpp из
  `http_archive`); монорепо собирает его в CI-юнитах `pulsar-bazel` и `pulsar`
  (`_bazel.yml`/`_native.yml`). Весь first-party Rust (Pulsar, BLE gateway,
  Windows logon provider, voice-recorder helper) — один корневой Cargo
  workspace: `/Cargo.toml`, `/Cargo.lock`, `/rust-toolchain.toml`, один
  `@crates//` и общие `[workspace.lints]` (решение 40). Общий core-слой —
  `packages/rust`: `home-telemetry` (tracing/фильтр/локальное время),
  `home-runtime` (SIGTERM/SIGINT + `Supervisor` на
  `CancellationToken`/`TaskTracker`, `spawn`/`spawn_local`), `home-actor`
  (`Handle`/`channel`; спавн через `Supervisor`), `home-linux` (errno и sysfs
  поверх `rustix`), `home-secret` (zeroize/0600/0700, constant-time) и
  `home-http` (axum bearer + graceful serve). Pulsar пишет логи через
  `tracing`; stderr-формат C++ и ESPHome-sink сохранены слоями
  `pulsar-core`, записи сторонних крейтов идут мостом `tracing-log`. BLE gateway
  переписан на них: нативный control-сокет `wpa_supplicant`, `zbus` proxies,
  `watch`-статус и сериализованные Commit'ы. Pulsar — publishable
  native-юнит общего `deploy-home` с
  `publish_when` по `projects/station/pulsar/VERSION`: любое изменение Pulsar
  собирается и публикуется PR CI из этого репозитория как `tree-<hash>` с
  provenance, а на `develop` `native-release` продвигает проверенный digest на
  релизные теги; `publish_when` ограничивает только релиз. Затем job
  `deploy-station` после
  approve в Environment `Station` входит в VPN группой `ci` (отдельный
  сертификат `ci-pulsar`) и по OIDC в OpenBao роль `deploy-station` (политика
  `ci-station`, `secret/ci/deploy-station`: `ha_url`, `ha_token`, `entity_id`)
  вызывает HA `update_entity` + `update.install`. Job ждёт
  `installed_version == version` и 90 с стабильности; откат `supervise.sh` на
  `.prev` валит job. GitHub Releases и теги в монорепо не используются.
  Повторный запуск — `workflow_dispatch` deploy-home с `units=pulsar` (катит
  уже опубликованный VERSION, тег не перезаписывается). Откат — не через CI,
  а на станции (`.prev` + `ctl.sh reload`).

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
- Pulsar Rust `v0.5.1013` — armhf-сборка из `projects/station/pulsar/`
  (Bazel), раскатана `deploy-station` 2026-11-08 (`.prev` = `v0.5.1012`;
  артефакт продвинут из PR-сборки без пересборки). «Громкость объявлений»
  (`number.…_gromkost_obiavlenii`, 0–100 %, по умолчанию 100) масштабирует
  только речевые анонсы и TTS-ответы; ANNOUNCING уходит в HA как PLAYING
  (HA 2026.10 не знает это состояние). Автояркость часов —
  перцептивная (гамма 2 к фактору освещённости, как у кольца): датчик под
  крышкой видит 0–60 лк, при ~18 лк ток часов 64/255, в темноте пол 5;
  линия Ricardo, база upstream `v0.5.4` = `f1f5b17`. Внутри — завершённый
  Rust-рефакторинг (решение 40): крейты за портами, явные состояния
  фронтенда, голоса и арбитража, `pulsar-playback`, Zigbee разделён и
  выключен; pairing PSK и токен Sendspin в журнал не пишутся.
  Включает повторный wake, заменяющий активный диалог, экспериментальный
  локальный endpointing, Ricardo pre-roll 256 ms и запись wake-событий
  (`/data/pulsar/wake-events/`, cutoff/window из manifest модели); из
  upstream `v0.5.4` — number «Follow-up duration», STT stop-word fallback,
  switch часов «Clock auto brightness», Mini gen1 и Mini 3 (experimental),
  S32 capture, переработанные volume/mixer/player/pcm_out/sendspin и
  `deploy/asound.conf`. «Wake: не услышала» сохраняет последние 30 с
  (кольцевой буфер 31 с). Процесс запущен с `PULSAR_LOCAL_ENDPOINTING=1`,
  унаследованным supervisor'ом от `ctl.sh reload`; это не boot-persistent:
  после reboot или нового supervisor без env feature выключен.
  `--version` различает сборки по тегу.
- `v0.5.4` в Ricardo-профиле: в диалоге, открытом «Рикардо», распознанный текст всегда
  уходит в HA — локальной отмены/паузы нет, поэтому «Рикардо, стоп» доходит
  до HA. Для диалогов по кнопке, follow-up и `start_conversation` отдельное
  «стоп»/«хватит» отменяет диалог локально и останавливает звонящий таймер.
- Версия Pulsar — `v<upstream major.minor>.<1000+N>`: updater демона ставит только строго большую
  `MAJOR.MINOR.PATCH`, а upstream уже имеет `v0.5.4`. Релиз, публикация и
  деплой — `projects/station/pulsar/RICARDO.md` «Релизы и деплой»; репозиторий
  обновлений — константа `pulsar_update::UPDATE_REPO`.
- Upstream `Morvex885/pulsar` вливается вручную патчем (`git diff vA..vB` →
  `git apply -3 --directory=projects/station/pulsar`), затем обычный PR.
  Прежние GitHub `proflyder/pulsar` и `pulsar-fork-old` удалены. Откат на станции —
  `pulsar.prev` (предыдущий бинарник рядом с текущим).
- ESPHome `6053`, Sendspin `8928`, HTTP control `8099` (bearer), ADB `5555`,
  SSH `22`; Noise; HA satellite работает.
  Switch «Clock auto brightness» включён: автояркость часов по датчику
  освещённости, при выключении — уровень «Clock brightness».
- `number.iandeks_stantsiia_midi_follow_up_duration` — «Follow-up duration»
  (0–10 с, по умолчанию 10, `followup_duration_s` в `state.conf`, ESPHome
  key 39).
- `select.iandeks_stantsiia_midi_voice_color` — цвет анимации слушания
  (Violet/Blue). Автоматизация HA `voice_profile_station_color` ставит Blue для
  рабочего профиля и Violet для личного: при смене
  `input_select.ricardo_voice_profile`, при старте HA и когда select появляется
  после недоступности; пока сущности нет — no-op. Прямое управление
  `light.ring`/`led_show` во время LISTENING перекрывается voice state прошивки,
  поэтому профиль задаётся только этим select.
- Модель Ricardo загружается с cutoff 0.90 / window 10 из manifest, `PULSAR_LOCAL_ENDPOINTING=1` сохранён, запись
  wake-событий включена (`state.conf`: `wake_event_recording=1`,
  `wake_dry_run=0`), первое near-событие записано. Сущности HA:
  `button.iandeks_stantsiia_midi_wake_lozhnoe_srabatyvanie`,
  `button.iandeks_stantsiia_midi_wake_ne_uslyshala`,
  `switch.iandeks_stantsiia_midi_wake_zapis_sobytii`,
  `switch.iandeks_stantsiia_midi_wake_testovyi_rezhim_bez_otveta`.
- SSH: dropbear `v2026.94`; ключ Mac `~/.ssh/home-lab/station_ed25519` →
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
  копия — `local-data/local/backups/wakeword-v24-pre-v27-20261030T160839Z/`.
  Все веса сохранены private на Mac и WSL; installed-запись и hashes —
  `local-data/model-registry/ricardo/active.json`.
- Актуальная уточнённая разметка и PCM: `local-data/datasets/ricardo-fixed-20261030/`;
  исходный приватный датасет — `local-data/datasets/ricardo-20261029/`
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
  updater'а; при смене диапазонов обновить `GITHUB_NETS`.
- Известная проблема: после reboot станция иногда не отвечает в LAN при
  online-статусе роутера; BLE остаётся доступен, снятие питания на 20–30 с
  восстанавливает IP/HA/голос. Первопричина не установлена (`PENDING.md`).

## Голос / музыка / расписания

- Pipeline «Рикардо»: GigaAM v3 CTC int8 primary, Whisper `medium-int8`
  fallback; локальные intents → Hermes только для свободной речи.
- **Профиль голоса.** `input_select.ricardo_voice_profile` (Личный/Рабочий) —
  единственный источник истины; переключается только вручную: голосом
  («рабочий/личный профиль», «какой профиль»), на дашборде «Помощник» → view
  «Голос», в Android карточкой «Личный | Рабочий». В HA это
  `switch.ricardo_work_profile` (template switch над `input_select`) с нативной
  меткой `voice_profile`; в каталоге ha-control он отдаётся как
  `kind: "profile"`, `on` = Рабочий. Автовозврата нет; после рестарта HA
  восстанавливает последнее значение, первый старт — «Личный». Профиль влияет
  только на свободную речь: локальные бытовые команды работают в обоих.
- **Маршрут речи.** Локальные intents остаются в HA. Остальное идёт в
  `voice-proxy` (Rust, `solutions/voice-proxy`, distroless), который читает
  профиль из HA и вызывает `mcp-gateway` `POST /v1/voice/utterance`:
  - профиль не прочитан/неизвестен → «Не могу подтвердить профиль.»,
    Hermes не вызывается;
  - рабочий профиль → `send` в постоянный тред-диспетчер Claude Code
    (Sonnet 5.5, reasoning low) в рабочем bb-проекте на рабочем Mac;
    ответ озвучивается `script.schedule_announce`; ошибка bb остаётся на
    маршруте `bb`, в Hermes не откатывается (fail-closed);
  - личный профиль + включённый режим агента → `send` в активный тред bb,
    ответ так же озвучивается;
  - иначе → Hermes.
  Режим агента включается после `bb_confirm`/ручного запуска треда и
  выключается фразой «выйди из режима агента»; состояние in-memory.
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
  reasoning + streaming; HA MCP 73 tools, MA MCP 86 tools. Третий MCP-сервер
  `infra` (`mcp-gateway`) несёт группы `infra.*`, `bb.read`/`bb.act`:
  `bb_status`, `bb_prepare_spawn` → `bb_confirm`/`bb_cancel`. Hermes видит
  только личные проекты из `bb-targets.yaml`; рабочие и проекты с неизвестным
  профилем отфильтрованы шлюзом и неотличимы от несуществующих. После запуска
  включается режим агента, выход — «выйди из режима агента».
- Voice proxy: маршрутизация по профилю (см. «Профиль голоса»); без всех
  четырёх значений URL+токен для HA и шлюза маршрутизации нет. Exact cancel
  guard: если предыдущий ответ Hermes закончился вопросом, HA включает
  `continue_conversation` и обходит `prefer_local_intents`, поэтому proxy
  перехватывает только точную
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
- **Комп (Windows-ПК).** Голос и приложение управляют проводным десктопом:
  включение — Wake-on-LAN (`switch.komp` → `wake_on_lan.send_magic_packet`,
  MAC только в `secrets.yaml`), выключение — SSH с forced command
  (`komp-power.ps1`, только слова `shutdown`/`cancel`) → Windows
  `shutdown /s /t 60` с отменой, состояние — `binary_sensor.komp_v_seti`
  (`command_line` ping каждые 10 с). Локальные intents `KompOn`/`KompOff`/
  `KompCancelOff`/`KompStatus` работают в обоих профилях без Hermes.
  `script.komp_shutdown` пишет `input_datetime.komp_off_requested` до SSH:
  ping отстаёт на 1–2 минуты, поэтому недавний запрос значит «выключается»,
  а голосовое включение идёт через `switch.turn_on switch.komp` (выдаёт
  пропуск) и не отвечает «уже включён» сразу после выключения. IP,
  MAC и пользователь — только в HA secrets; настройка Windows —
  `projects/home/windows-pc/README.md`. Принято вживую 2026-11-03:
  выключение голосом с отменой, WoL из полного выключения, ограниченный SSH.
- **Вход по пропуску (wakegrant).** `solutions/wakegrant` — Rust-сервис
  (решение 41, пилот): общий core-слой `packages/rust`, статический
  `x86_64-unknown-linux-musl` бинарник на том же distroless-образе; контракт и
  golden-фикстуры — замороженные данные. В K3s
  `home-assistant`: один под, SQLite на PVC `wakegrant-data`
  (`local-retain`), LAN NodePort `30130`, /healthz. `switch.komp.turn_on`
  сначала выдаёт одноразовый пропуск (`rest_command.wakegrant_issue`,
  best-effort: `continue_on_error: true`), затем WoL; логин-компонент ПК
  обменивает nonce и HMAC на `/v1/claim` (протокол
  `packages/contracts/wakegrant/PROTOCOL.md`, тест-вектор закреплён).
  Секреты — ESO `wakegrant` из `secret/k8s/home-assistant/wakegrant`
  (`issuer_token`, `machines`), в HA — `wakegrant_endpoint`/
  `wakegrant_bearer`. Деплой — `projects/home/kubernetes/wakegrant`.
  Пропуска бывают двух видов (`purpose`): `logon` — «включи комп» выдаёт его
  только при выключенном ПК; `unlock` — «разблокируй комп» (+ «разблокируй
  мой комп», «открой комп») выдаёт его на 60 с только при включённом ПК.
  Логин-компонент выбирает режим сам: нет сессии настроенного пользователя и
  с загрузки меньше 10 минут → `logon` (опрос 2 с, бюджет 90 с); сессия уже
  есть (экран блокировки: Windows 11 вызывает и тут `CPUS_LOGON`) → `unlock`
  (long-poll 25 с к `wakegrant`, без бюджета, пока экран на месте); иначе
  ничего. Пропуск одного вида не срабатывает за другой. Модуль входа —
  переиспользуемое решение `solutions/windows-logon-provider` (Rust, CLSID
  `{9459CCEA-4CCC-4435-89BF-E0E132E0C27F}`, журнал
  `C:\ProgramData\Ricardo\KompLogon\logon.log`). На ПК установлена сборка
  `1.3.15` (sha256 DLL `46d67528fff5eda59d0beb069ace326ed3f83c34907e904db81a94a996b9d5ab`;
  PE-таймстемп обнуляется, поэтому релизный тег содержит те же байты), принята
  вживую 2026-11-07: холодный вход по grant, разблокировка по grant,
  недоступный `wakegrant` (одна строка `send_failed`, затем backoff).
  Последний релиз в GHCR — `1.3.16` (кода с `1.3.15` не менялось, на ПК не
  ставился).
  Прежние сборки лежат рядом для отката: `1.3.14`, `1.3.0`, `1.2.0` (Rust)
  и C++ `1.0.0`. HA-файлы `ha-voice` выложены в HAOS вручную (копия прежних —
  `/mnt/data/ha-voice-backup-*.tar`).

## Планировщик, HA bridge, Android

- `solutions/scheduler` задеплоен в K3s `home-assistant`: один под, SQLite на
  PVC `scheduler-data` (`local-retain`), NodePort `30098`, bearer обязателен,
  readiness/liveness — аутентифицированный `-healthcheck`. Состояние
  переживает рестарт пода. Процесс — Rust-крейт
  `solutions/scheduler/crates/scheduler` (static musl, `rules_rust`); Go-код
  удалён, golden-фикстуры перенесены в `solutions/scheduler/testdata` как
  замороженные данные. Контракт — `packages/proto/home/v1` (`home.v1`),
  единый для scheduler, bridge, Android и HA. Connect JSON/binary и
  gRPC-Web по HTTP/1.1 работают, gRPC over h2c пока не отдаётся (connectrpc
  собран только с `json`).
- `solutions/ha-bridge` работает в режиме execute на очереди `home`
  (`BRIDGE_EXECUTE=true`, 1 реплика, ledger на PVC `ha-bridge-data`), зовёт
  allowlisted HA scripts; второй исполнитель не запущен. Dry-run Job
  (`ha-bridge-dryrun`) остаётся одноразовой проверкой. Исполнитель — Rust-крейт
  `solutions/ha-bridge/crates/ha-bridge` (static musl, `rules_rust`, uid 65534,
  тот же `/data/ledger.json`); golden-векторы
  `testdata/events`, `testdata/ledger` и `testdata/catalog` — замороженные
  данные для Rust-тестов.
- Действующий владелец будильников, таймеров и напоминаний — scheduler.
  Старые HA-планировщики и их вспомогательные поля удалены из исходников и
  живой конфигурации; оставшиеся недоступные записи реестра очищены.
  Повторного исполнителя нет. HA остаётся исполнителем звука
  (`alarm_start`, `schedule_announce`, `alarm_stop`).
- Схема scheduler — v2: добавлена таблица `scenarios`; существующая v1
  мигрирует на месте. `home.v1.ScenarioService` хранит определения сценариев
  (manual/time-триггеры, условия, шаги, revision), time-триггеры
  материализуются строками `schedules` (`kind=SCHEDULE_KIND_SCENARIO`) и
  событиями `scenario.fire` в очереди `scenario`; `RunScenario` ставит
  manual-событие туда же. Исполнитель — третий бинарь
  `solutions/ha-bridge` `ha-scenario` (static musl, uid 65534, entrypoint
  `/usr/local/bin/ha-scenario`) отдельным Deployment: 1 реплика, consumer
  `ha-scenario-home`, очередь `scenario`, ledger `/data/scenario-ledger.json`
  на PVC `ha-scenario-data`, `SCENARIO_EXECUTE=true`, `-healthcheck` —
  аутентифицированный `GetStatus.ready`. `GetStatus.scenario_store_supported=true`;
  `scenario_execution_supported` — deployment-флаг scheduler
  `SCENARIO_EXECUTION=true` в манифесте. Исполнитель читает
  `ScenarioService/GetScenario` (bearer как у ha-control), вычисляет условия
  (`entity_state`, `time_window` с окном через полночь и weekdays, timezone
  первого time-триггера или `Asia/Almaty`) и выполняет шаги
  catalog/speak/delay по живому каталогу. STOP отменяет `delay`, второй FIRE
  того же сценария — `SKIPPED`; ledger различает `started`/`completed`, поэтому
  прерванный прогон не повторяется (`FAILED interrupted`). Ретраи — только до
  первого шага. Текст speak не пишется в логи, ledger и detail.
- `RingWindow` = максимальная длительность срабатывания (900 с по
  умолчанию). Истечение окна закрывает occurrence: tick в одной транзакции
  отменяет pending `FIRE` того же occurrence и ставит STOP в durable очередь
  (одна логическая запись, dedupe `expiry:stop:<kind>:<id>:<occurrence>`;
  доставка at-least-once, HA повторный STOP игнорирует). Bridge доставляет
  STOP, HA проверяет source+FIRE event id и не трогает более новый звонок.
  Правка правила во время звонка (`UpdateSchedule`/enable) тоже закрывает
  occurrence и ставит STOP (`DetailRuleChanged`).
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
- Бытовые сценарии «Блэкаут» и «Сел работать» — нативные определения
  `home.v1.ScenarioService` (manual-триггер, шаги `catalog`,
  `continue_on_error=true`), а не HA-скрипты. Голосовые intents
  `StationBlackout`/`StationWorkMode` вызывают `script.scenario_run`: он делает
  `ListScenarios`, находит сценарий по точной метке (без учёта регистра),
  ставит `RunScenario` с idempotency key от `context.id` и при промахе только
  пишет `system_log` warning. Определения и идемпотентный сид живут в
  `projects/home/kubernetes/scheduler/scenarios/` (`blackout.json`,
  `work_mode.json`, `seed.mjs`): сид создаёт только отсутствующие по метке
  сценарии (`idempotencyKey=seed-<slug>`) и не перезаписывает существующие.
- `ha-control` (второй бинарь того же Rust-крейта, static musl, uid 65534,
  entrypoint `/usr/local/bin/ha-control`) развёрнут отдельным Deployment:
  каталог `GET /v1/catalog` из live HA, очередь `control`;
  `CONTROL_EXECUTE=true`,
  consumer `ha-control-home` в allowlist планировщика рядом с
  `ha-bridge-home` и `ha-scenario-home`. Скрипты попадают в каталог только с
  нативной меткой HA `scenario` (`CONTROL_SCENARIO_LABEL`); иконки — только
  отображение. После перевода «Блэкаута» и «Сесть работать» в scheduler метка
  `scenario` осталась у HA-сценариев обучения Рикардо
  (`script.wake_false_alarm`, `script.wake_missed`); нативные сценарии
  scheduler в этот каталог не входят, поэтому раздел сценариев каталога
  показывает только помеченные HA-скрипты (или пуст, если метки не
  назначены). 15 light/switch, служебные скрипты
  отсутствуют. LAN NodePort `30099`,
  HTTPS `https://control.home.example.com` (LAN/VPN). Доставка
  at-least-once, не exactly-once. Прежний образ `ha-control` остаётся в GHCR для
  отката по digest; Rust-контракт каталога закреплён `testdata/catalog`.
  Title/Room режутся по символам (не по байтам), `Light.Mode` определяется до
  live-значений, HTTP-сервер ограничен таймаутом запроса. Действие
  `action.station_speak` («Сказать на колонке», `kind: "action"`) появляется в
  каталоге только при наличии `script.speak_text` в HA; команда `speak`
  вызывает `script.turn_on` с `variables.message` (1–500 символов, без
  управляющих). Текст — пользовательские данные: не пишется в логи, detail и
  ledger. Device/room действия — от `assist_satellite` колонки.
- `solutions/intercom` — Rust-сервис сигналинга WebRTC-звонка телефон ↔
  колонка (SDP/ICE relay, `home.v1.IntercomService`, server-streaming
  `Watch`); медиа P2P, сервис его не несёт. Deploy-юнит `intercom` включён:
  образ `ghcr.io/proflyder/intercom`, ESO `intercom` из
  `secret/k8s/home-assistant/intercom` (`phone_token`, `station_token`),
  `intercom_env` в `secret/ci/deploy-home` (`INTERCOM_NODE_PORT=30097`),
  NodePort закрыт общим host-фильтром scheduler/control. Первый выкат — merge
  этой ветки в `develop`; до этого живой Deployment отсутствует.
- `projects/mobile/android`: Kotlin-клиент (Views, одна Activity, Metro DI,
  `applicationId dev.proflyder.home`, minSdk 26 / target 36, только
  `INTERNET`, debug-signed). Вкладки **Home**, Alarms, Timers, Reminders,
  Connection; Home читает `GET /v1/catalog`, команды идут через
  `home.control`/очередь `control`. Bearer token в Keystore AES-GCM;
  адреса scheduler/control и таймауты — Firebase Remote Config
  (`home_connection`), фильтр Home и зашифрованный токен — Jetpack DataStore
  с миграцией из SharedPreferences; idempotency keys, redirects выключены.
  Home также показывает действие «Сказать на колонке» (`kind: "action"`):
  карточка открывает диалог текста (до 500 символов) и ставит
  `{"entry":"action.station_speak","action":"speak","text":…}` в `control`.
  Debug APK установлен на телефон; холодный запуск и каталог проверены,
  интерактивные фильтры/сценарии/toggle ещё не приняты.

## Currency bot

- `solutions/currency-bot` (Rust, крейт `crates/currency-bot`) заменил
  Kotlin `tg-bot-currency` с тем же поведением: парсер kurs.kz, часовой job в
  :00 UTC, алерты только при порогах HOUR/DAY/WEEK/MONTH с dedup в
  `sent_alerts`, retention 35 дней, команды `/start` `/help` `/trigger`.
  `testdata/` — замороженные контрактные данные,
  которые Rust воспроизводит. Схема БД v1 и файл `/data/currency-bot.db`
  совместимы с прежним образом, откат — предыдущий digest.
- Публичный HTTPS (kurs.kz, `api.telegram.org`) идёт через общий крейт
  `packages/rust/home-tls` (решение 45): системный CA-бандл distroless
  `/etc/ssl/certs/ca-certificates.crt` или override `SSL_CERT_FILE`; без
  бандла процесс падает на старте, а не молча не ходит по TLS.
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
- Secrets — ESO из OpenBao `k8s/currency-bot/currency-bot` (`BOT_TOKEN`,
  `CHAT_ID`, `WEBHOOK_SECRET`, `API_TOKEN`) и
  `k8s/currency-bot/cloudflared-tunnel` (`TUNNEL_TOKEN`).
  `CHAT_ID` — два чата: первый (группа/канал) получает алерты, второй — личный
  чат владельца для команд; команды из других чатов игнорируются.
  NetworkPolicies: 8081 только от cloudflared; egress cloudflared к Cloudflare
  `7844/udp` + `443/tcp` и DNS.
- `/trigger` и `POST /api/trigger` всегда отправляют.
- CI: юниты `currency-bot` (bazel: Rust crate, musl-образ, deploy) и
  `currency-bot-deploy` (contract-тест `deploy.sh`); deploy-env —
  `secret/ci/deploy-home`,
  ключ `currency_bot_env` (в нём должен появиться `CURRENCY_BOT_NODE_PORT`
  рядом с `WEBHOOK_URL`, `LAN_CIDR`, `NODE_LAN_IP`).
  Деплой `currency-bot` зелёный.
- Голосовой intent «курс рубля/доллара/валют» (`StationCurrencyRate`,
  `projects/home-assistant/ha-voice`) live с 2026-11-02: HA читает
  `GET /api/latest` currency-bot через NodePort `CURRENCY_BOT_NODE_PORT`
  (host-nftables `ricardo_currency_bot`, LAN/pod CIDR, persistent unit
  `ricardo-currency-bot-firewall.service`) bearer-токеном из `secrets.yaml`.
  Ответ: «купить» = поле `sell`, «продать» = `buy`. Проверено через
  `conversation/process`; живой голос — за владельцем.
- Наблюдаемость: Alloy шипит логи namespace `currency-bot` (редакция
  Telegram bot tokens, Bearer и key=value/JSON секретов); дашборд Grafana
  `currency-bot` (uid `currency-bot`).
- VPS: сервисов нет, nginx на домене отдаёт только 404 (certbot продлевает
  сертификат), наружу открыты лишь 22/80/443. Backup конфигов и данных —
  `local-data/currency-bot/vps-backup/`.
  Приватные материалы — `secrets/currency-bot/` (VPS ssh
  key/config, CSV/DB, tunnel token, Cloudflare tunnel API token, `k8s.env`) и
  `secrets/home/currency-bot.env`.

## MCP gateway (инфраструктурный шлюз)

- `solutions/mcp-gateway` (Rust, статический musl, distroless, один бинарник) — единый MCP-шлюз
  для инфраструктуры: группы `infra.read` (Prometheus, Alertmanager, Loki,
  Kubernetes REST), `infra.fix` (только rollout restart, два шага),
  `infra.report` (Telegram с фиксированным получателем) и `bb.read`/`bb.act`
  (треды и запуск агентов bb, только личный профиль). Он же — точка входа
  голосового маршрута `/v1/voice/*`. Решение — `DECISIONS.md`;
  деплой-контракт — `projects/home/kubernetes/mcp-gateway/`.
- `infra_report` — полный отчёт по всем сервисам каталога: статус, готовые
  реплики, перезапуски за час, digest-образ, возраст, внешние проверки, версии
  HA и алерты; короткий вердикт + plain-text ≤ 3500 символов. `infra_status`
  тоже выполняет внешние проверки.
- Внешние проверки каталога (`checks:`): `tcp`, `http` (`auth: ha`, тело не
  возвращается), `ha_state`, `ha_template`, `ha_unavailable_summary`. HA URL и
  токен — `MCP_HA_URL`/`MCP_HA_TOKEN_FILE` из Secret `mcp-gateway-ha` (та же
  OpenBao-запись `k8s/home-assistant/ha-bridge-ha`, что у ha-bridge); если не
  заданы — «не настроено», не ошибка. Адреса HAOS/станции — плейсхолдеры
  `${HAOS_IP}`/`${STATION_IP}`, рендерятся из `secrets/home/mcp-gateway.env`.
- Namespace `home-assistant`, Deployment `mcp-gateway` (1 реплика,
  RollingUpdate, distroless, non-root, read-only rootfs), ClusterIP
  `mcp-gateway:8080` без ingress/NodePort; `/mcp` требует Bearer, `/healthz`,
  `/readyz`, `/metrics` — только внутри кластера; `ServiceMonitor` скрейпит
  `/metrics`.
- Каталог сервисов — ConfigMap `mcp-gateway-catalog` из
  `manifests/catalog.yaml`: spoken-алиасы ru/en, namespace/workload, PromQL
  сигналы (пустой результат = норма, непустой = проблема), источник логов
  (Loki там, где Alloy шипит namespace, иначе `pods/log`), предупреждение и
  `self_disrupting` для перезапуска.
- RBAC — ServiceAccount `home-assistant/mcp-gateway`, без ClusterRole:
  чтение pods/pods-log/events/services и четырёх видов workload в
  `home-assistant`, `monitoring`, `currency-bot`, `openbao`,
  `external-secrets`, `traefik`, `cert-manager`, `kube-system`; `patch`
  только на deployments/statefulsets/daemonsets с `resourceNames`, равными
  каталогу. `kube-system` — только чтение.
- NetworkPolicy: ingress на 8080 от адреса узла (Hermes в `hostNetwork`),
  LAN/pod CIDR и namespace `monitoring`; egress — kube-API 6443, monitoring
  9090/9093/3100, LAN-порты проверок (HAOS 8123/8095, станция
  6053/8928/8099), CT 103 `:22` только, DNS 53 и Telegram 443.
- **Мост в bb.** Кластер не открывает API bb: шлюз идёт по SSH с отдельным
  ограниченным ed25519-ключом и pinned `known_hosts` (алгоритмы хоста — из
  файла) к CT 103, пользователь `bb`, forced command `/usr/local/bin/bb-dispatch`.
  Мост принимает один JSON ≤ 16 KiB (`targets`, `status`, `validate_spawn`,
  `spawn`, `send`, `turn_result`, `updates`), подписывает тело HMAC-SHA256 и
  вызывает loopback-маршрут плагина `home-dispatcher` с токеном плагина. Токен и
  HMAC-секрет есть только на CT; шлюз знает только свой SSH-ключ. Любая
  другая команда по ключу тоже запускает мост, а не shell. Контракт полей
  (`contract/params.json`) общий у плагина и шлюза и проверяется тестами с
  обеих сторон.
- **Голосовые цели и маршруты.** `bb-targets.yaml` — allowlist личных
  проектов со spoken-алиасами (`BB_PROJECT_*` в private env); рабочие проекты
  за его пределами. `/v1/voice/utterance` принимает `{text, profile}` и
  отдаёт `{route}`: `local` (фраза выхода из режима агента), `bb` (рабочий
  профиль — постоянный тред `BB_DISPATCHER_THREAD_ID`; личный в режиме
  агента — активный тред), `hermes` (иначе). `/v1/voice/result` — long-poll
  `turn_result` для позднего ответа, `/v1/voice/updates` — завершённые ходы
  диспетчера с `initiator` `system`/`agent` (`spoken` + `completedAt`) для
  озвучивания (ходы владельца не возвращаются), `/v1/voice/state` — отладочное
  состояние режима.
- **Очередь озвучивания.** Поздние ответы агентов не звучат из потока
  запроса: voice-proxy (`solutions/voice-proxy`) ставит `EnqueueTask`
  `voice.announce` в durable очередь
  `voice-work`/`voice-personal` (idempotency key `voice:<thread>:<seq>`), а
  один consumer `voice-proxy-home` арендует событие только когда спутник
  `idle`, а media player не `announcing`. Ответы старше
  `HERMES_VOICE_ANNOUNCE_TTL` (3 ч) подтверждаются без звука; ожидавшие >2 мин
  получают префикс «Пока вас не было: ». Ошибка `EnqueueTask` откатывается на
  прямой announce. `HERMES_VOICE_QUEUE=0` выключает очередь. Дубли после
  рестарта отсекает idempotency key; TTL хранения ключей scheduler — 30 дней,
  больше TTL ответов.
- Секреты — ESO: `mcp-gateway-auth` (OpenBao
  `k8s/home-assistant/mcp-gateway-auth`, ключ `token`, тот же токен Hermes
  получает как `HERMES_INFRA_TOKEN`), `mcp-gateway-telegram`
  (`TELEGRAM_BOT_TOKEN` + первый элемент `TELEGRAM_ALLOWED_USERS` из
  `k8s/home-assistant/hermes-environment`), `mcp-gateway-ha` (HA REST-токен из
  `k8s/home-assistant/ha-bridge-ha`, ключ `token`), `mcp-gateway-bb-ssh`
  (`id_ed25519` + `known_hosts` из `k8s/home-assistant/mcp-gateway-bb-ssh`) и
  `mcp-gateway-voice` (`token`, отдельный Bearer для `/v1/voice/*`; не
  принимается на `/mcp`, MCP-токен не принимается на голосовых маршрутах).
  Telegram — отдельный инфра-бот; бот currency-bot остаётся только для
  currency-bot.
- Перезапуск разрешён для всех repo-managed workload'ов после голосового или
  Telegram-подтверждения, у каждого — честное предупреждение; `hermes` и
  `mcp-gateway` помечены `self_disrupting` (патч откладывается на ~20 с, чтобы
  ответ успели озвучить). HAOS, Music Assistant и станция — только чтение.
- CI: юниты `mcp-gateway` (bazel: crate fmt/clippy/тесты + musl-образ +
  deploy, env `mcp_gateway_env` в `secret/ci/deploy-home`) и
  `mcp-gateway-deploy` (catalog↔RBAC, манифесты, render, contract-тест
  `deploy.sh`); схема каталога и bb-allowlist проверяется настоящими
  Rust-валидаторами в
  `crates/mcp-gateway/tests/deploy_contract.rs`.
- Hermes: третий MCP-сервер `infra`
  (`mcp-gateway.home-assistant.svc.cluster.local:8080/mcp`, Bearer
  `HERMES_INFRA_TOKEN`); `platform_toolsets.api_server` и `telegram` =
  home/music/web/mcp-infra, без terminal/file/browser. Telegram-канал Hermes —
  тот же инфра-бот, long polling, allow-list владельца. Контекст —
  `hermes/HERMES.md` → `/opt/data/.hermes.md`; overlay на PVC ставит
  идемпотентный `home-deploy hermes render-config` (реальные endpoint'ы — из
  git-ignored `secrets/home/hermes-private.yaml`, `--private-values`;
  `agent.personalities.voice` рендерится из `voice_prompt.txt`).
  Telegram-промпт — `telegram_prompt.txt` в
  `platforms.telegram.channel_overrides[<owner chat id>].system_prompt`
  (иначе `display.personality: voice` режет ответы до 180 символов);
  `telegram_owner_chat_id` — renderer-only ключ, он же задаёт
  `platforms.telegram.home_channel`. Skill `skills/report` даёт команду
  `/report` → `infra_report`, виден в меню (`command_menu`), отключён на
  `api_server`.
- Алерты идут инфра-ботом (`alertmanager-telegram`); ответ на алерт в
  Telegram передаёт отпечаток Hermes для `infra_alert`.
- Статус: раскатано и принято 2026-11-01: статус и `/report` голосом и в
  Telegram, перезапуск с подтверждением и cooldown, ответ на алерт. Группы
  `bb.read`/`bb.act`, SSH-мост и голосовые маршруты раскатаны 2026-11-02/03;
  живая приёмка рабочего профиля через колонку ещё не пройдена
  (`PENDING.md`).

## Приватное / backups

- `secrets/NETWORK.md` — реальные IP/порты (из OpenBao `secret/infra/docs/network`).
- Доступы, kubeconfig, private values, Grafana password — OpenBao
  (`docs/SECRETS.md`), рабочее место — `docs/WORKSTATION.md`. Полные бэкапы хоста/VM на Mac не хранятся: их покрывают vzdump и
  offsite (ниже).
- HA automatic encrypted backup включён: ежедневно 03:30, 3 копии, ~45 MB,
  агент `hassio.local`.
- Offsite backup live: K3s timer 04:45, restic → личный Google Drive
  (`rclone:gdrive:home-offsite-v2`, scope `drive.file`, client-side encryption),
  ~45 MB/день. Внутри: OpenBao raft snapshot, SQLite scheduler/currency-bot,
  ledger ha-bridge, автоматический backup HA, `bb server export` с CT 103
  (с 2026-11-03; staging ~375 MB, в репозитории после restic ~70 MB). Ключ `openbao-seal`, recovery
  keys и restic password в архив не входят; restic password и ключ HA
  backup сохранены владельцем в password manager (2026-11-02). Restore из Drive проверен
  2026-11-02. OAuth — собственный client «Desktop app» в Google Cloud
  project `home-offsite-backup` (app `home-offsite`, In production, без
  verification); репозиторий `rclone:gdrive:home-offsite-v2`.
- Метрики/алерты бэкапов: PVE `prometheus-node-exporter` (+ vzdump/SMART/NVMe
  textfile), scrape job `pve-node`; K3s node-exporter textfile; PrometheusRule
  `home-backup-alerts` (vzdump > 30 ч, offsite failed/stale, HA backup > 36 ч,
  PVE exporter down) → Telegram.

## Ограничения

- Один K3s node, без HA control plane и распределённого storage.
- Публичного HA/Grafana нет; доступ только LAN/VPN. Единственный публичный
  путь — Telegram webhook `tg.example.com` через Cloudflare Tunnel.
- Offsite backup и внешний dead-man switch live; проверенного disaster
  restore с чистого железа пока нет.
- Станция — единственный экземпляр; factory/recovery image отсутствует.
