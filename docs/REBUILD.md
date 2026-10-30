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

Проверять SHA-256, `quick_check`/`integrity_check`. Не публиковать.

## 1. Потеря PVE / диска

1. Установить PVE по `INSTALL-PROXMOX.md`; диск будет стёрт.
2. Восстановить SSH-ключи, сеть, timezone, storage, `vmbr0`.
3. Восстановить terraform state/private tfvars; `terraform plan` → apply.
4. Создать VM 100/101 с теми же ресурсами и MAC/static IP.
5. Восстановить HAOS и K3s (ниже).
6. Вернуть Wi-Fi профиль/`ricardo-uplink`; проверить Ethernet fallback.
7. Проверить HA/MA/Grafana/HTTPS ingress/voice.

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
6. Импортировать custom images:
   `sudo k3s ctr images import --platform linux/amd64 <archive>`.
7. Применить Helm releases из репозитория/private values.
8. Проверить pods, PV, Grafana, HTTPS ingress, voice endpoints.

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
  http://192.0.2.27:30300/grafana/api/health
ssh labuser@192.0.2.27 'sudo k3s kubectl get pods -A'
```

Проверить reboot host/VM без Ethernet, HTTPS ingress, voice, alarm/stop/resume,
Spotify/MA. Полный disaster restore на запасном железе ещё не выполнялся;
это не заменяется обычным reboot-тестом.
