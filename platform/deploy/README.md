# platform/deploy

Общие шаблоны развёртывания и `resolve.sh`. Сам CLI `home-deploy` — Rust-крейт
[`platform/deploy-rs`](../deploy-rs/README.md); Go-реализация удалена, здесь
остались только точка входа скриптов и общие Kubernetes-манифесты.

```text
resolve.sh         находит и запускает Rust-бинарник из deploy-скриптов
kubernetes/        общие cluster-scoped манифесты (storage classes)
```

## Вызов

Скрипты юнитов подключают `resolve.sh` и вызывают функцию `home_deploy`:

```bash
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck source=platform/deploy/resolve.sh
source "$ROOT/platform/deploy/resolve.sh"
home_deploy render --unit scheduler --env "$PRIVATE_ENV" --out "$RENDER_DIR" \
  --set "BUILD_ID=$BUILD_ID" --rollout-key ROLLOUT_ID manifests/scheduler.yaml
```

Порядок поиска бинарника: `$HOME_DEPLOY` → `home-deploy` в `PATH` →
`bazel-bin/platform/deploy-rs/home-deploy` после `bazel build` →
`bazel run //platform/deploy-rs:home-deploy --` (Bazelisk — из `mise install`).

`deploy-home` не компилирует инструмент: он достаёт бинарник из промоутнутого
OCI-образа `ghcr.io/proflyder/home-deploy` (`crane export`, путь
`/usr/local/bin/home-deploy`) и экспортирует `HOME_DEPLOY`. Контракт — образ,
а не язык: сейчас его собирает `rust_service_image` из
`//platform/deploy-rs:home-deploy`.
`DEPLOY_NO_BUILD=1` в CI также запрещает запасные пути `bazel-bin` и `bazel
run`. На node, где запускается `nft/apply.sh`, нужен уже установленный бинарник
(например `/usr/local/bin/home-deploy`) или `HOME_DEPLOY`.

## Команды

| Команда | Назначение |
|---|---|
| `render --unit U --env F --out D [--set K=V]... [--rollout-key K] файлы...` | подставляет env в манифесты юнита; отказ на пустой или неизвестный placeholder. `--rollout-key` заполняет K первыми 12 hex sha256 всего отрендеренного юнита, маскируя `BUILD_ID` и сам K |
| `env get FILE KEY` | читает одно значение из env-файла |
| `build-id --revision R --fingerprint STR` | `<revision>-<12 hex sha256(fingerprint)>`; только для трассировки (Deployment-аннотация), не для pod template |
| `oci digest LAYOUT` | digest OCI-layout |
| `oci pack LAYOUT --ref R --out F` | tar OCI-layout без перепаковки |
| `oci docker LAYOUT --ref R --out F` | архив для `docker load` из layout |
| `converge --kubectl K --namespace NS --name N --file F [--preserve-replicas] [--preserve-env] [--summary-file S]` | заменяет живой Deployment отрендеренным, убирая drift (см. ниже); `--summary-file` пишет `<name>\t<status>` |
| `state record --state-dir D --namespace NS --name N [--kubectl K]` | сохраняет объект перед изменением (`.before.json` / `.absent`) |
| `state mark --state-dir D --name N` | помечает объект применённым (`.applied`) |
| `state rollback --state-dir D [--namespace NS] [--kubectl K]` | восстанавливает сохранённые объекты |
| `preflight --ha-url U --ha-token-file F --scheduler-url U --scheduler-token-file F [--expected-endpoint U]` | read-only проверки перед `deploy.sh --execute` |
| `runtime split --render F --private F --storage-out F --values-out F [--health-exporter-image D]` | делит Helm-рендер на storage (kubectl apply) и Helm values |
| `eso names --out F манифесты...` | список ExternalSecret как `<namespace>/<name>` |
| `secret opaque --name N --namespace NS --key K --from-env-file F --env KEY` | JSON одноключевого Opaque Secret (bootstrap) |
| `hermes rollout --values-file F --hermes-digest D --voice-proxy-digest D --exporter-digest D --smoke-image I --state-dir D [...]` | продвигает digest'ы Hermes, voice-proxy и exporter в релиз `runtime` с проверкой и откатом |
| `hermes render-config --home DIR [--private-values F] [--dry-run\|--check]` | сливает `config.fragment.yaml` в живой `HERMES_HOME` |
| `hermes assist-smoke` | один Assist-ход через HA API в кластере (клиент smoke Job) |

