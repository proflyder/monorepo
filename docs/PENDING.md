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

## Инфраструктура

- [ ] Выяснить причину потери LAN на станции после reboot: роутер online,
      ARP/ADB/ESPHome недоступны; BLE Wi-Fi TIMEOUT. Cold power cycle помог.
      Не считать перезапуск постоянным исправлением и не менять прошивку
      без диагностики; Rust/HA/Sendspin после восстановления работают.
      Проверить DPP auto-listen по отсутствию default route (гонка с WPA/DHCP
      и BLE provisioning) и общий Amlogic W1 Wi-Fi/BT SDIO при warm reboot.
      Read-only collector готов: projects/station/scripts/NETWORK-DIAGNOSTIC.md.
      Reboot test ожидает подтверждения физического доступа.


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
