# Документация стенда

Только актуальные эксплуатационные документы. История, ресерчи и планы
удалены; они остаются в Git-истории. Секреты — только в `secrets/`.

## Старт

| Документ | Содержимое |
|---|---|
| [`STATE.md`](STATE.md) | текущие факты: host, VM, HA, K3s, станция |
| [`OPERATIONS.md`](OPERATIONS.md) | ежедневные команды и откаты |
| [`NETWORK.md`](NETWORK.md) | публичная схема LAN/портов; реальные адреса — `secrets/NETWORK.md` |
| [`PENDING.md`](PENDING.md) | только открытые задачи |

## Эксплуатация

| Документ | Содержимое |
|---|---|
| [`KUBERNETES-OPERATIONS.md`](KUBERNETES-OPERATIONS.md) | K3s, Helm, Grafana, volumes |
| [`INSTALL-PROXMOX.md`](INSTALL-PROXMOX.md) | PVE, VM, Terraform, Wi-Fi uplink |
| [`REBUILD.md`](REBUILD.md) | восстановление после потери host/VM/станции |
| [`SNAPSHOTS.md`](SNAPSHOTS.md) | снапшоты и приватные копии |
| [`TESTING.md`](TESTING.md) | hermetic и live-проверки |

## Политики и дизайн

| Документ | Содержимое |
|---|---|
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | компоненты и потоки |
| [`DECISIONS.md`](DECISIONS.md) | ключевые решения |
| [`SECURITY.md`](SECURITY.md) | сеть, секреты, обновления |
| [`SENSITIVE-DATA.md`](SENSITIVE-DATA.md) | реестр приватных данных без значений |
| [`GRPC.md`](GRPC.md) | gRPC-контракт |
| [`MONOREPO.md`](MONOREPO.md) | структура репозитория |

## Станция

| Документ | Содержимое |
|---|---|
| [`PROVISIONING.md`](PROVISIONING.md) | BLE/DPP/AP/sound provisioning |
| [`OTA-FORTRESS.md`](OTA-FORTRESS.md) | OTA/ADB-защита |
| [`WAKE-WORD.md`](WAKE-WORD.md) | сборка и установка «Рикардо» |

## Правила

1. Факты — `STATE.md`; задачи — `PENDING.md`; команды — `OPERATIONS.md`.
2. Не добавлять историю, дневники, ресерчи и планы в `docs/`.
3. Не дублировать состояние между документами: ссылаться на владельца факта.
4. В публичных документах — адреса-примеры `192.0.2.x`; реальные — в `secrets/`.
5. Перед публикацией: `make test` + проверка `SENSITIVE-DATA.md`.
