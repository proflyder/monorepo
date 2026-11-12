# Wake word «Рикардо»

Каноническое описание модели, данных и обучения. Текущий статус — `STATE.md`;
сборка Pulsar — `projects/station/`; контракт записи событий — `RICARDO.md`;
цикл инструментов — `projects/station/tools/wakeword-personalize/README.md`.

## Модель

- On-device microWakeWord. Установлена v28
  (`v28-station-scratch-sel14-20261112`, SHA256 `a039ed2d…e05efb6e`, 207 640 Б),
  threshold `0.9`, окно `10`, pre-roll Pulsar `256 ms` (Ricardo `Fixed(8)`).
- Активный профиль — `projects/station/configs/wakeword-personal.json`; его
  SHA256 и путь к модели закреплены в профиле. Персональные модели, веса и
  происхождение — private в `local-data/wakeword/model-registry/ricardo/`
  (удобная ссылка `local-data/model-registry/ricardo/`), вне Git.
  `active.json` фиксирует последнюю установку, `index.json` — список версий.
- Откат: `wakeword-v27.json` (прежняя установленная) → `wakeword-v24.json`
  (best-known/reference) → `wakeword-v22.json` → `wakeword-baseline.json`
  (исходная v1). Сама v1 лежит в `artifacts/wakeword/ricardo-v1/`.
- Установка model-only не является приёмкой: private `active.json` хранит
  `live_voice_acceptance=pending` до живой проверки голосом.
- Бинарник и device-откат (`pulsar.prev`) — `/data/pulsar`.
- Изменяемые исходники — `projects/station/pulsar/` (монорепозиторий).

База Pulsar — upstream tag `v0.6.1` (предыдущая — `v0.5.4`, commit `f1f5b17`);
установленная версия — `docs/STATE.md`. Upstream вливается патчем (`git diff
vA..vB` → `git apply -3 --directory=projects/station/pulsar`), см.
`docs/OPERATIONS.md`.
ALSA ABI armhf обрабатывается runtime-обёрткой `pulsar-alsa`.

## Где лежат данные

Всё приватное — `local-data/wakeword/`, вне Git; точка входа —
`local-data/wakeword/README.md`, подробные пути и рецепт — `HANDOFF.md` там же.

| Что | Где |
|---|---|
| оригинальные записи и sidecar рекордера | `local-data/wakeword/recorder/` |
| исправленные датасеты, контексты, разметка | `local-data/wakeword/datasets/` |
| снимки станционных wake-событий | `local-data/datasets/station-events/` |
| кандидаты и веса эпох, provenance | `local-data/wakeword/model-registry/ricardo/` |
| staging манифестов и признаки экспериментов | `local-data/wakeword/experiments/` |
| журналы установки и отката | `local-data/wakeword/deployments/` |
| обучение (GPU) | отдельный приватный WSL-каталог; путь — в `HANDOFF.md` |

Случайные аугментации сохранены как массивы признаков и воспроизводятся по
рецептам/seed. Синтетические признаки исходной модели и дополнительного
голоса лежат только на WSL; на Mac они не дублируются.

## Проверочные данные

- **Валидация манер речи** — 32 блока (relaxed 3, normal 9, whisper 8,
  quiet 4, commands 6, sharp 2) из личных записей. По ней считается
  распознавание каждой манеры при 0.9/10.
- **Непрерывная validation** — те же дорожки целиком, а не отдельные
  короткие окна: replay потокового детектора, warm-up 100 кадров и сброс
  после срабатывания.
- **Станционный val/test-split** — подтверждённые ложные `hit` и все `near-miss`
  из `station-events` (near без разметки берутся, кроме near за ≤15 с до hit или
  «не услышала»; подозрительные hits без метки не берутся),
  разделённые по дням UTC хешем (70/15/15), чтобы соседние
  клипы не расходились по сплитам. Test-дни не участвуют в выборе эпохи.
- **Регрессионный набор** — 164 подтверждённых обращения из прежнего
  test-набора. Он многократно просмотрен, поэтому это регрессия, а не слепой
  тест.
