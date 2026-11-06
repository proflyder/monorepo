# TESTING

Тесты — рядом с кодом; `platform/quality/` — общие проверки и аудит.

## Быстрый старт

```bash
python3 -m pip install -r requirements-test.txt
make test          # hermetic, без сети/станции
make coverage      # coverage; reports/TEST-COVERAGE.md (not tracked)
make test-e2e      # live HA/BLE read-only; нужны токены
```

`make test` — gate. `make test-e2e` требует `HA_TOKEN`, опционально
`STATION_CONTROL_TOKEN`/`HERMES_API_KEY`; токены не коммитить.
CI/repo-quality проверки — Node 20+ (`node --test`), без npm-зависимостей.

```bash
export HA_URL=http://192.0.2.28:8123
export HA_TOKEN='...'
make test-e2e

STATION_E2E_MUTATE=1 make test-e2e
STATION_E2E_AUDIO=1 make test-e2e
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
| MCP gateway | Go unit (фейки backend'ов), каталог↔RBAC, contract `deploy.sh`, fake SSH |
| bb plugins | `node --test` диспетчера и `profiles`; контракт `params.json` с обеих сторон |
| Voice routing | Go-юнит `solutions/voice-proxy`, `test_voice_routing_contract.py`, мост `test_bb_dispatch.py` |
| Pulsar Rust | `bazel test //projects/station/pulsar/satellite/...` (fmt/clippy/tests, Linux) и `bazel build -c opt --platforms=//platform/bazel/zig:<arch>_linux //projects/station/pulsar/satellite:station_binary` для `aarch64`/`armhf` — CI-юниты `pulsar-bazel` (`_bazel.yml`) и `pulsar` (`_native.yml`) |
| Pulsar installer | Go-тесты установщика (nested module, `GOWORK=off`); исходники в `projects/station/pulsar/` |

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

`make test` включает Go-тесты корневого модуля (`solutions/...` и
`packages/...`). Отдельные команды:

```bash
go test ./solutions/...
go test ./packages/...
go test -race ./solutions/ha-bridge/...
go test -race ./platform/deploy/...   # converge, state, preflight, render, oci
go vet ./solutions/...
```

`platform/deploy/internal/converge` содержит parity-тест: fake kubectl
записывает argv и payload, а тест сравнивает разобранные JSON-объекты с
трассами, снятыми с Python `converge.py` до его удаления
(`platform/deploy/testdata/converge/`). `state` покрывает все случаи
`test_rollback.py`, `preflight` — legacy-writer проверки и HTTP-сценарии на
`httptest`.

Тесты scheduler/bridge герметичны: SQLite — во временном каталоге, scheduler,
HA и Connect-клиенты — через `httptest`/фейки; живые HA, ADB и
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

# шлюз: Go-тесты, каталог↔RBAC, контракт полей и деплой
go test ./solutions/mcp-gateway/...
python3 -m unittest projects/home/kubernetes/mcp-gateway/test_deploy.py -v

# мост и authorized_keys на локальном HTTP-double плагина
python3 -m unittest projects/home/proxmox/test_bb_dispatch.py -v

# голос: маршрут по профилю, fail-closed, cancel guard, announce, очередь
cargo test -p voice-proxy
bazel test //solutions/voice-proxy/crates/voice-proxy:unit_tests
python3 -m unittest projects/home-assistant/deploy/test_voice_routing_contract.py -v

# деплой-инструменты, Hermes rollout/render-config и health exporter
go test -race ./platform/deploy/... ./projects/home-assistant/deploy/health-exporter/...
python3 -m unittest discover -s projects/home-assistant/deploy -p 'test_*.py' -v
python3 -m unittest discover -s projects/home-assistant/hermes -p 'test_*.py' -v
```

Тесты герметичны: SSH и плагин подменяются фейками, живой CT/кластер не
нужны. Живая приёмка рабочего профиля через колонку — `PENDING.md`.

Bazel-гейты отдельные:

```bash
# backend-таргеты (нужен Bazelisk; локально — local-data/local-tools/bazelisk)
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
`projects/mobile/android` отдельно; они не входят в backend `make test`.

