# PENDING — открытая работа

Только незакрытые задачи. Факты — `STATE.md`; команды — `OPERATIONS.md`.
Приоритет: **P1** — безопасность/надёжность и восстановимость; **P2** —
проверки и улучшения инфраструктуры; **P3** — приёмка и развитие.

## P1 — безопасность и надёжность

- [ ] **OpenBao TOTP enforcement.** TOTP-метод создан, устройство ещё не
      зарегистрировано, enforcement выключен. Следующий шаг: человек
      регистрирует TOTP в UI (профиль → MFA), затем включить
      `./projects/home/kubernetes/openbao/bootstrap.sh --enforce-totp`
      (или API `identity/mfa/login-enforcement/userpass-totp`) и проверить
      вход с кодом и без него.
- [ ] **VPS: ротация пароля и key-only SSH.** Пароль был отправлен в чат, на
      VPS ещё живёт старый стек. Следующий шаг: сменить пароль, включить вход
      только по ключу и проверить доступ; затем вывести VPS (P2).
- [ ] **Offsite-бэкап с шифрованием.** Сейчас raft snapshots лежат на том же
      узле; HA/K3s/scheduler/`secrets/`, CA и recovery keys не вывозятся.
      Следующий шаг: выбрать offsite-назначение и ключ, вывозить OpenBao
      snapshots, SQLite scheduler, `secrets/` и recovery-материал в
      шифрованном архиве, добавить проверку свежести.
- [ ] **Проверка restore.** Восстановление OpenBao и scheduler на отдельном
      стенде не выполнялось.
      Следующий шаг: развернуть временный стенд, восстановить raft snapshot и
      SQLite, сравнить ключевые записи; зафиксировать процедуру в
      `REBUILD.md`.
- [ ] **RingWindow: правка правила во время звонка.** `UpdateSchedule` и
      `SetScheduleEnabled(true)` очищают активное окно без STOP, HA может
      остаться со звуком, а `StopSchedule` вернёт `FAILED_PRECONDITION`.
      Следующий шаг: durable STOP/явный контракт при изменении правила во
      время occurrence.

## P2 — инфраструктурные проверки

- [ ] **VPN профиль `full` с мобильной сети.** Проверены `lan`/DTLS и группа
      `ci`; `full` с WAN NAT/MTU/DNS — нет.
      Следующий шаг: подключиться с мобильной сети профилем `full`, проверить
      DNS, MTU и доступ к LAN/HTTPS-именам.
- [ ] **Внешний KMS/HSM для seal.** `static` seal хранит ключ рядом с данными.
      Следующий шаг: выбрать KMS/HSM/transit, оформить решение и миграцию
      отдельным изменением.
- [ ] **Вывод VPS после недели стабильности.** Бот остановлен, данные
      выгружены, но monitoring stack и старые containers/volumes ещё живы.
      Следующий шаг: после недели зелёного CI-деплоя и штатной работы
      currency-bot погасить Prometheus/Grafana/Loki, удалить старые
      контейнеры и volumes, затем вывести VPS.
- [ ] **Архивировать `tg-bot-currency`, `infra-monitoring`, `infra-ci`.**
      Функции заменены монорепо. Следующий шаг: подтвердить у владельца и
      перевести три GitHub-репозитория в archive (read-only), сохранив
      историю.

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
      речи/ложных срабатываний; откат — `wakeword-v24.json`, затем
      `wakeword-v22.json`.
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
- [ ] **`assist_satellite` `responding`.** Редкие зависания не
      воспроизводились; максимум 29.7 с, всегда возврат в `idle`.
      Следующий шаг: наблюдать; watchdog/reload трогать только при рецидиве.
- [ ] **BLE gateway: 3 нажатия без nRF Connect.** Голосовые
      «включи/выключи гейтвей» проверены.
      Следующий шаг: проверить физические 3 нажатия и запись в логе.
- [ ] **Wake word v1 recall/FP.** Нужны recall/FP в тишине/шуме и latency на
      независимых записях.
      Следующий шаг: записать независимый набор и прогнать метрики.

### Планировщик / Android

- [ ] **Home control.** Интерактивная проверка фильтров и подтверждение
      действий пользователем; автоматические нажатия блокирует HyperOS.
      Следующий шаг: согласовать окно и проверить на реальном телефоне, не
      трогая production-устройства без согласования.
- [ ] **Android-приёмка.** Остальные экраны на реальном устройстве.
      Следующий шаг: пройти сценарии, затем решить по push/FCM, offline cache
      и generic task UI.
- [ ] **Recurring generic task schedules.** `EnqueueTask` — one-shot.
      Следующий шаг: добавить `SCHEDULE_KIND_TASK` и тесты повторяемости.

### Инфраструктура и станция

- [ ] **OpenBao: пути currency-bot по конвенции.** Сейчас
      `k8s/currency-bot` и `k8s/cloudflared` вместо
      `k8s/<namespace>/<name>`. Следующий шаг: при следующем использовании
      admin-токена перенести значения в `k8s/currency-bot/currency-bot` и
      `k8s/currency-bot/cloudflared`, обновить ExternalSecrets и проверить
      синхронизацию.
- [ ] **Удалить неиспользуемый Unkey.** API currency-bot проверяет Bearer из
      OpenBao, Unkey больше не нужен. Следующий шаг: убедиться, что других
      клиентов нет, и удалить аккаунт/ключи.

- [ ] **Потеря LAN станцией после reboot.** Роутер online, ARP/ADB/ESPHome
      недоступны, BLE Wi-Fi TIMEOUT; cold power cycle восстанавливает.
      Следующий шаг: собрать read-only диагностику
      (`projects/station/scripts/NETWORK-DIAGNOSTIC.md`), прошивку без
      диагностики не менять.
- [ ] **Power-loss тест host/VM.** Следующий шаг: согласовать окно и проверить
      автозапуск VM, HA, K3s, Sendspin и Pulsar после жёсткого отключения.
- [ ] **Регулярный backup K3s/HA с проверкой freshness.** HA automatic backup
      не настроен (`sensor.backup_*` = unknown), CronJob/Velero нет.
      Следующий шаг: включить расписание и алерт на устаревший backup.
- [ ] **Внешний Alertmanager/heartbeat.** Внутренний Alertmanager есть,
      dead-man switch наружу нет.
      Следующий шаг: выбрать внешний приёмник и отправлять heartbeat.
- [ ] **Latency instrumentation voice pipeline.** Полных измерений нет.
      Следующий шаг: добавить метрики по этапам wake→STT→intent→TTS.
- [ ] **Secure Boot PK `DO NOT TRUST - AMI Test PK`.** Следующий шаг: решать
      только с физдоступом и планом, не менять вслепую.
- [ ] **Factory/recovery image Midi и raw dump.** Следующий шаг: согласовать
      read-only дамп на запасной станции.
- [ ] **ONT WAN `22/443`.** Следующий шаг: проверить и закрыть лишние
      пробросы, оставить только VPN `443/tcp+udp`.
