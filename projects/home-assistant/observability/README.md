# Observability проекта

Dashboard JSON лежит в `chart/dashboards/`; `chart/templates/dashboards.yaml`
пакует каждый файл в ConfigMap с `grafana_dashboard: "1"` и папкой Home Assistant.
LAN UI: `http://192.0.2.27:30300/` (NodePort `grafana-lan`); HTTPS
`https://grafana.home.example.com/` (LAN/VPN, Traefik).

Release `ha-observability` устанавливается в namespace monitoring, Grafana sidecar
читает ConfigMaps только из monitoring и provision’ит JSON. Имена Pod не нужны.

В репозитории пять дашбордов: K3s VM (файл `nettop.json` — историческое
имя), cluster, runtime voice endpoints, infrastructure logs и currency bot.
`currency-bot.json` показывает `currency_bot_rate` (USD/RUB → KZT, buy/sell),
job runs/failures, parse duration, Telegram send failures, HTTP requests по
route/status, а Loki-панели — объём логов по `level` и ERROR-строки
`{namespace="currency-bot"}`. Диапазон по умолчанию — `now-7d` под retention
Loki. Datasource UIDs: `prometheus`, `loki`; dashboard UIDs
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

Alloy собирает CRI-логи kube-system/monitoring и `currency-bot`, исключая
собственный Pod. Redaction вырезает Bearer/`token=`/`password=`/`api_key=`
маркеры и Telegram bot token (`<digits>:<35 chars>`); контракт проверяется
`projects/home-assistant/deploy/test_charts.py::AlloyPrivacy`.
Тексты речи, запросы/ответы Hermes и приватные данные HA не собираются. Фильтр
секретных маркеров проверен синтетическим Pod через полную цепочку Alloy→Loki.
Расширение списка источников требует отдельной проверки содержимого, одной
redaction regex недостаточно. Внешний канал alerts ещё не настроен.

Приложения не обязаны иметь нативный `/metrics`: TCP availability собирается
health exporter’ом, host/container state — node-exporter/kubelet/kube-state-metrics.
Полная STT/TTS/LLM latency instrumentation — отдельная последующая задача.
