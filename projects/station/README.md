# projects/station

Проект физической станции: Pulsar, ADB/SSH, OTA/ADB fortress, egress,
BLE gateway и provisioning. Это **не** HA и не TTS-стек.

## Что входит

```text
projects/station/upstream/pulsar/        upstream Pulsar + локальные изменения
projects/station/upstream/pulsar-rust/   отдельный Rust-форк (не установлен)
projects/station/patches/pulsar/          snapshots локальных правок
projects/station/ble-gateway/       BLE provisioning gateway (пока вне Bazel)
projects/station/scripts/station-*        station-side операции
docs/OTA-FORTRESS.md
docs/PROVISIONING.md
docs/WAKE-WORD.md
```

Rust-форк и профиль Ricardo: [`PULSAR-RUST.md`](PULSAR-RUST.md). Он не
заменяет активный Pulsar и не устанавливается без отдельной HIL-приёмки.

## Границы

- Pulsar — ESPHome satellite и Sendspin endpoint;
- HA/Pulsar audio/TTS — в `projects/home-assistant`;
- station не владеет HA-конфигом, MA, Piper, Whisper или Hermes;
- factory/recovery и OTA-защита остаются здесь.

## Статус

First-party station-файлы уже перенесены сюда: `upstream/pulsar`, `ble-gateway`,
`configs`, `scripts`, `patches`, `tools`. Живая станция и её `/data/pulsar`
не менялись; remote-пути и прошивка остаются прежними.
