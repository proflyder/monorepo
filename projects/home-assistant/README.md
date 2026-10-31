# Home Assistant / голос / музыка

Текущий runtime: HAOS VM `100` + K3s VM `101`. Docker/Compose — legacy.

## Где что

```text
ha-voice/      HA intents/scripts/dashboard, Assist, alarms
deploy/        K3s chart для GigaAM/Whisper/Piper/Hermes
observability/ dashboards, Prometheus/Loki/Alloy
hermes/        Hermes gateway + voice proxy + MCP
tts/puriso/    Piper Puriso model/install
configs/       host netplan/modprobe/wait-online
tools/tts-lab/ TTS evaluation/installer
```

## Текущий контур

- HAOS: `192.0.2.28:8123`; add-ons MA `2.10.8`, Matter `9.2.0`.
- K3s: GigaAM `10305`, Whisper `10300`, Piper `10200`, Hermes `8642`,
  voice proxy `8643`.
- Pipeline «Рикардо»: GigaAM → local intents → Hermes → Puriso.
- Pulsar Rust `v0.5.2-4-g9b5e86a-dirty` (armhf, Ricardo pre-roll 256 ms);
  ESPHome `6053`, Sendspin `8928`.

## Документы

- [`../../docs/STATE.md`](../../docs/STATE.md)
- [`../../docs/OPERATIONS.md`](../../docs/OPERATIONS.md)
- [`../../docs/KUBERNETES-OPERATIONS.md`](../../docs/KUBERNETES-OPERATIONS.md)
- [`../../docs/REBUILD.md`](../../docs/REBUILD.md)
- [`ha-voice/README.md`](ha-voice/README.md)
- [`hermes/README.md`](hermes/README.md)
- [`tts/puriso/README.md`](tts/puriso/README.md)

## Границы

- HA не владеет прошивкой Pulsar и BLE gateway.
- Hermes не владеет alarm/timer state; локальные intents должны работать без LLM.
- MA/Spotify — отдельный live-source; один аккаунт = один stream-slot.
- Секреты и реальные адреса — только в `secrets/`.
