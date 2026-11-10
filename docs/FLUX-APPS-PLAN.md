# FLUX-APPS-PLAN — перевод сервисов `deploy-home` на Flux

Статус: **предложение, не реализовано**. Документ описывает исследование и
пошаговый план; кластер, CI и `deploy-home` этим документом не меняются.
После принятия решения действующие факты переносятся в `STATE.md`/`DECISIONS.md`,
команды — в `OPERATIONS.md`, а этот план удаляется (правило `docs/README.md`).

Дата снимка состояния: 2026-11-10.

## 0. Кратко

- Сейчас `deploy-home` после merge в `develop` входит в дом по VPN (группа
  `ci`, сертификат `ci-github`), читает `secret/ci/deploy-home` по OIDC и
  катит сервисы `kubectl`/`helm` под ServiceAccount `ci-deploy`. Это
  единственный оставшийся входящий канал CI в кластер.
- Цель — оставить CI только сборку, проверку provenance и публикацию
  артефактов, а применение в кластер передать Flux. После перевода у CI не
  должно остаться ни VPN-маршрута к API, ни `ci-deploy`, ни kubeconfig.
- Инвентарь: семь деплой-юнитов — `scheduler`, `ha-bridge` (вместе с
  `ha-control`/`ha-scenario`), `wakegrant`, `intercom`, `currency-bot`,
  `mcp-gateway`, `hermes`. Отдельно остаются `deploy-station` (Pulsar через
  Home Assistant) и `native-release` — они не применяют объекты в K3s.
- Версии образов: рекомендуется **вариант B** — CI после зелёной сборки
  проверяет digest и provenance и коммитит digest в отдельную машинную ветку
  `deploy/home-apps`; Flux читает её как ConfigMap и подставляет в манифесты.
  `develop` остаётся PR-only, read-only deploy key Flux не меняется,
  image-reflector/image-automation не нужны.
- Preflight/rollback: `Kustomization.wait`, `timeout`, `retryInterval` и
  `healthChecks`; откат — revert коммита с digest (или manifest-коммита), а не
  `kubectl rollout undo`. Для Hermes — Helm remediation с откатом на прежнюю
  ревизию. Flagger не нужен.
- Секреты: приватные env/values переезжают из `secret/ci/deploy-home` в
  `secret/k8s/...`; ESO создаёт `flux-app-settings` и `runtime-values`, Flux
  применяет сами `ExternalSecret`. Значения в Git не попадают.
- Пилот — `wakegrant`: один Deployment + PVC + Service + один
  `ExternalSecret`, без режимов и cross-service зависимостей. Последний —
  `hermes`: Helm-релиз, PV, кастомный smoke и откат.
- Важное уточнение по финалу: группу VPN `ci` **нельзя отозвать целиком**,
  пока `deploy-station` ходит в HA сертификатом `ci-pulsar`. Сейчас можно
  отозвать `ci-github`, убрать правило `6443` и `ci-deploy`, а `443` и группу
  `ci` оставить для станции (см. §6.7).

## 1. Что есть сейчас

### 1.1. Flux

В кластере только три контроллера Flux `v2.10.0`:
`source-controller`, `kustomize-controller`, `helm-controller`
(образы `v1.10.1`/`v1.7.1`). `notification-controller`,
`image-reflector-controller` и `image-automation-controller` намеренно не
установлены; это закреплено тестом `projects/home/kubernetes/flux/test_flux.py`.

`GitRepository/flux-system` смотрит на `develop` по read-only deploy key из
OpenBao (`secret/k8s/flux-system/git-deploy-key`). Корневая Kustomization
`flux-system` применяет `projects/home/kubernetes/flux/cluster`; дальше
работают `platform` (ESO → cert-manager → Traefik → routes), `observability`,
`observability-rules`, `vaultwarden`, `nexus`, `nexus-provision`.
Приватные значения подставляются из `Secret/flux-settings`
(`secret/k8s/flux-system/settings`) через `postBuild.substituteFrom`.

Живой снимок на 2026-11-10: все Kustomizations `Ready=True`, ревизия
`develop@1a18b78`; в `home-assistant` работают `scheduler`, `ha-bridge`,
`ha-control`, `ha-scenario`, `intercom`, `mcp-gateway`, `wakegrant`,
`hermes`, `piper`, `whisper`, `gigaam`, `runtime-health-exporter`;
в `currency-bot` — `currency-bot` и `cloudflared`.

### 1.2. `deploy-home`

Push в `develop` (squash merge PR) запускает `.github/workflows/deploy-home.yml`:

1. `plan` маршрутизирует diff через `platform/ci/units.mjs` и
   `platform/ci/affected.mjs`; матрица `deploy` содержит затронутые
   деплой-юниты.
2. Job `deploy` (Environment `Home`): checkout, VPN `openconnect`
   (`--authgroup ci`), OIDC в OpenBao (`auth/github`, роль `deploy-home`),
   чтение `secret/ci/deploy-home` (`kubeconfig` + `env_key` юнита),
   материализация файлов `0600`, `kubectl` под `home-assistant/ci-deploy`,
   проверка RBAC-границы и `ExternalSecret ... Ready`.
3. Промоушен образов: `platform/ci/promote.mjs` проверяет
   `tree-<hash>` и provenance, ставит теги `<commit>`/`develop` на тот же
   digest и отдаёт `PREBUILT_DIGESTS`; `DEPLOY_NO_BUILD=1` запрещает сборку
   на деплой-раннере (DECISIONS 36).
4. Запуск `projects/home/kubernetes/<unit>/deploy.sh` (или
   `projects/home-assistant/deploy/rollout.sh` для Hermes). Скрипт рендерит
   шаблоны `${...}`, применяет объекты, ждёт rollout, выполняет
   app-specific проверки и при падении восстанавливает записанное состояние
   (`home-deploy state rollback` / `helm rollback`).
5. `deploy-station` (только при изменении `projects/station/pulsar/VERSION`)
   входит в VPN группой `ci` сертификатом `ci-pulsar` и ставит прошивку
   через HA. `native-release` публикует native-артефакты.

Секреты CI: в GitHub Environment `Home` лежат только `VPN_SERVER`,
`VPN_CERT_P12_B64`, `VPN_CERT_PASSWORD`, `OPENBAO_ADDR`, `OPENBAO_ROLE`.
Всё остальное — в OpenBao `secret/ci/deploy-home` (ключи `kubeconfig`,
`scheduler_env`, `wakegrant_env`, `intercom_env`, `currency_bot_env`,
`mcp_gateway_env`, `runtime_values`).

### 1.3. Живое состояние workloads

| Namespace | Объекты (сейчас) | Особенность |
|---|---|---|
| `home-assistant` | `scheduler`, `ha-bridge`, `ha-control`, `ha-scenario`, `intercom`, `mcp-gateway`, `wakegrant`, `hermes`, `piper`, `whisper`, `gigaam`, `runtime-health-exporter` | один узел, SQLite/PV, `hostNetwork` у Hermes и голосовых |
| `currency-bot` | `currency-bot`, `cloudflared` | отдельный namespace, публичный webhook через Cloudflare Tunnel |
| `flux-system` | 3 контроллера | GitOps-платформа |
| `monitoring` | Prometheus/Grafana/Loki/Alloy | наблюдаемость |

PVC приложений: `scheduler-data` 2Gi, `ha-bridge-data` 256Mi,
`ha-scenario-data` 256Mi, `wakegrant-data` 1Gi, `currency-bot-data` 1Gi —
все `local-retain`; у Hermes — `runtime-hermes` 10Gi и `runtime-{ha,onnx,piper,whisper}`
5Gi на `ricardo-existing`. PV Hermes применяет администратор
(`apply-runtime.sh`), не CI.

## 2. Инвентарь CI-деплоимых сервисов

### 2.1. Сводная таблица

