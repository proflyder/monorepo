# projects/station

Проект физической станции: Pulsar, ADB/SSH, OTA/ADB fortress, egress,
BLE gateway и provisioning. Это **не** HA и не TTS-стек.

## Что входит

```text
projects/station/upstream/pulsar-rust/   Rust-форк (upstream v0.5.2 интегрирован и установлен)
projects/station/patches/pulsar-rust/     snapshots локальных правок Rust
projects/station/ble-gateway/       BLE provisioning gateway (пока вне Bazel)
projects/station/scripts/station-*        station-side операции
docs/OTA-FORTRESS.md
docs/PROVISIONING.md
docs/WAKE-WORD.md
```

Rust-форк и профиль Ricardo: [`PULSAR-RUST.md`](PULSAR-RUST.md).
Установленный бинарник станции — Rust `v0.5.2-4-g9b5e86a-dirty`
(локальный форк на базе upstream `v0.5.2`) с Ricardo pre-roll 256 ms;
SHA256 `7081ef8e…`. `pulsar.prev` — прежний endpointing 512 ms
(`0b0c48e4…`); historical `pulsar.pre-endpoint-20261030` —
`v0.5.2-3-gc57d521-dirty`; `v0.5.2-2-g95ce568` и `v0.5.0-1-g908a027` —
private backup вне Git. Legacy исходники и binaries — private в
`secrets/local/recovery/`.

## Границы

- Pulsar — ESPHome satellite и Sendspin endpoint;
- HA/Pulsar audio/TTS — в `projects/home-assistant`;
- station не владеет HA-конфигом, MA, Piper, Whisper или Hermes;
- factory/recovery и OTA-защита остаются здесь.

## Статус

First-party station-файлы: `upstream/pulsar-rust`, `ble-gateway`,
`configs`, `scripts`, `patches`, `tools`. Живая станция — Rust Pulsar;
legacy C++ архив хранится private вне активного worktree. Прошивка/rootfs
не менялись.
