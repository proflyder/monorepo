# platform/deploy

Общие шаблоны развёртывания и **`home-deploy`** — единственный CLI инструментов
деплоя (решение 35: инструменты деплоя пишутся на Go).

```text
cmd/home-deploy/   CLI: render, env get, build-id, oci, converge, state, preflight
internal/envfile   разбор приватных KEY=VALUE файлов
internal/render    подстановка ${KEY} с allowlist по юниту
internal/buildid   короткий идентификатор сборки
internal/oci       OCI layout: digest, pack (ctr import), docker archive
internal/kube      запуск kubectl с подменяемым runner'ом
internal/converge  полная замена Deployment с prune drift
internal/state     маркеры .before.json/.absent/.applied и rollback
internal/preflight read-only проверки HA + scheduler для --execute
internal/...       далее: openbao, hermes
testdata/          golden-выходы, OCI-фикстура и трассы converge для parity
resolve.sh         находит и запускает бинарник из deploy-скриптов
docker/            reusable Dockerfile fragments
systemd/           reusable unit templates
```

## Вызов

Скрипты юнитов подключают `resolve.sh` и вызывают функцию `home_deploy`:

```bash
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck source=platform/deploy/resolve.sh
source "$ROOT/platform/deploy/resolve.sh"
home_deploy render --unit scheduler --env "$PRIVATE_ENV" --out "$RENDER_DIR" \
  --set "BUILD_ID=$BUILD_ID" manifests/scheduler.yaml
```

Порядок поиска бинарника: `$HOME_DEPLOY` → `home-deploy` в `PATH` →
`secrets/local-tools/home-deploy` → тот же путь в основном checkout (для linked
worktree) → `bazel run //platform/deploy/cmd/home-deploy --`.

CI собирает бинарник один раз на job (`bazel build
//platform/deploy/cmd/home-deploy`) и экспортирует `HOME_DEPLOY`. На node, где
запускается `nft/apply.sh`, нужен уже установленный бинарник (например
`/usr/local/bin/home-deploy`) или `HOME_DEPLOY`.

## Команды

| Команда | Заменяет |
|---|---|
| `render --unit U --env F --out D [--set K=V]... файлы...` | `scheduler/render.py`, `mcp-gateway/render.py`, `wakegrant/render.py`, heredoc из `ingress/deploy.sh` |
| `env get FILE KEY` | heredoc `env_value` в deploy-скриптах |
| `build-id --revision R --fingerprint STR` | `git rev-parse` + `python3 -c hashlib.sha256…` |
| `oci digest LAYOUT` | `platform/ci/oci_image.py digest` |
| `oci pack LAYOUT --ref R --out F` | `platform/ci/oci_image.py pack` |
| `oci docker LAYOUT --ref R --out F` | `platform/ci/oci_image.py docker` |
| `converge --kubectl K --namespace NS --name N --file F [--preserve-replicas] [--preserve-env]` | `scheduler/converge.py` |
| `state record --state-dir D --namespace NS --name N [--kubectl K]` | `record_before` из deploy-скриптов |
| `state mark --state-dir D --name N` | `mark_applied` из deploy-скриптов |
| `state rollback --state-dir D [--namespace NS] [--kubectl K]` | `scheduler/rollback.py` |
| `preflight --ha-url U --ha-token-file F --scheduler-url U --scheduler-token-file F [--expected-endpoint U]` | `scheduler/preflight.py` |

`converge` вызывает kubectl как Python-версия: support objects (Service/PVC)
применяются через `apply`, отсутствующий Deployment создаётся, существующий
заменяется через `replace` с повторным использованием `resourceVersion`.
`--preserve-replicas` и `--preserve-env` переносят ручной режим/масштаб;
остальные поля и drift удаляются. Parity-тест сравнивает argv и разобранные
JSON-payload'ы с трассами Python (`testdata/converge/`).

`state` держит тот же контракт маркеров, поэтому старый deploy.sh и новый
бинарник совместимы в одном прогоне. `rollback` не трогает Deployment без
`.applied`, не делает запрос, если generation не изменился, и восстанавливает
записанный объект целиком.

`preflight` — те же read-only проверки и коды выхода, что у Python: retired
HA-writers отсутствуют или `off`, нет звонящего будильника, scheduler отвечает
`ready=true`, endpoint HA указывает на этот scheduler.

`render` отказывает на неизвестный/пустой placeholder и на оставшийся `${`,
поэтому полу-отрендеренный манифест не попадает в кластер. Golden-тесты
(`go test ./platform/deploy/...`) сравнивают вывод побайтно с выходами
Python-рендерера, зафиксированными в `testdata/golden/`; `oci` сравнивает
байты архивов и digest с `testdata/oci/`.

Образ `home-deploy` (`//projects/home/kubernetes/scheduler:home-deploy_image`,
repo `ghcr.io/proflyder/home-deploy`) — тот же бинарник на distroless
(`user 65534`); его запускает in-cluster preflight Job, поэтому Python-образ в
кластере не нужен. Собирается и продвигается вместе с образами юнита
scheduler (DECISIONS 36).

Правила развёртывания:

- сервисы слушают loopback или LAN, не публикуются в интернет;
- внешний вход — только собственный VPN (ocserv);
- секреты — через env/secret files;
- `ha-bridge` общается с HA, `alarm` не управляет Pulsar напрямую.
