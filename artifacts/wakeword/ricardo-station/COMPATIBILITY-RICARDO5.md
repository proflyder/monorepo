# Проверка совместимости ricardo5 с on-device wake word

Дата: 2026-10-27, после установки ricardo5 другой сессией.
Проверка read-only: бинарник, конфиг, музыка и будильник не менялись.

## Сверка с сохранённым исходным деревом ricardo4

Побайтно не изменились:
- satellite/src/audio/microwakeword.cpp и .h;
- satellite/src/audio/frontend.cpp и .h;
- satellite/src/audio/alsa_abi.h;
- satellite/Makefile.

В controller.cpp/.h изменены только правила обработки локальных команд:
убран arb_command_, отключён command spotter во время LISTENING/PROCESSING/
арбитража, добавлен аналогичный guard для уже поставленных в очередь callback.
Wake callback, 0.5 s pre-roll, передача аудио после арбитража, меню «Рикардо»,
mute-проверки и запуск HA pipeline сохранены. kDuckGain=0.15 и update_duck()
сохранены; восстановление gain идёт существующим путём.

## Станция

- Версия: v0.4.1-ricardo5.
- SHA-256 бинарника совпадает с артефактом проекта и summary пользователя:
  52e5163e8134d3502e9cb340cd33383fe119bff7e076f1b489f2ba195d454fb9.
- SHA-256 модели совпадает с обученной в этой сессии:
  f1ba6dca7c380b584d5e4e1d83f9fd2144e09f4ef9a68135e206f7aab5fd96b2.
- wake_word=ricardo, mic_muted=0.
- ALSA: 96 kHz, 8 каналов, period 3072 — ABI-исправление не потеряно.
- В логах загрузка microWakeWord, подключение HA и подписка voice assistant.
- Отдельный offline mww_probe: load/inference OK, на тишине 0 срабатываний;
  основной процесс Pulsar не перезапускался.
- В текущем Sendspin логе музыка играет, xruns=0, duck gain=1.00 вне запроса.

## Границы вывода

В проверенной части регрессий интеграции «Рикардо» не обнаружено.
Живой сценарий «Рикардо, стоп» во время будильника в этой проверке не
воспроизводился: его текстовый/API путь подтверждён другой сессией,
но микрофонный retest остаётся отдельной проверкой.
Модель не переобучалась: чувствительность к шёпоту не изменилась.

Протокол: logs/status-20261027-0418-ricardo5-compatibility.txt.
