# bb server on Proxmox (CT 103)

Отдельный unprivileged Debian 13 LXC `bb` для сервера bb. Контейнер получает
статический адрес, запускается после HAOS и K3s (`order=3`) и хранит данные
bb на локальном диске Proxmox. Ночной vzdump снимает гостя целиком, а
systemd-таймер внутри CT заранее делает согласованную SQLite-копию.

Реальные адреса, DHCP reservation и путь enrollment-команды не входят в
репозиторий: примеры ниже используют `192.0.2.0/24`, фактические значения —
`secrets/NETWORK.md`.

## Файлы

| Файл | Назначение |
|---|---|
| `create-lxc.sh` | создаёт CT 103: Debian 13, 2 vCPU / 2048 MiB / swap 1024 / 16 GiB, static IP + фиксированный MAC, `nesting=1`, timezone `Asia/Almaty`, `onboot=1` |
| `install-bb.sh` | внутри CT: пакеты, Node.js по SHA-256, пользователь `bb`, linger, проверка user bus, backup timer, опциональные offsite- и dispatch-ключи; enrollment не выполняет |
| `bb-dispatch` | forced command SSH-моста: один JSON из stdin → подпись HMAC → POST на loopback-маршрут плагина `home-dispatcher` |
| `authorized-key.sh` | общий помощник для `authorized_keys`: одна ограниченная строка на ключ, идемпотентно по телу ключа |
| `bb-db-backup.sh` | `sqlite3 .backup` + `PRAGMA integrity_check` копии в `/var/backups/bb` и полный `bb server export` в `bb-server-export.tgz` |
| `test_bb.py` | герметичные проверки dry-run и backup без Proxmox/сети |
| `test_bb_dispatch.py` | герметичные проверки моста и `authorized_keys` на локальном HTTP-double плагина |

Параметры `install-bb.sh`: `--data-dir PATH`, `--offsite-pubkey FILE --offsite-from ADDRESS`
и `--dispatch-pubkey FILE --dispatch-from ADDRESS`. Offsite-ключ добавляет в
`/root/.ssh/authorized_keys` строку с `from="ADDRESS"` и forced command
`tar -C /var/backups/bb -cf - bb-server-export.tgz` (только чтение одного
архива, без tty и проброса). Dispatch-ключ добавляет в
`/home/bb/.ssh/authorized_keys` (пользователь `bb`, не root) строку с
`from="ADDRESS"` и forced command `/usr/local/bin/bb-dispatch`; сам
`bb-dispatch` ставится в `/usr/local/bin`, если файл лежит рядом со скриптом
(при `--dispatch-pubkey` его отсутствие — ошибка), а каталог
`/home/bb/.config/home-dispatcher` создаётся `0700 bb:bb` (файлы `token`/`hmac`
скрипт не создаёт — см. раздел 3). Повторный запуск не создаёт дубль ключа.
Скрипт проверяет, что `ssh.service` запущен, но не меняет конфигурацию sshd.

## 1. Создание CT

Скопировать каталог и общий `versions.sh` на PVE (версии Node и шаблона
берутся оттуда):

```bash
scp -r projects/home/proxmox/bb projects/home/proxmox/versions.sh \
  root@192.0.2.26:/root/bb-setup/
ssh root@192.0.2.26 \
  '/root/bb-setup/bb/create-lxc.sh \
     --ip 192.0.2.30 --gateway 192.0.2.1 --dns 192.0.2.1 \
     --mac 02:00:00:00:00:30'
```

MAC обязателен: на роутере для CT 103 уже сделана DHCP-привязка к
фиксированному адресу, поэтому `--net0` создаётся с `hwaddr=`. В примере —
локально администрируемый плейсхолдер; реальный MAC и IP брать из приватного
`secrets/NETWORK.md`. MAC также можно передать через `BB_HWADDR`.