`converge`: support objects (Service/PVC)
применяются через `apply`, отсутствующий Deployment создаётся, существующий
заменяется через `replace` с повторным использованием `resourceVersion`.
`--preserve-replicas` и `--preserve-env` переносят ручной режим/масштаб;
остальные поля и drift удаляются. Тест сравнивает argv и разобранные
JSON-payload'ы с эталонными трассами
(`platform/deploy-rs/testdata/frozen/cases/converge-*/trace/`).

`--summary-file` пишет по строке `<name>\t<status>` на каждый Deployment:
`created`, `unchanged` или `rolled-out`. Статус определяется сравнением
`metadata.generation` до и после `replace` (это то же, по чему Kubernetes
создаёт новый ReplicaSet), поэтому rollout не угадывается по сообщению
kubectl. `deploy-home` собирает эти строки в job summary.

### Rollout без перезапуска при неизменном содержимом

Pod template аннотируется `ricardo.dev/rollout: ${ROLLOUT_ID}` —
content-addressed хешем всего, что рендерится для юнита (digest'ы всех
образов, значения из private env, данные ConfigMap, probes, resources).
`BUILD_ID` (`<git rev>-<image fingerprint>`) остаётся только в аннотации
`metadata.annotations.ricardo.dev/build` самого Deployment: смена ревизии
Git видна для трассировки, но не меняет pod template, поэтому
`kubectl apply`/`replace` не создаёт новый ReplicaSet. Изменение образа,
отрендеренного конфига или смонтированного ConfigMap по-прежнему меняет
`ROLLOUT_ID` и перезапускает pods. Тот же подход применяется к юнитам
scheduler, currency-bot, mcp-gateway и wakegrant; первый deploy после
смены формата аннотации один раз перезапускает все pods. ESO-managed
Secret'ы в хеш не входят: они не рендерятся юнитом и управляются отдельно
(как и до этого изменения); отрендеренные ConfigMap, которые монтирует
pod, входят.


`state` хранит маркеры `.applied` в `--state-dir`. `rollback` не трогает Deployment без
`.applied`, не делает запрос, если generation не изменился, и восстанавливает
записанный объект целиком.

`preflight` — read-only проверки с ненулевым кодом выхода при отказе: retired
HA-writers отсутствуют или `off`, нет звонящего будильника, scheduler отвечает
`ready=true`, endpoint HA указывает на этот scheduler.

`render` отказывает на неизвестный/пустой placeholder и на оставшийся `${`,
поэтому полу-отрендеренный манифест не попадает в кластер. Frozen-паритет
(`bazel test //platform/deploy-rs:unit_tests //platform/deploy-rs:parity`)
сравнивает вывод побайтно с эталонами в `platform/deploy-rs/testdata/frozen/`,
включая байты OCI-архивов и digest.

Образ `home-deploy` (`//projects/home/kubernetes/scheduler:home-deploy_image`,
repo `ghcr.io/proflyder/home-deploy`) — тот же бинарник на distroless
(`user 65534`); его запускают in-cluster preflight Job и Assist smoke Job
юнита `hermes`. Собирается и
продвигается вместе с образами юнитов scheduler и hermes (DECISIONS 36).

Образ `health-exporter`
(`//projects/home-assistant/deploy/health-exporter:health-exporter_image`, repo
`ghcr.io/proflyder/health-exporter`, исходники в `projects/home-assistant/deploy/health-exporter/crates/health-exporter`).
`hermes rollout` подставляет его digest, command и пустые volume-монтирования
в релиз `runtime`.

`hermes render-config` сохраняет правила Python-рендерера (deep merge,
отказ на TEST-NET placeholder, `.prev`, `--dry-run`/`--check`). Поведение
закреплено unit-тестами `hermesconfig`.

Правила развёртывания:

- сервисы слушают loopback или LAN, не публикуются в интернет;
- внешний вход — только собственный VPN (ocserv);
- секреты — через env/secret files;
- `ha-bridge` общается с HA, `ha-bridge` не управляет Pulsar напрямую.
