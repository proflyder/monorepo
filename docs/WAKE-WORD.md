# Wake word «Рикардо»

Модель, данные, обучение и установка. Статус — `STATE.md`; команды
инструментов — `projects/station/tools/wakeword-personalize/README.md`;
контракт записи событий — `projects/station/pulsar/RICARDO.md`.

## Модель

- On-device microWakeWord. Установлена v28
  (`v28-station-scratch-sel14-20261112`, SHA256 `a039ed2d…e05efb6e`),
  threshold `0.9`, окно `10`, pre-roll Pulsar `PrerollRule::Fixed(8)`.
- Активный профиль — `projects/station/configs/wakeword-personal.json` (SHA256
  и путь к модели в private registry). Веса и происхождение —
  `local-data/wakeword/model-registry/ricardo/` (ссылка
  `local-data/model-registry/ricardo/`), вне Git; `active.json` — последняя
  установка, `index.json` — список версий.
- Откат: `wakeword-v27.json` → `wakeword-v24.json` (best-known/reference) →
  `wakeword-v22.json` → `wakeword-baseline.json` (исходная v1, сама модель —
  `artifacts/wakeword/ricardo-v1/`).
- Установка model-only не является приёмкой: `active.json` хранит
  `live_voice_acceptance=pending` до живой проверки голосом.

## Где лежат данные

Всё приватное — `local-data/wakeword/`, вне Git; точка входа —
`local-data/wakeword/README.md`, пути и рецепт — `HANDOFF.md` там же.

| Что | Где |
|---|---|
| записи и sidecar рекордера | `local-data/wakeword/recorder/` |
| датасеты, контексты, разметка | `local-data/wakeword/datasets/` |
| снимки станционных wake-событий | `local-data/datasets/station-events/` |
| кандидаты, веса эпох, provenance | `local-data/wakeword/model-registry/ricardo/` |
| staging манифестов и признаки | `local-data/wakeword/experiments/` |
| журналы установки и отката | `local-data/wakeword/deployments/` |
| обучение (GPU) | приватный WSL-каталог; путь — в `HANDOFF.md` |

Аугментации сохранены как признаки и воспроизводятся по рецептам/seed;
синтетические признаки исходной модели и дополнительного голоса лежат только
на WSL.

## Проверочные данные

- **Валидация манер речи** — 32 блока (relaxed 3, normal 9, whisper 8,
  quiet 4, commands 6, sharp 2) из личных записей, распознавание каждой
  манеры при 0.9/10; прогон — непрерывный replay целых дорожек.
- **Станционный val/test** — подтверждённые ложные `hit` и `near-miss` из
  `station-events`, делятся по дням UTC (70/15/15). Test-дни не участвуют в
  выборе эпохи.
- **Регрессия** — 164 подтверждённых обращения прежнего test-набора
  (многократно просмотрен, это не слепой тест).
- **Сложные негативы (test)** — отдельная дорожка сложных слов/фона,
  157,184 с; не смешивается с бытовым FP/ч.

## Обучение v28

Только с нуля (решение 56): дообучение от весов ухудшало распознавание даже
с малым LR и замороженным BatchNorm.

- `finetune.py --from-scratch --steps 15000 --learning-rate 0.001`, конфиг
  `ricardo-w128.yaml` (MixedNet pointwise 128×4, first conv 64, stride 3,
  контекст 204×40), `--extra-features` — синтетический голос Puriso2,
  `--extra-manifest` — станционные события, `--feature-mode continuous
  --no-freeze-batchnorm`; early stopping на эпохе 21.
- Батч 128: 12.5% личные позитивы, 25% исходные синтетические позитивы,
  12.5% Puriso-позитивы, 12.5% личный фон, 6.25% личные hard negatives,
  6.25% станционные ложные, 12.5% исходные синтетические негативы, 12.5%
  Puriso-негативы.
- Данные: manifest v27-run (74 трека); станционный manifest без
  `--include-suspected` — 39 confirmed false + 289 near-miss (train 206,
  val 63, test 59 клипов); снимки `20261104T080941Z`, `20261110T195906Z`,
  215,36 ч прослушивания.
- Выбор эпохи: непрерывная validation всех 21 эпох против v27 — каждая манера
  не ниже v27, затем минимум FP на station-val и ambient-val, затем больше
  matched, затем более ранняя эпоха. Выбрана 14; test прогнан один раз.

| Проверка (офлайн) | v27 | v28 |
|---|---|---|
| validation по манерам | 32/32 | 32/32 |
| станционный val, клипов с FP | 7 | 2 |
| станционный test, клипов с FP | 13 | 1 |
| регрессия 164 | 161 | 164 |
| hard-negative test events | 15 | 14 |
| FP/ч на 39 confirmed (`calibrate.py`) | ~0.14 | ~0.01 |

Ambient-val: 2 → 3 клипа с FP (station-val + ambient-val: 9 → 5). Офлайн-числа
не заменяют живую приёмку.

## Wake-события и FP/ч

При `wake_event_recording=1` в `state.conf` Pulsar пишет в
`/data/pulsar/wake-events/` (схема 1) тот же PCM, что слышит детектор:
`hit` — срабатывание (3 с до + 1 с после), `near` — пик ниже порога, `miss` —
кнопка «не услышала» (30 с до). `stats.json` — часы прослушивания, счётчики,
`model_sha256`; `report` группирует FP/ч по модели.

Сущности HA (устройство «Яндекс Станция Миди»): кнопки «Wake: ложное
срабатывание» (`button.iandeks_stantsiia_midi_wake_lozhnoe_srabatyvanie`,
помечает последний hit за 10 минут как `false`) и «Wake: не услышала»
(`button.iandeks_stantsiia_midi_wake_ne_uslyshala`), переключатели «Wake:
запись событий» и «Wake: тестовый режим (без ответа)» (dry-run пишет
`outcome=dry_run`). «Рикардо: не услышала» в приложении Home или HA нажимать
в течение ~20 с, пока обращение попадает в окно.

Основная метрика — подтверждённые FP на час прослушивания, отдельно от
подозрительных (пустой STT/отмена) и near-miss. Цикл: `collect` →
`review`/`label` → `report` → `to-manifest` → обучение → `calibrate` →
`deploy.py`.

## Сборка Pulsar

```bash
bazel build -c opt --platforms=//platform/bazel/zig:armhf_linux \
  --define=pulsar_version="$(tr -d '[:space:]' < projects/station/pulsar/VERSION)" \
  //projects/station/pulsar/satellite:station_binary
```

Rust (`rust-toolchain.toml`), zig и sendspin-cpp Bazel скачивает по
`MODULE.bazel`; сборка — Linux/WSL2. Результат —
`bazel-bin/projects/station/pulsar/satellite/pulsar` и `pulsar.debug`.
Модель общего профиля кладётся в `satellite/wakewords/` по его README;
персональные веса туда не копируются — они ставятся model-only deploy.

## Установка и откат

- Модель — `deploy.py` (`wakeword-personalize/README.md`, «Model-only deploy
  и откат»); после него вернуть `PULSAR_LOCAL_ENDPOINTING=1 /data/pulsar/ctl.sh reload`.
- Бинарник — `pulsar.prev` (`OPERATIONS.md`); `ctl.sh disable` возвращает
  `maind`, `enable` — Pulsar.

## Приёмка

- Wake → ответ HA; «Рикардо, стоп» доходит до HA и не глушит музыку локально.
- Пустой STT после wake — тихая отмена; duck/resume музыки сохраняются.
- Recall/FP в тишине и шуме, latency — на живом микрофоне, синтетика не
  заменяет.
