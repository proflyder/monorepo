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
  - `traefik`: ingress-контроллер, wildcard Certificate и IngressRoute;
  - `cert-manager`: cert-manager и ACME solver Secrets;
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
`80/443` открыты для LAN/VPN и обслуживаются Traefik (IP allow-list на уровне
Middleware).
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

- LAN: `http://192.0.2.27:30300/` (корень; прежний `/grafana/` больше не
  используется после перехода `root_url` на отдельный хост)
- HTTPS (LAN/VPN): `https://grafana.home.example.com/`
- Tailscale: `https://pl-home.<tailnet>.ts.net/`; старый
  `/grafana/` после смены `root_url` больше не работает (ожидаемо) —
  использовать HTTPS-имя или LAN NodePort.
- Loki/Prometheus наружу не опубликованы.
- Port-forward для диагностики:

```bash
kubectl -n monitoring port-forward svc/monitoring-prometheus 19090:9090
kubectl -n monitoring port-forward svc/loki 13100:3100
```

Grafana admin password — `secrets/kubernetes-research/grafana-password`.

## HTTPS ingress

`projects/home/kubernetes/ingress/` — Traefik `42.2.0` + cert-manager
`v1.21.3` и wildcard-сертификат Let's Encrypt (`*.home.example.com`).

- Traefik работает в `hostNetwork` и слушает `80/443` на узле K3s, поэтому
  HA видит источником `192.0.2.27` и продолжает доверять
  `http.trusted_proxies: 192.0.2.27/32`.
- `80` редиректит на `443`; дашборд Traefik выключен, access log включён.
- `Middleware/home-allowlist` пропускает только LAN `192.0.2.0/24`, pod CIDR
  `10.42.0.0/16` и loopback. VPN-клиенты приходят NAT-ом с LAN-адреса.
- ClusterIssuer ACME DNS-01 через Cloudflare; wildcard Certificate,
  Secret `traefik/home-wildcard-tls`.
- Маршруты: `ha.home.example.com` → `monitoring/home-assistant-external:8123`,
  `grafana.home.example.com` → `monitoring/monitoring-grafana:80`,
  `alarms.home.example.com` → `home-assistant/scheduler:8098`,
  `control.home.example.com` → `home-assistant/ha-control:8099`.
- Все маршруты и сертификат — в namespace `traefik`; `allowCrossNamespace`
  разрешает Service из `monitoring`/`home-assistant`. Solver-Secret
  `cloudflare-api-token` живёт в namespace `cert-manager`, потому что
  `ClusterIssuer` резолвит его в cluster resource namespace.
- Реальные имена и `LAN_CIDR` — `secrets/home/ingress.env`; токен Cloudflare —
  приватный файл, в Git не попадает.

```bash
export KUBECONFIG=/private/path/kubeconfig
./projects/home/kubernetes/ingress/deploy.sh --staging   # проверка DNS-01
./projects/home/kubernetes/ingress/deploy.sh             # prod-сертификат
NODE_IP=192.0.2.27 ./projects/home/kubernetes/ingress/verify.sh
```

Диагностика: `kubectl -n cert-manager get pods,challenge,order`,
`kubectl -n traefik get certificate,secret,ingressroute`,
`kubectl -n traefik logs deploy/traefik`. Порты `80/443` на узле заняты
Traefik — не запускать там второй прокси без остановки release.

## Tailscale

- Operator `1.104.2`, release `home-access`.
- `/` → HAOS `.28:8123`; `/grafana/` больше не обслуживается после перехода
  Grafana на отдельный `root_url`.
- `pl-home` DNS → operator proxy `tag:k8s`.
- При смене HAOS IP обновить private `haosAddress` и применить chart.
- Публичного HA нет; Funnel/port-forward не используются.
- Tailscale остаётся как есть до отдельной приёмки и шага вывода; новые
  HTTPS-имена его не заменяют автоматически.

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
