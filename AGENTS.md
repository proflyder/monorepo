# Работа с yandex-station-lab

## Agent orchestration and model policy

- The primary Codex thread owns planning, architecture, task decomposition,
  coordination, review, and acceptance. Delegate research, implementation, and
  test execution to BB child threads using provider `acp-opencode`.
- Default worker model is `opencode-go/deepseek-v4.1-flash` with
  `--reasoning-level low`. Research workers use `mode=plan`; implementation
  workers use `mode=build`. Prefer Flash throughout; raise the reasoning level
  or switch model only with explicit user approval.
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
- Use `bb thread spawn` with `--parent-self`, `--reasoning-level low`, and
  explicit project, environment, provider, and model. Reuse the existing
  environment for sequential shared-checkout tasks; use separate worktrees only
  when appropriate. Do not assume ignored or nested Pulsar sources are copied
  into worktrees.
- Start with one write worker per shared checkout. Parallel research is allowed;
  concurrent writers require nonoverlapping file ownership or deliberate
  isolation.
- The primary agent inspects actual diffs and validation evidence before
  accepting changes. Worker self-reports alone are insufficient. The primary owns
  final local commits; workers do not commit unless explicitly assigned.
- If delegation is unavailable, report the blocker; do not silently switch models
  or take over implementation. The primary may directly edit orchestration
  instructions and perform review/integration checks.

## Home platform architecture

- Build the scheduler as a reusable task scheduling and delivery service.
  Alarms, reminders, and timers are domain conveniences over durable scheduling
  and queues. Keep HA, Hermes, audio, and mobile concerns out of the core.
- Start with SQLite-backed durable queues and explicit leases/acknowledgements.
  Introduce an external broker only for a demonstrated deployment requirement;
  preserve the versioned protobuf API and idempotency semantics.
- The home server owns execution. Android manages server state; phone reminder
  notifications are an optional delivery channel, not the scheduling authority.
- Android lives in `projects/mobile/android`: Kotlin, root Bazel/Bazelisk,
  Metro compiler-plugin DI, Android Views, one Activity, and Fragment-based
  navigation behind typed contracts. Preserve feature API/implementation
  boundaries, clean architecture, and reusable Kotlin/navigation utilities.
- The approved public application ID is `dev.proflyder.home`, derived from the
  user's public domain `proflyder.dev`. Endpoints are configurable LAN
  addresses or private HTTPS names under the home suffix. Remote access goes
  through the self-hosted VPN (ocserv, external Cisco Secure Client/OpenConnect
  app); use system routing and verified TLS, without embedding a VPN client.
- HA integrates through an executor adapter. Hermes and future projects should
  share one extensible MCP gateway with domain tool groups, rather than one MCP
  server per project. Start MCP only when an actual client needs it.
- Keep the current HA scheduler authoritative until an explicit, tested cutover;
  never enable a second executor for the same live schedules implicitly.

## Git и pull request

Репозиторий опубликован на GitHub (`origin`). `develop` меняется только через
PR со squash-merge и зелёным `ci-ok`; merge в `develop` = деплой затронутых
юнитов домой. Подробно — `docs/GIT.md`.

- Каждое изменение — рабочая ветка `<type>/<name>` от свежего
  `origin/develop` (или отдельный `git worktree`), коммиты с понятным
  описанием после проверки diff и подходящих проверок.
- Push рабочей ветки и PR — только по задаче пользователя или в рамках
  согласованного потока; прямой push в `develop` запрещён. Не создавать
  новые remote.
- Не переписывать историю и не откатывать чужие изменения; в проекте могут
  одновременно работать несколько чатов. Перед правкой читать актуальные
  файлы; чужие незакоммиченные изменения сохранять.
- Изменения сохранять внутри этого проекта; документацию вести здесь.

## Вложенные upstream-репозитории

`resources/` содержит независимые checkout сторонних проектов. Их `.git`,
историю, staged/unstaged изменения и remote сохранять; не превращать их
автоматически в submodule и не включать как gitlink в корневой репозиторий.
Существующие upstream remote не означают разрешения публиковать наши изменения.

Активный Pulsar: Rust-форк `projects/station/upstream/pulsar-rust/` (ветка
`feat/ricardo-rust`, база upstream `v0.5.2` = `4cba44a`). База и текущие
локальные изменения фиксируются в корневой истории через
`projects/station/patches/pulsar-rust/`:

```bash
python3 projects/station/scripts/snapshot-pulsar.py
```

Запускать после правок Rust Pulsar и перед корневым коммитом. Скрипт сохраняет
pinned base commit, полный delta от базы до текущего HEAD (committed, staged,
unstaged и новые исходники), не изменяя индекс или историю вложенного
репозитория. Legacy C++ checkout, patch и binaries заархивированы private
в `secrets/local/recovery/`; активный worktree содержит только Rust v0.5.
Следовать также `projects/station/upstream/pulsar-rust/AGENTS.md`; прямой запрос
пользователя о локальной работе имеет приоритет над upstream-процессом PR/push.

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

- `docs/STATE.md` — текущие факты; `OPERATIONS.md` — актуальные команды;
  `PENDING.md` — только открытая работа; `DECISIONS.md` — действующие решения.
  История изменений остаётся в Git. Не сохранять завершённые планы миграций,
  разовые исследования, аудиты и сгенерированные отчёты в исходном дереве.
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

## Публикация и защита

- Публиковать только рабочие ветки `<type>/<name>`; `develop` обновляется
  merge PR. `local-history` содержит приватную старую историю; никогда не
  отправлять её, другие архивные refs или теги.
- Локальный `core.hooksPath=.githooks` включает pre-push: только текущая
  рабочая ветка в одноимённую, без удаления; проверка отсутствия общей
  истории с архивом, чистого дерева, Gitleaks исходящих коммитов и аудита.
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
