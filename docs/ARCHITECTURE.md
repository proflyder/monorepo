# ARCHITECTURE — устройство стенда

Текущие факты — `STATE.md`; сеть/порты — `NETWORK.md`; деплой — `GIT.md`.

## Компоненты

```text
Интернет
  ├─ VPN 443/tcp+udp ──► CT 102 vpn (ocserv): группы lan / full / ci
  │      lan: 10.77.0.0/24 → домашняя LAN, DNS 192.0.2.1
  └─ Cloudflare Tunnel ─► K3s cloudflared
         публичный только /telegram/webhook → currency-bot:8081
  │
  ▼
роутер/DNS 192.0.2.1
  │ LAN 192.0.2.0/24
  ├─ Proxmox PVE .26 — Ethernet / Wi-Fi 4addr uplink
  │    ├─ HAOS VM 100 .28 — HA + MA + Matter add-ons
  │    └─ K3s VM 101 .27 — Traefik, OpenBao, ESO, STT/TTS,
  │                         Hermes, scheduler, currency-bot, observability
  ├─ Станция Midi .13 — Pulsar, ESPHome, Sendspin
  └─ Mac .11 — DHCP, рабочая машина
```

- **PVE** — гипервизор; VM получают L2 через `vmbr0`.
- **HAOS** — единственный HA runtime; MA/Matter — add-ons, не K3s.
- **K3s** — приложения реального времени, scheduler/ha-bridge/ha-control,
  currency-bot, OpenBao/ESO и наблюдаемость.
- **VPN CT** — основная точка входа снаружи; публично открыт ещё только
  webhook currency-bot через Cloudflare Tunnel.
- **Станция** — ESPHome-спутник HA и Sendspin-плеер; `maind` остановлен.
- **Scheduler** — единственный владелец расписаний/таймеров; HA bridge
  исполняет очередь `home`, ha-control — каталог Home и очередь `control`.
- **Currency bot** — Go-сервис курсов: SQLite на PVC, алерты в Telegram,
  webhook через Cloudflare Tunnel, внутренний JSON API.
- **Mac** — управление, тесты, recovery.

## Потоки

### Голос

1. Pulsar: VQE + on-device wake «Рикардо» → ESPHome `:6053`.
2. HA pipeline: GigaAM v3 primary / Whisper fallback.
3. Локальные intents (свет, музыка, таймеры, будильники) — в HA и canonical
   SchedulerService; HA остаётся исполнителем звука.
4. Свободная речь → Hermes `:8643` → LLM → Piper Puriso.
5. TTS/announce/alarm → Pulsar; Pulsar не владеет alarm/timer state.

### Музыка

- MA `2.10.8` в HAOS; Spotify Soloist и Connect.
- Sendspin-плеер «Яндекс Станция Миди»; source router
  `Music Assistant` / `Spotify Connect`.
- Voice play/volume идут через HA scripts; Connect — через Spotify Web API.

### Currency bot

1. Часовой job (`:00` UTC) и `/trigger` парсят kurs.kz и пишут USD/RUB
   в SQLite на PVC; старые записи старше 35 дней удаляются weekly.
2. Алерты уходят в Telegram только при порогах HOUR/DAY/WEEK/MONTH;
   dedup — `sent_alerts`, сообщения не дублируются.
3. Telegram → Cloudflare Tunnel → `cloudflared` → `/telegram/webhook` на
   `currency-bot-webhook:8081`; проверяются secret token и allowlist чатов.
4. LAN/VPN → `https://currency.home.example.com/api/*` → Traefik →
   Service `:8080` (Bearer). `/metrics` скрейпит Prometheus.

### Сеть и HTTPS

- PVE `ricardo-uplink`: ровно один physical uplink — Ethernet при carrier,
  иначе Wi-Fi 4-address/WDS. Оба одновременно в мост не подключать.
- VM сохраняют `.27/.28`, L2, ARP/mDNS/IPv6; NAT и PCI passthrough нет.
- Traefik в `hostNetwork` (`80/443`) публикует `ha`, `grafana`, `alarms`,
  `control`, `vault` и `currency` как `https://<name>.home.example.com`;
  сертификат — wildcard Let's Encrypt от cert-manager (ACME DNS-01
  Cloudflare).
- Трафик снаружи приходит через VPN; единственное исключение — Cloudflare
  Tunnel `home-k3s`, который отдаёт публично только `tg.example.com` путь
  `^/telegram/webhook$` на webhook Service (остальное — 404). Allow-list
  Traefik пускает LAN и pod CIDR, VPN-клиенты приходят NAT-ом с адреса CT.
