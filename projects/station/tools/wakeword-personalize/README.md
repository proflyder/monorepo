# Персонализация wake word по приватным записям

Отдельный offline pipeline. Не изменяет Pulsar, HA, текущую модель или
параметры станции. Результат — кандидат для отдельной живой приёмки.
Личные WAV, транскрипты, annotations, метрики и персональные веса остаются
в `secrets/` на Mac и в приватном training/run каталоге своего WSL.
Не коммитить их даже при успешных метриках.

## Входные данные

WAV + JSON из `voice-recorder`: PCM16 mono 16 kHz, блок, train/test,
исключение, признак прерывания и SHA256. Оригиналы не нормализуются и
не перезаписываются. Изменённый после записи файл отклоняется.

Длинную положительную дорожку нельзя объявлять одним положительным
примером: её тишина и команды не являются wake word. `align.py` выделяет
кандидаты по энергии и распознаёт короткие фрагменты на GPU. Транскрипция
длинного повтора одного слова ненадёжна: модель может свести повторения
к одному слову или выдать слова с нулевой длительностью.

ASR используется как помощник разметки, не как эталон. `prepare.py` сохраняет
неподтверждённые интервалы в `review_regions`. Они не становятся позитивами
автоматически; для полной оценки их нужно проверить вручную. Процент
распознавания на подтверждённом поднаборе нельзя выдавать за результат на
всех произнесениях пользователя.

## Разделение данных

- Исходные test-записи не используются для обучения или выбора checkpoint.
- В каждой train-дорожке начало идёт в train, последний участок — в val;
  между ними временной guard, чтобы окна соседних обращений не пересекались.
- Фон делится целыми независимыми дублями между train/val/test. Найденные
  потенциальные wake word исключаются из негативных окон с запасом.
- Аугментация выполняется только после разделения. Проверочные записи
  не изменяются. Нельзя случайно делить соседние перекрывающиеся окна на
  train и test.

## Окружение

Для `transcribe.py`/`align.py` нужен отдельный GPU ASR venv:
`faster-whisper`, NumPy, SciPy и совместимые CUDA/cuDNN библиотеки.
Скачивается публичная ASR-модель; записи в облако не отправляются.
Для подготовки manifest достаточно стандартного Python.

`finetune.py` и `evaluate.py` запускаются в прежнем проверенном окружении
microwakeword-trainer. Версии оригинального обучения закреплены в
`artifacts/wakeword/ricardo-v1/requirements.lock.txt` и `provenance.txt`.
Нужен существующий trainer с checkpoint, mww_features и локальными патчами
mmap/GPU/export. Новые веса не восстанавливаются из одного TFLite.
GPU для обучения обязателен; скрытый fallback на CPU запрещён.

## Команды

Параметры `PRIVATE_RUN` и `TRAINER` — локальные приватные пути своего
окружения. В `PRIVATE_RUN/recordings/` заранее копируются WAV и JSON;
не переносить весь каталог secrets. На macOS при tar-передаче отключайте
AppleDouble/xattrs или игнорируйте служебные `._*` файлы.

```sh
python transcribe.py --run "$PRIVATE_RUN" --model small
python align.py --run "$PRIVATE_RUN" --model small
python prepare.py --recordings "$PRIVATE_RUN/recordings" \
  --alignments "$PRIVATE_RUN/segmented" --asr "$PRIVATE_RUN/asr" \
  --output "$PRIVATE_RUN/manifest.json"
python evaluate.py --model "$TRAINER/ru_ricardo_mww.tflite" \
  --recordings "$PRIVATE_RUN/recordings" --output "$PRIVATE_RUN/baseline.json"
python finetune.py --trainer "$TRAINER" --run "$PRIVATE_RUN" --steps 1000
python evaluate.py --model "$PRIVATE_RUN/ru_ricardo_personal_v1_mww.tflite" \
  --recordings "$PRIVATE_RUN/recordings" --output "$PRIVATE_RUN/candidate.json"
python compare.py --manifest "$PRIVATE_RUN/manifest.json" \
  --baseline "$PRIVATE_RUN/baseline.json" --candidate "$PRIVATE_RUN/candidate.json" \
  --output "$PRIVATE_RUN/comparison.json"
```