Скрипт проверяет, что CTID свободен, адрес не отвечает на ping/ARP, и
скачивает закреплённый шаблон Debian 13, если его нет. Если CT 103 уже
существует, скрипт ничего не меняет и сообщает об этом. `--dry-run` печатает
команду `pct create` без изменений на хосте.

Проверить ресурсы, порядок старта и timezone:

```bash
ssh root@192.0.2.26 \
  'pct config 103 | grep -E "^(hostname|cores|memory|swap|rootfs|net0|onboot|startup|timezone|features):"'
```

## 2. Установка внутри CT

`install-bb.sh` ищет `versions.sh` на уровень выше своего каталога
(`../versions.sh`), поэтому live-раскладка такая: `versions.sh` лежит в
`/root/bb-setup/`, а `install-bb.sh` и `bb-db-backup.sh` — в
`/root/bb-setup/bb/`.

```bash
ssh root@192.0.2.26 'mkdir -p /root/bb-setup/bb'
scp projects/home/proxmox/bb/install-bb.sh \
    projects/home/proxmox/bb/bb-db-backup.sh \
    projects/home/proxmox/bb/bb-dispatch \
    projects/home/proxmox/bb/authorized-key.sh \
    root@192.0.2.26:/root/bb-setup/bb/
scp projects/home/proxmox/versions.sh root@192.0.2.26:/root/bb-setup/
ssh root@192.0.2.26 \
  'pct push 103 /root/bb-setup/bb/install-bb.sh /root/bb-setup/bb/install-bb.sh --perms 755 &&
   pct push 103 /root/bb-setup/bb/bb-db-backup.sh /root/bb-setup/bb/bb-db-backup.sh --perms 755 &&
   pct push 103 /root/bb-setup/bb/bb-dispatch /root/bb-setup/bb/bb-dispatch --perms 755 &&
   pct push 103 /root/bb-setup/bb/authorized-key.sh /root/bb-setup/bb/authorized-key.sh --perms 644 &&
   pct push 103 /root/bb-setup/versions.sh /root/bb-setup/versions.sh --perms 644 &&
   pct exec 103 -- bash /root/bb-setup/bb/install-bb.sh'
```

`pct exec` запускает команду с урезанным `PATH`
(`/usr/sbin:/usr/bin:/sbin:/bin`), поэтому `install-bb.sh` сам выставляет
`PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin` и
`LC_ALL=C.UTF-8` в начале. Пакет `libatomic1` ставится вместе с остальными:
без него Node.js 26 из tarball падает с
`libatomic.so.1: cannot open shared object file`.

Скрипт идемпотентен: повторный запуск не меняет данные bb и существующий
`/etc/bb-backup/env`. Для offsite-доступа передать публичный ключ (приватный
лежит вне Git, на K3s-узле):

```bash
pct exec 103 -- bash /root/bb-setup/bb/install-bb.sh \
  --offsite-pubkey /root/bb-setup/offsite-bb.pub --offsite-from 192.0.2.27
```

`--offsite-from` — адрес K3s-узла; ключ разрешает только
`tar -C /var/backups/bb -cf - bb-server-export.tgz`.

Обязательная проверка systemd user bus до enrollment:

```bash
ssh root@192.0.2.26 \
  "pct exec 103 -- bash -lc 'uid=\$(id -u bb); \
     runuser -u bb -- env XDG_RUNTIME_DIR=/run/user/\$uid \
       DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/\$uid/bus \
       systemctl --user show-environment'"
```

Если команда завершилась ошибкой, **не enroll-ить контейнер**: bb поставит
только detached daemon, который не переживёт reboot. `install-bb.sh` сам
проверяет это с retry и завершается с текстом «LXC не подходит,
используйте VM». В этом случае нужна Debian 13 VM с теми же ресурсами.

## 3. SSH-мост `bb-dispatch`

