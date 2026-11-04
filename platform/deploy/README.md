# platform/deploy

Общие шаблоны развёртывания и **`home-deploy`** — единственный CLI инструментов
деплоя (решение 35: инструменты деплоя пишутся на Go).

```text
cmd/home-deploy/   CLI: render, env get, build-id, oci, converge, state,
                   preflight, runtime split, eso names, secret opaque,
                   hermes rollout|render-config|assist-smoke
internal/envfile   разбор приватных KEY=VALUE файлов
internal/render    подстановка ${KEY} с allowlist по юниту
internal/buildid   короткий идентификатор сборки
internal/oci       OCI layout: digest, pack (ctr import), docker archive
internal/kube      запуск kubectl с подменяемым runner'ом
internal/converge  полная замена Deployment с prune drift
internal/state     маркеры .before.json/.absent/.applied и rollback
internal/preflight read-only проверки HA + scheduler для --execute
internal/hermes    rollout digest'ов в релиз runtime + Assist smoke клиент
internal/hermesconfig  merge config.fragment.yaml в живой HERMES_HOME
internal/runtimevalues split helm template, values и pin exporter'а
internal/manifests имена ExternalSecret и bootstrap Opaque Secret
internal/...       далее: openbao
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

| Команда | Назначение |
|---|---|
| `render --unit U --env F --out D [--set K=V]... файлы...` | подставляет env в манифесты юнита; отказ на пустой или неизвестный placeholder |
| `env get FILE KEY` | читает одно значение из env-файла |
| `build-id --revision R --fingerprint STR` | `<revision>-<12 hex sha256(fingerprint)>` |
| `oci digest LAYOUT` | digest OCI-layout |
| `oci pack LAYOUT --ref R --out F` | tar OCI-layout без перепаковки |
| `oci docker LAYOUT --ref R --out F` | архив для `docker load` из layout |
| `converge --kubectl K --namespace NS --name N --file F [--preserve-replicas] [--preserve-env]` | заменяет живой Deployment отрендеренным, убирая drift (см. ниже) |
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
JSON-payload'ы с эталонными трассами (`testdata/converge/`).

`state` хранит маркеры `.applied` в `--state-dir`. `rollback` не трогает Deployment без
`.applied`, не делает запрос, если generation не изменился, и восстанавливает
записанный объект целиком.

`preflight` — read-only проверки с ненулевым кодом выхода при отказе: retired
HA-writers отсутствуют или `off`, нет звонящего будильника, scheduler отвечает
`ready=true`, endpoint HA указывает на этот scheduler.

`render` отказывает на неизвестный/пустой placeholder и на оставшийся `${`,
поэтому полу-отрендеренный манифест не попадает в кластер. Golden-тесты
(`go test ./platform/deploy/...`) сравнивают вывод побайтно с эталонами
в `testdata/golden/`; `oci` сравнивает
байты архивов и digest с `testdata/oci/`.

Образ `home-deploy` (`//projects/home/kubernetes/scheduler:home-deploy_image`,
repo `ghcr.io/proflyder/home-deploy`) — тот же бинарник на distroless
(`user 65534`); его запускают in-cluster preflight Job и Assist smoke Job
юнита `hermes`. Собирается и
продвигается вместе с образами юнитов scheduler и hermes (DECISIONS 36).

Образ `health-exporter`
(`//projects/home-assistant/deploy/health-exporter:health-exporter_image`, repo
`ghcr.io/proflyder/health-exporter`, исходники в `projects/home-assistant/deploy/health-exporter/cmd/health-exporter`).
`hermes rollout` подставляет его digest, command и пустые volume-монтирования
в релиз `runtime`.

`hermes render-config` сохраняет правила Python-рендерера (deep merge,
отказ на TEST-NET placeholder, `.prev`, `--dry-run`/`--check`). Документ
держится как упорядоченное дерево (`yaml.MapSlice`), поэтому порядок ключей
совпадает; golden-тест сравнивает разобранные документы с Python-выходом
(`testdata/golden/hermes-config/`), потому что emitter по-разному оформляет
длинные строки и кавычки.

Правила развёртывания:

- сервисы слушают loopback или LAN, не публикуются в интернет;
- внешний вход — только собственный VPN (ocserv);
- секреты — через env/secret files;
- `ha-bridge` общается с HA, `alarm` не управляет Pulsar напрямую.