| Юнит | NS | Что применяет `deploy.sh` | Образы | Состояние / PV | Секреты и env | Preflight / rollback | Главные особенности |
|---|---|---|---|---|---|---|---|
| `scheduler` | `home-assistant` | Deployment `scheduler`, PVC `scheduler-data`, Service `scheduler` (NodePort) | `scheduler`, `home-deploy` (preflight); siblings `ha-bridge`/`ha-control`/`ha-scenario` рендерятся, но не применяются | SQLite 2Gi, Recreate, 1 реплика | ESO `scheduler-auth`, `ha-bridge-ha`; `scheduler_env` (HA_URL, LAN_CIDR, NODE_LAN_IP, порты, SCHEDULER_PUBLIC_URL); digest-переменные | in-cluster preflight только для `--execute`; `home-deploy state rollback`; `-healthcheck` и `GetStatus.ready` | критичный планировщик, NodePort 30098, host nft-фильтр, cutover-режимы |
| `ha-bridge` | `home-assistant` | Deployment `ha-bridge` (0/1), `ha-control` (1), `ha-scenario` (1), PVC `ha-bridge-data`/`ha-scenario-data`, Service `ha-control` (NodePort) | `ha-bridge`, `ha-control`, `ha-scenario`; `scheduler` — optional sibling | SQLite 256Mi ×2, Recreate; `ha-control` без PVC | те же ESO-секреты; `scheduler_env`; `BRIDGE_EXECUTE`, `CONTROL_EXECUTE`, replicas — live-состояние | `--shadow`/`--execute`/`--control-*` только вручную, fail-closed preflight; `state rollback` | режим и масштаб не выводятся из манифеста; Flux не должен менять их неявно |
| `wakegrant` | `home-assistant` | Deployment `wakegrant`, PVC `wakegrant-data`, Service `wakegrant` (NodePort) | `wakegrant` | SQLite 1Gi, Recreate, 1 реплика | ESO `wakegrant` (`issuer_token`, `machines`); `wakegrant_env` (`WAKE_GRANT_NODE_PORT`) | preflight нет; rollback не документирован (fix-forward); `-healthcheck` | самый простой юнит: один Deployment + Service + PVC, без cross-service зависимостей |
| `intercom` | `home-assistant` | Deployment `intercom`, Service `intercom` (NodePort) | `intercom` | состояние звонка in-memory, без PVC, Recreate | ESO `intercom` (`station_token`; `phone_token` из `scheduler-auth`); `intercom_env` (`INTERCOM_NODE_PORT`) | preflight нет; fix-forward; `-healthcheck` | ingress-route и NetworkPolicy уже во Flux-юните `ingress/routes` |
| `currency-bot` | `currency-bot` | Deployment `currency-bot`, PVC `currency-bot-data`, Service `currency-bot` (NodePort) и `currency-bot-webhook` (ClusterIP), Deployment `cloudflared`, 4 NetworkPolicy, ServiceMonitor | `currency-bot`, `cloudflared` (digest из `versions.sh`) | SQLite 1Gi, Recreate | ESO `currency-bot`, `cloudflared-tunnel`; `currency_bot_env` (WEBHOOK_URL, LAN_CIDR, NODE_LAN_IP, NodePort) | preflight нет; `state rollback`; smoke `/readyz` и endpoint-slice webhook | namespace, публичный Cloudflare Tunnel, ServiceMonitor, host nft-фильтр NodePort |
| `mcp-gateway` | `home-assistant` | Deployment `mcp-gateway`, Service (ClusterIP), 2 NetworkPolicy, ServiceMonitor, ConfigMap `mcp-gateway-catalog` (`catalog.yaml` + `bb-targets.yaml`), ServiceAccount/Roles | `mcp-gateway` | без PVC | 5 ESO-секретов (`auth`, `telegram`, `ha`, `bb-ssh`, `voice`); `mcp_gateway_env` (LAN_CIDR, NODE_LAN_IP, HAOS_IP, STATION_IP, BB_CT_IP, BB_PROJECT_*, BB_DISPATCHER_THREAD_ID) | preflight нет; `state rollback`; `-healthcheck` и `/readyz` | generated ConfigMaps, RBAC, внешние проверки, bb-dispatch по SSH |
| `hermes` | `home-assistant` | Helm release `runtime`: `hermes` (gateway + voice-proxy), `piper`, `whisper`, `gigaam`, `runtime-health-exporter`; PV/PVC — администратор | `hermes` (Docker), `voice-proxy`, `health-exporter`, `home-deploy` (smoke) | PV/PVC `ricardo-existing`, долгоживущий SQLite/state | ESO `hermes-environment`; `runtime_values` из OpenBao (без токенов) | `home-deploy hermes rollout`: digest no-op, `helm upgrade`, smoke (rollout, exporter, Assist, voice-proxy health, логи), `helm rollback` | самый сложный: Helm, hostNetwork, PV, кастомный smoke/откат, несколько образов |

### 2.2. `scheduler`

`projects/home/kubernetes/scheduler/deploy.sh` — общий скрипт для
`scheduler` и `ha-bridge`. Он:

- разрешает образы из `PREBUILT_DIGESTS`/`*_IMAGE`; при `DEPLOY_NO_BUILD=1`
  ничего не собирает и не пушит;
- считает `BUILD_ID` (revision + fingerprint) и `ROLLOUT_ID` (fingerprint
  отрендеренного набора) и рендерит шаблоны `home-deploy render --unit
  scheduler`; под Flux `BUILD_ID` приходит из pins, а `ROLLOUT_ID` из
  pod-template убирается (см. §3.3);
- применяет только `scheduler.yaml` (Deployment, PVC, Service) для юнита
  `scheduler`; `bridge.yaml`, `control.yaml`, `control-service.yaml`,
  `scenario.yaml`, `preflight-job.yaml` рендерятся, но не применяются;
- проверяет `ExternalSecret/scheduler-auth` и `ha-bridge-ha` в состоянии
  `Ready`;
- пишет `state`-маркеры до/после converge и при падении CI откатывает
  Deployment через `home-deploy state rollback`;
- после rollout выполняет `scheduler -healthcheck` и
  `scheduler -status` (проверка `.ready == true`).

Шаблоны содержат `${SCHEDULER_IMAGE}`, `${SCHEDULER_NODE_PORT}`,
`${HA_URL}`, `${LAN_CIDR}`, `${NODE_LAN_IP}`, `${SCHEDULER_PUBLIC_URL}`,
`${BUILD_ID}`, `${ROLLOUT_ID}`. preflight (`home-deploy preflight`) запускается
только при `--execute` как Job `scheduler-preflight` в namespace и проверяет
HA, scheduler и ожидаемый endpoint; на обычном push он не выполняется.

Для Flux: Deployment/PVC/Service и `ExternalSecret` — обычные ресурсы;
`BUILD_ID` подставляется из pins, `ROLLOUT_ID` убирается из pod-template;
`preflight-job.yaml` пока остаётся ручным/admin-инструментом (cutover —
редкая операция).

### 2.3. `ha-bridge`

Тот же скрипт, но с `ONLY=ha-bridge`: применяет `ha-bridge`,
`ha-control`, `ha-scenario` и `Service/ha-control`. Ключевая особенность —
режим исполнения и число реплик являются **live-состоянием**, а не только
манифестом:

- в `bridge.yaml` у `ha-bridge` `replicas: 0`, фактически на стенде — 1;
- `BRIDGE_EXECUTE` и `CONTROL_EXECUTE` переключаются `kubectl set env`;
- `deploy.sh` в preserve-режиме использует `--preserve-env` и
  `--preserve-replicas`, чтобы повторный деплой не остановил исполнитель;
- `--shadow`, `--execute`, `--control-execute`, `--control-catalog` — только
  ручной `workflow_dispatch` для `ha-bridge`, с fail-closed preflight
  (проверка, что очередь `control` пуста);
