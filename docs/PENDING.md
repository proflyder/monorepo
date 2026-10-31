# PENDING — открытые задачи

Только незакрытые задачи. Факты — `STATE.md`; команды — `OPERATIONS.md`.

## Приёмка

- [ ] Ricardo pre-roll 256 ms установлен 30.10 (`Fixed(8)`, SHA256
      `7081ef8e…`, armhf) с `PULSAR_LOCAL_ENDPOINTING=1`. Нужна живая приёмка
      трёх команд («Рикардо отмена/который час/сколько времени») и других фраз:
      база n=3, риск среза первого слога, гарантии для остальных команд нет.
      При регрессе — откат только pre-roll на `pulsar.prev` `0b0c48e4…`
      (512 ms) с сохранением endpointing; on-device backup
      `/data/pulsar/backups/deploy-preroll8-20261030T185700Z/`.
- [ ] Локальный endpointing Rust Pulsar: armhf-кандидат `v0.5.2-4-g9b5e86a`
      установлен 30.10, feature включена только в текущем supervised-процессе
      через `PULSAR_LOCAL_ENDPOINTING=1` (не boot-persistent). Нужны живая
      проверка «Рикардо отмена» + повтор, обычных пауз, continued
      conversation и earcon; затем калибровка onset/release/trailing на
      реальных записях и решение о boot-persistent/default-on. Не считать
      исправлением целевого случая до этой приёмки.
- [ ] Кандидат v27 (`v27-ambient-capacity-sel22-20261030`, SHA256
      `4f645834…b0e12444`) установлен 30.10 model-only по прямому запросу
      пользователя; `live_voice_acceptance=pending`. Firmware-совместимость
      проверена (C++ 162/164 против 159/164 у v24 при 16 срабатываниях на
      похожих словах против 17; Python-стенд дал одинаковые 161/164). Открыто:
      свежий независимый ambient и живая приёмка всех манер речи/ложных
      срабатываний. V24 остаётся best-known/reference по решению пользователя;
      откат — `projects/station/configs/wakeword-v24.json`.

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
- [ ] Внешний Alertmanager/heartbeat; внутренний Alertmanager есть, dead-man
      switch наружу нет.
- [ ] Полная latency instrumentation voice pipeline.
- [ ] Регулярный backup K3s/HA с проверкой freshness. HA automatic backup не
      настроен (`sensor.backup_*` = unknown), CronJob/Velero в репозитории нет.

## OpenBao

- [ ] OpenBao TOTP: пользователь регистрирует TOTP в UI, затем включить
      enforcement: `./projects/home/kubernetes/openbao/bootstrap.sh --enforce-totp`
      (или API `identity/mfa/login-enforcement/userpass-totp`).
- [ ] Offsite-копия raft snapshot и проверка restore на отдельном стенде;
      сейчас 7 снапшотов лежат на том же узле, что и данные.
- [ ] Внешний KMS/HSM для seal вместо `static` (ключ рядом с данными);
      решение и миграцию оформить отдельно.

## VPN

- [ ] Тест профиля `full` с мобильной сети (WAN NAT, MTU, DNS).

## CI/CD (GitHub-hosted runners)

- [ ] Pulsar собирается и релизится в своём репозитории, монорепо деплоит
      релиз на станцию через Environment с ручным approve; конфиг HA — по
      решению.

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

- [ ] Wake word: живая приёмка установленного v27-кандидата и новый независимый
  ambient; v24 остаётся best-known/reference. Обе новые фоновые дорожки
  обучающие: 0 событий не является независимой оценкой. Прежняя регрессия
  161/164 против 163/164 у v22; проверить все манеры речи. Профили отката:
  `wakeword-v24.json` (best-known) и `wakeword-v22.json`; все веса и записи — private.
