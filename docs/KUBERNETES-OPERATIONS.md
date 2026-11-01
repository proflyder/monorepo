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
  - `currency-bot`: Telegram-бот валют, SQLite на `local-retain` и
    `cloudflared` для webhook, см. `projects/home/kubernetes/currency-bot/`;
  - `traefik`: ingress-контроллер, wildcard Certificate и IngressRoute;
  - `cert-manager`: cert-manager и ACME solver Secrets;
  - `openbao`: OpenBao (raft, UI, auth), см. `projects/home/kubernetes/openbao/`;
  - `external-secrets`: External Secrets Operator, читает OpenBao и создаёт
    Secrets приложений.
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
- `/srv/k3s/volumes/runtime-hermes` — Hermes home, в Pod `/opt/data`.
- `/srv/k3s/volumes/openbao{, -backups}` — OpenBao raft и raft snapshots
  (PV `openbao-data`/`openbao-backups`, `ricardo-existing`, `Retain`).
- `/srv/k3s/recovery` — закрытые recovery-файлы.
- Host-дерева `/opt/ricardo` и запасных релизов нет: откат — только
  digest-pinned образы в GHCR/containerd, деплой — `converge.py`.
- `runtime-ha` — архив старого HA Container, не активный writer.
- `runtime-music`/`runtime-matter` удалены; tar-копии на Mac.

Секреты Hermes (`Secret/hermes-environment`), scheduler/HA
(`scheduler-auth`, `ha-bridge-ha`), currency-bot (`currency-bot`,
`cloudflared-tunnel`), Grafana (`grafana-admin`) и Cloudflare
(`cloudflare-api-token`) создаются External Secrets Operator из OpenBao
`secret/k8s/<namespace>/<name>`. Источник правды — OpenBao; локальные файлы в
`secrets/` — только бэкап/bootstrap.

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

### Проверка: приватные values совпадают с live

Полный `apply-runtime.sh` меняет все объекты чарта, поэтому приватный
values-файл (`RUNTIME_PRIVATE_VALUES`, лежит в `secrets/`) должен давать
рендер, совпадающий с живым релизом. Единственная надёжная проверка —
сравнить рендер со всеми объектами (и storage, который применяется отдельно):

```bash
export KUBECONFIG=secrets/kubernetes-research/kubeconfig

helm template runtime projects/home-assistant/deploy/chart \
  -n home-assistant -f "$RUNTIME_PRIVATE_VALUES" \
  | kubectl diff -n home-assistant -f -
```

Пустой вывод и код 0 означают совпадение. `helm get values runtime` после
разделения storage/workloads намеренно **не содержит** `storage`, поэтому
сравнивать его с приватным файлом напрямую нельзя; ориентир — рендер.
Непустой diff означает, что приватный файл разошёлся с live: сначала обновить
его (бэкап рядом, режим 600) и добиться пустого diff, и только затем делать
`apply-runtime.sh`. `helm upgrade --dry-run=server` проверяет только схему и
доступ к API, поэтому недостаточен как доказательство отсутствия изменений.

### Два владельца: storage и workloads

`apply-runtime.sh` применяет один и тот же приватный файл двумя путями:

- `PersistentVolume`/`PersistentVolumeClaim` — `kubectl apply` от администратора
  (cluster-scoped и `Retain`); они не входят в Helm-релиз;
- остальные объекты — `helm upgrade --install runtime` (namespace-scoped).

Это разделение обязательно: CI-SA `ci-deploy` не имеет cluster-scoped прав, а
Helm при удалении объекта из релиза делает GET и без `get persistentvolumes`
падает. PV/PVC помечены `helm.sh/resource-policy: keep`, поэтому переход
безопасен и для релиза, который ещё помнит storage. Полный ручной apply —
единственный путь менять storage; CI его не трогает. На чистой установке
сначала `apply-runtime.sh` (создаёт PV/PVC), затем CI может катить workloads.

## Автодеплой Hermes / voice-proxy

`deploy-home` для юнита `hermes` (builder `docker`) после build+push вызывает
`projects/home-assistant/deploy/hermes_rollout.py`:

1. значения берутся из OpenBao `secret/ci/deploy-home` (key `runtime_values`,
   исходный текст без base64; токенов в нём нет — они в Kubernetes Secrets),
   job материализует их в файл 600 после OIDC-входа;
2. если digest'ы не изменились, upgrade не запускается;
3. `helm upgrade runtime` получает только `--set` образов (без
   `--reuse-values`), `storage` вырезается;
