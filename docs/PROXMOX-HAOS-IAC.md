# Следующий инфраструктурный слой: Proxmox, HAOS и IaC

Обновление: 2026-10-28. **Proxmox, HAOS VM, K3s VM и Tailscale Operator
развёрнуты; cutover принят для read-only E2E.** SSH, Ethernet bridge,
storage, Terraform state и обе VM проверены. Предыдущая Ubuntu/K3s
сохранена в recovery archives на Mac, но на неттопе удалена установщиком.
[Порядок установки и восстановления](INSTALL-PROXMOX.md), текущие факты — STATE.md.

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

## Сеть: текущее состояние и Wi-Fi

Сейчас Proxmox host использует Ethernet `vmbr0`; Wi-Fi RTL8821CE на host
`DOWN`. PCI mapping `home-wifi` создан, но карта **не передана** ни одной VM:
client Wi-Fi нельзя автоматически считать эквивалентом Ethernet bridge.
Типовая схема bridge, где VM — отдельный L2 участник LAN, на Wi-Fi
client-интерфейсе не гарантируется. Это не запрещает VM: NAT/маршрутизация
возможны, но discovery/multicast, входящие соединения и media URLs нужно
проектировать и тестировать. Выбор гипервизора не устраняет этот вопрос.

До отдельного routed/NAT-проекта или внешнего Wi-Fi bridge Ethernet остаётся
обязательным. Не отключать кабель и не передавать Wi-Fi PCI VM без плана
возврата host-доступа. HAOS/K3s используют LAN `.28/.27`; HA-совместимый
endpoint `.27:8123` обслуживает HAProxy, а не host loopback.

Proxmox bare-metal — смена хостовой ОС/ядра/сетевого управления, а не установка
обычного приложения рядом с Ubuntu. На одном физическом узле это отдельное
окно простоя с проверенной полной recovery-копией и доступом к boot/консоли.
Свободный VG объём не делает переустановку системного диска неразрушающей.

RAM неттопа — 16 GB (14.85 GiB доступны ОС). Текущее распределение: HAOS
3 GiB, K3s 9 GiB, остальное Proxmox host; после cutover занято около 13 GiB,
доступно около 1.9 GiB без swap. Не увеличивать гостей без измерения;
power-loss/reboot-тест ещё впереди.

## Tailscale Operator

Operator `1.104.2` развёрнут в K3s и публикует `home` Ingress:
`pl-home` → HAOS, `/grafana` → Grafana. Старый host Serve route заменён;
OAuth credentials и private values находятся вне Git. Operator не заменяет
host Tailscale для аварийного доступа при падении кластера: на Proxmox host
`tailscaled` сейчас не запущен, поэтому его доступ только LAN/Ethernet.

Для повторной установки нужны tag ownership и OAuth credentials с
необходимыми scopes либо workload identity federation. Машинный вход
host tailscaled не заменяет управленческий OAuth client.
[Operator prerequisites](https://tailscale.com/docs/kubernetes-operator/install-operator).

Сделано:

1. ✅ Proxmox установлен, SSH/bridge/storage проверены.
2. ✅ Terraform definitions и state; обе VM созданы, plan — no changes.
3. ✅ HAOS восстановлен; K3s VM с приложениями и observability.
4. ✅ Operator OAuth/tags, pinned chart, Secret; тестовые Service/Ingress.
5. ✅ `pl-home` и `/grafana` проверены через operator proxy.
6. ✅ Старый host Serve route удалён из DNS-маршрута.

Осталось: Wi-Fi/routed-проект, DHCP/static reservations, host Tailscale
emergency access, power-loss/reboot тест и offsite backup.
