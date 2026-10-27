# Tests — hermetic + live E2E

Проект вырос из набора shell-скриптов в несколько runtime-контуров: HA/YAML,
BLE gateway (Go), HA stack в `projects/home-assistant`, pairing (Python) и Pulsar. Тесты разделены по
цене и среде, чтобы быстрый прогон не трогал станцию.

## Быстрый старт

```bash
# окружение (PyYAML/Jinja2; coverage — только для make coverage)
python3 -m pip install -r requirements-test.txt
# shellcheck, Go, ffmpeg — по необходимости

make test          # локальные hermetic-тесты, без сети/станции
make coverage      # то же + Go/Python coverage + reports/TEST-COVERAGE.md
make test-e2e      # live HA/BLE gateway, read-only
```

`make test` — это gate для обычных изменений. `make test-e2e` требует явных
токенов (`HA_TOKEN`, опционально `STATION_CONTROL_TOKEN`/`HERMES_API_KEY`)
и не запускается в hermetic-наборе.

## Слои

| Слой | Файлы | Что ловит |
|---|---|---|
| HA static | `tests/ha/test_ha_config.py` | битый YAML, duplicate keys, неизвестные entity/service/script, битый Jinja, drift intent ↔ sentence, сломанный alarm/music state machine |
| Shell hermetic | `tests/shell/test_ctl.py` | `ctl.sh`: команды, aliases, up/down/status/logs/backup/doctor; fake docker/curl/ss/systemd |
| Security/ops hermetic | `tests/shell/test_egress_guard.py` | allowlist IPv4/IPv6, NTP toggle, idempotency shape; fake iptables |
| Static gates | `tests/shell/test_static.py` | `bash -n` всех shell (кроме документированного legacy), ShellCheck errors, Python syntax, secret patterns |
| Pairing | `tests/pairing/test_station_pairing_sound.py` | WAV/OGG CLI, token redaction |
| BLE gateway | `projects/station/ble-gateway/*_test.go` | token, Wi-Fi validation/rollback, WPA parsing, GATT read/write/offset, commit, HTTP auth/status, event parser |
| HA live | `tests/e2e/live_ha.py` | API, entities, text intents, optional mutation/alarm |
| Gateway live | `tests/e2e/live_gateway.py` | bearer auth, status shape, method guards (read-only) |
| Existing pairing | `projects/station/resources/quasar-connect-bot/tests/` | payload/RS/audio roundtrip (15 tests, 88% lines) |
| Hermes profile | `tests/hermes/test_hermes_config.py` | model/provider, HA MCP allowlist, no dangerous tools, web search backend |
| Hermes voice proxy | `tests/hermes/test_voice_proxy.py` | URL/Markdown stripping for TTS, source citations, whitespace |
| Voice stack | `tests/ha/test_ha_config.py` | GigaAM ONNX Wyoming service, Assist satellite watchdog |

## Текущая площадь

Актуальные цифры и оговорки — [`reports/TEST-COVERAGE.md`](../reports/TEST-COVERAGE.md).

Коротко на 2026-10-26:

- HA voice: 12 YAML, 22 scripts, 22 intents, 3 automations, 115 фраз,
  497 Jinja-вхождений — структурно проверены все.
- BLE Go: **45.5% statements** (package 47.0%).
- pairing core: **88% lines**.
- Pulsar installer: 17.1% statements (upstream tests).
- Hermetic: 76 Python + 15 pairing + 22 Go + 10 installer = **123** функции.
- Взвешенная оценка первой-party поверхности: **~52%**; это не CI-метрика,
  см. методику в отчёте.

## Live E2E

### HA

Нужен токен (Long-Lived Access Token или короткоживущий из refresh-token
владельца). Токен не коммитить и не печатать.

```bash
export HA_URL=http://192.0.2.25:8123
export HA_TOKEN='...'
export HA_ASSIST_DEVICE_ID='...'   # опционально, для conversation
make test-e2e
```

Read-only набор проверяет API, обязательные entities, `какой будильник`,
`что с таймером`, source/route. Mutation/audio — отдельно:

```bash
# громкость + alarm set/cancel; музыка восстанавливается
STATION_E2E_MUTATE=1 make test-e2e

# короткий будильник на низкой громкости + текст «стоп» + resume
STATION_E2E_AUDIO=1 make test-e2e
```

Audio-тест:

- пропускается, если уже включён пользовательский будильник;
- ставит `alarm_volume=0.10` на несколько секунд;
- проверяет `alarm_ringing`, `alarm_music_was_playing`, stop через текст
  «стоп» и resume;
- в teardown восстанавливает source/volume/флаги и не вызывает
  `alarm_stop/cancel` без необходимости (иначе можно остановить музыку).

### Hermes

`HERMES_API_KEY` is read from the environment; never commit it. Read-only
checks `/health`, `/v1/models`, `/v1/capabilities` and `/v1/toolsets`. A real
model turn is separately opt-in because it spends ChatGPT quota.

```bash
export HERMES_API_URL=http://127.0.0.1:8642
export HERMES_API_KEY='...'
python3 -m unittest tests.e2e.live_hermes -v

HERMES_E2E_CHAT=1 python3 -m unittest tests.e2e.live_hermes -v
```

### BLE gateway

```bash
export STATION_CONTROL_TOKEN="$(adb -s 192.0.2.13:5555 shell cat /data/quasar/ble-gateway/control-token | tr -d '\r\n')"
export STATION_GATEWAY_URL=http://192.0.2.13:8099
make test-e2e
```

Read-only: 401 без токена, `/status`, 405 на неверные методы. Мутирующие
`/gateway/on` и `/bluetooth/on` в E2E не вызываются.

## Границы

Не автоматизировано unit-тестами:

- реальный BlueZ/D-Bus, ALSA, event2, Wi-Fi rollback на станции;
- Pulsar C++/firmware и cross-toolchain;
- реальный микрофон/Whisper/ASR, Spotify 429;
- `resources/spotify-autostart/spotify.sh` — документированный битый legacy,
  исключён из syntax gate; тест следит, что исключение остаётся в
  `docs/RESOURCES.md`.

Эти пути проверяются голосом, live E2E и ручными сценариями; не выдавать
структурную проверку HA за 100% runtime coverage.

## Чистый checkout и аудит публикации

`python3 tests/audit_repository.py` проверяет отслеживаемые файлы: локальные
Markdown-ссылки, gitlink, приватные пути/медиа и известные credential patterns.
Это эвристика, не гарантия отсутствия любых секретов.

`go test ./...` и Bazel targets доступны без приватной части. Полный
`make test` дополнительно требует игнорируемые checkout’ы Pulsar installer,
quasar-connect-bot и reference resources (включая документированный legacy
spotify.sh), а также зависимости из `requirements-test.txt` и pairing upstream.
Не считать его воспроизводимым на одном лишь публичном checkout.
На 2026-10-27 локальный прогон: 94 Python-теста + BLE/installer Go + 15 pairing.
Цифры 123 выше — исторический срез 2026-10-26, не число текущего прогона.
