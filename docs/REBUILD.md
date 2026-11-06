# REBUILD — восстановление стенда

Текущая конфигурация — `STATE.md`; установка — `INSTALL-PROXMOX.md`;
K3s — `KUBERNETES-OPERATIONS.md`.

## Recovery-комплект

Приватно, вне Git:

- Гости PVE — nightly vzdump на backup SSD (`/mnt/pve/backup`).
- Offsite restic → Google Drive (`rclone:gdrive:home-offsite-v2`): OpenBao
  raft snapshot, SQLite scheduler/currency-bot, ledger ha-bridge, backup HA,
  `bb server export`. Restic password и ключ HA backup — в password manager.
- `secrets/proxmox-install/` — ISO, install settings, SSH keys, TF state,
  Wi-Fi profile.
- `secrets/kubernetes-research/` — kubeconfig, private values.
- `secrets/local/backups/`, `local-data/local/snapshots/station/` — станция
  (`/data`, `/etc`, оригиналы, модели wake word).
- `secrets/openbao/` — recovery keys, seal key, username; `secrets/home/vpn/` —
  CA и клиентские сертификаты VPN.

Проверять SHA-256, `quick_check`/`integrity_check`. Не публиковать.

## 1. Потеря PVE / диска

Если уцелел backup SSD: после установки PVE смонтировать `/dev/sda1`
(`pve-backup`) в `/mnt/pve/backup`, добавить storage `backup` и восстановить
гостей целиком: `qmrestore <vzdump-qemu-100-…> 100`, `… 101`,
`pct restore 102 <vzdump-lxc-102-…>`, `pct restore 103 <vzdump-lxc-103-…>`.
Это быстрее пунктов 4–7 ниже; затем проверка §5.

1. Установить PVE по `INSTALL-PROXMOX.md`; диск будет стёрт.
2. Восстановить SSH-ключи, сеть, timezone, storage, `vmbr0`.
3. Восстановить terraform state/private tfvars; `terraform plan` → apply.
4. Создать VM 100/101 с теми же ресурсами и MAC/static IP.
5. Восстановить HAOS и K3s (ниже).
6. Вернуть Wi-Fi профиль/`ricardo-uplink`; проверить Ethernet fallback.
7. Пересоздать CT 102 `vpn` скриптами `projects/home/proxmox/vpn/`;
   восстановить пользователей/клиентские сертификаты или выпустить новые
   (старые `.p12` в `secrets/home/vpn/clients/`), проверить `443/tcp+udp`.
8. Восстановить CT 103 `bb` из vzdump либо пересоздать скриптами
   `projects/home/proxmox/bb/` (см. §4a).
9. Проверить HA/MA/Grafana/HTTPS ingress/voice/VPN/bb.

### 1a. Offsite-копия (Google Drive + restic)

Если локальный backup SSD потерян вместе с PVE, источник — зашифрованная
offsite-копия в Google Drive владельца (папка `home-offsite-v2`). Нужны restic
password из password manager и доступ к Google-аккаунту (новый
`rclone authorize`, см. `OPERATIONS.md`); без пароля копия не читается.
Проверено 2026-11-02: restore на Mac, все файлы совпали с `MANIFEST.json`.

```bash
export RCLONE_CONFIG=/path/to/rclone.conf     # remote [gdrive], scope drive.file
export RESTIC_REPOSITORY='rclone:gdrive:home-offsite-v2'
export RESTIC_PASSWORD='...'
restic snapshots
restic check --read-data-subset=5%
restic restore latest --target /srv/offsite-restore
cat /srv/offsite-restore/MANIFEST.json
```

Дальше: `openbao/*.snap` — в восстановление OpenBao (§3.7), но `openbao-seal`
и recovery keys в offsite-копии **нет** — они только в отдельной offline-копии.
`sqlite/scheduler.db` и `sqlite/currency_bot-*.db` — остановить writer, вернуть
в PVC и выполнить `PRAGMA integrity_check`; `ha-bridge/ledger.json` — в PVC
ha-bridge; `home-assistant/backup_*.tar` — в HA onboarding;
`bb/bb-server-export.tar` — в bb server (см. §4a). Подробно —
`projects/home/backup/offsite/README.md`.

## 2. HAOS

