# REBUILD — восстановление стенда

Текущая конфигурация — `STATE.md`; установка — `INSTALL-PROXMOX.md`;
K3s — `KUBERNETES-OPERATIONS.md`.

## Recovery-комплект

Приватно, вне Git:

- `secrets/proxmox-install/` — ISO, install settings, SSH keys, TF state,
  HA/K3s recovery, Wi-Fi profile.
- `secrets/kubernetes-research/recovery/` — cluster/auth и custom OCI images.
- `secrets/local/backups/` — HA config/DB, MA/STT/TTS, станция.
- `secrets/local/snapshots/nettop/` — PVE/VM конфигурация.
- `secrets/openbao/` — recovery keys, seal key, username; `secrets/home/vpn/` —
  CA и клиентские сертификаты VPN.

Проверять SHA-256, `quick_check`/`integrity_check`. Не публиковать.

## 1. Потеря PVE / диска

1. Установить PVE по `INSTALL-PROXMOX.md`; диск будет стёрт.
2. Восстановить SSH-ключи, сеть, timezone, storage, `vmbr0`.
3. Восстановить terraform state/private tfvars; `terraform plan` → apply.
4. Создать VM 100/101 с теми же ресурсами и MAC/static IP.
5. Восстановить HAOS и K3s (ниже).
6. Вернуть Wi-Fi профиль/`ricardo-uplink`; проверить Ethernet fallback.
7. Пересоздать CT 102 `vpn` скриптами `projects/home/proxmox/vpn/`;
   восстановить пользователей/клиентские сертификаты или выпустить новые
   (старые `.p12` в `secrets/home/vpn/clients/`), проверить `443/tcp+udp`.
8. Проверить HA/MA/Grafana/HTTPS ingress/voice/VPN.

## 2. HAOS

1. Установить HAOS VM 100; UEFI, 2 vCPU/4 GiB/32 GiB.
2. Восстановить supported HA backup через Supervisor onboarding.
3. Проверить `internal_url`, trusted proxy, MFA, add-ons MA/Matter.
4. MA data: `/mnt/data/supervisor/apps/data/d5369777_music_assistant`;
   Matter: `core_matter_server`.
5. Проверить HA integration MA, Sendspin player, alarm media URLs.
6. Не запускать HA Container параллельно с HAOS.

## 3. K3s

1. VM 101: Ubuntu 24.04, 8 GiB/200 GiB, static IP, MAC из private tfvars.
2. Установить **ту же** K3s version; data-dir `/srv/k3s/runtime`.
3. Остановить K3s перед восстановлением datastore.
4. Вернуть SQLite cluster snapshot, token, TLS/cred, config.
5. Вернуть `/srv/k3s/volumes/*` до старта Pods.
6. Поднять образы: сначала попробовать pull из GHCR по digest
   (`scheduler`, `ha-bridge`, `ha-control`, `hermes`, `voice-proxy`); если
   пакеты недоступны — импортировать OCI-архивы
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

- ADB/SSH: только LAN; пароль — `secrets/CREDENTIALS.md`.
- Pulsar: `/data/pulsar`; бинарник/модель/rollback — `artifacts/wakeword/`
  и `projects/station/resources/`.
- Factory reset сносит `/data`, root/SSH/OTA/guard. Делать только с
  согласованием и запасной станцией.
- После восстановления: ESPHome entities, wake word, HA pipeline, Sendspin,
  MA/Spotify, egress guard, BLE gateway.

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

Источник — каталог `backup.sh` (`secrets/local/backups/scheduler/<ts>`):

```bash
B=secrets/local/backups/scheduler/<ts>
(cd "$B" && shasum -a 256 -c SHA256SUMS)
sqlite3 -readonly "$B/scheduler-data/scheduler.db" 'pragma integrity_check'
```

Стенд: копия `scheduler.db` в отдельный каталог, live image по digest
(`kubectl -n home-assistant get deploy scheduler`), тот же env, что в
`manifests/scheduler.yaml`, свой временный token-файл, порт только на
`127.0.0.1`. HA bridge не запускать: без consumer события не доставляются.
Сравнить `GetStatus`, `ListSchedules`, `ListTimers` стенда и live
(`POST /ricardo.v1.SchedulerService/<Method>`, `Authorization: Bearer`) по
`id`, `revision`, `nextDueUnix`, `payloadJson`. Затем удалить контейнер и
копию БД.

В live восстановление — процедура из README scheduler: scale 0, файлы в PVC
`scheduler-data`, scale 1, `GetStatus`. Правило в live не должно получить
второй исполнитель.

В бэкапах до 2026-11-02 `SHA256SUMS` содержит строку для самого себя;
ошибка `shasum -c` только на `./SHA256SUMS` ложная.
