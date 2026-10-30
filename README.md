# Yandex Station Midi — home lab

Стенд: Proxmox + HAOS VM + Ubuntu/K3s VM; станция Midi с Pulsar.
Публичные адреса — примеры; реальные — в `secrets/`.

## Старт

- [Текущее состояние](docs/STATE.md)
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
solutions/              переиспользуемые scheduler/alarm/bridge
packages/               proto, общий Go-код
docs/                   эксплуатация и политики
secrets/                реальные адреса, доступы, recovery (вне Git)
```

## Доступ

```bash
adb connect 192.0.2.13:5555
ssh root@192.0.2.13          # пароль: secrets/CREDENTIALS.md
ssh root@192.0.2.26          # PVE
ssh labuser@192.0.2.27       # K3s VM
```

HA `192.0.2.28:8123`, MA `192.0.2.28:8095`, Grafana
`192.0.2.27:30300/grafana/`. Только LAN/VPN; в интернет не публиковать.

## Статус

- PVE `9.2.24`; HAOS `2026.10.4`; K3s `v1.35.9`.
- Wi-Fi 4addr/WDS + Ethernet fallback; VM IP не меняются.
- Pulsar Rust `v0.5.0-1-g908a027`, wake word «Рикардо», BLE gateway.
- HAOS add-ons: MA `2.10.8`, Matter `9.2.0`; K3s: STT/TTS/Hermes/observability.
- Remote — собственный VPN (ocserv) и HTTPS-имена, без публичного HA.

## Тесты

```bash
make test          # hermetic
make coverage      # coverage (reports/TEST-COVERAGE.md, not tracked)
make test-e2e      # live HA/BLE, нужны токены
```

Подробности — `docs/TESTING.md`.

## Публикация

Публикуется только `develop`, без секретов и Git-истории с приватными данными.
Перед push: `make test`, `python3 platform/quality/audit_repository.py`,
проверка `docs/SENSITIVE-DATA.md` и Gitleaks. Не использовать `push --all`,
`--mirror`, `--tags` или `--no-verify`.

Секреты, записи голоса, backups и тяжёлые сборки остаются вне Git.
