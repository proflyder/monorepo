# PENDING — открытая работа

Только незакрытые задачи. Факты — `STATE.md`; команды — `OPERATIONS.md`.
Приоритет: **P1** — безопасность/надёжность и восстановимость; **P2** —
проверки и улучшения инфраструктуры; **P3** — приёмка и развитие.

## P1 — безопасность и надёжность

- [ ] **Бэкапы: offsite с шифрованием.** Код и алерты подготовлены:
      `projects/home/backup/offsite` (restic → Cloudflare R2) и
      `projects/home/backup/pve` (метрики возраста vzdump), правило
      `prometheusrule-backups.yaml`, textfile-коллектор node-exporter.
      HA automatic backup включён (03:30, 3 копии). Осталось выполнить live:
      создать R2 bucket и API-токен (Object Read & Write), заполнить
      `secrets/home/offsite-backup.env` и `secrets/home/offsite-restic.password`
      (restic password и R2 credentials продублировать вне дома, в password
      manager), установить оба `install.sh` на K3s и PVE, добавить private
      values с реальным PVE target, применить `deploy-observability.sh`,
      выполнить первый запуск и restore-тест из R2 на чистой машине. Ключ
      `openbao-seal` и recovery keys не класть в один архив с raft-снапшотами;
      они и restic password в offsite-копию не входят.

## P3 — приёмка и развитие

### Станция и голос

- [ ] **Ricardo pre-roll 256 ms.** Установлен `Fixed(8)` в релизе
      `v0.5.1002` (SHA256 `3a544979…`, база upstream `v0.5.3`) с
      `PULSAR_LOCAL_ENDPOINTING=1`; база n=3, риск среза первого слога.
      Следующий шаг: живая проверка «Рикардо отмена/который час/сколько
      времени» и других фраз. Откат на предыдущую сборку — `pulsar.prev`
      (`v0.5.1001`, `80aea2ec…`, тот же Ricardo-код); более старые — в
      `/data/pulsar/backups/`.
- [ ] **Локальный endpointing.** Feature включена только в текущем
      supervised-процессе, не boot-persistent; пороги не откалиброваны.
      Следующий шаг: живая проверка «Рикардо отмена» + повтор, обычных пауз,
      continued conversation и earcon; затем калибровка onset/release/trailing
      и решение о boot-persistent/default-on.
- [ ] **Wake word v27.** Установлен model-only (`4f645834…b0e12444`),
      `live_voice_acceptance=pending`; v24 остаётся best-known/reference.
      Следующий шаг: новый независимый ambient и живая приёмка всех манер
      речи/ложных срабатываний, recall/FP в тишине/шуме и latency; откат — `wakeword-v24.json`, затем
      `wakeword-v22.json`.
- [ ] **Wake-события: первый FP/ч.** Инструменты сбора, разметки, отчёта,
      manifest и калибровки готовы (`station_events.py`, `calibrate.py`);
      запись работает на станции с `v0.5.1003` (2026-11-01).
      Следующий шаг: собрать не менее трёх суток, `collect` → `review`/`label`
      подозрительных hits → `report` с подтверждёнными FP/ч; затем
      дообучение с `--extra-manifest` и калибровка cutoff/window.
- [ ] **Двухступенчатый верификатор.** Следующий шаг: после первых
      station-данных проверить второй каскад поверх Ricardo на подтверждённых
      false hits; сравнить FP/ч и recall, не ухудшая текущую приёмку 0.9/10.
- [ ] **Wake-free команды.** «продолжи» не распознаётся локальным споттером,
      «назад» упирается в отсутствие previous transport у MA.
      Следующий шаг: расширить локальный набор или сделать fallback в HA.
- [ ] **Отмена в LISTENING/PROCESSING/SPEAKING.** Тихие API-проверки прошли,
      включая voice proxy v6 и free follow-up.
      Следующий шаг: голосовая проверка повторного wake во время
      LISTENING/PROCESSING/SPEAKING и что «Рикардо, стоп» останавливает
      будильник.
