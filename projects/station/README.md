# projects/station

Проект физической станции: Pulsar, ADB/SSH, OTA/ADB fortress, egress,
BLE gateway и provisioning. Это **не** HA и не TTS-стек.

## Что входит

```text
projects/station/pulsar/          Rust Pulsar (AGPL-3.0), включая radio/ и installer/
projects/station/ble-gateway/     BLE provisioning gateway (Rust, Bazel)
projects/station/scripts/station-*  station-side операции
docs/OTA-FORTRESS.md
docs/PROVISIONING.md
docs/WAKE-WORD.md
```

Профиль Ricardo — [`pulsar/RICARDO.md`](pulsar/RICARDO.md); установленная
версия — `docs/STATE.md`.

Исходники Pulsar — first-party в этом монорепозитории. Релиз — изменение
`projects/station/pulsar/VERSION`: merge в `develop` проводит Pulsar через
общий `deploy-home` (`publish_when` по VERSION), `publish-native` публикует
OCI-артефакт `ghcr.io/proflyder/pulsar-firmware:<version>` (+ `:latest`),
после approve в Environment `Station` job `deploy-station` катит его на
станцию через HA updater. GitHub Releases и теги не используются. Upstream
`Morvex885/pulsar` вливается вручную патчем. Процедура —
`docs/OPERATIONS.md`, полная версия — `projects/station/pulsar/RICARDO.md`,
«Релизы и деплой». Откат — только на станции (`pulsar.prev` +
`ctl.sh reload`).

## Границы

- Pulsar — ESPHome satellite и Sendspin endpoint;
- HA/Pulsar audio/TTS — в `projects/home-assistant`;
- station не владеет HA-конфигом, MA, Piper, Whisper или Hermes;
- factory/recovery и OTA-защита остаются здесь.

## Статус

First-party station-файлы: `pulsar`, `ble-gateway`, `configs`, `scripts`,
`tools`. Прошивка/rootfs станции не менялись (только `/data` и OTA fortress).
