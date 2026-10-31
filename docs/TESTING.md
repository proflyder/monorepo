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
| HA live | API, entities, text intents, optional mutation/alarm |
| Gateway live | bearer auth, status shape, method guards |
| Pulsar installer | upstream Go tests (только при наличии nested checkout) |

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
go vet ./solutions/...
```

Тесты scheduler/bridge герметичны: SQLite — во временном каталоге, scheduler,
HA и Connect-клиенты — через `httptest`/фейки; живые HA, ADB и
устройство не используются. Scheduler API отдаёт Connect JSON на
`/ricardo.v1.SchedulerService/<Method>`; bridge проверяется как настоящий
клиент этого контракта.

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

Bazel-гейты отдельные:

```bash
# backend-таргеты (нужен Bazelisk; локально — secrets/local-tools/bazelisk)
bazel test //solutions/... //packages/...
```

Android-сборка/тесты требуют Android SDK и выполняются из
`projects/mobile/android` отдельно; они не входят в backend `make test`.

```bash
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
./secrets/local-tools/bazelisk test \
  //projects/mobile/android:android_tests \
  //projects/mobile/android/app:main_activity_smoke_test \
  //projects/mobile/android:app \
  //solutions/scheduler/... //solutions/alarm/... //packages/proto/... \
  --nocache_test_results --test_output=errors
./secrets/local-tools/bazelisk build //projects/mobile/android:app
# debug APK: bazel-bin/projects/mobile/android/app/app.apk
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

## Не автоматизировано

- реальный BlueZ/ALSA/event2/Wi-Fi rollback на станции;
- Pulsar C++/прошивка/cross-toolchain;
- микрофон/ASR, Spotify 429, реальный звук;
- `resources/spotify-autostart/spotify.sh` — документированный legacy,
  исключён из syntax gate.

Эти пути — live/ручная приёмка. Структурная проверка HA не равна runtime coverage.

## CI (GitHub Actions)

`.github/workflows/ci.yml` — hermetic-проверки без деплоя и без секретов.
Триггеры: push и pull request в `develop`, ручной `workflow_dispatch` (он
прогоняет все юниты); `permissions: contents: read`, `concurrency` отменяет
устаревший прогон только для PR. Выкладка — отдельный
`.github/workflows/deploy-home.yml` на push в `develop` (см. ниже).

### Маршрутизация по юнитам

CI не гоняет всё на каждый push. Единица запуска — **юнит**: один сервис,
одно приложение, общая библиотека или каталог тестов. Юнит = одна запись в
matrix одного платформенного workflow.

| Платформа | Reusable workflow | Что делает юнит |
|---|---|---|
| go | `_go.yml` | `go vet`, `go test` (`-race` где объявлено), `go build` бинарей |
| android | `_android.yml` | JDK 21 + SDK 36, `bazel test`/`build`, APK в artifact |
| python | `_python.yml` | `compileall` + `unittest discover` по каталогам юнита |
| quality | `_quality.yml` | audit, static gates, unit-map, actionlint, shellcheck, Gitleaks |

`ci.yml` содержит только координацию: `changes` → matrix-вызовы reusable
workflow → `ci-ok`. Отдельного generic Bazel-workflow нет: Bazel — внутренний
инструмент `_android.yml` и deploy-control регрессии, а не платформа.

`changes` выдаёт outputs `go_units`, `android_units`, `python_units`,
`deploy_control`, `base_sha`, `docs_only`; каждый output — JSON-массив записей
matrix. Пустой массив означает «пропустить job» (законный skip для `ci-ok`).

### Карта юнитов

`platform/ci/units.yml` — единственный источник правды: `id`, `platform`,
`paths`, способ запуска (`go.packages`, `bazel.test_targets`, `python.suites`)
и Bazel-корни для сопоставления графа с юнитами. Рядом:

* `invalidations` — пути, которые валят больше юнитов, чем совпало по `paths`
  (`MODULE.bazel*`, `.bazelrc`, `.bazelversion`, `BUILD.bazel`, toolchain-файлы
  → все платформы; `go.mod/go.sum/go.work` → все go-юниты; `platform/android/**`
  → все android-юниты; `requirements-test.txt` → python+quality).
* `deploy_control` — какие go-юниты и пути включают дорогую регрессию
  `make test-deploy-control` (scheduler, ha-bridge/ha-control,
  `projects/home/kubernetes/scheduler/**`).
* `deploy` (у деплоимых юнитов) — `images`, `workloads` и `builder`
  (`bazel`/`docker`). По нему `deploy-home` и job `images` в `ci.yml`
  выбирают, что именно собирать и раскатывать; инфраструктурные юниты этой
  секции не имеют.

### Как считается затронутость

| Платформа | Источник зависимостей | Пример |
|---|---|---|
| go | `go list -deps` пакетов юнита | правка `packages/proto` выбирает все юниты, которые его импортируют |
| android | обратные зависимости Bazel (`bazel query --output=xml`, loading phase) | правка `packages/kotlin/core` выбирает `kotlin-shared` и только те приложения, что от него зависят |
| python | пути юнита (+ `depends_on`) | правка `projects/home/proxmox/**` выбирает `proxmox` |
| quality | всегда | audit/Gitleaks/shellcheck идут на любом изменении |

Android-граф берётся из loading-фазы намеренно: он не требует JDK и Android
SDK, поэтому маршрутизация дешёвая и не может «сломаться из-за toolchain».
Изменённый файл сопоставляется с `rule-input` (source file); для нового,
удалённого файла или правки `BUILD.bazel`/`.bzl` берётся весь пакет — это
безопасное over-approximation.

### Fallback: только «больше», никогда «ничего»

