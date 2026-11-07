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
   Bitwarden CLI `bw`, Python/Node/Go тех же версий, что в CI.
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

## Каждый день

| Задача | Команда |
|---|---|
| файл для старого скрипта | `mise run secrets:file home/ingress.env` → `mise run secrets:clean home/ingress.env` |
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