- регрессия — `projects/home/kubernetes/scheduler/test_deploy_control.sh` и
  `.github/workflows/_deploy-control.yml`.

Для Flux это главный риск: если применить манифест с `replicas: 0` и
`BRIDGE_EXECUTE=false`, Flux остановит живой исполнитель. В плане (§4.2)
предлагается зафиксировать steady-state в Git (`replicas: 1`,
`BRIDGE_EXECUTE=true`, `CONTROL_EXECUTE` — по фактическому состоянию) и
делать переключения режимов через Git/PR либо через `flux suspend` +
admin-скрипт на время учений.

### 2.4. `wakegrant`

`deploy.sh`: один образ, один Deployment (Recreate, 1 реплика), PVC
`wakegrant-data` 1Gi `local-retain`, Service NodePort 30130,
`ExternalSecret/wakegrant`. Проверяет namespace, `ExternalSecret Ready`,
применяет `wakegrant.yaml`, ждёт rollout, выполняет `wakegrant
-healthcheck` (загружает токен и machines, проверяет `/healthz`).
Rollback не документирован — считается fix-forward. Миграций нет.

Это лучший пилот: риск ограничен выдачей wake-грантов, PVC один, нет
зависимости от scheduler/HA-режимов, а проверка adoption PVC — самая
полезная репетиция для остальных stateful-сервисов.

### 2.5. `intercom`

`deploy.sh`: Deployment (Recreate, in-memory состояние) + Service NodePort
30097 + `ExternalSecret/intercom`. Проверяет секрет, применяет, ждёт
rollout, выполняет `intercom -healthcheck`. PVC нет. Ingress-маршрут,
TLS и NetworkPolicy `intercom-ingress` живут в `projects/home/kubernetes/ingress/routes`
и уже управляются Flux. `ExternalSecret/intercom` в live-кластере есть, но
в списке `projects/home/kubernetes/external-secrets/deploy.sh` его нет —
Flux должен взять его под управление вместе с остальными.

### 2.6. `currency-bot`

`deploy.sh` (namespace `currency-bot`):

- проверяет `WEBHOOK_URL` (`https://.../telegram/webhook`), `LAN_CIDR`,
  `NODE_LAN_IP`, NodePort;
- рендерит `currency-bot.yaml`, `cloudflared.yaml`, `network-policies.yaml`,
  `servicemonitor.yaml` (`CLOUDFLARED_IMAGE` берётся из `versions.sh`);
- проверяет `ExternalSecret/currency-bot` и `cloudflared-tunnel`, применяет
  NetworkPolicy и ServiceMonitor, затем converge двух Deployment;
- на падении откатывает записанные Deployment;
- smoke в CI: `/readyz` через API-server Service proxy и готовые endpoints
  `currency-bot-webhook`.

PV/PVC один, SQLite. Внешние зависимости: Cloudflare Tunnel (маршрут
настраивается в Cloudflare вручную) и host nft-фильтр NodePort
(`projects/home/kubernetes/currency-bot/nft/`). Namespace — cluster-scoped
объект; в плане он остаётся admin-owned (Flux его не создаёт и не удаляет).

### 2.7. `mcp-gateway`

`deploy.sh`: Deployment + ClusterIP Service, 2 NetworkPolicy,
ServiceMonitor, ConfigMap `mcp-gateway-catalog` (собирается из отрендеренных
`catalog.yaml` и `bb-targets.yaml`), проверка ServiceAccount и пяти
`ExternalSecret`. `catalog.yaml`/`bb-targets.yaml` содержат приватные
адреса (`HAOS_IP`, `STATION_IP`, `BB_CT_IP`) и id личных проектов; они
рендерятся из `mcp_gateway_env`. PVC нет. RBAC приложения —
`manifests/rbac.yaml`; `ci/rbac.yaml` нужен только `ci-deploy` и после
миграции удаляется.

Для Flux: каталог и bb-targets удобно оформить `configMapGenerator` с
`disableNameSuffixHash: true` (Deployment монтирует ConfigMap по
стабильному имени), а `${...}` подставить через `postBuild`.

### 2.8. `hermes`

`projects/home-assistant/deploy/rollout.sh` — тонкая обёртка над
`home-deploy hermes rollout`. Логика:

- читает `runtime-values.yaml` (OpenBao `runtime_values`, без токенов);
- если digest'ы live-подов совпадают с собранными — ничего не делает;
- делает `helm upgrade runtime` с `--set` образов (без `--reuse-values`),
  вырезает `storage` (PV/PVC — отдельно, администратором), переводит
  exporter на distroless-образ и убирает старый volume/script;
- запоминает ревизию, проверяет rollout, health exporter (все endpoint'ы
  `1`), Assist через `conversation.hermes_agent`, `voice-proxy -healthcheck`
  и логи; при падении — `helm rollback runtime <revision>`.

`hermes`, `piper`, `whisper`, `gigaam` работают с `hostNetwork: true`;
PVC/PV имеют `helm.sh/resource-policy: keep` и применяются вне релиза.
Это последний и самый сложный шаг; для него нужен отдельный HelmRelease
(§4.3) и сохранение smoke/отката через Helm hook.

### 2.9. Вне перевода

`native-release` (Pulsar, Windows logon provider) и `deploy-station`
(установка прошивки Pulsar через HA) не применяют объекты в K3s. Их
оставляем в CI; `deploy-station` продолжает использовать VPN-группу `ci`
(сертификат `ci-pulsar`) и `443` до K3s/Traefik.

## 3. Доставка версии образа в Git

### 3.1. Требования

Схема должна сохранять текущие свойства:

- **build once** (DECISIONS 36): деплой не собирает и не пушит образы;
- **immutable digest**: в кластер попадает `@sha256:...`, а не плавающий тег;
- **provenance**: digest взят из `tree-<hash>` коммита, чья сборка
  подтверждена аттестацией этого репозитория;
- **порядок и «последняя версия»**: понятно, какой digest новее, без
  сортировки случайных хешей;
- **откат**: возврат на предыдущий digest без пересборки;
- **защита `develop`**: Flux не получает права писать в основную ветку;
- **минимум компонентов**: не добавлять контроллеры и долгоживущие ключи без
  необходимости.

### 3.2. Вариант A: Flux image-reflector/automation

Потребуется установить `image-reflector-controller` и
`image-automation-controller` (сейчас их нет; тест
`projects/home/kubernetes/flux/test_flux.py` прямо запрещает их появление),
создать `ImageRepository` на каждый образ, `ImagePolicy` и
`ImageUpdateAutomation`.

Проблема тегов: `tree-<40hex>` — хеш содержимого, `<commit>` — sha коммита,
оба неупорядочены; `develop` — один движущийся тег, по которому нельзя
выбрать «следующую» версию. `ImagePolicy` умеет сортировать semver,
numerical или alphabetical, но не «время сборки». Пришлось бы ввести новый
монотонный тег, например `build-<YYYYMMDDHHMMSS>-<tree12>`, и ставить его
при промоушене через `crane tag` (это перетегирование, не пересборка, так что
build once сохраняется).

Даже с монотонным тегом остаются минусы:

- `ImageUpdateAutomation` пишет в Git **тег**, а не digest; тег остаётся
  изменяемым и может быть удалён/перезаписан, тогда как digest-пин
  воспроизводим;
- нужен write-доступ к Git (ключ/бот) и отдельная ветка или PR-процесс;
  проблема защиты `develop` не исчезает;
- добавляются два контроллера и CRD на один домашний узел, а также
  периодическое сканирование registry по всем тегам;
- обновление манифестов требует setter-комментариев
  (`# {"$imagepolicy": ...}`) в каждом Deployment, то есть правки всех
  шаблонов;
