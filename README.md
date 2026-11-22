# Yandex Station Midi — home lab

Стенд: Proxmox + HAOS VM + Ubuntu/K3s VM; станция Midi с Pulsar.
Публичные адреса — примеры; реальные — в `secrets/`.

## Старт

- [Текущее состояние](docs/STATE.md)
- [Архитектура](docs/ARCHITECTURE.md)
- [Эксплуатация](docs/OPERATIONS.md)
- [Kubernetes/K3s](docs/KUBERNETES-OPERATIONS.md)
- [Сеть и порты](docs/NETWORK.md)
- [Восстановление](docs/REBUILD.md)
- [Секреты и доступы](docs/SECRETS.md)
- [Все документы](docs/README.md)

## Что где

```text
projects/station/       Pulsar, BLE gateway, инструменты станции
projects/home/          Proxmox, K3s, Terraform, VPN
projects/home-assistant/HA, voice, Hermes, MA, TTS, dashboards
projects/mobile/        Android-клиент
solutions/              переиспользуемые сервисы (scheduler, mcp-gateway, …)
packages/               proto, контракты, общий Rust/Kotlin-код
platform/               CI, Bazel, деплой, проверки качества
docs/                   эксплуатация и политики
secrets/                реальные адреса, доступы, recovery (вне Git)
```

## Компоненты

- **HAOS** — единственный HA runtime: Home Assistant, Music Assistant,
  Matter Server как add-ons.
- **K3s** — STT/TTS, Hermes + voice-proxy, scheduler и его исполнители,
  intercom, mcp-gateway, currency-bot, OpenBao, Vaultwarden, Nexus,
  observability; всё, кроме OpenBao, применяет Flux.
- **VPN** — собственный WireGuard + ocserv в CT 102.
- **Станция** — Pulsar (Rust, armhf), wake word «Рикардо», BLE gateway.
- **Android** — `dev.proflyder.home` (`projects/mobile/android`).
- **CI/CD** — PR → `ci-ok` → squash merge → `deploy-home` пинует digest'ы,
  Flux применяет. Входящего доступа CI в K3s нет.

Версии, адреса и порты — `docs/STATE.md` и `docs/NETWORK.md`; доступы —
`docs/SECRETS.md` (раздел «Доступ к хостам»).

## Тесты

```bash
mise run test      # hermetic
mise run coverage  # coverage (reports/TEST-COVERAGE.md, not tracked)
mise run test:e2e  # live HA/BLE, нужны токены
```

Подробности — `docs/TESTING.md`: слои, live-проверки и CI
(`.github/workflows/ci.yml`, `deploy-home.yml`).

## Публикация

Изменения попадают в `develop` только через pull request (squash, обязательный
`ci-ok`); merge в `develop` пересчитывает pins, а сервисы применяет Flux.
Перед push: `mise run test`, `node platform/quality/audit_repository.mjs`,
реестр приватных данных (`docs/SECURITY.md`) и Gitleaks (их же выполняет
pre-push guard). Поток веток и PR — `docs/GIT.md`.

Секреты, записи голоса, backups и тяжёлые сборки остаются вне Git.
