# NETTOP — домашний сервер HA

Отдельная папка по мини-ПК **DEXP Jupiter JU01CY**: воспроизводимый сетап
Ubuntu Server + Docker + Home Assistant Container + Wi-Fi/статические IP.

**Текущий статус:** HA Container работает; Music Assistant `2.10.8` с Spotify
готов; HA Assist base (Whisper/Piper/`Рикардо`) и голосовые команды проверены.
Pulsar `v0.4.1-ricardo6` подключён; HA integration Music Assistant добавлена.
Spotify 429 снят 2026-10-27; Connect search/play проверены, generic
«включи музыку» идёт локально. Станция видна в Spotify, volume sync
app↔Pulsar работает, «громкость 1…10» — голосом. Таймеры Assist уже
работают; добавлен один будильник (непрерывный рингтон, snooze, resume
музыки после stop). Добавлен Hermes Agent `v0.22.5` как fallback для
свободной речи/веб-поиска; HA MCP Server, модель `gpt-5.6-luna`,
`web_search` через ChatGPT/Codex; личность — Рикардо, личный ассистент.
Фактический голосовой retest таймера/будильника/«стоп» — открыт.
Wi-Fi/Ethernet failover, Secure Boot + MOK; boot без Ethernet ~20 с.

## Быстрые факты

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
| Piper | `wyoming-piper:2.5.2`, `:10200`, `ru_RU-irina-medium` |
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
- [`HA.md`](HA.md) — HA Container: compose, обновление, backup, безопасность.
- [`../../docs/REMOTE-ACCESS.md`](../../docs/REMOTE-ACCESS.md) — удалённый доступ к
  HA: исследование, Tailscale/Serve, Link/Cloudflare/VPS, план и rollback.
- [`../../docs/REBUILD.md`](../../docs/REBUILD.md) — восстановление неттопа/станции
  с нуля (SSD, factory reset, Quasar/Mac).
- [`../../docs/REMOTE-ACCESS-TAILSCALE-PLAN.md`](../../docs/REMOTE-ACCESS-TAILSCALE-PLAN.md)
  — **выбранный план на аппрув**: Tailscale на `lab-host` + Serve + MFA.
- [`../../docs/REMOTE-ACCESS-STATIC-IP.md`](../../docs/REMOTE-ACCESS-STATIC-IP.md) —
  альтернатива для статического IP: WireGuard на `lab-host`, ONT/AX3 forward,
  Caddy/HTTPS, mini-PC router.
- [`scripts/verify-remote-access.sh`](scripts/verify-remote-access.sh) —
  read-only проверка HA/Tailscale/Serve/NTP/портов (запускать на `lab-host`).
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