- в итоге всё равно нужна машинная ветка с digest'ами, потому что тег в
  Git — не то же самое, что проверенный digest.

Вывод: вариант A не даёт выигрыша для этой задачи и противоречит
действующему решению «только три контроллера».

### 3.3. Вариант B: CI коммитит digest, Flux читает

CI после merge (и после ручного `workflow_dispatch`) выполняет ту же
проверку, что и сейчас: `promote.mjs` резолвит `tree-<hash>`, проверяет
provenance, при необходимости использует PR-head fallback и ставит
`<commit>`/`develop` теги (для трассировки и политики очистки GHCR). Затем
вместо `kubectl`/`helm` job записывает digest'ы в маленький файл и коммитит
его в отдельную ветку `deploy/home-apps`.

Пример содержимого (ветка-сирота, только `pins/`):

```yaml
# pins/home-app-images.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: home-app-images
  namespace: flux-system
  annotations:
    proflyder.dev/source-commit: "<40-hex>"
    proflyder.dev/source-tree: "<40-hex>"
    proflyder.dev/verified-at: "2026-11-10T12:00:00Z"
    proflyder.dev/run: "https://github.com/<owner>/<repo>/actions/runs/<id>"
data:
  BUILD_ID: "<commit12>"
  SCHEDULER_IMAGE: "ghcr.io/proflyder/scheduler@sha256:<64-hex>"
  HA_BRIDGE_IMAGE: "ghcr.io/proflyder/ha-bridge@sha256:<64-hex>"
  HA_CONTROL_IMAGE: "ghcr.io/proflyder/ha-control@sha256:<64-hex>"
  HA_SCENARIO_IMAGE: "ghcr.io/proflyder/ha-scenario@sha256:<64-hex>"
  WAKEGRANT_IMAGE: "ghcr.io/proflyder/wakegrant@sha256:<64-hex>"
  INTERCOM_IMAGE: "ghcr.io/proflyder/intercom@sha256:<64-hex>"
  CURRENCY_BOT_IMAGE: "ghcr.io/proflyder/currency-bot@sha256:<64-hex>"
  MCP_GATEWAY_IMAGE: "ghcr.io/proflyder/mcp-gateway@sha256:<64-hex>"
  HERMES_IMAGE: "ghcr.io/proflyder/hermes@sha256:<64-hex>"
  VOICE_PROXY_IMAGE: "ghcr.io/proflyder/voice-proxy@sha256:<64-hex>"
  HEALTH_EXPORTER_IMAGE: "ghcr.io/proflyder/health-exporter@sha256:<64-hex>"
  HOME_DEPLOY_IMAGE: "ghcr.io/proflyder/home-deploy@sha256:<64-hex>"
```

Flux читает эту ветку отдельным `GitRepository/flux-pins` и применяет
ConfigMap Kustomization'ом `home-app-pins`. Дальше манифесты приложений
остаются в `develop`, а Kustomization'ы приложений подставляют digest'ы:

```yaml
apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: app-wakegrant
  namespace: flux-system
spec:
  interval: 1m
  retryInterval: 1m
  timeout: 5m
  sourceRef:
    kind: GitRepository
    name: flux-system
  path: ./projects/home/kubernetes/wakegrant/manifests
  prune: true
  wait: true
  dependsOn:
    - name: platform
    - name: home-app-pins
  postBuild:
    substituteFrom:
      - kind: ConfigMap
        name: home-app-images
      - kind: Secret
        name: flux-app-settings
  healthChecks:
    - apiVersion: apps/v1
      kind: Deployment
      name: wakegrant
      namespace: home-assistant
```

`BUILD_ID` — обычный ключ pins-ConfigMap, поэтому подставляется напрямую.
`ROLLOUT_ID` лучше убрать из pod-template: Flux и так применяет новый
manifest при изменении конфига, а новый образ меняет `image` и запускает
rollout. Если annotation нужна для трассировки, в манифесте можно
использовать образ юнита, например `${WAKEGRANT_IMAGE}`: тогда она меняется
только вместе с образом.

Теги при B: `tree-<hash>` остаётся идентичностью сборки, `<commit>` и
`develop` ставит промоушен (трассировка и защита от `ghcr-cleanup`). При
желании можно дополнительно ставить читаемый `build-<UTC>-<tree12>` — это
только перетегирование того же digest, без пересборки; Flux его не
использует, потому что применяет digest. Тег `develop` у каждого образа —
движущийся; источником истины для кластера является pins-файл, а не тег.

Свойства схемы:

- **digest, не тег**: Flux всегда применяет `@sha256:...`;
- **develop не трогается**: машинная ветка отдельная, `develop` остаётся
  PR-only, read-only deploy key Flux не меняется;
- **права CI**: достаточно `permissions: contents: write` у job и
  `GITHUB_TOKEN`; долгоживущий ключ не нужен. `GITHUB_TOKEN`-push не
  запускает другие workflow, а ветка `deploy/home-apps` — не код, а
  машинное состояние. Если политика GitHub не разрешает push от
  `github-actions[bot]`, используем отдельный GitHub App с
  `contents: write` или write deploy key в OpenBao; правило то же — токен
  не имеет доступа к `develop`;
- **защита ветки**: ruleset на `deploy/home-apps` запрещает force-push и
  удаление, разрешает push только боту; `develop` не меняется;
- **fail closed**: если хотя бы один обязательный образ не найден или
  provenance не проверен, его ключ не обновляется, job падает; остальные
  юниты могут обновиться (как сейчас в матрице);
- **частичное обновление**: job читает текущий pins-файл из ветки, меняет
  только ключи затронутых юнитов, коммитит один раз;
- **трассировка**: аннотации содержат исходный commit/tree, время проверки
  и ссылку на run; commit tag/develop tag на digest остаются для
  `ghcr-cleanup` (он защищает версии с commit-тегом);
- **ручной прогон**: `workflow_dispatch` с `units=...` пересчитывает digest
  по истории/PR-head так же, как сейчас, и перезаписывает pins;
- **откат**: revert коммита в `deploy/home-apps` (или `workflow_dispatch`
  `rollback` с повторной проверкой старого tree) → Flux применяет прежний
  digest.

Минусы: появляется машинная ветка и job, который должен иметь write;
pins-файл не проходит человеческое ревью (но он и не должен — это
проверенный CI digest). Компенсация: provenance-проверка до записи,
ruleset, аудит коммитов, аварийный revert.

### 3.4. Сравнение

| Критерий | A: image automation | B: CI → digest в ветку |
|---|---|---|
| Новые контроллеры | +2 (image-reflector, image-automation) | 0 |
| Иммутабельность | тег (изменяемый) | digest |
| Сортировка версий | нужен новый монотонный тег | не нужна: CI пишет актуальный digest |
| Права на Git | write-ключ/бот + ветка | `GITHUB_TOKEN`/бот + ветка |
| `develop` | защищён, но нужен write-путь | защищён, write-путь только в машинную ветку |
| Build once | да (retag) | да (digest из PR-сборки) |
| Откат | смена тега/коммит | revert digest-коммита |
| Сложность | выше | ниже |
| Соответствие DECISIONS 53 | нет (ломает «три контроллера») | да |

**Рекомендация: вариант B.**

### 3.5. Детали реализации B

- В `platform/ci/units.mjs` добавить `deploy.runtime: "ci" | "flux"`
  (по умолчанию `ci` на время миграции). `deploy-home` фильтрует
  `runtime === "ci"`, новый job `pins` — `runtime === "flux"`.
- `platform/ci/check_units.mjs` валидирует значение; `affected.mjs`
  продолжает использовать `deploy.images` для сборки и промоушена.
- Job `pins` (push и dispatch) не использует VPN, OpenBao, `kubectl` и
  `helm`: только `node`, `git`, `crane`, `gh` и `promote.mjs`. Он получает
  `packages: write` (теги) и `attestations: read` (provenance).