1. Установить HAOS VM 100; UEFI, 2 vCPU/4 GiB/32 GiB.
2. Восстановить supported HA backup через Supervisor onboarding.
3. Проверить `internal_url`, trusted proxy, MFA, add-ons MA/Matter.
4. MA data: `/mnt/data/supervisor/apps/data/d5369777_music_assistant`;
   Matter: `core_matter_server`.
5. Проверить HA integration MA, Sendspin player, alarm media URLs.
6. Не запускать HA Container параллельно с HAOS.

## 3. K3s

1. VM 101: Ubuntu 24.04, 12 GiB/200 GiB, static IP, MAC из private tfvars.
2. Установить **ту же** K3s version; data-dir `/srv/k3s/runtime`.
3. Остановить K3s перед восстановлением datastore.
4. Вернуть SQLite cluster snapshot, token, TLS/cred, config.
5. Вернуть `/srv/k3s/volumes/*` до старта Pods.
6. Поднять образы: сначала попробовать pull из GHCR по digest
   (`scheduler`, `ha-bridge`, `ha-control`, `hermes`, `voice-proxy`;
   `hermes` — Docker, остальные — Bazel `rules_oci`); если пакеты
   недоступны — импортировать OCI-архивы
   (`sudo k3s ctr images import --platform linux/amd64 <archive>`).
7. Восстановить OpenBao: PV raft/backups, Secret `openbao-seal`,
   recovery keys из `secrets/openbao/`; проверить auto-unseal и логин.
   Если PV потерян — поднять пустой OpenBao, восстановить snapshot (§6).
8. Применить Helm releases из репозитория/private values.
9. Проверить pods, PV, Grafana, HTTPS ingress, ExternalSecrets и voice
   endpoints.

Без server token восстановление datastore невозможно. Не заменять свежий
Hermes auth старой копией.

## 4. Станция

- ADB/SSH: только LAN; пароль — `secrets/README.md`.
- Pulsar: `/data/pulsar`; бинарник — `ghcr.io/proflyder/pulsar-firmware:<version>`
  (установка — `deploy-station` или `pulsar-install`), модель —
  `artifacts/wakeword/`, откат — `pulsar.prev`.
- Factory reset сносит `/data`, root/SSH/OTA/guard. Делать только с
  согласованием и запасной станцией.
- После восстановления: ESPHome entities, wake word, HA pipeline, Sendspin,
  MA/Spotify, egress guard, BLE gateway.

## 4a. BB server (CT 103)

Полная установка — `projects/home/proxmox/bb/README.md`; факты — `STATE.md`,
сеть — `NETWORK.md`.

1. Если есть vzdump CT 103, восстановить его в тот же CTID:
   `pct restore 103 <vzdump-lxc-103-…> --storage local-lvm --unprivileged 1`,
   затем `pct start 103`. Проверить `pct config 103`: static IP, MAC из
   `secrets/NETWORK.md`, `startup order=3,up=30,down=60`, timezone
   `Asia/Almaty`.
2. Пересоздание без дампа: `create-lxc.sh` с адресом/MAC из приватной карты,
   затем `install-bb.sh`. Enrollment выполнить заново только если данные bb
   потеряны; одноразовую команду из UI не сохранять.
3. Проверить user bus и user-unit:
   `loginctl show-user bb -p Linger`,
   `systemctl --user` под `bb` должен видеть `bb-host-daemon-*`.
4. SQLite: `/var/backups/bb/bb.db` — согласованная копия перед vzdump;
   live-база — по `BB_DATA_DIR` в `/etc/bb-backup/env`. Для обеих:
   `sqlite3 <db> 'PRAGMA integrity_check;'` → `ok`. Там же лежит
   `bb-server-export.tgz` — полный `bb server export` (bb.db, auth secret,
   plugins, attachments), который делает тот же таймер 03:30.
5. Если vzdump и `/var/backups/bb` потеряны, восстановить bb из
   offsite-копии (§1a): `restic restore` → `bb/bb-server-export.tar` →
   `gzip -c bb-server-export.tar > bb-server-export.tgz` →
   `bb server import bb-server-export.tgz --data-dir <dir>` →
   `bb server allow-connect`. Останавливать старый сервер до импорта.
   Одной `bb.db` для восстановления недостаточно.
6. Проверить `bb machine list`, доступ к `<handle>.getbb.app`, threads и
   plugins. Если host key/identity не совпал, enrollment откажется перезаписать
   чужой data dir — не удалять каталог вслепую, сначала сверить `host-id`.
