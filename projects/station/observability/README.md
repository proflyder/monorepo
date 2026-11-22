# Наблюдаемость колонки

Дашборды и алерты для Yandex Station Midi с Pulsar. Метрики отдаёт сам демон:
`http://<колонка>:9101/metrics` (Prometheus text), см.
[`projects/station/pulsar/README.md`](../pulsar/README.md#мониторинг-prometheus).

## Дашборды

JSON лежит в `chart/dashboards/`; `chart/templates/dashboards.yaml` пакует
каждый файл в ConfigMap с `grafana_dashboard: "1"` и аннотацией
`grafana_folder: "Колонка"`. Grafana-sidecar (release `monitoring`) читает
ConfigMaps из namespace `monitoring` и создаёт отдельную папку, не смешивая её
с «Home Assistant».

| Файл | UID | Содержимое |
|---|---|---|
| `station-hardware.json` | `station-hardware` | CPU, load, температура SoC, RAM, диск, uptime |
| `station-pulsar.json` | `station-pulsar` | Ассистент, звук и звонки, связь с HA, wake-word |

Datasource UID — `prometheus`; dashboard'ы провиженятся, правки в UI не
сохраняются — менять JSON в Git.

Release `station-observability` (namespace `monitoring`, chart из этого
каталога) ставит Flux из `develop`:
`projects/home/kubernetes/observability/manifests/helmrelease-station-dashboards.yaml`.

## Scrape и алерты

- Job `station` — `projects/home/kubernetes/observability/values/prometheus.yaml`,
  target `${STATION_IP}:9101` из `Secret/flux-app-settings`.
- PrometheusRule `home-station-alerts` —
  `projects/home/kubernetes/observability/rules/prometheusrule-station.yaml`,
  Telegram, severity `warning`:
  - `StationDown` — нет `up{job="station"}` 5 минут;
  - `StationCpuHot` — SoC > 85 °C 10 минут (запас до штатного троттлинга
    Amlogic около 90 °C);
  - `StationMemoryHigh` — available < 10% RAM 10 минут;
  - `StationDiskFilling` — свободно < 15% `/data` или прогноз заполнения за
    24 ч, 30 минут.
- Prometheus в `monitoring` не ограничен egress-NetworkPolicy, отдельных
  правил для внешнего таргета не нужно.

Контракт имён проверяется `projects/home/kubernetes/observability/test_prometheusrule.py`:
каждая метрика из правил и этих дашбордов должна быть в
`projects/station/pulsar/satellite/crates/pulsar-metrics/src/catalog.rs`
(`python3 -m unittest projects.home.kubernetes.observability.test_prometheusrule`).
