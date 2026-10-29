# PENDING — открытые задачи

Только незакрытые задачи. Факты — `STATE.md`; команды — `OPERATIONS.md`.

## Приёмка

- [ ] Spotify/MA: phone→speaker/transfer. Pause и next проверены 29.10
      wake-free; resume — с wake; previous для Spotify Connect исправлен через
      Spotify Web API (MA не предлагает previous transport).
- [ ] Wake-free команды Pulsar: «продолжи» не распознаётся локальным
      споттером, «назад» упирается в отсутствие previous transport у MA.
      Нужно либо расширить локальный набор, либо fallback в HA.
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

- [ ] Wake word: исправить регрессию обычной/резкой речи при подавлении
  сложных негативов. Активна v2; v7 и последующие кандидаты отклонены.
  Выбирать веса по непрерывным validation-дорожкам: короткие окна, включая
  streaming replay нарезок, скрывают падение распознавания длинных записей.
  Новые train/test-дорожки и PCM-контексты сохранены private; повторные
  проверки уже использованного test считать регрессионными, не слепыми.
