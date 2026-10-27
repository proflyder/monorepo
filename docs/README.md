# Документация лаборатории

**Один факт — один владелец.** Документы не дублируют состояние; вместо копий —
ссылки. Сессии/транскрипты не являются документацией; для истории — `HISTORY.md`,
для восстановления — `REBUILD.md`.

## Start here

| Документ | Чем владеет |
|---|---|
| `STATE.md` | канонические текущие факты (станция, Pulsar, HA, сеть, remote) |
| `REBUILD.md` | **восстановление с нуля**: SSD неттопа, factory reset колонки, потеря Mac/Quasar |
| `DECISIONS.md` | ключевые решения и «почему это, а не иначе» |
| `OPERATIONS.md` | runbook: команды, доступ, откат |
| `SECURITY.md` | политики/риски; чувствительные данные — `SENSITIVE-DATA.md` |
| `SENSITIVE-DATA.md` | реестр путей/типов секретов (без значений) |
| `CONTEXT.md` | короткая ориентация/фокус |
| `MONOREPO.md` | структура монорепозитория, Bazel, выбор Go/Rust, границы |
| `GRPC.md` | gRPC/protobuf-контракт, транспорты, ошибки, auth |
| `REPOSITORY-AUDIT.md` | аудит переноса/публикации, проверки и ограничения |
| `SNAPSHOTS.md` | локальные снапшоты вне Git: формат, исключения, проверка |

## Задачи и история

| Документ | Чем владеет |
|---|---|
| `PENDING.md` | открытые задачи/решения |
| `PLAN-KUBERNETES-MIGRATION.md` | live-исследование неттопа, переход Docker → K3s, Helm/Prometheus/Loki/Grafana |
| `PLAN-SCHEDULES.md` | план/границы будильников, таймеров, напоминаний, заметок |
| `HISTORY.md` | хронология; append-only, без текущего статуса |

## Станция / Quasar

| Документ | Чем владеет |
|---|---|
| `ARCHITECTURE.md` | устройство стенда, потоки, сертификаты |
| `OTA-FORTRESS.md` | OTA/ADB fortress |
| `PROVISIONING.md` | BLE/DPP/AP/sound provisioning |
| `REPLICATION-GUIDE.md` | пошаговая station-процедура (историческая; recovery — `REBUILD.md`) |
| `WAKE-WORD.md` | обучение, исходники, сборка и установка своего wake word |
| `RESOURCES.md` | каталог внешних проектов/совместимость |
| `REMOTE-ACCESS.md` | текущий remote HA + варианты (Tailscale/Serve; research) |
| `REMOTE-ACCESS-TAILSCALE-PLAN.md` | реализованный план/проверки/rollback |
| `REMOTE-ACCESS-STATIC-IP.md` | альтернатива WireGuard на static IP (не выбрана) |

## Неттоп / стек

| Документ | Чем владеет |
|---|---|
| `../projects/home-assistant/NETTOP.md` | обзор `lab-host` |
| `../projects/home-assistant/INVENTORY.md` | железо/адреса/версии |
| `../projects/home-assistant/SETUP.md` | воспроизводимый сетап |
| `../projects/home-assistant/HA.md` | HA Container runbook |
| `../projects/home-assistant/stack/README.md` | HA + MA + STT/TTS, `ctl.sh`, backup |
| `../projects/home-assistant/hermes/README.md` | Hermes/MCP/web search |
| `../projects/home-assistant/TROUBLESHOOTING.md` | диагностика |

## Исследования (не обязательны для rebuild)

| Документ | Чем владеет |
|---|---|
| `RESEARCH-OWN-ASSISTANT.md` | Pulsar/HA/Spotify/MCP/wake word |
| `DEPLOY-RICARDO-PURISO.md` | внедрение Puriso: хеши, проверки, backup, откат и этапы дообучения | при изменении TTS/обучения |
| `RESEARCH-RICARDO-TTS-ALTERNATIVES.md` | новый поиск после прослушивания Piper: Puriso, Utrobin, Vosk, свой Piper | при выборе альтернатив |
| `RESEARCH-RICARDO-TTS.md`, `PLAN-RICARDO-TTS.md`, `RICARDO-TTS-HANDOFF.md` | TTS-ресерч/план/передача |
| `TTS-WAKE-WORD-RESEARCH.md` | исторический TTS/wake v1 |
| `RESEARCH-RICARDO-WAKE-WORD.md`, `TRAINING-RICARDO-WSL.md` | wake word «Рикардо» |

## Тесты / инструменты

- `../docs/TESTING.md` — стратегия, live E2E;
- `../reports/TEST-COVERAGE.md` — покрытие;
- `../projects/station/ble-gateway/README.md` — BLE gateway;
- `../projects/station/tools/quasar-root-kit/` — root kit;
- `../archive/legacy/` — старые проекты/Quasar/v3rt3p/Mini2;
- `../archive/logs/` — старые статусы.

## Правила

1. **Факт** → `STATE.md`; **задача** → `PENDING.md`; **событие** → `HISTORY.md`.
2. **Процедура** → `OPERATIONS.md`/`REPLICATION-GUIDE.md`/`REBUILD.md`.
3. **Дизайн/решение** → `ARCHITECTURE.md`/`DECISIONS.md`/`SECURITY.md`.
4. **Секреты** — только `secrets/`/HMAC; в docs — имена, не значения.
5. Не копировать текущий статус в README/ARCHITECTURE/SECURITY/OPERATIONS.
6. Сессии — только краткий статус в `logs/` + запись в `HISTORY.md`; без
   транскриптов в docs.
7. Перед публикацией — `SENSITIVE-DATA.md` + secret-pattern тест.

## Точки входа

- Состояние: `STATE.md`
- Восстановление: `REBUILD.md`
- Решения: `DECISIONS.md`
- Команды: `OPERATIONS.md`
- Задачи: `PENDING.md`
- История: `HISTORY.md`
- Remote: `REMOTE-ACCESS.md`
- Неттоп/HA: `../projects/home-assistant/NETTOP.md`
- Тесты: `../docs/TESTING.md`
