# Proxmox / HAOS / K3s: установка и переустановка

Текущие факты — `STATE.md`. Реальные IP/MAC — `secrets/proxmox-install/install-settings.md`.
Приватная копия Wi-Fi-профиля и recovery — в `secrets/`.

## Текущая конфигурация

```text
PVE host         192.0.2.26   DEXP Jupiter, 16 GiB, NVMe 512 GB
                 vmbr0; Wi-Fi 4addr/WDS + Ethernet fallback
VM 100 haos      192.0.2.28   2 vCPU / 4 GiB / 32 GiB, HAOS 2026.10.4
                 add-ons: Music Assistant 2.10.8, Matter 9.2.0
VM 101 lab-k3s   192.0.2.27   8 vCPU / 8 GiB / 200 GiB, Ubuntu + K3s
CT 102 vpn       192.0.2.29   1 vCPU / 512 MiB / 4 GiB, Debian 13, ocserv
```

- PVE `9.2.24`, root 64 GiB, swap 8 GiB, `local-lvm` ~379 GiB.
- VM — UEFI, guest agent, startup order 1/2.
- Terraform — `projects/home/proxmox/terraform/`; state root-only на PVE,
  копия в `secrets/`. `terraform plan` — No changes.
- Wi-Fi uplink — `projects/home/proxmox/wifi-uplink/`; Ethernet fallback.

## Переустановка PVE

**Установщик стирает весь NVMe.** Сначала проверить backup и приватные
IP/MAC. Ethernet и локальная консоль — на время установки.

1. Записать локальный ISO `secrets/proxmox-install/proxmox-ve_9.2-1.iso`.
2. Boot UEFI; Install Proxmox VE; диск `nvme0n1`.
3. ext4: `maxroot=64`, `swapsize=8`, `minfree=16`; остальное — LVM-thin.
4. Country Kazakhstan, timezone `Asia/Almaty`, keyboard US.
5. Network: Ethernet + статический IP/gateway/DNS из приватной памятки;
   `vmbr0` создаётся установщиком.
6. После reboot — `https://<PVE_IP>:8006`, self-signed TLS.
7. Добавить отдельный SSH-ключ через `install-settings.md`/wrapper;
   сравнить host-key fingerprint с консолью.
8. Проверить PVE version, storage, bridge, time, updates.

## Terraform и VM

```bash
cd projects/home/proxmox/terraform
terraform init
terraform plan
terraform apply
```

Перед apply проверить приватные tfvars: VM ID, memory, disk, static CIDR,
gateway, MAC. Не переиспользовать старый config-drive instance ID без
проверки SSH host keys и сети.

После apply:

- HAOS — восстановить supported backup через Supervisor, проверить
  `internal_url`, trusted proxy, add-ons MA/Matter;
- K3s VM — восстановить data/PV/Helm по `KUBERNETES-OPERATIONS.md`;
- проверить `.28:8123`, `.27:30300/`, HTTPS ingress, voice/music.

## Wi-Fi uplink

- Wi-Fi остаётся на PVE; `ricardo-uplink` подключает либо Ethernet, либо
  Wi-Fi 4addr/WDS к `vmbr0`. VM IP/URL не меняются.
- Не подключать оба uplink в мост одновременно.
- При смене AP/драйвера повторить canary: отдельный MAC, DHCP, gateway,
  IPv6 multicast; обычный ping PVE не доказывает L2 для VM.
- Профиль wpa_supplicant — 0600, вне Git; installer:
  `projects/home/proxmox/wifi-uplink/install.py`.

## Acceptance

```bash
# PVE
pveversion
ip -br addr
bridge link

# HA/MA/Grafana
curl -fsS -o /dev/null -w '%{http_code}\n' http://192.0.2.28:8123/
curl -fsS -o /dev/null -w '%{http_code}\n' http://192.0.2.28:8095/
curl -fsS -o /dev/null -w '%{http_code}\n' \
  http://192.0.2.27:30300/api/health

# K3s
ssh labuser@192.0.2.27 'sudo k3s kubectl get pods -A'
```

Проверить после reboot: VM autostart, HA, K3s, Sendspin, Spotify, Pulsar,
HTTPS ingress. Полный disaster restore и power-loss тест — отдельная приёмка.
