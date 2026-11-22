# projects/station

Проект физической станции: Pulsar, ADB/SSH, OTA/ADB fortress, egress guard,
BLE gateway и provisioning. HA-конфиг, MA, TTS, STT и Hermes сюда не входят
(`projects/home-assistant`).

```text
pulsar/          Rust Pulsar (AGPL-3.0), включая radio/ и installer/; профиль — pulsar/RICARDO.md
ble-gateway/     BLE provisioning gateway (Rust, Bazel)
manifest.yaml    состав ПО колонки и пути/версии для сверки
configs/         применённая защита, диагностика, профили wake word
scripts/         station-side операции: egress guard, откат fortress, диагностика сети
tools/           station-audit, voice-recorder, wakeword-personalize, quasar-root-kit
observability/   дашборды Grafana колонки
e2e/             live-проверки (только чтение)
```

- Состав ПО, OTA/ADB fortress, provisioning —
  [`docs/STATION-SOFTWARE.md`](../../docs/STATION-SOFTWARE.md); сверка с живой
  колонкой — `mise run station:audit` (только чтение по SSH).
- Wake word — [`docs/WAKE-WORD.md`](../../docs/WAKE-WORD.md).
- Установленная версия — `docs/STATE.md`; открытые этапы — `docs/PENDING.md`.

## Релиз и откат Pulsar

PR CI собирает обе архитектуры и публикует
`ghcr.io/proflyder/pulsar-firmware:tree-<hash>` с provenance; merge в `develop`
продвигает этот digest на `<version>` и `:latest`. Версию вычисляет
`platform/ci/version.mjs` (`VERSION` — только major.minor). После approve в
Environment `Station` job `deploy-station` катит образ через HA updater.
GitHub Releases и теги не используются. Процедура — `docs/OPERATIONS.md` и
`pulsar/RICARDO.md`, «Релизы и деплой». Откат — только на станции
(`pulsar.prev` + `ctl.sh reload`).

## Скрипты

- `scripts/station-egress-guard.sh` — разрешает только loopback, LAN,
  link-local, multicast/broadcast, DHCP и опционально NTP/HTTPS для updater;
  остальной egress режется, INPUT (ADB/SSH) не трогается. Снятие —
  `/data/quasar/egress-guard-off.sh`; `station-egress-guard-watchdog.sh`
  снимает guard, если его не подтвердили за 180 с. Тесты —
  `scripts/test_egress_guard.py`.
- `scripts/restore-ota-fortress.sh` — откат fortress (с перезагрузкой).
- `scripts/station-pairing-sound.py` — локальная генерация звукового кода
  сопряжения.
