# Kubernetes: K3s VM

Один узел `lab-k3s` в VM 101. Публичные адреса — примеры; реальные —
`secrets/NETWORK.md`.

## Состав

- Ubuntu 24.04.5, 12 GiB RAM, 200 GiB, `192.0.2.27`, node `lab-host`.
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
    Secrets приложений;
  - `vaultwarden`: Vaultwarden (пароли владельца), см.
    `projects/home/kubernetes/vaultwarden/`;
  - `nexus`: Nexus Repository OSS — кэш артефактов CI, см.
    `projects/home/kubernetes/nexus/`;
  - `flux-system`: Flux (source/kustomize/helm controllers), pull-based GitOps
    для платформы (ESO, cert-manager, Traefik, маршруты), observability,
    Vaultwarden и Nexus.
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
# admin kubeconfig (OpenBao, fnox profile kube) + SSH-туннель к API на время
# команды; `helm` — так же через `mise run helm --`
mise run kubectl -- get pods -A
```

Ключ, `known_hosts` и `config` (алиасы `pve`/`k3s`/`haos`) — `~/.ssh/home-lab/`
(`mise run secrets:pull`); kubeconfig — fnox, профиль `kube`
(`mise run secrets:exec -- -P kube exec -- platform/workstation/with-kube-tunnel.sh
kubectl …` поднимает этот туннель сам).

API `6443`, kubelet `10250`, node-exporter `9100`, health `9116`,
Hermes config-drift `9118` и VXLAN `8472/udp` закрыты firewall от обычного LAN.
`80/443` открыты для LAN/VPN и обслуживаются Traefik (IP allow-list на уровне
Middleware).
Firewall: `projects/home/kubernetes/management-firewall/`.

## Данные

- `/srv/k3s/runtime` — K3s data, containerd, datastore.
- `/srv/k3s/volumes/runtime-hermes` — Hermes home, в Pod `/opt/data`.
- `/srv/k3s/volumes/openbao{, -backups}` — OpenBao raft и raft snapshots
  (PV `openbao-data`/`openbao-backups`, `ricardo-existing`, `Retain`).
- `/srv/k3s/recovery` — закрытые recovery-файлы.
- Host-дерева `/opt/home` и запасных релизов нет: откат — revert
  pins-коммита или `flux suspend`; штатный деплой — Flux, `home-deploy
  converge` остаётся admin-путём (drills, cutover).
- `runtime-ha` — архив старого HA Container, не активный writer.

Секреты Hermes (`Secret/hermes-environment`, `Secret/hermes-config-private`),
scheduler/HA
(`scheduler-auth`, `ha-bridge-ha`), currency-bot (`currency-bot`,
`cloudflared-tunnel`), Grafana (`grafana-admin`) и Cloudflare
(`cloudflare-api-token`) создаются External Secrets Operator из OpenBao
`secret/k8s/<namespace>/<name>`. Источник правды — OpenBao; локальные файлы в
`secrets/` — только бэкап/bootstrap.

## Обновление

```bash
# chart runtime штатно катит Flux (`HelmRelease/runtime`, `app-hermes`)
# после merge в develop; ручной upgrade — только аварийный путь и будет
# перетёрт сверкой Flux. PV/PVC (storage) по-прежнему применяет admin
# через `apply-runtime.sh`.
mise run helm -- upgrade --install runtime projects/home-assistant/deploy/chart \
  -n home-assistant -f "$RUNTIME_PRIVATE_VALUES" \
  --wait --timeout 5m
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

mise run helm -- template runtime projects/home-assistant/deploy/chart \
  -n home-assistant -f "$RUNTIME_PRIVATE_VALUES" \
  | mise run kubectl -- diff -n home-assistant -f -
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

Это разделение обязательно: Flux применяет только namespace-scoped объекты
(`targetNamespace: home-assistant`), а Helm при удалении объекта из релиза
делает GET и без `get persistentvolumes` падает. PV/PVC помечены
`helm.sh/resource-policy: keep`, поэтому переход безопасен и для релиза,
который ещё помнит storage. Полный ручной apply (`apply-runtime.sh`) —
единственный путь менять storage; Flux его не трогает. На чистой установке
сначала `apply-runtime.sh` (создаёт PV/PVC), затем Flux катит workloads.

## Автодеплой Hermes / voice-proxy / health exporter

