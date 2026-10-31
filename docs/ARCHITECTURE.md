# ARCHITECTURE — устройство стенда

Текущие факты — `STATE.md`; сеть/порты — `NETWORK.md`; деплой — `GIT.md`.

## Компоненты

```text
Интернет
  │  единственный проброс: VPN 443/tcp+udp
  ▼
CT 102 vpn (ocserv)                    группы lan / full / ci
  │  lan: 10.77.0.0/24 → домашняя LAN, DNS 192.0.2.1
  ▼
роутер/DNS 192.0.2.1
  │ LAN 192.0.2.0/24
  ├─ Proxmox PVE .26 — Ethernet / Wi-Fi 4addr uplink
  │    ├─ HAOS VM 100 .28 — HA + MA + Matter add-ons
  │    └─ K3s VM 101 .27 — Traefik, OpenBao, ESO, STT/TTS,
  │                         Hermes, scheduler, observability
  ├─ Станция Midi .13 — Pulsar, ESPHome, Sendspin
  └─ Mac .11 — DHCP, рабочая машина
```

- **PVE** — гипервизор; VM получают L2 через `vmbr0`.
- **HAOS** — единственный HA runtime; MA/Matter — add-ons, не K3s.
- **K3s** — приложения реального времени, scheduler/ha-bridge/ha-control,
  OpenBao/ESO и наблюдаемость.
- **VPN CT** — единственная точка входа снаружи; публичных сервисов нет.
- **Станция** — ESPHome-спутник HA и Sendspin-плеер; `maind` остановлен.
- **Scheduler** — единственный владелец расписаний/таймеров; HA bridge
  исполняет очередь `home`, ha-control — каталог Home и очередь `control`.
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

### Сеть и HTTPS

- PVE `ricardo-uplink`: ровно один physical uplink — Ethernet при carrier,
  иначе Wi-Fi 4-address/WDS. Оба одновременно в мост не подключать.
- VM сохраняют `.27/.28`, L2, ARP/mDNS/IPv6; NAT и PCI passthrough нет.
- Traefik в `hostNetwork` (`80/443`) публикует `ha`, `grafana`, `alarms`,
  `control` и `vault` как `https://<name>.home.example.com`; сертификат —
  wildcard Let's Encrypt от cert-manager (ACME DNS-01 Cloudflare).
- Трафик снаружи приходит только через VPN; allow-list пускает LAN и pod
  CIDR, VPN-клиенты приходят NAT-ом с адреса CT.
- Станция: egress guard; публичный WAN и Яндекс закрыты.

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
  `grafana-admin`, `cloudflare-api-token`); deploy-скрипты их не пишут.
- В GitHub Environment `Home` остаются только VPN-креды и адрес/роль
  OpenBao; kubeconfig, scheduler env и runtime values выдаёт OpenBao после
  OIDC-входа.

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
- Образы scheduler/ha-bridge/ha-control собираются Bazel `rules_oci`
  (distroless) и адресуются по digest в GHCR; Hermes/voice-proxy — тоже
  digest-pinned GHCR.
- `converge.py` заменяет Deployment целиком и сохраняет env/режим/replicas;
  откат восстанавливает ровно тот объект, который применил job.
- Режимы исполнения (`execute`, `shadow`, `control-*`) меняются только
  ручным `workflow_dispatch`.

## Данные

- Расписания/таймеры — SQLite scheduler на PVC; каталог Home — live HA.
- HA state — HAOS Supervisor backup.
- K3s data/PV — `/srv/k3s/runtime`, `/srv/k3s/volumes/*`; `Retain`.
- OpenBao — raft/audit/backups на отдельных PV; recovery keys и seal key —
  только `secrets/openbao/`.
- Секреты приложений — OpenBao → ESO; приватные копии в `secrets/` — backup.
- Recovery — `REBUILD.md`, `KUBERNETES-OPERATIONS.md`.

## Границы

- Один K3s node, без HA control plane/distributed storage.
- Публичного HA/Grafana/OpenBao нет; только LAN/VPN.
- Нет offsite backup, внешнего alerting и проверенного disaster restore.
- Станция — единственный экземпляр; factory/recovery image отсутствует.