- Для каждого затронутого юнита вызывается существующий промоушен; при
  успехе digest'ы попадают в общий pins-файл. Запись — один commit:
  `chore(pins): <unit> <commit12>`.
- Ветка `deploy/home-apps` создаётся один раз (phase 0) с текущими live
  digest'ами, чтобы приложения могли стартовать до первого нового деплоя.
- `GitRepository/flux-pins` и `Kustomization/home-app-pins` добавляются в
  `projects/home/kubernetes/flux/cluster/apps.yaml`; `home-app-pins`
  зависит от `platform` (ESO) и имеет `prune: true`.
- Все app-Kustomization'ы получают `dependsOn: [platform, home-app-pins]` и
  `postBuild.substituteFrom` из `home-app-images` + `flux-app-settings`.
- Тест `test_flux.py` расширяется: разрешён второй `GitRepository`
  (`flux-pins`), проверяется, что app-Kustomization'ы не имеют
  `image-reflector`/`image-automation`, и что pins-ключи покрывают образы
  каждого flux-юнита.
- `ghcr-cleanup` не меняется: digest'ы защищены commit-тегами, которые
  по-прежнему ставит промоушен. Если позже откажемся от тегов, cleanup
  должен читать pins-файл и защищать его digest'ы.

## 4. Preflight, health checks, rollback и миграции

### 4.1. Что даёт Flux

- `Kustomization.spec.wait: true` ждёт готовности применённых объектов
  (Deployment — доступные реплики; Job — завершение, если он в
  `healthChecks`).
- `healthChecks` перечисляет критичные объекты явно; `timeout` ограничивает
  ожидание; `retryInterval` задаёт повтор.
- `prune: true` удаляет объекты, убранные из манифеста (для PVC это опасно,
  см. §4.2).
- `suspend: true` замораживает Kustomization, не удаляя ресурсы.
- HelmRelease умеет `wait`, `timeout` и remediation с откатом на прежнюю
  ревизию.

### 4.2. Что теряется и чем заменяется

| Сейчас в `deploy.sh` | После Flux |
|---|---|
| `kubectl rollout status` | `wait: true` + `healthChecks` Deployment |
| `exec ... -healthcheck` (загрузка конфига/токенов) | readiness/startup probe + отдельный smoke Job (для критичных юнитов) |
| `home-deploy state rollback` на падении job | `Kustomization` становится `Ready=False`, алерт; откат — revert pins/manifest-коммита |
| `helm rollback` при падении smoke | HelmRelease remediation (`strategy: rollback`) + post-upgrade hook (Hermes) |
| `--preserve-env`/`--preserve-replicas` | steady-state режимов и реплик фиксируется в Git; на время учений — `flux suspend` + admin-скрипт |
| cutover preflight scheduler | остаётся ручным admin-Job (или отдельная Kustomization с Job, см. §4.3) |

Важно: Flux не делает автоматический откат при неудачном rollout. Если
новый образ не стартует, Kustomization останется `Ready=False`, а сервис —
недоступным до revert. Для single-replica SQLite-сервисов это неизбежно:
`Recreate` не даёт держать две версии одновременно. Компенсация:

- интервал app-Kustomization 1m и отдельного `GitRepository/flux-pins` 1m,
  чтобы revert применялся за 1–2 минуты;
- алерты `FluxKustomizationNotReady`, `HelmReleaseNotReady`,
  `PinsStale` (pins не обновлялись, хотя `develop` ушёл вперёд);
- для критичных юнитов (`scheduler`, `hermes`) — опциональный
  «кандидат → подтверждение»: CI пишет digest в `*_CANDIDATE`, человек
  переносит его в основной ключ (PR/ручной dispatch). Это медленнее, но
  сохраняет контроль.

PVC: Flux применяет `scheduler-data`, `ha-bridge-data`, `ha-scenario-data`,
`wakegrant-data`, `currency-bot-data`. Чтобы `prune` не удалил том при
случайном удалении манифеста, на PVC ставится аннотация
`kustomize.toolkit.fluxcd.io/prune: disabled`. Для PV Hermes и
`ricardo-existing` владельцем остаётся администратор (`apply-runtime.sh`).

### 4.3. Jobs: миграции, smoke, cutover

Сейчас миграций БД как отдельных шагов нет: SQLite-схема обновляется
процессом при старте. Если появятся, паттерн для Flux:

- отдельная Kustomization `migrate-<service>-<id>` с Job (`backoffLimit: 0`,
  `ttlSecondsAfterFinished`, digest-pinned образ);
- `healthChecks` на Job (condition `Complete`) и `wait: true`;
- app-Kustomization имеет `dependsOn: migrate-...`, поэтому Deployment
  обновится только после успешной миграции;
- имя Job'а содержит id миграции (immutable), старые Job'ы удаляются
  `prune`;
- откат миграции — отдельный Job/PR, а не автоматический.

Smoke для приложений:

- обычные HTTP-сервисы — readiness/liveness probes (уже есть) + при
  необходимости отдельный Job, имя которого содержит digest (тогда каждая
  новая версия получает новый Job);
- Hermes — post-upgrade Helm hook, который выполняет факторизованную
  проверку (существующий `home-deploy hermes assist-smoke` +
  `voice-proxy -healthcheck` + проверка exporter). Падение hook'а валит
  upgrade, и `remediation.strategy: rollback` возвращает прежнюю ревизию.
  Это ближайший аналог текущего `home-deploy hermes rollout`.

Cutover `ha-bridge`/`ha-control`:

- режимы (`shadow`, `execute`, `control-catalog`, `control-execute`)
  остаются ручными. Предпочтительный путь: steady-state в Git, а
  переключение — PR + `flux suspend` на время учения; preflight Job
  запускается администратором (`mise run admin:*`) или отдельной
  Kustomization с `dependsOn`.
- `test_deploy_control.sh` и `_deploy-control.yml` сохраняются, пока
  `deploy.sh` остаётся admin-инструментом. После перевода режимов в Git
  регрессия переносится на рендер Flux-манифестов.

**Flagger не нужен.** На одном узле, с одной репликой, `Recreate` и
SQLite-единственным писателем канареечный трафик невозможен без второго
экземпляра и внешнего состояния. Helm remediation для Hermes и
health-check + revert для остальных покрывают реальные сценарии.

### 4.4. Учения по откату

Для каждого юнита до перевода:

1. Зафиксировать текущий digest и `kubectl get ... -o yaml` (read-only).
2. Перевести, дождаться `Ready`.
3. Сделать «плохой» pins-коммит на предыдущий/заведомо неверный digest в
   тестовом окне (или `flux suspend` + ручной откат).
4. Убедиться, что revert возвращает рабочий pod и метрики.
5. Записать время восстановления и команды в `docs/OPERATIONS.md`.

Не использовать `kubectl rollout undo` как штатный путь: он обходит Git и
теряет аудит. Исключение — аварийное восстановление, после которого
состояние всё равно приводится к Git.

## 5. Секреты

### 5.1. Что CI пишет сейчас

CI не пишет значения секретов в Git и не создаёт application Secrets: он
материализует файлы `0600` из OpenBao и передаёт их `deploy.sh`. После
перевода эти файлы не нужны.