Юнит `hermes` (builder `mixed`: `hermes` — Docker; `voice-proxy`,
`health-exporter` и `home-deploy` — Bazel/rules_oci, явная карта
`deploy.bazel_images`) катит Flux: `HelmRelease/runtime` (в `flux-system`,
`targetNamespace`/`storageNamespace: home-assistant`) берёт chart из Git,
образы — из `ConfigMap/home-app-images`, приватные values — из
`Secret/runtime-values` (`valuesFrom`, без блока `storage`). Job `pins`
только записывает проверенные digest'ы.

1. `valuesFrom` читает `Secret/runtime-values`; ESO создаёт его из OpenBao
   `secret/k8s/flux-system/runtime-values` (поле `values`, токенов нет — они
   в Kubernetes Secrets);
2. если digest не изменился, pod template не меняется и rollout не идёт;
3. `postRenderers` подставляют `${HERMES_IMAGE}`, `${VOICE_PROXY_IMAGE}`,
   `${HEALTH_EXPORTER_IMAGE}`, `${HOME_DEPLOY_IMAGE}`, `${HERMES_CONFIG_IMAGE}`
   через strategic-merge/JSON6902, не перезаписывая приватные массивы values;
   exporter получает distroless command и без старого ConfigMap-mount;
   `hermes-config` добавляет initContainer `render-config` (reviewed overlay →
   PVC, fail-closed), sidecar `config-drift` (`:9118`) и preStop drain у
   `gateway`;
