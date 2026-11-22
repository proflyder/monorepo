# ARCHITECTURE — устройство стенда

Текущие факты и версии — `STATE.md`; сеть/порты — `NETWORK.md`; секреты —
`SECRETS.md`; решения — `DECISIONS.md`; поток веток — `GIT.md`.

## Компоненты

```text
Интернет
  ├─ VPN 443/tcp+udp ──► CT 102 vpn (ocserv): группы lan / full / ci
  │      lan: 10.77.0.0/24 → домашняя LAN, DNS 192.0.2.1
  ├─ VPN 51820/udp ───► CT 102 vpn (WireGuard wg0): 10.77.2.0/24, как full;
  │      peer'ы выдаёт scheduler (/v1/vpn/profile), CT тянет /v1/vpn/peers
  └─ Cloudflare Tunnel ─► K3s cloudflared
         публично /telegram/webhook → currency-bot:8081
         и /repository/* → Nexus (read-only CI cache)
  │
  ▼
роутер/DNS 192.0.2.1
  │ LAN 192.0.2.0/24
  ├─ Proxmox PVE .26 — Ethernet / Wi-Fi 4addr uplink
  │    ├─ HAOS VM 100 .28 — HA + MA + Matter add-ons
  │    ├─ K3s VM 101 .27 — Traefik, OpenBao, ESO, STT/TTS,
  │    │                    Hermes, scheduler, currency-bot, observability
  │    └─ CT 103 bb .30 — сервер bb, SQLite, плагины profiles/home-dispatcher
  ├─ Станция Midi .13 — Pulsar, ESPHome, Sendspin
  └─ Mac .11 — DHCP, рабочая машина (треды bb, рабочий диспетчер)
```

- **PVE** — гипервизор; VM получают L2 через `vmbr0`.
- **HAOS** — единственный HA runtime; MA/Matter — add-ons, не K3s.
- **K3s** — приложения реального времени, scheduler/ha-bridge/ha-control/
  ha-scenario, intercom, currency-bot, Nexus, OpenBao/ESO и наблюдаемость.
- **VPN CT** — основная точка входа снаружи; через Cloudflare Tunnel публично
  открыты только webhook currency-bot и read-only `/repository/*` Nexus.
- **Станция** — ESPHome-спутник HA и Sendspin-плеер; `maind` остановлен.
- **Scheduler** — единственный владелец расписаний/таймеров; HA bridge
  исполняет очередь `home`, ha-control — каталог Home и очередь `control`.
- **Currency bot** — Rust-сервис курсов: SQLite на PVC, алерты в Telegram,
  webhook через Cloudflare Tunnel, внутренний JSON API
  (`solutions/currency-bot/README.md`).
- **Intercom** — Rust-сигналинг WebRTC-звонка телефон ↔ колонка: SDP/ICE
  relay, server-streaming `Watch`; внутреннее имя
  `intercom.home.<home-suffix>` через Traefik, медиа идёт P2P.
- **Nexus** — кэш артефактов CI: Maven/Google Maven/PyPI/npm через свои
  прокси, read-only публичный путь `/repository/*` через Cloudflare Tunnel.
- **MCP gateway** — единый MCP-сервер для инфраструктуры: чтение
  Prometheus/Alertmanager/Loki/Kubernetes и подтверждаемый rollout restart.
  Hermes сам не имеет доступа к Kubernetes API; весь доступ идёт через шлюз
  (ClusterIP, Bearer, без ingress). Там же группы `bb.read`/`bb.act` для
  личных тредов и голосовой маршрут `/v1/voice/*`; до bb шлюз доходит только
  SSH forced command'ом до CT 103.
- **bb server (CT 103)** — сервер bb и его SQLite; входящих портов не
  публикует. Домашние сервисы трогают bb только через плагины `profiles`
  (изоляция work/personal и allowlist провайдеров/машин) и `home-dispatcher`
  (шесть методов для голоса и Hermes). Треды и агенты выполняются на машинах
  bb: рабочий диспетчер — Claude Code на рабочем Mac.
- **Mac** — управление, тесты, recovery; рабочая машина bb.

## Потоки

### Голос

1. Pulsar: VQE + on-device wake «Рикардо» → ESPHome `:6053`.
2. HA pipeline: GigaAM v3 primary / Whisper fallback.
3. Локальные intents (свет, музыка, таймеры, будильники) — в HA и canonical
   SchedulerService; HA остаётся исполнителем звука.
