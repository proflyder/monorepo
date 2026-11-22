# bb server on Proxmox (CT 103)

Unprivileged Debian 13 LXC `bb` для сервера bb: статический адрес и
фиксированный MAC (DHCP-привязка на роутере), старт после HAOS и K3s
(`order=3`), данные на локальном диске Proxmox. Mac — машина bb, треды
выполняются там. Примеры используют `192.0.2.0/24`; реальные адреса, MAC и
data directory — `secrets/NETWORK.md`.

| Файл | Назначение |
|---|---|
| `create-lxc.sh` | создаёт CT 103 (2 vCPU / 2048 MiB / swap 1024 / 16 GiB, `nesting=1`, `Asia/Almaty`, `onboot=1`); `--dry-run` печатает `pct create`; существующий CT не трогает |
| `install-bb.sh` | внутри CT: пакеты, Node.js по SHA-256, пользователь `bb`, linger, проверка user bus, backup timer, опциональные offsite- и dispatch-ключи; enrollment не делает |
| `bb-dispatch` | forced command SSH-моста к плагину `home-dispatcher` |
| `authorized-key.sh` | одна ограниченная строка `authorized_keys` на ключ, идемпотентно по телу ключа |
| `bb-db-backup.sh` | `sqlite3 .backup` + `integrity_check` и `bb server export` |

Версии Node и шаблона — `projects/home/proxmox/versions.sh`.

## 1. Создание CT и установка

```bash
scp -r projects/home/proxmox/bb projects/home/proxmox/versions.sh \
  root@192.0.2.26:/root/bb-setup/
ssh root@192.0.2.26 '/root/bb-setup/bb/create-lxc.sh \
  --ip 192.0.2.30 --gateway 192.0.2.1 --dns 192.0.2.1 --mac 02:00:00:00:00:30'
```

`install-bb.sh` ищет `../versions.sh`, поэтому внутри CT раскладка та же:
`/root/bb-setup/versions.sh` и `/root/bb-setup/bb/*`. Файлы заносятся
`pct push 103 <src> <dst> --perms 755`, запуск —
`pct exec 103 -- bash /root/bb-setup/bb/install-bb.sh [опции]`:

- `--data-dir PATH`;
- `--offsite-pubkey FILE --offsite-from <k3s-node>` — строка в
  `/root/.ssh/authorized_keys` с forced command
  `tar -C /var/backups/bb -cf - bb-server-export.tgz`;
- `--dispatch-pubkey FILE --dispatch-from <k3s-node>` — строка в
  `/home/bb/.ssh/authorized_keys` (`from=…,command="/usr/local/bin/bb-dispatch",restrict`),
  ставит `bb-dispatch` (должен лежать рядом) и каталог
  `/home/bb/.config/home-dispatcher` `0700`.

Скрипт идемпотентен (данные bb, `/etc/bb-backup/env` и существующие ключи не
меняются), конфиг sshd не трогает. Сам выставляет `PATH` и `LC_ALL`, потому что
`pct exec` урезает `PATH`; `libatomic1` нужен Node.js из tarball.

Проверка user bus обязательна (скрипт делает её с retry): если
`runuser -u bb -- systemctl --user show-environment` с
`XDG_RUNTIME_DIR=/run/user/<uid>` падает, не enroll-ить — bb поставит только
detached daemon, который не переживёт reboot. Тогда — раздел «Откат на VM».

## 2. Enrollment

Команда из bb UI (Settings → Machines → Add machine) одноразовая и является
credential: не передавать аргументом, не коммитить, не логировать.

1. Войти в shell `bb` с `HOME=/home/bb`,
   `PATH=/home/bb/.local/bin:/usr/local/bin:/usr/bin:/bin`,
   `XDG_RUNTIME_DIR=/run/user/<uid>`,
   `DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/<uid>/bus`
   (`pct exec 103 -- bash -lc 'runuser -u bb -- env … bash'`).
2. Вставить команду; появится user-unit `bb-host-daemon-*`
   (`systemctl --user list-units "bb-host-daemon-*"`).
3. Записать data directory из `bb machine show`:
   `umask 077; echo BB_DATA_DIR=<path> > /etc/bb-backup/env`.
4. `systemctl start bb-db-backup.service` и проверить `/var/backups/bb`.

## 3. SSH-мост `bb-dispatch`

mcp-gateway на K3s вызывает агентов без открытого наружу порта CT 103:

```text
mcp-gateway --ssh bb@CT103--> /usr/local/bin/bb-dispatch
  -> POST http://127.0.0.1:38886/api/v1/plugins/home-dispatcher/http/rpc
     (x-bb-plugin-token + HMAC-SHA256 над "<timestamp>\n<body>", окно ±30 с)
```

Шлюз знает только SSH-ключ (Secret `mcp-gateway-bb-ssh`); токен плагина и HMAC
живут только на CT. Контракт плагина — `solutions/bb-home-dispatcher/README.md`.

