# TESTING

Тесты — рядом с кодом; `platform/quality/` — общие проверки и аудит; E2E —
`projects/<project>/e2e/`. Общей корневой папки tests нет.

## Быстрый старт

```bash
python3 -m pip install -r requirements-test.txt
mise run test          # hermetic, без сети/станции — gate
mise run test <unit>   # тесты одного CI-юнита (список — `mise run units`)
mise run test:bazel    # bazel test //...
mise run coverage      # reports/TEST-COVERAGE.md (не в Git)
mise run test:e2e      # live HA/BLE read-only; нужны токены
mise run check         # pre-push: test + audit + Gitleaks
```

`mise run test:e2e` требует `HA_TOKEN`, опционально
`STATION_CONTROL_TOKEN`/`HERMES_API_KEY`; токены не коммитить. CI/quality
проверки — Node 20+ (`node --test`), без npm-зависимостей.

Python-тесты — stdlib `unittest`. Каталоги задаёт
`platform/quality/run-tests.sh`: `python3 -m unittest discover -s <каталог>
-p 'test_*.py'`. `-s` кладёт каталог в `sys.path`, и тесты импортируют
соседний модуль по имени, поэтому запуск одного файла из корня падает на
импорте — это не поломка. Запускать так:

```bash
python3 -m unittest discover -s projects/home/kubernetes/openbao -p 'test_*.py' -v
```

На чистом checkout `mise run test` печатает `SKIP` для тестов, которым нужен
игнорируемый nested checkout `projects/station/resources/quasar-connect-bot`.

Live E2E:

```bash
export HA_URL=http://192.0.2.28:8123
export HA_TOKEN='...'
mise run test:e2e
STATION_E2E_MUTATE=1 mise run test:e2e
STATION_E2E_AUDIO=1 mise run test:e2e
```

Audio-тест: короткий будильник на низкой громкости, «стоп» + resume;
teardown восстанавливает source/volume.

## Слои