Голосовой путь не открывает порт CT 103 наружу: mcp-gateway на K3s заходит по
ssh ключом с forced command, а мост сам подписывает запрос к плагину
`home-dispatcher`. Плагин и его контракт описаны в
`solutions/bb-home-dispatcher/README.md`.

```text
mcp-gateway (K3s) --ssh bb@CT103--> /usr/local/bin/bb-dispatch
  -> POST http://127.0.0.1:38886/api/v1/plugins/home-dispatcher/http/rpc
  -> plugin home-dispatcher (x-bb-plugin-token + HMAC-SHA256)
```

Токен плагина и HMAC-секрет живут **только на CT**: шлюз знает только свой
SSH-ключ. Ключ ставится пользователю `bb`, не root, и разрешает ровно одну
команду:

```text
from="192.0.2.27",command="/usr/local/bin/bb-dispatch",restrict ssh-ed25519 AAAA...
```

`restrict` (OpenSSH ≥ 7.2) отключает pty, проброс портов/агента/X11 и
`~/.ssh/rc`; отдельные `no-pty`/`no-port-forwarding` не нужны.

### Контракт моста

| | |
|---|---|
| stdin | один JSON-объект ≤ 16 KiB с `method` из `targets`, `status`, `validate_spawn`, `spawn`, `send`, `turn_result`; `params`, если есть, — объект |
| stdout | ответ плагина как есть, либо `{"ok":false,"code":...,"speech":"..."}` |
| exit 0 | плагин ответил (в том числе HTTP 4xx/5xx с `ok:false`) — шлюз читает `ok/code/speech` |
| exit 1 | транспортная ошибка, `code: bridge_error`, «Сервер агентов недоступен.» |
| exit 2 | отказ без обращения к bb: `body_too_large`, `unknown_method`, `invalid_request` |
| exit 3 | мост не настроен: нет/небезопасные права `token`/`hmac`, неверный `BB_DISPATCH_URL` |

Мост отправляет **исходные байты** запроса и подписывает
`"<timestamp>\n<body>"` (HMAC-SHA256, окно плагина ±30 с). `SSH_ORIGINAL_COMMAND`
игнорируется: любую команду ssh заменяет forced command. Тело запроса и ответа
не логируется, токен и секрет не печатаются никогда.

Файлы конфигурации (каталог `0700 bb:bb`, файлы `0600 bb:bb`):

| Путь | Назначение |
|---|---|
| `/home/bb/.config/home-dispatcher/token` | токен `bb plugin token home-dispatcher` |
| `/home/bb/.config/home-dispatcher/hmac` | значение настройки `hmacSecret` плагина |
| `/home/bb/.config/home-dispatcher/env` | необязательные `BB_DISPATCH_URL` (по умолчанию `http://127.0.0.1:38886`) и `BB_DISPATCH_TIMEOUT` (1–120 с, по умолчанию 15) |

URL принимается только на loopback (`127.0.0.0/8`, `::1`, `localhost`) и без
учётных данных: токен уходит в заголовке и не должен попасть на другой хост.
`install-bb.sh` создаёт каталог и сам `bb-dispatch`, но **не** генерирует
токен и секрет.

### Раскатка

1. На K3s-узле создать отдельный ключ (приватный ключ вне Git; в кластер он
   попадает как Secret `mcp-gateway-bb-ssh`):

   ```bash
   install -d -m 700 /etc/mcp-gateway
   ssh-keygen -t ed25519 -N '' -C bb-dispatch@gw -f /etc/mcp-gateway/bb_dispatch_key
   ```

2. Публичный ключ скопировать на PVE и запушить внутрь CT вместе с остальными
   файлами (см. раздел 2), затем выполнить установку с ключом шлюза:

   ```bash
   ssh root@192.0.2.26 \
     'pct exec 103 -- bash /root/bb-setup/bb/install-bb.sh \
        --dispatch-pubkey /root/bb-setup/bb-dispatch.pub \
        --dispatch-from 192.0.2.27'
   ```

   Повторный запуск безопасен: данные bb, `/etc/bb-backup/env` и существующая
   строка `authorized_keys` не меняются. `bb-dispatch` требует `python3` из
   Debian — `install-bb.sh` ставит его вместе с остальными пакетами.

