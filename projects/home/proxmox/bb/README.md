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
| `install-bb.sh` | внутри CT: пакеты, Node.js по SHA-256, пользователь `bb`, linger, проверка user bus, backup timer, опциональный offsite-ключ; enrollment не выполняет |
| `bb-db-backup.sh` | `sqlite3 .backup` + `PRAGMA integrity_check` копии в `/var/backups/bb` и полный `bb server export` в `bb-server-export.tgz` |
| `test_bb.py` | герметичные проверки dry-run и backup без Proxmox/сети |

Параметры `install-bb.sh`: `--data-dir PATH` и
`--offsite-pubkey FILE --offsite-from ADDRESS`. Второй добавляет в
`/root/.ssh/authorized_keys` ключ с `from="ADDRESS"` и forced command
`tar -C /var/backups/bb -cf - bb-server-export.tgz` (только чтение одного
архива, без tty и проброса). Повторный запуск не создаёт дубль. Скрипт
проверяет, что `ssh.service` запущен, но не меняет конфигурацию sshd.

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
    root@192.0.2.26:/root/bb-setup/bb/
scp projects/home/proxmox/versions.sh root@192.0.2.26:/root/bb-setup/
ssh root@192.0.2.26 \
  'pct push 103 /root/bb-setup/bb/install-bb.sh /root/bb-setup/bb/install-bb.sh --perms 755 &&
   pct push 103 /root/bb-setup/bb/bb-db-backup.sh /root/bb-setup/bb/bb-db-backup.sh --perms 755 &&
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

## 3. Enrollment

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

## 4. Перенос сервера

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

## 5. Бэкап и restore

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

## 6. Откат на VM

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
  projects/home/proxmox/bb/bb-db-backup.sh
shellcheck projects/home/proxmox/bb/*.sh
python3 -m unittest projects/home/proxmox/test_bb.py -v
python3 platform/quality/audit_repository.py
```
