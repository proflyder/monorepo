# Wake word: обучение и установка

Текущие факты и проверенный статус — [STATE.md](STATE.md).

## Где лежит работа

| Содержимое | Путь в проекте |
|---|---|
| Модель, manifest, конфиг обучения, метрики и lock-файл | `artifacts/wakeword/ricardo-v1/` |
| Бинарник станции, debug symbols, исходники и откат | `artifacts/wakeword/ricardo-station/` |
| Изменяемые исходники Pulsar | `projects/station/upstream/pulsar/`, ветка `feat/ricardo-microwakeword-v041` |
| Протокол проверки станции | `logs/status-20261027-0308-wakeword.txt` |
| Обучение в WSL | [TRAINING-RICARDO-WSL.md](TRAINING-RICARDO-WSL.md) |
| Исторический ресерч | [RESEARCH-RICARDO-WAKE-WORD.md](RESEARCH-RICARDO-WAKE-WORD.md) |

## Интеграция

`audio/microwakeword.*` загружает модель через встроенную `libtflite.so`
с Yandex FULL resolver. Vendored microfrontend сохраняет исходные лицензии.
`audio/frontend.*` подаёт VQE-аудио в детектор, передаёт wake-событие и
0.5 секунды pre-roll в существующий контроллер. `controller.*` добавляет
выбор «Рикардо», сохраняя mute, арбитраж и HA pipeline.

База — точный tag v0.4.1, commit `15554498219eccd4fb4fd736cf896df60aa2bb51`.
На armhf Midi обязательно сохранить `audio/alsa_abi.h`: без него ALSA
выбирает неверные rate/period из-за совместимых ABI symbols.
Не использовать старую локальную базу `9eef9df` для сборки под эту станцию.

Порог и окно пока задаются в C++ (0.9 и 10); JSON должен им соответствовать.
Редактирование одного JSON чувствительность не меняет.

## Сборка

```bash
cd projects/station/upstream/pulsar
make -C satellite ARCH=armhf ZIG=/path/to/zig-0.14.1 \
  VERSION=v0.4.1-ricardo6 -j8 binary
```

Нужны Zig 0.14.1, make, CMake, Ninja, git, curl и patch.
Зависимости скачивает `satellite/third_party/fetch.sh`.
Архив `artifacts/wakeword/ricardo-station/pulsar-ricardo6-source.tar.gz`
содержит итоговое дерево satellite для воспроизведения независимо от ветки.
Сборка проверена в Linux-контейнере (Debian/arm64) с Zig 0.14.1: macOS
`realpath`/`nproc` не подходят для `third_party/sysroot.sh`.

В `ricardo6` локальный command spotter выключен во время wake-запроса и
арбитража, чтобы «Рикардо, стоп» доходил до HA и останавливал будильник.
Пустой STT после wake считается отменой, а не ошибкой; «отмена/передумал»
обрабатывает `StationCancel`. Duck/resume и локальные команды без wake
сохранены.

## Установка и откат

Команды и возврат к исходному Pulsar — [OPERATIONS.md](OPERATIONS.md),
раздел «Рикардо». Бинарник и модель восстанавливать из указанных выше
папок проекта, сверяя контрольные суммы.

## Следующая итерация

Реальные записи держать отдельно от обучающих, измерять recall, ложные
активации в час и задержку на микрофонах Midi. Тест на синтетических
файлах не заменяет эту оценку. Открытые задачи — [PENDING.md](PENDING.md).