| Ключ `secret/ci/deploy-home` | Поля | Куда переезжает |
|---|---|---|
| `kubeconfig` | доступ `ci-deploy` | не нужен: Flux работает in-cluster |
| `scheduler_env` | `HA_URL`, `LAN_CIDR`, `NODE_LAN_IP`, `SCHEDULER_NODE_PORT`, `SCHEDULER_PUBLIC_URL`, `CONTROL_NODE_PORT` | `secret/k8s/flux-system/app-settings` → `Secret/flux-app-settings` |
| `wakegrant_env` | `WAKE_GRANT_NODE_PORT` | туда же |
| `intercom_env` | `INTERCOM_NODE_PORT` | туда же |
| `currency_bot_env` | `WEBHOOK_URL`, `LAN_CIDR`, `NODE_LAN_IP`, `CURRENCY_BOT_NODE_PORT` | туда же |
| `mcp_gateway_env` | `LAN_CIDR`, `NODE_LAN_IP`, `HAOS_IP`, `STATION_IP`, `BB_CT_IP`, `BB_PROJECT_STATION_LAB`, `BB_PROJECT_ENGLISH_NOTES`, `BB_DISPATCHER_THREAD_ID` | туда же |
| `runtime_values` | приватные Helm values Hermes (без токенов) | `secret/k8s/flux-system/runtime-values` → `Secret/runtime-values` в `flux-system` (ESO) |

Application Secrets уже живут в OpenBao и создаются ESO:

| Secret | OpenBao | Потребитель |
|---|---|---|
| `scheduler-auth`, `ha-bridge-ha` | `secret/k8s/home-assistant/{scheduler-auth,ha-bridge-ha}` | scheduler, ha-bridge/control/scenario |
| `wakegrant` | `secret/k8s/home-assistant/wakegrant` | wakegrant |
| `intercom` | `secret/k8s/home-assistant/intercom` (+ `scheduler-auth`) | intercom |
| `hermes-environment` | `secret/k8s/home-assistant/hermes-environment` | Hermes |
| `mcp-gateway-{auth,telegram,ha,bb-ssh,voice}` | `secret/k8s/home-assistant/mcp-gateway-*` | mcp-gateway |
| `currency-bot`, `cloudflared-tunnel` | `secret/k8s/currency-bot/*` | currency-bot |

### 5.2. Целевая схема

1. `secret/k8s/flux-system/app-settings` — новая запись OpenBao с приватными
   env-значениями приложений (те же поля, что в таблице выше). ESO создаёт
   `Secret/flux-app-settings` в `flux-system` рядом с `flux-settings`.
   Разделение нужно, чтобы ротация/чтение app-настроек не смешивалось с
   инфраструктурными (`NEXUS_HOST`, `HOME_DOMAIN`, `HAOS_ADDRESS`).
2. `secret/k8s/flux-system/runtime-values` — приватные Helm values Hermes.
   ESO создаёт `Secret/runtime-values` в `flux-system`; `HelmRelease/runtime`
   (тоже в `flux-system`, с `targetNamespace`/`storageNamespace:
   home-assistant`) читает их через `valuesFrom`: helm-controller ищет
   `valuesFrom` только в namespace самого HelmRelease.
3. `ExternalSecret` для приложений (`scheduler`, `wakegrant`, `intercom`,
   `currency-bot`, `mcp-gateway`, `hermes-environment`, `runtime-values`)
   включаются в соответствующие Flux-Kustomization'ы. Отдельный ручной
   `external-secrets/deploy.sh` для приложений больше не нужен; его можно
   оставить только как аварийный инструмент.
4. `postBuild.substituteFrom` подставляет `flux-app-settings` и
   `home-app-images`; значения в манифестах остаются `${...}`.
5. Политика ESO `k8s-read` уже покрывает `secret/data/k8s/*`; новые пути
   (`k8s/flux-system/app-settings`, `k8s/flux-system/runtime-values`)
   попадают под неё без изменения политики.
6. После финала: удалить `secret/ci/deploy-home`, политику `ci-deploy` и
   роль `deploy-home` в OpenBao (admin-действие владельца), обновить
   `platform/workstation/catalog.json`, `docs/SECRETS.md`, `docs/STATE.md`.
   `secret/ci/deploy-station` и роль `deploy-station` остаются.

Ограничения:

- значения секретов никогда не попадают в Git, в pins-ветку и в отчёты;
- `runtime_values` не содержит токенов (они в Kubernetes Secrets), но
  содержит приватные адреса — поэтому остаётся в OpenBao, а не в Git;
- `app-settings` не должен содержать ничего, что нельзя прочитать
  kustomize-controller'у (он читает Secret в `flux-system`).

## 6. План миграции

### 6.0. Подготовка (без изменения поведения)

1. Создать ветку `deploy/home-apps` (orphan) с `pins/kustomization.yaml`
   и `pins/home-app-images.yaml`, заполненным текущими live digest'ами
   (снять read-only `kubectl get deploy -o jsonpath=...`). Это стартовое
   состояние, чтобы приложения не ждали первого деплоя.
2. Добавить `GitRepository/flux-pins` (read-only ключ `flux-system`) и
   `Kustomization/home-app-pins` (`path: ./pins`, `prune: true`).
3. Добавить `apps.yaml` в `flux/cluster` и app-Kustomization'ы в состоянии
   `suspend: true` (сначала только wakegrant; остальные — по мере миграции).
4. Создать `secret/k8s/flux-system/app-settings` и
   `ExternalSecret/flux-app-settings`; проверить `Ready`.
5. Добавить `deploy.runtime` в `units.mjs`/`check_units.mjs`; job `pins`
   пишет pins для `flux`-юнитов. Пока все юниты `ci`, job только проверяет
   сборку (dry-run).
6. Добавить алерты `FluxKustomizationNotReady`, `HelmReleaseNotReady`,
   `PinsStale` и дашборд (по желанию).
7. Обновить `test_flux.py` (второй GitRepository, app-Kustomization'ы,
   pins-ключи).

Критерий: `deploy-home` ведёт себя как раньше, Flux ничего не применяет
(`suspend: true`), тесты зелёные.

### 6.1. Пилот: `wakegrant`

Почему он: один Deployment + PVC + Service + один `ExternalSecret`, нет
режимов, нет cross-service зависимостей, ограниченный blast radius. Он же
проверяет adoption PVC — самый рискованный общий элемент.

Шаги:

1. Добавить `kustomization.yaml` в
   `projects/home/kubernetes/wakegrant/manifests` (Deployment, PVC, Service,
   ExternalSecret) и `Kustomization/app-wakegrant` (`suspend: true`).
2. Снять live-объекты (read-only) и сравнить `kubectl kustomize` +
   `kubectl diff` (admin, вне CI): ожидается изменение только annotation
   `BUILD_ID` (и удаление `ROLLOUT_ID`, если он убирается из шаблона);
   PVC/PV и `volumeName` не меняются.
3. Поставить на PVC аннотацию `kustomize.toolkit.fluxcd.io/prune: disabled`.
4. `suspend: false`, `flux reconcile`; проверить `Ready=True`, pod, NodePort
   `/healthz`, `wakegrant -healthcheck`, отсутствие рестартов.
5. Проверить, что Secret не пересоздан (тот же UID) и ESO `Ready`.
6. Проверить drift: изменить annotation/лейбл вручную, reconcile, вернуть.
7. В `units.mjs` поставить `wakegrant.deploy.runtime: "flux"`; убедиться,
   что `deploy-home` больше не выбирает его, а `pins` обновляет digest.
8. Сделать тестовый digest-деплой (следующий реальный merge или
   `workflow_dispatch`) и убедиться, что Flux применил его без CI-доступа.
9. Провести учение по откату (revert pins на предыдущий digest).

Откат шага: `flux suspend kustomization app-wakegrant` (ресурсы остаются),
вернуть `runtime: "ci"`, выполнить `deploy.sh` с предыдущего коммита.
Удалять Kustomization с `prune: true` нельзя — это удалит workload и PVC.

Критерии готовности: 7 дней без деплоя из CI, Flux `Ready`, digest
совпадает с pins, PVC тот же, алертов нет, rollback-учение выполнено.

### 6.2. `intercom`

