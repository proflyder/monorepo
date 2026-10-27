# Домашний Kubernetes

Рабочая композиция: один K3s server/worker в Ubuntu VM `lab-k3s` на Proxmox,
собственный containerd, MA/Matter/GigaAM/Whisper/Piper/Hermes, observability
и Tailscale Operator. HA Core вынесен в отдельную HAOS VM. Pulsar и BLE
gateway остаются на станции. Версии зафиксированы в `versions.sh`.

- [Исследование и исходный план](../../../docs/PLAN-KUBERNETES-MIGRATION.md).
- [Эксплуатация и восстановление](../../../docs/KUBERNETES-OPERATIONS.md).
- [Proxmox/HAOS/IaC](../../../docs/PROXMOX-HAOS-IAC.md).
- [Chart приложений](../../home-assistant/deploy/README.md).
- [Дашборды и мониторинг](../../home-assistant/observability/README.md).
- [Tailscale Operator](tailscale/).

`/srv/k3s` находится на диске VM 200 GiB. K3s data-dir:
`/srv/k3s/runtime`. Новые observability PVC используют `local-retain`;
существующие данные приложений — static local PV с Retain. Это локальное
хранение, не репликация. VM `lab-k3s` использует Ethernet `192.0.2.27`;
HAOS — `192.0.2.28`; host Proxmox — `192.0.2.26`.

## Обновление

Нужны Helm repositories `prometheus-community`, `grafana-community`, `grafana`,
приватный kubeconfig и закрытые values. Kubernetes Secret `grafana-admin`
создаётся отдельно; значения не передаются через командную строку.

```bash
export KUBECONFIG=/private/path/kubeconfig
export PROMETHEUS_PRIVATE_VALUES=/private/path/prometheus.yaml
./projects/home/kubernetes/deploy-observability.sh
export RUNTIME_PRIVATE_VALUES=/private/path/runtime-values.yaml
./projects/home/kubernetes/apply-runtime.sh
```

Для первого переноса недостаточно выполнить helm upgrade с replicas=1:
старые writers должны быть остановлены, данные сохранены, volumes/images
подготовлены. Подробности и rollback — в runbook. Не выполнять параллельные
Helm операции над одним release.

Администрирование — по SSH или через закрытый tunnel. API/kubelet/node metrics
закрыты от обычного LAN; Grafana доступна через Tailscale Operator по
`/grafana/`, HAOS — по `/`. Host Tailscale на Proxmox пока не включён;
аварийный доступ к host — LAN/Ethernet. Реальные URL и credentials — в secrets.