3. Поставить плагин path-источником (как `profiles`) от пользователя `bb`.
   Исходники переносятся архивом, чтобы не зависеть от `pct push` по файлам:

   ```bash
   tar -czf /tmp/bb-home-dispatcher.tgz -C solutions bb-home-dispatcher
   scp /tmp/bb-home-dispatcher.tgz root@192.0.2.26:/root/bb-setup/
   ssh root@192.0.2.26 \
     'pct push 103 /root/bb-setup/bb-home-dispatcher.tgz /root/bb-setup/bb-home-dispatcher.tgz &&
      pct exec 103 -- install -d -o bb -g bb -m 700 /home/bb/plugins &&
      pct exec 103 -- tar -xzf /root/bb-setup/bb-home-dispatcher.tgz -C /home/bb/plugins &&
      pct exec 103 -- chown -R bb:bb /home/bb/plugins/bb-home-dispatcher'
   ssh root@192.0.2.26 \
     "pct exec 103 -- bash -lc 'uid=\$(id -u bb); \
        runuser -u bb -- env HOME=/home/bb \
          PATH=/home/bb/.local/bin:/usr/local/bin:/usr/bin:/bin \
          XDG_RUNTIME_DIR=/run/user/\$uid \
          DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/\$uid/bus \
          bb plugin install path:/home/bb/plugins/bb-home-dispatcher'"
   ```

4. Выдать токен и секрет и записать их в файлы моста (значения не выводить в
   чат, тикеты и логи):

   ```bash
   # в shell пользователя bb (раздел 4, шаг 2)
   umask 077
   bb plugin token home-dispatcher > /home/bb/.config/home-dispatcher/token
   bb plugin config home-dispatcher set hmacSecret '<random-secret>'
   printf '%s\n' '<random-secret>' > /home/bb/.config/home-dispatcher/hmac
   chmod 600 /home/bb/.config/home-dispatcher/token \
              /home/bb/.config/home-dispatcher/hmac
   ```

5. Задать остальные настройки плагина: `permissionMode` (`auto` или
   `accept-edits`, `full` отклоняется), `workerLimit`, `profileProviders`
   (зеркало allowlist провайдеров из `bb profiles providers`) и
   `dispatcherThreadId`:

   ```bash
   bb plugin config home-dispatcher set permissionMode auto
   bb plugin config home-dispatcher set profileProviders '{"work":["claude-code"],"personal":null}'
   bb plugin config home-dispatcher set dispatcherThreadId thr_...
   ```

6. Если bb server слушает не стандартный `38886` (проверить `bb status` и
   `BB_SERVER_URL`), записать URL в `env` моста:

   ```bash
   printf 'BB_DISPATCH_URL=http://127.0.0.1:38886\nBB_DISPATCH_TIMEOUT=15\n' \
     > /home/bb/.config/home-dispatcher/env
   chmod 600 /home/bb/.config/home-dispatcher/env
   ```

7. Проверка с K3s-узла (ключ шлюза):

   ```bash
   printf '%s' '{"method":"targets","params":{}}' |
     ssh -i /etc/mcp-gateway/bb_dispatch_key -o BatchMode=yes bb@192.0.2.30
   ```

   Ожидается JSON плагина (`{"ok":true,...}` или отказ с `ok:false`). Любая
   другая команда по этому ключу тоже запускает мост, а не shell:

   ```bash
   ssh -i /etc/mcp-gateway/bb_dispatch_key bb@192.0.2.30 'id'
   # stdin пуст -> {"ok":false,"code":"invalid_request",...}, exit 2
   ```

### Ротация и удаление

