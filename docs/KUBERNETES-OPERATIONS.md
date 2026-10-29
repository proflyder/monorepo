# Kubernetes: K3s VM

Один узел `lab-k3s` в VM 101. Публичные адреса — примеры; реальные —
`secrets/NETWORK.md`.

## Состав

- Ubuntu 24.04.5, 8 GiB RAM, 200 GiB, `192.0.2.27`, node `lab-host`.
- K3s `v1.35.9+k3s1`, data-dir `/srv/k3s/runtime`, SQLite.
- Namespaces:
  - `home-assistant`: GigaAM, Whisper, Piper, Hermes gateway+proxy,
    health exporter;
  - `monitoring`: kube-prometheus-stack, Loki, Alloy, Grafana;
  - `tailscale`: Operator и ingress proxy.
- HAOS и add-ons MA/Matter — отдельная VM `.28`; в K3s их больше нет.
- App PV — local-path, `Retain`; старые PV не удалять без проверки.

## Доступ

Внутри VM:

```bash
sudo k3s kubectl get pods -A
sudo k3s kubectl get pvc -A
sudo k3s kubectl top nodes
sudo systemctl status k3s
```

С Mac:

```bash
ssh -N -i secrets/proxmox-install/proxmox_admin \
  -o UserKnownHostsFile=secrets/proxmox-install/guest-known_hosts \
  -L 127.0.0.1:16443:127.0.0.1:6443 labuser@192.0.2.27
export KUBECONFIG=secrets/kubernetes-research/kubeconfig
kubectl get pods -A
```

Ключ и `known_hosts` — `secrets/proxmox-install/` (см. `CREDENTIALS.md`).

API `6443`, kubelet `10250`, node-exporter `9100`, health `9116` и
VXLAN `8472/udp` закрыты firewall от обычного LAN.
Firewall: `projects/home/kubernetes/management-firewall/`.

## Данные

- `/srv/k3s/runtime` — K3s data, containerd, datastore.
- `/srv/k3s/volumes/hermes` — Hermes home, в Pod `/opt/data`.
- `/srv/k3s/recovery` — закрытые recovery-файлы.
- `runtime-ha` — архив старого HA Container, не активный writer.
- `runtime-music`/`runtime-matter` удалены; tar-копии на Mac.

Секреты Hermes — `Secret/hermes-environment`; Grafana — `Secret/grafana-admin`.
Значения — только в приватных values/`secrets/`.

## Обновление

```bash
# chart приложений и dashboard — из репозитория
helm upgrade --install runtime projects/home-assistant/deploy/chart \
  -n home-assistant -f secrets/kubernetes-research/runtime-values.yaml \
  --wait --timeout 5m

helm upgrade --install ha-observability \
  projects/home-assistant/observability/chart \
  -n monitoring --wait --timeout 2m
```

Перед upgrade: backup, сравнить versions/image digests. Не менять модели,
API и storage schema одновременно. Один Deployment = один writer.
Helm rollback не откатывает SQLite/auth/state.

Dashboard JSON: `projects/home-assistant/observability/chart/dashboards/`;
доставка через ConfigMap → Grafana sidecar. Ручной import не нужен.

## Grafana / observability

- LAN: `http://192.0.2.27:30300/grafana/`
- Tailscale: `https://pl-home.<tailnet>.ts.net/grafana/`
- Loki/Prometheus наружу не опубликованы.
- Port-forward для диагностики:

```bash
kubectl -n monitoring port-forward svc/monitoring-prometheus 19090:9090
kubectl -n monitoring port-forward svc/loki 13100:3100
```

Grafana admin password — `secrets/kubernetes-research/grafana-password`.

## Tailscale

- Operator `1.104.2`, release `home-access`.
- `/` → HAOS `.28:8123`; `/grafana/` → Grafana.
- `pl-home` DNS → operator proxy `tag:k8s`.
- При смене HAOS IP обновить private `haosAddress` и применить chart.
- Публичного HA нет; Funnel/port-forward не используются.

## Backup / restore

- Сохранять: cluster SQLite snapshot, token, TLS/cred, app PVC, custom images.
- SQLite snapshots — через backup API, проверять `integrity_check`.
- Без server token восстановление datastore невозможно.
- Custom images Hermes/proxy — recovery OCI archive; `imagePullPolicy: Never`.
- Полный restore:
  1. восстановить Ubuntu/K3s;
  2. остановить K3s;
  3. вернуть data-dir/token/ключи;
  4. вернуть PVC до запуска Pods;
  5. импортировать images;
  6. проверить endpoints, Tailscale, voice/music.

Disaster restore на запасном железе ещё не выполнялся.

## Ограничения

- Один узел, без HA control plane и распределённого storage.
- Нет внешнего Alertmanager/heartbeat.
- Нет полной latency instrumentation voice pipeline.
- Свои images пока из recovery archive, без registry/build pipeline.