```bash
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
./local-data/local-tools/bazelisk test \
  //projects/mobile/android:android_tests \
  //projects/mobile/android/app:main_activity_smoke_test \
  //projects/mobile/android:app \
  //solutions/scheduler/... //solutions/ha-bridge/... //packages/proto/... \
  --nocache_test_results --test_output=errors
./local-data/local-tools/bazelisk build //projects/mobile/android:app
# debug APK: bazel-bin/projects/mobile/android/app/app.apk

# Android Lint (separate CI unit `android / android-lint`, not in android_tests)
# needs cmdline-tools 23.0.
./local-data/local-tools/bazelisk test //projects/mobile/android:android_lint

# Bazel-инструменты Android (Go, exec-конфигурация): генераторы
# ViewBinding/google-services, lint runner и APK closure; их тесты входят
# в android_tests.
./local-data/local-tools/bazelisk test //platform/android/...
go vet ./platform/android/...
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
make test-e2e
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
ищется в `PATH`, затем в `local-data/local-tools/bazelisk` (в linked worktree —
в главном checkout'е), либо задаётся `BAZELISK`. Отдельного mingw-архива и
host clang++ нет: `build.sh` собирает
`//solutions/windows-logon-provider/crates/provider:provider` под
`//platform/bazel/windows:x86_64_windows`, обнуляет PE-таймстемп для
воспроизводимости и проверяет импорты/экспорты через `pe_artifacts.py`.
CI (платформа `native`, `_native.yml`) ставит pinned Bazelisk, кеширует Bazel
и запускает оба скрипта, затем публикует DLL, `cp-selftest.exe` и
`SHA256SUMS` артефактом; ПК из CI недоступен. PR из этого репозитория
публикует тот же артефакт через ORAS под `tree-<hash>` с provenance
(build once); merge продвигает проверенный digest на теги `VERSION` и
`sha-<12>` либо, при missing/unverified, собирает и публикует заново
(`publish-oci.sh`). Тег версии неизменяем по содержимому. Форки не публикуют;
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
`develop` (см. ниже); его job `warm-caches` после merge сохраняет общие кэши
`setup-go`/`setup-bazel` под теми же ключами, которые восстанавливают PR.
Кэши `actions/cache` для Android и deploy-control остаются PR-локальными:
их содержимое создаёт только соответствующий build/regression.

### Маршрутизация по юнитам

CI не гоняет всё на каждый push. Единица запуска — **юнит**: один сервис,
одно приложение, общая библиотека или каталог тестов. Юнит = одна запись в
matrix одного платформенного workflow.

| Платформа | Reusable workflow | Что делает юнит |
|---|---|---|
| go | `_go.yml` | `go vet`, `go test` (`-race` где объявлено), `go build` бинарей |
| android | `_android.yml` | JDK 21 + SDK 36, `bazel build` затем `bazel test`, APK в artifact |
| bazel | `_bazel.yml` | Linux-runner, `bazel build` затем `bazel test` (Rust-хост Pulsar: тесты, fmt, clippy) |
| python | `_python.yml` | `compileall` + `unittest discover` по каталогам юнита |
| js | `_js.yml` | pinned Node + `node --test` по `*.test.js`, без npm |
| native | `_native.yml` | pinned toolchain download + sha256, host tests, cross-build, `SHA256SUMS` artifact; same-repo PR публикует release-shaped `tree-<hash>` с provenance |
| quality | `_quality.yml` | audit, static gates, unit-map, actionlint, shellcheck, Gitleaks |

`ci.yml` содержит только координацию: `changes` → matrix-вызовы reusable
workflow → `ci-ok`. Единственное исключение — job `android-distribute`: он ждёт `android` и при
PR (не из форка) с `android_distribute` = true собирает и загружает APK в
Firebase. Этот output true, когда изменился реальный вход приложения
(`projects/mobile/android/**`, `packages/kotlin/**` или `platform/android/**`)
либо `MODULE.bazel` изменился только внутри Maven-расширения
rules_jvm_external (виртуальный путь `MODULE.bazel#maven`): тогда меняется
Maven/APK-замыкание. Прочие глобальные инвалидации (остальной `MODULE.bazel`,
`.bazelrc`, toolchain-файлы) прогоняют проверки `android`, но APK не
публикуют; `.bazelrc` с диффом только cache/output-флагов
(`.bazelrc#cache`) не выбирает `android` вовсе. Job намеренно не входит в
`ci-ok`, поэтому его падение не блокирует merge.

