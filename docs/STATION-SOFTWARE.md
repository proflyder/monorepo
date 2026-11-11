# Состав ПО колонки

Колонка (Yandex Station Midi) получает по OTA **только Pulsar**. Остальное ПО
поставлено вручную поверх прошивки и не обновляется автоматически: BLE
gateway, dropbear, ADB/rootkeeper, egress guard, yandex-block, конфиги
unbound/chrony и OTA-fortress. Этот документ фиксирует состав и владельцев;
машинный источник правды — [`projects/station/manifest.yaml`](../projects/station/manifest.yaml).

Что именно установлено сейчас и какие версии — `STATE.md` (раздел «Станция»).
OTA/ADB-защита — `OTA-FORTRESS.md`; восстановление — `REBUILD.md`.

## Что покрывает OTA

| Путь | Обновление | Кто ставит |
|---|---|---|
| `/data/pulsar/pulsar`, `boot.sh`, `supervise.sh`, `ctl.sh` | `ota` | CI → GHCR → HA updater → `pulsar-install` |
| BLE gateway, dropbear, ADB, rootkeeper, guard, block, конфиги, OTA-fortress | `manual-rootfs` / `firmware` | вручную по SSH/ADB; см. манифест |

OTA обновляет один бинарник Pulsar и его скрипты. Правки в
`/system/vendor/quasar` и `/etc` живут на read-only rootfs и переживают
перезагрузку, но не восстановятся после factory reset; `/data` сносится
factory reset'ом. Это осознанная граница: OTA не должен трогать защиту и
аварийные каналы.

## Компоненты

Полные поля (назначение, источник в репозитории, путь, запуск, владелец
конфига/состояния, критичность) — в манифесте. Кратко:

| Компонент | На устройстве | Запуск | Обновление | recovery |
|---|---|---|---|---|
| `pulsar` | `/data/pulsar` | `S99zpulsar` → `boot.sh` → `supervise.sh` | `ota` | да |
| `ble-gateway` | `/data/quasar/ble-gateway` | `rootkeeper.sh` каждые 60 с | `manual-rootfs` | да |
| `dropbear` | `/usr/sbin/dropbear`, `/etc/init.d/S99dropbear` | `S99dropbear` | `manual-rootfs` | да |
| `adbd` | `/usr/bin/adbd` + маркеры ADB | `rootkeeper.sh` каждые 60 с | `firmware` | да |
| `rootkeeper` | `/data/quasar/rootkeeper.sh` | `onboot.sh` в фоне | `manual-rootfs` | да |
| `ota-fortress` | `/system/vendor/quasar/*` | `onboot.sh`, wrapper'ы | `manual-rootfs` | да |
| `egress-guard` | `/data/quasar/egress-guard*.sh` | `onboot.sh` + `rootkeeper.sh` | `manual-rootfs` | нет |
| `yandex-block` | `/data/quasar/yandex-block.sh`, `/etc/hosts` | `onboot.sh` + `rootkeeper.sh` | `manual-rootfs` | нет |
| `unbound` | `/etc/unbound/unbound.conf` | прошивочный `S70unbound` | `manual-rootfs` | нет |
| `chrony` | `/etc/chrony.conf` | прошивочный `S49chrony` при флаге | `manual-rootfs` | нет |

`recovery: true` — без компонента нельзя вернуться на устройство и починить
остальное (SSH, BLE, ADB, root, OTA-защита). Потеря такого компонента не
ломает звук сразу, но убирает путь восстановления.

## Сверка: `station-audit`

Скрипт [`projects/station/tools/station-audit`](../projects/station/tools/station-audit)
читает манифест, открывает **одно** SSH-соединение и только читает:
`sha256sum`, `ls` и `--version` бинарников. На устройство ничего не пишется,
содержимое файлов и секреты не печатаются.

```bash
mise run station:audit                       # адрес из STATION_IP или secrets/NETWORK.md
mise run station:audit -- --ip 192.0.2.13
mise run station:audit -- --json             # машинный вывод
mise run station:audit -- --fail-on any      # ненулевой код при любом расхождении
```

Адрес берётся по приоритету: `--ip` → `STATION_IP` → строка `Станция` в
приватном `secrets/NETWORK.md` (его нет в Git). Ключ по умолчанию —
`~/.ssh/home-lab/station_ed25519`.

Статусы:

