# Kubernetes runtime Home Assistant

Chart `chart/` управляет MA, Matter, Piper, Whisper, GigaAM, Hermes
(gateway+proxy в одном Pod), TCP health exporter и hostNetwork HAProxy
`haos-proxy`. HA Core сейчас работает отдельно в HAOS VM; HA workload в
чарте оставлен только как legacy-опция. Все приложения используют
hostNetwork для сохранения loopback и LAN discovery. Каждый Deployment
имеет Recreate; данные сохраняются в Retain local PV/PVC.

`haProxy.enabled` включает HAProxy `192.0.2.27:8123` → HAOS
`192.0.2.28:8123`; `haProxy.privateBridge` открывает `.27:8643` (voice proxy)
и `.27:5580` (Matter) только для HAOS `/32`. Реальные upstream/ACL — в
private values. HAOS `internal_url` и trusted proxy должны указывать на
`.27`, а не на старый host loopback.

Tracked `values.yaml` пуст: реальные images/digests, host paths и параметры
развёртывания находятся в приватных values. Ключи Hermes передаются отдельно
через Kubernetes Secret `hermes-environment`; refreshable auth находится на PVC.
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

Proxy image собирается из Dockerfile `images/voice-proxy/` и актуального
`projects/home-assistant/hermes/voice_proxy.py` в build context.
Локальные образы импортированы в K3s containerd, `imagePullPolicy: Never`.
Их OCI archive сохранён в закрытом recovery-комплекте; они не опубликованы
в registry. На новом узле архив необходимо импортировать до запуска Pod.

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
helm lint projects/home-assistant/deploy/chart
helm lint projects/home-assistant/observability/chart
```

Health exporter проверяет только TCP connect и экспортирует
`ricardo_endpoint_up{endpoint="..."}`; это не измерение end-to-end voice latency.
Полные live проверки — `e2e/kubernetes_runtime.py`, `e2e/wyoming_smoke.py`
и существующие HA/Hermes E2E. Пути приведены относительно проекта home-assistant.
[Эксплуатация](../../../docs/KUBERNETES-OPERATIONS.md).
