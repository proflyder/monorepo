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

## Wake-события со станции

Pulsar записывает реальные срабатывания on-device «Рикардо» в
`/data/pulsar/wake-events/` (контракт схемы 1): `hit` (подтверждённый детектор),
`near` (пик ниже порога) и `miss` (кнопка «Wake: не услышала»). Боковые JSON
содержат окно, `score_peak`, `outcome`, `model_sha256` и пользовательскую
метку. Это единственный источник настоящих бытовых false positives; синтетика
и короткие фоновые записи их не заменяют.

Цикл: `collect → review/label → report → to-manifest → finetune → evaluate →
calibrate → deploy`. Записи, метрики и манифесты остаются в `secrets/` и
приватном run-каталоге, вне Git.

```sh
python station_events.py collect --host 192.0.2.13 --ssh-key PRIVATE_KEY \
  --dest secrets/datasets/station-events
python station_events.py review --events secrets/datasets/station-events
python station_events.py label --events secrets/datasets/station-events \
  --id EVENT_ID --label false        # false | true | missed
python station_events.py report --events secrets/datasets/station-events \
  --output PRIVATE_RUN/station-report.json
python station_events.py to-manifest --events secrets/datasets/station-events \
  --output PRIVATE_RUN/station-manifest.json
```

- `collect` тянет каталог одним потоком `ssh host tar -C … -cf - .` (busybox,
  без rsync) в неизменяемый каталог-снимок по UTC с `SHA256SUMS`. Уже
  полученные `id`+SHA256 пропускаются; снимок не перезаписывается. Схема
  sidecar проверяется, неизвестная отклоняется. `stats.json` сохраняется в
  `stats/` и служит источником часов прослушивания. `--prune-labeled` —
  единственный путь удаления на устройстве: только файлы, уже проверенные
  локально и имеющие метку (`false`/`true`/`missed`); без флага устройство
  не меняется.
- `label` пишет локальный `labels.json`; оригиналы снимка не меняются.
- `review` печатает неразмеченные подозрительные `hit` с `outcome`
  `empty_stt`/`cancelled` и путями к WAV.
- `report` считает по `kind`/`label`/`outcome` и по `model_sha256`:
  часы прослушивания (дельты кумулятивного `stats.json`), подтверждённые
  false hits/час (`label=false`), подозрительные/час, near-miss/час и
  отдельно dry-run FP/ч.
- `to-manifest` копирует нужные WAV в `PRIVATE_RUN/recordings/` и пишет
  записи, совместимые с `finetune.py`:
  - `label=false` hits и `near` → отрицательные, `hard_negative=true`,
    весь 4-секундный клип отрицателен; near можно исключить
    `--no-near-negatives`;
  - неразмеченные `empty_stt`/`cancelled` по умолчанию не используются;
    `--include-suspected` добавляет их как hard negatives — риск: часть
    таких клипов содержит настоящее обращение, это осознанный компромисс;
  - `label=true`/`missed` → только `review_regions` (позиция слова внутри
    клипа неизвестна), автоматических позитивов нет.
  Дни UTC детерминированно делятся хешем на train/val/test (70/15/15), поэтому
  соседние клипы не расходятся по сплитам. Нужно, чтобы среди собранных дней
  были и train-, и val-дни (обычно не меньше трёх суток); при нехватке
  `finetune.py` явно остановится.

`finetune.py --extra-manifest PRIVATE_RUN/station-manifest.json` объединяет
записи с основным manifest по уникальному `id`. Станция идёт отдельным
bucket «station false positives» (12.5% батча вместе с личными hard negatives;
25%, если личных hard negatives нет), чтобы не растворяться в обычном фоне.
Без `--extra-manifest` прежние рецепты bucket'ов не меняются. Сочетание со
`--extra-features` и `synthetic_hard_features` пока запрещено явной ошибкой.

Калибровка порога и окна — на тех же replay-оценках, без дублирования
потоковой симуляции:

```sh
python calibrate.py --model PRIVATE_MODEL_DIR/ru_ricardo_mww.tflite \
  --positives PRIVATE_RUN/recordings --events secrets/datasets/station-events \
  --output PRIVATE_RUN/calibration.json --manifest PRIVATE_MODEL_DIR/ru_ricardo_mww.json
```

Таблица `(cutoff, window)` показывает recall на позитивах и FP/ч на
подтверждённых station false hits (часы — из `stats.json`). `--positives` —
каталог положительных записей (фоновые дорожки и записи с `excluded`
пропускаются по sidecar; id станционных событий исключаются, даже если
`to-manifest` скопировал их в тот же `recordings/`). Рекомендуется
настройка с наименьшим FP/ч, у которой recall не ниже текущей 0.9/10;
`--manifest` обновляет только `micro.probability_cutoff` и
`micro.sliding_window_size`, остальные поля модели сохраняются. Запись
манифеста не заменяет живую приёмку и deploy.

## Обучение и приёмка

