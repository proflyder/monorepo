# Wake word «Рикардо»

Текущий статус — `STATE.md`; сборка Pulsar — `projects/station/`.

## Модель

- On-device microWakeWord v1, FULL resolver встроенной `libtflite`.
- Threshold `0.9`, окно `10`, pre-roll `0.5 с`.
- Модель/manifest/метрики — `artifacts/wakeword/ricardo-v1/`.
- Бинарник и device-откат — `/data/pulsar`; legacy исходники и binaries —
  private в `secrets/local/recovery/`.
- Изменяемые исходники — `projects/station/upstream/pulsar-rust/`,
  ветка `feat/ricardo-rust`.

База — tag `v0.5.0`, commit `1934b68`. Снимок локальных изменений —
`projects/station/patches/pulsar-rust/`; обновляется
`projects/station/scripts/snapshot-pulsar.py` перед корневым коммитом.
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
