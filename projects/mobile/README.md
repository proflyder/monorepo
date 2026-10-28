# projects/mobile

Мобильные клиенты. Основной и единственный активный срез — Android в
[`android/`](android/README.md); iOS пока не инициализируется.

- Android: Kotlin, Bazel/Bazelisk корневого модуля, Android Views, одна
  Activity, Fragment-навигация за типизированными контрактами, Metro
  compiler-plugin DI. Application ID — `dev.proflyder.home`.
- Срезы: alarms (one-shot/recurring, snooze/stop), reminders (one-shot),
  timers (server-owned, pause/resume/cancel), connection settings с
  обязательным bearer token, Keystore AES-GCM и save-and-test. Стабильные
  idempotency keys переживают неоднозначный retry; redirects выключены.
- Сетевые адреса настраиваются (LAN или Tailscale). VPN-подключение остаётся
  за системным приложением Tailscale; клиент использует обычную маршрутизацию
  и проверенный TLS.
- Телефон управляет серверным состоянием. Владелец расписаний — серверный
  scheduler, не телефон; push-уведомления — только канал доставки.

Статус сборки, таргетов и ограничений — в [`android/README.md`](android/README.md)
и [`docs/STATE.md`](../../docs/STATE.md). Debug APK собирается, общий gate —
16/16 targets (9 Android, включая cold-launch smoke и 14 nav lifecycle, и
7 backend). На реальном устройстве/эмуляторе приложение не принималось; push/
FCM, offline cache и generic task/scenario UI ещё не сделаны. Android-гейты
не входят в backend `make test`.
