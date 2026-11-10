# Инструменты

## `station-audit`

Read-only сверка файлов на колонке с `projects/station/manifest.yaml`:
sha256, версии бинарников и лишние `.bak`/`.prev`/`.new` рядом. Одно SSH-
соединение, только чтение; адрес — `--ip`, `STATION_IP` или приватный
`secrets/NETWORK.md`. Запуск — `mise run station:audit -- --ip 192.0.2.13`;
описание и статусы — [`docs/STATION-SOFTWARE.md`](../../../docs/STATION-SOFTWARE.md).
Тесты — `projects/station/tools/test_station_audit.py` (без сети).

## `voice-recorder/`

Временная локальная веб-консоль записи микрофонов колонки на Mac для
датасета Ricardo. Явное переключение с отключением Pulsar на время сеанса,
SSH-поток PCM, watchdog восстановления и удаление без изменения постоянной
конфигурации. Инструкция — [voice-recorder/README.md](voice-recorder/README.md).

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
