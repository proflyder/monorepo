# Wake word «Рикардо»

Текущий статус — `STATE.md`; сборка Pulsar — `projects/station/`.

## Модель

- On-device microWakeWord: установлен кандидат v27 (более ёмкий MixedNet);
  best-known/reference по решению пользователя — v24. Базовая v1 остаётся
  baseline и путём отката.
- Threshold `0.9`, окно `10`, pre-roll Pulsar `256 ms` (Ricardo `Fixed(8)`).
- Активный профиль — `projects/station/configs/wakeword-personal.json`
  (`v27-ambient-capacity-sel22-20261030`); его SHA256 закреплён в профиле.
  Персональные модели и обучаемые веса — private в
  `secrets/model-registry/ricardo/`, вне Git.
- Откат: `projects/station/configs/wakeword-v24.json` (best-known) и
  `projects/station/configs/wakeword-v22.json`; исходная v1 — профиль
  `wakeword-baseline.json` и `artifacts/wakeword/ricardo-v1/`.
- v27 обучена с нуля и прошла firmware-совместимость (C++ 162/164 против
  159/164 у v24 на регрессионном наборе); Python-стенд дал 161/164 одинаково
  с v24. Живая приёмка всех манер речи и независимый ambient ещё нужны;
  установка model-only не является приёмкой.
- Записи и нарезанные PCM-контексты с разметкой по версиям сохранены в
  `secrets/datasets/`, вне Git. Случайные аугментации сохранены как массивы
  признаков и воспроизводятся по рецептам/seed.
- Бинарник и device-откат — `/data/pulsar`; legacy исходники и binaries —
  private в `secrets/local/recovery/`.
- Изменяемые исходники — `projects/station/upstream/pulsar-rust/`,
  ветка `feat/ricardo-rust`.

База локального форка — tag `v0.5.2`, commit `4cba44a`; на станции установлена
сборка форка `v0.5.2-4-g9b5e86a-dirty` (SHA256 `7081ef8e…`). `pulsar.prev` —
прежний endpointing 512 ms (`0b0c48e4…`); historical
`pulsar.pre-endpoint-20261030` — `v0.5.2-3-gc57d521-dirty`; `v0.5.2-2-g95ce568`
и `v0.5.0-1-g908a027` — private backup вне Git.
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