* `MODULE.bazel*`, `.bazelrc`, `.bazelversion`, `.bazelignore`, `BUILD.bazel`,
  `platform/bazel/**` → все платформы и все юниты;
* `go list` упал → все go-юниты; `bazel query` упал → все android-юниты;
* база не определена (новый push, `workflow_dispatch`) → все юниты
  (`affected.py --all`);
* база определена, но `git diff` пуст → ни один тяжёлый юнит, только quality.

Внутри android-юнита список целей дополнительно сужается
`target-determinator` (`v0.34.0`, sha256 закреплён в
`platform/ci/install-target-determinator.sh`). Он анализирует граф базовой
ревизии, поэтому запускается в job с JDK/SDK и полной историей; при любой
ошибке, таймауте или отсутствии бинаря `platform/ci/bazel-targets.sh` печатает
весь объявленный список целей.

### Инварианты и проверки

`platform/ci/check_units.py` падает, если:

* в карте дубли id, неизвестная платформа, пустые `paths` или слишком широкий
  шаблон (`**`);
* android-юнит не объявил `bazel.roots`, `test_targets` или цель вне корней;
* `depends_on` ссылается на несуществующий юнит;
* объявленный python-suite не содержит `test_*.py` (для каталогов без
  герметичных тестов есть `python.compile`: только `compileall`);
* **любой tracked `*.go`/`*.py`/`*.kt`/`*.sh`/`*.bzl`/`BUILD.bazel` не
  принадлежит ни одному юниту** — новый тест не может молча выпасть из CI.

Тесты маршрутизатора: `platform/ci/test_affected.py` (включая фикстуру
`testdata/units-two-apps.yml` с двумя Android-приложениями и общим
Kotlin-юнитом).

### Расширение

Новая платформа (например Rust): добавить reusable workflow `_rust.yml` с
inputs юнита, добавить `platform: rust` в `units.yml` (`cargo`-поля), научить
`affected.py` считать затронутые юниты (для Cargo — `cargo metadata`/
`cargo tree`, для Bazel-целей — `bazel.roots`, как у android) и добавить job с
matrix в `ci.yml`. Новый юнит — это новая запись в `units.yml` и путь в
`paths`; отдельный job не нужен.

### Локальное воспроизведение

```bash
# какие юниты выбрал бы CI для диапазона (по умолчанию origin/develop...HEAD)
make ci-affected
platform/ci/affected.sh --base origin/develop --format json
platform/ci/affected.sh --base HEAD~3 --format text

# имитация конкретного изменения без коммита
platform/ci/affected.sh --changed-file solutions/scheduler/internal/store/tick.go
platform/ci/affected.sh --changed-file packages/kotlin/core/src/main/kotlin/.../TimeText.kt

# принудительно все юниты (то же, что workflow_dispatch)
platform/ci/affected.sh --all

make ci-units                                   # инварианты карты юнитов
python3 -m unittest discover -s platform/ci -p 'test_*.py' -v
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
`make test` явно печатает `SKIP` для тестов, которым нужен nested Pulsar
(`projects/station/upstream/pulsar-rust/installer`) или
`projects/station/resources/quasar-connect-bot`; локально, где источники
есть, те же тесты выполняются. `test_deploy_control.sh` больше не требует
собранных бинарников: он передаёт deploy.sh заранее разрешённые
digest-пины образов, поэтому `make test` и `make test-deploy-control`
запускают его без Bazel.

### Required check

`ci-ok` (`if: always()`) — единственный required status check для ruleset
ветки `develop`. Он зелёный только если все зависимые job'ы завершились
`success` или законно `skipped`; любой `failure`/`cancelled` (включая упавший
`changes`) делает его красным. Логика вынесена в `platform/ci/ci_gate.py`.

### Деплой (`deploy-home`)

Push в `develop` — это squash-merge PR, поэтому workflow получает секреты
Environment `Home`. `plan` прогоняет тот же `units.yml` по диапазону
`before..sha` и деплоит только затронутые деплоимые юниты параллельно:
VPN группой `ci` → GitHub OIDC в OpenBao (`hashicorp/vault-action`,
роль `deploy-home`) → `secret/ci/deploy-home` → kubectl/helm к K3s `6443`
и Traefik `443`.
Документация и инфраструктурные юниты без `deploy` ничего не катят.

- Смена режима (`preserve`, `execute`, `shadow`, `control-*`) — только
  `workflow_dispatch`; push всегда conservative preserve.
- Hermes/voice-proxy: build+push → digest в релиз `runtime` →
  health/Assist-проверка → `helm rollback` при провале.
- Откат — только применённого объекта; изменение целиком откатывается новым
  PR с `git revert`.

Pulsar собирается и релизится в своём репозитории; монорепо в CI его не
собирает. Деплой релиза Pulsar на станцию — открытая задача (`PENDING.md`).

## Публикация

```bash
python3 platform/quality/audit_repository.py
python3 -m unittest discover -s platform/quality -p 'test_*.py' -v
make test
```

Проверить локальные Markdown-ссылки, приватные пути, историю Git и
`SENSITIVE-DATA.md`. Эвристика не заменяет ручной аудит.

`audit_repository.py` содержит два узких публичных исключения, одобренных
пользователем: точный домен `proflyder.dev` и Android namespace
`dev.proflyder.home` с подпакетами/путём `dev/proflyder/home`. Исключения
удаляются только как целые токены с границами; другие TLD, username,
поддомены, длинные хосты/пути и смешанные строки по-прежнему дают ошибку.
Границы закреплены `platform/quality/test_audit_repository.py`. Общих
исключений Gitleaks нет.
