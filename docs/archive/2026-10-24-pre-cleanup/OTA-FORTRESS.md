# OTA-запрет и fortress (установлено)

Дата: **2026-10-23 20:15 MSK**. Прошивка: `0.369.1.46.4778262955.20261013.160`.

## Что установлено

| Файл / механизм | Что сделано |
|---|---|
| `/system/vendor/quasar/activate_adb.sh` | Обёртка **FORTRESS-WRAPPER v2**: `deactivate` → no-op, `activate` → всегда `--network`. Оригинал: `activate_adb.sh.orig`. |
| `/data/quasar/rootkeeper.sh` | **KEEPER v4** (rw-раздел): каждые 60 с проверяет LISTEN на `:5555`; если adbd нет — вызывает `activate_adb.sh --network` и **явно** запускает `/usr/bin/adbd` через `start-stop-daemon`. |
| `/system/vendor/quasar/onboot.sh` | В конец добавлен автозапуск keeper. Оригинал: `onboot.sh.orig`. |
| `/system/vendor/quasar/device_ota_update.sh` | Заглушка **FORTRESS-NO-OTA**: удаляет переданный файл, пишет в лог, `exit 0`, без reboot. Оригинал: `.orig`. |
| `/system/vendor/quasar/ota_script.pub` | Подменён dummy-ключом: подпись любого OTA не пройдёт. Оригинал: `.orig`. |
| `/system/vendor/quasar/launch.json` | Удалён task `updatesd`; JSON провалидирован (`--mediad`, `--platformd`, без `--updatesd`). Оригинал: `.orig`. |

Корень `/` после установки снова `ro`.

## Бэкапы

- На колонке: `/data/opt/backup/*.orig` (activate_adb, device_ota_update,
  ota_script.pub, launch.json, onboot.sh).
- На Mac: `backups/station-originals-20261023/`.
- Применённые файлы: `configs/station-applied/`.

## Проверки (все прошли)

1. `adb reconnect`, ребут → `:22` и `:5555` открылись сами; SSH-пароль работает.
2. `activate_adb.sh --quasmodrom deactivate` → `rc=0`, adbd жив, маркеры на месте.
3. `device_ota_update.sh /tmp/ota.test` → `rc=0`, файл удалён, **reboot не произошёл**.
4. `ps | grep updatesd` → **none**; `launch.json` → `updatesd=0`.
5. `ota_script.pub` ≠ `.orig` (REPLACED); `device_ota_update.sh` содержит `FORTRESS`.
6. Убили `adbd` → keeper поднял его за ≤60 с, `:5555` вернулся (PID 1633).
7. `mount | grep ' / '` → `ro`.

## Важный нюанс

Первая версия keeper вызывала только `activate_adb.sh --network`. На этой
прошивке `yandex-debugctl` отвечает «Required interfaces are already
configured» и **не запускает adbd заново**, поэтому keeper v4 добавлен
явный `start-stop-daemon -S -b -x /usr/bin/adbd`. После этого восстановление
adbd проверено.

## Как откатить

Скрипт:

```bash
~/Desktop/yandex-station-lab/scripts/restore-ota-fortress.sh
# или с --yes без подтверждения
```

Он вернёт `.orig`, удалит keeper и его автозапуск, перезагрузит колонку.
После ребута ADB может быть выключен — включить fake-sync/стендом или SSH.

Вручную (ADB/SSH, root):

```sh
mount -o remount,rw /
cd /system/vendor/quasar
cp activate_adb.sh.orig activate_adb.sh
cp device_ota_update.sh.orig device_ota_update.sh
cp ota_script.pub.orig ota_script.pub
cp launch.json.orig launch.json
cp onboot.sh.orig onboot.sh
chmod 755 activate_adb.sh device_ota_update.sh onboot.sh
pkill -f rootkeeper.sh
rm -f /data/quasar/rootkeeper.sh
sync
mount -o remount,ro /
reboot
```

## Остаточные риски

- Factory reset сносит `/data` (keeper, пароль SSH, ключи) — защита исчезает;
  повторять установку.
- Если будущий OTA использует другой вход (не `device_ota_update.sh`),
  no-op может не сработать; ключ `ota_script.pub` и отсутствие `updatesd`
  остаются слоями защиты.
- Не обновлять прошивку, пока не сделан осознанный `restore`.

## Дополнительно: фейковый OTA-сервер и телеметрия (23.10)

- В `docker-compose.yml` выставлено `ALLOW_UPDATES=false`: `quasar-server`
  больше не предлагает обновления, `/check_updates` → `{"hasUpdate":false}`.
- В `https-proxy/generate.sh` для `*.s3.yandex.net` и
  `*.svc.cdn.yandex.net` вместо `proxy_pass https://$host` стоит `return 403`:
  OTA/CDN-ссылки не уходят в настоящий Яндекс.
- Телеметрия: task `telemetry` удалён из `launch.json`; `clck`,
  `report.appmetrica`, `startup.mobile` и `/quasar_metrics` → `204`.
- Старые файлы: `backups/pre-telemetry-20261023-2223/`.

## Factory/recovery (важно)

- Проверенного factory-образа Midi `cucumber` **нет**. В `artifacts/firmware/`
  лежит `yandexmidi-0.372...298.zip` — это **Station 2**, не Midi; флешить нельзя.
- Raw-дампа `recovery` (192 МиБ) / `aml_sysrecovery` (1.5 ГиБ) / bootloader нет.
- OTA-блок защищает рабочую систему, но **не recovery/factory** и не
  immutable boot. Factory reset снесёт `/data`, root/SSH/OTA-скрипты и может
  не снять серверный лок/`stolen`.
- Для реального rescue-пути нужен read-only дамп разделов и проверка recovery
  на запасной Midi (по согласованию).
