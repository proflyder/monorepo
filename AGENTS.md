# Работа с yandex-station-lab

## Agent orchestration and model policy

- The primary Codex thread owns planning, architecture, task decomposition,
  coordination, review, and acceptance. Delegate research, implementation, and
  test execution to BB child threads using provider `acp-opencode`.
- Only the primary thread delegates. A worker thread (`bb status` shows a
  parent thread) never runs `bb thread spawn` and never hands its task to
  another thread or model: it does the assigned work itself, whatever its
  provider (Claude Code, Codex, opencode). If the task is too large, it reports
  that to the parent instead of splitting it.
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
  when appropriate. Ignored files (secrets, local tools, build outputs) are not
  copied into worktrees.
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
- The scheduler owns live schedules and `ha-bridge` is their only executor;
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
- Merge в `develop` — только `mise run pr:merge <N>`: скрипт подтягивает
  свежий `develop` в ветку и ждёт `ci-ok` на итоговом head; ручной
  `gh pr merge` на уехавшую базу даёт непроверенный CI tree.
- Не переписывать историю и не откатывать чужие изменения; в проекте могут
  одновременно работать несколько чатов. Перед правкой читать актуальные
  файлы; чужие незакоммиченные изменения сохранять.
- Изменения сохранять внутри этого проекта; документацию вести здесь.

## Вложенные upstream-репозитории

`resources/` содержит независимые checkout сторонних проектов. Их `.git`,
историю, staged/unstaged изменения и remote сохранять; не превращать их
автоматически в submodule и не включать как gitlink в корневой репозиторий.
Существующие upstream remote не означают разрешения публиковать наши изменения.

Активный Pulsar — first-party код в `projects/station/pulsar/` (AGPL-3.0;
`LICENSE`, README, docs и RICARDO.md сохранены). Upstream `Morvex885/pulsar`
вливается вручную патчем:

```bash
git -C <upstream-checkout> diff vOLD..vNEW > /tmp/pulsar-upstream.patch
git apply -3 --directory=projects/station/pulsar /tmp/pulsar-upstream.patch
```

затем обычный squash PR. Релиз Pulsar — изменение
`projects/station/pulsar/VERSION`: PR CI собирает обе архитектуры и публикует
`ghcr.io/proflyder/pulsar-firmware:tree-<hash>` с provenance; merge в
`develop` (`publish_when` — только VERSION) только продвигает этот digest на
теги `<version>` и `:latest`, затем `deploy-station` после approve в
Environment `Station` катит его на колонку. Повторной сборки в деплое нет.
GitHub Releases и теги в монорепо запрещены. Процедуры —
`docs/OPERATIONS.md` и `projects/station/pulsar/RICARDO.md`. Правила самого
Pulsar — `projects/station/pulsar/AGENTS.md`.

## Монорепозиторий и Bazel

- Горизонтальная структура: `solutions/` — переиспользуемые решения,
  `packages/` — общий код/контракты, `projects/` — композиции и deployment.
  `projects/home-assistant/` — HA runtime, `projects/station/` — станция/Pulsar,
  `resources/` — только внешние checkout'ы.
- Корневой build contract — `MODULE.bazel` + Bazelisk. Не добавлять
  `WORKSPACE` и не дублировать toolchain-настройки по приложениям.
- First-party Rust собирается `rules_rust`. Весь Rust — один корневой Cargo
  workspace (`/Cargo.toml`, `/Cargo.lock`, `/rust-toolchain.toml`,
  `@crates//`), включая `projects/station/ble-gateway` (`zbus`/`tokio`, armhf
  через `//platform/bazel/zig:armhf_linux`) и Windows logon provider; общий
  код с несколькими потребителями — `packages/rust/`. Go в репозитории нет.
- После добавления Rust-зависимости обновлять `Cargo.toml`/`Cargo.lock` и
  `crate-universe.lock` (`CARGO_BAZEL_REPIN=1 bazel build ...`), затем
  проверять `bazel build` и `bazel test`.
- Не коммитить Bazel output base, `bazel-*` symlinks и тяжёлые кэши;
  `MODULE.bazel.lock` — коммитить для воспроизводимости.
- Solution не должен импортировать project или содержать project-specific
  endpoint/DB path. Один и тот же solution запускается в разных projects.
- Архитектурные границы и выбор языков — `docs/MONOREPO.md`.

## GitOps под Flux

- Инфраструктура, отданная Flux (`projects/home/kubernetes/flux/`):
  платформа (`platform`: ESO → cert-manager → Traefik → общие маршруты),
  observability, Vaultwarden и Nexus. Меняется только через Git и merge в
  `develop`. Ручной `kubectl apply` этих объектов не использовать — Flux
  вернёт состояние из Git. OpenBao, host-firewall (nftables) и Cloudflare DNS
  остаются ручными.