Android-платформа состоит из двух юнитов с одними и теми же `paths` и
Bazel-корнем: `android-app` (сборка APK и unit/UI-тесты) и `android-lint`
(только `//projects/mobile/android:android_lint`). Любое изменение Android
модуля, ресурса или `platform/android/**` выбирает оба, поэтому они видны как
отдельные matrix-job'ы `android / android-app` и `android / android-lint`.
Находка lint валит только lint-job и не обрывает `bazel test` приложения;
`android-distribute` требует зелёный весь `android` matrix, поэтому находка
lint также пропускает публикацию APK. `_android.yml` сначала выполняет
`bazel build`, затем `bazel test`; дополнительные SDK-пакеты юнит объявляет
полем `bazel.sdk_packages` (у lint — `cmdline-tools;23.0`), и workflow
устанавливает их только для этого job.

`changes` выдаёт outputs `go_units`, `android_units`, `python_units`,
`js_units`, `native_units`, `deploy_control`, `android_distribute`, `base_sha`,
`docs_only`; каждый output — JSON-массив записей matrix (`android_distribute` —
`true`/`false`).
Пустой массив означает «пропустить job» (законный skip для `ci-ok`).

Образы (`images` → `_images.yml`) собираются на PR и, если PR не из форка,
сразу публикуются как `tree-<git tree hash>` с build provenance (решение 36).
Форки и `workflow_dispatch` только собирают: у них нет прав на registry.
Отдельный job `attest` матрицей по опубликованным digest'ам создаёт
attestation; `ci-ok` не меняется.

Native-юниты с `native.publish_script` (`pulsar`, `logon-provider`) следуют
тому же правилу: `_native.yml` после сборки публикует release-shaped артефакт
под `tree-<hash>` и аттестует digest. Pulsar собирается так на каждом PR
(версия из `VERSION`, radio, `SHA256SUMS`, проверка `--version` под qemu), а
не только при изменении `VERSION`. Права `packages`/`id-token`/`attestations`
выданы только job `native`.

### Карта юнитов

`platform/ci/units.mjs` (ES-модуль, без npm) — единственный источник правды: `id`, `platform`,
`paths`, способ запуска (`go.packages`, `bazel.test_targets`, `python.suites`)
и Bazel-корни для сопоставления графа с юнитами. Рядом:

* `invalidations` — пути, которые валят больше юнитов, чем совпало по `paths`
  (`MODULE.bazel`, `MODULE.bazel.lock`, `.bazelrc`, `.bazelversion`,
  `BUILD.bazel`, toolchain-файлы → все платформы; `go.mod/go.sum/go.work` →
  все go-юниты; `platform/android/**` → все android-юниты;
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
  Всё остальное (`bazel_dep` любого модуля, `go_deps`, `oci`, `rules_go`,
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
* `platform/bazel/zig/**` и `platform/bazel/windows/**` — только платформа
  `native` (виртуальный `platform/bazel#native`): это target-constrained
  zig cc-toolchain'ы и rules_rust-патчи для кросс-сборки станции и Windows
  provider; хост-сборки Go/образов/Android их не выбирают. Остальной
  `platform/bazel/**` остаётся глобальным.
* Любое другое изменение (`bazel_dep`, toolchain, `go_deps`, `oci`,
  python), неразбираемый файл, отсутствующая base-ревизия или
  добавление/удаление `MODULE.bazel` оставляют глобальную инвалидацию:
  fail safe — «больше», не «меньше».
* `deploy_control` — какие go-юниты и пути включают дорогую регрессию
  `make test-deploy-control` (scheduler, ha-bridge/ha-control,
  `projects/home/kubernetes/scheduler/**`).
* `deploy` (у деплоимых юнитов) — `images`, `workloads` и `builder`
  (`bazel`/`docker`/`mixed`). По нему `deploy-home` и job `images` в `ci.yml`
  выбирают, что именно собирать и раскатывать; инфраструктурные юниты этой
  секции не имеют. `mixed` — один юнит с обоими типами образов: сегодня
  `hermes` (Docker) и `voice-proxy` (Bazel `//solutions/voice-proxy`).

### Как считается затронутость

| Платформа | Источник зависимостей | Пример |
|---|---|---|
| go | `go list -deps` пакетов юнита | правка `packages/proto` выбирает все юниты, которые его импортируют |
| android | обратные зависимости Bazel (`bazel query --output=xml`, loading phase) | правка `packages/kotlin/core` выбирает `kotlin-shared` и только те приложения, что от него зависят |
| bazel | те же обратные зависимости Bazel, что у android | правка `projects/station/pulsar/satellite/crates/pulsar-core` выбирает `pulsar-bazel` |
| python | пути юнита (+ `depends_on`) | правка `projects/home/proxmox/**` выбирает `proxmox` |
| native | пути юнита | правка `solutions/windows-logon-provider/**` выбирает `logon-provider` |
| quality | всегда | audit/Gitleaks/shellcheck идут на любом изменении |

Android-граф берётся из loading-фазы намеренно: он не требует JDK и Android
SDK, поэтому маршрутизация дешёвая и не может «сломаться из-за toolchain».

Замыкание `go list` работает только внутри go-юнитов. Деплой-юнит `hermes`
имеет платформу `python` и собственных go-пакетов, поэтому изменения
`packages/go/**` не доходят до него через зависимость; путь добавлен в
`hermes.paths` явно (тесты при этом остаются у go-юнита `voice-proxy`).
Изменённый файл сопоставляется с `rule-input` (source file); для нового,
удалённого файла или правки `BUILD.bazel`/`.bzl` берётся весь пакет — это
безопасное over-approximation.

### Fallback: только «больше», никогда «ничего»

* `MODULE.bazel`/`MODULE.bazel.lock`, `.bazelrc`, `.bazelversion`,
  `.bazelignore`, `BUILD.bazel`, `platform/bazel/**` → все платформы и все
  юниты; исключения — scoped-диффы, описанные выше: `MODULE.bazel#maven` →
  только android, `MODULE.bazel#native` → native + pulsar-bazel,
  `.bazelrc#cache` → ни одной платформы, `platform/bazel#native` → native;
* `go list` упал → все go-юниты; `bazel query` упал → все android и bazel юниты;
* база не определена (новый push, `workflow_dispatch`) → все юниты
  (`affected.mjs --all`);
* база определена, но `git diff` пуст → ни один тяжёлый юнит, только quality.

Внутри android-юнита список целей дополнительно сужается
`target-determinator` (`v0.34.0`, sha256 закреплён в
`platform/ci/install-target-determinator.sh`). Он анализирует граф базовой
ревизии, поэтому запускается в job с JDK/SDK и полной историей; при любой
ошибке, таймауте или отсутствии бинаря `platform/ci/bazel-targets.sh` печатает
весь объявленный список целей.

### Инварианты и проверки

`platform/ci/check_units.mjs` падает, если:

* в карте дубли id, неизвестная платформа, пустые `paths` или слишком широкий
  шаблон (`**`);
* android-юнит не объявил `bazel.roots`, `test_targets` или цель вне корней;
* `depends_on` ссылается на несуществующий юнит;
* объявленный python-suite не содержит `test_*.py` (для каталогов без
  герметичных тестов есть `python.compile`: только `compileall`);
* **любой tracked `*.go`/`*.py`/`*.kt`/`*.js`/`*.mjs`/`*.sh`/`*.bzl`/`BUILD.bazel` не
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
# какие юниты выбрал бы CI для диапазона (по умолчанию origin/develop...HEAD)
make ci-affected
platform/ci/affected.sh --base origin/develop --format json
platform/ci/affected.sh --base HEAD~3 --format text

# имитация конкретного изменения без коммита
platform/ci/affected.sh --changed-file solutions/scheduler/crates/scheduler/src/store/tick.rs
platform/ci/affected.sh --changed-file packages/kotlin/core/src/main/kotlin/.../TimeText.kt

# принудительно все юниты (то же, что workflow_dispatch)
platform/ci/affected.sh --all

make ci-units                                   # инварианты карты юнитов
node --test platform/ci/*.test.mjs platform/quality/*.test.mjs        # маршрутизатор, gate и аудит
# продвижение по tree hash: теги, digest, fallback — без registry
node --test platform/ci/promote.test.mjs
# политика чистки GHCR: pure-функция + локальный dry-run по фикстуре
node --test platform/ci/ghcr_cleanup.test.mjs
node platform/ci/ghcr_cleanup.mjs --fixture platform/ci/testdata/ghcr-versions.json \
  --now 2026-11-04T00:00:00Z
make test                                       # герметичный gate (включает CI-тесты)
make test-deploy-control BAZEL=bazelisk         # регрессия deploy control
actionlint .github/workflows/*.yml
shellcheck -S error platform/quality/*.sh platform/ci/*.sh \
  projects/home/kubernetes/scheduler/*.sh projects/station/scripts/*.sh
```

`affected.sh` только читает Git и граф загрузки Bazel: он ничего не собирает,
не деплоит и не трогает устройства. Версии сторонних actions закреплены
commit SHA (сверены с тегами через `gh api`); Bazelisk `1.30.0`, actionlint
`1.7.12`, target-determinator `0.34.0` и Gitleaks `8.30.1` скачиваются с
проверкой sha256.

На чистом checkout (CI) отсутствуют игнорируемые источники, поэтому
`make test` явно печатает `SKIP` для тестов, которым нужен nested
`projects/station/resources/quasar-connect-bot`; локально, где источник
есть, те же тесты выполняются. Pulsar теперь лежит в дереве
(`projects/station/pulsar/`), поэтому его installer и Rust-тесты доступны
всегда; installer запускается с `GOWORK=off` (это отдельный Go-модуль, не
входящий в корневой `go.work`). `test_deploy_control.sh` больше не требует
собранных бинарников: он передаёт deploy.sh заранее разрешённые
digest-пины образов, поэтому `make test` и `make test-deploy-control`
запускают его без Bazel. Путь продвижения (`PREBUILT_DIGESTS`,
`PREBUILT_DIGESTS_FILE`, решение 36) покрыт там же и в `test_deploy.py`
юнитов `scheduler`/`currency-bot`/`mcp-gateway`/`wakegrant`:
образ не собирается и не пушится, digest попадает в отрендеренный манифест,
а кривое/дублирующее значение падает fail-closed до обращения к кластеру.
`projects/home-assistant/deploy/test_build_and_push.py` проверяет то же для
`build-and-push.sh` (`hermes`, `voice-proxy`, `health-exporter`,
`home-deploy`) с заглушками docker/bazel/git. Hermes rollout и
render-config покрыты Go-тестами `platform/deploy/internal/hermes` и
`.../hermesconfig` (включая golden parity с Python-выходом рендерера);
health exporter — `.../deploy/health-exporter`.

### Required check

`ci-ok` (`if: always()`) — единственный required status check для ruleset
ветки `develop`. Он зелёный только если все зависимые job'ы завершились
`success` или законно `skipped`; любой `failure`/`cancelled` (включая упавший
`changes`) делает его красным. Логика вынесена в `platform/ci/ci_gate.mjs`.
Matrix-entry `android / android-lint` не требует отдельной записи в gate: он
входит в job `android`, чей результат `ci-ok` уже проверяет, поэтому падение
lint делает `ci-ok` красным как и любой другой android-юнит.

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
  Bazel/rules_oci; build+push → digest в релиз `runtime` →
  `-healthcheck`/health/Assist-проверка → `helm rollback` при провале.
- Build once (решение 36): перед сборкой job считает tree деплоимого коммита
  и пробует продвинуть digest'ы PR-сборки (`tree-<hash>` + проверенный
  provenance). Если все образы юнита прошли проверку, скрипт получает
  `PREBUILT_DIGESTS` и не собирает/push'ит их; иначе — обычная сборка.
  Выбранный путь виден в job summary (`image path: prebuilt|build`).
- Native (Pulsar, logon provider): job `native-release` вызывает `_native.yml`
  с `promote: true`. Тот проверяет `tree-<hash>` и provenance, ставит релизные
  теги на тот же digest и пропускает сборку; при missing/unverified собирает и
  публикует как раньше. Summary пишет `path: promoted|build` и
  `fallback build`. `deploy-station` ждёт `native-release` и ставит версию.
- Откат — только применённого объекта; изменение целиком откатывается новым
  PR с `git revert`.

Pulsar: хост-сборка и тесты — Bazel-юнит `pulsar-bazel`; станционные
бинарники (Bazel), radio, ssh, installer и релиз — native-юнит `pulsar`
(`_native.yml`). Каждый PR собирает release-shaped артефакт и публикует
`tree-<hash>`; merge продвигает его либо пересобирает fallback-путём.

## Публикация

```bash
node platform/quality/audit_repository.mjs
node --test platform/ci/*.test.mjs platform/quality/*.test.mjs
make test
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
