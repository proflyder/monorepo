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
| `install-bb.sh` | внутри CT: пакеты, Node.js по SHA-256, пользователь `bb`, linger, проверка user bus, backup timer; enrollment не выполняет |
| `bb-db-backup.sh` | `sqlite3 .backup` + `PRAGMA integrity_check` копии в `/var/backups/bb` |
| `test_bb.py` | герметичные проверки dry-run и backup без Proxmox/сети |

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

`install-bb.sh` должен видеть рядом `versions.sh` и `bb-db-backup.sh`:

```bash
scp projects/home/proxmox/bb/install-bb.sh \
    projects/home/proxmox/bb/bb-db-backup.sh \
    projects/home/proxmox/versions.sh \
    root@192.0.2.26:/root/bb-setup/
ssh root@192.0.2.26 \
  'pct push 103 /root/bb-setup/install-bb.sh /root/install-bb.sh --perms 755 &&
   pct push 103 /root/bb-setup/bb-db-backup.sh /root/bb-db-backup.sh --perms 755 &&
   pct push 103 /root/bb-setup/versions.sh /root/versions.sh --perms 644 &&
   pct exec 103 -- /root/install-bb.sh'
```

Скрипт идемпотентен: повторный запуск не меняет данные bb и существующий
`/etc/bb-backup/env`.

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

Перенос экспериментальный и выполняется только primary после явного
подтверждения владельца. Сначала включить `serverMove` в Settings →
Experiments, затем показать checklist:

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
