# Wake word «Рикардо»

Текущий статус — `STATE.md`; сборка Pulsar — `projects/station/`.

## Модель

- On-device microWakeWord: персональная v2, FULL resolver встроенной `libtflite`.
- Threshold `0.9`, окно `10`, pre-roll `0.5 с`.
- Активный профиль — `projects/station/configs/wakeword-personal.json`;
  его SHA256 закреплён в профиле. Персональная модель и обучаемые веса —
  private в `secrets/model-registry/ricardo/v2-personal-20261029/`.
- Исходная v1 сохранена для отката в private registry и как общий baseline
  в `artifacts/wakeword/ricardo-v1/`. Профиль отката —
  `projects/station/configs/wakeword-baseline.json`.
- Активна v2: начатый откат после сообщения о регрессии v7 завершился
  до просьбы пользователя продолжить доработку. V7 сохранена для анализа;
  несмотря на улучшение суммарного offline-результата, обычная/громкая/резкая
  речь в живой работе регрессировала. Дальнейшее обучение требует отдельной
  проверки этих манер, помимо шёпота и сложных негативов.
- Кандидат `v22-scratch-puriso-selected-20261030` обучен с нуля на исходной
  синтетике, дополнительном Puriso2 и личных train-записях. Checkpoint выбран
  по непрерывной валидации, затем прошёл offline-регрессию по манерам речи.
  Все веса и модель private; нативный inference probe прошёл, живая приёмка
  ожидается. Активный профиль остаётся v2; ошибки на сложных словах ещё остаются.
- Записи и нарезанные PCM-контексты с разметкой по версиям сохранены в
  `secrets/datasets/ricardo-20261029/`, вне Git. Случайные аугментации
  сохранены как массивы признаков и воспроизводятся по рецептам/seed.
- Бинарник и device-откат — `/data/pulsar`; legacy исходники и binaries —
  private в `secrets/local/recovery/`.
- Изменяемые исходники — `projects/station/upstream/pulsar-rust/`,
  ветка `feat/ricardo-rust`.

База локального форка — tag `v0.5.2`, commit `4cba44a`; на станции установлена
сборка форка `v0.5.2-3-gc57d521` (`pulsar.prev` = `v0.5.2-2-g95ce568`;
`v0.5.0-1-g908a027` — private backup вне Git).
Снимок локальных изменений — `projects/station/patches/pulsar-rust/`;
обновляется `projects/station/scripts/snapshot-pulsar.py` перед корневым
коммитом.
ALSA ABI armhf обрабатывается runtime-обёрткой `pulsar-alsa`.

## Сборка

```bash
cd projects/station/upstream/pulsar-rust
make -C satellite -j"$(nproc)" ZIG=/path/to/zig-0.14.1
```

Нужны rustup (версия из `satellite/rust-toolchain.toml`), cargo-zigbuild
0.24.0, zig 0.14.1, make, git, curl, patch, cmake и ninja. Сборка проверена в
Linux/WSL2; результат — `satellite/build/<arch>/pulsar` и `pulsar.debug`.
Модель для профиля Ricardo кладётся в `satellite/wakewords/` по
`satellite/wakewords/README.md`.
Это путь сборки общего upstream-профиля. Персональные веса туда не
копировать: они должны оставаться в private registry, вне source snapshot.
Для персонального профиля после установки Pulsar применяется model-only
deploy из `projects/station/tools/wakeword-personalize/README.md`.

## Установка / откат

- Установка и `ctl.sh` — `OPERATIONS.md`.
- Rollback: `/data/pulsar/backups/pre-ricardo*/` и
  `/data/pulsar/rollback-ricardo.sh`.
- `ctl.sh disable` возвращает `maind`; `enable` — Pulsar.

## Проверка

- Wake → ответ HA; «Рикардо, стоп» доходит до HA и не глушит музыку локально.
- Пустой STT после wake — тихая отмена.
- Duck/resume музыки сохраняются.
- Метрики: recall/FP в тишине/шуме, latency; синтетика не заменяет микрофон.
