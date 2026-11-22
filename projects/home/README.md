# projects/home

Композиция домашнего проекта поверх двух независимых проектов
(`projects/station` — Pulsar/ADB/BLE, `projects/home-assistant` —
HA/MA/STT/TTS/Hermes) и домашних solutions (`solutions/scheduler`,
`solutions/ha-bridge`, `solutions/intercom`, `solutions/mcp-gateway` и др.).
Станцию и HA можно обновлять независимо; project-specific конфигурация не
попадает в solutions. Архитектура — `docs/ARCHITECTURE.md`.

| Каталог | Что |
|---|---|
| [kubernetes/](kubernetes/README.md) | K3s: Flux, OpenBao, ingress, юниты приложений |
| [proxmox/](proxmox/README.md) | хост Proxmox, VM/CT, VPN, bb server, Wi-Fi uplink |
| [backup/offsite/](backup/offsite/README.md) | offsite-бэкап (restic → Google Drive) |
| `backup/pve/` | метрики свежести vzdump на хосте Proxmox (`install.sh` с Mac) |
| [cloudflare/heartbeat/](cloudflare/heartbeat/README.md) | внешний dead man's switch для Alertmanager |
| [windows-pc/](windows-pc/README.md) | Windows-десктоп «комп»: WoL, OpenSSH forced command |
| `e2e/` | локальный E2E scheduler + ha-bridge |

## E2E

`projects/home/e2e/run-scheduler-smoke.sh` собирает реальные scheduler и
ha-bridge во временный каталог и гоняет их против fake HA на loopback; живые
HA/ADB и деплойная конфигурация (`*_TOKEN_FILE`, пути БД, очереди) не
используются.
