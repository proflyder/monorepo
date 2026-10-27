# Домашний Kubernetes

Рабочая композиция: один K3s server/worker на существующей Ubuntu, собственный
containerd, HA/MA/Matter/GigaAM/Whisper/Piper/Hermes и голосовой proxy.
Pulsar и BLE gateway остаются на станции. Версии зафиксированы в `versions.env`.

- [Исследование и исходный план](../../../docs/PLAN-KUBERNETES-MIGRATION.md).
- [Эксплуатация и восстановление](../../../docs/KUBERNETES-OPERATIONS.md).
- [Chart приложений](../../home-assistant/deploy/README.md).
- [Дашборды и мониторинг](../../home-assistant/observability/README.md).

`/srv/k3s` — отдельный LV 150 GiB. K3s data-dir: `/srv/k3s/runtime`.
Новые observability PVC используют `local-retain`; существующие данные
приложений — static local PV с Retain. Это локальное хранение, не репликация.

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
закрыты от обычного LAN; Grafana доступна в существующем Tailscale Serve
по `/grafana/`, HA остаётся по `/`. Реальные URL и credentials — в secrets.