Fine-tuning стартует с исходных `.weights.h5`, LR 3e-5, bounded 1000 steps,
seed 42. Четыре равных bucket: личные позитивы, прежние синтетические
позитивы, личный фон, прежние негативы. Небольшие изменения усиления и
скорости только в train; шёпот берётся из настоящих записей. Без сложных негативов лучшие веса выбираются по val_loss. При их наличии
checkpoint должен сохранять число распознанных validation-позитивов каждой
манеры речи при пороге 0.9; среди допустимых выбирается меньшее число
ошибок на сложных validation-негативах, затем val_loss. Это оконная проверка,
она не заменяет потоковый replay после экспорта. Тест не используется для
перебора настроек.

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

## Model-only deployment и откат

`projects/station/configs/wakeword-personal.json` закрепляет выбранную
персональную модель через SHA256 и private registry path, не публикуя веса.
Сейчас в нём установленный кандидат v27
(`v27-ambient-capacity-sel22-20261030`). `wakeword-v24.json` — pinned
best-known/reference и профиль быстрого отката, `wakeword-v22.json` — прежний
откат, `wakeword-baseline.json` — исходная версия для обратного переключения.
Профиль не означает приёмку: private `active.json` хранит
`live_voice_acceptance=pending` до живой голосовой проверки.

```sh
python deploy.py --ha-url http://192.0.2.28:8123
# Откат на best-known v24 тем же способом (создаёт backup текущей модели):
python deploy.py --ha-url http://192.0.2.28:8123 \
  --profile projects/station/configs/wakeword-v24.json
```

Подставьте свой HA URL. Доступ берётся из приватного connection.json
рекордера и secrets/home/ha.token; модели — из приватного registry.
Перед запуском закончите режим записи и остановите музыку. При активном
будильнике или голосовом pipeline deploy отказывается работать.
Сначала сохраняется пара текущих model/manifest в device backups, затем
меняется только эта пара и выполняется штатный ctl.sh reload. Бинарник,
его версия, ключи, firmware и конфигурация HA не обновляются. Проверяются
HA Assist и MA, hash бинарника и установленной модели; при неуспехе
предпринимается откат к сохранённой паре. При сетевом отказе смотрите
приватный журнал в secrets/model-deployments, а не запускайте второй deploy.

secrets/model-registry/ricardo/active.json фиксирует последний успешный
выбор; регистрационный VERSION.json старых архивов не переписывается.
Копия прежней модели на станции остаётся в /data/pulsar/backups/model-*.
Применение не заменяет живой приёмки голосом и проверки ложных срабатываний.

## Подтверждённые сложные негативы

Для записей слов, которые пользователь явно подтвердил как ложное
срабатывание (например, «авокадо»), manifest задаёт hard_negative=true.
Их разметка не строится по предсказаниям самого wake-детектора: иначе
его ошибки попадут в позитивы. ASR может спутать близкое слово с wake
word, поэтому решение об отрицательной метке требует подтверждённого
содержимого записи, а не одного ASR-текста.

Отдельная test-дорожка остаётся test. В обучающем дубле validation_boundary
и validation_guard задают временное разделение train/val; перекрывающиеся
окна у границы исключаются. Сложные негативы подаются отдельным bucket,
чтобы не растворяться в обычном фоне. Позитивы, шёпот и прежняя синтетика
сохраняются для защиты от регрессии.

Параметры продолжения: --base-weights PRIVATE_PARENT_WEIGHTS,
--model-name NEW_MODEL_NAME, --manifest-version NEW_NUMBER,
--learning-rate RATE, --version UNIQUE_REGISTRY_ID. Исходные checkpoints
не меняются; каждая эпоха и итоговый кандидат сохраняются отдельно.
Ранее использованный положительный test-набор становится регрессионным
набором: его нельзя выдавать за новый независимый эксперимент.

Для адресного исправления используйте mined_events — времена ложных
срабатываний исходной модели **только на обучающей отрицательной дорожке**.
Hard-negative feature extraction тогда берёт короткие PCM-контексты вокруг
этих событий с тем же reset/frontend и умеренной аугментацией, что для
позитивов. Паузы длинной дорожки не должны доминировать в этом bucket.
Train/val граница выбирается внутри содержательной части обучающего дубля
с временным guard; пустая hard-negative validation считается ошибкой.
Тестовые срабатывания не используются для mining и построения признаков.

При нескольких проверочных дублях одной манеры речи compare.py суммирует
их результаты. Исключённые в рекордере дорожки не входят в новый датасет.

## Дообучение с сохранением нормализации и акустического контекста

По умолчанию BatchNormalization заморожен до compile: его moving mean/variance
не должны подстраиваться к маленькому личному набору. Малый learning rate
сам по себе не предотвращает изменение этих статистик. Для контрольного
эксперимента доступен --no-freeze-batchnorm; изменение фиксируется в provenance.

