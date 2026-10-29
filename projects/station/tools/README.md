# Инструменты

## `voice-recorder/`

Временная локальная веб-консоль записи микрофонов колонки на Mac для
датасета Ricardo. Явное переключение с отключением Pulsar на время сеанса,
SSH-поток PCM, watchdog восстановления и удаление без изменения постоянной
конфигурации. Инструкция — [voice-recorder/README.md](voice-recorder/README.md).

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

## `wakeword-personalize/`

Offline разметка, подготовка, GPU fine-tuning и сравнение персональной
wakeword-модели. Станцию не меняет; данные и персональные веса остаются
приватными. Инструкция — [wakeword-personalize/README.md](wakeword-personalize/README.md).

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

## Удалённые legacy-утилиты

`misc/yandex-micro-kit.sh`, `misc/rollback.sh` и Mini 2 downgrade guide
относились к другим устройствам (Light/Micro/Mini 2) и удалены из активного
дерева; private archive — `archive/legacy/mini2/`. Не применять к Midi.