- **Сложные негативы (test)** — отдельная test-дорожка сложных слов/фона из
  личных записей, 157,184 с; считаются события детектора. Их нельзя смешивать
  с бытовым FP/ч станции.

## Обучение v28

Правило: **только обучение с нуля**. Дообучение от готовых весов во всех
экспериментах ухудшало распознавание: v23 сохранила эпоху 0 (исходные веса),
а частичный v28-finetune после 9 эпох дал `PROTECTED_VALIDATION safe=False`.
Нельзя продолжать обучение от checkpoint, даже с малым learning rate и
замороженным BatchNorm.

Рецепт v28:

- `finetune.py --from-scratch --steps 15000 --learning-rate 0.001`;
  конфиг `ricardo-w128.yaml` — MixedNet pointwise 128×4, first conv 64,
  stride 3, контекст 204×40;
- `--extra-features` — синтетический голос Puriso2, `--feature-mode continuous`,
  `--no-freeze-batchnorm`;
- `--extra-manifest` — станционные события;
- early stopping на эпохе 21 (2100 шагов).

Смесь батча 128: 12.5% личные позитивы, 25% исходные синтетические позитивы,
12.5% Puriso-позитивы, 12.5% личный фон, 6.25% личные hard negatives
(YouTube + клавиатура), 6.25% станционные ложные, 12.5% исходные
синтетические негативы, 12.5% Puriso-негативы. Проценты задаёт
`mixture_buckets` в `finetune.py`.

Данные v28:

- manifest v27-run: 74 трека — личные записи рекордера и ambient-repair
  YouTube/клавиатура как hard negatives;
- станционный manifest (`station_events.py to-manifest`, без
  `--include-suspected`): 39 confirmed false + 289 near-miss; split по дням —
  train 206 клипов (03, 05–09.11), val 63 (02, 10.11), test 59 (01, 04.11);
- 24 подозрительных (`empty_stt`/`cancelled`) остались без метки, ещё 10 near
  после hit/miss не использованы;
- снимки `local-data/datasets/station-events` (`20261104T080941Z`,
  `20261110T195906Z`), суммарно 215,36 ч прослушивания.

Выбор эпохи: post-hoc непрерывная validation всех 21 сохранённых эпох против
v27. Критерий — распознавание каждой манеры речи не ниже v27, затем минимум
FP на station-val и ambient-val, затем больше matched, затем более ранняя
эпоха. Выбрана эпоха 14. Test (станционный test-split и регрессия 164)
прогнан один раз после выбора.

Результаты v27 → v28 (офлайн):

| Проверка | v27 | v28 |
|---|---|---|
| validation по манерам | 32/32 | 32/32 |
| станционный val, клипов с FP | 7 | 2 |
| станционный test, клипов с FP | 13 | 1 |
| регрессия 164 | 161 | 164 |
| hard-negative test events | 15 | 14 |
| FP/ч на 39 confirmed (`calibrate.py`) | ~0.14 | ~0.01 |

Ambient-val дал 2 → 3 клипа с FP; суммарно station-val + ambient-val
улучшился с 9 до 5. Станционный train дал 21 → 0 срабатываний, но это
обучающие клипы, а не оценка. Числа офлайн и не заменяют живую приёмку.

## Запись данных продолжается

На станции включена запись (`state.conf`: `wake_event_recording=1`), Pulsar
пишет `/data/pulsar/wake-events/` (`hit`/`near`/`miss`, `stats.json`). На
устройстве ничего не удалялось: `collect --prune-labeled` не использовался.
События после установки v28 (2026-11-12 ~16:12 UTC) несут
`model_sha256=a039ed2d…`; `report` группирует FP/ч по модели, поэтому v27 и
v28 не смешиваются.

Кнопка «Рикардо: не услышала» (окно 30 с) есть в приложении Home и в HA.
На 2026-11-12 нажатий не было. Цикл сбора: `collect` → `review`/`label` →
`report` → `to-manifest` → обучение → `calibrate`; команды —
`docs/OPERATIONS.md`, контракт — `RICARDO.md`.