Мост: stdin — один JSON ≤ 16 KiB с `method` из `targets`, `status`,
`validate_spawn`, `spawn`, `send`, `turn_result`, `updates`; stdout — ответ
плагина или `{"ok":false,"code":…,"speech":…}`. Exit: `0` плагин ответил
(даже 4xx/5xx), `1` транспорт (`bridge_error`), `2` отказ без обращения к bb
(`body_too_large`, `unknown_method`, `invalid_request`), `3` мост не настроен.
`SSH_ORIGINAL_COMMAND` игнорируется, тела и секреты не логируются.

Конфигурация (`0600 bb:bb` в `/home/bb/.config/home-dispatcher/`): `token`
(`bb plugin token home-dispatcher`), `hmac` (= настройка `hmacSecret`
плагина), необязательный `env` с `BB_DISPATCH_URL` (по умолчанию
`http://127.0.0.1:38886`, только loopback без учётных данных) и
`BB_DISPATCH_TIMEOUT` (1–120 с, по умолчанию 15).

Раскатка:

1. На K3s-узле: `ssh-keygen -t ed25519 -N '' -C bb-dispatch@gw -f
   /etc/mcp-gateway/bb_dispatch_key`.
2. `install-bb.sh --dispatch-pubkey … --dispatch-from <k3s-node>`.
3. Плагин — path-источником от `bb`: архив `solutions/bb-home-dispatcher` →
   `/home/bb/plugins/bb-home-dispatcher` (`chown bb:bb`), затем
   `bb plugin install path:/home/bb/plugins/bb-home-dispatcher`.
4. В shell `bb` с `umask 077`: `bb plugin token home-dispatcher > …/token`;
   `bb plugin config home-dispatcher set hmacSecret '<random>'` и то же
   значение в `…/hmac`. Значения не выводить.
5. `bb plugin config home-dispatcher set permissionMode auto` (`auto` или
   `accept-edits`, `full` отклоняется), `workerLimit`, `dispatcherThreadId`.
   Провайдеры и машины задаёт плагин `profiles`
   (`bb profiles providers …`, `bb profiles machines …`).
6. Проверка с K3s-узла:
   `printf '%s' '{"method":"targets","params":{}}' | ssh -i /etc/mcp-gateway/bb_dispatch_key -o BatchMode=yes bb@192.0.2.30`;
   любая другая команда по ключу тоже запускает мост (пустой stdin → exit 2).

Ротация: `bb plugin token home-dispatcher --rotate` + перезапись `token`;
новый `hmacSecret` одновременно с `hmac` (иначе 401). Удаление: строка ключа,
`/usr/local/bin/bb-dispatch`, каталог конфигурации,
`bb plugin remove home-dispatcher`.

## 4. Бэкап и restore

Таймер внутри CT (ежедневно 03:30): `/var/backups/bb/bb.db` (`.backup` +
`integrity_check`) и `/var/backups/bb/bb-server-export.tgz` (`bb server export`,
проверка `gzip -t` и `files/bb.db`). Восстанавливаемый артефакт — именно export
(`bb server import`: база, auth secret, данные плагинов, вложения); одной
sqlite-копии недостаточно. Если export упал, sqlite-копия остаётся, а сервис
завершается с ошибкой. Без `BB_DATA_DIR` или базы — успех с предупреждением.

CT 103 входит в vzdump job `nightly` (`pvesh set /cluster/backup/nightly
--vmid 100,101,102,103`). Offsite забирает только export-архив —
`projects/home/backup/offsite/README.md`. Restore (vzdump и offsite) —
`docs/REBUILD.md` («4a. BB server (CT 103)»).

## 5. Перенос сервера и откат на VM

Плагины с `path:`-источником (`profiles`, `home-dispatcher`) не переносятся:
исходники заранее положить на цель по тому же пути. Включить `serverMove` в
Settings → Experiments, `bb server move --to <host> --check`, разобрать
предупреждения, сверить data directory с `/etc/bb-backup/env` и только после
подтверждения владельца `bb server move --to <host>` (без `--yes`). Timezone
цели должна совпадать с хостом (`Asia/Almaty`).

Если LXC не годится (user bus, reboot, обновления bb): Debian 13 VM с теми же
ресурсами, `install-bb.sh` внутри, enrollment и `server move` по той же
процедуре. CT 103 удалять только после успешного переключения и отдельного
подтверждения.

## Проверки

```bash
shellcheck projects/home/proxmox/bb/*.sh
python3 -m unittest projects/home/proxmox/test_bb.py projects/home/proxmox/test_bb_dispatch.py
```

Тесты герметичны: dry-run, backup, подпись моста (включая вектор из
`solutions/bb-home-dispatcher/lib/hmac.js`), лимиты, отсутствие секретов в
выводе, идемпотентность `authorized_keys` на локальном HTTP-double.
