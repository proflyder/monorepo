# TEST COVERAGE — площадь проверки

Сгенерировано: `2026-10-29 04:30 +0500`. Это не отчёт «покрыто всё»: ниже разделены
машинно измеренное покрытие, статическая проверка артефактов и живые E2E.

## Итог

| Область | Объём | Что проверено | Статус |
|---|---:|---|---|
| HA voice YAML | 4969 строк, 14 файлов | 49/49 scripts, 46/46 intents, 7/7 automations, 286 фраз, 991 Jinja-вхождений | ✅ структурно 100% |
| BLE gateway (Go) | 1304 строк | unit/HTTP/GATT/Wi-Fi mocks | 45.1% statements |
| home-assistant shell | 432 строк | `bash -n`, ShellCheck errors, `ctl.sh` команды | ✅ command paths + static |
| scripts (security/ops) | 433 строк | syntax, egress-guard rules, pairing CLI | ✅ targeted |
| pairing core | 440 строк | 15 unit tests | 88.0% lines |
| Pulsar installer (Go, Rust fork) | 1410 строк | upstream unit tests | 18.0% statements |
| Pulsar satellite (Rust) | 36781 строк | unit-тесты Rust этим отчётом не исполняются | ⚠️ не измерено |

**Взвешенная оценка по строкам:** ~**15%** первой-party поверхности.
Это оценка, а не CI-метрика: HA YAML считается структурно проверенным (100%),
Rust satellite/hardware — 0%, Go/Python — по измеренному покрытию. Вендорные
`resources/` (кроме активного pairing) в оценку не входят.

## Тесты

- Python hermetic: **146**
- Existing pairing: **15**
- Go BLE gateway: **22**
- Go Pulsar installer: **12**
- Итого: **195** тестовых функций

## E2E (live)

| Набор | Режим | Результат |
|---|---|---|
| HA API/entities/intents | read-only | 3/3 passed |
| BLE gateway HTTP | read-only, auth | 3/3 passed |
| Volume command keeps playback | opt-in mutation | 1/1 passed |
| Alarm set/cancel without ringing | opt-in mutation | 1/1 passed |
| Alarm start → text «стоп» → resume | opt-in audio | 1/1 passed |
| Hermes API/models/toolsets | read-only | 3/3 passed |
| Hermes real chat turn | opt-in quota | 1/1 passed |
| Real microphone voice (GigaAM + Hermes) | manual | 4/4 passed |

E2E запускается отдельно: `make test-e2e` (нужен `HA_TOKEN`; для gateway —
`STATION_CONTROL_TOKEN`; для Hermes — `HERMES_API_KEY`). Audio/mutation
и реальный chat turn — только с явными флагами.
Результаты выше — последний прогон на этом стенде 2026-10-26; не путать
с hermetic-тестами, которые не трогают станцию.

## Что сознательно вне gate

- Реальный BlueZ/D-Bus, ALSA, кнопки event2 и Wi-Fi откат: требуют станции;
  покрыты live smoke/E2E и ручными сценариями, не unit-тестами.
- Pulsar Rust/hardware: unit-тесты запускаются `make -C satellite check`;
  этот отчёт их не исполняет и покрытие по ним не заявляет.
- Реальный микрофон/Whisper/ASR и Spotify 429: недетерминированы; проверяются
  голосом и live E2E там, где возможно.
- `resources/spotify-autostart/spotify.sh` — документированный битый legacy;
  исключён из syntax gate, но проверяется, что исключение остаётся в docs.

## Как воспроизвести

```bash
make test                  # hermetic: Python + Go + pairing
make coverage              # hermetic + coverage + этот отчёт
make test-e2e              # HA/gateway read-only
STATION_E2E_AUDIO=1 make test-e2e   # + короткий alarm/resume тест
```
