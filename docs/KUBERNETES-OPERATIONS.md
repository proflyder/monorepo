# Kubernetes: K3s VM

Один узел в VM 101; версии, состав и адреса — `STATE.md`, реальные адреса —
`secrets/NETWORK.md`. Компоненты описаны в своих README:
`projects/home/kubernetes/{flux,ingress,openbao,external-secrets,vaultwarden,nexus,mcp-gateway,currency-bot}/README.md`.

## Доступ

```bash
mise run kubectl -- get pods -A          # admin kubeconfig (fnox kube) + туннель
mise run helm -- -n home-assistant history runtime
ssh -F ~/.ssh/home-lab/config k3s 'sudo -n k3s kubectl get pods -A'
```

Голый `kubectl`/`helm` на рабочей машине смотрит в чужие кластеры — не
использовать. API `6443`, kubelet `10250`, node-exporter `9100`, health
`9116`, Hermes config-drift `9118` и VXLAN `8472/udp` закрыты от обычного LAN
host-firewall'ом (`projects/home/kubernetes/management-firewall/`, ручной);
`80/443` открыты LAN/VPN и обслуживаются Traefik (`Middleware/home-allowlist`).

## Данные

- `/srv/k3s/runtime` — K3s data, containerd, datastore (SQLite).
- `/srv/k3s/volumes/*` — app PV (local-path, `Retain`), в т.ч.
  `runtime-hermes` (Hermes `/opt/data`) и `openbao{,-backups}`.
- `/srv/k3s/recovery` — закрытые recovery-файлы.
- `runtime-ha` — архив старого HA Container, не активный writer.

Secrets приложений создаёт ESO из OpenBao `secret/k8s/<namespace>/<name>`;
локальные файлы в `secrets/` — только bootstrap.

## GitOps

Flux тянет `develop`; `deploy-home` только пинует проверенные digest'ы в
ветку `deploy/home-apps` (`ConfigMap/home-app-images`). Под Flux: `platform`
(ESO → cert-manager → Traefik → маршруты), `observability`, Vaultwarden,
Nexus и `app-*` юниты. OpenBao, host-firewall и Cloudflare DNS — ручные.
Ручной `kubectl apply` объектов Flux будет перезаписан.

```bash
mise run kubectl -- get kustomizations,helmreleases,gitrepositories -A
mise run kubectl -- -n flux-system annotate kustomization flux-system \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite
mise run kubectl -- -n flux-system suspend kustomization/<name>
mise run kubectl -- -n flux-system resume kustomization/<name>
```

Bootstrap, ключи `flux-settings` и секреты — `projects/home/kubernetes/flux/README.md`.

## Runtime chart: storage и workloads

Chart `projects/home-assistant/deploy/chart` имеет двух владельцев:

- PV/PVC (блок `storage` приватных values) применяет администратор
  `projects/home/kubernetes/apply-runtime.sh` (`kubectl apply`, единственный
  путь менять storage); они cluster-scoped, `Retain` и помечены
  `helm.sh/resource-policy: keep`;
- остальное — `HelmRelease/runtime` (Flux, `targetNamespace: home-assistant`).
  Значения из `Secret/runtime-values` не содержат `storage`, поэтому Helm не
  нужны права на PersistentVolume.

На чистой установке сначала `apply-runtime.sh`, затем Flux катит workloads. Приватный
values (`RUNTIME_PRIVATE_VALUES`) должен давать рендер, совпадающий с live;
проверка — пустой diff и код 0 (`helm get values` не содержит `storage`, а
`--dry-run=server` недостаточен). Непустой diff — сначала поправить приватный
файл, иначе `apply-runtime.sh` заденет несвязанные объекты:

```bash
mise run helm -- template runtime projects/home-assistant/deploy/chart \
  -n home-assistant -f "$RUNTIME_PRIVATE_VALUES" \
  | mise run kubectl -- diff -n home-assistant -f -
```

Перед изменением: backup, сравнить image digests; не менять модели, API и
storage schema одновременно. Один Deployment = один writer. Helm rollback не
откатывает SQLite/auth/state.

## Автодеплой Hermes / voice-proxy / health exporter

Юнит `hermes` (`hermes` — Docker; `voice-proxy`, `health-exporter`,
`home-deploy` — Bazel/rules_oci) катит Flux через `HelmRelease/runtime`:

1. `valuesFrom` читает `Secret/runtime-values` (ESO из
   `secret/k8s/flux-system/runtime-values`, поле `values`);
