# Kubernetes runtime Home Assistant

Chart `chart/` управляет Piper, Whisper, GigaAM, Hermes
(gateway+proxy в одном Pod) и TCP health exporter. HA Core, **Music
Assistant** и **Matter Server** работают отдельно в HAOS как add-ons;
их workload/storage-блоков в приватных values нет (chart остаётся общим и
может отрендерить любые values для rollback-сценария).
Все приложения используют hostNetwork для сохранения loopback и LAN
discovery. Каждый Deployment имеет Recreate; данные сохраняются в Retain
local PV/PVC.

HAProxy `haos-proxy` удалён. HAOS обращается к HA напрямую `.28:8123`;
Hermes voice proxy слушает `.27:8643` и ограничен nftables-правилом:
только HAOS `.28` и сам K3s `.27`. HAOS `internal_url` — `.28:8123`.

Tracked `values.yaml` пуст: реальные images/digests, host paths и параметры
развёртывания находятся в приватных values. Ключи Hermes передаются отдельно
через Kubernetes Secret `hermes-environment`, который создаёт External Secrets
Operator из OpenBao (`secret/k8s/home-assistant/hermes-environment`);
refreshable auth находится на PVC.
Не добавлять реальные private values в Git и не выводить `helm get values`
в публичные отчёты.

## Images

Hermes собирается из `git archive` revision, указанного в
`projects/home/kubernetes/versions.sh`, без home/secrets/старого venv.
Dockerfile находится в `images/hermes/`, лицензия upstream сохранена рядом.
API server требует messaging extra (aiohttp), дополнительно устанавливаются
mcp/web. SQLite 3.53.4 собирается по checksum из Dockerfile того же upstream
revision; tini reaps дочерние процессы. Изменение источника требует нового image
и проверки API + реального модельного запроса, а не одного TCP health.

Voice proxy — Rust-сервис `solutions/voice-proxy`, а не Dockerfile: образ
собирается Bazel `rules_oci` из `//solutions/voice-proxy:voice-proxy_image`
(distroless, без shell и python) и пушится байт-в-байт целью
`//solutions/voice-proxy:voice-proxy_push`; `docker push` переписал бы
media type и digest. Все образы публикуются в GHCR (`ghcr.io/proflyder/...`)
по digest `imagePullPolicy: IfNotPresent`, без pull-secret. Юнит `hermes`
имеет `deploy.builder: mixed`: `hermes` собирается Docker, а
`voice-proxy`, `health-exporter` и `home-deploy` — Bazel (явная карта
`deploy.bazel_images` в `platform/ci/units.mjs`). `deploy-home` только пинует
проверенные digest'ы; Flux (`HelmRelease/runtime`) обновляет релиз `runtime`, а
пост-апгрейд hook проверяет rollout, health exporter,
`voice-proxy -healthcheck`, Assist через Hermes и логи voice-proxy и
откатывается на прежнюю ревизию при провале. Сборка без новых digest'ов не
перезапускает контур.

Hermes-конфиг — отдельный воспроизводимый образ `hermes-config`
(`//projects/home-assistant/hermes:hermes-config_image`,
`ghcr.io/proflyder/hermes-config`): статический `home-deploy` и reviewed-overlay
в `/opt/hermes-overlay` (файлы, `skills/**`). `pins` пишет
`HERMES_CONFIG_IMAGE`; `postRenderers` в `flux/helmrelease.yaml` запускают его
как initContainer `render-config` (пишет `config.yaml`, `.hermes.md`, `SOUL.md`,
skills и prompts на PVC; при ошибке Pod не стартует, Helm откатывает релиз) и
как sidecar `config-drift` (`hermes drift-exporter`, read-only, `:9118`).
Приватные значения рендера — `Secret/hermes-config-private` из OpenBao
`k8s/home-assistant/hermes-config` поле `private.yaml`, монтируется в
`/run/hermes-config`. Chart рендерит opt-in Service `hermes-config-drift`
(`hermesConfigDrift.enabled`, у Flux включён) для ServiceMonitor. Плановый
перезапуск gateway использует preStop drain, поэтому «shutting down» в
Telegram не приходит; ручной render остаётся только аварийным путём.