4. Свободная речь → `voice-proxy` (Rust, `solutions/voice-proxy`): сначала
   точный cancel guard, затем
   профиль из `input_select.home_voice_profile` и
   `mcp-gateway /v1/voice/utterance`:

   ```text
   профиль не прочитан            → «Не могу подтвердить профиль.»
   work                           → bb home-dispatcher → постоянный тред-диспетчер
   personal + режим агента        → bb home-dispatcher → активный тред
   иначе                          → Hermes → Piper Puriso
   ```

   В рабочем профиле откат в Hermes запрещён: ошибка остаётся на маршруте
   `bb` с короткой фразой. Поздний ответ треда озвучивается
   `script.schedule_announce`.
5. Инфраструктурные вопросы («проверь сервисы», «почему не работает X»)
   Hermes решает через `mcp-gateway` (MCP Streamable HTTP, Bearer): только
   чтение, а перезапуск — лишь после голосового/Telegram-подтверждения, одним
   двухшаговым действием. Запуск агента — `bb_prepare_spawn` → подтверждение →
   `bb_confirm`, только личный профиль.
6. TTS/announce/alarm → Pulsar; Pulsar не владеет alarm/timer state.

### Музыка

- MA в HAOS (add-on); Spotify Connect.
- Sendspin-плеер «Яндекс Станция Миди»; source router
  `Music Assistant` / `Spotify Connect`.
- Voice play/volume идут через HA scripts; Connect — через Spotify Web API.

### Сеть и HTTPS

- PVE `home-uplink`: ровно один physical uplink — Ethernet при carrier,
  иначе Wi-Fi 4-address/WDS. Оба одновременно в мост не подключать.
- VM сохраняют `.27/.28`, L2, ARP/mDNS/IPv6; NAT и PCI passthrough нет.
- Traefik в `hostNetwork` (`80/443`) публикует `ha`, `grafana`, `alarms`,
  `control`, `currency`, `intercom`, `vault`, `passwords` и `nexus` как
  `https://<name>.home.example.com`; сертификат — wildcard Let's Encrypt от
  cert-manager (ACME DNS-01 Cloudflare).
- Трафик снаружи приходит через VPN; исключение — Cloudflare Tunnel
  `home-k3s`, который публикует `tg.example.com` путь `^/telegram/webhook$`
  на webhook Service и `ci-nexus.<public-zone>` путь `^/repository/` на
  Nexus (остальное — 404). Allow-list
  Traefik пускает LAN и pod CIDR, VPN-клиенты приходят NAT-ом с адреса CT.
- Станция: egress guard; публичный WAN и Яндекс закрыты, разрешён только
  `tcp/443` к публичным IPv4-диапазонам GitHub для updater'а.

### Секреты

```text
GitHub Actions ──OIDC──► OpenBao jwt роли deploy-station (Environment
                              │  Station), android-distribute / android-release /
                              │  android-cleanup (Environment Android)
                              │  secret/ci/deploy-station, secret/ci/android-distribute
                              ▼
                         VPN группа ci → Traefik 443 → HA/OpenBao

OpenBao KV v2 secret/k8s/* ──► ESO ClusterSecretStore/openbao
                              │  Kubernetes auth
                              ▼
                     Secrets приложений в namespace
```

- **OpenBao** — единственный источник правды для секретов: raft на локальном
  PV, статический auto-unseal, audit в stdout → Loki, UI `vault`.
- **External Secrets Operator** синхронизирует `secret/k8s/*` в Kubernetes
  Secrets приложений; deploy-скрипты их не пишут. Пути — `SECRETS.md`.
- Входящего CI-доступа в K3s нет. Приватные
  env приложений лежат в `secret/k8s/flux-system/app-settings`, values Hermes —
  в `secret/k8s/flux-system/runtime-values`; ESO создаёт из них
  `flux-app-settings`/`runtime-values`, и Flux подставляет их при применении.

### CI/CD

```text
ветка <type>/<name> → PR → ci.yml (обёртка) → _ci.yml (units.mjs, reusable) → ci-ok
   → squash merge в develop
   → deploy-home.yml (push, обёртка) → _deploy-home.yml plan (units.mjs)
        → pins job: verified digest → машинная ветка deploy/home-apps
             → Flux: ConfigMap/home-app-images → применяет workload из develop
        → deploy-station: Pulsar через HA (Environment Station)
```

- Все приложения (scheduler/executors, wakegrant, intercom, currency-bot,
  mcp-gateway, hermes) применяет Flux из кластера: CI не входит в VPN, не
  читает OpenBao и не вызывает kubectl/helm. Откат — pins revert (или manifest
  revert), не `kubectl rollout undo`; аварийная заморозка — `flux suspend`.
