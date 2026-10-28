# NETTOP — домашний сервер HA

**Текущий runtime 2026-10-28:** Proxmox VE; HAOS VM `100` (`.28`) и
Ubuntu/K3s VM `101` (`.27`). HA Core, MA и Matter add-ons — в HAOS;
STT/TTS/Hermes — в K3s; вход — Tailscale Operator. Ethernet host `.26`; Wi-Fi пока DOWN.
Текущие команды — [Kubernetes runbook](../../docs/KUBERNETES-OPERATIONS.md)
и [Proxmox runbook](../../docs/INSTALL-PROXMOX.md).

Сведения о прежнем bare-metal Ubuntu/HA Container ниже — **история установки**,
не текущий writer и не актуальные адреса.

Отдельная папка по мини-ПК **DEXP Jupiter JU01CY**: воспроизводимый сетап
Proxmox + HAOS + Ubuntu/K3s + Home Assistant.

**Текущий статус:** HAOS `2026.10.4` healthy; Music Assistant `2.10.8` с
Spotify в K3s; HA Assist base (Whisper/Piper/`Рикардо`) и голосовые команды
сохранены. Pulsar `v0.4.1-ricardo6` подключён; HA integration Music
Assistant добавлена. Tailscale Operator `1.104.2` публикует `pl-home` →
HAOS и `/grafana` → Grafana; read-only E2E pass. Wi-Fi/routed-проект,
DHCP reservations, power-loss reboot и offsite backup — открыты.

## Быстрые факты (текущий runtime)

| Параметр | Значение |
|---|---|
| Proxmox host | PVE `9.2.24`, DNS `pve.home.arpa`, management `vmbr0` `.26` |
| HAOS VM 100 | HAOS `2026.10.4`, 2 vCPU / 4 GiB / 32 GiB, `.28`; Matter/MA add-ons |
| K3s VM 101 | Ubuntu 24.04.5, 8 vCPU / 8 GiB / 200 GiB, `.27` |
| K3s | `v1.35.9+k3s1`, node `lab-host`; voice/Hermes/observability |
| HA endpoint | HAOS `.28:8123` (direct) |
| Remote | Tailscale Operator `1.104.2`; `pl-home` → HAOS, `/grafana` |
| Terraform | provider `bpg/proxmox` 0.118.1; state root-only на PVE |
| Wi-Fi | RTL8821CE host DOWN; PCI mapping prepared, not attached |

## Историческая схема (не текущий runtime)

| Параметр | Значение |
|---|---|
| Железо | DEXP Jupiter JU01CY, i7-12650H, 16 ГБ DDR4, 512 ГБ NVMe |
| Hostname | `lab-host` |
| OS | Ubuntu Server 26.04.1 LTS, kernel `7.0.0-38-generic` |
| Пользователь | `labuser`; shell `zsh` + Oh My Zsh (sudo NOPASSWD; см. ниже) |
| Ethernet | `enp3s0` — `192.0.2.26/24`, metric 100 |
| Wi-Fi | `wlp4s0` — `192.0.2.25/24`, SSID `Stanislav1_Wi-Fi5`, 5 ГГц, metric 200 |
| Gateway/DNS | `192.0.2.1` |
| HA | `http://192.0.2.26:8123` / `http://192.0.2.25:8123`, версия `2026.10.4` |
| Music Assistant | `http://192.0.2.26:8095` / `.25:8095`, `2.10.8`, Spotify playback проверен |
| STT | `wyoming-onnx-stt` `:10305` GigaAM v3 int8 (primary); Whisper `medium-int8` `:10300` fallback |
| Piper | `wyoming-piper:2.5.2`, `:10200`, `ru_puriso2-medium` |
| Assist | pipeline `Рикардо`: local intents → `conversation.hermes_agent`; Pulsar satellite; MA-плеер |
| Hermes | `v0.22.5`, `~/.hermes`, `hermes-gateway.service`, API `127.0.0.1:8642`, HA MCP |
| Docker | 29.1.3, Compose 2.40.3 |
| Secure Boot | enabled; MOK `lab-host Secure Boot Module Signature key` enrolled |
| Wi-Fi driver | DKMS `rtl8821ce` (tomaspinho), пропатчен от UBSAN |
| Bluetooth | отключён (blacklist), чтобы не мешал Wi-Fi |
| Boot | `wait-online --any --timeout=30`; при вынутом Ethernet boot ~20 с |

