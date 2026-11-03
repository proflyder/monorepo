# platform/deploy

Общие шаблоны развёртывания и **`home-deploy`** — единственный CLI инструментов
деплоя (решение 35: инструменты деплоя пишутся на Go).

```text
cmd/home-deploy/   CLI: render, env get, build-id, oci digest|pack|docker
internal/envfile   разбор приватных KEY=VALUE файлов
internal/render    подстановка ${KEY} с allowlist по юниту
internal/buildid   короткий идентификатор сборки
internal/oci       OCI layout: digest, pack (ctr import), docker archive
internal/...       далее: converge, state/rollback, preflight, openbao, hermes
testdata/          зафиксированные golden-выходы и OCI-фикстура для parity-тестов
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

`render` отказывает на неизвестный/пустой placeholder и на оставшийся `${`,
поэтому полу-отрендеренный манифест не попадает в кластер. Golden-тесты
(`go test ./platform/deploy/...`) сравнивают вывод побайтно с выходами
Python-рендерера, зафиксированными в `testdata/golden/`; `oci` сравнивает
байты архивов и digest с `testdata/oci/`.

Правила развёртывания:

- сервисы слушают loopback или LAN, не публикуются в интернет;
- внешний вход — только собственный VPN (ocserv);
- секреты — через env/secret files;
- `ha-bridge` общается с HA, `alarm` не управляет Pulsar напрямую.