Health exporter — Rust-бинарник
`health-exporter/crates/health-exporter` (distroless-образ
`//projects/home-assistant/deploy/health-exporter:health-exporter_image`,
`ghcr.io/proflyder/health-exporter`): метрики `up` по endpoint'ам, порт 9116,
цели из переменных `ENDPOINT_HOST_*`. Карта больше не владеет exporter'ом:
`hermes rollout` (и `apply-runtime.sh` через `HEALTH_EXPORTER_IMAGE`) задают
image, command и пустые `volumeMounts`/`volumes` через `helm --set/--set-json`,
поэтому приватные values менять не нужно. Assist smoke Job тоже собран из
`home-deploy`, а не из python-образа: `home-deploy hermes assist-smoke`.

## Voice routing: приватные values

Маршрутизация профиля включается только переменными контейнера
`voice-proxy` (`workloads.hermes.containers[1]`). Пока их нет, прокси работает
как раньше, поэтому новый образ раскатывается безопасно и до настройки
маршрутов.

В `runtime_values` (OpenBao `secret/k8s/flux-system/runtime-values`) нужны адреса и ссылки на
секреты; значения токенов остаются в Kubernetes Secrets:

```yaml
workloads:
  hermes:
    containers:
      - name: gateway   # без изменений
        # ...
      - name: voice-proxy
        env:
          - {name: HERMES_HA_URL, value: "http://192.0.2.28:8123"}
          - name: HERMES_HA_TOKEN
            valueFrom: {secretKeyRef: {name: hermes-environment, key: HERMES_HA_TOKEN}}
          - {name: HERMES_VOICE_GATEWAY_URL,
             value: "http://mcp-gateway.home-assistant.svc.cluster.local:8080"}
          - name: HERMES_VOICE_GATEWAY_TOKEN
            valueFrom: {secretKeyRef: {name: mcp-gateway-voice, key: token}}
          - {name: HERMES_SCHEDULER_URL,
             value: "http://scheduler.home-assistant.svc.cluster.local:8098"}
          - name: HERMES_SCHEDULER_TOKEN
            valueFrom: {secretKeyRef: {name: scheduler-auth, key: token}}
          - {name: HERMES_VOICE_QUEUE, value: "1"}
          - {name: HERMES_VOICE_MEDIA_PLAYER,
             value: "media_player.iandeks_stantsiia_midi_media_player"}
        volumeMounts: []   # *_FILE не нужен, если токены приходят через env
```

`Secret/mcp-gateway-voice` уже создаётся ESO в namespace `home-assistant`;
`Secret/scheduler-auth` (`key: token`) создаётся ESO для scheduler и уже
используется его же healthcheck. `HERMES_HA_TOKEN` уже есть в
`Secret/hermes-environment`. Pod Hermes работает с `hostNetwork`, поэтому
NetworkPolicy на него не действует; scheduler тоже не имеет ingress-policy,
a egress к `mcp-gateway:8080`, `scheduler:8098` и HA `:8123` разрешён. Новых
сетевых правил для очереди не требуется. Имена переменных и их defaults не
менялись при переходе на Rust, поэтому приватным values нужна только одна
правка — readiness/liveness probe контейнера `voice-proxy`:

```yaml
          - name: voice-proxy
            # ...
            readinessProbe:
              tcpSocket: {port: 8643}
```

Exec-probe вида `python3 …` или `sh -c` в distroless-образе не работает;
TCP-проверка порта 8643 — единственный корректный вариант (аналог прежнего
health exporter). Обновляет приватные values оператор; в Git их нет. Полный
контракт переменных — в `solutions/voice-proxy/README.md`. Если понадобится
файловый вариант, используйте `HERMES_HA_TOKEN_FILE`,
`HERMES_VOICE_GATEWAY_TOKEN_FILE` и `HERMES_SCHEDULER_TOKEN_FILE` с тем же
Secret в volume.

## Storage и CI

`apply-runtime.sh` применяет storage (PV/PVC) через `kubectl`, а workloads —
через Helm. Для ручного полного apply нужен digest exporter'а:
`HEALTH_EXPORTER_IMAGE=ghcr.io/proflyder/health-exporter@sha256:…`
(`kubectl -n home-assistant get deployment runtime-health-exporter -o jsonpath='{.spec.template.spec.containers[0].image}'`). PV/PVC помечены `helm.sh/resource-policy: keep` и не входят в
Helm-релиз: их применяет администратор (`apply-runtime.sh`), а Flux владеет
только namespace-scoped workloads. OpenBao
`secret/k8s/flux-system/runtime-values` (поле `values`) содержит приватные
values без токенов (они остаются в Kubernetes Secrets); ESO создаёт
`Secret/runtime-values` для `valuesFrom`. Ручной `hermes rollout` остаётся
аварийным путём.

