# ПО колонки: состав, OTA/ADB fortress, Wi-Fi provisioning

Колонка (Yandex Station Midi) получает по OTA **только Pulsar**. Остальное ПО
поставлено вручную поверх прошивки и не обновляется автоматически: BLE
gateway, dropbear, ADB/rootkeeper, egress guard, yandex-block, конфиги
unbound/chrony и OTA-fortress. Машинный источник правды —
[`projects/station/manifest.yaml`](../projects/station/manifest.yaml) (назначение,
источник, путь, запуск, критичность каждого компонента). Установленные
версии — `STATE.md` (раздел «Станция»); восстановление — `REBUILD.md`.

## Граница OTA

| Путь | Обновление | Кто ставит |
|---|---|---|
| `/data/pulsar/pulsar`, `boot.sh`, `supervise.sh`, `ctl.sh` | `ota` | CI → GHCR → HA updater → `pulsar-install` |
| BLE gateway, dropbear, ADB, rootkeeper, guard, block, конфиги, OTA-fortress | `manual-rootfs` / `firmware` | вручную по SSH/ADB |

Правки в `/system/vendor/quasar` и `/etc` живут на read-only rootfs и
переживают перезагрузку, но не factory reset; `/data` factory reset сносит.
OTA сознательно не трогает защиту и аварийные каналы.

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

`recovery: да` — без компонента нельзя вернуться на устройство и починить
остальное (SSH, BLE, ADB, root, OTA-защита).

Неочевидное:

- dropbear — предустановленный прошивкой `/usr/sbin/dropbear`, запуск
  `/etc/init.d/S99dropbear` (копия — `projects/station/configs/station-applied/`).
  `projects/station/pulsar/ssh/` (`/data/dropbear`) на этой станции не
  используется; он нужен только для `pulsar-install --ssh`.
- Установленный BLE gateway отвечает `--version` как `0.0.0` (собран до
  `version = PACKAGE_VERSION` в `BUILD.bazel`), поэтому в манифесте он
  закреплён `deployed_sha256`.

## Сверка: `station-audit`

[`projects/station/tools/station-audit`](../projects/station/tools/station-audit)
читает манифест, открывает **одно** SSH-соединение и только читает
(`sha256sum`, `ls`, `--version`); содержимое файлов и секреты не печатает.

```bash
mise run station:audit                       # адрес из STATION_IP или secrets/NETWORK.md
mise run station:audit -- --ip 192.0.2.13
mise run station:audit -- --json
mise run station:audit -- --fail-on any
```

Адрес: `--ip` → `STATION_IP` → строка `Станция` в приватном
`secrets/NETWORK.md`. Ключ — `~/.ssh/home-lab/station_ed25519`.

| Статус | Значение |
|---|---|
| `OK` | содержимое/версия/`deployed_sha256` совпадает |
| `DRIFT` | файл есть, но sha256, версия или `deployed_sha256` отличается |
| `MISSING` | ожидаемого пути нет |
| `EXTRA` | рядом лежит `.bak`/`.prev`/`.new`/`.old`, кроме `audit.extra_allow` (`pulsar.prev`, `ble-gateway.prev`) |

`compare: version` сверяет версию с `VERSION`/`Cargo.toml`; при заданном
`deployed_sha256` авторитетен хэш, версия печатается информативно.
`normalize: ipv4` маскирует адреса: в репозитории `192.0.2.x`, на устройстве
реальный LAN. Код возврата: `0` — в пределах порога, `1` — статус из
`--fail-on` (по умолчанию `MISSING`), `2` — ошибка манифеста/SSH. В
`mise run check` не входит (нужна живая колонка); тесты —
`projects/station/tools/test_station_audit.py`.

## Обновление BLE gateway

1. Сборка:
   ```bash
   bazel build -c opt --platforms=//platform/bazel/zig:armhf_linux \
     --define=ble_gateway_git_rev=$(git rev-parse --short HEAD) \
     //projects/station/ble-gateway:ble-gateway
   bazel test //projects/station/ble-gateway/...
   ```
2. Скопировать бинарник в `/data/quasar/ble-gateway/ble-gateway`, сохранив
   текущий как `ble-gateway.prev`; `start.sh`/`stop.sh`/`status.sh` не меняются.
3. Проверить: `--dry-run --auto=false --no-button --force` → `DRY_RUN_OK`,
   затем provisioning в тестовой сети.
4. Обновить `deployed_sha256` в `manifest.yaml`, `mise run station:audit` до
   `0 DRIFT, 0 EXTRA`.
5. Откат: остановить gateway, вернуть `ble-gateway.prev`, `start.sh`.

## OTA / ADB fortress

| Механизм | Что |
|---|---|
| `activate_adb.sh` | wrapper: `deactivate` — no-op, `activate` всегда `--network` |
| `/data/quasar/rootkeeper.sh` | каждые 60 с проверяет `:5555`, поднимает adbd |
| `onboot.sh` | автозапуск keeper |
| `device_ota_update.sh` | no-op, файл обновления удаляется, reboot нет |
| `ota_script.pub` | dummy, OTA-подпись не проходит |
| `launch.json` | task `updatesd` удалён |

Применённые файлы — `projects/station/configs/station-applied/`; оригиналы на
устройстве — `/data/opt/backup/*.orig`. Корень `/` после установки — `ro`.

Проверка: `mount | grep " / "` → `ro`, `ps | grep updatesd` пусто,
`/system/vendor/quasar/ota_script.pub` заменён; после reboot ADB/SSH
поднимаются.

Откат — `projects/station/scripts/restore-ota-fortress.sh` (`--yes` для
скриптов): возвращает `.orig`, удаляет keeper/автозапуск и перезагружает
станцию. После этого ADB может быть выключен — включается стендом
`projects/station/tools/quasar-root-kit/`.

## Wi-Fi provisioning

Единственный путь настройки Wi-Fi без приложения Яндекса — BLE gateway
(UUID, клиент, кнопка, авто-режим — [`projects/station/ble-gateway/README.md`](../projects/station/ble-gateway/README.md)).
Egress guard пропускает private LAN; публичный WAN/Яндекс закрыт. При ошибке
новой сети gateway возвращает прежний профиль; если Wi-Fi пропал после окна
Bluetooth, gateway остаётся аварийным каналом. QR/DPP provisioning на
станции нет. Диагностика потери сети —
`projects/station/scripts/NETWORK-DIAGNOSTIC.md`.

Приёмка: 3 нажатия → SSID/PSK/token/commit → `CONNECTED`; профиль переживает
перезагрузку; неверный SSID → `ERROR:TIMEOUT` без потери старого профиля;
без Wi-Fi BLE включается сам через 20 с; LAN/ADB/SSH живы, Яндекс/WAN закрыты.

## Риски

- Factory reset сносит `/data` (keeper, root, SSH, guard, ключи) — защита
  исчезает. Проверенного factory-образа Midi `cucumber` нет (`yandexmidi` —
  Station 2), raw boot/recovery не сохранён; factory reset не считать unbrick.
- OTA-блок не защищает recovery/factory.

## Мониторинг

Дашборды, scrape job `station` и алерты —
[`projects/station/observability/README.md`](../projects/station/observability/README.md).

Открытые этапы — `PENDING.md`, «Станция: установка и защита от дрейфа».