| Слой | Что проверяет |
|---|---|
| HA static | YAML, duplicate keys, entities/services/scripts, Jinja, intents |
| Hermes | model/provider, MCP allowlist, web search, voice sanitizer |
| Egress guard | IPv4/IPv6 allowlist, NTP, idempotency |
| Static gates | shell syntax, ShellCheck errors, Python syntax, secret patterns |
| Scheduler / HA bridge | SQLite store/tick/planner, Connect API, auth, очереди, at-least-once, ledger |
| BLE gateway | token, Wi-Fi validation/rollback, WPA, GATT, HTTP auth/status |
| Windows logon provider | protocol/HMAC/config-векторы, PE exports/imports, install-скрипты, self-test на ПК |
| Heartbeat Worker | grace-логика, auth, KV transitions, Telegram (моки) |
| MCP gateway | Rust-юнит (фейки backend'ов), каталог↔RBAC, fake SSH |
| Voice routing | `solutions/voice-proxy`, `test_voice_routing_contract.py`, `test_bb_dispatch.py` |
| Pulsar | `bazel test //projects/station/pulsar/...` (fmt/clippy/tests, Linux) и кросс-сборка `aarch64`/`armhf` |
| HA / Gateway live | API, entities, text intents, bearer auth, method guards |

## Сервисы и голос

```bash
bazel test //solutions/... //packages/... //platform/deploy-rs/...
cargo test -p scheduler -p ha-bridge -p voice-proxy -p health-exporter
node --test solutions/heartbeat/src/*.test.js
node --test solutions/bb-home-dispatcher/tests/*.test.js
(cd solutions/bb-profiles && npm test)
python3 -m unittest discover -s projects/home-assistant/ha-voice -p 'test_*.py' -v
python3 -m unittest discover -s projects/home-assistant/deploy -p 'test_*.py' -v
python3 -m unittest discover -s projects/home-assistant/hermes -p 'test_*.py' -v
python3 -m unittest discover -s projects/home/kubernetes/mcp-gateway -p 'test_*.py' -v
python3 -m unittest discover -s projects/home/proxmox -p 'test_*.py' -v
```

Все герметичны: SQLite во временном каталоге, HA/Connect/SSH/плагин bb —
локальные фейки. `bazel test //platform/deploy-rs:parity` сравнивает argv и
payload fake kubectl с замороженными фикстурами.

Процессный E2E scheduler + bridge (настоящие бинарники, fake HA на loopback):

```bash
projects/home/e2e/run-scheduler-smoke.sh
```

Проверяет auth reject, timer → FIRE → fake HA → DELIVERED, generic task в
очереди `tasks`, STOP. Живой звук не проверяет.

Живая проверка голосового маршрута — тихий `conversation.process` через
builtin-агент (`conversation.home_assistant`), сверка через
`ListSchedules`/`ListTimers`.

Hermes live:

```bash
export HERMES_API_URL=http://127.0.0.1:8642 HERMES_API_KEY='...'
python3 projects/home-assistant/e2e/live_hermes.py -v
HERMES_E2E_CHAT=1 python3 projects/home-assistant/e2e/live_hermes.py -v
STATION_E2E=1 HA_HERMES_AGENT_TEST=1 python3 projects/home-assistant/e2e/live_ha.py -v
```

Прямой Hermes `/health` не заменяет проверку регистрации
`conversation.hermes_agent` в HA.

## Pulsar

```bash
bazel build //projects/station/pulsar/...
bazel test  //projects/station/pulsar/...
bazel build -c opt --platforms=//platform/bazel/zig:armhf_linux \
  --define=pulsar_version="$(node platform/ci/version.mjs --unit pulsar)" \
  //projects/station/pulsar/satellite:station_binary
```

На macOS цели несовместимы и пропускаются (`target_compatible_with`); реальные
тесты — Linux. `clippy` линтует `[workspace.lints]` корневого `Cargo.toml`
(`extract_cargo_lints`), поэтому Cargo и Bazel дают один набор
предупреждений. Кросс-сборка кладёт `pulsar` (stripped, `.gnu_debuglink`) и
`pulsar.debug` в `bazel-bin/.../satellite/`.

## Android

```bash
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
bazel test --config=android //projects/mobile/android:android_tests
bazel build --config=android //projects/mobile/android:app
bazel test --config=android //projects/mobile/android:android_lint
bazel test --config=android //projects/mobile/android:detekt
bazel test --config=android //platform/android/...
```

- `--config=android` обязателен (решение 50).
- Robolectric-тесты — один `robolectric_test` на модуль
  (`home_android_local_test` с `test_classes`): макрос генерирует JUnit-suite,
  android-all общий (`//platform/android:robolectric_android_all_sdk35`).
  Чистые JVM-тесты остаются `kt_jvm_test`. `JavaDeployJar` помечен `no-cache`:
  deploy-jar'ы (~240 МБ каждый) не попадают в disk cache, Android-кэш CI
  пишет только `warm-android` (`bazel test` + GC до 4 ГБ) под ключом
  `bazel-android-<os>-shared-*`.
- Тяжёлые `robolectric_test` шардируются: `shard_count` у
  `home_android_local_test` (по умолчанию 1; 3 у `app` и `core/designsystem`,
  2 у модулей, где тест идёт дольше ~20 с). Шарды — отдельные JVM на одном
  deploy-jar; JUnit-раннер Bazel фильтрует тесты по
  `TEST_SHARD_INDEX`/`TEST_TOTAL_SHARDS`, каждый тест попадает ровно в один
  шард. Новый шард добавляет ~5 с старта Robolectric, поэтому шардировать
  стоит только таргеты длиннее ~20 с. Тест не должен зависеть от порядка и
  соседних классов: общие файлы — только в `TEST_TMPDIR`/`filesDir`,
  порты — эфемерные.
- Каждая JVM Robolectric ограничена `-Xmx1g` и резервирует у Bazel
  `resources:memory:1536`, так что параллельные тесты не выходят за память
  раннера. В CI (4 vCPU, 16 ГБ) тест-шаг `android-app` идёт с
  `--local_test_jobs=4`; persistent workers для тестов не используются.
- Каждый `*_test` под `projects/mobile/android/**` входит в `android_tests`
  или `test_targets` юнита, иначе `mise run ci:units`
  (`androidSuiteCoverageProblems`) красный.
- `android_lint` (нужен `cmdline-tools;23.0`) и `detekt` — отдельные CI-юниты,
  не в `android_tests`. Lint — partial analysis: кэшируемое действие
  `AndroidLintAnalyze` на модуль и `--report-only` в тесте; правка одного
  модуля переанализирует только его. Ошибки считаются по XML-отчёту.
- `AndroidDesignSystemTests` в `platform/quality/test_static.py` запрещает
  стили `Widget.*` и компоненты без стиля дизайн-системы вне
  `core/designsystem` — см. `projects/mobile/android/core/designsystem/README.md`.

## BLE gateway

```bash
export STATION_CONTROL_TOKEN="$(adb -s 192.0.2.13:5555 shell cat /data/quasar/ble-gateway/control-token | tr -d '\r\n')"
export STATION_GATEWAY_URL=http://192.0.2.13:8099
mise run test:e2e
```

Read-only: 401 без токена, `/status`, 405 на неверные методы.

## Windows logon provider

```bash
cd solutions/windows-logon-provider
./run-tests.sh
./build.sh
python3 -m unittest discover -s . -p 'test_*.py' -v
```

`run-tests.sh` — host-векторы и rustfmt/clippy (host и Windows); `build.sh` —
кросс-сборка, обнуление PE-таймстемпа и проверка exports/imports
(`pe_artifacts.py`); Python — статический контракт install-скриптов. На ПК:
`.\cp-selftest.exe`. Живой чеклист: «включи комп» → рабочий стол без PIN;
кнопка → обычный вход; wakegrant недоступен → обычный вход без плитки.

## Не автоматизировано

Реальный BlueZ/ALSA/Wi-Fi rollback на станции, установка Pulsar вне
`deploy-station`, микрофон/ASR, реальный звук. Структурная проверка HA не
равна runtime coverage.

## CI (GitHub Actions)

Причины решений — `DECISIONS.md` 36, 49–51, 54, 59.

| Триггер | Обёртка | Reusable |
|---|---|---|
| `pull_request` в `develop` | `ci.yml` (публикует `ci-ok`) | `_ci.yml` |
| `workflow_dispatch` (все юниты) | `ci-manual.yml` | `_ci.yml` |
| `workflow_dispatch` (Android-релиз) | `android-release.yml` | `_android.yml` + `_android-release.yml` |
| `push` в `develop` | `deploy-home.yml` | `_deploy-home.yml` → `_pin.yml`, `_native.yml`, `_station-deploy.yml` |
| `workflow_dispatch` (допин develop) | `deploy-home-manual.yml` | `_deploy-home.yml` |
| `workflow_dispatch` (пересборка) | `deploy-services.yml` | `_unit.yml`, `_pin.yml`, `_native.yml`, `_station-deploy.yml` |
| `schedule` (уборка) | `cleanup.yml` | `_cleanup.yml` |
| `workflow_dispatch` (уборка, dry run) | `cleanup-manual.yml` | `_cleanup.yml` |

Push в `develop` CI не запускает: туда попадают только PR с зелёным `ci-ok`.

### Правило: один триггер на воркфлоу

Ровно одно событие в `on:`; reusable — только `_*.yml` с `workflow_call`, без
ветвления по `github.event_name` (явные inputs `mode`, `dry_run`,
`same_repository`, `distribution`, `save_cache`). Новый триггер — новый тонкий
файл, вызывающий существующий `_*.yml`; общие шаги — composite action в
`.github/actions/<name>/`. Проверка — `workflowTriggerProblems`
(`mise run ci:units`).

### Правило: caller выдаёт права reusable

Права caller-job (job-level, иначе workflow-level, иначе ничего) должны быть
не меньше union прав всех job'ов reusable, рекурсивно, по уровню
`none < read < write`. Иначе GitHub падает на `startup_failure` до старта
(`actionlint` не ловит). Проверка — `workflowCallerPermissionProblems`.

### Правило: параллельные шаги и полный отчёт

Независимые шаги job'а идут фоновыми шагами GitHub Actions
([changelog 2026-06-25](https://github.blog/changelog/2026-06-25-actions-steps-can-now-be-run-in-parallel),
[`background`](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idstepsbackground),
[`wait`](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idstepswait),
[`wait-all`](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idstepswait-all)):

- Подготовка (Java, Android SDK, mise, восстановление кэшей, Nexus, apt) —
  `background: true`, затем `wait-all:`. Ограничение: `setup-bazel`
  перезаписывает `~/.bazelrc`, поэтому он идёт обычным шагом до фоновых
  `bazel-repository-cache` и `nexus-cache`, которые дописывают туда строки.
- Независимые проверки (quality, тест + сборка native) — фоновые шаги или
  шаги с `continue-on-error: true`; после `wait-all:` шаг-отчёт
  `platform/ci/step_report.mjs` (по `toJSON(steps)`) пишет таблицу в
  `$GITHUB_STEP_SUMMARY` и падает со списком **всех** упавших проверок.
  Fail-fast нет: упавшая проверка не останавливает соседние.
- Bazel не запускается параллельными шагами на один output base (сервер
  держит блокировку): параллелизм — внутри одного вызова. Android и
  Bazel-юниты собирают и тестируют всё одним `mise run ci:unit <unit>`:
  `bazel test --keep_going --target_pattern_file=…` над тестами юнита и его
  сборочными целями. Сборочный паттерн (`//packages/rust/...`) остаётся
  паттерном, а чужие тесты под ним (`bazel query 'tests(set(P)) except
  set(T)'`) идут отрицательными паттернами `-//label`: они только собираются,
  как в отдельном `bazel build`. Явно названную несовместимую цель Bazel
  считает ошибкой, через паттерн — пропускает (BEP `aborted: SKIPPED`), и
  отчёт это ошибкой не считает. `platform/ci/bazel_report.mjs` читает BEP и группирует упавшие
  цели по виду проверки (`build`, `test`, `lint`, `detekt`, `clippy`,
  `format` — по имени цели) с хвостом `test.log`/stderr каждой.
- Не больше 10 фоновых шагов одновременно, у каждого `id`, каждый закрыт
  `wait`/`wait-all`; `parallel:` не используется (actionlint не проверяет
  вложенные шаги), в composite actions фоновых шагов нет. Проверка —
  `workflowParallelStepProblems` (`mise run ci:units`).
- actionlint 1.7.12 не знает `background`/`wait-all`: `mise run
  lint:actions` игнорирует ровно эти два сообщения, структуру проверяет
  `workflowParallelStepProblems`.
- `mise run test <unit>` для Python-юнита выполняет все сьюты и печатает
  список упавших (стоп только на подготовительной сборке `home-deploy`).
- Matrix — всегда `fail-fast: false`.

### Маршрутизация по юнитам

Единица запуска — юнит из `platform/ci/units.mjs` (единственный источник:
`id`, `platform`, `paths`, команды, Bazel-корни, `invalidations`, `deploy`).
Проверки внутри workflow — те же `mise run test|build <unit>`, что локально.

| Платформа | Reusable | Что делает |
|---|---|---|
| android | `_android.yml` | один `bazel test --keep_going`: APK, тесты, Android Lint, detekt (`android-app`); отчёт по видам проверок; APK artifact |
| unit | `_unit.yml` | `kind=bazel` (один `bazel test --keep_going` + отчёт) / `python`, затем образы, push `tree-<hash>` и attestation в том же job |
| python | `_python.yml` | все затронутые Python-юниты одним job |
| js | `_js.yml` | `node --test`, без npm |
| native | `_native.yml` | кросс-сборка, `SHA256SUMS`, release-shaped `tree-<hash>` |
| quality | `_quality.yml` | audit, lint:comments, static, ci:units, test:ci, shellcheck, actionlint, Gitleaks — параллельно, всегда; падает со списком всех упавших |

Затронутость:

- android/bazel — обратные зависимости Bazel (`bazel query`, loading phase:
  без JDK/SDK); изменённый файл — `rule-input`, новый/удалённый файл или
  `BUILD`/`.bzl` — весь пакет.
- `Cargo.lock`/`crate-universe.lock` сравниваются структурно, изменённые
  крейты → rdeps в Bazel. `packages/rust/**` выбирает только юниты, чьи цели
  затронуты.
- python — `paths` (+ `depends_on`), `hermes` — ещё rdeps его образов;
  native — `paths` + rdeps своих корней.
- `*.md`/`*.mdx` не выбирают юниты сборки, тестов и деплоя
  (`routableChanges` в `affected.mjs`), кроме путей, которые юнит или
  инвалидация перечисляет паттерном на `.md`/`.mdx` (Hermes `HERMES.md`,
  `SOUL.md`, `skills/**/*.md`; `docs/SECRETS.md` у `workstation`). Новый
  md-контент, который попадает в образ или читается тестом, добавлять в
  `paths` юнита явно. `quality` и `ci-ok` идут всегда, `docs_only` — по
  полному списку изменений.
- База PR — `HEAD^1` merge-коммита, а не `pull_request.base.sha` (иначе в
  diff попадают чужие PR); push — `github.event.before`.

Fallback — только «больше», никогда «ничего»: `MODULE.bazel`, `.bazelrc`,
`.bazelversion`, `BUILD.bazel`, `platform/bazel/**` → все юниты. Узкие
исключения (`affected.mjs` сравнивает структурно, при сомнении — глобально):
Maven-расширение `MODULE.bazel` → только android; Rust/zig/crate_universe →
native + bazel; дифф `.bazelrc` только по cache/output-флагам → только
quality; `platform/bazel/zig|windows/**` → native;
`platform/android/android.bazelrc` → android. Ошибка `bazel query` → все
android/bazel/native; база не определена → все; пустой diff → только quality.

`android-distribute` (вне `ci-ok`) подписывает и выкладывает готовый
`android-app-apk`, только если изменился реальный вход приложения
(`projects/mobile/android/**`, `packages/kotlin/**`, `platform/android/**`,
Maven-часть `MODULE.bazel`).

`platform/ci/check_units.mjs` (`mise run ci:units`) падает, если: дубли id,
пустые или слишком широкие `paths`; android-юнит без корней/целей; битый
`depends_on`; python-suite без `test_*.py`; **tracked
`*.rs`/`*.py`/`*.kt`/`*.js`/`*.mjs`/`*.sh`/`*.bzl`/`BUILD.bazel` не
принадлежит ни одному юниту**; запись в GHCR без логина; caller выдаёт
меньше прав, чем reusable. Новый юнит — запись в `units.mjs`.

### Required check и релиз

`ci-ok` (`if: always()`) — единственный required check `develop`: зелёный,
только если все зависимые job'ы `success` или законно `skipped`
(`platform/ci/ci_gate.mjs`). `deploy-home` ничего не собирает: `pins`
продвигает проверенные `tree-<hash>` и коммитит digest'ы в `deploy/home-apps`,
`native-release` ставит релизные теги, `deploy-station` ставит версию после
approve (решения 36, 54, 55).

### Локальное воспроизведение

```bash
mise run units
mise run ci:affected
mise run ci:affected --base HEAD~3 --head HEAD
platform/ci/affected.sh --changed-file solutions/scheduler/crates/scheduler/src/store/tick.rs
platform/ci/affected.sh --all
mise run ci:units
mise run test:ci
node --test platform/ci/promote.test.mjs
node platform/ci/ghcr_cleanup.mjs --fixture platform/ci/testdata/ghcr-versions.json --now 2026-11-04T00:00:00Z
node platform/ci/fad_cleanup.mjs --fixture platform/ci/testdata/fad-releases.json --now 2026-11-04T12:00:00Z
mise run test:deploy-control
mise run audit
mise run test:static
mise run lint:shell
mise run lint:actions
mise run scan:secrets
```

`affected.sh` только читает Git и граф загрузки Bazel. Версии инструментов CI
— из `mise.toml`/`mise.lock` через `jdx/mise-action`; job'ы с `mise run`
выставляют `MISE_TASK_RUN_AUTO_INSTALL=false`.

## Публикация

```bash
node platform/quality/audit_repository.mjs
mise run test
```

`audit_repository.mjs` проверяет локальные Markdown-ссылки, приватные пути и
реестр `SECURITY.md` («Приватные данные»); эвристика не заменяет ручной аудит. Публичные
исключения — только точный домен `proflyder.dev` и namespace
`dev.proflyder.home` (`dev/proflyder/home`) как целые токены; границы
закреплены `audit_repository.test.mjs`. Общих исключений Gitleaks нет.

## Код без комментариев

В коде на любом языке нет комментариев: ни строчных, ни блочных, ни
doc-комментариев (`//`, `/* */`, `///`, `//!`, KDoc, `#`, docstring,
`<!-- -->`, `--`). Пояснения — только в Markdown: README модуля (раздел
«Заметки к коду»), `docs/`, `AGENTS.md`. Код, которому нужно пояснение,
переписывается: имя функции или константы вместо комментария.

Проверка — `platform/quality/no_comments.mjs`:

```bash
mise run lint:comments
node platform/quality/no_comments.mjs --staged
node platform/quality/no_comments.mjs --fix
node platform/quality/no_comments.mjs --stats
node --test platform/quality/no_comments.test.mjs
```

Работает в `.githooks/pre-commit` (staged), `.githooks/pre-push` (весь
репозиторий) и шаге `No comments in code` в `_quality.yml` → `ci-ok`.

Сканер — лексеры по языкам, а не regex: `#` и `//` внутри строк, URL,
raw-строк Rust, шаблонов Kotlin/JS, regex JS, heredoc и `$#`/`${x#y}` не
считаются. Скрипты в YAML-блоках `run:`/`script:` проверяются как код своего
языка; файлы без расширения — по shebang. Markdown и `.txt` не проверяются.

Исключения (`EXCLUDED`): `testdata/`, `fixtures/`, `tests/golden/`,
`third_party/`, `vendor/`, `generated/`, `resources/`, `artifacts/`,
вендорные TensorFlow microfrontend и kissfft, `gradlew`/`gradlew.bat`,
`*.lock`, `*.patch`.

Allowlist (`ALLOWED`) — только директивы, без которых не работает
инструмент: shebang `#!` в первой строке; `#MISE`/`#USAGE` в `mise-tasks/`;
`# shellcheck disable|source|shell=`; директивы парсера Dockerfile
(`# syntax=`, `# escape=`, `# check=`) в первой строке; `#cloud-config`;
`#Requires` в PowerShell; однострочный `// SAFETY: …` в Rust (его требует
`clippy::undocumented_unsafe_blocks`). Поэтому в workspace
`clippy::missing_safety_doc = "allow"`, а контракт unsafe-функции описывается
в README крейта. Новое исключение — только с обоснованием здесь и тестом в
`no_comments.test.mjs`.

Версии GitHub Actions, закреплённых по SHA (комментарий после SHA запрещён).
При обновлении менять SHA в workflow и строку здесь:

| Action | Версия | SHA |
| --- | --- | --- |
| actions/attest-build-provenance | v4.2.2 | 4d101475d8b20a2381f78447822ac1eab6504dd8 |
| actions/cache (restore, save) | v6.1.0 | 55cc8345863c7cc4c66a329aec7e433d2d1c52a9 |
| actions/checkout | v7.0.1 | 3d3c42e5aac5ba805825da76410c181273ba90b1 |
| actions/download-artifact | v8.0.2 | 9000827ccba6bdab643e8b6fd33ac0654aef8333 |
| actions/github-script | v9.0.0 | 3a2844b7e9c422d3c10d287c895573f7108da1b3 |
| actions/setup-java | v6.2.0 | 77dd5b2ce225ea11469928ac317850245a74ae78 |
| actions/upload-artifact | v7.0.2 | cf430e030ddbb5b0abf93d22962f4752f3646cd9 |
| android-actions/setup-android | v4.0.4 | be39fa834029ff78f1a44aa3bb0819b8fc2bd8fd |
| bazel-contrib/setup-bazel | v0.19.1 | 104e3eea9447cdaa4b00d0fb46191a3985d26ab3 |
| docker/login-action | v4.8.0 | 8aa5831fb8d48d8f76594c7ddde61ae58019f550 |
| docker/setup-buildx-action | v4.4.1 | f87e5991a6d7451dcb8d9637bfbc97413f497069 |
| hashicorp/vault-action | v4.0.0 | 892a26828f195e65540a40b4768ae4571f51ebfc |
| jdx/mise-action | v5.2.7 | b44d12e7d0d8ee7461bb893e1b1993500433185b |
| mlugg/setup-zig | v2.2.1 | d1434d08867e3ee9daa34448df10607b98908d29 |