- `deploy-home` содержит только `plan`, `pins`, `warm-*`, `native-release` и
  `deploy-station` (pins и станция — общие reusable `_pin.yml` /
  `_station-deploy.yml`, их же использует ручной `deploy-services.yml`).
  VPN-группа `ci` (только `tcp/443` до K3s) нужна `deploy-station`
  (`ci-pulsar`) и Android-джобам OIDC (`ci-android`).
- Прямой push в `develop` запрещён; merge PR = деплой затронутых юнитов.
- CI-скачивания Maven/Google Maven/PyPI/npm у Bazel идут напрямую с раннеров,
  а Nexus — запасной путь (второе rewrite-правило) при 429/5xx/обрыве; pip/npm
  шаги используют Nexus, когда заданы `NEXUS_HOST` и `NEXUS_CI_PASSWORD`.
  Nexus, платформу и наблюдаемость разворачивает Flux из Git; вручную
  остаются только OpenBao, host-firewall (nftables) и Cloudflare DNS.
- Сервисные образы собираются Bazel `rules_oci` (distroless) и адресуются по
  digest в GHCR; `hermes` собирается Docker из upstream-ревизии, юнит имеет
  builder `mixed`. `home-deploy converge` — только admin-инструмент (drills,
  cutover).
- Режимы исполнения (`execute`, `shadow`, `control-*`) — Git-состояние в
  `scheduler/manifests/kustomization.yaml`; учения — `flux suspend` плюс
  admin-скрипт `scheduler/deploy.sh`, затем `flux resume`.

Pulsar живёт в монорепозитории (`projects/station/pulsar/`). Релиз — любое
смерженное изменение `projects/station/pulsar/**`; сборка и публикация — в
монорепо, без GitHub Releases и тегов. Поток:

```text
PR CI (_native.yml, promote: false): сборка aarch64/armhf с вычисленной
   версией (platform/ci/version.mjs), radio,
   SHA256SUMS, проверка --version под qemu → publish-oci.sh → oras push
   ghcr.io/proflyder/pulsar-firmware:tree-<hash> (+ provenance)
merge PR в develop, затрагивающего Pulsar
   → deploy-home plan: юнит pulsar → native_units
   → native-release: проверка tree-<hash>/provenance, теги <version> и :latest
        на тот же digest (повторной сборки нет)
   → deploy-station: ручной approve в Environment `Station`
        → VPN группа ci
        → OIDC в OpenBao: роль deploy-station → secret/ci/deploy-station
        → HA update_entity + update.install (Firmware станции)
        → проверка installed_version == version и 90 с стабильности
```

- Версия вычисляется CI: база `VERSION` (`v0.6`) + число коммитов Pulsar
  (решение 60); updater ставит только строго большую `vMAJOR.MINOR.PATCH`.
- Откат через CI невозможен (updater только повышает версию): на станции
  `.prev` + `ctl.sh reload`.
- CI-юнит `pulsar-bazel` (`.github/workflows/_unit.yml`) выполняет fmt, clippy и
  тесты; юнит `pulsar` (`.github/workflows/_native.yml`) — Bazel-кросс-сборку
  обеих архитектур, installer, radio и ssh.

## Данные

- Расписания/таймеры — SQLite scheduler на PVC; каталог Home — live HA.
- История курсов и `sent_alerts` — SQLite currency-bot на PVC
  `currency-bot-data`; логи — stdout → Alloy → Loki.
- HA state — HAOS Supervisor backup.
- K3s data/PV — `/srv/k3s/runtime`, `/srv/k3s/volumes/*`; `Retain`.
- OpenBao — raft/backups на отдельных PV, audit — stdout → Loki; живой seal
  key — Secret `openbao-seal`, recovery keys — у владельца, не на диске.
- bb — SQLite и данные плагинов на CT 103 (`/home/bb/.bb-machines/<host>/`);
  восстанавливаемый артефакт — `bb server export` (`/var/backups/bb`).
- Секреты приложений — OpenBao → ESO.
- Recovery — `REBUILD.md`, `KUBERNETES-OPERATIONS.md`.

## Границы

- Один K3s node, без HA control plane/distributed storage.
- Публичного HA/Grafana/OpenBao нет; только LAN/VPN, кроме webhook
  currency-bot `tg.example.com/telegram/webhook` и read-only Nexus
  `/repository/*` через Cloudflare Tunnel.
- Offsite backup и внешний dead-man switch live; проверенного disaster
  restore с чистого железа пока нет.
- API bb не публикуется: дом ходит в него только плагином + SSH forced
  command; профиль голоса переключается вручную и не выводится из речи.
- Станция — единственный экземпляр; factory/recovery image отсутствует.
