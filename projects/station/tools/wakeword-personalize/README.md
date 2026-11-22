# Персонализация wake word по приватным записям

Offline pipeline: разметка, обучение с нуля, сбор станционных wake-событий,
сравнение, калибровка и model-only deploy. Станцию меняет только явный
`deploy.py`; остальное не трогает Pulsar, HA и текущую модель. Личные WAV,
разметка, метрики и веса остаются в `local-data/wakeword/` (точка входа —
`local-data/wakeword/README.md`) и в приватном run-каталоге WSL; в Git не
коммитятся. Модель, рецепт v28, данные и результаты —
[`docs/WAKE-WORD.md`](../../../../docs/WAKE-WORD.md).

## Входные данные и разделение

- WAV + JSON из `voice-recorder` (PCM16 mono 16 kHz, блок, train/test,
  исключение, прерывание, SHA256). Изменённый после записи файл отклоняется.
- Длинная положительная дорожка — не один позитив: `align.py` выделяет
  кандидаты по энергии и распознаёт фрагменты ASR на GPU. ASR — помощник:
  `prepare.py` кладёт неподтверждённые интервалы в `review_regions`, они не
  становятся позитивами автоматически. Отрицательная метка требует
  подтверждённого содержимого, не одного ASR-текста и не предсказания самого
  детектора.
- Test-записи не участвуют в обучении и выборе checkpoint. В train-дорожке
  начало — train, конец — val, между ними временной guard
  (`validation_boundary`/`validation_guard`). Фон делится целыми дублями.
  Аугментация — только после разделения; validation использует исходный PCM.
- Однажды просмотренный test становится регрессионным набором, а не слепым
  тестом. `mined_events` (ложные срабатывания для адресного hard-negative
  mining) берутся только с обучающей отрицательной дорожки.

## Окружение

- `transcribe.py`/`align.py` — отдельный GPU ASR venv (`faster-whisper`,
  NumPy, SciPy, CUDA/cuDNN); скачивается публичная ASR-модель, записи в
  облако не уходят. `prepare.py` — стандартный Python.
- `finetune.py`/`evaluate.py` — существующее окружение microwakeword-trainer
  с checkpoint, `mww_features` и локальными патчами mmap/GPU/export; версии —
  `artifacts/wakeword/ricardo-v1/requirements.lock.txt` и `provenance.txt`.
  GPU обязателен, fallback на CPU запрещён.
- На WSL копировать весь каталог инструментов (рядом с `finetune.py` нужен
  `registry.py`). При tar-передаче с macOS отключать AppleDouble/xattrs.
- `PRIVATE_RUN`, `TRAINER` — приватные пути; в `PRIVATE_RUN/recordings/`
  заранее копируются WAV и JSON. Run-каталог после обучения не
  переиспользуется; новое обучение — новый run и новая registry version.
  Обучение — под отдельным lock/tmux, без второго процесса.

## Разметка и оценка

```sh
python transcribe.py --run "$PRIVATE_RUN" --model small
python align.py --run "$PRIVATE_RUN" --model small
python prepare.py --recordings "$PRIVATE_RUN/recordings" \
  --alignments "$PRIVATE_RUN/segmented" --asr "$PRIVATE_RUN/asr" \
  --output "$PRIVATE_RUN/manifest.json"
python evaluate.py --model "$TRAINER/ru_ricardo_mww.tflite" \
  --recordings "$PRIVATE_RUN/recordings" --output "$PRIVATE_RUN/baseline.json"
python compare.py --manifest "$PRIVATE_RUN/manifest.json" \
  --baseline "$PRIVATE_RUN/baseline.json" --candidate "$PRIVATE_RUN/candidate.json" \
  --output "$PRIVATE_RUN/comparison.json"
```

Replay моделирует порог 0.9, окно 10, warm-up 100 кадров и сброс после
срабатывания с шагом 10 мс. На станции звук идёт чанками VQE, а
HA-controller арбитрирует wake, поэтому raw detector events не равны числу
запусков ассистента. `compare.py` сопоставляет один hit одному
подтверждённому target с заданным допуском и суммирует дубли одной манеры.
`evaluate.py --cutoff/--window` исследует порог offline, не меняя живой
детектор. Сложные негативы не смешиваются с бытовым FP/ч.

## Wake-события со станции

Формат событий и кнопки HA — `docs/WAKE-WORD.md`, «Wake-события и FP/ч».
Это единственный источник настоящих бытовых false positives.

```sh
python station_events.py collect --host 192.0.2.13 --ssh-key PRIVATE_KEY \
  --dest local-data/datasets/station-events
python station_events.py review --events local-data/datasets/station-events
python station_events.py label --events local-data/datasets/station-events \
  --id EVENT_ID --label false        # false | true | missed
python station_events.py report --events local-data/datasets/station-events \
  --output PRIVATE_RUN/station-report.json
python station_events.py to-manifest --events local-data/datasets/station-events \
  --output PRIVATE_RUN/station-manifest.json
```

- `collect` тянет каталог одним `ssh host tar -cf - .` (busybox, без rsync) в
  неизменяемый снимок по UTC с `SHA256SUMS`; уже полученные `id`+SHA256
  пропускаются, неизвестная схема sidecar отклоняется, sidecar без аудио не
  принимается. `--prune-labeled` — единственное удаление на устройстве и
  только для локально проверенных размеченных файлов.
- `label` пишет локальный `labels.json`; `review` печатает неразмеченные
  подозрительные `hit` (`empty_stt`/`cancelled`).
- `report` считает часы прослушивания (дельты `stats.json`), подтверждённые
  FP/ч, подозрительные/ч, near-miss/ч и dry-run FP/ч отдельно по `model_sha256`.
