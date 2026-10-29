# ARCHITECTURE — устройство стенда

Текущие факты — `STATE.md`; сеть/порты — `NETWORK.md`.

## Компоненты

```text
Интернет
  │
  └─ роутер/DNS 192.0.2.1
       │ LAN 192.0.2.0/24
       ├─ Proxmox PVE .26 — Ethernet / Wi-Fi 4addr uplink
       │    ├─ HAOS VM 100 .28 — HA + MA + Matter add-ons
       │    └─ K3s VM 101 .27 — STT/TTS, Hermes, Grafana
       ├─ Станция Midi .13 — Pulsar, ESPHome, Sendspin
       └─ Mac .11 — DHCP, рабочая машина
```

- **PVE** — гипервизор; VM получают L2 через `vmbr0`.
- **HAOS** — единственный HA runtime; MA/Matter — add-ons, не K3s.
- **K3s** — приложения реального времени, scheduler/ha-bridge/ha-control и
  наблюдаемость.
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

### Сеть

- PVE `ricardo-uplink`: ровно один physical uplink — Ethernet при carrier,
  иначе Wi-Fi 4-address/WDS. Оба одновременно в мост не подключать.
- VM сохраняют `.27/.28`, L2, ARP/mDNS/IPv6; NAT и PCI passthrough нет.
- Tailscale Operator в K3s публикует HA (`pl-home`), Grafana, scheduler
  (`pl-scheduler`) и ha-control (`pl-control`) без входящих портов.
- Станция: egress guard; публичный WAN и Яндекс закрыты.

## Данные

- Расписания/таймеры — SQLite scheduler на PVC; каталог Home — live HA.
- HA state — HAOS Supervisor backup.
- K3s data/PV — `/srv/k3s/runtime`, `/srv/k3s/volumes/*`; `Retain`.
- Секреты — Kubernetes Secret/private values/`secrets/`.
- Recovery — `REBUILD.md`, `KUBERNETES-OPERATIONS.md`.

## Границы

- Один K3s node, без HA control plane/distributed storage.
- Публичного HA/Grafana нет; только LAN/Tailscale.
- Matter devices пока не добавлены; LED strip — первый кандидат.
- Нет offsite backup, внешнего alerting и полного disaster restore.
- Станция — единственный экземпляр; factory/recovery image отсутствует.