| Статус | Значение |
|---|---|
| `OK` | содержимое/версия/зафиксированный sha256 совпадает |
| `DRIFT` | файл есть, но sha256, версия или `deployed_sha256` отличается |
| `MISSING` | ожидаемого пути на устройстве нет |
| `EXTRA` | рядом с управляемыми файлами лежит `.bak`/`.prev`/`.new`/`.old`, кроме путей из `audit.extra_allow` |

Для собираемых бинарников есть два режима. `compare: version` сверяет
строку версии с `VERSION`/`Cargo.toml`/пином сборки. Если у бинарника задан
`deployed_sha256`, сверяется sha256 установленного файла (`OK`/`DRIFT`), а
версия печатается только информативно: так фиксируются сборки, которые
старше исправления версии, и бинарники из прошивки/rootfs, для которых в
репозитории нет исходников. `audit.extra_allow` — точные пути штатных слотов
отката (`pulsar.prev`, `ble-gateway.prev`), которые не считаются мусором.

Для конфигов с адресом домашней сети включён `normalize: ipv4`:
репозиторные копии используют `192.0.2.x`, на устройстве — реальный LAN-адрес,
и это не считается дрейфом. Любое другое отличие содержимого — дрейф.

Код возврата: `0` — в пределах порога, `1` — найден статус из `--fail-on`
(по умолчанию только `MISSING`), `2` — ошибка манифеста/SSH. Скрипт не входит
в `mise run check`: ему нужна живая колонка, поэтому он остаётся ручным
чтением. Тесты парсинга/отчёта — герметичные, без сети.

## Закрытые расхождения (источник правды — устройство, 2026-11-10)

Расхождения ниже были зафиксированы аудитом и теперь закрыты в репозитории;
устройство при этом не менялось.

- **BLE gateway — версия.** Любая Bazel-сборка печатала `ble-gateway 0.0.0`,
  потому что `rules_rust` не передавал `CARGO_PKG_VERSION`. Теперь в
  `crates/ble-gateway/BUILD.bazel` задан `version = PACKAGE_VERSION` (0.2.0 из
  `Cargo.toml`), unit-тест сверяет `--version` с манифестом Cargo. Установленный
  бинарник (сборка 2026-11-05, sha256 `cdadac02…a9377f`) пока отвечает `0.0.0`;
  в манифесте он закреплён через `deployed_sha256`, версия информативна.
  Gateway на устройстве сейчас не переустанавливается.
- **dropbear.** На устройстве стоит предустановленный прошивкой
  `/usr/sbin/dropbear` `v2026.94` (sha256 `374be9f8…4a654db2`), запуск —
  `/etc/init.d/S99dropbear` (`DAEMON=/usr/sbin/dropbear`,
  `KEY=/etc/dropbear/dropbear_ed25519_host_key`, `-p 22 -r $KEY -P
  /var/run/dropbear.pid`); `/data/dropbear` отсутствует. Точная копия
  init-скрипта лежит в `projects/station/configs/station-applied/S99dropbear` и
  сверяется по sha256, бинарник закреплён `deployed_sha256` с пометкой
  `origin: prebuilt dropbear 2026.94, rootfs`. Репозиторный
  `projects/station/pulsar/ssh/` (build.sh 2025.88 → `/data/dropbear`,
  запуск из `S99zpulsar`) на этой станции не используется и помечен legacy;
  он остаётся только для `pulsar-install --ssh`.
- **Лишние файлы.** Нештатные `.bak`/`.new`/`.old` удалены с устройства.
  Штатные слоты отката `pulsar.prev` и `ble-gateway.prev` оставлены осознанно
  и внесены в `audit.extra_allow`.

## Обновление BLE gateway

1. Собрать armhf-бинарник с версией и ревизией:
   ```bash
   bazel build -c opt --platforms=//platform/bazel/zig:armhf_linux \
     --define=ble_gateway_git_rev=$(git rev-parse --short HEAD) \
     //projects/station/ble-gateway:ble-gateway
   bazel-bin/projects/station/ble-gateway/crates/ble-gateway/ble-gateway --version
   ```
   Ожидается `ble-gateway 0.2.0`; без `version` в BUILD снова будет `0.0.0`.
2. `bazel test //projects/station/ble-gateway/...` (unit, clippy, rustfmt).
3. Скопировать бинарник в `/data/quasar/ble-gateway/ble-gateway`, сохранив
   текущий как `ble-gateway.prev`; `start.sh`/`stop.sh`/`status.sh` не меняются.
4. Проверить с телефона: BLE-устройство видно, GATT-сервис отвечает,
   `--dry-run --auto=false --no-button --force` возвращает `DRY_RUN_OK`;
   после этого — обычный provisioning в тестовой сети.
