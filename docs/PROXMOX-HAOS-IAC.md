# Следующий инфраструктурный слой: Proxmox, HAOS и IaC

Подготовка: 2026-10-27. **Proxmox пока не установлен.** Текущий работающий runtime —
[K3s на Ubuntu](KUBERNETES-OPERATIONS.md). Подготовлены ISO и свежая внешняя recovery-копия;
[порядок установки](INSTALL-PROXMOX.md). ОС/гипервизор ещё
не менялись; HAOS VM, Terraform resources и Tailscale Operator пока не созданы.

## Что даёт Proxmox

Proxmox VE — платформа виртуализации с API. Terraform/OpenTofu provider может
управлять VM, дисками, сетевыми адаптерами и другими объектами после подготовки
самого Proxmox. Provider `bpg/proxmox` — сторонний open-source проект, не продукт
Proxmox; его текущая документация ориентирована на PVE 9.x.
[Документация provider](https://github.com/bpg/terraform-provider-proxmox).

Terraform не означает, что HA configuration, модели, application state и
secret rotation автоматически описаны декларативно. Границы владения:

| Слой | Предлагаемый владелец |
|---|---|
| Bootstrap Proxmox на железе | Установщик/подготовленный unattended bootstrap, отдельный rollback и консоль |
| VM/templates/disks/NICs | Terraform/OpenTofu с pinned provider и защищённым state |
| Linux guest/K3s bootstrap | cloud-init/Ansible либо проверенный bootstrap script |
| Kubernetes workloads/observability | Helm charts; при управлении Helm через Terraform — import существующих releases |
| HAOS | VM image + поддерживаемое восстановление HA backup, конфигурация HA отдельно |
| Секреты/auth | Закрытое хранилище/Secret + writable refreshable state, не Git и не открытый tfstate |

Terraform state может содержать чувствительные данные. Remote backend, locking,
шифрование, резервирование и права доступа должны быть выбраны до production apply.
Нельзя просто объявить уже существующие пять Helm releases и выполнить apply:
сначала import в состояние и **проверка plan без нежелательного recreate/delete**.

## Целевая схема при выборе Proxmox

```text
неттоп / Proxmox VE
  HAOS VM                    Home Assistant + Supervisor
  Linux VM / K3s             MA/Matter/STT/TTS/Hermes + observability
```

Существующие Helm charts, dashboard JSON, pinned images и модели переиспользуются.
Перенос HA в HAOS потребует backup/restore и смены интеграционных адресов:
`127.0.0.1` в HAOS — сама VM, не соседняя K3s VM. Нужно заново проверить HA↔Wyoming,
Matter WebSocket, HA↔MA, Hermes↔HA MCP и обратные media URLs от станции/MA.
Нельзя считать смену HA runtime простой перестановкой image.

HAOS поддерживается в VM, включая KVM/Proxmox; сам Proxmox для HAOS не обязателен.
На существующей Ubuntu тоже можно сделать HAOS VM через KVM/libvirt.
[HA installation](https://www.home-assistant.io/installation),
[официальные VM инструкции](https://www.home-assistant.io/installation/alternative).

## Почему обсуждалась проводная сеть

Провод не является требованием для запуска VM. Сейчас Ethernet неттопа DOWN,
uplink — Wi-Fi с vendor DKMS driver. Типовая схема bridge, где VM — отдельный
L2 участник LAN, не должна предполагаться рабочей на Wi-Fi client-интерфейсе.
Это не запрещает VM: NAT/маршрутизация возможны, но discovery/multicast,
входящие соединения и публикуемые media URLs нужно проектировать и тестировать.
Выбор гипервизора не устраняет этот сетевой вопрос.

Proxmox bare-metal — смена хостовой ОС/ядра/сетевого управления, а не установка
обычного приложения рядом с Ubuntu. На одном физическом узле это отдельное
окно простоя с проверенной полной recovery-копией и доступом к boot/консоли.
Свободный VG объём не делает переустановку системного диска неразрушающей.

RAM неттопа — 16 GB (14.85 GiB доступны ОС). При двух VM необходимо заново
измерить бюджет Proxmox + guest OS + HAOS + K3s; нельзя выделить гостям все
16 GB и оставить гипервизор без памяти. Текущий успешно протестированный
bare-metal Kubernetes потреблял около 6–7 GiB вместе с хостом/observability.

## Tailscale Operator

Подходит к текущему K3s и к будущему K3s в VM: может декларативно публиковать
Kubernetes Services/Ingress в tailnet. Не заменяет host Tailscale для аварийного
доступа при падении кластера; текущий HA/Grafana Serve остаётся до приёмки нового входа.

Для установки нужны tag ownership и OAuth credentials с необходимыми scopes
либо заранее настроенная workload identity federation. В доступных локальных
credentials Tailscale API/OAuth credentials не обнаружены. Машинный вход
host tailscaled не заменяет управленческий OAuth client для operator.
[Operator prerequisites](https://tailscale.com/docs/kubernetes-operator/install-operator).

Следующие шаги после выбора архитектуры:

1. Выбран Proxmox: выполнить физическую установку по INSTALL-PROXMOX.md.
2. Зафиксировать VM network plan и доступ к восстановлению хоста.
3. Описать ресурсы в IaC и импортировать существующее управление без recreate.
4. Подготовить operator OAuth/tags с минимальными scopes, закрытый Secret и pinned chart.
5. Сначала опубликовать тестовый Service, проверить ACL/TLS, затем Grafana/HA.
6. Удалять старые Serve routes только после проверки нового доступа.
