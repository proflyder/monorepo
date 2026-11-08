# WORKSTATION — рабочее место за одну команду

Инструменты и доступ к секретам описаны в репозитории: `mise.toml` (версии и
задачи), `platform/workstation/` (CLI, каталог секретов). Копировать с другого
компьютера ничего не нужно.

## Новый компьютер

```bash
brew install mise                       # или https://mise.jdx.dev
git clone https://github.com/<owner>/<repo>.git && cd <repo>
mise trust && mise run setup
```

`setup` выполняет:

1. `mise install` — все инструменты из `mise.toml` с контрольными суммами из
   `mise.lock`: Bazelisk (`bazel` — shim в `platform/workstation/bin`),
   gitleaks, kubectl, helm, oras, OpenBao CLI `bao`, gh, shellcheck, restic,
   Bitwarden CLI `bw`, Python/Node тех же версий, что в CI.
2. `mise run secrets:login` — адрес OpenBao, логин, пароль и TOTP человека.
   Скрипт выпускает для этой машины orphan periodic token (политика
   `operator-kv`, period 30 дней, имя `workstation-<host>`), отзывает
   human-сессию и предыдущий токен этой машины. Файлы:
   `secrets/openbao/{addr,operator.token,username}` (0600, git-ignored).
   Нужен LAN или VPN (VPN-логин — Bitwarden).
3. `mise run secrets:pull` — `~/.ssh/home-lab/` (ключи, known_hosts, `config`
   с алиасами `pve`/`k3s`/`haos`) и приватные заметки `secrets/NETWORK.md`.
4. `mise run doctor` — инструменты, токен, SSH до узлов, `gh`.

`gh auth login` выполняется отдельно (браузер).

## Секреты для скриптов: одна точка входа

Скрипты деплоя и обслуживания запускаются только через `mise run admin:*`.
Задача выбирает профиль fnox (`fnox.toml` в корне — только ссылки на OpenBao),
fnox берёт значения из OpenBao токеном машины (`workstation.py token`, заодно
продлевает его), кладёт файловые секреты во временные файлы 0600, передаёт
пути в тех переменных, которые скрипт уже читает (`INGRESS_PRIVATE_ENV`,
`KUBECONFIG`, `OFFSITE_ENV_FILE`, …), и удаляет файлы по завершении команды.
Секрет получает только тот скрипт и только те значения, что указаны в его
профиле; shell остаётся чистым. Задачам, которым нужен кластер, туннель к API
K3s (`127.0.0.1:16443` → узел) поднимает `with-kube-tunnel.sh` на время
команды. fnox вызывает CLI `vault` — это shim на `bao`.

| Задача | Что запускает | Профиль fnox |
|---|---|---|
| `admin:ingress` / `admin:ingress-verify` | Traefik, cert-manager, маршруты / проверка | `ingress` |
| `admin:openbao` / `admin:openbao-verify` | OpenBao (живой seal не трогает) / проверка | `openbao` |
| `admin:vaultwarden` | Vaultwarden (`--render-only DIR` без кластера) | `vaultwarden` |
| `admin:observability` | Prometheus/Loki/Alloy | `observability` |
| `admin:external-secrets` | ESO и ExternalSecrets | `kube` |
| `admin:heartbeat` | Cloudflare Worker heartbeat | `heartbeat` |
| `admin:offsite-install` | offsite restic на узле K3s (`--dry-run`) | `offsite` |
| `admin:pve-backup-install` | метрики vzdump на PVE (`--dry-run`) | — |
| `admin:scheduler-backup` | бэкап SQLite scheduler в `local-data/` | `kube` |
| `admin:service <unit>` | deploy.sh сервиса руками (обычно это делает CI) | `scheduler`, `currency-bot`, `mcp-gateway`, `wakegrant` |
| `admin:android-distribute` | сборка, подпись, Firebase (`--dry-run`) | `android` |
| `deploy:status <unit> --live`, `rollback --yes` | kubectl / helm по кластеру | `kube` (автоматически) |
| `secrets:exec -- -P <p> exec -- <cmd>` | любая команда с секретами профиля | любой |

`mise run secrets:file <file>` / `secrets:clean` остаются запасным путём для
ручной работы с файлом (например, CA VPN для `vpncerts.py`).

## Каждый день

| Задача | Команда |
|---|---|
| скрипт деплоя/обслуживания | `mise run admin:<задача>` (секреты — fnox, см. выше) |
| файл вручную (запасной путь) | `mise run secrets:file home/ingress.env` → `mise run secrets:clean home/ingress.env` |
| сверить каталог с OpenBao | `mise run secrets:check` |
| копия в Vaultwarden | `export BW_SESSION=$(bw unlock --raw)` → `mise run vault:sync` |
| всё ли на месте | `mise run doctor` |
| новая версия инструмента | правка `mise.toml` → `mise lock --platform linux-x64,macos-arm64` |

`BAO_ADDR` выставляется в окружении mise автоматически, поэтому `bao kv get
secret/infra/...` работает напрямую (токен — `BAO_TOKEN=$(cat
secrets/openbao/operator.token)` только на время команды). В linked worktree
токен берётся из основной копии, восстановленные файлы — в `secrets/` текущей.

Отзыв доступа машины: admin в UI OpenBao → Access → Tokens (или
`bao token revoke -accessor`), токены называются `workstation-<host>`.

## CI

CI берёт версии из того же `mise.toml` через `jdx/mise-action` (каждый job —
только свои инструменты, `install_args`), контрольные суммы — из `mise.lock`.
Bazel по-прежнему ставит `bazel-contrib/setup-bazel` (свой кэш), Java/Android/Zig
и buildx — своими официальными actions.