4. запоминается ревизия до upgrade, затем проверяются rollout, health exporter
   (все endpoint'ы = 1), Assist-запрос через `conversation.hermes_agent` и
   отсутствие ошибок в логах voice-proxy. HA-токен читается Job'ом из Secret
   `ha-bridge-ha`, HA вызывается по cluster-internal Service, поэтому в GitHub
   он не попадает;
5. при провале — `helm rollback runtime <прежняя ревизия>`, повторная проверка;
   если upgrade не создал ревизию, откат не выполняется.

Ручной прогон того же пути:

```bash
KUBECONFIG=secrets/kubernetes-research/kubeconfig \
  python3 projects/home-assistant/deploy/hermes_rollout.py \
    --values-file secrets/kubernetes-research/runtime-values.yaml \
    --hermes-digest sha256:… --voice-proxy-digest sha256:… \
    --state-dir /tmp/hermes-state
```

`--force` прогоняет upgrade при совпадающих digest'ах (no-op проверка);
`--simulate-smoke-failure` проверяет откат. Ручной `helm rollback runtime N`
остаётся аварийным путём.

Dashboard JSON: `projects/home-assistant/observability/chart/dashboards/`;
доставка через ConfigMap → Grafana sidecar. Ручной import не нужен.

## MCP gateway (инфраструктурный шлюз)

`solutions/mcp-gateway` в namespace `home-assistant`: ClusterIP `:8080`, один
Deployment, `ServiceMonitor`. Hermes ходит в `/mcp` с Bearer-токеном
(`HERMES_INFRA_TOKEN`); сам Hermes доступа к Kubernetes API не имеет
(`automountServiceAccountToken: false`) — весь доступ к кластеру идёт через
шлюз. Инструкция по каталогу и RBAC —
`projects/home/kubernetes/mcp-gateway/README.md`.

Одноразово (admin, до первого деплоя):

```bash
export KUBECONFIG=/private/path/kubeconfig
# токен шлюза↔Hermes; значение никогда не печатать
bao kv put secret/k8s/home-assistant/mcp-gateway-auth token="$(openssl rand -hex 32)"
kubectl apply -f projects/home/kubernetes/mcp-gateway/manifests/rbac.yaml
kubectl apply -f projects/home/kubernetes/mcp-gateway/ci/rbac.yaml
kubectl apply -f projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml
kubectl -n home-assistant wait --for=condition=Ready \
  externalsecret/mcp-gateway-auth externalsecret/mcp-gateway-telegram --timeout=120s
```

`ci/rbac.yaml` добавляет `home-assistant/ci-deploy` только NetworkPolicy и
ServiceMonitor; остальные глаголы деплоя уже есть в scheduler-Role.

Деплой и проверки:

```bash
NODE_SSH=labuser@192.0.2.27 ./projects/home/kubernetes/mcp-gateway/deploy.sh

kubectl -n home-assistant get deploy,pod,svc -l app.kubernetes.io/name=mcp-gateway
kubectl -n home-assistant logs deploy/mcp-gateway --tail=50
kubectl -n home-assistant get --raw \
  "/api/v1/namespaces/home-assistant/services/http:mcp-gateway:8080/proxy/readyz"
kubectl -n home-assistant exec deploy/mcp-gateway -- \
  /usr/local/bin/mcp-gateway -healthcheck
```

Проверка прав ровно по каталогу:

```bash
kubectl auth can-i --as=system:serviceaccount:home-assistant:mcp-gateway \
  patch deployment/scheduler -n home-assistant      # yes
kubectl auth can-i --as=system:serviceaccount:home-assistant:mcp-gateway \
  delete deployment/scheduler -n home-assistant     # no
kubectl auth can-i --as=system:serviceaccount:home-assistant:mcp-gateway \
  patch statefulset/openbao -n openbao              # yes
kubectl auth can-i --as=system:serviceaccount:home-assistant:mcp-gateway \
  patch deployment/traefik -n monitoring            # no
```

Перезапуск выполняет только шлюз, всегда два шага (`infra_prepare_restart` →
`infra_confirm`, TTL 90 с, cooldown 5 мин на workload). Ручной эквивалент для
диагностики — обычный rollout restart, но он обходит предупреждения:

```bash
kubectl -n home-assistant rollout restart deployment/piper
kubectl -n home-assistant annotate deployment/piper mcp-gateway/last-restart- --overwrite
```

Ротация токена: обновить OpenBao `k8s/home-assistant/mcp-gateway-auth`, затем
перезапустить `mcp-gateway` и `hermes` (иначе Hermes продолжит слать старый
Bearer). Ротация Telegram-бота: обновить `TELEGRAM_BOT_TOKEN` /
`TELEGRAM_ALLOWED_USERS` в `k8s/home-assistant/hermes-environment` и
дождаться синхронизации обоих ExternalSecret (force-sync аннотацией).

## Grafana / observability

- LAN: `http://192.0.2.27:30300/` (корень; прежний `/grafana/` больше не
  используется после перехода `root_url` на отдельный хост)
- HTTPS (LAN/VPN): `https://grafana.home.example.com/`
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
  `control.home.example.com` → `home-assistant/ha-control:8099`,
  `currency.home.example.com` → `currency-bot/currency-bot:8080`.
- Все маршруты и сертификат — в namespace `traefik`; `allowCrossNamespace`
  разрешает Service из `monitoring`/`home-assistant`/`currency-bot`. Solver-Secret
  `cloudflare-api-token` живёт в namespace `cert-manager`, потому что
  `ClusterIssuer` резолвит его в cluster resource namespace.
- Реальные имена и `LAN_CIDR` — `secrets/home/ingress.env`; токен Cloudflare
  хранится в OpenBao (`secret/k8s/cert-manager/cloudflare-api-token`) и
  попадает в Secret через ESO. `--bootstrap-secrets` — только для первой
  установки без OpenBao/ESO.

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

## OpenBao

`openbao` namespace: StatefulSet `openbao-0`, raft на PV, декларативный
audit в stdout (Loki: `{namespace="openbao"}`), UI/API `https://vault.home.example.com/` через Traefik.
Auto-unseal — `static` seal (Secret `openbao-seal`), поэтому после рестарта
пода распечатывание не требуется.

```bash
kubectl -n openbao get pod,pvc,cronjob
kubectl -n openbao exec openbao-0 -- /bin/sh -ec 'BAO_ADDR=http://127.0.0.1:8200 bao status'
kubectl -n openbao delete pod openbao-0   # OnDelete: применяет новый конфиг
```

Обновление/бутстрап/backup/восстановление — `projects/home/kubernetes/openbao/README.md`.

## External Secrets Operator

ESO `2.14.0` в namespace `external-secrets` читает KV v2 `secret/` и создаёт
Secrets приложений. `ClusterSecretStore/openbao` использует in-cluster Service
`http://openbao.openbao.svc:8200`, Kubernetes auth и роль `external-secrets`
(политика `k8s-read`, только `secret/data/k8s/*`).

```bash
export KUBECONFIG=/private/path/kubeconfig
./projects/home/kubernetes/external-secrets/deploy.sh
kubectl get clustersecretstore openbao
kubectl get externalsecret -A
kubectl -n home-assistant annotate externalsecret scheduler-auth \
  force-sync="$(date +%s)" --overwrite   # немедленная синхронизация
```

Манифесты — рядом с проектами; таблица путей, миграция и ротация —
`projects/home/kubernetes/external-secrets/README.md`. `deploy-home`
проверяет `Ready` ExternalSecret перед выкаткой и не пишет их значения.

## Удалённый доступ

- Вход снаружи — только собственный VPN (ocserv, CT `102`,
  `vpn.example.com`), см. `projects/home/proxmox/vpn/`.
- Внутри LAN/VPN — HTTPS-имена `*.home.example.com` из раздела выше.
- HAOS в K3s подключён через Service `monitoring/home-assistant-external`
  и EndpointSlice с адресом HAOS; при смене IP обновить `HAOS_ADDRESS`
  в приватном env и применить `ingress/manifests/backends.yaml`.
- Публичного HA нет; port-forward на роутере только для VPN.

## Backup / restore

- Сохранять: cluster SQLite snapshot, token, TLS/cred, app PVC, custom images.
- SQLite snapshots — через backup API, проверять `integrity_check`.
- Без server token восстановление datastore невозможно.
- Образы scheduler/ha-bridge/ha-control/currency-bot собираются Bazel
  (`rules_oci`, distroless, digest-pinned base) и адресуются по digest; в CI
  они пушатся в GHCR, на живой узел импортируются в containerd
  (`ctr images import`).
- currency-bot хранит историю в SQLite на PVC `currency-bot-data` (1Gi,
  `local-retain`); H2→SQLite перенос и импорт — в
  `projects/home/kubernetes/currency-bot/README.md`.
- Hermes и voice-proxy — публичные digest-pinned образы GHCR
  (`ghcr.io/proflyder/{hermes,voice-proxy}@sha256:…`), `imagePullPolicy:
  IfNotPresent`, без pull-secret. deploy-home собирает и пушит их, затем
  `hermes_rollout.py` подставляет digest'и в релиз `runtime`, прогоняет
  голосовую проверку и откатывается на предыдущую ревизию при провале.
  Текущие пины — `versions.sh`; ручной откат — `helm rollback`.
- Полный restore:
  1. восстановить Ubuntu/K3s;
  2. остановить K3s;
  3. вернуть data-dir/token/ключи;
  4. вернуть PVC до запуска Pods;
  5. сначала попробовать pull из GHCR (digest-пины); если пакеты недоступны —
     импортировать OCI-архивы (`k3s ctr images import`);
  6. проверить endpoints, HTTPS-имена, voice/music.

Disaster restore на запасном железе ещё не выполнялся.

## Ограничения

- Один узел, без HA control plane и распределённого storage.
- Алерты уходят в Telegram, но внешнего heartbeat (dead-man switch) нет.
- Нет полной latency instrumentation voice pipeline.
- Scheduler/bridge/control и Hermes/voice-proxy собираются в CI и
  публикуются в GHCR; runtime ссылается на них по digest.
