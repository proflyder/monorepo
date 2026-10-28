# projects/station

Проект физической станции: Pulsar, ADB/SSH, OTA/ADB fortress, egress,
BLE gateway и provisioning. Это **не** HA и не TTS-стек.

## Что входит

```text
projects/station/upstream/pulsar/        upstream Pulsar + локальные изменения
projects/station/upstream/pulsar-rust/   Rust-форк (активен с 2026-10-28)
projects/station/patches/pulsar/          snapshots локальных правок
projects/station/ble-gateway/       BLE provisioning gateway (пока вне Bazel)
projects/station/scripts/station-*        station-side операции
docs/OTA-FORTRESS.md
docs/PROVISIONING.md
docs/WAKE-WORD.md
```

Rust-форк и профиль Ricardo: [`PULSAR-RUST.md`](PULSAR-RUST.md).
Активный бинарник — Rust `v0.5.0-1-g908a027`; C++ `v0.4.1-ricardo6`
сохранён в `.prev` для отката.

## Границы

- Pulsar — ESPHome satellite и Sendspin endpoint;
- HA/Pulsar audio/TTS — в `projects/home-assistant`;
- station не владеет HA-конфигом, MA, Piper, Whisper или Hermes;
- factory/recovery и OTA-защита остаются здесь.

## Статус

First-party station-файлы: `upstream/pulsar`, `upstream/pulsar-rust`, `ble-gateway`,
`configs`, `scripts`, `patches`, `tools`. Живая станция — Rust Pulsar;
C++ сохранён для отката. Прошивка/rootfs не менялись.