- Станция: egress guard; публичный WAN и Яндекс закрыты, разрешён только
  `tcp/443` к публичным IPv4-диапазонам GitHub для updater'а.

### Секреты

```text
GitHub Actions ──OIDC──► OpenBao jwt роль deploy-home
                              │  secret/ci/deploy-home
                              ▼
                         VPN группа ci → K3s 6443
                              │
OpenBao KV v2 secret/k8s/* ──► ESO ClusterSecretStore/openbao
                              │  Kubernetes auth
                              ▼
                     Secrets приложений в namespace
```

- **OpenBao** — единственный источник правды для секретов: raft на локальном
  PV, статический auto-unseal, файловый audit, UI `vault`.
- **External Secrets Operator** синхронизирует `secret/k8s/*` в Kubernetes
  Secrets (`scheduler-auth`, `ha-bridge-ha`, `hermes-environment`,
  `grafana-admin`, `cloudflare-api-token`, `currency-bot`,
  `cloudflared-tunnel`); deploy-скрипты их не пишут.
- В GitHub Environment `Home` остаются только VPN-креды и адрес/роль
  OpenBao; kubeconfig, scheduler env, `currency_bot_env` и runtime values
  выдаёт OpenBao после OIDC-входа.

### CI/CD

```text
ветка <type>/<name> → PR → CI (units.yml, reusable workflows) → ci-ok
   → squash merge в develop
   → deploy-home: затронутые юниты параллельно
        → VPN группа ci → OIDC в OpenBao → kubectl/helm
        → K3s API :6443, образы GHCR по digest
   → smoke (Hermes Assist/health) → при провале откат только применённого
```

- Прямой push в `develop` запрещён; merge PR = деплой затронутых юнитов.
- Образы scheduler/ha-bridge/ha-control/currency-bot собираются Bazel
  `rules_oci` (distroless) и адресуются по digest в GHCR; Hermes/voice-proxy —
  тоже digest-pinned GHCR.
- currency-bot — data-driven юнит `platform/ci/units.yml`: свой namespace,
  script, `externalsecrets`, `env_key`/`env_file`; contract-тест
  `currency-bot-deploy`. `converge.py` заменяет Deployment целиком и сохраняет
  env/режим/replicas; откат восстанавливает ровно тот объект, который применил
  job.
- Режимы исполнения (`execute`, `shadow`, `control-*`) меняются только
  ручным `workflow_dispatch`.

Pulsar живёт вне монорепо: релиз собирается и публикуется в форке (origin
вложенного checkout `projects/station/upstream/pulsar-rust`), монорепо его не
собирает и не деплоит. Поток:

```text
push тега v0.5.100N в форке
   → build.yml: сборка + GitHub release (latest)
   → deploy-station.yml: ручной approve в Environment `Station` (теги v*)
        → VPN группа ci (сертификат ci-pulsar)
        → OIDC в OpenBao: роль deploy-station → secret/ci/deploy-station
        → HA update_entity + update.install (Firmware станции)
        → проверка installed_version == tag и 90 с стабильности
```

- Тег `v<upstream major.minor>.<1000+N>` больше любого upstream-патча, поэтому
  updater (`vMAJOR.MINOR.PATCH`) видит релиз форка новее.
- Откат через CI невозможен (updater только повышает версию): на станции
  `.prev` + `ctl.sh reload`.

## Данные

- Расписания/таймеры — SQLite scheduler на PVC; каталог Home — live HA.
- История курсов и `sent_alerts` — SQLite currency-bot на PVC
  `currency-bot-data`; логи — stdout → Alloy → Loki.
- HA state — HAOS Supervisor backup.
- K3s data/PV — `/srv/k3s/runtime`, `/srv/k3s/volumes/*`; `Retain`.
- OpenBao — raft/audit/backups на отдельных PV; recovery keys и seal key —
  только `secrets/openbao/`.
- Секреты приложений — OpenBao → ESO; приватные копии в `secrets/` — backup.
- Recovery — `REBUILD.md`, `KUBERNETES-OPERATIONS.md`.

## Границы

- Один K3s node, без HA control plane/distributed storage.
- Публичного HA/Grafana/OpenBao нет; только LAN/VPN, кроме webhook
  currency-bot `tg.example.com/telegram/webhook` через Cloudflare Tunnel.
- Нет offsite backup, внешнего alerting и проверенного disaster restore.
- Станция — единственный экземпляр; factory/recovery image отсутствует.
