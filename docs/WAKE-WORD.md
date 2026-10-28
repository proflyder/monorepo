# Wake word «Рикардо»

Текущий статус — `STATE.md`; сборка Pulsar — `projects/station/`.

## Модель

- On-device microWakeWord v1, FULL resolver встроенной `libtflite`.
- Threshold `0.9`, окно `10`, pre-roll `0.5 с`.
- Модель/manifest/метрики — `artifacts/wakeword/ricardo-v1/`.
- Бинарник/исходники/откат — `artifacts/wakeword/ricardo-station/`.
- Изменяемые исходники — `projects/station/upstream/pulsar/`,
  ветка `feat/ricardo-microwakeword-v041`.

База — tag `v0.4.1`, commit `15554498`; на armhf обязателен `audio/alsa_abi.h`.

## Сборка

```bash
cd projects/station/upstream/pulsar
make -C satellite ARCH=armhf ZIG=/path/to/zig-0.14.1 \
  VERSION=v0.5.0 -j8 binary
```

Нужны Zig 0.14.1, make, CMake, Ninja, git, curl, patch. Сборка проверена в
Linux-контейнере Debian/arm64; macOS `realpath`/`nproc` не подходят.

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
