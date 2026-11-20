# projects/station

Проект физической станции: Pulsar, ADB/SSH, OTA/ADB fortress, egress,
BLE gateway и provisioning. Это **не** HA и не TTS-стек.

## Что входит

```text
projects/station/pulsar/          Rust Pulsar (AGPL-3.0), включая radio/ и installer/
projects/station/ble-gateway/     BLE provisioning gateway (Rust, Bazel)
projects/station/manifest.yaml    состав ПО колонки и пути/версии для сверки
projects/station/tools/station-audit  read-only сверка устройства с манифестом
projects/station/scripts/station-*  station-side операции
docs/OTA-FORTRESS.md
docs/STATION-SOFTWARE.md
docs/PROVISIONING.md
docs/WAKE-WORD.md
```

Состав ПО и известные расхождения —
[`docs/STATION-SOFTWARE.md`](../../docs/STATION-SOFTWARE.md); открытые этапы —
[`docs/PENDING.md`](../../docs/PENDING.md). Сверка с живой
колонкой — `mise run station:audit` (только чтение по SSH).

Профиль Ricardo — [`pulsar/RICARDO.md`](pulsar/RICARDO.md); установленная
версия — `docs/STATE.md`.

Исходники Pulsar — first-party в этом монорепозитории. PR CI собирает обе
архитектуры и публикует `ghcr.io/proflyder/pulsar-firmware:tree-<hash>` с
provenance; merge в `develop` только продвигает этот digest на теги `<version>`
и `:latest` (`native-release`). Версия вычисляется (`platform/ci/version.mjs`:
`VERSION` — только major.minor), поэтому отдельного PR с бампом нет. После approve в Environment `Station` job
`deploy-station` катит его на станцию через HA updater. GitHub Releases и
теги не используются. Upstream
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

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `e2e/live_gateway.py` — Read-only live checks for the Station BLE gateway HTTP control.
- `manifest.yaml` — Station software manifest — what actually runs on the Yandex Station Midi besides the OTA-managed Pulsar binary.
- `scripts/restore-ota-fortress.sh` — Restore the original OTA/ADB files and remove fortress (revert the on-device protection). Reboots the station. Use with --yes in scripts.
- `scripts/station-egress-guard-off.sh` — FORTRESS-EGRESS-GUARD-OFF v1 Remove only the egress guard; keep the existing Yandex block/OTA fortress.
- `scripts/station-egress-guard-watchdog.sh` — FORTRESS-EGRESS-GUARD-WATCHDOG v1 Safety net for first deployment: if not confirmed within 180s, remove guard.
- `scripts/station-egress-guard.sh` — FORTRESS-EGRESS-GUARD v1 Allow only loopback, local LAN, link-local, multicast/broadcast, DHCP, (optionally) Google NTP and (optionally) HTTPS to GitHub for Pulsar's updater. Drop all other IPv4/IPv6 egress. INPUT is untouched: ADB/SSH and LAN access keep working. Teardown: /data/quasar/egress-guard-off.sh
- `scripts/station-network-diagnostic.sh` — Read-only, bounded network evidence. Logs may identify the LAN: keep private. Arm the rootkeeper hook separately for a controlled reboot; no radio resets.
- `scripts/station-pairing-sound.py` — Generate a Yandex Station sound pairing code locally (no Telegram, no cloud).
- `scripts/test_egress_guard.py` — Hermetic tests for the station egress-guard rule set.
- `tools/station-audit` (`mise run station:audit -- --ip 192.0.2.13`, `--json`) — read-only reconciliation of the station against `projects/station/manifest.yaml`: BLE gateway, dropbear, egress guard, yandex-block, OTA fortress, rootkeeper and config files installed by hand. One SSH connection, only `sha256sum`, `ls` and `--version`; never writes to the station, never prints file contents or secrets. `deployed_sha256` pins build artifacts (the pinned hash is authoritative). Native release versions are computed by `platform/ci/version.mjs`, the same source CI uses. IPv4 addresses are masked so LAN values do not drift. Statuses: OK, DRIFT, MISSING, EXTRA (stale `.bak/.prev/.new` siblings unless in `audit.extra_allow`).