## Сборка

```bash
bazel build -c opt --platforms=//platform/bazel/zig:armhf_linux \
  --define=pulsar_version="$(tr -d '[:space:]' < projects/station/pulsar/VERSION)" \
  //projects/station/pulsar/satellite:station_binary
```

Нужен Bazelisk; Rust 1.99.1, zig 0.14.1 и sendspin-cpp скачивает сам Bazel по
`MODULE.bazel`. Сборка проверена в Linux/WSL2; результат —
`bazel-bin/projects/station/pulsar/satellite/pulsar` и `pulsar.debug` (CI
копирует их в `satellite/build/<arch>/`).
Модель для профиля Ricardo кладётся в `satellite/wakewords/` по
`satellite/wakewords/README.md`.
Это путь сборки общего upstream-профиля. Персональные веса туда не
копировать: они должны оставаться в private registry, вне source snapshot.
Для персонального профиля после установки Pulsar применяется model-only
deploy из `projects/station/tools/wakeword-personalize/README.md`.

## Установка / откат

- Установка и `ctl.sh` — `OPERATIONS.md`. Быстрый откат (HA token из OpenBao):
  `mise run secrets:exec -- -P station exec -- python3
  projects/station/tools/wakeword-personalize/deploy.py --ha-url
  http://192.0.2.28:8123 --profile projects/station/configs/wakeword-v27.json`.
  `deploy.py` сохраняет пару v28 в `/data/pulsar/backups/` (backup v27 при
  установке v28 — `model-20261112T161203Z-3a2e7fd6`).
- `deploy.py` перезапускает Pulsar без `PULSAR_LOCAL_ENDPOINTING=1`; после
  установки вернуть его: `PULSAR_LOCAL_ENDPOINTING=1 /data/pulsar/ctl.sh reload`.
- Rollback бинарника — `pulsar.prev` (`OPERATIONS.md`).
- `ctl.sh disable` возвращает `maind`; `enable` — Pulsar.

## Wake-события и FP/ч

Pulsar пишет реальные события детектора в `/data/pulsar/wake-events/`
(контракт схемы 1): `hit` — подтверждённое срабатывание (3 с до + 1 с после),
`near` — пик ниже порога, `miss` — кнопка «Wake: не услышала» (30 с до).
Это тот же PCM, что слышит детектор, без усиления. Рядом `stats.json`:
часы активного прослушивания, счётчики и `model_sha256`.

Сущности HA (устройство «Яндекс Станция Миди»): кнопки «Wake: ложное
срабатывание» (`button.iandeks_stantsiia_midi_wake_lozhnoe_srabatyvanie`) и
«Wake: не услышала» (`button.iandeks_stantsiia_midi_wake_ne_uslyshala`),
переключатели «Wake: запись событий» и «Wake: тестовый режим (без ответа)».
«Ложное срабатывание» помечает последний hit за 10 минут как `false`;
«не услышала» сохраняет последние 30 с как `miss`. Если «Рикардо» не
ответил, нажмите «Рикардо: не услышала» в приложении Home (или эту кнопку
в HA) в течение ~20 с, пока момент обращения ещё попадает в окно записи.
Запись выключена только явным переключателем; dry-run пишет `outcome=dry_run`
и не запускает ассистента.

Основная метрика — подтверждённые ложные срабатывания на час прослушивания
(FP/ч), отдельно от подозрительных (пустой STT/отмена) и near-miss. На v27 за
215,36 ч это 0,18 подтверждённых FP/ч. Сбор, разметка, отчёт, manifest и
калибровка порога — `station_events.py` и `calibrate.py`, цикл описан в
`../projects/station/tools/wakeword-personalize/README.md`. Записи и отчёты
остаются в `local-data/`, вне Git.

## Проверка

- Wake → ответ HA; «Рикардо, стоп» доходит до HA и не глушит музыку локально.
- Пустой STT после wake — тихая отмена.
- Duck/resume музыки сохраняются.
- Метрики: recall/FP в тишине/шуме, latency; синтетика не заменяет микрофон.
