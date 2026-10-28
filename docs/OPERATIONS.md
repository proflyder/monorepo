# OPERATIONS — ежедневные команды

Факты — `STATE.md`; K3s — `KUBERNETES-OPERATIONS.md`; сеть — `NETWORK.md`.

## Доступ

```bash
# станция
adb connect 192.0.2.13:5555
adb devices
ssh root@192.0.2.13                 # пароль: secrets/CREDENTIALS.md

# Proxmox / K3s
ssh root@192.0.2.26                 # PVE
ssh labuser@192.0.2.27              # K3s VM
export KUBECONFIG=secrets/kubernetes-research/kubeconfig
```

HA: `http://192.0.2.28:8123`  
MA: `http://192.0.2.28:8095`  
Grafana LAN: `http://192.0.2.27:30300/grafana/`  
Grafana Tailscale: `https://pl-home.<tailnet>.ts.net/grafana/`

Только LAN/Tailscale. Не пробрасывать `8123/8095/8099/6053/6638`.

## Проверка

```bash
cd ~/Desktop/yandex-station-lab
make test                    # hermetic; не трогает стенд
make test-e2e                # live HA/BLE read-only; нужны токены
STATION_E2E_MUTATE=1 make test-e2e
STATION_E2E_AUDIO=1 make test-e2e
```

`make test-e2e` требует `HA_TOKEN`, опционально `STATION_CONTROL_TOKEN`.
Токены не коммитить и не печатать.

```bash
curl -fsS -o /dev/null -w 'HA %{http_code}\n' http://192.0.2.28:8123/
curl -fsS -o /dev/null -w 'MA %{http_code}\n' http://192.0.2.28:8095/
curl -fsS -o /dev/null -w 'Grafana %{http_code}\n' \
  http://192.0.2.27:30300/grafana/api/health
```

## K3s

```bash
ssh labuser@192.0.2.27
sudo k3s kubectl get pods -A
sudo k3s kubectl get deploy,pvc -A
sudo k3s kubectl top nodes
sudo systemctl status k3s
```

С Mac без прямого API:

```bash
ssh -N -L 127.0.0.1:16443:127.0.0.1:6443 labuser@192.0.2.27
export KUBECONFIG=secrets/kubernetes-research/kubeconfig
kubectl get pods -A
```

Helm releases и обновление — `KUBERNETES-OPERATIONS.md`.

## HA / MA / Matter / K3s компоненты

```bash
# HAOS
curl -fsS http://192.0.2.28:8123/api/ | head
# MA
curl -fsS http://192.0.2.28:8095/ | head
# Matter — только внутри HAOS
# ws://127.0.0.1:5580/ws
```

В K3s:

```bash
sudo k3s kubectl -n home-assistant get pods
sudo k3s kubectl -n monitoring get pods
```

Не запускать второй writer для HA/MA/SQLite. Один Deployment — одна replica,
Recreate; Helm rollback не откатывает state.

## Станция / Pulsar

```bash
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh status'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh log 100'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh restart'
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh disable'  # вернуть maind
adb -s 192.0.2.13:5555 shell '/data/pulsar/ctl.sh enable'   # Pulsar
adb -s 192.0.2.13:5555 shell '/data/pulsar/pulsar --version'
```

Текущая версия — Rust `v0.5.0-1-g908a027`. C++ rollback —
`/data/pulsar/backups/pre-ricardo*`; протокол — `WAKE-WORD.md`.

Rust → C++ rollback:

```bash
adb -s 192.0.2.13:5555 shell \
  'cp /data/pulsar/pulsar.prev /data/pulsar/pulsar && \
   chmod 755 /data/pulsar/pulsar && /data/pulsar/ctl.sh restart'
```

C++ copy также в `/data/pulsar/backups/pre-rust-ricardo6/pulsar`.

Проверка OTA/ADB:

```bash
projects/station/scripts/station-status.sh
projects/station/scripts/restore-ota-fortress.sh   # осторожно, откат
```

## Egress / карантин

```bash
adb -s 192.0.2.13:5555 shell \
  'iptables -S EGRESS_GUARD; ip6tables -S EGRESS_GUARD6'
adb -s 192.0.2.13:5555 shell /data/quasar/egress-guard.sh
adb -s 192.0.2.13:5555 shell /data/quasar/egress-guard-off.sh
adb -s 192.0.2.13:5555 shell /data/quasar/yandex-block.sh
```

Guard разрешает LAN/DHCP/NTP; публичный WAN и Яндекс — DROP.
Снятие guard не снимает Яндекс-блок/OTA.

## BLE gateway

```bash
adb -s 192.0.2.13:5555 shell /data/quasar/ble-gateway/status.sh
adb -s 192.0.2.13:5555 shell /data/quasar/ble-gateway/start.sh
adb -s 192.0.2.13:5555 shell /data/quasar/ble-gateway/stop.sh
adb -s 192.0.2.13:5555 shell 'tail -40 /data/quasar/ble-gateway/ble-gateway.log'
```

GATT: service `a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d`; SSID `…4c01`,
PSK `…4c02`, token `…4c03`, commit `…4c04`, status `…4c05`.
Токен — `/data/quasar/ble-gateway/token` (600), не публиковать.
Кнопки: 1 — mute, 3 — gateway 15 мин, 4 — Bluetooth 90 с, 5 с — gateway
вместо Yandex setup. 4 нажатия могут уронить Wi-Fi.

## Голос / расписания

Проверка pipeline и фраз — `projects/home-assistant/ha-voice/README.md`.
Будильники: `todo.alarms_once/daily/weekdays/weekends`; таймеры
`timer.taimer_1…8`; напоминания `todo.reminders`; заметки `todo.notes`.
Legacy `input_datetime.alarm_next` выключен; не использовать.

Hermes:

```bash
# внутри K3s VM/Pod
curl -fsS http://127.0.0.1:8642/health
curl -fsS http://127.0.0.1:8643/v1/models
```

`8642` — loopback; `8643` — только HAOS/K3s/loopback.

## Reboot / откат

```bash
# HAOS/VM
ssh root@192.0.2.26
qm reboot 100       # HAOS
qm reboot 101       # K3s

# PVE
reboot
```

После reboot проверить HA, K3s pods, Grafana, Tailscale, Sendspin и Pulsar.
Аварийный доступ к PVE — LAN/Wi-Fi; host Tailscale не настроен.

## Backup

HA full backup — через HA UI/Supervisor. K3s recovery и private values —
`KUBERNETES-OPERATIONS.md`, `REBUILD.md`. Не удалять:
`/srv/k3s/volumes/*`, `/srv/k3s/recovery`, `secrets/`, `backups/`.
