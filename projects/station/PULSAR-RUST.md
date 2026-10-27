# Pulsar Rust fork (Ricardo profile, not deployed)

Отдельный проект, не замена активному C++ Pulsar.

- Fork: <https://github.com/OWNER/pulsar>
- Ветка: `feat/ricardo-rust`
- База: upstream `v0.5.0` (`1934b68`)
- Локальный checkout: `projects/station/upstream/pulsar-rust/` (gitignored, свой `.git`)
- Активная станция по-прежнему работает на C++ `v0.4.1-ricardo6`; этот форк
  **не установлен** и не должен ставиться без отдельного gate.

## Что изменено относительно upstream

- on-device wake word «Рикардо» через C++ bridge + вендоренный
  microfrontend и `libtflite.so`;
- pre-roll 0.5 с в wake-событии;
- `stt-no-text-recognized` — тихая отмена, не ошибка;
- локальный command spotter выключен во время wake-запроса: «Рикардо, стоп»
  уходит в HA, чтобы остановить будильник и вернуть музыку;
- локальные команды без wake — только во время воспроизведения, как в
  `v0.4.1-ricardo6`;
- `wake_word=ricardo` и `nearest_answers=0` по умолчанию;
- Zigbee runtime отключён (`ENABLE_ZIGBEE=false`);
- обновления только с `OWNER/pulsar`, `auto_update=0`;
- модель не коммитится: `satellite/wakewords/` — локальный артефакт.

Полное описание, сборка, проверки и границы — в
`upstream/pulsar-rust/RICARDO.md` (`upstream/pulsar-rust/RICARDO.md`, локальный upstream checkout).

## Проверки

- cross-build `aarch64` и `armhf` — проходит;
- `cargo-zigbuild clippy` для обоих targets — проходит;
- `cargo test -p pulsar --bin pulsar` — 30/30 в Linux-контейнере;
- `cargo test -p pulsar-audio` — 20+ тестов, включая missing-model;
- installer `go test ./...` — pass.

Не проверялось: живой микрофон, реальная загрузка `libtflite.so` на станции,
ESPHome-сущности, Spotify/MA, откат. Это отдельная HIL-приёмка.

## Правило

Не устанавливать форк на станцию и не менять текущий `/data/pulsar` в рамках
этой задачи. Для будущего теста — отдельный backup, `pulsar.prev`, модель в
`/data/pulsar/wakewords/`, проверка `ctl.sh status/log`, откат на C++.
