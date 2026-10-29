# PENDING — открытые задачи

Только незакрытые задачи. Факты — `STATE.md`; команды — `OPERATIONS.md`.

## Приёмка

- [ ] Живой микрофон: wake «Рикардо», обычный вопрос, локальные intents,
      будильник, таймер, напоминание. Wake + немедленная команда после имени
      подтверждены пользователем на v0.5.2-2; остальные сценарии — нет.
- [ ] Будильник: звонок, «стоп», snooze, resume музыки; проверить на реальном
      Pulsar, не только API.
- [ ] Spotify/MA: pause/resume, next/previous, phone→speaker/transfer.
- [ ] MA Liked Songs sync; «включи любимое» при пустом плейлисте.
- [ ] Bluetooth-пара телефона; помнить, что 4 нажатия могут уронить Wi-Fi.
- [ ] `assist_satellite` иногда остаётся `responding`; проверить watchdog/reload.
- [ ] 3 нажатия и голосовые «включи/выключи гейтвей» без nRF Connect.

## Matter / Xiaomi

- [ ] Добавить Yandex LED strip через HA Companion (Matter, 2.4 ГГц,
      Bluetooth/location); проверить on/off, яркость, цвет.
- [ ] Xiaomi лампы: on/off, brightness, color temperature.
- [ ] Matter paired nodes сейчас 0; LED — первое устройство.

## Планировщик / HA bridge / Android

- [ ] Живая звуковая приёмка scheduler→bridge: реальный alarm fire, STOP,
  snooze 9 минут, resume source/volume. API/ACK проверены (task SKIPPED,
  consumer alive, bounded recovery 5s), audible — нет; запускать только по
  согласованию.
- [ ] Reminder-грамматика: поддерживается один слот `{text}` до или после
  даты/времени; фраза с двумя текстовыми фрагментами вокруг даты не
  распознаётся (осознанное ограничение, не блокер).
- [ ] Reminder delete проверен только статическими тестами; живой прогон
  удаления напоминания по имени — при следующей голосовой приёмке.
- [ ] Home control: интерактивная проверка фильтров и подтверждение действий
  пользователем; автоматические нажатия заблокированы HyperOS. Не запускать
  production-устройства без согласования.
- [ ] Приложение/Android: остальная приёмка на реальном устройстве,
  push/FCM, offline cache, generic task UI.
- [ ] Recurring generic task schedules; сейчас `EnqueueTask` — one-shot.

## Инфраструктура

- [ ] MA не переподключается к Sendspin автоматически после рестарта Pulsar
      (WS close `1006`, авто-redial нет); наблюдается на обеих сборках.
      Временно восстанавливает один рестарт add-on MA; нужен автоматический
      reconnect.
- [ ] Причина потери LAN на станции после reboot не установлена (роутер
      online, ARP/ADB/ESPHome недоступны; BLE Wi-Fi TIMEOUT). Cold power
      cycle восстанавливает; прошивку без диагностики не менять. Read-only
      collector: `projects/station/scripts/NETWORK-DIAGNOSTIC.md`.


- [ ] Offsite encrypted backup + restore rehearsal на запасном железе.
- [ ] Аварийный power-loss тест host/VM.
- [ ] Host Tailscale emergency access; сейчас PVE только LAN/Wi-Fi.
- [ ] Внешний Alertmanager/heartbeat.
- [ ] Полная latency instrumentation voice pipeline.
- [ ] Регулярный backup K3s/HA с проверкой freshness.
- [ ] Private registry/build pipeline для Hermes/proxy images.

## Безопасность / станция

- [ ] `quasar-server :31116` без auth — закрыть на loopback или добавить auth.
- [ ] Secure Boot PK `DO NOT TRUST - AMI Test PK`: решить отдельно, только
      с физдоступом; не менять вслепую.
- [ ] Factory/recovery image Midi и raw dump — нужен согласованный read-only
      дамп на запасной станции.
- [ ] ONT WAN `22/443`: проверить/закрыть.
- [ ] SSH по ключам, password auth off; ADB/SSH только LAN.

## Голос / разработка

- [ ] Wake word v1: recall/FP в тишине/шуме и latency; независимые записи.
- [ ] Rust Pulsar: живой «Рикардо, стоп» во время будильника, snooze/resume,
      кнопки/mute/clock/volume, phone→speaker.
- [ ] Проверить поведение при падении Hermes: локальные таймеры/будильник/
      громкость должны работать.
- [ ] Hermes free-speech quota/provider: нужен fallback при 429.
- [ ] Решить: динамический `timer_list` или HACS Wakey pilot; сейчас 8 таймеров.
