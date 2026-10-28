# NETTOP — домашний сервер

Текущий runtime — Proxmox VE на DEXP Jupiter JU01CY.

## Быстрые факты

| Параметр | Значение |
|---|---|
| Host | PVE `9.2.24`, `pve`, 16 GiB RAM, NVMe 512 GB |
| Uplink | Wi-Fi RTL8821CE 4addr/WDS; Ethernet fallback |
| VM 100 | HAOS `2026.10.4`, 2/4 GiB/32 GiB, `.28` |
| VM 101 | Ubuntu 24.04.5 + K3s `v1.35.9`, 8/8 GiB/200 GiB, `.27` |
| HA | `http://192.0.2.28:8123` |
| MA/Matter | HAOS add-ons; MA `.28:8095`, Matter loopback `5580` |
| Remote | Tailscale Operator: `pl-home` → HAOS, `/grafana` → Grafana |

## Что где

- `projects/home/proxmox/` — PVE/VM/Terraform/Wi-Fi uplink.
- `projects/home/kubernetes/` — K3s, Helm, firewall, Tailscale.
- `projects/home-assistant/` — HA config, voice, Hermes, MA/TTS.
- `projects/station/` — Pulsar, ADB/BLE/provisioning, станция.
- `docs/` — эксплуатация; `secrets/` — реальные адреса/доступы.

## Текущие документы

- [`../../docs/STATE.md`](../../docs/STATE.md)
- [`../../docs/OPERATIONS.md`](../../docs/OPERATIONS.md)
- [`../../docs/KUBERNETES-OPERATIONS.md`](../../docs/KUBERNETES-OPERATIONS.md)
- [`../../docs/NETWORK.md`](../../docs/NETWORK.md)
- [`../../docs/REBUILD.md`](../../docs/REBUILD.md)

Legacy HA Container/Compose больше не используются; данные MA/STT/TTS
перенесены в HAOS/K3s, старые копии — только recovery.
