# INVENTORY — неттоп

Текущий runtime: Proxmox VE `9.2.24` на DEXP Jupiter JU01CY. HAOS и K3s —
отдельные VM; Docker/Compose retired. Факты — `../../docs/STATE.md`.

## Железо

| Параметр | Значение |
|---|---|
| CPU | Intel Core i7-12650H, 10C/16T |
| RAM | 16 GiB DDR4, 1 модуль |
| SSD | NVMe 476.9 GB |
| Сеть | 1 GbE + Wi-Fi RTL8821CE 2.4/5 ГГц, BT 5.x |
| BIOS | UEFI, VT-x, Secure Boot |
| PSU | 120 W |

## VM

| VM | ОС | Ресурсы | IP |
|---|---|---|---|
| 100 `haos` | HAOS `2026.10.4` | 2 vCPU / 4 GiB / 32 GiB | `192.0.2.28` |
| 101 `lab-k3s` | Ubuntu 24.04.5 + K3s | 8 vCPU / 8 GiB / 200 GiB | `192.0.2.27` |

PVE host: `192.0.2.26`; uplink — Wi-Fi 4addr/WDS, Ethernet fallback.

## Сервисы

| Сервис | Где | Порт |
|---|---|---|
| Home Assistant | HAOS | `8123` |
| Music Assistant | HAOS add-on | `8095/8097/8927` |
| Matter Server | HAOS add-on | `127.0.0.1:5580` |
| GigaAM STT | K3s | `10305` |
| Whisper fallback | K3s | `10300` |
| Piper TTS | K3s | `10200` |
| Hermes gateway | K3s | `127.0.0.1:8642` |
| Hermes voice proxy | K3s | `8643` * |
| Grafana | K3s | `30300` |
| Pulsar/ESPHome | станция | `6053` |
| Sendspin | станция | `8928` |
| HTTP control | станция | `8099` * |
| ADB/SSH | станция | `5555/22` |

`*` — ограниченный доступ/bearer. Полная карта — `../../docs/NETWORK.md`.

## Данные

- HA config/DB — HAOS Supervisor.
- MA/Matter data — HAOS add-on data.
- K3s — `/srv/k3s/runtime`, `/srv/k3s/volumes/*`.
- Recovery/backups — `secrets/`, вне Git.