## Перенос Hermes home

При перемещении state из native home в `/opt/data` старые API binding receipts
сохраняют абсолютный profile_id и дают HTTP 409 `profile_mismatch` для прежних
разговоров. `rebase_hermes_state.py` переносит только структурированные поля
`profile_id` в JSON `state_meta`; не меняет сообщения/auth и оставляет SQLite
backup. Выполнять только при остановленном gateway и с известным старым home.
Не лечить эту ошибку удалением state.db или историй разговоров.

## Проверки

```bash
python3 -m unittest discover -s projects/home-assistant/deploy -p 'test_*.py' -v
bazel test //platform/deploy-rs/...
cargo test -p health-exporter
bazel test //projects/home-assistant/deploy/health-exporter/...
cargo test -p voice-proxy
helm lint projects/home-assistant/deploy/chart
helm lint projects/home-assistant/observability/chart
```

Health exporter проверяет только TCP connect и экспортирует
`home_endpoint_up{endpoint="..."}`; это не измерение end-to-end voice latency.
Полные live проверки — `e2e/kubernetes_runtime.py`, `e2e/wyoming_smoke.py`
и существующие HA/Hermes E2E. Пути приведены относительно проекта home-assistant.
[Эксплуатация](../../../docs/KUBERNETES-OPERATIONS.md).

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `chart/templates/hermes-config-drift.yaml` — Opt-in scrape target for the `config-drift` sidecar. The sidecar runs in the hostNetwork Hermes Pod, so Prometheus reaches it through the Pod IP; this ClusterIP Service only gives the ServiceMonitor a stable port name.
- `chart/templates/runtime.yaml`
  - Storage is applied by apply-runtime.sh with kubectl and deliberately kept out of the Helm release so a CI `helm upgrade` never needs cluster-scoped RBAC. Keep also protects the data if it is still tracked by an older release.
- `chart/templates/smoke-hook.yaml` — Post-upgrade smoke hook (решение 54).
- `flux/external-secret-hermes-config.yaml` — Hermes private render values (real HA/MA endpoints and the Telegram owner chat id). The reviewed overlay itself ships in the `hermes-config` image; this Secret supplies only the values the renderer deep-merges after the fragment.
- `flux/helmrelease.yaml` — Flux entry point for the Hermes runtime Helm release (решение 54).
- `flux/kustomization.yaml` — Manifest set for Kustomization/app-hermes (решение 54).
- `health-exporter/crates/health-exporter/src/lib.rs` — Runtime chart's TCP health exporter.
- `health-exporter/crates/health-exporter/src/main.rs` — Command-line entry point: serve `/metrics`, or fetch it with `-query` for the rollout's exec probe. Everything reusable lives in the library.
- `images/build-and-push.sh` — Build and push the Hermes gateway and voice-proxy images to GHCR.
- `images/hermes/Dockerfile` — Debian 13 still ships SQLite 3.46.1, which contains the upstream WAL-reset corruption bug. Build a pinned shared library for the runtime image instead of relying on a distro backport that trixie does not currently provide. See #70480 and https://sqlite.org/wal.html#walresetbug.
  - apt, dpkg, alternatives and fontconfig keep wall-clock state; leaving their logs/caches in the layer would make every build a different image.
  - Build context is a git archive of the pinned, reviewed Hermes revision, never HOME.
- `images/hermes/build.sh` — Build the Hermes gateway image reproducibly (DECISIONS 36).
- `images/hermes/verify-reproducible.sh` — Prove the Hermes image is reproducible (DECISIONS 36).
- `manifests/external-secrets.yaml` — Hermes environment (API key, HA/MA tokens and runtime flags).
- `rebase_hermes_state.py` — Rebase persisted Hermes profile references after relocating HERMES_HOME.
- `rollout.sh` — Deploy entry point for the `hermes` unit (deploy-home.yml matrix `script`).
- `test_build_and_push.py` — build-and-push.sh: the prebuilt-digest path used by deploy-home (DECISIONS 36).
- `test_voice_routing_contract.py` — Contract for the private voice-routing values of the Hermes Deployment.