5. Обновить `deployed_sha256` в `manifest.yaml` и прогнать
   `mise run station:audit` до `0 DRIFT, 0 EXTRA`.
6. Откат: остановить gateway, вернуть `ble-gateway.prev` на место
   `ble-gateway`, запустить `start.sh`.

## Мониторинг

Pulsar отдаёт `/metrics` на `http://<колонка>:9101/metrics` (Prometheus text,
без авторизации, только LAN; `PULSAR_PORT_OFFSET` сдвигает порт). Бинарник и
набор файлов на колонке от этого не меняются — OTA достаточно.

Домашний Prometheus забирает колонку как **внешний** таргет: job `station` в
`projects/home/kubernetes/observability/values/prometheus.yaml`, адрес —
`${STATION_IP}:9101` из `Secret/flux-app-settings` (в Git значения нет).
Prometheus в namespace `monitoring` не ограничен egress-NetworkPolicy, поэтому
дополнительных правил не нужно. Grafana-дашборды («Колонка — железо»,
«Колонка — Pulsar») и PrometheusRule `home-station-alerts` провиженятся Flux:

| Объект | Где |
|---|---|
| Scrape job `station` | `projects/home/kubernetes/observability/values/prometheus.yaml` |
| Дашборды, папка Grafana «Колонка» | `projects/station/observability/chart/` |
| HelmRelease `station-observability` | `projects/home/kubernetes/observability/manifests/helmrelease-station-dashboards.yaml` |
| Алерты | `projects/home/kubernetes/observability/rules/prometheusrule-station.yaml` |

Алерты (Telegram, severity `warning`):

- `StationDown` — `up{job="station"}` нет 5 минут (питание, сеть, порт 9101).
- `StationCpuHot` — температура SoC > 85 °C 10 минут. У Amlogic Tj max
  125 °C, штатный троттлинг около 90 °C; 85 °C — запас до него.
- `StationMemoryHigh` — available < 10% RAM 10 минут.
- `StationDiskFilling` — свободно < 15% `/data` 30 минут или линейный прогноз
  заполнения за 24 часа по 6 часам.

Проверки без кластера: `python3 -m unittest
projects.home.kubernetes.observability.test_prometheusrule` (контракт алертов,
сверка имён метрик с `pulsar-metrics/src/catalog.rs`, `promtool` при наличии),
`mise run check` и `node platform/quality/audit_repository.mjs`.

## План этапов 2–5

Ниже — план, не реализация. Каждый этап — отдельный PR со своими проверками.

1. **Этап 2 — один установщик.** Расширить `pulsar-install` (или добавить
   `station-install`) до идемпотентной установки всего манифеста: версии,
   sha256, права, порядок запуска, `--dry-run` и отчёт. Единственный вход для
   BLE gateway, dropbear, guard, block, конфигов и rootfs-правок; ручной
   `adb push` остаётся аварийным путём. Критерий: повторный запуск не меняет
   устройство, аудит остаётся `OK`.
2. **Этап 3 — один супервизор.** Убрать дублирование `rootkeeper.sh` / `supervise.sh` /
   `S99*`: один супервизор с декларативным списком процессов (adbd,
   ble-gateway, guard, block) и health-проверками. Pulsar остаётся под своим
   `supervise.sh` до отдельного решения. Критерий: после reboot все процессы
   поднимаются без второго цикла и без гонок.
3. **Этап 4 — наблюдаемость guard/gateway в HA.** Публиковать состояние
   egress guard (цепочки/счётчики), BLE gateway (running/auto/ошибка) и
   rootkeeper в HA (сенсоры + алерт при пропаже). Только чтение состояния;
   управление — существующие `ctl.sh`/`start.sh`. Критерий: алерт при
   остановленном guard/gateway и отсутствии ADB.
4. **Этап 5 — rootfs-правки вне OTA.** Формализовать `/system/vendor/quasar` и `/etc`:
   снимок оригиналов, версия применённого набора, проверка после обновления
   прошивки, откат. Не пытаться вернуть эти файлы в OTA Pulsar. Критерий:
   новая прошивка не ломает fortress молча; аудит показывает понятный DRIFT.
5. **После этапа 5 — приёмка и защита от дрейфа.** Сделать
   `station-audit --fail-on drift` регулярной проверкой (по расписанию/после
   обновления), добавить в отчёт ожидаемые версии из CI, закрыть известные
   расхождения из раздела выше. Критерий: аудит зелёный на живой колонке и в
   отчёте о восстановлении.