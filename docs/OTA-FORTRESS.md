# OTA / ADB fortress

Текущая прошивка: `0.369.1.46.4778262955.20261013.160`.
Факты — `STATE.md`; откат — `OPERATIONS.md`.

## Установлено

| Механизм | Что |
|---|---|
| `activate_adb.sh` | wrapper: deactivate no-op, activate всегда `--network` |
| `/data/quasar/rootkeeper.sh` | каждые 60 с проверяет `:5555`, поднимает adbd |
| `onboot.sh` | автозапуск keeper |
| `device_ota_update.sh` | no-op, файл удаляется, reboot нет |
| `ota_script.pub` | dummy, OTA-подпись не проходит |
| `launch.json` | task `updatesd` удалён |

Корень `/` после установки — `ro`. Оригиналы — `/data/opt/backup/*.orig`
и `backups/station-originals-20261023/`; применённые файлы —
`projects/station/configs/station-applied/`.

## Проверка

```bash
adb -s 192.0.2.13:5555 shell 'mount | grep " / "'
adb -s 192.0.2.13:5555 shell 'ps | grep updatesd'
adb -s 192.0.2.13:5555 shell 'ls -l /system/vendor/quasar/ota_script.pub'
```

Ожидается `ro`, `updatesd` отсутствует, `ota_script.pub` заменён.
После reboot ADB/SSH должны подняться.

## Откат

```bash
projects/station/scripts/restore-ota-fortress.sh
```

Скрипт вернёт `.orig`, удалит keeper/автозапуск и перезагрузит станцию.
После reboot ADB может быть выключен — включить через fake-sync стенд.

## Риски

- Factory reset сносит `/data` (keeper, SSH-пароль, guard, ключи); защита
  исчезает.
- OTA-блок не защищает recovery/factory и не заменяет recovery image.
- Проверенного factory-образа Midi `cucumber` нет; `yandexmidi` — Station 2.
- Raw boot/recovery не сохранён; rescue — только с согласованием.