Проще пилота (нет PVC), но проверяет app без состояния и зависимость от
`ingress/routes`. Шаги аналогичны: kustomization + `suspend`, diff, reconcile,
`runtime: "flux"`, smoke `-healthcheck`, откат. `ExternalSecret/intercom`
включается в Flux (сейчас он не входит в `external-secrets/deploy.sh`).
Критерий: звонок/сигналинг работает, NodePort и ingress не изменились.

### 6.3. `currency-bot`

Проверяет namespace, два Deployment, NetworkPolicy, ServiceMonitor,
Cloudflare Tunnel и PVC. Namespace остаётся admin-owned (не включать
`namespace.yaml` в Flux-путь). Шаги:

1. `kustomization.yaml` (PVC, два Deployment, Services, NetworkPolicy,
   ServiceMonitor, ExternalSecrets); `CLOUDFLARED_IMAGE` заменить на
   литерал из `versions.sh` (или добавить ключ в `app-settings`); добавить
   тест соответствия `versions.sh`.
2. `suspend`, diff, reconcile; проверить `/readyz`, webhook endpoint,
   `cloudflared` `/ready`, ServiceMonitor `up=1`.
3. `runtime: "flux"`, следующий деплой, откат.

Вне Flux остаются Cloudflare-маршрут туннеля и host nft-фильтр NodePort —
это ручные операции, как и сейчас.

### 6.4. `mcp-gateway`

Проверяет `configMapGenerator` и RBAC. Шаги:

1. `kustomization.yaml` с `configMapGenerator` (`catalog.yaml`,
   `bb-targets.yaml`, `disableNameSuffixHash: true`), Deployment, Service,
   NetworkPolicy, ServiceMonitor, ExternalSecrets, `rbac.yaml`.
2. `suspend`, diff: ConfigMap должен совпасть с live по содержимому;
   Deployment — только аннотации.
3. reconcile; проверить `/readyz`, `-healthcheck`, каталог, доступ к
   Kubernetes API (RBAC), bb-dispatch (без реального запуска).
4. `runtime: "flux"`, следующий деплой, откат.

`ci/rbac.yaml` в Flux не включается; после финала удаляется вместе с
`ci-deploy`.

### 6.5. `scheduler` + `ha-bridge` (executors)

Это одна группа шаблонов и один скрипт, поэтому на первом шаге — одна
Kustomization `app-ha-executors` с четырьмя workload'ами (scheduler,
ha-bridge, ha-control, ha-scenario). Разделять её на две имеет смысл только
отдельным рефакторингом каталогов `manifests/`; для миграции это не
требуется.

Порядок:

1. Зафиксировать live-режимы: `BRIDGE_EXECUTE`, `CONTROL_EXECUTE`,
   replicas, образы; записать в PR/документ (без секретов).
2. Привести манифесты к steady-state: `ha-bridge` `replicas: 1`,
   `BRIDGE_EXECUTE: "true"`, `CONTROL_EXECUTE` — по факту; либо оставить
   `replicas: 0` и управлять режимом только через Git/PR.
3. Добавить kustomization + `suspend`, diff; особое внимание:
   `enableServiceLinks: false`, PVC, NodePort, `HA_URL`, отсутствие
   `--preserve-env`.
4. reconcile в оконном режиме: сначала `scheduler` (он не зависит от
   bridge), затем bridge/control/scenario. Проверить `GetStatus.ready`,
   `-healthcheck`, очередь `control`, отсутствие дублей исполнения.
5. `runtime: "flux"` для обоих юнитов; следующий деплой.
6. Учения: `flux suspend` + `--shadow`/`--execute` admin-скриптом; вернуть
   steady-state; убедиться, что Flux не включает второй executor.

Откат: suspend + вернуть `runtime: "ci"` + `deploy.sh` предыдущего коммита.
Не удалять Kustomization с prune.

Критерий: 14 дней без CI-деплоя, режимы не менялись неявно, будильники и
сценарии работают, очередь `control` пуста, алертов нет.

### 6.6. `hermes`

Самый сложный. Предлагаемая схема:

1. `HelmChart` из Git (`sourceRef: GitRepository/flux-system`,
   `chart: projects/home-assistant/deploy/chart`) и `HelmRelease/runtime`
   в `flux-system` (`releaseName: runtime`, `targetNamespace:
   home-assistant`, `storageNamespace: home-assistant`), чтобы
   helm-controller принял живой релиз как upgrade.
2. `valuesFrom: Secret/runtime-values` в `flux-system` (ESO) +
   `spec.values`/`postRenderers`
   для digest'ов. `postRenderers` с strategic-merge patch по контейнерам
   позволяет подставить `${HERMES_IMAGE}`, `${VOICE_PROXY_IMAGE}`,
   `${HEALTH_EXPORTER_IMAGE}` без перезаписи массивов приватных values.
3. Exporter: убрать runtime-перезапись из `home-deploy`, оформить
   distroless-контейнер в chart (command, пустые volumeMounts) — это
   небольшое изменение chart, поведение сохраняется.
4. Smoke/rollback: post-upgrade Helm hook Job с образом `home-deploy` и
   командами `hermes assist-smoke`/`voice-proxy -healthcheck`;
   `remediation.strategy: rollback` и
   `remediateLastFailure: true`. Падение hook'а → откат на прежнюю ревизию.
5. PV/PVC остаются admin-owned (`apply-runtime.sh`), в chart `storage`
   пустой; Flux не трогает PV.
6. После стабилизации — `runtime: "flux"` для юнита `hermes`, затем убрать
   `rollout.sh`/`home-deploy hermes rollout` из CI-пути, оставив команду для
   ручного аварийного отката.

Откат: HelmRelease remediation; вручную — `helm rollback runtime <rev>` или
revert pins. Критерий: 14 дней, голосовой smoke зелёный, PV не пересоздан,
откат проверен на `--simulate-smoke-failure`-аналоге.

### 6.7. Финал: убрать входящий доступ CI

После того как все семь юнитов живут в Flux и выдержали наблюдение:

1. Удалить из `deploy-home` job `deploy` (VPN, OIDC, kubectl/helm,
   `promote` в виде применения), оставить `plan`, `pins`, `warm-*`,
   `native-release`, `deploy-station`. Workflow можно переименовать в
   `release-home`, но имя менять не обязательно.
2. Удалить `secret/ci/deploy-home`, роль `deploy-home` и политику
   `ci-deploy` в OpenBao (admin, с подтверждением владельца); обновить
   каталог и `docs/SECRETS.md`.
3. Удалить `ci-deploy-token`, Role/RoleBinding `ci-deploy` в
   `home-assistant`, `currency-bot`, `mcp-gateway`; удалить файлы
   `*/ci/rbac.yaml`; проверить `kubectl auth can-i --as=...` — доступа нет.
4. Отозвать сертификат `ci-github` (`vpncerts.py revoke`), убрать из
   `management-firewall/rules.nft` строку
   `ip saddr CI_VPN_SOURCE tcp dport 6443 accept`; оставить `443` для
   `ci-pulsar`/`deploy-station`. Если позже `deploy-station` переедет на
   другой путь, отозвать группу `ci` целиком.
5. Удалить Environment `Home` (или его VPN-секреты), оставить `Station`.
6. Удалить `_deploy-control.yml`/`test_deploy_control.sh` или переписать
   их под проверку Flux-манифестов (решение — после перевода режимов).
7. Обновить `docs/STATE.md`, `docs/OPERATIONS.md`,
   `docs/KUBERNETES-OPERATIONS.md`, `docs/ARCHITECTURE.md`,
   `docs/DECISIONS.md` (новое решение), `projects/home/kubernetes/flux/README.md`.

Откат финала: восстановить `ci/rbac.yaml` из Git, выпустить **новый**
сертификат (отозванный не возвращается), вернуть правило `6443`, создать
`secret/ci/deploy-home` заново, включить job `deploy`. Поэтому финал —
отдельный PR после стабильного наблюдения, а не часть пилота.