7. Откат на VM: если LXC не даёт working user bus или сервис не переживает
   reboot, поднять Debian 13 VM, повторить `install-bb.sh` и перенести сервер
   через `bb server move --to <vm> --check` с подтверждением владельца.

## 5. Проверка

```bash
curl -fsS -o /dev/null -w 'HA %{http_code}\n' http://192.0.2.28:8123/
curl -fsS -o /dev/null -w 'MA %{http_code}\n' http://192.0.2.28:8095/
curl -fsS -o /dev/null -w 'Grafana %{http_code}\n' \
  http://192.0.2.27:30300/api/health
ssh labuser@192.0.2.27 'sudo k3s kubectl get pods -A'
```

Проверить reboot host/VM без Ethernet, HTTPS ingress, voice, alarm/stop/resume,
Spotify/MA. Полный disaster restore на запасном железе ещё не выполнялся;
это не заменяется обычным reboot-тестом.

## 6. Restore OpenBao и scheduler (проверено 2026-11-02)

Учения проводятся на временном локальном стенде (Docker/Colima на Mac) и
ничего не меняют в live. Live только читается: snapshot, ExternalSecret,
synced Secrets, `ListSchedules`.

### OpenBao из raft snapshot

```bash
# при необходимости — свежий snapshot (иначе берётся последний ночной)
kubectl -n openbao create job --from=cronjob/openbao-raft-snapshot snapshot-manual

projects/home/kubernetes/openbao/restore_drill.py \
  --ssh-config secrets/proxmox-install/ssh_config --ssh-host k3s
```

Скрипт берёт тот же pinned image и `static` seal key, восстанавливает snapshot
(`sys/storage/raft/snapshot-force`), проверяет auto-unseal после restore и
после рестарта, выпускает root-токен стенда из recovery keys, сверяет по
SHA-256 каждое поле ExternalSecret с live Secret и в конце отзывает токен и
удаляет стенд. Значения не печатаются. Exit 0 — восстановление совпало.

Вручную (для реального восстановления) важно:

- `node_id = "openbao-0"`, конфиг как в `values/openbao.yaml`; иначе после
  restore нет лидера (`cannot find peer`).
- Каталог `/openbao/audit` должен быть доступен на запись, иначе post-unseal
  падает на `failed to setup audit table`.
- Пустой узел инициализировать временно (`recovery_shares=1`), его токеном
  выполнить `snapshot-force`; после restore действуют старые recovery keys
  (3 из 5) и auto-unseal тем же seal key.
- В OpenBao 2.7 unauthenticated generate-root выключен по умолчанию. Если
  нет admin-входа, временно добавить в listener
  `disable_unauthed_generate_root_endpoints = false`, выпустить root по
  recovery keys, сделать работу, отозвать токен и вернуть настройку.
- Вне кластера `kubernetes` auth не стартует (нет SA CA) — для стенда это
  ожидаемо.
- RPO ночного snapshot — до 24 ч: 2026-11-02 ночной snapshot уже не содержал
  изменений за 1 ноября. После изменения секретов снимать snapshot вручную.

### Scheduler из SQLite

Источник — каталог `backup.sh` (`local-data/local/backups/scheduler/<ts>`):

```bash
B=local-data/local/backups/scheduler/<ts>
(cd "$B" && shasum -a 256 -c SHA256SUMS)
sqlite3 -readonly "$B/scheduler-data/scheduler.db" 'pragma integrity_check'
```

Стенд: копия `scheduler.db` в отдельный каталог, live image по digest
(`kubectl -n home-assistant get deploy scheduler`), тот же env, что в
`manifests/scheduler.yaml`, свой временный token-файл, порт только на
`127.0.0.1`. HA bridge не запускать: без consumer события не доставляются.
Сравнить `GetStatus`, `ListSchedules`, `ListTimers` стенда и live
(`POST /home.v1.SchedulerService/<Method>`, `Authorization: Bearer`) по
`id`, `revision`, `nextDueUnix`, `payloadJson`. Затем удалить контейнер и
копию БД.

В live восстановление — процедура из README scheduler: scale 0, файлы в PVC
`scheduler-data`, scale 1, `GetStatus`. Правило в live не должно получить
второй исполнитель.
