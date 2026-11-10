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
| `OK` | содержимое/версия совпадает с репозиторием |
| `DRIFT` | файл есть, но sha256 или версия отличается |
| `MISSING` | ожидаемого пути на устройстве нет |
| `EXTRA` | рядом с управляемыми файлами лежит `.bak`/`.prev`/`.new`/`.old` |

Для собираемых бинарников sha256 не сравнивается: манифест помечает их
`compare: version` и сверяет строку версии с `VERSION`/`Cargo.toml`/пином
сборки. Для конфигов с адресом домашней сети включён `normalize: ipv4`:
репозиторные копии используют `192.0.2.x`, на устройстве — реальный LAN-адрес,
и это не считается дрейфом. Любое другое отличие содержимого — дрейф.

Код возврата: `0` — в пределах порога, `1` — найден статус из `--fail-on`
(по умолчанию только `MISSING`), `2` — ошибка манифеста/SSH. Скрипт не входит
в `mise run check`: ему нужна живая колонка, поэтому он остаётся ручным
чтением. Тесты парсинга/отчёта — герметичные, без сети.

## Известные расхождения (зафиксированы аудитом)

Это факты текущего устройства, а не ошибка манифеста:

- **BLE gateway**: бинарник на устройстве отвечает `0.0.0`, в репозитории
  `Cargo.toml` уже `0.2.0`. Нужна пересборка/переустановка бинарника либо
  осознанное решение не сверять версию.
- **dropbear**: в репозитории `projects/station/pulsar/ssh` собирается
  dropbear `2025.88` и ставится в `/data/dropbear` с запуском из
  `S99zpulsar`; на устройстве вручную стоит `v2026.94` в
  `/usr/sbin/dropbear` с отдельным `S99dropbear`. Два пути сосуществуют;
  переустановка Pulsar с `--ssh` создаст второй экземпляр в `/data`.
- **Лишние файлы**: `pulsar.prev*`, `ble-gateway.prev*`,
  `egress-guard.sh.bak-*`, `onboot.sh.new` — кандидаты на уборку после
  подтверждения, что текущие версии рабочие.

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