--feature-mode continuous (по умолчанию) извлекает признаки из полной
дорожки и только затем вырезает окна. Временное отображение учитывает первое
30 мс окно microfrontend и шаг 10 мс. Аугментация применяется к полной дорожке,
времена меток пересчитываются по скорости; val использует только исходный PCM.
--feature-mode reset оставлен для воспроизводимого сравнения со старым режимом.
Тестовые дорожки не загружаются в извлечение обучающих признаков.

Выбор по коротким validation-окнам не является разрешением на установку.
Экспортированные checkpoints сравниваются на целых непрерывных validation-
участках с активной моделью, по каждой манере речи и отдельному ambient-фону.
Порог и сглаживание можно исследовать offline через evaluate.py --cutoff/--window;
это не меняет настройки живого детектора. Общий фон оценивается отдельно от
дублей сложных слов: их нельзя смешивать в оценку бытовых false accepts/hour.

ASR-разметка шёпота требует проверки: совпадение двух перекрывающихся
распознаваний усиливает свидетельство, но не заменяет ручную разметку.
Неподтверждённые участки не объявляются положительными автоматически.

Обоснование заморозки BN: [Keras fine-tuning guide](https://keras.io/guides/transfer_learning/#important-notes-about-batchnormalization-layer).
Обоснование длинного контекста: [microWakeWord training process](https://github.com/OHF-Voice/micro-wake-word#training-process).

## Обучение с нуля с дополнительным голосом

--from-scratch запрещает --base-weights: веса создаются случайно и ни один
checkpoint не загружается. BatchNorm в этом режиме обучается, поскольку
полезных исходных статистик ещё нет. Применяются SpecAugment, заданные
class weights, cosine learning-rate schedule и сохранение каждой эпохи.
Выбор предварительного checkpoint идёт по val_loss; окончательная приёмка
экспорта требует непрерывной валидации и независимой проверки записей.

## Более ёмкий MixedNet в том же потоковом контракте

`--config PRIVATE_YAML` читает другой trainer-конфиг вместо `TRAINER/ricardo.yaml`.
Ширину можно задать явно: `--pointwise-filters 128,128,128,128` и
`--first-conv-filters 64`. Остальные ключи архитектуры (MixConv-ядра, stride 3,
контекст 204×40, residual-блоки) не меняются, поэтому потоковый вход остаётся
`int8[1,3,40]`, выход — `uint8[1,1]`. Число pointwise-ширин должно совпадать
с `repeat_in_block`.

Изменение ширины требует `--from-scratch`: попытка загрузить checkpoint другой
формы завершается явной ошибкой, а не частичным восстановлением весов. `--config`
должен указывать абсолютный путь, потому что скрипт меняет рабочий каталог.
Измеренное число параметров (`model.count_params()`, не размер HDF5) и форма
экспорта пишутся в `training-provenance.json`; каждая эпоха сохраняется отдельно,
а выбор checkpoint для scratch-запуска по-прежнему идёт по val_loss, после чего
обязателен непрерывный post-hoc replay validation перед единственным test.

--extra-features PRIVATE_DIR добавляет positive_train.npy, positive_val.npy,
negative_train.npy и negative_val.npy нового синтетического голоса.
Используются только реальные hard negatives в основном manifest.
Batch 128 содержит: 16 личных позитивов, 32 исходных синтетических позитива,
16 позитивов нового голоса и по 16 личных, сложных реальных, исходных
синтетических и новых синтетических негативов. Отрицательные фразы нового
голоса обязательны, чтобы не связывать сам тембр с wake word.
Новый голос также участвует в validation и calibration; контрольные суммы
модели голоса, рецепта и признаков сохраняются private рядом с экспериментом.

```sh
python finetune.py --trainer "$TRAINER" --run "$PRIVATE_RUN" \
  --from-scratch --extra-features "$PRIVATE_EXTRA_FEATURES" \
  --steps 15000 --learning-rate 0.001 --model-name NEW_MODEL \
  --version NEW_REGISTRY_VERSION
```

## Продолжение работы в новой сессии

Приватная точка входа со всеми deployment-путями, данными, результатами,
командами и незавершёнными проверками — `secrets/WAKEWORD-HANDOFF.md`.
Каталог аудио и архивов с исходными метаданными —
`secrets/wakeword-assets-index.json`. Эти файлы остаются вне Git.
Сначала читайте актуальные `AGENTS.md` и `docs/STATE.md`, затем handoff.
Не объединяйте архивы вслепую: они могут содержать пересекающиеся записи;
разбиение конкретного эксперимента определяет его manifest.

Перед повторным обучением создайте новые run и registry version, проверьте
checksums, сохраните происхождение данных и исходники. После завершения
сохраните все веса и отчёты на сервере и Mac, обновите private handoff/index.
Исходные большие synthetic caches могут оставаться только на сервере;
локальный registry и Git не являются полным резервным образом сервера.

Приватные данные wake word собраны в `secrets/wakeword/`; начните с
`secrets/wakeword/README.md` и `HANDOFF.md`. Старые private-пути сохранены
символическими ссылками для совместимости, исходники остаются в монорепозитории.
