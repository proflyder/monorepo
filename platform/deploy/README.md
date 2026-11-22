# platform/deploy

Общие шаблоны развёртывания и `resolve.sh`. Сам CLI `home-deploy` — Rust-крейт
[`platform/deploy-rs`](../deploy-rs/README.md).

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

CI этот инструмент для деплоя не запускает (применяет Flux, решение 54): его
вызывают admin-скрипты и in-cluster Job'ы. `DEPLOY_NO_BUILD=1` запрещает
запасные пути `bazel-bin` и `bazel run`. На узле, где запускается
`nft/apply.sh`, нужен установленный бинарник или `HOME_DEPLOY`.

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

- `converge`: Service/PVC — `apply`; отсутствующий Deployment создаётся,
  существующий заменяется `replace` с его `resourceVersion`, drift удаляется
  (кроме `--preserve-replicas`/`--preserve-env`). `--summary-file` пишет
  `created|unchanged|rolled-out` по сравнению `metadata.generation`.
- `render --rollout-key`: pod template получает хеш всего отрендеренного юнита,
  а `BUILD_ID` — только аннотация Deployment, поэтому новая ревизия Git без
  изменения содержимого не перезапускает pods (решение 38).
- `render` отказывает на неизвестный/пустой placeholder и оставшийся `${` —
  полу-отрендеренный манифест не попадёт в кластер.
- `state rollback` не трогает Deployment без `.applied` и восстанавливает
  записанный объект целиком.
- `preflight` — read-only: retired HA-writers выключены, нет звонящего
  будильника, scheduler `ready=true`, endpoint HA указывает на него.

Образ `ghcr.io/proflyder/home-deploy`
(`//projects/home/kubernetes/scheduler:home-deploy_image`) — тот же бинарник
на distroless; его запускают preflight Job и Assist smoke Job юнита `hermes`.

Правила развёртывания:

- сервисы слушают loopback или LAN, не публикуются в интернет;
- внешний вход — только собственный VPN (ocserv);
- секреты — через env/secret files;
- `ha-bridge` общается с HA, `ha-bridge` не управляет Pulsar напрямую.
