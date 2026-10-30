# PENDING — открытые задачи

Только незакрытые задачи. Факты — `STATE.md`; команды — `OPERATIONS.md`.

## Приёмка

- [ ] Более ёмкий MixedNet, кандидат e22: firmware-совместимость проверена
      (C++ 162/164 против 159/164 у v24 при 16 срабатываниях на похожих
      словах против 17; Python-стенд дал одинаковые 161/164). Открыто: свежий
      независимый ambient и живая приёмка; установка не выполнялась, решения
      о замене v24 нет.

- [ ] Spotify/MA: phone→speaker/transfer. Pause и next проверены 29.10
      wake-free; resume — с wake; previous для Spotify Connect исправлен через
      Spotify Web API (MA не предлагает previous transport).
- [ ] Wake-free команды Pulsar: «продолжи» не распознаётся локальным
      споттером, «назад» упирается в отсутствие previous transport у MA.
      Нужно либо расширить локальный набор, либо fallback в HA.
- [ ] Живая приёмка отмены: повторный wake во время LISTENING/PROCESSING
      отменяет старый вызов и начинает новый, поздний RUN_END старого run не
      завершает новый диалог. «Отмена» после wake идёт в локальный
      `StationCancel` без TTS; в continued-диалоге (Hermes закончил вопросом)
      её теперь перехватывает voice proxy v6 до Hermes. Отмена без wake во
      время думающего вызова не поддерживается: в command-модели станции нет
      этого слова. Тихие API-проверки 30.10 прошли: direct local intent,
      continued `assist_pipeline/run` (`end_stage=tts`, 0.02 с, без tts events,
      без upstream), свободный follow-up после отмены доходит до Hermes.
      Осталась голосовая проверка LISTENING/PROCESSING/SPEAKING и что
      «Рикардо, стоп» по-прежнему останавливает будильник; распознавание
      `StationAlarmStop` проверено без запуска звонка.
- [ ] Живая приёмка expiry→STOP: будильник должен замолкнуть сам после
      `SCHEDULER_RING_WINDOW_SECONDS` (900 с), без участия HA-таймера, с
      восстановлением source/volume/resume. Требует реального звонка на
      15 минут (громко) — не запускалось; unit/store/bridge/HA-template
      покрытие и live no-op guard проверены.
- [ ] MA Liked Songs sync; «включи любимое» при пустом плейлисте.
- [ ] Bluetooth-пара телефона. `bluetoothctl devices` на станции пуст; в HA
      entry `bluetooth` в `setup_retry` (adapter станции недоступен из HAOS VM),
      поэтому спаривание делать на станции, не через HA. Помнить: 4 нажатия
      могут уронить Wi-Fi.
- [ ] `assist_satellite`: наблюдать редкие `responding`. За 2 суток (27–29.10)
      не воспроизводилось: максимум 29.7 с, всегда завершается `idle`; 26
      коротких `unavailable` ≤5 с. Watchdog/reload проверять только при
      рецидиве.
- [ ] 3 нажатия BLE-гейтвея без nRF Connect. Голосовые «включи/выключи
      гейтвей» проверены 29.10 (интент → rest_command → gateway, `manual`
      открывается и закрывается).

## Планировщик / HA bridge / Android

- [ ] Ring window lifecycle: `UpdateSchedule` и `SetScheduleEnabled(true)`
      очищают активное окно без STOP (подтверждённый related bug того же
      класса, что исправленный expiry). Нужен durable STOP/явный контракт,
      чтобы правка правила во время звонка не оставляла HA со звуком и
      `StopSchedule=FAILED_PRECONDITION`. Отдельная правка, не входит в
      expiry-fix.
- [ ] Home control: интерактивная проверка фильтров и подтверждение действий
      пользователем; автоматические нажатия заблокированы HyperOS. Не запускать
      production-устройства без согласования.
- [ ] Приложение/Android: остальная приёмка на реальном устройстве,
      push/FCM, offline cache, generic task UI.
- [ ] Recurring generic task schedules; сейчас `EnqueueTask` — one-shot.

## Инфраструктура

- [ ] Причина потери LAN на станции после reboot не установлена (роутер
      online, ARP/ADB/ESPHome недоступны; BLE Wi-Fi TIMEOUT). Cold power
      cycle восстанавливает; прошивку без диагностики не менять. Read-only
      collector: `projects/station/scripts/NETWORK-DIAGNOSTIC.md`.
- [ ] Offsite encrypted backup + restore rehearsal на запасном железе.
- [ ] Аварийный power-loss тест host/VM.
- [ ] Host Tailscale emergency access; сейчас PVE только LAN/Wi-Fi.
- [ ] Внешний Alertmanager/heartbeat; внутренний Alertmanager есть, dead-man
      switch наружу нет.
- [ ] Полная latency instrumentation voice pipeline.
- [ ] Регулярный backup K3s/HA с проверкой freshness. HA automatic backup не
      настроен (`sensor.backup_*` = unknown), CronJob/Velero в репозитории нет.
- [ ] Private registry/build pipeline для Hermes/proxy images.

## Безопасность / станция

- [ ] Secure Boot PK `DO NOT TRUST - AMI Test PK`: решить отдельно, только
      с физдоступом; не менять вслепую.
- [ ] Factory/recovery image Midi и raw dump — нужен согласованный read-only
      дамп на запасной станции.
- [ ] ONT WAN `22/443`: проверить/закрыть.

## Голос / разработка

- [ ] Wake word v1: recall/FP в тишине/шуме и latency; независимые записи.
- [ ] Rust Pulsar: кнопки/mute/clock/volume, phone→speaker.
- [ ] Проверить поведение при падении Hermes: локальные таймеры/будильник/
      громкость должны работать.
- [ ] Hermes free-speech quota/provider: нужен fallback при 429.

- [ ] Wake word: живая приёмка установленной v24 и новый независимый ambient.
  Обе новые фоновые дорожки обучающие: 0 событий не является независимой оценкой.
  Прежняя регрессия 161/164 против 163/164 у v22; проверить все манеры речи.
  V22 сохранена с отдельным профилем отката; все веса и записи — private.
