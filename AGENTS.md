# Работа с yandex-station-lab

## Agent orchestration and model policy

- The primary Codex thread owns planning, architecture, task decomposition,
  coordination, review, and acceptance. Delegate research, implementation, and
  test execution to BB child threads using provider `acp-opencode`.
- Default worker model is `opencode-go/deepseek-v4.1-flash`. Research workers use
  `mode=plan`; implementation workers use `mode=build`. Prefer Flash throughout.
- Never launch `opencode-go/deepseek-v4-pro`, any other Pro model, or a fallback
  model without explicit user approval for that specific escalation. First explain
  why Flash is insufficient and which bounded task needs Pro. A timeout, failure,
  or quota limit does not authorize escalation.
- Every worker assignment specifies objective, relevant context and paths,
  permitted edit scope, constraints, acceptance criteria, validation commands,
  and the expected report: findings, changed files, checks/results, blockers.
- Worker threads must read the applicable `AGENTS.md` files and current sources,
  preserve other sessions' changes, and obey the existing local Git, privacy,
  Pulsar, and device-operation rules.
- Use `bb thread spawn` with `--parent-self` and explicit project, environment,
  provider, and model. Reuse the existing environment for sequential
  shared-checkout tasks; use separate worktrees only when appropriate. Do not
  assume ignored or nested Pulsar sources are copied into worktrees.
- Start with one write worker per shared checkout. Parallel research is allowed;
  concurrent writers require nonoverlapping file ownership or deliberate
  isolation.
- The primary agent inspects actual diffs and validation evidence before
  accepting changes. Worker self-reports alone are insufficient. The primary owns
  final local commits; workers do not commit unless explicitly assigned.
- If delegation is unavailable, report the blocker; do not silently switch models
  or take over implementation. The primary may directly edit orchestration
  instructions and perform review/integration checks.

## Локальный Git — осознанная конфигурация

Репозиторий готовится к публикации на GitHub. До отдельного запроса
работаем локально: история изменений, сравнение версий и восстановление.
Отсутствие remote, PR и CI не является ошибкой.

- Не создавать remote, не публиковать проект, не выполнять push без прямого
  запроса пользователя. Не предлагать публикацию как обязательный шаг.
- После законченного логического изменения делать локальный коммит с понятным
  описанием, предварительно проверив diff и подходящие проверки.
- Не переписывать историю и не откатывать чужие изменения; в проекте могут
  одновременно работать несколько чатов. Перед правкой читать актуальные файлы.
- Изменения сохранять внутри этого проекта; документацию вести здесь.

## Вложенные upstream-репозитории

`resources/` содержит независимые checkout сторонних проектов. Их `.git`,
историю, staged/unstaged изменения и remote сохранять; не превращать их
автоматически в submodule и не включать как gitlink в корневой репозиторий.
Существующие upstream remote не означают разрешения публиковать наши изменения.

Активный Pulsar: `projects/station/upstream/pulsar/`. База и текущие локальные изменения
фиксируются в корневой истории через `projects/station/patches/pulsar/`:

```bash
python3 projects/station/scripts/snapshot-pulsar.py
```

Запускать после правок Pulsar и перед корневым коммитом. Скрипт сохраняет
базовый commit, полный diff относительно HEAD (включая staged/unstaged) и
новые исходники, не изменяя индекс или историю вложенного репозитория.
Следовать также `projects/station/upstream/pulsar/AGENTS.md`; прямой запрос пользователя
о локальной работе имеет приоритет над upstream-процессом PR/push.

## Монорепозиторий и Bazel

- Горизонтальная структура: `solutions/` — переиспользуемые решения,
  `packages/` — общий код/контракты, `projects/` — композиции и deployment.
  `projects/home-assistant/` — HA runtime, `projects/station/` — станция/Pulsar,
  `resources/` — только внешние checkout'ы.
- Корневой build contract — `MODULE.bazel` + Bazelisk. Не добавлять
  `WORKSPACE` и не дублировать toolchain-настройки по приложениям.
- Go-сервисы собираются `rules_go`, Rust-крейты — `rules_rust`. Для
  существующего `projects/station/ble-gateway` миграция в Bazel — отдельный этап.
- После добавления Go-зависимости обновлять `go.mod` и `go_deps`/Gazelle,
  затем проверять `bazel build` и `go test`.
- Не коммитить Bazel output base, `bazel-*` symlinks и тяжёлые кэши;
  `MODULE.bazel.lock` — коммитить для воспроизводимости.
- Solution не должен импортировать project или содержать project-specific
  endpoint/DB path. Один и тот же solution запускается в разных projects.
- Архитектурные границы и выбор языков — `docs/MONOREPO.md`.

## Совместимость функциональности

Pulsar развиваем поверх актуального общего состояния всех сессий. Нельзя
восстанавливать старую сборку или исходники поверх свежего исправления.
Сохранять on-device «Рикардо», pre-roll, ALSA ABI shim, mute, HA/MA,
duck/resume и передачу wake-команд в HA. Текущая версия — в `docs/STATE.md`.

## Документация и приватные данные

- `docs/STATE.md` — текущие факты; `OPERATIONS.md` — команды и откат;
  `HISTORY.md` — история; `PENDING.md` — оставшаяся работа.
- Секреты, env с ключами, голосовые записи, логи, дампы, тяжёлые бэкапы и
  бинарники сборки не коммитить. Исключения заданы в `.gitignore`.
- В `artifacts/wakeword/ricardo-v1/` версионируется компактная модель и её
  метаданные; сборки станции остаются на диске вне Git.
- Не выводить значения секретов в чат или диагностические отчёты.
- Git не заменяет резервное копирование: игнорируемые recovery-данные
  сохранять отдельно, не удалять ради чистого git status.

## Проверки и изменения станции

Проверки выбирать по изменению. Документация/Git-настройки не требуют
перезапуска колонки или повторного полного live-теста. Для кода использовать
тесты рядом с кодом и профильные проверки. Не менять устройство, музыку,
громкость или будильник только ради обслуживания Git.

## Публичная ветка и защита публикации

- Публиковать только `develop`. `local-history` содержит приватную старую
  историю; никогда не отправлять её, другие архивные refs или теги.
- Локальный `core.hooksPath=.githooks` включает pre-push: запрет иных refs,
  проверку отсутствия общей истории с архивом и Gitleaks всех коммитов develop.
- Не обходить hook через `--no-verify`, смену hooksPath или mirror push.
- В публичных документах и примерах использовать обезличенные пути/имена
  и TEST-NET `192.0.2.0/24`. Оригинальные deployment-конфиги — только secrets/.
- После правок запускайте `python3 platform/quality/audit_repository.py` и Gitleaks.
  Не добавлять широкие исключения сканера; каждое исключение проверять.

## Размещение тестов

- Unit/компонентные тесты размещать рядом с проверяемым кодом.
- E2E размещать в `projects/<project>/e2e/`, запуск — проектным скриптом.
- Не создавать общую корневую папку tests. Общие проверки репозитория
  и оркестрация — `platform/quality/`; правила запуска — `docs/TESTING.md`.
