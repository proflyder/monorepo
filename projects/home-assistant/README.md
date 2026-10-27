# projects/home-assistant

Проект домашнего сервера: HA Container, Music Assistant, STT/TTS, Hermes,
Tailscale и HA-конфиг. Станция здесь только как внешний ESPHome/Sendspin
endpoint.

## Что входит

```text
/home/labuser/nettop/homeassistant/config/   live HA configuration (remote)
projects/home-assistant/ha-voice/               HA intents/scripts/dashboard
projects/home-assistant/stack/                  compose, ctl.sh, Piper/Whisper/MA
projects/home-assistant/hermes/                 Hermes/MCP config
projects/home-assistant/tts/                    Puriso model/runbook
projects/home-assistant/configs/                host netplan/modprobe
projects/home-assistant/tools/tts-lab/          TTS evaluation/installer
docs/OPERATIONS.md
docs/REMOTE-ACCESS*.md
```

## Solutions

- `solutions/ha-bridge` — мост HA ↔ alarm;
- `solutions/mcp-alarm` — MCP для Hermes;
- `solutions/assistant-ui` — PWA/UI, если нужен отдельный интерфейс;
- `solutions/alarm` — локальный alarm-инстанс.

## Границы

- HA не владеет Pulsar-кодом и прошивкой станции;
- Pulsar не владеет HA YAML;
- звук/рингтон/TTS остаются в HA/Pulsar bridge;
- секреты и endpoint — в project env, не в solutions.

## Статус

First-party voice/stack/hermes/tts/configs уже перенесены в
`projects/home-assistant/`. Живой HA-конфиг остаётся на сервере в
`/home/labuser/nettop/homeassistant/config`; его перенос в репозиторий
потребует отдельного backup и обновления `ctl.sh`/systemd/live-проверок.
