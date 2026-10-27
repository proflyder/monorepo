# Yandex Station Midi — root lab


> Публичная копия: serial/MAC, внешний адрес и tailnet-домен заменены примерами.
> Реальные значения и исходные локальные документы — только в `secrets/`.
> LAN-адреса и remote-пути описывают стенд; перед применением адаптировать.

Лаборатория по своей **Station Midi** (`STATION_SERIAL`, `cucumber`, `192.0.2.13`):
root/ADB, SSH, локальный MITM-стенд, OTA/ADB-защита, Яндекс-карантин и
эксперименты с network provisioning.

## С чего начать

- **Восстановление с нуля:** [`docs/REBUILD.md`](docs/REBUILD.md)
- **Ключевые решения:** [`docs/DECISIONS.md`](docs/DECISIONS.md)
- **Текущее состояние:** [`docs/STATE.md`](docs/STATE.md)
- **Что делать дальше:** [`docs/PENDING.md`](docs/PENDING.md)
- **Компактная память:** [`docs/CONTEXT.md`](docs/CONTEXT.md)
- **Команды и откат:** [`docs/OPERATIONS.md`](docs/OPERATIONS.md)
- **Монорепозиторий и Bazel:** [`docs/MONOREPO.md`](docs/MONOREPO.md)
- **Карта всех доков и правила обновления:** [`docs/README.md`](docs/README.md)
- **Чувствительные данные (реестр, без значений):**
  [`docs/SENSITIVE-DATA.md`](docs/SENSITIVE-DATA.md)
- **Исследование своего ассистента (Pulsar/HA/Spotify/MCP/wake word):**
  [`docs/RESEARCH-OWN-ASSISTANT.md`](docs/RESEARCH-OWN-ASSISTANT.md)
- **Удалённый доступ к HA (Tailscale/Serve, Link, Cloudflare, VPS):**
  [`docs/REMOTE-ACCESS.md`](docs/REMOTE-ACCESS.md)
- **Тесты и покрытие:** [`tests/README.md`](tests/README.md),
  [`reports/TEST-COVERAGE.md`](reports/TEST-COVERAGE.md)
- **Неттоп `lab-host` / Home Assistant:** [`projects/home-assistant/NETTOP.md`](projects/home-assistant/NETTOP.md)
- **Голос «Рикардо», внедрение и следующий этап:** [`docs/RICARDO-TTS-HANDOFF.md`](docs/RICARDO-TTS-HANDOFF.md)
- **Hermes fallback / MCP / web search:** [`projects/home-assistant/hermes/README.md`](projects/home-assistant/hermes/README.md)

## Доступ

```bash
adb connect 192.0.2.13:5555
ssh root@192.0.2.13
```

Пароль SSH — в `secrets/CREDENTIALS.md`. Порты `22/5555` не выставлять наружу.

## Статус

Станция в Яндекс-карантине, egress-guard разрешает только private LAN/DHCP/NTP;
ADB/SSH работают. Установлен **Pulsar v0.4.1-ricardo6** (HA ESPHome satellite +
Sendspin), штатная Алиса/`maind` остановлены. Свой BLE gateway `Station-Gateway`
отвечает за 1/3/4/5-жесты; голосовые команды — HA. STT — GigaAM v3
(`stt.onnx_asr`) с Whisper fallback; Hermes `v0.22.5` на неттопе даёт
fallback для свободной речи и веб-поиска. Matter Server `matterjs-server`
(`ws://127.0.0.1:5580/ws`) подключён к HA; Yandex LED strip ждёт pairing.
Xiaomi Home `v0.5.0` установлена для `MJGJD02YL`; вход в China/импорт — вручную.
Remote HA — Tailscale/Serve
(`https://lab-host.example-tailnet.ts.net`), HA не публичный. Детали — `docs/STATE.md`;
восстановление — `docs/REBUILD.md`.

## Монорепозиторий

Горизонтальная структура: переиспользуемые `solutions/`, общие `packages/` и
композиции `projects/`. Станция и Home Assistant — независимые проекты
`projects/station` и `projects/home-assistant`; `projects/home` собирает их
вместе с alarm/MCP/UI.

```bash
bazel build //solutions/alarm
bazel test //solutions/alarm/...
bazel run //solutions/alarm
```

Bazelisk: `go install github.com/bazelbuild/bazelisk@latest`. Подробности,
горизонтальная структура и порядок миграции — [`docs/MONOREPO.md`](docs/MONOREPO.md).
`projects/station/ble-gateway` пока остаётся отдельным Go-модулем.

Внутренний API нового control plane — gRPC/protobuf:
`packages/proto/ricardo/v1/alarms.proto`; для HA/браузера — Connect/gRPC-Web на
том же контракте. Детали — [`docs/GRPC.md`](docs/GRPC.md).

## Тесты

```bash
make test          # hermetic: HA YAML, shell, BLE Go, pairing
make coverage      # + coverage отчёт
make test-e2e      # live HA/BLE gateway (нужны токены)
```

Текущая площадь и границы — [`reports/TEST-COVERAGE.md`](reports/TEST-COVERAGE.md);
инструкции — [`tests/README.md`](tests/README.md).

## Локальное версионирование

Репозиторий готовится к публикации на GitHub; remote/push — по отдельному запросу.
Результат проверки и ограничения — `docs/REPOSITORY-AUDIT.md`.
Правила для всех сессий — `AGENTS.md`. Перед коммитом изменений Pulsar
выполнить `python3 projects/station/scripts/snapshot-pulsar.py`: версия лаборатории сохранит
полный патч и базовый commit, не затрагивая вложенный upstream-репозиторий.
Секреты, записи голоса, бэкапы и тяжёлые сборки остаются вне Git.

## Публичные примеры и приватное окружение

Адреса `192.0.2.0/24`, `lab-host`, `labuser`, `/Users/developer` и
`/home/trainer` — примеры. Перед deployment подставьте свои настройки.
Оригинальные локальные конфиги сохранены отдельно под `secrets/`; эта
очистка не меняет запущенные устройства.

Публикуется только `develop`. Ветка `local-history` хранится исключительно
локально и содержит прежние приватные данные. Не используйте push --all,
--mirror, --tags или --no-verify. Для локальной защиты включите
`git config --local core.hooksPath .githooks`; перед push нужен Gitleaks.
Копировать весь каталог проекта или `.git` для публикации нельзя.
