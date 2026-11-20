# Домашний Kubernetes

Рабочая композиция: один K3s server/worker в Ubuntu VM `lab-k3s` на Proxmox,
собственный containerd, GigaAM/Whisper/Piper/Hermes, observability и
HTTPS ingress. HA Core, **Music Assistant** и **Matter Server** живут
в HAOS VM как add-ons. Pulsar и
BLE gateway остаются на станции. Версии зафиксированы в `versions.sh`.

- [Эксплуатация и восстановление](../../../docs/KUBERNETES-OPERATIONS.md).
- [Текущее состояние](../../../docs/STATE.md).
- [Сеть и порты](../../../docs/NETWORK.md).
- [Chart приложений](../../home-assistant/deploy/README.md).
- [Дашборды и мониторинг](../../home-assistant/observability/README.md).
- [HTTPS ingress Traefik + cert-manager](ingress/).
- [Nexus — кэш артефактов для CI](nexus/).
- [Flux — pull-based GitOps для платформы, observability, Vaultwarden, Nexus и всех деплой-юнитов приложений](flux/).
- [OpenBao](openbao/).
- [External Secrets Operator](external-secrets/).
- [Hermes voice-proxy firewall](voice-firewall/).
- Intercom signaling (WebRTC call) — `intercom/`, деплой merge'ем в `develop`.

`/srv/k3s` находится на диске VM 200 GiB. K3s data-dir:
`/srv/k3s/runtime`. Новые observability PVC используют `local-retain`;
существующие данные приложений — static local PV с Retain. Это локальное
хранение, не репликация. VM `lab-k3s` — 12 GiB, `192.0.2.27`;
HAOS — 4 GiB, `192.0.2.28`; host Proxmox — `192.0.2.26`.
MA/Matter add-ons: `core_matter_server 9.2.0`, `d5369777_music_assistant 2.10.8`.

## Обновление

Traefik, cert-manager, ESO, observability и Vaultwarden обновляются merge'ем
в `develop` (Flux). Chart приложений (`runtime`) катит Flux, а его storage — вручную
(`apply-runtime.sh`):

```bash
export RUNTIME_PRIVATE_VALUES=/private/path/runtime-values.yaml
./projects/home/kubernetes/apply-runtime.sh
```

Изменения HelmRelease и values — в Git; вручную Helm-релизы не трогать.
Не выполнять параллельные Helm операции над одним release; rollback — в
runbook (`docs/KUBERNETES-OPERATIONS.md`).

Администрирование — по SSH или через закрытый tunnel. API/kubelet/node metrics
закрыты от обычного LAN; Grafana доступна по `https://grafana.home.example.com/`,
в LAN по `http://192.0.2.27:30300/`. HTTPS-имена `*.home.example.com`
обслуживает Traefik (`hostNetwork`, `80/443`, IP allow-list) с
wildcard-сертификатом Let's Encrypt от cert-manager; см. [ingress/](ingress/).
Вход снаружи — собственный VPN (ocserv, CT `102`); CI ходит группой `ci`
только до узла K3s по `tcp/443` (Traefik → HA/OpenBao). Аварийный доступ к
host — LAN/Wi-Fi или Ethernet. Реальные URL и credentials — в secrets.

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `apply-runtime.sh` — Apply the full runtime state from the private values file.
- `cloudflare_dns.py` — Ensure a DNS-only Cloudflare A record for a private home host.
- `observability/kustomization.yaml` — Observability stack reconciled by Flux.
- `observability/manifests/external-secret-alertmanager-telegram.yaml` — Alertmanager configuration with the Telegram receiver.
- `observability/manifests/external-secret-grafana-admin.yaml` — Grafana administrator credentials.
- `observability/manifests/grafana-lan.yaml` — Read-only operational Grafana UI for the home LAN. Grafana remains a ClusterIP Service behind Traefik; this is a separate NodePort Service so the monitoring Helm release can be upgraded independently.
- `observability/manifests/helmrelease-dashboards.yaml` — Project-owned dashboards and alert rules, built from the chart in this repository. Same release name/namespace as the live `ha-observability` release; the chart comes from the Flux GitRepository instead of a local path.
  - The chart lives in this repository and keeps version 0.1.0; with the default ChartVersion strategy template changes (dashboards) never reached the release. Revision upgrades on every new Git revision.
- `observability/manifests/helmrelease-prometheus.yaml` — kube-prometheus-stack, adopted from the live `monitoring` release.
- `observability/manifests/helmrelease-station-dashboards.yaml` — Station dashboards («Колонка») from the chart in this repository.
- `observability/rules/kustomization.yaml` — Backup freshness, Alertmanager delivery and Flux delivery rules, plus the Flux controller PodMonitor. Split out because PrometheusRule/PodMonitor need the Prometheus Operator CRDs that the `monitoring` release installs.
- `observability/rules/podmonitor-flux.yaml` — Scrape the three Flux controllers for `gotk_*` metrics.
- `observability/rules/prometheusrule-alertmanager.yaml` — Alertmanager delivery alerts.
- `observability/rules/prometheusrule-backups.yaml` — Backup freshness alerts.
- `observability/rules/prometheusrule-flux.yaml` — GitOps delivery alerts for the Flux-managed applications.
  - Suspended Kustomizations are skipped on purpose (drills, rollback); `suspended` is absent when the manifest never set `spec.suspend`.
- `observability/rules/prometheusrule-station.yaml` — Alerts for the station (Yandex Station Midi running Pulsar).
- `observability/test_prometheusrule.py` — Structural sanity checks for the backup, Alertmanager and Flux PrometheusRules.
  - Flux 2.x controllers only export gotk_reconcile_duration_seconds_* and gotk_token_*; the old reconcile-condition expression could never fire, and its absent() branch alerts forever.
  - A healthy steady state is an empty result: the ConfigMap exists, the pins source is Ready, and its series exists. Each failure mode below is one branch, so it must not degrade into a permanent alert.
  - A selector on a label kube-state-metrics never emits silently disables the alert (or makes absent() fire forever), so cross-check every matcher against the customResourceState config.
- `observability/values/prometheus.yaml` — The default delivery-rate alerts cannot separate the heartbeat webhook from real integrations. They are redeclared with an `integration!="webhook"` filter in rules/prometheusrule-alertmanager.yaml, together with HeartbeatPingFailing for persistent ping failures.
  - The station (Yandex Station Midi running Pulsar) is an external target on the LAN: Pulsar serves Prometheus text on 9101. The address comes from flux-app-settings (`STATION_IP`), never from Git.
