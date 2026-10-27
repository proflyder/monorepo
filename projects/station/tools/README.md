# Инструменты

## `ble-gateway/`

Свой BLE Wi-Fi gateway для Midi (`Station-Gateway`). Go + BlueZ D-Bus GATT;
SSID/PSK/token/commit/status. Кнопка mute: 3 коротких нажатия, окно 15 минут;
авто при отсутствии Wi-Fi >20 с. Подробности и UUID — `ble-gateway/README.md`.

Сборка:

```bash
cd projects/station/ble-gateway
GOOS=linux GOARCH=arm GOARM=7 CGO_ENABLED=0 go build -trimpath \
  -ldflags='-s -w' -o ble-gateway-armhf .
```

Станция: `/data/quasar/ble-gateway/`; запуск — `station/start.sh`.

## `quasar-root-kit/`

Полный набор для root/защиты (Лайт 2, Миди, Лайт 1):

- `station-root.sh` — интерактивное меню/мастер: MITM-прокси на Python,
  DNS-подсказки, root, `--fortress`, `--ota-off`, `--backup`, `--status`,
  `--shell`, `--export`;
- `server.py` — прозрачный HTTPS-прокси для `quasar.yandex.net` с правилами
  `rules.json` (ADB + OTA block), upstream через DoH;
- `chain.pem` / `leaf.key` — сертификат/ключ eMudhra (лист + intermediate);
- `roots/aaa_root.pem`;
- `rules.json` — включение ADB, OTA-блок.

**Использование сейчас:** брать оттуда логику `--ota-off` и `--fortress`
(или запускать функции через ADB), пути маркеров
`/data/quasar/activate_adb_*` совпадают с текущей прошивкой.

## `misc/yandex-micro-kit.sh`

Скрипт «OTA off + вечный ADB» для Лайт/Микро. В нём те же три слоя:
подмена `ota_script.pub`, заглушка `device_ota_update.sh`, удаление `updatesd`
из `launch.json`, маркеры + `chattr +i`.

**Осторожно:** в нём пути `/data/quasar/activate_adb_*` (совпадают с текущей
прошивкой, но на старой 2024 были другие). Не запускать вслепую — сверять.

## `misc/rollback.sh` + `README-mini2-rollback.md`

Откат/прошивка **Станции Мини 2** (MTD/UBI, `mtd2/3/6`). **Не применять к
Миди**: у Миди eMMC/ext4 и другие разделы (`mmcblk0p*`). Можно окирпичить.

## `misc/README-mini2-rollback.md`

Инструкция по downgrade Мини 2, Amlogic USB Burning Tool, тестпоинт.
Полезно как справка, не как готовый сценарий для Миди.
