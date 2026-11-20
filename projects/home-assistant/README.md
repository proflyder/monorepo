# Home Assistant / голос / музыка

Runtime: HAOS VM `100` + K3s VM `101`.

## Где что

```text
ha-voice/      HA intents/scripts/dashboard, Assist, alarms
deploy/        K3s chart для GigaAM/Whisper/Piper/Hermes
observability/ dashboards, Prometheus/Loki/Alloy
hermes/        Hermes gateway + voice proxy + MCP
tts/puriso/    Piper Puriso model/install
tools/tts-lab/ TTS evaluation/installer
e2e/           live e2e проекта
scripts/       операторские скрипты
configs/, patches/  PVE/Wi-Fi-драйвер и сетевые примеры (прежний неттоп)
```

## Текущий контур

- HAOS: `192.0.2.28:8123`; add-ons MA `2.10.9`, Matter `9.3.0`.
- K3s: GigaAM `10305`, Whisper `10300`, Piper `10200`, Hermes `8642`,
  voice proxy `8643`.
- Pipeline «Рикардо»: GigaAM → local intents → Hermes → Puriso.
- Станция: Pulsar (версия — `docs/STATE.md`), ESPHome `6053`, Sendspin `8928`.

## Документы

- [`../../docs/STATE.md`](../../docs/STATE.md)
- [`../../docs/OPERATIONS.md`](../../docs/OPERATIONS.md)
- [`../../docs/KUBERNETES-OPERATIONS.md`](../../docs/KUBERNETES-OPERATIONS.md)
- [`../../docs/REBUILD.md`](../../docs/REBUILD.md)
- [`ha-voice/README.md`](ha-voice/README.md)
- [`hermes/README.md`](hermes/README.md)
- [`tts/puriso/README.md`](tts/puriso/README.md)
- [`TROUBLESHOOTING.md`](TROUBLESHOOTING.md)

## Границы

- HA не владеет прошивкой Pulsar и BLE gateway.
- Hermes не владеет alarm/timer state; локальные intents должны работать без LLM.
- MA/Spotify — отдельный live-source; один аккаунт = один stream-slot.
- Секреты и реальные адреса — только в `secrets/`.

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `e2e/kubernetes_runtime.py` — Read-only acceptance of the migrated cluster and Git-owned dashboards.
- `e2e/live_ha.py` — Opt-in end-to-end tests against the real Home Assistant.
  - The confirmation turn is deliberately NOT sent: the gateway must keep the action pending and answer with a question, never restart by itself.
  - alarm_start is a multi-step script. Do not race its source/volume capture with the stop request: ring goes on before the sequence ends.
- `e2e/live_hermes.py` — Opt-in live checks for the Hermes fallback API.
- `e2e/live_scheduler_voice.py` — Opt-in live check: HA voice intents -> canonical SchedulerService.
  - Unique per process so cleanup can never touch another session's resources. Cyrillic only: the Russian sentence slots do not reliably match Latin labels.
- `e2e/wyoming_smoke.py` — Synthesize and transcribe a fixed phrase without playback or HA mutations.