### 6.8. Общие критерии готовности

- все app-Kustomization'ы `Ready=True` 14 дней, алертов нет;
- для каждого юнита наблюдался хотя бы один автоматический pins-деплой;
- `deploy-home` не содержит применения в K3s для home-сервисов;
- `ci-deploy` и `ci-github` не используются; `git grep` по активным
  конфигам не находит `ci-deploy` (кроме истории/документации);
- rollback-учение проведено для `wakegrant` и `scheduler`;
- ротация секрета (ESO force-sync) не ломает pod;
- PVC переживает рестарт, PV не пересоздан;
- `mise run audit`, `test_flux.py` и профильные тесты зелёные.

## 7. Риски

| Риск | Влияние | Смягчение |
|---|---|---|
| Flux применяет манифест `ha-bridge` с `replicas: 0`/`BRIDGE_EXECUTE=false` и останавливает живой исполнитель | потеря будильников/сценариев | steady-state в Git, `flux suspend` на время учений, ревью PR, алерт на `AvailableReplicas` |
| SSA-конфликты с объектами, созданными `kubectl apply`/`set env` | reconcile не проходит или меняет drift | `kubectl diff` перед включением, чистка `last-applied`/managedFields в окне, `force: false`; PVC не force-recreate |
| `prune` удаляет PVC/PV при удалении манифеста/Kustomization | потеря данных | аннотация `prune: disabled` на PVC; PV Hermes — admin-owned; не удалять Kustomization с `prune: true` |
| Нет автоматического отката при плохом образе | простой до revert | интервал 1m, алерты, быстрый revert pins; для критичных — кандидат-гейт; rehearsal |
| readiness probe не проверяет токены/конфиг так, как `-healthcheck` | «зелёный» pod с нерабочим конфигом | startup/readiness проверки, smoke Job для критичных, сохранить `-healthcheck` в smoke |
| Pins-ветка скомпрометирована/запушен чужой digest | произвольный образ в кластере | ruleset, только бот, provenance до записи, аудит коммитов, опционально cosign/admission |
| Pins job падает или отстаёт | Flux не получает новую версию; «заморозка» | алерт `PinsStale`, fail closed, ручной dispatch; старый digest продолжает работать |
| Flux/ESO недоступны | приложения не обновляются, но работают | ExternalSecret `deletionPolicy: Retain`, приложения уже запущены; алерты; Flux не единственная точка для текущего трафика |
| `postBuild` подставляет `${...}` в неожиданных местах | испорченные ConfigMap/values | не держать литеральные `${}` в app-манифестах или экранировать `$${`; тест рендера |
| HelmRelease не принимает живой релиз/values | пересоздание или неверные values | `releaseName`/`storageNamespace`, `valuesFrom` + `postRenderers`, dry-run `helm template`/diff до включения |
| Hermes smoke/rollback слабее текущего | плохой релиз остаётся | post-upgrade hook + remediation rollback; ручной `helm rollback` |
| Финал ломает `deploy-station` | нет обновления Pulsar | не отзывать группу `ci`/`443`, пока станция не переведена; отдельный шаг |
| Документация/ops отстают | неверные действия в инциденте | обновить STATE/OPERATIONS/DECISIONS в том же PR; чек-лист |

## 8. Что остаётся вне Flux

- **Pulsar/станция**: сборка и публикация native-артефакта, `deploy-station`
  через HA (`update_entity` + `update.install`), откат `.prev` на станции.
  Для него нужен VPN-сертификат `ci-pulsar` и `443` до K3s/Traefik.
- **Android**: сборка, подпись и Firebase App Distribution — CI, не кластер.
- **HAOS**: HA Core, Music Assistant, Matter Server, automations, secrets и
  настройки add-on'ов живут в HAOS, не в этом репозитории. В Git есть только
  внешний endpoint HA (EndpointSlice) и маршруты.
- **OpenBao**: bootstrap, seal/recovery, raft snapshot, политики — ручные
  admin-действия владельца.
- **Host-конфигурация**: nftables (management-firewall, voice-firewall,
  NodePort-фильтры scheduler/currency-bot/intercom), VPN/ocserv, сертификаты,
  Cloudflare DNS и маршруты туннеля, PVE/K3s host, backup/offsite.
- **Hermes PV/PVC**: применяются `apply-runtime.sh` администратором; Flux
  владеет только Helm-релизом. В будущем можно передать Flux, но это
  отдельное решение.
- **Публикация/релизы**: `native-release`, `warm-android`/`warm-rust`, теги
  GHCR и `ghcr-cleanup` остаются в CI.
- **Аварийные admin-инструменты**: `deploy.sh`, `home-deploy hermes rollout`,
  `mise run admin:*` — сохраняются для ручных операций и отката, но не
  являются штатным путём деплоя.

## 9. Приложения

### 9.1. Порядок и оценка

| Этап | Юнит | Сложность | Ориентир |
|---|---|---|---|
| 1 | `wakegrant` | низкая | 1–2 дня |
| 2 | `intercom` | низкая | 1 день |
| 3 | `currency-bot` | средняя | 2–3 дня |
| 4 | `mcp-gateway` | средняя | 2–3 дня |
| 5 | `scheduler` + executors | высокая | 3–5 дней |
| 6 | `hermes` | высокая | 3–5 дней |
| 7 | финал (CI/RBAC/VPN) | средняя | 1–2 дня |

Оценки — только порядок величины; основное время уйдёт на проверки и
наблюдение, а не на манифесты.

### 9.2. Чек-лист пилота

```text
[ ] pins-ветка создана и содержит текущие digest'ы
[ ] home-app-pins Ready, ConfigMap home-app-images существует
[ ] flux-app-settings Ready
[ ] app-wakegrant suspend=true; kubectl diff чистый (кроме аннотаций)
[ ] PVC имеет prune: disabled; volumeName/UID не меняются
[ ] app-wakegrant suspend=false; Ready=True; pod Ready
[ ] NodePort /healthz и wakegrant -healthcheck
[ ] ESO Secret тот же UID, Ready
[ ] drift-тест: ручная правка откатывается
[ ] runtime: flux; deploy-home не выбирает wakegrant
[ ] следующий digest приехал через pins
[ ] rollback: revert pins → прежний digest работает
[ ] 7 дней наблюдения, алертов нет
```

### 9.3. Команды чтения (без изменений)

```bash
mise run kubectl -- get kustomizations -n flux-system
mise run kubectl -- get gitrepositories -n flux-system
mise run kubectl -- get helmreleases -A
mise run kubectl -- -n home-assistant get deploy,pod,svc,pvc
mise run kubectl -- -n currency-bot get deploy,pod,svc,pvc
mise run kubectl -- get externalsecret -A
```

Перед включением конкретного юнита:

```bash
kubectl kustomize projects/home/kubernetes/<unit>/manifests
kubectl diff -f <(kubectl kustomize projects/home/kubernetes/<unit>/manifests)
```

(`kubectl diff` выполняется admin-доступом в окне обслуживания; это
read-only проверка, не применение.)

### 9.4. Что проверить в реализации (не в этом PR)

- `test_flux.py`: второй `GitRepository`, app-Kustomization'ы, pins-ключи,
  отсутствие image automation.
- `check_units.mjs`: `deploy.runtime` и покрытие pins.
- `deploy-home.yml`: job `pins` без VPN/OpenBao/kubectl; удаление `deploy`.
- `versions.sh` vs `CLOUDFLARED_IMAGE` в манифесте (тест соответствия).
- `ghcr-cleanup`: защита digest'ов из pins (если теги перестанут ставиться).
- Обновление `docs/SECRETS.md`, `docs/STATE.md`, `docs/OPERATIONS.md`,
  `docs/KUBERNETES-OPERATIONS.md`, `docs/DECISIONS.md`.
