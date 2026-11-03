# PENDING — открытая работа

Только незакрытые задачи. Факты — `STATE.md`; команды — `OPERATIONS.md`.
Приоритет: **P1** — безопасность/надёжность и восстановимость; **P2** —
проверки и улучшения инфраструктуры; **P3** — приёмка и развитие.

## P1 — безопасность и надёжность

## P2 — проверки и улучшения инфраструктуры

- [ ] **Вход `gh` и Codex на CT 103.** Нужен только для агентов, запускаемых
      на самом сервере bb; треды на Mac работают без него. Следующий шаг:
      авторизовать `gh` и Codex под пользователем `bb` перед первым таким
      запуском.

- [ ] **Решение 35: остаток перевода Python.** CI-обвязка (#97),
      Android-инструменты сборки (#98) и voice-proxy (#99) уже переведены.
      Осталось по исследованию: (1) Go-утилита `platform/deploy`
      (`home-deploy`): шаг 1 (`feat/home-deploy-go`) добавил `render`,
      `env get`, `build-id` и `oci digest/pack/docker` и переключил на них
      `deploy.sh` юнитов, `nft/apply.sh`, ingress-render и сборку образов в CI
      (Python-рендерер и `platform/ci/oci_image.py` удалены, parity
      зафиксирован в `platform/deploy/testdata/`); осталось
      `converge`/`rollback`/`preflight` (Job без `python:3.13-slim`),
      inline-Python в `apply-runtime.sh`, external-secrets, `hermes_rollout.py`,
      `render_hermes_config.py`; health
      exporter — Go-образ (сейчас он запущен из последнего Python-образа
      voice-proxy, закреплённого digest в приватных values; пока это так, этот
      digest защищён записью `PROTECTED_DIGESTS` в
      `platform/ci/ghcr_cleanup.mjs` — удалить запись вместе с миграцией
      exporter'а). Проверка —
      побайтный рендер и разобранные объекты kubectl против Python, затем
      ручной `workflow_dispatch` каждого юнита. (2) Хосты и оператор:
      `offsite_backup.py` → Go-бинарник из GHCR по digest (как решение 34),
      `vpncerts.py`/`vpnusers.py` → `vpn-pki`, `openbao configure/drill`.
      Python остаётся: короткие хост-скрипты (`backup_metrics`,
      `restore-volumes`, `wifi-uplink`, `backup-before-install`), ML
      wake-word, dev-утилиты станции, проверки YAML, live e2e.
- [ ] **CI: правка документации внутри юнита не должна деплоить.** После
      #97 перевыкатились mcp-gateway/currency-bot/wakegrant только из-за
      README в их папках. Следующий шаг: в `affected.mjs` не выбирать юнит
      для деплоя, если все его изменённые файлы — `*.md`.

## P3 — приёмка и развитие

### Станция и голос

- [ ] **voice-proxy на Go: рефакторинг после переноса (#99).** Порт
      сделан один-в-один с Python, поэтому код (~4 тыс. строк) и тесты
      (~4,5 тыс.) объёмнее нужного. Следующий шаг: убрать дубли (повторные
      HA/gateway-клиенты внутри `announce`/`profile`/`result`, общая
      тестовая обвязка `httptest`-фейков в один `internal/testfake`),
      сократить эталонный корпус `testdata/parity` (~4,2 тыс. строк JSON) до
      показательных случаев, когда Python-версия уже не нужна для отката;
      удалить «port of voice_proxy.py»-комментарии. Поведение и покрытие не
      менять; `go test -race` и живая проверка голоса после.
- [ ] **Ricardo pre-roll 256 ms.** Поставлен `Fixed(8)` в релизе
      `v0.5.1002` (база upstream `v0.5.3`; сейчас стоит `v0.5.1004`) с
      `PULSAR_LOCAL_ENDPOINTING=1`; база n=3, риск среза первого слога.
      Следующий шаг: живая проверка «Рикардо отмена/который час/сколько
      времени» и других фраз. Откат на предыдущую сборку — `pulsar.prev`
      (`v0.5.1003`); более старые — в `/data/pulsar/backups/`.
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
- [ ] **Живая приёмка голосового профиля через колонку.** Тихие проверки
      прошли: маршрут work → постоянный тред, personal + режим агента →
      активный тред, fail-closed без Hermes, карточка «Личный | Рабочий» в
      Android, цвет кольца по профилю.
      Следующий шаг (за владельцем): проговорить с колонки в рабочем профиле
      и убедиться, что ответ пришёл из рабочего треда и озвучен
      `script.schedule_announce`; затем то же в личном профиле без режима
      агента (Hermes) и с режимом; проверить смену цвета Blue/Violet и что
      рабочая фраза не уходит в Hermes.
- [ ] **Pulsar `cargo test` на Linux для `v0.5.1004`.** Раскатан релиз
      с select «Voice color».
      Следующий шаг: проверить, покрывает ли CI форка `cargo test` на Linux
      для тега `v0.5.1004`; если workflow только собирает armhf — прогнать
      `cargo test` на Linux вручную и зафиксировать результат перед следующим
      релизом.

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
- [ ] **Latency instrumentation voice pipeline.** Полных измерений нет.
      Следующий шаг: добавить метрики по этапам wake→STT→intent→TTS.
- [ ] **Secure Boot PK `DO NOT TRUST - AMI Test PK`.** Следующий шаг: решать
      только с физдоступом и планом, не менять вслепую.
- [ ] **Factory/recovery image Midi и raw dump.** Следующий шаг: согласовать
      read-only дамп на запасной станции.