- Bootstrap/секреты/статус — `projects/home/kubernetes/flux/README.md`;
  деплой-ключ и настройки живут в OpenBao (`secret/k8s/flux-system/*`), не в
  Git. Сервисы приложений применяет Flux из pins; входящего доступа CI в
  K3s нет (этап 6.7 завершён).

## Совместимость функциональности

Pulsar развиваем поверх актуального общего состояния всех сессий. Нельзя
восстанавливать старую сборку или исходники поверх свежего исправления.
Сохранять on-device «Рикардо», pre-roll, ALSA ABI shim, mute, HA/MA,
duck/resume и передачу wake-команд в HA. Текущая версия — в `docs/STATE.md`.

## Секреты: только OpenBao

- Единственный источник секретов — OpenBao (`secret/k8s/*` для ESO,
  `secret/ci/*` для CI, `secret/infra/*` для доступа владельца). Локально
  лежат только `secrets/openbao/operator.token` (политика `operator-kv`:
  чтение/запись `secret/{ci,k8s,infra}/*`, без delete), `secrets/openbao/addr`
  и SSH-ключи `~/.ssh/home-lab/` (алиасы `ssh -F ~/.ssh/home-lab/config
  pve|k3s|haos`). Карта путей — `docs/SECRETS.md` и
  `platform/workstation/catalog.json`; рабочее место — `docs/WORKSTATION.md`.
- Нужен доступ или креды (SSH к хосту, токен, пароль, адрес) — сначала
  каталог, а не перебор ключей и не вопрос владельцу: `docs/SECRETS.md`
  (раздел «Доступ к хостам») → `secrets/NETWORK.md` (адреса, пользователи) →
  `~/.ssh/home-lab/` (ключи после `mise run secrets:pull`) → OpenBao
  `secret/infra/*` через `operator.token`. Просить у владельца — только если
  в каталоге этого нет; найденный пробел дописать в каталог.
- Инструменты — только из `mise.toml` (`mise install`); не скачивать бинарники
  вручную и не класть их в `local-data/`.
- Скрипты деплоя и обслуживания запускать только через `mise run admin:*`
  (`mise tasks | grep admin:`): секреты передаёт fnox по профилю скрипта
  (`fnox.toml`, только ссылки на OpenBao), во временных файлах, которые
  удаляются после команды; туннель к API K3s поднимается сам. Новый скрипт с
  секретами — профиль в `fnox.toml` + задача `admin:*`. Разовая команда —
  `mise run secrets:exec -- -P <profile> exec -- <cmd>`. Запасной путь для
  файла вручную — `mise run secrets:file`/`secrets:clean`. Значения не
  печатать и не копировать в другие файлы, логи, чат.
  `mise run doctor` показывает, чего не хватает; `secrets:login` (пароль +
  TOTP) выполняет только человек.
- Кластер K3s: только `mise run kubectl -- <args>` и `mise run helm -- <args>`
  (профиль `kube`: admin kubeconfig из OpenBao + SSH-туннель к API). Голый
  `kubectl`/`helm` на рабочей машине смотрит в чужие кластеры (текущий
  контекст по умолчанию — не домашний), его не использовать и `~/.kube/config`
  не менять. Логи и `exec` — тем же путём, например
  `mise run kubectl -- -n nexus logs nexus-0 -c nexus`.
- Запись: GET → merge → POST с `options.cas`. Новый путь — сразу в OpenBao
  плюс запись в `platform/workstation/catalog.json` и `docs/SECRETS.md`
  (`mise run secrets:check` должен быть чистым); локальные копии не создавать.
- Admin OpenBao (политики, auth) — только по запросу человека; recovery keys и
  seal key на диске больше не хранятся (копии у владельца в Bitwarden).
- После изменений в OpenBao владелец обновляет копию в Vaultwarden:
  `mise run vault:sync` (нужен `BW_SESSION`).

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
  merge PR. Никогда не отправлять архивные refs или теги.
- Локальный `core.hooksPath=.githooks` включает pre-push: только текущая
  рабочая ветка в одноимённую, без удаления; проверка чистого дерева, Gitleaks исходящих коммитов и аудита.
- Не обходить hook через `--no-verify`, смену hooksPath или mirror push.
- В публичных документах и примерах использовать обезличенные пути/имена
  и TEST-NET `192.0.2.0/24`. Оригинальные deployment-конфиги — только secrets/.
- После правок запускайте `node platform/quality/audit_repository.mjs` и Gitleaks.
  Не добавлять широкие исключения сканера; каждое исключение проверять.

## Размещение тестов

- Unit/компонентные тесты размещать рядом с проверяемым кодом.
- E2E размещать в `projects/<project>/e2e/`, запуск — проектным скриптом.
- Не создавать общую корневую папку tests. Общие проверки репозитория
  и оркестрация — `platform/quality/`; правила запуска — `docs/TESTING.md`.