- [ ] **Живая приёмка expiry→STOP.** Юнит/store/bridge/HA-template и live
      no-op guard проверены; реального 15-минутного звонка не было.
      Следующий шаг: запустить громкий звонок, дождаться
      `SCHEDULER_RING_WINDOW_SECONDS` (900 с) и проверить восстановление
      source/volume/resume без HA-таймера.
- [ ] **Музыка/MA.** Phone→speaker/transfer, MA Liked Songs sync и «включи
      любимое» при пустом плейлисте.
      Следующий шаг: прогнать transfer с телефона и наполнить/проверить
      Liked Songs.
- [ ] **Bluetooth-пара телефона.** `bluetoothctl devices` на станции пуст;
      HA `bluetooth` в `setup_retry`, adapter станции недоступен из HAOS VM.
      Следующий шаг: спаривать на станции, не через HA; помнить, что 4
      нажатия могут уронить Wi-Fi.
- [ ] **Hermes free-speech quota/provider.** Нет fallback при 429.
      Следующий шаг: определить локальный/альтернативный fallback и проверить
      поведение при ошибке провайдера.
- [ ] **Падение Hermes.** Локальные таймеры/будильник/громкость должны
      работать без LLM.
      Следующий шаг: остановить Hermes и прогнать локальные команды и
      будильник.
- [ ] **BLE gateway: 3 нажатия без nRF Connect.** Голосовые
      «включи/выключи гейтвей» проверены.
      Следующий шаг: проверить физические 3 нажатия и запись в логе.

### Планировщик / Android

- [ ] **Home control.** Интерактивная проверка фильтров и подтверждение
      действий пользователем; автоматические нажатия блокирует HyperOS.
      Следующий шаг: согласовать окно и проверить на реальном телефоне, не
      трогая production-устройства без согласования.
- [ ] **Первый релиз в Firebase App Distribution и первый тестовый краш.**
      Готово: сервисный аккаунт `github-app-distribution` (Firebase App
      Distribution Admin), запись `secret/ci/android-distribute`, роль OpenBao
      `android-distribute` на PR merge-ref, Environment `Android` без branch
      policy, группа `admins` («Admins»). Загрузка из CI проверена
      (`0.1.0-1f3a812`), раздача тогда упала только из-за отсутствующей группы.
      Следующий шаг: первый успешный `android-distribute` на PR, установка
      через App Tester, краш через `adb shell am crash dev.proflyder.home` и
      отчёт в Crashlytics.
- [ ] **Android-приёмка.** Остальные экраны на реальном устройстве.
      Следующий шаг: пройти сценарии, затем решить по push/FCM, offline cache
      и generic task UI.
- [ ] **Recurring generic task schedules.** `EnqueueTask` — one-shot.
      Следующий шаг: добавить `SCHEDULE_KIND_TASK` и тесты повторяемости.

### Инфраструктура и станция

- [ ] **Потеря LAN станцией после reboot.** Роутер online, ARP/ADB/ESPHome
      недоступны, BLE Wi-Fi TIMEOUT; cold power cycle восстанавливает.
      Следующий шаг: собрать read-only диагностику
      (`projects/station/scripts/NETWORK-DIAGNOSTIC.md`), прошивку без
      диагностики не менять.
- [ ] **Power-loss тест host/VM.** После штатного `shutdown` PVE (2026-11-02,
      дважды) автозапуск HAOS → K3s → VPN, все поды и HA проверены; жёсткого
      отключения питания ещё не было. Следующий шаг: согласовать окно и
      проверить автозапуск VM, HA, K3s, Sendspin и Pulsar после выдёргивания
      питания.
- [ ] **Latency instrumentation voice pipeline.** Полных измерений нет.
      Следующий шаг: добавить метрики по этапам wake→STT→intent→TTS.
- [ ] **Secure Boot PK `DO NOT TRUST - AMI Test PK`.** Следующий шаг: решать
      только с физдоступом и планом, не менять вслепую.
- [ ] **Factory/recovery image Midi и raw dump.** Следующий шаг: согласовать
      read-only дамп на запасной станции.