## Документы

- [`INVENTORY.md`](INVENTORY.md) — железо, адреса, версии, пути.
- [`SETUP.md`](SETUP.md) — пошаговое воспроизведение с нуля.
- [`ha-voice/README.md`](ha-voice/README.md) — голосовые команды HA для кнопок/громкости.
- [`hermes/README.md`](hermes/README.md) — Hermes fallback, full HA/MA MCP, web search.
- [`stack/README.md`](stack/README.md) — **единая точка входа**: HA + MA + Whisper/Piper, `ctl.sh`.
- [`HA.md`](HA.md) — HAOS VM, compatibility proxy и legacy Container notes.
- [`../../docs/REMOTE-ACCESS.md`](../../docs/REMOTE-ACCESS.md) — удалённый доступ к
  HA: исследование, Tailscale/Serve, Link/Cloudflare/VPS, план и rollback.
- [`../../docs/REBUILD.md`](../../docs/REBUILD.md) — восстановление неттопа/станции
  с нуля (SSD, factory reset, Quasar/Mac).
- [`../../docs/REMOTE-ACCESS-TAILSCALE-PLAN.md`](../../docs/REMOTE-ACCESS-TAILSCALE-PLAN.md)
  — исторический план Tailscale Serve; текущий runtime — Operator.
- [`../../docs/REMOTE-ACCESS-STATIC-IP.md`](../../docs/REMOTE-ACCESS-STATIC-IP.md) —
  альтернатива для статического IP: WireGuard на `lab-host`, ONT/AX3 forward,
  Caddy/HTTPS, mini-PC router.
- [`../home/kubernetes/tailscale/verify-tailnet.sh`](../home/kubernetes/tailscale/verify-tailnet.sh) —
  текущая read-only проверка operator route; старый
  [`scripts/verify-remote-access.sh`](scripts/verify-remote-access.sh) — legacy.
- [`TROUBLESHOOTING.md`](TROUBLESHOOTING.md) — h2c/UBSAN, Secure Boot/MOK, IP.
- [`configs`](configs) — netplan, compose, modprobe/blacklist, wait-online drop-in.
- [`patches/rtl8821ce-ubsan.patch`](patches/rtl8821ce-ubsan.patch) — патч драйвера.
- [`scripts/verify.sh`](scripts/verify.sh) — проверка состояния.
- [`../../docs/TESTING.md`](../../docs/TESTING.md) — hermetic/live E2E тесты;
  [`../../reports/TEST-COVERAGE.md`](../../reports/TEST-COVERAGE.md) — покрытие.

## Важно

- **Пароли/PSK/HA-аккаунт здесь не хранятся.** Wi-Fi PSK — в `secrets/`,
  HA-пароль — только у владельца.
- **Secure Boot включён.** Не удалять `/var/lib/shim-signed/mok/` — иначе
  DKMS-модуль перестанет загружаться.
- **HA Container без add-ons.** Music Assistant/Whisper/Piper — отдельные
  контейнеры; HAOS VM — отдельный путь, если понадобятся add-ons.
- **Не выставлять 8123 в интернет.** Только LAN; удалённый доступ — через VPN.
- **Wi-Fi — вендорский DKMS-драйвер.** После обновления ядра проверить
  `dkms status` и при необходимости `sudo update-initramfs -u`.
- **Boot без кабеля:** Ethernet не обязателен; `wait-online` ждёт любой из
  `enp3s0`/`wlp4s0` до 30 с. Drop-in — в `projects/home-assistant/configs/systemd-networkd-wait-online.service.d/`.
- `sudo` без пароля — удобно, но это фактически root. Если сервер станет
  доступен недоверенным пользователям, убрать NOPASSWD.

## Связанные документы лаборатории

- `../docs/STATE.md` — текущие факты станции/сети.
- `../docs/ARCHITECTURE.md` — место неттопа в стенде.
- `../docs/OPERATIONS.md` — команды HA и станции.
- `../docs/RESEARCH-OWN-ASSISTANT.md` — Pulsar/HA/Spotify/MCP/wake word.