- Токен: `bb plugin token home-dispatcher --rotate`, затем перезаписать
  `token` (0600). Старый токен сразу перестаёт работать.
- Секрет: `bb plugin config home-dispatcher set hmacSecret <new>` и
  одновременно перезаписать `hmac`; иначе плагин отвечает 401.
- Удаление моста: убрать строку ключа из
  `/home/bb/.ssh/authorized_keys`, удалить `/usr/local/bin/bb-dispatch` и
  каталог `/home/bb/.config/home-dispatcher`. Плагин удаляется
  `bb plugin remove home-dispatcher`.

## 4. Enrollment

Команда из bb UI одноразовая и является credential. Её нельзя передавать
аргументом в скрипт, коммитить или сохранять в логах.

1. В bb UI: Settings → Machines → Add machine (manual provider).
2. На CT войти в shell пользователя `bb`:

   ```bash
   ssh root@192.0.2.26 \
     "pct exec 103 -- bash -lc 'uid=\$(id -u bb); \
        runuser -u bb -- env HOME=/home/bb \
          PATH=/home/bb/.local/bin:/usr/local/bin:/usr/bin:/bin \
          XDG_RUNTIME_DIR=/run/user/\$uid \
          DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/\$uid/bus bash'"
   ```

3. Вставить в этот shell команду из UI. Enrollment создаст приватный CLI и
   user-unit `bb-host-daemon-*`.
4. Проверить, что сервис действительно работает под `bb`:

   ```bash
   ssh root@192.0.2.26 \
     "pct exec 103 -- bash -lc 'uid=\$(id -u bb); \
        runuser -u bb -- env XDG_RUNTIME_DIR=/run/user/\$uid \
          systemctl --user list-units \"bb-host-daemon-*\" --no-pager'"
   ```

5. Узнать data directory (`bb machine show` на Mac или в UI) и записать его:

   ```bash
   ssh root@192.0.2.26 \
     'pct exec 103 -- sh -c "umask 077; echo BB_DATA_DIR=/home/bb/.bb-machines/<server-host> > /etc/bb-backup/env"'
   ```

   Путь в примере условный: использовать фактический из `bb machine show`.
6. Проверить backup вручную:

   ```bash
   ssh root@192.0.2.26 \
     'pct exec 103 -- systemctl start bb-db-backup.service &&
      pct exec 103 -- systemctl status bb-db-backup.service --no-pager &&
      pct exec 103 -- ls -l /var/backups/bb'
   ```

## 5. Перенос сервера

Выполнено 2026-11-03: `bb server move --to bb` переключил сервер bb `0.48.1`
на CT 103, результат проверен primary. Mac остался машиной bb (launchd-агент,
Node из Homebrew) — проекты и их треды выполняются там. Ниже — процедура,
если перенос нужен повторно или на другой хост.

Плагины с `path:`-источником (например `profiles`, изоляция work/personal)
автоматически не переносятся: исходники нужно заранее скопировать на цель по
тому же пути. Иначе `bb server move --check` выдаёт предупреждение и плагин
на новом сервере не загрузится.

Сначала включить `serverMove` в Settings → Experiments, затем показать
checklist:

```bash
bb server move --to bb --check
```

Разобрать blockers/warnings, убедиться, что data directory с CT совпадает с
`/etc/bb-backup/env`, и только после подтверждения владельца выполнить
`bb server move --to bb` без `--yes`. После переключения Mac остаётся обычной
машиной; desktop-приложение можно вернуть в режим фонового сервиса
`bb server install-machine-service` на Mac.

## 6. Бэкап и restore

Внутри CT ежедневно в 03:30 таймер создаёт `/var/backups/bb/bb.db`:
`sqlite3 "$BB_DATA_DIR/bb.db" ".backup ..."`, затем проверяет копию через
`PRAGMA integrity_check`. Каталог `0700`, файл `0600`; при отсутствии
`BB_DATA_DIR` или базы таймер завершается успешно с предупреждением.

