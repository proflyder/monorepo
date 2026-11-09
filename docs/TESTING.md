# TESTING

Тесты — рядом с кодом; `platform/quality/` — общие проверки и аудит.

## Быстрый старт

```bash
python3 -m pip install -r requirements-test.txt
mise run test          # hermetic, без сети/станции
mise run test <unit>   # тесты одного CI-юнита (список — `mise run units`)
mise run coverage      # coverage; reports/TEST-COVERAGE.md (not tracked)
mise run test:e2e      # live HA/BLE read-only; нужны токены
mise run check         # pre-push: test + repository audit + Gitleaks
```

`mise run test` — gate. `mise run test:e2e` требует `HA_TOKEN`, опционально
`STATION_CONTROL_TOKEN`/`HERMES_API_KEY`; токены не коммитить.
CI/repo-quality проверки — Node 20+ (`node --test`), без npm-зависимостей.

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
| HA static | YAML, duplicate keys, entities/services/scripts, Jinja, intents, alarm/music state |
| Hermes | model/provider, MCP allowlist, web search, voice sanitizer |
| Egress guard | IPv4/IPv6 allowlist, NTP, idempotency |
| Static gates | shell syntax, ShellCheck errors, Python syntax, secret patterns |
| Scheduler | SQLite store/tick/planner, Connect API, auth, tasks/queues |
| HA bridge | config/queue, Connect JSON, HA REST, dry-run, at-least-once, ledger |
| Audit scanner | approved public domain/namespace allowances and lookalikes |
| Pairing | WAV/OGG, token redaction |
| BLE gateway | token, Wi-Fi validation/rollback, WPA, GATT, HTTP auth/status |
| Windows logon provider | portable protocol/HMAC/config vectors, cross-build exports/imports, static install scripts, on-PC self-test |
| Heartbeat Worker | grace-логика, auth, KV transitions, Telegram-доставка (моки) |
| HA live | API, entities, text intents, optional mutation/alarm |
| Gateway live | bearer auth, status shape, method guards |
| MCP gateway | Rust/Bazel unit (фейки backend'ов), каталог↔RBAC, contract `deploy.sh`, fake SSH |
| bb plugins | `node --test` диспетчера и `profiles`; контракт `params.json` с обеих сторон |
| Voice routing | Rust-юнит `solutions/voice-proxy`, `test_voice_routing_contract.py`, мост `test_bb_dispatch.py` |
| Pulsar Rust | `bazel test //projects/station/pulsar/satellite/...` (fmt/clippy/tests, Linux) и `bazel build -c opt --platforms=//platform/bazel/zig:<arch>_linux //projects/station/pulsar/satellite:station_binary` для `aarch64`/`armhf` — CI-юниты `pulsar-bazel` (`_unit.yml`) и `pulsar` (`_native.yml`) |
| Pulsar installer | Rust-тесты установщика и golden-фикстуры из [`projects/station/pulsar/installer/`](../projects/station/pulsar/installer); `bazel test //projects/station/pulsar/installer/...` |

## Hermes

```bash
export HERMES_API_URL=http://127.0.0.1:8642
export HERMES_API_KEY='...'
python3 projects/home-assistant/e2e/live_hermes.py -v

HERMES_E2E_CHAT=1 python3 projects/home-assistant/e2e/live_hermes.py -v

# Реальный model turn через conversation.hermes_agent в Home Assistant:
STATION_E2E=1 HA_HERMES_AGENT_TEST=1 python3 projects/home-assistant/e2e/live_ha.py -v
```

HA read-only gate проверяет доступность `conversation.hermes_agent`; прямой
Hermes `/health` и `/v1/models` не заменяют регистрацию агента в HA.

## Scheduler и HA bridge

`mise run test` запускает герметичные suites, `mise run test <unit>` — проверки
одного юнита. Отдельные команды:

```bash
bazel test //solutions/... //packages/...
bazel test //platform/deploy-rs/...
cargo test -p scheduler -p ha-bridge
```

`platform/deploy-rs` содержит frozen parity-фикстуры: fake kubectl
записывает argv и payload, а `bazel test //platform/deploy-rs:parity`
сравнивает их с трассами и файлами, снятыми с Go-реализации до её удаления.
`state` покрывает record (live/absent/fail-closed), mark, rollback и
first-install delete; `preflight` — legacy-writer проверки и HTTP-сценарии на
локальных фейковых HTTP-серверах.

Тесты scheduler/bridge герметичны: SQLite — во временном каталоге, scheduler,
HA и Connect-клиенты — через локальные фейковые HTTP-серверы; живые HA, ADB и
устройство не используются. Scheduler API отдаёт Connect JSON на
`/home.v1.SchedulerService/<Method>`; bridge проверяется как настоящий
клиент этого контракта.

Heartbeat Worker тестируется встроенным Node-раннером без npm:

```bash
node --test solutions/heartbeat/src/*.test.js
```

Локальный процессный E2E (настоящие бинарники scheduler + bridge, fake HA на
loopback, временные SQLite/ledger, dummy-токены):

```bash
python3 projects/home/e2e/scheduler_smoke.py -v
# или
projects/home/e2e/run-scheduler-smoke.sh
```

Он проверяет: auth reject без/с неверным токеном, timer → FIRE → fake HA →
DELIVERED, generic task остаётся в очереди `tasks`, STOP через StopSchedule.
Живой звук и HA shadow-import этот тест **не** проверяет.

Голосовые адаптеры:

```bash
python3 -m unittest discover -s projects/home-assistant/ha-voice -p 'test_*.py' -v
```

HA voice tests проверяют контракт Connect JSON, truthfulness речи, отсутствие
legacy snooze/start и сохранение `enabled:false` при update; живые
HA/scheduler не используются.

Живая проверка голосового маршрута — тихий `conversation.process` через
builtin-агент (`conversation.home_assistant`), не через Hermes: create/list/
update/enable/disable/delete, timer start/pause/resume/cancel и reminder
create/list. Результаты и scheduler state сверяются через
`ListSchedules`/`ListTimers`; звук и микрофон этим не проверяются.

### bb, шлюз и голосовой профиль

```bash
# плагины bb (без bb SDK и сети)
node --test solutions/bb-home-dispatcher/tests/*.test.js
(cd solutions/bb-profiles && npm test)

# шлюз: Rust-тесты, каталог↔RBAC, контракт полей и деплой
bazel test //solutions/mcp-gateway/...
python3 -m unittest projects/home/kubernetes/mcp-gateway/test_deploy.py -v

# мост и authorized_keys на локальном HTTP-double плагина
python3 -m unittest projects/home/proxmox/test_bb_dispatch.py -v

# голос: маршрут по профилю, fail-closed, cancel guard, announce, очередь
cargo test -p voice-proxy
bazel test //solutions/voice-proxy/crates/voice-proxy:unit_tests
python3 -m unittest projects/home-assistant/deploy/test_voice_routing_contract.py -v

# деплой-инструменты и Hermes rollout/render-config
bazel test //platform/deploy-rs/...
cargo test -p health-exporter
bazel test //projects/home-assistant/deploy/health-exporter/...
python3 -m unittest discover -s projects/home-assistant/deploy -p 'test_*.py' -v
python3 -m unittest discover -s projects/home-assistant/hermes -p 'test_*.py' -v
```

Тесты герметичны: SSH и плагин подменяются фейками, живой CT/кластер не
нужны. Живая приёмка рабочего профиля через колонку — `PENDING.md`.

Bazel-гейты отдельные:

```bash
# backend-таргеты (Bazelisk — `mise install`; `bazel` — shim на него)
bazel test //solutions/... //packages/...

# Rust-хост Pulsar (Linux; на macOS цели пропускаются через
# target_compatible_with, поэтому build проходит без сборки)
bazel build //projects/station/pulsar/...
bazel test  //projects/station/pulsar/satellite/...
```

`bazel test` для Pulsar включает юнит- и интеграционные тесты, golden-файлы,
`rustfmt` и `clippy` (warnings = errors). `clippy` линтует ровно
`[workspace.lints]` корневого `/Cargo.toml`: каждый crate-пакет передаёт
`extract_cargo_lints(... workspace = "//:Cargo.toml")` в `lint_config`, поэтому
`cargo clippy --workspace` и Bazel дают один набор предупреждений. На macOS те же
цели несовместимы и пропускаются; реальная сборка/тесты — только на Linux.
Станционная кросс-сборка и разбор crash-файла — там же:

```bash
bazel build -c opt --platforms=//platform/bazel/zig:armhf_linux \
  --define=pulsar_version="$(tr -d '[:space:]' < projects/station/pulsar/VERSION)" \
  //projects/station/pulsar/satellite:station_binary
# pulsar (stripped, .gnu_debuglink) и pulsar.debug (unstripped) лежат в bazel-bin/.../satellite/
```
Android-сборка/тесты требуют Android SDK и выполняются из
`projects/mobile/android` отдельно; они не входят в backend `mise run test`.

```bash
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
bazel test --config=android \
  //projects/mobile/android:android_tests \
  //projects/mobile/android/app:main_activity_smoke_test \
  //projects/mobile/android:app \
  //solutions/scheduler/... //solutions/ha-bridge/... //packages/proto/... \
  --nocache_test_results --test_output=errors
bazel build --config=android //projects/mobile/android:app
# debug APK: bazel-bin/projects/mobile/android/app/app.apk

# Android Lint (separate CI unit `android / android-lint`, not in android_tests)
# needs cmdline-tools 23.0.
bazel test --config=android //projects/mobile/android:android_lint

# detekt (separate CI unit `android / android-detekt`, not in android_tests):
# built-in rules, formatting and the first-party `home-rules` plug-in.
bazel test --config=android //projects/mobile/android:detekt

# Bazel-инструменты Android (Kotlin/JVM, exec-конфигурация): генераторы
# ViewBinding/google-services, lint runner и APK closure; их тесты входят
# в android_tests.
bazel test --config=android //platform/android/...
```

Android-гейты и их актуальный состав перечислены в
`projects/mobile/android/README.md`; на устройстве установлен debug APK,
интерактивная приёмка (фильтры, сценарии, toggle) ещё не завершена. Push/FCM,
offline cache и generic task UI не реализованы. APK debug-signed, appId
`dev.proflyder.home`, minSdk 26, target/compile 36, только `INTERNET`.

## BLE gateway

```bash
export STATION_CONTROL_TOKEN="$(adb -s 192.0.2.13:5555 shell cat /data/quasar/ble-gateway/control-token | tr -d '\r\n')"
export STATION_GATEWAY_URL=http://192.0.2.13:8099
mise run test:e2e
```

Read-only: 401 без токена, `/status`, 405 на неверные методы.

## Windows logon provider

`solutions/windows-logon-provider` — Rust-креденшел-провайдер, кросс-сборка
Bazel/rules_rust + герметичный zig под `x86_64-pc-windows-gnu`. Проверки:

```bash
cd solutions/windows-logon-provider
./run-tests.sh      # bazel test: protocol/HMAC/config-векторы на host + rustfmt/clippy (host и Windows)
./build.sh          # кросс-сборка; pe_artifacts.py проверяет exports (DllGetClassObject/DllCanUnloadNow) и imports
python3 -m unittest discover -s . -p 'test_*.py' -v   # статический контракт install/set-secret/uninstall/status
```

Bazel и Rust/zig-тулчейн приходят из `MODULE.bazel`; на macOS/Linux Bazelisk
берётся из `PATH` (`mise install`, см. `mise.toml`), либо задаётся `BAZELISK`. Отдельного mingw-архива и
host clang++ нет: `build.sh` собирает
`//solutions/windows-logon-provider/crates/provider:provider` под
`//platform/bazel/windows:x86_64_windows`, обнуляет PE-таймстемп для
воспроизводимости и проверяет импорты/экспорты через `pe_artifacts.py`.
CI (платформа `native`, `_native.yml`) ставит pinned Bazelisk, кеширует Bazel
и запускает оба скрипта, затем публикует DLL, `cp-selftest.exe` и
`SHA256SUMS` артефактом; ПК из CI недоступен. PR из этого репозитория
публикует тот же артефакт через ORAS под `tree-<hash>` с provenance
(build once); merge только продвигает проверенный digest на теги `VERSION` и
`sha-<12>`; при missing/unverified job падает и ничего не собирает
(`publish-oci.sh` в деплое не вызывается). Тег версии неизменяем по
содержимому. Форки не публикуют;
проверка получения и проверки digest — `fetch.sh`.

На самом ПК — self-test (когда грант не ожидается) и живая приёмка:

```powershell
.\cp-selftest.exe     # загрузка провайдера вне LogonUI
```

Живой чеклист: голос «включи комп» → экран входа → рабочий стол без PIN;
включение кнопкой → обычный вход; wakegrant недоступен → обычный вход, без
плитки и зависаний.

## Не автоматизировано

- реальный BlueZ/ALSA/event2/Wi-Fi rollback на станции;
- установка прошивки Pulsar на реальную станцию вне `deploy-station`;
- микрофон/ASR, Spotify 429, реальный звук;
- `resources/spotify-autostart/spotify.sh` — документированный legacy,
  исключён из syntax gate.

Эти пути — live/ручная приёмка. Структурная проверка HA не равна runtime coverage.

## CI (GitHub Actions)

`.github/workflows/ci.yml` — hermetic-проверки без деплоя и без секретов.
Триггеры: pull request в `develop` и ручной `workflow_dispatch` (он прогоняет
все юниты). Push в `develop` CI не запускает: он меняется только через PR с
зелёным `ci-ok`, повторный прогон после squash-merge избыточен.
`permissions: contents: read`, `concurrency` отменяет устаревший прогон только
для PR. Выкладка — отдельный `.github/workflows/deploy-home.yml` на push в
`develop` (см. ниже); его job `warm-caches` после merge — единственный
писатель develop-стороны общих кэшей: `setup-bazel` и одного дискового
кэша Bazel. Одна команда `bazel build --config=android` в одном сервере
собирает Android-цели (`//projects/mobile/android:app`,
`//projects/mobile/android:android_tests`) и все build/test-цели юнитов
платформы `bazel` из `platform/ci/units.mjs`; `--config=android` задаёт
только Android-флаги и version defines и не меняет host-платформу Rust.
После `bazel shutdown` каталог `~/.cache/bazel-disk` сохраняется под одним
ключом `bazel-<os>-warm-<hash>`. PR-джобы находят его через `restore-keys`
(`bazel-<os>-warm-`), сохраняя свои уровни «точный ключ → юнит», поэтому
смена `maven_install.json` или lock-файлов не даёт полный холод.
Версии инструментов CI (Python, Node, gitleaks, actionlint, shellcheck,
kubectl, helm, oras) — из `mise.toml`/`mise.lock` через `jdx/mise-action`,
как и на рабочем месте (`docs/WORKSTATION.md`).

PR-прогоны кэши только читают (`actions/cache/restore`): GitHub привязывает
кэш к ref'у, который его записал, поэтому кэш, сохранённый PR'ом, не виден
другим PR'ам — он лишь тратит общий лимит 10 ГБ и минуты на загрузку. Save
в reusable workflow остаётся только для `develop` (ручной
`workflow_dispatch`) и для `warm-caches`. У юнитов без develop-писателя
(`_native.yml`, `_deploy-control.yml`) кэш диска только восстанавливается:
пока такой сборки на develop нет, лучше не плодить копии на каждый PR.
Один общий архив ограничен `--experimental_disk_cache_gc_max_size=7G`
в `warm-caches`: Android и host-Rust лежат в одном каталоге и не дублируют
байты в лимите GitHub 10 ГБ (два `actions/cache/save` одного каталога под
разными ключами дали бы две копии). Host-Rust юниты (`_unit.yml`: сервисы,
`proto`, `pulsar-bazel`) и Android-юниты (`_android.yml`) восстанавливают
этот кэш через общий префикс, приоритет остаётся у их собственных ключей.


Проверки, тесты и сборки внутри reusable workflow запускаются теми же
mise-задачами, что и локально: `mise run test <unit>` / `mise run build
<unit>` (диспетчер по `platform/ci/units.mjs`) и задачи `_quality.yml` —
`audit`, `test:static`, `ci:units`, `test:ci`, `lint:shell`, `lint:actions`,
`scan:secrets`. Список задач — `mise tasks`, их код — `platform/mise-tasks/`.
Каждый job с `mise run` выставляет `MISE_TASK_RUN_AUTO_INSTALL=false`:
инструменты ставит только `jdx/mise-action` (`install_args`), а не сама
задача. Механика Actions (matrix, кэши, artifacts, OIDC, публикация,
деплой) остаётся в workflow.

### Маршрутизация по юнитам

CI не гоняет всё на каждый push. Единица запуска — **юнит**: один сервис,
одно приложение, общая библиотека или каталог тестов. Юнит = одна запись в
matrix одного платформенного workflow.

| Платформа | Reusable workflow | Что делает юнит |
|---|---|---|
| android | `_android.yml` | JDK 21 + SDK 36, `bazel build` затем `bazel test`, APK в artifact |
| unit | `_unit.yml` | Linux-runner: `kind=bazel` — `bazel build`/`test` (Rust-сервисы, `home-deploy`, хост Pulsar), `kind=python` — Python-suite Hermes; затем образы юнита (`bazel`/`docker`/`mixed`) в том же Bazel-сервере, push `tree-<hash>` и attestation в том же job |
| python | `_python.yml` | один job на все затронутые Python-юниты: один checkout/mise/pip, затем `mise run test <id>` последовательно с `::group::`, в конце список упавших |
| js | `_js.yml` | pinned Node + `node --test` по `*.test.js`, без npm |
| native | `_native.yml` | pinned toolchain download + sha256, host tests, cross-build, `SHA256SUMS` artifact; same-repo PR публикует release-shaped `tree-<hash>` с provenance |
| quality | `_quality.yml` | audit, static gates, unit-map, actionlint, shellcheck, Gitleaks |

`ci.yml` содержит только координацию: `changes` → matrix-вызовы reusable
workflow → `ci-ok`. Исключение — `android-distribute`.
`android-distribute` ждёт весь `android` matrix и при
PR (не из форка) с `android_distribute` = true скачивает готовый
`android-app-apk`, подписывает его релизным ключом и загружает в Firebase: он
ничего не собирает. Этот output true, когда изменился реальный вход
приложения
(`projects/mobile/android/**`, `packages/kotlin/**` или `platform/android/**`)
либо `MODULE.bazel` изменился только внутри Maven-расширения
rules_jvm_external (виртуальный путь `MODULE.bazel#maven`): тогда меняется
Maven/APK-замыкание. Прочие глобальные инвалидации (остальной `MODULE.bazel`,
`.bazelrc`, toolchain-файлы) прогоняют проверки `android`, но APK не
публикуют; `.bazelrc` с диффом только cache/output-флагов
(`.bazelrc#cache`) не выбирает `android` вовсе. Job намеренно не входит в
`ci-ok`, поэтому его падение не блокирует merge.

Распространяемый APK собирается ровно один раз — в matrix-записи
`android / android-app` (`_android.yml`) без секретов и Environment:
`google-services.json` из переменной репозитория `GOOGLE_SERVICES_JSON_B64` →
`bazel build` `:app` с release-`versionCode`/`versionName` (`BAZEL_EXTRA_ARGS`)
→ `bazel test` с теми же `--define`, поэтому `apk_runtime_closure_test` и
`apk_abi_test` проверяют ровно публикуемый APK → неподписанный artifact
`android-app-apk` (`app.apk` + `release-info.json`). Ни одна запись Android
matrix секретов не читает.
`android-distribute` (Environment `Android`, роль OpenBao `android-distribute`)
скачивает artifact, подписывает его `platform/android/sign-release.sh`,
обновляет `release-info.json` (`release_info.py resign`) и вызывает
`distribute.sh --upload-only`.


Android-платформа состоит из трёх юнитов с одними и теми же `paths` и
Bazel-корнем: `android-app` (сборка APK и unit/UI-тесты), `android-lint`
(только `//projects/mobile/android:android_lint`) и `android-detekt`
(только `//projects/mobile/android:detekt`). Любое изменение Android
модуля, ресурса или `platform/android/**` выбирает все три, поэтому они видны
как отдельные matrix-job'ы `android / android-app`, `android / android-lint` и
`android / android-detekt`. Находка lint или detekt валит только свой job и не
обрывает `bazel test` приложения; `android-distribute` требует зелёный весь
`android` matrix, поэтому такая находка также пропускает публикацию APK.
`_android.yml` сначала выполняет `bazel build`, затем `bazel test`; шаги
`android-lint` и `android-detekt` названы «Android Lint» и «detekt», остальные
— `bazel test <unit>`. Дополнительные SDK-пакеты юнит объявляет полем
`bazel.sdk_packages` (у lint — `cmdline-tools;23.0`), и workflow устанавливает
их только для этого job. Юнит с `bazel.release: true`
(`android-app`) владеет распространяемым APK: когда `android_distribute`
выбран, `matrixEntries` добавляет его в матрицу, даже если изменение
затронуло только общий Kotlin-юнит, — иначе `android-distribute` не нашёл бы
artifact.

`changes` выдаёт outputs `android_units`, `unit_units`, `python_units`,
`js_units`, `native_units`, `deploy_control`, `android_distribute`,
`base_sha`, `docs_only`; каждый output — JSON-массив записей matrix
(`android_distribute` — `true`/`false`). `unit_units` объединяет Bazel-юниты
и те Python-юниты, которые владеют образами (сегодня Hermes), поэтому у
такого юнита один build/test job, а не два; attestation — шаги этого же
job'а, без отдельных `attest / <id>`-задач. `python_units` — список всех
затронутых Python-юнитов без образов, и `_python.yml` прогоняет их в одном
job последовательно.
Типичный сервисный PR теперь запускает `changes`, `quality`,
один `unit / <id>` (проверки + образы + attestation) и `ci-ok` (плюс
`deploy-control`, если затронут scheduler/ha-bridge); раньше были отдельные
`bazel / <id>`, `images / <id>` и по job'у на каждый digest. На
CI-global PR вместо ~22 `python / <id>` остаётся один `python` job.
Пустой массив означает «пропустить job» (законный skip для `ci-ok`).
Для `pull_request` база маршрутизации — первый родитель проверенного
merge-коммита (`git rev-parse HEAD^1`), а не `pull_request.base.sha` из
события: последний может быть старше tip'а `develop`, на котором собран
`refs/pull/N/merge`, и тогда в diff `base..merge` попадают файлы чужих PR.
`push` использует `github.event.before`.

Образы деплой-юнитов собираются в том же job `unit / <id>` (`_unit.yml`):
сначала идут проверки юнита, затем `<image>_image`/Docker-образы в том же
Bazel-сервере, поэтому второй сборки тех же целей нет. На PR из этого
репозитория они сразу публикуются как `tree-<git tree hash>` с build
provenance (решение 36); форки и `workflow_dispatch` только собирают — у них
нет прав на registry. Attestation создаётся шагами того же job `unit`
(по одному статическому шагу на индекс образа, максимум 4 — это проверяет
`check_units.mjs`); отдельных `attest`-job'ов больше нет, и у юнита без
образов никаких attest-строк не появляется. Docker и
mixed (Hermes Buildx) — ветки того же шага `platform/ci/build-images.sh`.

Native-юниты с `native.publish_script` (`pulsar`, `logon-provider`) следуют
тому же правилу: `_native.yml` после сборки публикует release-shaped артефакт
под `tree-<hash>` и аттестует digest. Pulsar собирается так на каждом PR
(версия из `VERSION`, radio, `SHA256SUMS`, проверка `--version` под qemu), а
не только при изменении `VERSION`. Права `packages`/`id-token`/`attestations`
выданы только job `native`.

### Карта юнитов

`platform/ci/units.mjs` (ES-модуль, без npm) — единственный источник правды: `id`, `platform`,
`paths`, способ запуска (`bazel.test_targets`, `python.suites`, `native.*`)
и Bazel-корни для сопоставления графа с юнитами. Рядом:

* `invalidations` — пути, которые валят больше юнитов, чем совпало по `paths`
  (`MODULE.bazel`, `MODULE.bazel.lock`, `.bazelrc`, `.bazelversion`,
  `BUILD.bazel`, toolchain-файлы → все платформы; `platform/android/**` →
  все android-юниты; `rust-toolchain.toml` → native + bazel;
  `requirements-test.txt` → python+quality). Флаг `deploy: false` оставляет
  правило только для CI: правка workflow или `platform/ci/**` перезапускает
  проверки, но `deploy-home` не выкатывает юнит, выбранный лишь таким
  правилом.
* `MODULE.bazel` — глобальная инвалидация с тремя исключениями
  (fail safe — «больше», не «меньше»). `affected.mjs` сравнивает base/head
  структурно и подменяет путь виртуальным, только если после снятия нужной
  категории остальные токены совпадают:
  * Maven-расширение rules_jvm_external (`maven.install(...)`, любые
    `maven.<tag>(...)`, `use_repo(maven, ...)`, где `maven` — переменная из
    `use_extension("@rules_jvm_external//:extensions.bzl", "maven")`) →
    `MODULE.bazel#maven`, только `android` (меняется Maven/APK-замыкание);
  * Rust/zig/native-входы станции: `rust.toolchain(...)`/`use_repo(rust, ...)`,
    `crate.from_cargo(...)`/`use_repo(crate, ...)` (crate_universe),
    `zig_sdk_repository(...)`, `register_toolchains(...)` только с аргументами
    `@zig_0_14_1//...`, `single_version_override(module_name = "rules_rust", ...)`
    и `http_archive(...)` сторонних C/C++ станции (build_file с
    `_SENDSPIN_BAZEL`) → `MODULE.bazel#native`, платформы `native` и `bazel`
    (хост-проверки Pulsar компилируют те же крейты, что и кросс-сборка);
  * Maven и native вместе → оба виртуальных пути (объединение платформ).
  Всё остальное (`bazel_dep` любого модуля, `oci`,
  python, другая extension, неразбираемый файл, отсутствующая base или
  добавление/удаление `MODULE.bazel`) оставляет глобальную инвалидацию.
  `MODULE.bazel.lock` следует scope только вместе со scoped-изменением
  `MODULE.bazel`; отдельно от него — глобальный.
* `.bazelrc` — глобальная инвалидация, кроме диффа, который меняет только
  cache/output-флаги и комментарии. Белый список — по имени опции после
  префикса команды (`common`/`build`/`test`/...): `--disk_cache`,
  `--experimental_disk_cache_gc_*`, `--repository_cache`,
  `--repo_contents_cache*`, `--announce_rc`, `--color`, `--curses`,
  `--show_progress*`, `--test_output`, `--test_summary`,
  `--verbose_failures`. `--remote_cache` в белый список не входит. Такой
  дифф становится `.bazelrc#cache`, не совпадает ни с одним юнитом и гоняет
  только always-on `quality` (deploy ничего не катит). Строка, смешивающая
  cache- и настоящий флаг, значима целиком; `import`/`try-import`,
  неизвестный флаг, неразбираемая строка, отсутствующая base или
  добавление/удаление файла — глобально.
* `platform/android/android.bazelrc` — **не** глобальный вход, хотя и
  называется `*.bazelrc`: он попадает под `platform/android/**` и выбирает
  только android-юниты. Здесь лежат Android-специфичные флаги (version
  defines, Java-toolchain build-инструментов, protoc/dex workers,
  `--android_platforms`), а корневой `.bazelrc` подключает файл через
  `try-import` и не содержит их. Активация — `--config=android`: его
  передают диспетчер юнитов (`mise run build|test` для android-юнитов и
  `mise run build`/`mise run test:bazel` на весь workspace),
  `projects/mobile/android/distribute.sh` и Android-сборка в
  `deploy-home` → `warm-caches`. Без конфига Android-сборка падает на
  анализе (`$(home_version_name) not defined`), а не собирает тихо APK
  чужой ABI; после #234 это дополнительно ловит `apk_abi_test`.
* `platform/bazel/zig/**` и `platform/bazel/windows/**` — только платформа
  `native` (виртуальный `platform/bazel#native`): это target-constrained
  zig cc-toolchain'ы и rules_rust-патчи для кросс-сборки станции и Windows
  provider; хост-сборки Rust/образов/Android их не выбирают. Остальной
  `platform/bazel/**` остаётся глобальным.
* Любое другое изменение (`bazel_dep`, toolchain, `oci`, python),
  неразбираемый файл, отсутствующая base-ревизия или
  добавление/удаление `MODULE.bazel` оставляют глобальную инвалидацию:
  fail safe — «больше», не «меньше».
* `deploy_control` — какие юниты и пути включают дорогую регрессию
  `mise run test:deploy-control` (scheduler, ha-bridge/ha-control,
  `projects/home/kubernetes/scheduler/**`).
* `deploy` (у деплоимых юнитов) — `images`, `workloads` и `builder`
  (`bazel`/`docker`/`mixed`). По нему `deploy-home` и job `unit` в `ci.yml`
  выбирают, что именно собирать и раскатывать; инфраструктурные юниты этой
  секции не имеют. `mixed` — один юнит с обоими типами образов: сегодня
  `hermes` (Docker) и `voice-proxy` (Bazel `//solutions/voice-proxy`).

### Как считается затронутость

| Платформа | Источник зависимостей | Пример |
|---|---|---|
| android | обратные зависимости Bazel (`bazel query --output=xml`, loading phase) | правка `packages/kotlin/core` выбирает `kotlin-shared` и только те приложения, что от него зависят |
| bazel | те же обратные зависимости Bazel, что у android; общие Rust-крейты и `Cargo.lock` выбираются по rdeps | правка `packages/rust/home-ha` выбирает `ha-bridge` и `voice-proxy`, но не `scheduler`/`pulsar-bazel` |
| python | пути юнита (+ `depends_on`), а для `hermes` ещё и rdeps его Bazel-образов | правка `home-tls` выбирает `hermes` через `health-exporter` |
| native | пути юнита; `pulsar` и `logon-provider` дополнительно по rdeps своих Bazel-корней | правка `wakegrant-protocol` выбирает `logon-provider`, правка `home-runtime` — `pulsar` |
| quality | всегда | audit/Gitleaks/shellcheck идут на любом изменении |

Android-граф берётся из loading-фазы намеренно: он не требует JDK и Android
SDK, поэтому маршрутизация дешёвая и не может «сломаться из-за toolchain».

Изменённый файл сопоставляется с `rule-input` (source file); для нового,
удалённого файла или правки `BUILD.bazel`/`.bzl` берётся весь пакет — это
безопасное over-approximation.

### Общие Rust-входы: `packages/rust`, `Cargo.lock`, `crate-universe.lock`

`packages/rust/**` не валидирует все Rust-юниты. Изменённый файл попадает в
Bazel-граф как source input, и юнит выбирается, только если затронутая цель
лежит под его routing-корнем. `//packages/rust` как общий корень для
clippy/rustfmt исключён: иначе правка одного крейта выбирала бы каждый юнит,
который линтует общий слой. Юнит, который явно тестирует крейт
(`test_targets` вида `//packages/rust/<crate>:unit_tests`), получает
`//packages/rust/<crate>` в routing-корни.

`Cargo.lock` и `crate-universe.lock` сравниваются между base и head как TOML
(`[[package]]`: name/version/source/checksum/dependencies) и JSON
(`crates`/`direct_deps`/`workspace_members`). Изменёнными считаются
добавленные, удалённые пакеты и пакеты с другой версией, checksum или списком
зависимостей. Для них строится обратное замыкание по графу зависимостей:
* прямой крейт → `@crates//:<name>` (alias crate_universe);
* транзитивный крейт без alias → ближайший прямой предок, от которого Bazel
  rdeps уже доходят до реальных потребителей;
* изменённый workspace-member → его путь, чтобы выбрать сам крейт.
Затем берутся rdeps в графе Bazel. `Cargo.toml` сужается только если дифф
затрагивает исключительно `[workspace.dependencies]` и есть разбираемый
lock-diff; `[workspace] members`, `[profile]`, `[workspace.lints]`, правка
`[workspace.dependencies]` без изменения lock (могли поменяться
`features`/`default-features`), неразбираемый/добавленный/удалённый lock,
изменённый пакет без alias/workspace-пути или ошибка Bazel — «все Rust-юниты»
(native + bazel, а для deploy ещё и graph-aware python). `rust-toolchain.toml`
всегда «все Rust-юниты».

### Fallback: только «больше», никогда «ничего»

* `MODULE.bazel`/`MODULE.bazel.lock`, `.bazelrc`, `.bazelversion`,
  `.bazelignore`, `BUILD.bazel`, `platform/bazel/**` → все платформы и все
  юниты; исключения — scoped-диффы, описанные выше: `MODULE.bazel#maven` →
  только android, `MODULE.bazel#native` → native + pulsar-bazel,
  `.bazelrc#cache` → ни одной платформы, `platform/bazel#native` → native;
* `bazel query` упал → все android, bazel и native юниты (и graph-aware
  python-юнит `hermes`); ошибка разбора `Cargo.lock`/`crate-universe.lock`
  откатывает к тому же «все Rust-юниты»;
* база не определена (новый push, `workflow_dispatch`) → все юниты
  (`affected.mjs --all`);
* база определена, но `git diff` пуст → ни один тяжёлый юнит, только quality.

Внутри android-юнита идут все объявленные цели: незатронутые изменением
тесты Bazel берёт из кэша (действий и результатов тестов), поэтому отдельного
сужения через target-determinator нет.


### Инварианты и проверки

`platform/ci/check_units.mjs` падает, если:

* в карте дубли id, неизвестная платформа, пустые `paths` или слишком широкий
  шаблон (`**`);
* android-юнит не объявил `bazel.roots`, `test_targets` или цель вне корней;
* `depends_on` ссылается на несуществующий юнит;
* объявленный python-suite не содержит `test_*.py` (для каталогов без
  герметичных тестов есть `python.compile`: только `compileall`);
* **любой tracked `*.rs`/`*.py`/`*.kt`/`*.js`/`*.mjs`/`*.sh`/`*.bzl`/`BUILD.bazel` не
  принадлежит ни одному юниту** — новый тест не может молча выпасть из CI;
* job workflow пишет в registry (`crane tag`/`crane push`, `oras push`/`oras
  tag`, `platform/ci/promote.mjs`, `push-to-registry: true`) без логина в
  `ghcr.io` до первой записи.

Тесты маршрутизатора: `platform/ci/affected.test.mjs` (включая фикстуру
`testdata/units-two-apps.mjs` с двумя Android-приложениями и общим
Kotlin-юнитом).

### Расширение

Новый язык по возможности идёт через Bazel (`platform: bazel`,
`bazel.roots`), как Rust у Pulsar. Действительно новая платформа: reusable
workflow `_<platform>.yml` с inputs юнита, `platform` в `units.mjs`, расчёт
затронутых юнитов в `affected.mjs` и job с matrix в `ci.yml`. Новый юнит — это новая запись в `units.mjs` и путь в
`paths`; отдельный job не нужен.

### Локальное воспроизведение

```bash
# каталог юнитов и их команды
mise run units                    # id, platform, title, deployable
mise run build <unit>             # bazel build целей юнита / go build бинарей (без юнита — //...)
mise run test <unit>              # команды юнита из units.mjs
mise run test:bazel               # bazel test //...

# какие юниты выбрал бы CI для диапазона (по умолчанию origin/develop...HEAD)
mise run ci:affected
mise run ci:affected --base HEAD~3 --head HEAD
platform/ci/affected.sh --base origin/develop --format json
platform/ci/affected.sh --base HEAD~3 --format text

# имитация конкретного изменения без коммита
platform/ci/affected.sh --changed-file solutions/scheduler/crates/scheduler/src/store/tick.rs
platform/ci/affected.sh --changed-file packages/kotlin/core/src/main/kotlin/.../TimeText.kt

# принудительно все юниты (то же, что workflow_dispatch)
platform/ci/affected.sh --all

mise run ci:units                               # инварианты карты юнитов
node --test platform/ci/*.test.mjs platform/quality/*.test.mjs        # маршрутизатор, gate и аудит
# продвижение по tree hash: теги, digest, история, PR-head дельта и отказ —
# без registry и сети (exec/compute инжектируются)
node --test platform/ci/promote.test.mjs
# политика чистки GHCR: pure-функция + локальный dry-run по фикстуре
node --test platform/ci/ghcr_cleanup.test.mjs
node platform/ci/ghcr_cleanup.mjs --fixture platform/ci/testdata/ghcr-versions.json \
  --now 2026-11-04T00:00:00Z
mise run test                                   # герметичный gate (включает CI-тесты)
mise run test:deploy-control                    # регрессия deploy control

# те же проверки, что в _quality.yml
mise run audit                                  # private data, ссылки, gitlinks
mise run test:static                            # Python static gate
mise run ci:units                               # инварианты карты юнитов
mise run test:ci                                # node --test platform/ci + platform/quality + mise dispatcher
mise run lint:shell                             # shellcheck -S error
mise run lint:actions                           # actionlint
mise run scan:secrets                           # Gitleaks (working tree)
```

`affected.sh` только читает Git и граф загрузки Bazel: он ничего не собирает,
не деплоит и не трогает устройства. Версии сторонних actions закреплены
commit SHA (сверены с тегами через `gh api`); Bazelisk `1.30.0`, actionlint
`1.7.12` и Gitleaks `8.30.1` скачиваются с
проверкой sha256.

На чистом checkout (CI) отсутствуют игнорируемые источники, поэтому
`mise run test` явно печатает `SKIP` для тестов, которым нужен nested
`projects/station/resources/quasar-connect-bot`; локально, где источник
есть, те же тесты выполняются. Pulsar теперь лежит в дереве
(`projects/station/pulsar/`), поэтому его установщик и Rust-тесты доступны
всегда; установщик — крейт `pulsar-installer` корневого Cargo workspace и
проверяется Bazel-целями `//projects/station/pulsar/installer/...`. `test_deploy_control.sh` больше не требует
собранных бинарников: он передаёт deploy.sh заранее разрешённые
digest-пины образов, поэтому `mise run test` и `mise run test:deploy-control`
запускают его без Bazel. Путь продвижения (`PREBUILT_DIGESTS`,
`PREBUILT_DIGESTS_FILE`, решение 36) покрыт там же и в `test_deploy.py`
юнитов `scheduler`/`currency-bot`/`mcp-gateway`/`wakegrant`:
образ не собирается и не пушится, digest попадает в отрендеренный манифест,
а кривое/дублирующее значение падает fail-closed до обращения к кластеру.
`projects/home-assistant/deploy/test_build_and_push.py` проверяет то же для
`build-and-push.sh` (`hermes`, `voice-proxy`, `health-exporter`,
`home-deploy`) с заглушками docker/bazel/git. Hermes rollout и
render-config покрыты Rust-тестами `platform/deploy-rs`: Hermes rollout —
frozen parity (`hermes`), `hermesconfig` — unit-тестами слияния, отказа на
TEST-NET и `.prev`; health exporter — Rust `.../deploy/health-exporter`
(`cargo test -p health-exporter` и Bazel-юнит `health-exporter`).

### Required check

`ci-ok` (`if: always()`) — единственный required status check для ruleset
ветки `develop`. Он зелёный только если все зависимые job'ы завершились
`success` или законно `skipped`; любой `failure`/`cancelled` (включая упавший
`changes`) делает его красным. Логика вынесена в `platform/ci/ci_gate.mjs`.
Matrix-entry `android / android-lint` (и `android / android-detekt`) не требует
отдельной записи в gate: они входят в job `android`, чей результат `ci-ok`
уже проверяет, поэтому падение lint или detekt делает `ci-ok` красным как и
любой другой android-юнит.

### Деплой (`deploy-home`)

Push в `develop` — это squash-merge PR, поэтому workflow получает секреты
Environment `Home`. `plan` прогоняет тот же `units.mjs` по диапазону
`before..sha` и деплоит только затронутые деплоимые юниты параллельно:
VPN группой `ci` → GitHub OIDC в OpenBao (`hashicorp/vault-action`,
роль `deploy-home`) → `secret/ci/deploy-home` → kubectl/helm к K3s `6443`
и Traefik `443`.
Документация и инфраструктурные юниты без `deploy` ничего не катят.

- Смена режима (`preserve`, `execute`, `shadow`, `control-*`) — только
  `workflow_dispatch`; push всегда conservative preserve.
- Hermes/voice-proxy (builder `mixed`): `hermes` — Docker, `voice-proxy` —
  Bazel/rules_oci; PR CI собирает и публикует их; deploy-home берёт
  промоутнутые digest'ы → релиз `runtime` → `-healthcheck`/health/Assist-
  проверка → `helm rollback` при провале. С `DEPLOY_NO_BUILD=1` отсутствие
  digest'а — ошибка, а не сборка.
- Build once (решение 36): job считает tree деплоимого коммита и продвигает
  digest'ы PR-сборки (`tree-<hash>` + проверенный provenance), включая
  `home-deploy`, из образа которого достаётся CLI. На `workflow_dispatch` и
  для образов-соседей (`deploy.optional_images`) берётся самый свежий
  верифицированный тег в first-parent истории; коммит-источник — в summary.
  Если `develop` ушёл вперёд после PR CI, на push берётся head PR, но только
  когда `affected.compute` по дельте `git diff H <merge>` не выбирает юнит
  (`promote.test.mjs`: дельта не затрагивает → promoted from PR head;
  затрагивает / нет PR / provenance не сходится → refused). Deploy-home
  ничего не собирает: если хоть один образ не прошёл проверку, job падает, а
  не откатывается на сборку. Выбранный путь виден в job summary
  (`image path: prebuilt|build`, где `build` = отказ promotion).
- Native (Pulsar, logon provider): job `native-release` вызывает `_native.yml`
  с `promote: true`. Тот проверяет `tree-<hash>` и provenance, ставит релизные
  теги на тот же digest; при missing/unverified job падает — деплой ничего не
  собирает и не публикует, история для native не применяется. PR-head
  fallback действует так же, как для образов: артефакт head PR продвигается,
  только если дельта до merge не затрагивает native-юнит, иначе job падает.
  PR CI (`promote: false`) по-прежнему собирает и публикует `tree-<hash>`.
  `deploy-station` ждёт `native-release` и только ставит опубликованную версию.
- Откат — только применённого объекта; изменение целиком откатывается новым
  PR с `git revert`.

Pulsar: хост-сборка и тесты — Bazel-юнит `pulsar-bazel`; станционные
бинарники (Bazel), radio, ssh, installer и релиз — native-юнит `pulsar`
(`_native.yml`). Каждый PR собирает release-shaped артефакт и публикует
`tree-<hash>`; merge только продвигает его на релизные теги, а при
отсутствии/непроверенном теге job падает.

## Публикация

```bash
node platform/quality/audit_repository.mjs
node --test platform/ci/*.test.mjs platform/quality/*.test.mjs
mise run test
```

Проверить локальные Markdown-ссылки, приватные пути, историю Git и
`SENSITIVE-DATA.md`. Эвристика не заменяет ручной аудит.

`audit_repository.mjs` содержит два узких публичных исключения, одобренных
пользователем: точный домен `proflyder.dev` и Android namespace
`dev.proflyder.home` с подпакетами/путём `dev/proflyder/home`. Исключения
удаляются только как целые токены с границами; другие TLD, username,
поддомены, длинные хосты/пути и смешанные строки по-прежнему дают ошибку.
Границы закреплены `platform/quality/audit_repository.test.mjs`. Общих
исключений Gitleaks нет.
