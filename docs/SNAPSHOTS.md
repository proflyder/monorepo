# Snapshots

Локальные снапшоты проекта лежат в:

```text
snapshots/
```

Папка **целиком исключена из Git**. В Git остаются только правила и
документация, а сами снапшоты — нет.

## Категории

```text
snapshots/
  project/     git archive проекта
  station/     /data, Pulsar/quasar config, station metadata
  nettop/      Proxmox host config + HA/K3s recovery artifacts
```

Для каждой категории — свой timestamp:

```text
snapshots/<category>/<UTC timestamp>/
  ...archives...
  manifest.json
  SHA256SUMS
```

## Что входит

### project

`repo.tar.gz` — `git archive HEAD`, только tracked-файлы. Секреты, backups,
logs, resources и snapshots исключены.

### station

- `/data` целиком;
- `/system/vendor/quasar`;
- `/etc/init.d/S99zpulsar`, `S97quasar`, `/etc/dropbear`;
- `uname`, `df`, `mount`, `ps`, `state.conf`, список `/data/pulsar`.

Содержит station secrets (`/data/pulsar/api_key`, BLE/SSH), поэтому только
private external storage.

### nettop (Proxmox)

- `/etc/pve`, `/etc/network`, `/etc/ssh`, `/etc/apt/sources.list.d`;
- `/var/lib/ricardo-terraform`;
- `pveversion`, `qm list`, `qm config 100/101`, `pvesm`, `lsblk`, network;
- selected recovery: HA native backup, EFI, K3s `cluster-and-auth`.

**Не** включает VM disk images/full vzdump. Для полного восстановления VM
нужен отдельный `vzdump`/PVC snapshot.

## Команды

Project:

```bash
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
mkdir -p "snapshots/project/$STAMP"
git archive --format=tar.gz HEAD > "snapshots/project/$STAMP/repo.tar.gz"
(cd "snapshots/project/$STAMP" && shasum -a 256 repo.tar.gz > SHA256SUMS)
```

Station (ADB; BusyBox tar — archive then gzip):

```bash
adb -s 192.168.3.13:5555 shell \
  'tar -C /data -cf /tmp/station-data.tar . && gzip -9 /tmp/station-data.tar'
adb pull /tmp/station-data.tar.gz snapshots/station/<stamp>/
```

Nettop (Proxmox host):

```bash
ssh -i secrets/proxmox-install/proxmox_admin \
  -o UserKnownHostsFile=secrets/proxmox-install/known_hosts \
  root@192.168.3.26 \
  'tar -C / -czf /tmp/pve-config.tar.gz etc/pve etc/network etc/hosts etc/ssh etc/apt/sources.list.d var/lib/ricardo-terraform'
scp ... root@192.168.3.26:/tmp/pve-config.tar.gz snapshots/nettop/<stamp>/
```

## External copy

Текущая внешняя копия:

```text
/Volumes/512 GB/data/snapshots/yandex-station-lab/
  project/
  station/
  nettop/
```

Копировать только `rsync -a --exclude='._*'`; после копирования проверять
`SHA256SUMS` на месте. exFAT создаёт AppleDouble `._*` — их удалять.

Снапшоты в Git не коммитить.
