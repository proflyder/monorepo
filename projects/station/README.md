# projects/station

Проект физической станции: Pulsar, ADB/SSH, OTA/ADB fortress, egress,
BLE gateway и provisioning. Это **не** HA и не TTS-стек.

## Что входит

```text
projects/station/pulsar/          Rust Pulsar (AGPL-3.0), включая radio/ и installer/
projects/station/ble-gateway/     BLE provisioning gateway (пока вне Bazel)
projects/station/scripts/station-*  station-side операции
docs/OTA-FORTRESS.md
docs/PROVISIONING.md
docs/WAKE-WORD.md
```

Pulsar и профиль Ricardo: [`PULSAR-RUST.md`](PULSAR-RUST.md).
Установленный бинарник станции — Rust `v0.5.1005` (линия Ricardo, база
upstream `v0.5.4` = `f1f5b17`, HEAD `427a8e3`) с Ricardo pre-roll 256 ms,
number «Follow-up duration», STT stop-word fallback и «Wake: не услышала» с
сохранением последних 30 с; SHA256 `89415c5a…71671b6`. `pulsar.prev` —
`v0.5.1004` (`fb0695b2…`); historical: `3a544979…` (`v0.5.1002`),
`80aea2ec…` (`v0.5.1001`, база `v0.5.2`, тот же Ricardo-код), `7081ef8e…`
(`v0.5.2-4-g9b5e86a-dirty`), `0b0c48e4…` (endpointing 512 ms),
`pulsar.pre-endpoint-20261030` — `v0.5.2-3-gc57d521-dirty`;
`v0.5.2-2-g95ce568` и `v0.5.0-1-g908a027` — private backup вне Git.
Legacy исходники и binaries — private в `secrets/local/recovery/`.

Исходники Pulsar — first-party в этом монорепозитории. Релиз — изменение
`projects/station/pulsar/VERSION`: merge в `develop` запускает
`release-pulsar.yml`, который собирает обе архитектуры и публикует OCI-артефакт
`ghcr.io/proflyder/pulsar-firmware:<version>` (+ `:latest`); после approve в
Environment `Station` job катит его на станцию через HA updater. GitHub
Releases и теги не используются. Upstream `Morvex885/pulsar` вливается
вручную патчем. Процедура — `docs/OPERATIONS.md`, полная версия —
`projects/station/pulsar/RICARDO.md`, «Релизы и деплой». Откат — только на
станции (`pulsar.prev` + `ctl.sh reload`).

## Границы

- Pulsar — ESPHome satellite и Sendspin endpoint;
- HA/Pulsar audio/TTS — в `projects/home-assistant`;
- station не владеет HA-конфигом, MA, Piper, Whisper или Hermes;
- factory/recovery и OTA-защита остаются здесь.

## Статус

First-party station-файлы: `pulsar`, `ble-gateway`, `configs`, `scripts`,
`tools`. Живая станция — Rust Pulsar; legacy C++ архив хранится private вне
активного worktree. Прошивка/rootfs не менялись.
