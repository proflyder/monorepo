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
- [Все документы](docs/README.md)

## Что где

```text
projects/station/       Pulsar, ADB, BLE, provisioning
projects/home/          Proxmox, K3s, Terraform, VPN
projects/home-assistant/HA, voice, Hermes, MA, TTS, dashboards
projects/mobile/        Android-клиент
solutions/              переиспользуемые scheduler/alarm/bridge
packages/               proto, общий Go/Kotlin-код
docs/                   эксплуатация и политики
secrets/                реальные адреса, доступы, recovery (вне Git)
```

## Компоненты

- **HAOS** — единственный HA runtime: Home Assistant, Music Assistant,
  Matter Server как add-ons.
- **K3s** — GigaAM/Whisper/Piper, Hermes + voice proxy, scheduler,
  ha-bridge, ha-control, observability.
- **Traefik + cert-manager** — HTTPS-имена `ha`, `grafana`, `alarms`,
  `control`, `vault` под `*.home.example.com` с wildcard-сертификатом
  Let's Encrypt (ACME DNS-01 Cloudflare) и allow-list LAN/VPN.
- **VPN** — собственный ocserv в CT 102: `443/tcp+udp`, вход клиентским
  сертификатом или паролем + TOTP, группы `lan`/`full`/`ci`. Публичных
  сервисов нет.
- **OpenBao + External Secrets Operator** — единственный источник правды для
  секретов; ESO создаёт Kubernetes Secrets из `secret/k8s/*`.
- **CI/CD** — GitHub Actions: PR → `ci-ok` → squash merge → `deploy-home`
  деплоит затронутые юниты через VPN и OpenBao OIDC; образы GHCR по digest.

## Доступ

```bash
adb connect 192.0.2.13:5555
ssh root@192.0.2.13          # ключ; пароль — fallback
ssh root@192.0.2.26          # PVE
ssh labuser@192.0.2.27       # K3s VM
```

HA `192.0.2.28:8123`, MA `192.0.2.28:8095`, Grafana
`192.0.2.27:30300/`. Имена `https://<name>.home.example.com` — только через
LAN/VPN. В интернет ничего не публиковать, кроме VPN `443/tcp+udp`.

## Статус

- PVE `9.2.24`; HAOS `2026.10.4`; K3s `v1.35.9`.
- Wi-Fi 4addr/WDS + Ethernet fallback; VM IP не меняются.
- Pulsar Rust `v0.5.1002` (armhf, база upstream `v0.5.3`, Ricardo pre-roll
  256 ms), wake word «Рикардо», BLE gateway.
- HAOS add-ons: MA `2.10.8`, Matter `9.2.0`; K3s: STT/TTS/Hermes/
  scheduler/observability.
- Remote — собственный VPN (ocserv), HTTPS-имена, без публичного HA.

## Тесты

```bash
make test          # hermetic
make coverage      # coverage (reports/TEST-COVERAGE.md, not tracked)
make test-e2e      # live HA/BLE, нужны токены
```

Подробности — `docs/TESTING.md`: слои, live-проверки и CI
(`.github/workflows/ci.yml`, `deploy-home.yml`).

## Публикация

Изменения попадают в `develop` только через pull request (squash, обязательный
`ci-ok`); merge в `develop` выкатывает затронутые сервисы. Публикуются рабочие
ветки `<type>/<name>`, без секретов и Git-истории с приватными данными. Перед
push: `make test`, `python3 platform/quality/audit_repository.py`, проверка
`docs/SENSITIVE-DATA.md` и Gitleaks (их же выполняет pre-push guard). Не
использовать `push --all`, `--mirror`, `--tags` или `--no-verify`. Поток
веток и PR — `docs/GIT.md`.

Секреты, записи голоса, backups и тяжёлые сборки остаются вне Git.
