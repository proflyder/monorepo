# PENDING — открытые задачи

Только незакрытые задачи. Факты — `STATE.md`; команды — `OPERATIONS.md`.

## Приёмка

- [ ] Живой микрофон: wake «Рикардо», обычный вопрос, локальные intents,
      будильник, таймер, напоминание.
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
- [x] HA voice: имя таймера, «завтра» в reminder create и durable
  `scheduler_endpoint` (`!secret`) исправлены и проверены live.
- [ ] Reminder-грамматика: поддерживается один слот `{text}` до или после
  даты/времени; фраза с двумя текстовыми фрагментами вокруг даты не
  распознаётся (осознанное ограничение, не блокер).
- [ ] Reminder delete проверен только статическими тестами; живой прогон
  удаления напоминания по имени — при следующей голосовой приёмке.
- [ ] Import: реальных активных alarms/reminders/timers не осталось (0/0/0),
  поэтому timer migration проверен только герметичными тестами; при появлении
  активных таймеров прогнать snapshot/execute/verify на живом HA.
- [x] Home control: `ha-control` развёрнут с ролью «сценарий» (HA label
  `scenario` на `script.work_mode`/`script.blackout`), LAN 30099 + pl-control,
  консюмер в allowlist, каталог 2 сценария + 15 устройств; исполнение включено.
- [x] На телефоне установлен последний APK; настройки перенесены в DataStore,
  холодный запуск без падения, Home получил HTTP 200 и показал 17 элементов
  (2 сценария, 4 лампы, 11 выключателей) с чипами категорий.
- [ ] Home control: интерактивная проверка фильтров и подтверждение действий
  пользователем; автоматические нажатия заблокированы HyperOS. Не запускать
  production-устройства без согласования.
- [ ] Приложение/Android: остальная приёмка на реальном устройстве,
  push/FCM, offline cache, generic task UI; debug APK и 11 Android-целей
  проходят.
- [ ] Recurring generic task schedules; сейчас `EnqueueTask` — one-shot.
- [ ] Один общий MCP gateway — только при появлении реального клиента;
  текущий голосовой путь в MCP не нуждается.

## Инфраструктура

- [ ] Выяснить причину потери LAN на станции после reboot: роутер online,
      ARP/ADB/ESPHome недоступны; BLE Wi-Fi TIMEOUT. Cold power cycle помог.
      Не считать перезапуск постоянным исправлением и не менять прошивку
      без диагностики; Rust/HA/Sendspin после восстановления работают.
      QR/DPP helper и автозапуски удалены по решению пользователя.
      Если отказ повторится, проверить Amlogic W1 Wi-Fi/BT SDIO и DHCP/ARP.
      Read-only collector готов: projects/station/scripts/NETWORK-DIAGNOSTIC.md.
      Один контролируемый warm reboot прошёл: сеть вернулась за ~18 с,
      HA/Sendspin и HA agent tests pass; rootkeeper restored, boot hook удалён.
      DPP fault test отменён: механизм выведен из эксплуатации.


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
      кнопки/mute/clock/volume, phone→speaker; проверить полный откат на C++.
- [ ] Проверить поведение при падении Hermes: локальные таймеры/будильник/
      громкость должны работать.
- [ ] Hermes free-speech quota/provider: нужен fallback при 429.
- [ ] Удалить legacy alarm helpers/automation после недели приёмки schedules.
- [ ] Решить: динамический `timer_list` или HACS Wakey pilot; сейчас 8 таймеров.
