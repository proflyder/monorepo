# «Рикардо» на Station Midi

Текущая установка: **Pulsar v0.4.1-ricardo6**, 27 октября 2026, 04:32 (+05).
SHA256: `ebe3e47e82e3e39e0cb014a9c487861986e3c0843dbe9bee610feb6e77fc8e95`.
Предыдущие: `v0.4.1-ricardo5` и `v0.4.1-ricardo4` (сохранены для отката).
Основа исходников — точный релиз v0.4.1, commit
`15554498219eccd4fb4fd736cf896df60aa2bb51`.
Ветка: `feat/ricardo-microwakeword-v041` в `resources/pulsar`.

## Что работает

- microWakeWord непосредственно на колонке; постоянной передачи микрофона на
  сервер нет.
- VQE/эхоподавление, аппаратный и программный mute, штатные команды, арбитраж
  и HA Assist сохраняются.
- В меню wake word добавлено «Рикардо»; остаются «Алиса» и «Яндекс».
- Последние 0,5 секунды аудио перед срабатыванием передаются в Assist, чтобы
  не обрезать начало команды.
- Модель `ru_ricardo_mww.tflite` лежит в `/data/pulsar/wakewords/`.
- Runtime — встроенная `libtflite.so` с FULL resolver; признаки соответствуют
  microfrontend 30/10 мс, 40 полос.
- Порог 0.9 и окно 10 сейчас заданы в C++ и соответствуют manifest; простое
  редактирование JSON порог не меняет.

## Что изменено в ricardo6

- Локальный command spotter (`стоп`, `пауза`, `громче`, …) выключен на время
  wake-запроса и арбитража. Поэтому «Рикардо, стоп» уходит в Home Assistant,
  а не приглушает/останавливает музыку на колонке.
- HA `StationAlarmStop` останавливает будильник, `alarm_restore` возвращает
  source/volume, `alarm_resume` продолжает музыку. Без wake-слова локальные
  команды и barge-in по-прежнему работают.
- «Рикардо, отмена/передумал/не надо» — HA `StationCancel`, без side effects.
  `stt-no-text-recognized` после wake теперь тихая отмена: без красной
  ошибки, музыка возвращается.
- Duck/resume музыки на время ответа/объявления сохранён (`kDuckGain=0.15`).

## Проверки

- ARM-сборка и `git diff --check` прошли; Docker Linux/Zig 0.14.1.
- На библиотеке самой Midi: тишина без срабатываний, 12/12 синтетических
  обращений, 0/24 негативных примеров.
- Драйвер подтверждает 96 kHz, 8 каналов, period 3072; сохранён
  `audio/alsa_abi.h` из точного v0.4.1.
- Пользователь подтвердил wake → ответ HA; логи подтвердили
  wake → Assist → TTS. Это проверка работоспособности, не измерение recall/FP.
- v0.4.1-ricardo6 установлен и запущен; HA/MA переподключились.
- Текстовый alarm start → stop → resume проверен на Spotify Connect: source
  вернулся на `Spotify Connect`, музыка продолжилась, `alarm_ringing=off`.
- Текстовые «отмена» и «передумал» дают `StationCancel` → «Хорошо.», без
  изменения музыки/source.
- **Живой микрофонный тест «Рикардо, стоп» и «Рикардо, отмена» ещё нужен.**

## Артефакты

| Файл | Назначение |
|---|---|
| `pulsar-v0.4.1-ricardo6-armhf.bin` | текущий установленный бинарник |
| `pulsar-v0.4.1-ricardo6.debug` | debug symbols |
| `pulsar-ricardo6-source.tar.gz` | итоговое дерево `satellite/` без build/третьих исходников |
| `pulsar-v0.4.1-ricardo5-*`, `pulsar-ricardo5-source.tar.gz` | предыдущая версия для отката |
| `pulsar-v0.4.1-ricardo4-*`, `pulsar-ricardo-source.tar.gz` | версия до исправления command spotter |
| `rollback-ricardo-device.sh` | возврат к исходному Pulsar/Алисе |

## Откат

На колонке сохранены:

- `/data/pulsar/backups/pre-ricardo/` — исходный Pulsar и state до «Рикардо»;
- `/data/pulsar/backups/pre-ricardo5/` — ricardo4 и state перед установкой
  ricardo5;
- `/data/pulsar/backups/pre-ricardo5b/` — ricardo5 до финальной очистки;
- `/data/pulsar/backups/pre-ricardo6/` — ricardo5 и state перед ricardo6.

Полный возврат оригинального Pulsar и «Алисы»:

```bash
adb -s 192.0.2.13:5555 shell /data/pulsar/rollback-ricardo.sh
```

Возврат к ricardo5:

```bash
adb -s 192.0.2.13:5555 push pulsar-v0.4.1-ricardo5-armhf.bin /data/pulsar/pulsar.new
adb -s 192.0.2.13:5555 shell 'chmod 755 /data/pulsar/pulsar.new && mv /data/pulsar/pulsar.new /data/pulsar/pulsar && /data/pulsar/ctl.sh restart'
```

Переключать слово без отката бинарника можно через select wake word в HA.
Не перепрошивались rootfs, OTA, root/ADB, BLE gateway и Zigbee.

## Следующая проверка

Измерить пропуски, ложные активации и задержку в тишине, с музыкой и на
разных расстояниях. Отдельно проверить «Рикардо, стоп» во время звонка
будильника, «Рикардо, отмена» после wake и продолжение музыки. При
необходимости собрать независимые записи и дообучить модель.