4. post-upgrade Helm hook (`chart/templates/smoke-hook.yaml`,
   `home-deploy hermes assist-smoke` под `smoke.enabled`) проверяет rollout,
   health exporter (все endpoint'ы = 1), Assist через
   `conversation.hermes_agent` и voice-proxy; HA-токен читается из Secret
   `ha-bridge-ha`, в GitHub он не попадает;
5. `remediation.strategy: rollback` + `remediateLastFailure: true` возвращают
   прежнюю ревизию при провале upgrade или hook'а.

### Конфиг Hermes из Git

Reviewed-overlay из `projects/home-assistant/hermes` больше не рендерится
вручную: образ `hermes-config` (`ghcr.io/proflyder/hermes-config`, digest —
checksum overlay) запускает `home-deploy hermes render-config` как initContainer
до gateway и `hermes drift-exporter` как sidecar `config-drift` (read-only,
`0.0.0.0:9118`, интервал 300 с). InitContainer пишет `config.yaml`,
`.hermes.md`, `SOUL.md`, skills и prompts на `/opt/data`; ошибка (placeholder
refusal, нечитаемый private Secret, плохой YAML) валит Pod, а Helm откатывает
релиз. Sidecar публикует `hermes_config_drift_files`,
`hermes_config_check_success`, `hermes_config_last_check_timestamp_seconds` и
`hermes_config_overlay_info`; алерты `HermesConfigDrift`,
`HermesConfigCheckFailing`, `HermesConfigDriftExporterDown` — warning в тот же
Telegram. Opt-in Service `hermes-config-drift` рендерится chart'ом только при
`hermesConfigDrift.enabled` (у Flux включён).

Приватные значения рендера — `Secret/hermes-config-private` из OpenBao
`k8s/home-assistant/hermes-config` поле `private.yaml` (ESO,
`ExternalSecret/hermes-config-private`). Токены остаются в
`hermes-environment`; в private.yaml только реальные endpoint'ы и Telegram
chat id. До merge владелец один раз копирует старое поле (значения не печатать):

```bash
export VAULT_TOKEN="$(cat secrets/openbao/operator.token)"
tmp=$(mktemp) && chmod 600 "$tmp"
mise exec -- bao kv get -format=json secret/infra/kubernetes/values-private |
  jq '{"private.yaml": .data.data.hermes_private_yaml}' > "$tmp"
version=$(mise exec -- bao kv metadata get -format=json \
  secret/k8s/home-assistant/hermes-config 2>/dev/null |
  jq -r '.data.current_version // 0')
mise exec -- bao kv put -cas="$version" secret/k8s/home-assistant/hermes-config @"$tmp"
rm -f "$tmp"; unset VAULT_TOKEN
```

`-cas` защищает от перезаписи чужой правки; при `version=0` путь создаётся.
После проверки старое поле `infra/kubernetes/values-private#hermes_private_yaml`
можно удалить (оно больше не читается).

После merge владелец проверяет: init-логи `render-config` без ошибок,
`hermes_config_drift_files 0` и `hermes_config_check_success 1` на `:9118`
(например, `mise run kubectl -- -n home-assistant port-forward
deploy/hermes 9118:9118` и `curl -s localhost:9118/metrics`, либо график
Prometheus), и что плановый rollout не прислал «shutting down» в Telegram. Отдельно применяется host-firewall: `9118` добавлен в drop-список
`projects/home/kubernetes/management-firewall/rules.nft` (Prometheus скрейпит
порт из pod-сети, которую правило пропускает; LAN — нет); владелец
переустанавливает правило по README этого каталога. Откат конфига — revert PR/pins (Flux
перезапустит Pod со старым образом) или аварийный ручной render.

Ручной аварийный путь остаётся `projects/home-assistant/deploy/rollout.sh` →
`home-deploy hermes rollout` (digest no-op, `helm upgrade`, smoke,
`helm rollback`) и `helm rollback runtime <revision>`.

План и документированную команду отката печатает
`mise run rollback hermes --dry-run` (источник — `platform/ci/units.mjs`).

Ручной прогон того же пути:

```bash
mise run secrets:exec -- -P kube exec -- platform/workstation/with-kube-tunnel.sh \
  home-deploy hermes rollout \
    --values-file "$RUNTIME_PRIVATE_VALUES" \
    --hermes-digest sha256:… --voice-proxy-digest sha256:… \
    --exporter-digest sha256:… \
    --smoke-image ghcr.io/proflyder/home-deploy@sha256:… \
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

Кроме `infra.*`, шлюз несёт группы `bb.read`/`bb.act` (треды и запуск
агентов, только личный профиль) и голосовой маршрут `/v1/voice/*` с отдельным
токеном `mcp-gateway-voice`. До bb он доходит только по SSH к CT 103 с
ограниченным ключом и forced command `bb-dispatch` (Secret
`mcp-gateway-bb-ssh`); API bb наружу не открывается. Ротация ключей и токенов
— в `OPERATIONS.md` (раздел «MCP-шлюз») и `projects/home/proxmox/bb/`.

Одноразово (admin, до первого деплоя):

```bash
# токен шлюза↔Hermes; значение никогда не печатать
BAO_TOKEN=$(cat secrets/openbao/operator.token) bao kv put secret/k8s/home-assistant/mcp-gateway-auth token="$(openssl rand -hex 32)"
mise run kubectl -- apply -f projects/home/kubernetes/mcp-gateway/manifests/rbac.yaml
mise run kubectl -- apply -f projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml
mise run kubectl -- -n home-assistant wait --for=condition=Ready \
  externalsecret/mcp-gateway-auth externalsecret/mcp-gateway-telegram \
  externalsecret/mcp-gateway-ha externalsecret/mcp-gateway-bb-ssh \
  externalsecret/mcp-gateway-voice --timeout=120s
```

RBAC самого gateway — `manifests/rbac.yaml`; CI-only `ci-deploy` удалён.
Деплой — Flux (`app-mcp-gateway`); проверки:

```bash
mise run kubectl -- -n home-assistant get deploy,pod,svc -l app.kubernetes.io/name=mcp-gateway
mise run kubectl -- -n home-assistant logs deploy/mcp-gateway --tail=50
mise run kubectl -- -n home-assistant get --raw \
  "/api/v1/namespaces/home-assistant/services/http:mcp-gateway:8080/proxy/readyz"
mise run kubectl -- -n home-assistant exec deploy/mcp-gateway -- \
  /usr/local/bin/mcp-gateway -healthcheck
```

Проверка прав ровно по каталогу:

```bash
mise run kubectl -- auth can-i --as=system:serviceaccount:home-assistant:mcp-gateway \
  patch deployment/scheduler -n home-assistant      # yes
mise run kubectl -- auth can-i --as=system:serviceaccount:home-assistant:mcp-gateway \
  delete deployment/scheduler -n home-assistant     # no
mise run kubectl -- auth can-i --as=system:serviceaccount:home-assistant:mcp-gateway \
  patch statefulset/openbao -n openbao              # yes
mise run kubectl -- auth can-i --as=system:serviceaccount:home-assistant:mcp-gateway \
  patch deployment/traefik -n monitoring            # no
```

Перезапуск выполняет только шлюз, всегда два шага (`infra_prepare_restart` →
`infra_confirm`, TTL 90 с, cooldown 5 мин на workload). Ручной эквивалент для
диагностики — обычный rollout restart, но он обходит предупреждения:

```bash
mise run kubectl -- -n home-assistant rollout restart deployment/piper
mise run kubectl -- -n home-assistant annotate deployment/piper mcp-gateway/last-restart- --overwrite
```

Ротация токена: обновить OpenBao `k8s/home-assistant/mcp-gateway-auth`, затем
перезапустить `mcp-gateway` и `hermes` (иначе Hermes продолжит слать старый
Bearer). Ротация Telegram-бота: обновить `TELEGRAM_BOT_TOKEN` /
`TELEGRAM_ALLOWED_USERS` в `k8s/home-assistant/hermes-environment` и
дождаться синхронизации обоих ExternalSecret (force-sync аннотацией).

## Grafana / observability

- LAN: `http://192.0.2.27:30300/` (корень)
- HTTPS (LAN/VPN): `https://grafana.home.example.com/`
- Loki/Prometheus наружу не опубликованы.
- Port-forward для диагностики:

```bash
mise run kubectl -- -n monitoring port-forward svc/monitoring-prometheus 19090:9090
mise run kubectl -- -n monitoring port-forward svc/loki 13100:3100
```

Grafana admin password — OpenBao `secret/k8s/monitoring/grafana-admin`.

### Алерты доставки Flux

`FluxKustomizationNotReady`, `HelmReleaseNotReady` и `PinsStale`
(`observability/rules/prometheusrule-flux.yaml`) читают `gotk_resource_info` —
info-метрику, которую kube-state-metrics строит из Flux CRD через
`kube-state-metrics.customResourceState` в `observability/values/prometheus.yaml`
(RBAC list/watch на `kustomizations`, `helmreleases`, `gitrepositories`,
`helmrepositories`, `ocirepositories` — там же). Контроллеры Flux 2.x отдают
только `gotk_reconcile_duration_seconds_*` и `gotk_token_*`, статуса Ready в их
метриках нет; `gotk_reconcile_condition` не существует, ссылки на него давали
вечный `PinsStale` и два алерта, которые не могли сработать никогда.

- Лейблы метрики: `customresource_kind` (не `kind`), `exported_namespace`
  (kube-state-metrics не добавляет `namespace` к custom-resource метрикам),
  `name`, `ready`, `suspended`, `revision`.
- Отсутствующий путь в объекте = метки нет; поэтому правила сравнивают через
  `!=` (`ready!="True"`, `suspended!="true"`), а не равенством.
- `PinsStale` дополнительно смотрит `kube_configmap_info` (collectors
  kube-state-metrics остаются включёнными) и отдельным `absent()` — полное
  отсутствие `gotk_resource_info` для `flux-pins`.
- Проверка правил: `python3 -m unittest discover -s
  projects/home/kubernetes/observability -p 'test_*.py' -v` (структурные
  контракты + `promtool check rules`, если `promtool` есть в PATH).

## HTTPS ingress

`projects/home/kubernetes/ingress/` — Traefik `42.2.0` + cert-manager
`v1.21.3` и wildcard-сертификат Let's Encrypt (`*.home.example.com`). Всё,
кроме DNS-записей, reconciled Flux: `platform-cert-manager` →
`platform-traefik` → `platform-routes` под umbrella `platform`.

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
  `currency.home.example.com` → `currency-bot/currency-bot:8080`,
  `intercom.home.example.com` → `home-assistant/intercom:8099`
  (`responseForwarding.flushInterval: 100ms` для streaming `Watch`;
  NodePort `30097` остаётся аварийным путём).
  Собственные маршруты: `vault.home.example.com` → `openbao/openbao:8200`,
  `passwords.home.example.com` → `vaultwarden/vaultwarden:8080`,
  `nexus.home.example.com` (UI/admin, LAN/VPN) и публичный
  `ci-nexus.<public-zone>/repository/*` → `nexus/nexus:8081` (Flux).
- Все маршруты и сертификат — в namespace `traefik`; `allowCrossNamespace`
  разрешает Service из `monitoring`/`home-assistant`/`currency-bot`. Solver-Secret
  `cloudflare-api-token` живёт в namespace `cert-manager`, потому что
  `ClusterIssuer` резолвит его в cluster resource namespace.
- Реальные имена, `LAN_CIDR` и `HAOS_ADDRESS` — `secrets/home/ingress.env` и
  ключи `HOME_DOMAIN`/`LAN_CIDR`/`HAOS_ADDRESS` в
  `secret/k8s/flux-system/settings`; токен Cloudflare хранится в OpenBao
  (`secret/k8s/cert-manager/cloudflare-api-token`) и материализуется ESO.
- **DNS — ручной шаг.** A-записи `ha`, `grafana`, `alarms`, `control`,
  `currency`, `intercom`, `passwords`, `vault` создаются
  `cloudflare_dns.py` с приватным токеном; Flux их не трогает.

```bash
# после merge в develop дождаться сверки
mise run kubectl -- get kustomizations -A
mise run kubectl -- -n flux-system annotate kustomization/platform \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite
NODE_IP=192.0.2.27 ./projects/home/kubernetes/ingress/verify.sh
```

Диагностика: `mise run kubectl -- -n cert-manager get pods,challenge,order`,
`mise run kubectl -- -n traefik get certificate,secret,ingressroute`,
`mise run kubectl -- -n traefik logs deploy/traefik`. Порты `80/443` на узле заняты
Traefik — не запускать там второй прокси без остановки release.

## OpenBao

`openbao` namespace: StatefulSet `openbao-0`, raft на PV, декларативный
audit в stdout (Loki: `{namespace="openbao"}`), UI/API `https://vault.home.example.com/` через Traefik.
Auto-unseal — `static` seal (Secret `openbao-seal`), поэтому после рестарта
пода распечатывание не требуется.

```bash
mise run kubectl -- -n openbao get pod,pvc,cronjob
mise run kubectl -- -n openbao exec openbao-0 -- /bin/sh -ec 'BAO_ADDR=http://127.0.0.1:8200 bao status'
mise run kubectl -- -n openbao delete pod openbao-0   # OnDelete: применяет новый конфиг
```

Обновление/бутстрап/backup/восстановление — `projects/home/kubernetes/openbao/README.md`.

## External Secrets Operator

ESO `2.14.0` в namespace `external-secrets` читает KV v2 `secret/` и создаёт
Secrets приложений. `ClusterSecretStore/openbao` использует in-cluster Service
`http://openbao.openbao.svc:8200`, Kubernetes auth и роль `external-secrets`
(политика `k8s-read`, только `secret/data/k8s/*`).

Сам оператор, `ClusterSecretStore/openbao` и инфраструктурные
ExternalSecrets (cert-manager, monitoring, vaultwarden) — под Flux
(`platform-external-secrets` и компонентные Kustomization).
`external-secrets/deploy.sh` применяет только ExternalSecrets приложений
(аварийный ручной путь); в обычной работе их ведёт Flux.

```bash
mise run admin:external-secrets        # только application ExternalSecrets
mise run kubectl -- get clustersecretstore openbao
mise run kubectl -- get externalsecret -A
mise run kubectl -- -n home-assistant annotate externalsecret scheduler-auth \
  force-sync="$(date +%s)" --overwrite   # немедленная синхронизация
```

Манифесты — рядом с проектами; таблица путей, миграция и ротация —
`projects/home/kubernetes/external-secrets/README.md`. Flux применяет их и
ждёт готовности workload'ов; CI значения не пишет.

## Flux / GitOps (домашняя инфраструктура)

`projects/home/kubernetes/flux/` устанавливает три контроллера Flux
(`source-controller`, `kustomize-controller`, `helm-controller`; без
notification и image automation) и один `GitRepository` на
`github.com/proflyder/monorepo`, ветка `develop`, с read-only deploy key из
OpenBao. Кластер сам тянет изменения; у CI нет admin-доступа, а `deploy-home`
только пинует проверенные digest'ы в машинную ветку `deploy/home-apps`,
которую Flux читает как `ConfigMap/home-app-images`.

Под Flux: платформа (`platform`: ESO → cert-manager → Traefik → общие
маршруты), observability (`observability` + `observability-rules`),
Vaultwarden, Nexus (`nexus` + `nexus-provision`) и все деплой-юниты
приложений: `app-wakegrant`, `app-intercom`, `app-currency-bot`,
`app-mcp-gateway`, `app-ha-executors` (scheduler + три executor'а) и
`app-hermes` (`HelmRelease/runtime`). OpenBao, host-firewall (nftables) и
Cloudflare DNS остаются ручными. Входящего доступа CI в K3s больше нет:
`deploy-home` не содержит job `deploy`, `ci-deploy`/`ci-github` отозваны.

`platform` — umbrella-Kustomization: она применяет четыре дочерние
(`platform-external-secrets`, `platform-cert-manager`, `platform-traefik`,
`platform-routes`) и ждёт их готовности; остальные юниты зависят только от
`platform`. Helm-release'ы сохраняют живые имена и namespace
(`external-secrets`, `cert-manager`, `traefik`, `monitoring`), поэтому
helm-controller принимает их как upgrade, без переустановки и пересоздания
PVC.

Bootstrap и ручные шаги владельца — `projects/home/kubernetes/flux/README.md`
(`mise run admin:flux-bootstrap`). Секреты — OpenBao
`secret/k8s/flux-system/{git-deploy-key,settings}`, в Git их нет. Из
`flux-settings` подставляются `NEXUS_HOST`, `NEXUS_PUBLIC_HOST`,
`NODE_LAN_IP`, `HOME_DOMAIN`, `LAN_CIDR`, `HAOS_ADDRESS`, `PVE_NODE_TARGET`.
Kustomization `observability` дополнительно подставляет `STATION_IP` из
`flux-app-settings` (scrape-таргет колонки `${STATION_IP}:9101`).

```bash
# что синхронизировано и готово
mise run kubectl -- get kustomizations -A
mise run kubectl -- get helmreleases -A
mise run kubectl -- get gitrepositories -A
mise run kubectl -- -n flux-system get pods

# форсировать sync после merge в develop
mise run kubectl -- -n flux-system annotate kustomization flux-system \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite

# приостановить / возобновить юнит
mise run kubectl -- -n flux-system suspend kustomization/observability
mise run kubectl -- -n flux-system resume kustomization/observability
```

Инфраструктуру под Flux меняют только merge'ем в `develop`; ручной
`kubectl apply` тех же объектов будет перезаписан при следующей сверке.
Cloudflare DNS остаётся ручным шагом (см. README Flux и ingress).
Логи и `exec` — через тот же `mise run kubectl -- ...`.

## Удалённый доступ

- Вход снаружи — только собственный VPN (ocserv, CT `102`,
  `vpn.example.com`), см. `projects/home/proxmox/vpn/`.
- Внутри LAN/VPN — HTTPS-имена `*.home.example.com` из раздела выше.
- HAOS в K3s подключён через Service `monitoring/home-assistant-external`
  и EndpointSlice с адресом HAOS; при смене IP обновить `HAOS_ADDRESS`
  в `flux-settings` (`HAOS_ADDRESS`) и дождаться сверки `platform-routes`.
- Публичного HA нет; port-forward на роутере только для VPN.

## Backup / restore

- Сохранять: cluster SQLite snapshot, token, TLS/cred, app PVC, custom images.
- SQLite snapshots — через backup API, проверять `integrity_check`.
- Без server token восстановление datastore невозможно.
- Образы scheduler/ha-bridge/ha-control/currency-bot собираются Bazel
  (`rules_oci`, distroless, digest-pinned base) и адресуются по digest; в CI
  они пушатся в GHCR, деплой подставляет digest и узел тянет образ из GHCR.
- currency-bot хранит историю в SQLite на PVC `currency-bot-data` (1Gi,
  `local-retain`); подробности — `projects/home/kubernetes/currency-bot/README.md`.
- Hermes и voice-proxy — публичные digest-pinned образы GHCR
  (`ghcr.io/proflyder/{hermes,voice-proxy}@sha256:…`), `imagePullPolicy:
  IfNotPresent`, без pull-secret. `hermes` собирается Docker, `voice-proxy` —
  Bazel `rules_oci` (distroless, `oci_push` без переписывания digest).
  Flux (`HelmRelease/runtime` в `app-hermes`) подставляет digest'ы (включая
  `ghcr.io/proflyder/health-exporter`) из pins-ветки, прогоняет голосовой
  smoke hook и откатывается на предыдущую ревизию при провале
  (`remediation.strategy: rollback`). CI только пинует digest'ы; ручной
  аварийный путь — `projects/home-assistant/deploy/rollout.sh` и
  `helm rollback`.
- Полный restore:
  1. восстановить Ubuntu/K3s;
  2. остановить K3s;
  3. вернуть data-dir/token/ключи;
  4. вернуть PVC до запуска Pods;
  5. pull из GHCR по digest-пинам; если пакеты недоступны — пересобрать и
     запушить образы из монорепо (CI/Bazel);
  6. проверить endpoints, HTTPS-имена, voice/music.

Disaster restore на запасном железе ещё не выполнялся.

## Ограничения

- Один узел, без HA control plane и распределённого storage.
- Внешний dead-man switch (Cloudflare Worker `home-heartbeat`) сообщает только
  о полной тишине Alertmanager; отдельные сервисы покрывают обычные алерты.
- Нет полной latency instrumentation voice pipeline.
- Scheduler/bridge/control/voice-proxy собираются Bazel в CI, Hermes — Docker;
  все публикуются в GHCR, runtime ссылается на них по digest.
