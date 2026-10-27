# Observability проекта

Dashboard JSON лежит в `chart/dashboards/`; `chart/templates/dashboards.yaml`
пакует каждый файл в ConfigMap с `grafana_dashboard: "1"` и папкой Home Assistant.
Release `ha-observability` устанавливается в namespace monitoring, Grafana sidecar
читает ConfigMaps только из monitoring и provision’ит JSON. Имена Pod не нужны.

В репозитории четыре дашборда: nettop, cluster, runtime voice endpoints и
infrastructure logs. Datasource UIDs: `prometheus`, `loki`; dashboard UIDs
стабильны, UI-изменения provisioned dashboard не сохраняются. Экспериментальную
копию можно экспортировать в JSON и принять обычным code review.

```bash
helm upgrade --install ha-observability projects/home-assistant/observability/chart \
  -n monitoring --wait --timeout 2m
```

Общие pinned values Prometheus/Grafana, Loki и Alloy — `platform/observability/values`
от корня репозитория. Composition/версии — `projects/home/kubernetes`.
Retention: Prometheus 7d/12GB при PVC 20Gi; Loki 7d при PVC 20Gi и включённом
Compactor. Это не жёсткая дисковая квота: следить за свободным местом.

Alloy собирает CRI-логи только kube-system/monitoring, исключая собственный Pod.
Тексты речи, запросы/ответы Hermes и приватные данные HA не собираются. Фильтр
секретных маркеров проверен синтетическим Pod через полную цепочку Alloy→Loki.
Расширение списка источников требует отдельной проверки содержимого, одной
redaction regex недостаточно. Внешний канал alerts ещё не настроен.

Приложения не обязаны иметь нативный `/metrics`: TCP availability собирается
health exporter’ом, host/container state — node-exporter/kubelet/kube-state-metrics.
Полная STT/TTS/LLM latency instrumentation — отдельная последующая задача.
