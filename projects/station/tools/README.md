# Инструменты станции

| Инструмент | Что | Документация |
|---|---|---|
| `station-audit` | read-only сверка колонки с `projects/station/manifest.yaml` (`mise run station:audit`); тесты — `test_station_audit.py`, без сети | [`docs/STATION-SOFTWARE.md`](../../../docs/STATION-SOFTWARE.md) |
| `voice-recorder/` | временная веб-консоль записи микрофонов колонки на Mac для датасета wake word | [README](voice-recorder/README.md) |
| `wakeword-personalize/` | разметка, обучение с нуля, сбор wake-событий, сравнение и model-only deploy персональной модели; станцию меняет только `deploy.py` | [README](wakeword-personalize/README.md), модель — [`docs/WAKE-WORD.md`](../../../docs/WAKE-WORD.md) |
| `quasar-root-kit/` | root, fortress и OTA-блок через локальный HTTPS-прокси (Лайт 2, Миди, Лайт 1); логика `--ota-off`/`--fortress` и пути маркеров `/data/quasar/activate_adb_*` совпадают с текущей прошивкой | [README](quasar-root-kit/README.md) |