Тот же таймер делает `bb server export` от пользователя `bb` во временный
файл и атомарно кладёт `/var/backups/bb/bb-server-export.tgz` (`0600`,
root). Архив проверяется `gzip -t` и наличием `files/bb.db`; если export
не удался, sqlite-копия всё равно остаётся, а сервис завершается с ошибкой,
чтобы таймер показал failure. Это официальный восстанавливаемый артефакт
(`bb server import`): внутри `bb.db`, auth secret, данные плагинов и
вложения. Одна sqlite-копия для восстановления недостаточна.

Offsite-источник `bb` на K3s-узле забирает только этот архив по ssh с
forced command; подробности — `projects/home/backup/offsite/README.md`.

Добавить CT 103 в существующий job `nightly` (04:00), чтобы vzdump содержал
согласованную копию:

```bash
ssh root@192.0.2.26 \
  'pvesh set /cluster/backup/nightly --vmid 100,101,102,103'
ssh root@192.0.2.26 'pvesh get /cluster/backup/nightly'
```

Restore в тот же CTID после остановки/удаления старого гостя:

```bash
ssh root@192.0.2.26 \
  'pct restore 103 /mnt/pve/backup/dump/<vzdump-lxc-103-...> \
     --storage local-lvm --unprivileged 1 && pct start 103'
ssh root@192.0.2.26 \
  'pct exec 103 -- sqlite3 /var/backups/bb/bb.db "PRAGMA integrity_check;"'
```

Проверить live-базу по пути из `/etc/bb-backup/env`, ownership `/home/bb` и
статус user-unit после reboot. Для проверки restore можно восстановить копию
в другой CTID без сети и сравнить `integrity_check`, не трогая живой сервер.

Восстановление из offsite-копии (когда vzdump нет): достать
`bb/bb-server-export.tar` из restic, затем

```bash
gzip -c bb-server-export.tar > bb-server-export.tgz
bb server import bb-server-export.tgz --data-dir <data-dir>
bb server allow-connect
```

Старый сервер остановить до импорта; после запуска проверить threads,
plugins и доступ через `<handle>.getbb.app`.

## 7. Откат на VM

Если user bus не работает, сервис не переживает reboot или LXC мешает
обновлению bb:

1. Не выполнять enrollment в CT 103.
2. Создать Debian 13 VM с 2 vCPU / 2048 MiB / 16 GiB и статическим адресом.
3. Повторить `install-bb.sh` внутри VM; проверка user bus там также остаётся.
4. Выполнить enrollment и `bb server move --to <vm> --check` по той же
   процедуре с подтверждением владельца.
5. Удалять CT 103 только после успешного переключения и отдельного
   подтверждения; vzdump CT остаётся запасной копией.

Timezone CT должна совпадать с хостом (`Asia/Almaty`), иначе `server move`
выдаст warning о разнице времени.

## Проверки

```bash
bash -n projects/home/proxmox/bb/create-lxc.sh \
  projects/home/proxmox/bb/install-bb.sh \
  projects/home/proxmox/bb/bb-db-backup.sh \
  projects/home/proxmox/bb/authorized-key.sh
shellcheck projects/home/proxmox/bb/*.sh
python3 -m unittest projects/home/proxmox/test_bb.py -v
python3 -m unittest projects/home/proxmox/test_bb_dispatch.py -v
python3 platform/quality/audit_repository.py
```

`test_bb_dispatch.py` поднимает локальный HTTP-double маршрута плагина и
проверяет подпись (включая фиксированный вектор из
`solutions/bb-home-dispatcher/lib/hmac.js`), лимит 16 KiB, неразрешённый
метод, мусорный ввод, транспортную ошибку, отсутствие токена/секрета в выводе
и идемпотентность `authorized_keys`. Живой CT и сеть не нужны.