2. `postRenderers` подставляют `${HERMES_IMAGE}`, `${VOICE_PROXY_IMAGE}`,
   `${HEALTH_EXPORTER_IMAGE}`, `${HOME_DEPLOY_IMAGE}`, `${HERMES_CONFIG_IMAGE}`
   из pins; неизменный digest не даёт rollout;
3. `hermes-config` добавляет initContainer `render-config` (overlay из
   `projects/home-assistant/hermes/personal` → `/opt/data`, fail-closed) и sidecar
   `config-drift` (`:9118`); приватные значения рендера —
   `Secret/hermes-config-private` из `k8s/home-assistant/hermes-config`
   (поле `private.yaml`);
4. post-upgrade hook (`chart/templates/smoke-hook.yaml`,
   `home-deploy hermes assist-smoke`) проверяет rollout, health exporter,
   Assist через `conversation.hermes_agent` и voice-proxy; HA-токен — из
   Secret `ha-bridge-ha`;
5. `remediation.strategy: rollback` возвращает прежнюю ревизию при провале.

Проверка после выката: init-логи `render-config` без ошибок,
`hermes_config_drift_files 0` и `hermes_config_check_success 1`:

```bash
mise run kubectl -- -n home-assistant port-forward deploy/hermes 9118:9118
curl -s localhost:9118/metrics | grep hermes_config
```

Откат — revert pins/PR (план: `mise run rollback hermes --dry-run`). Аварийный
путь — `projects/home-assistant/deploy/rollout.sh` → `home-deploy hermes
rollout` или `helm rollback runtime <revision>`:

```bash
mise run secrets:exec -- -P kube exec -- platform/workstation/with-kube-tunnel.sh \
  home-deploy hermes rollout \
    --values-file "$RUNTIME_PRIVATE_VALUES" \
    --hermes-digest sha256:… --voice-proxy-digest sha256:… \
    --exporter-digest sha256:… \
    --smoke-image ghcr.io/proflyder/home-deploy@sha256:… \
    --state-dir /tmp/hermes-state
```

`--force` — upgrade при тех же digest'ах, `--simulate-smoke-failure` —
проверка отката.

## Observability

Grafana LAN `http://192.0.2.27:30300/`, HTTPS
`https://grafana.home.example.com/`; admin password —
`secret/k8s/monitoring/grafana-admin`. Loki/Prometheus наружу не
опубликованы. Dashboards — `projects/home-assistant/observability/chart/dashboards/`
(ConfigMap → sidecar).

```bash
mise run kubectl -- -n monitoring port-forward svc/monitoring-prometheus 19090:9090
mise run kubectl -- -n monitoring port-forward svc/loki 13100:3100
```

Алерты Flux (`FluxKustomizationNotReady`, `HelmReleaseNotReady`, `PinsStale`,
`observability/rules/prometheusrule-flux.yaml`) читают `gotk_resource_info`,
которую строит kube-state-metrics `customResourceState`
(`observability/values/prometheus.yaml`): сами контроллеры Flux статуса Ready
не отдают. Лейблы — `customresource_kind`, `exported_namespace`, `name`,
`ready`, `suspended`; отсутствующий путь = нет метки, поэтому правила
сравнивают через `!=`. Проверка:
`python3 -m unittest discover -s projects/home/kubernetes/observability -p 'test_*.py' -v`.

HAOS подключён Service `monitoring/home-assistant-external` + EndpointSlice;
при смене IP обновить `HAOS_ADDRESS` в `flux-settings`.

## Backup / restore

- Сохранять: cluster SQLite snapshot, server token, TLS/cred, app PVC.
  Без server token восстановление datastore невозможно.
- SQLite snapshots — через backup API, проверять `integrity_check`.
- Образы — публичные digest-pinned пакеты GHCR; если недоступны —
  «Пересобрать и задеплоить сервисы» (`OPERATIONS.md`).
- Полный restore:
  1. восстановить Ubuntu/K3s;
  2. остановить K3s;
  3. вернуть data-dir/token/ключи;
  4. вернуть PVC до запуска Pods;
  5. Flux тянет образы по pins;
  6. проверить endpoints, HTTPS-имена, voice/music.

Пошагово с нуля — `REBUILD.md`. Disaster restore на запасном железе ещё не
выполнялся.

## Ограничения

- Один узел, без HA control plane и распределённого storage.
- Dead-man switch (`solutions/heartbeat`) сообщает только о полной тишине
  Alertmanager.
- Нет полной latency instrumentation voice pipeline.
