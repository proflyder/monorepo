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

`.github/workflows/ci.yml` — этап 1: hermetic-проверки без деплоя и без
секретов. Триггеры: push и pull request в `develop`, ручной
`workflow_dispatch`; `permissions: contents: read`, `concurrency` отменяет
устаревший прогон только для PR.

| Job | Что делает | Таймаут |
|---|---|---|
| `changes` | определяет затронутые области (`dorny/paths-filter`) | — |
| `quality` | `audit_repository.py`, static gates, shellcheck, Gitleaks 8.30.1 | 15 мин |
| `test` | `make test` на чистом checkout | 30 мин |
| `bazel` | Bazelisk, `bazel build/test //solutions/... //packages/...`, deploy control regression | 45 мин |
| `android` | Android SDK 36, Android-тесты, debug APK в artifact | 60 мин |

`bazel` и `android` не запускаются на изменениях только в документации.
Версии сторонних actions закреплены commit SHA; Bazelisk `1.30.0` и
Gitleaks `8.30.1` скачиваются с проверкой sha256.

На чистом checkout (CI) отсутствуют игнорируемые источники, поэтому
`make test` явно печатает `SKIP` для тестов, которым нужен nested Pulsar
(`projects/station/upstream/pulsar-rust/installer`) или
`projects/station/resources/quasar-connect-bot`; локально, где источники
есть, те же тесты выполняются. `test_deploy_control.sh` требует собранные
бинарники в `projects/home/kubernetes/scheduler/dist/`: CI собирает их в
job `bazel` через `make test-deploy-control`; без бинарников `make test`
печатает `SKIP` с причиной, а `REQUIRE_DEPLOY_CONTROL=1` превращает это
в ошибку.

Локальное воспроизведение CI:

```bash
make test
make test-deploy-control BAZEL=bazelisk   # после сборки dist/ прогонит deploy control
actionlint .github/workflows/ci.yml
shellcheck -S error platform/quality/*.sh \
  projects/home/kubernetes/scheduler/*.sh projects/station/scripts/*.sh
```

Pulsar собирается и релизится в своём репозитории; монорепо в CI его не
собирает. Деплой (этапы 2–3) — отдельные workflows с Environment и ручным
approve.

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