- `to-manifest`: `label=false` hits и `near` → hard negatives (весь клип);
  near за ≤15 с до hit или «не услышала» без метки не берётся
  (`--no-near-negatives` убирает все near); неразмеченные
  `empty_stt`/`cancelled` берёт только `--include-suspected` (риск настоящих
  обращений); `true`/`missed` → только `review_regions`. Дни UTC делятся
  хешем на train/val/test 70/15/15; нужны и train-, и val-дни, иначе
  `finetune.py` остановится.

## Обучение

Только с нуля (`--from-scratch`, решение 56 в `docs/DECISIONS.md`):
дообучение от весов ухудшало распознавание. `--from-scratch` запрещает
`--base-weights`; BatchNorm обучается, каждая эпоха сохраняется.

```sh
python finetune.py --trainer "$TRAINER" --run "$PRIVATE_RUN" \
  --from-scratch --steps 15000 --learning-rate 0.001 \
  --config "$PRIVATE_RUN_V27/ricardo-w128.yaml" \
  --extra-features "$PRIVATE_EXTRA_FEATURES" \
  --extra-manifest "$PRIVATE_RUN/station-manifest.json" \
  --feature-mode continuous --no-freeze-batchnorm \
  --model-name NEW_MODEL --version NEW_REGISTRY_VERSION
python evaluate.py --model "$PRIVATE_RUN/NEW_MODEL_mww.tflite" \
  --recordings "$PRIVATE_RUN/recordings" --output "$PRIVATE_RUN/candidate.json"
```

- `--config` — абсолютный путь (скрипт меняет рабочий каталог).
  `--pointwise-filters 128,128,128,128` и `--first-conv-filters 64` меняют
  ширину; число ширин должно совпадать с `repeat_in_block`. Потоковый
  контракт не меняется: экспорт обязан дать `int8[1,3,40]` → `uint8[1,1]`,
  иначе отклоняется. Параметры и форма экспорта —
  `training-provenance.json`.
- `--extra-features` — `positive/negative_{train,val}.npy` дополнительного
  синтетического голоса; его негативы обязательны, чтобы тембр не стал
  признаком wake word.
- `--extra-manifest` объединяет станционные события с manifest по
  уникальному `id`; они идут отдельным bucket. Доли батча — `mixture_buckets`
  в `finetune.py`.
- `--feature-mode continuous` извлекает признаки из полной дорожки, затем
  режет окна; `reset` — только для сравнения со старым режимом.
- Выбор по `val_loss` предварительный: после экспорта все эпохи прогоняются
  непрерывной validation против активной модели, test — один раз после выбора.

## Калибровка

```sh
python calibrate.py --model PRIVATE_MODEL_DIR/ru_ricardo_mww.tflite \
  --positives PRIVATE_RUN/recordings --events local-data/datasets/station-events \
  --output PRIVATE_RUN/calibration.json --manifest PRIVATE_MODEL_DIR/ru_ricardo_mww.json
```

Таблица `(cutoff, window)`: recall на позитивах и FP/ч на подтверждённых
станционных false hits. Брать наименьший FP/ч при recall не ниже текущей
0.9/10. `--manifest` меняет только `micro.probability_cutoff` и
`micro.sliding_window_size`.

## Реестр версий

Реестр — `local-data/model-registry/ricardo/` (Mac) и
`TRAINER/training/model-registry/ricardo/` (WSL, по умолчанию для
`finetune.py`; `--registry`/`--version` переопределяют). `index.json` —
список версий, `VERSION.json` — происхождение и инвентарь, `SHA256SUMS` —
проверки. Версия неизменяема; синхронизация Mac↔WSL ручная и не заменяет
бэкап.

```sh
python registry.py --registry PRIVATE_REGISTRY --version VERSION --verify
python registry.py --registry PRIVATE_REGISTRY --version NEW_VERSION \
  --from-dir PRIVATE_EXPORT_DIRECTORY --metadata PRIVATE_METADATA_JSON
```

## Model-only deploy и откат

Профили — `projects/station/configs/wakeword-*.json` (SHA256 и путь в
private registry; порядок отката — `docs/WAKE-WORD.md`).

```sh
mise run secrets:exec -- -P station exec -- python3 \
  projects/station/tools/wakeword-personalize/deploy.py --ha-url http://192.0.2.28:8123
mise run secrets:exec -- -P station exec -- python3 \
  projects/station/tools/wakeword-personalize/deploy.py --ha-url http://192.0.2.28:8123 \
  --profile projects/station/configs/wakeword-v27.json
```

HA token — профиль fnox `station` (`HA_TOKEN_FILE`), доступ к станции —
`local-data/voice-recorder/connection.json`. Deploy отказывается при активном
будильнике или голосовом pipeline. Он сохраняет текущую пару model/manifest
в `/data/pulsar/backups/model-*`, меняет только её, делает `ctl.sh reload`,
проверяет HA Assist, MA и хэши, при неуспехе откатывается. Перезапуск идёт
без `PULSAR_LOCAL_ENDPOINTING=1` — после deploy вернуть его:
`PULSAR_LOCAL_ENDPOINTING=1 /data/pulsar/ctl.sh reload`. При сетевом отказе
смотреть журнал в `local-data/wakeword/deployments/`, а не запускать второй
deploy. `active.json` фиксирует последний выбор; установка не заменяет живую
приёмку голосом.

## Проверки

```sh
python3 -m unittest discover -s projects/station/tools/wakeword-personalize -p 'test_*.py' -v
```

Без TensorFlow/GPU и инфраструктуры.