Запускать обучение под отдельным lock/tmux, исключив двойной процесс.
На WSL копируйте весь каталог инструментов, а не один finetune.py:
рядом с ним нужен registry.py. Архивация модели — часть успешного
завершения обучения.
Run-каталог после обучения не переиспользуется для нового эксперимента:
сохраняйте исходные результаты и создавайте новый run. Изменение исходных
файлов/модели ASR требует нового cache/run, а не использования старых меток.

## Обучение и приёмка

Fine-tuning стартует с исходных `.weights.h5`, LR 3e-5, bounded 1000 steps,
seed 42. Четыре равных bucket: личные позитивы, прежние синтетические
позитивы, личный фон, прежние негативы. Небольшие изменения усиления и
скорости только в train; шёпот берётся из настоящих записей. Лучшие веса
выбираются по val_loss, тест не используется для перебора настроек.

Экспорт обязан дать int8 `[1,3,40]` -> uint8. Непотоковый/неправильно
квантизованный fallback отклоняется. Calibration использует старые и новые
train features. JSON arena size исходного экспортера не является измерением
RAM для TFLite Micro.

Replay моделирует порог 0.9, окно 10, warm-up 100 frames и сброс состояния
после срабатывания. Сравниваются одинаковые PCM. Гранулярность replay 10 мс;
на станции audio поступает чанками VQE, и HA-controller дополнительно
блокирует/арбитрирует wake, поэтому raw detector events не равны числу
запущенных ассистентов. Границы ASR имеют погрешность: compare сопоставляет
один hit одному подтверждённому target с явно заданным временным допуском.

Нулевые false positives на нескольких минутах фона не доказывают их
отсутствие в быту. Нужны более долгий независимый фон и живая проверка
голосом, включая реальные проблемные звуки. Установка/откат — отдельное
действие; этот pipeline автоматически ничего на колонку не ставит.

## Реестр версий и checkpoint

Основной приватный реестр на Mac: `secrets/model-registry/ricardo/`.
В нём отдельные каталоги исходной модели, персональных кандидатов и
технических smoke-моделей. `index.json` — список версий, `VERSION.json`
внутри каждой версии — происхождение и инвентарь, `SHA256SUMS` — проверки
файлов и самого manifest. Персональные веса не публикуются в GitHub.

При следующих обучениях finetune.py сохраняет веса каждой завершённой
эпохи в `model/epochs/`, отдельно лучшие/финальные веса и, после успешного
экспорта, регистрирует новую неизменяемую версию. Путь по умолчанию на
WSL — `TRAINER/training/model-registry/ricardo/`; переопределение:
`--registry PRIVATE_PATH --version UNIQUE_VERSION_ID`. На Mac копию
такого нового архива нужно отдельно синхронизировать и проверить:
автоматической постоянной репликации между компьютерами нет.

```sh
python registry.py --registry PRIVATE_REGISTRY --version VERSION --verify
python registry.py --registry PRIVATE_REGISTRY --version NEW_VERSION \
  --from-dir PRIVATE_EXPORT_DIRECTORY --metadata PRIVATE_METADATA_JSON
```

Зарегистрированная версия не перезаписывается: новый эксперимент получает
новый ID. Копируются веса, модель, параметры, исходники trainer/fine-tuning,
разметка и метрики; проверяются SHA256. Исходные данные не перемещаются.
Для старых обучений доступны только действительно найденные checkpoints;
промежуточные эпохи, ранее не сохранявшиеся, восстановить нельзя.
Оригинальный baseline сохраняется для отката. Наличие двух копий на Mac
и WSL не заменяет независимую резервную копию.

## Локальные проверки

```sh
python3 -m unittest discover -s projects/station/tools/wakeword-personalize -p 'test_*.py' -v
```

Тесты не требуют TensorFlow/GPU и не обращаются к инфраструктуре.
