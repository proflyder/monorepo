# OPERATIONS — ежедневные команды

Факты — `STATE.md`; K3s — `KUBERNETES-OPERATIONS.md`; сеть — `NETWORK.md`.

## Доступ

```bash
# станция
adb connect 192.0.2.13:5555
adb devices
ssh -i ~/.ssh/ricardo_station_ed25519 root@192.0.2.13   # ключ; пароль — fallback

# Proxmox / K3s / HAOS: один ключ из secrets/proxmox-install/
ssh -i secrets/proxmox-install/proxmox_admin \
  -o UserKnownHostsFile=secrets/proxmox-install/known_hosts root@192.0.2.26
ssh -i secrets/proxmox-install/proxmox_admin \
  -o UserKnownHostsFile=secrets/proxmox-install/guest-known_hosts labuser@192.0.2.27
ssh -p 22222 -i secrets/proxmox-install/proxmox_admin \
  -o UserKnownHostsFile=secrets/proxmox-install/haos-known_hosts root@192.0.2.28
export KUBECONFIG=secrets/kubernetes-research/kubeconfig
```

HA: `http://192.0.2.28:8123`  
MA: `http://192.0.2.28:8095`  
Grafana LAN: `http://192.0.2.27:30300/`  
Grafana HTTPS: `https://grafana.home.example.com/`  
Grafana Tailscale: `https://pl-home.<tailnet>.ts.net/grafana/` (после смены
`root_url` путь не работает; использовать HTTPS-имя или LAN)  
HTTPS вход (LAN/VPN): `https://{ha,grafana,alarms,control}.home.example.com/`

Только LAN/VPN/Tailscale. Не пробрасывать `8123/8095/8099/6053/6638` и
`80/443` на роутере.

## VPN (CT 102)

```bash
# состояние
pct status 102
pct exec 102 -- systemctl status ocserv nftables --no-pager
pct exec 102 -- ss -lntup | grep ':443'
pct exec 102 -- nft list ruleset
pct exec 102 -- occtl show users
pct exec 102 -- occtl show ip bans

# сертификат Let's Encrypt: продление автоматическое (certbot.timer)
pct exec 102 -- certbot certificates
pct exec 102 -- certbot renew --dry-run --no-random-sleep-on-renew
pct exec 102 -- systemctl list-timers certbot.timer

# служебные действия
pct exec 102 -- ocserv --test-config
pct exec 102 -- systemctl reload ocserv    # без разрыва сессий
pct exec 102 -- systemctl restart ocserv   # нужен при смене auth-методов
pct exec 102 -- systemctl restart nftables
pct reboot 102
pct exec 102 -- occtl unban ip <ADDRESS>

# pin публичного ключа для openconnect --servercert
pct exec 102 -- sh -c \
  'openssl x509 -in /etc/ocserv/ssl/server-cert.pem -pubkey -noout \
   | openssl pkey -pubin -outform DER \
   | openssl dgst -sha256 -binary | openssl base64'
```

Добавление/ротация пользователя (пароль + TOTP; секреты не печатаются):

```bash
python3 projects/home/proxmox/vpn/vpnusers.py <user> \
  --pve 192.0.2.26 \
  --ssh-key secrets/proxmox-install/proxmox_admin \
  --known-hosts secrets/proxmox-install/known_hosts
# повторная выдача: --force
```

Приватные файлы — `secrets/home/vpn/<user>.credentials` и `<user>.png`
(QR для Google Authenticator), каталог `700`, файлы `600`.

Клиентские сертификаты (вход без пароля/TOTP; пароль + TOTP остаётся):

```bash
# один раз: частный CA; приватный ключ остаётся только на Mac
python3 projects/home/proxmox/vpn/vpncerts.py init-ca \
  --pve 192.0.2.26 --ssh-key secrets/proxmox-install/proxmox_admin \
  --known-hosts secrets/proxmox-install/known_hosts

# выпуск: p12 + legacy p12 + пароль импорта рядом (600)
python3 projects/home/proxmox/vpn/vpncerts.py issue <user> --device phone

# отзыв по serial или .p12; обновляет CRL на CT и перезагружает ocserv
python3 projects/home/proxmox/vpn/vpncerts.py revoke <serial|file.p12> \
  --pve 192.0.2.26 --ssh-key secrets/proxmox-install/proxmox_admin \
  --known-hosts secrets/proxmox-install/known_hosts

python3 projects/home/proxmox/vpn/vpncerts.py list
```

CA: `secrets/home/vpn/client-ca/` (ключ CA на сервер не копируется);
клиентские файлы — `secrets/home/vpn/clients/<user>-<device>.p12`.
На CT лежат только `/etc/ocserv/ssl/client-ca.pem` и `client-ca.crl`.
После изменения методов аутентификации нужен restart ocserv, после смены
CRL достаточно reload. Проверка конфигурации — `ocserv --test-config`.

Сертификат продлевается автоматически через DNS-01 Cloudflare
(`certbot.timer`, credentials `/etc/letsencrypt/cloudflare.ini`, deploy hook
`/etc/letsencrypt/renewal-hooks/deploy/ocserv-ssl.sh`). Ручное продление и
правка TXT-записи не нужны. Настройка и откат — `projects/home/proxmox/vpn/README.md`.

Проверка только аутентификации (не поднимает туннель на Mac; серверный
сертификат — публичный Let's Encrypt, `--servercert` не нужен):

```bash
# пароль + свежий TOTP
openconnect --authenticate --user=<user> --authgroup=lan \
  --resolve vpn.example.com:192.0.2.29 https://vpn.example.com/

# клиентский сертификат, без пароля/OTP
openconnect --authenticate --certificate <user>-phone.p12 \
  --passwd-on-stdin --authgroup=lan \
  --resolve vpn.example.com:192.0.2.29 https://vpn.example.com/ \
  < secrets/home/vpn/clients/<user>-phone.password
```

`--authenticate` печатает cookie — вывод не публиковать. Верный пароль +
свежий TOTP или валидный сертификат — успех (код 0, `COOKIE`); неверный
TOTP или отозванный сертификат — отказ. Один и тот же TOTP повторно не
использовать (ocserv считает это replay), несколько неудач подряд банят IP
на `min-reauth-time`; свой IP разбанить: `pct exec 102 -- occtl unban ip <IP>`.

## Проверка

```bash
cd ~/Desktop/yandex-station-lab
make test                    # hermetic; не трогает стенд
make test-e2e                # live HA/BLE read-only; нужны токены
STATION_E2E_MUTATE=1 make test-e2e
STATION_E2E_AUDIO=1 make test-e2e
```

`make test-e2e` требует `HA_URL` (реальный LAN-адрес HA, не пример из
документации), `HA_TOKEN`, опционально `STATION_CONTROL_TOKEN` и
`STATION_GATEWAY_URL`. Токены не коммитить и не печатать.

```bash
curl -fsS -o /dev/null -w 'HA %{http_code}\n' http://192.0.2.28:8123/
curl -fsS -o /dev/null -w 'MA %{http_code}\n' http://192.0.2.28:8095/
curl -fsS -o /dev/null -w 'Grafana %{http_code}\n' \
  http://192.0.2.27:30300/api/health
```

```bash
# HTTPS ingress (LAN/VPN)
./projects/home/kubernetes/ingress/deploy.sh --staging   # проверка DNS-01
./projects/home/kubernetes/ingress/deploy.sh             # prod-сертификат
NODE_IP=192.0.2.27 ./projects/home/kubernetes/ingress/verify.sh

KUBECONFIG=secrets/kubernetes-research/kubeconfig kubectl -n traefik get pods,certificate,ingressroute
KUBECONFIG=secrets/kubernetes-research/kubeconfig kubectl -n cert-manager get challenge,order
```

Реальные имена и LAN_CIDR — `secrets/home/ingress.env`; токен Cloudflare —
`secrets/home/vpn/cloudflare.env` (не печатать).

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

Экспериментальный локальный endpointing (см. `PENDING.md`) включается только
переменной окружения процесса Pulsar: `PULSAR_LOCAL_ENDPOINTING=1`; без неё
работает прежний путь (HA VAD + 30-секундный watchdog). `ctl.sh reload`
передаёт своё окружение новому supervisor'у, поэтому включение для текущего
процесса:

```bash
adb -s 192.0.2.13:5555 shell 'PULSAR_LOCAL_ENDPOINTING=1 /data/pulsar/ctl.sh reload'
```

Это не boot-persistent: после reboot или нового supervisor без переменной
feature снова выключен. Выключить feature без замены бинарника — новый
`adb shell` без переменной: `/data/pulsar/ctl.sh reload`. Полный откат на
known-good бинарник — атомарно, не поверх работающего файла:

```bash
adb -s 192.0.2.13:5555 shell 'cd /data/pulsar && cp -p pulsar.prev pulsar.rollback && mv pulsar.rollback pulsar && /data/pulsar/ctl.sh reload'
```

Установлен Ricardo pre-roll 256 ms (`Fixed(8)`, SHA256 `7081ef8e…`, armhf).
Строка версии не менялась (`v0.5.2-4-g9b5e86a-dirty`), поэтому отличать
установленный бинарник по SHA256 и 256 ms, а не по `--version`. Процесс
запущен с `PULSAR_LOCAL_ENDPOINTING=1`. `pulsar.prev` — прежний
endpointing-бинарник `0b0c48e4…` (512 ms); старый historical `d2197f6a…`
(`v0.5.2-3-gc57d521-dirty`) сохранён в
`/data/pulsar/backups/deploy-preroll8-20261030T185700Z/` и
`pulsar.pre-endpoint-20261030`; off-device private
`secrets/local/backups/pulsar-preroll8-20261030T185157Z/`.

Откат только pre-roll 256→512 ms, с сохранением endpointing:

```bash
adb -s 192.0.2.13:5555 shell 'cd /data/pulsar && cp -p pulsar.prev pulsar.rollback && mv pulsar.rollback pulsar && PULSAR_LOCAL_ENDPOINTING=1 /data/pulsar/ctl.sh reload'
```

Полное выключение endpointing (не откат pre-roll) — по-прежнему reload без
переменной, как выше. Повторное включение endpointing — reload с переменной;
длина pre-roll при этом не меняется. Возврат pre-roll 256 ms после бинарного
отката требует повторной установки `7081ef8e…`.

Установленная версия — Rust `v0.5.2-4-g9b5e86a-dirty` (armhf, локальный форк
на базе upstream `v0.5.2`, сборка проектным Makefile `ARCH=armhf`; повторный
wake + endpointing + Ricardo pre-roll 256 ms, SHA256 `7081ef8e…`); `pulsar.prev`
— прежний endpointing-бинарник `0b0c48e4…` (512 ms). Historical known-good:
`pulsar.pre-endpoint-20261030` — `d2197f6a…` (`v0.5.2-3-gc57d521-dirty`);
`pulsar.prev.pre-endpoint-20261030` — более старый pre-endpoint backup
`69310a37…`; `v0.5.2-2-g95ce568` и
`v0.5.0-1-g908a027` — только private backup вне Git. StationCancel без речи —
HA-конфиг, а не бинарник.
Штатный `quasar-server`
остановлен вместе со stock-стеком; при возврате на него (`ctl.sh disable` или
crash-loop) снова откроется `:31116` без auth — перед возвратом закрыть порт
правилом INPUT. Legacy C++ checkout, patch и
binaries не входят в активное дерево; приватный архив и способ восстановления —
в `secrets/local/recovery/` (не публикуется). Возврат к прежней C++-сборке не
является штатным rollback и не должен выполняться без отдельного плана.

Модель wake word меняется отдельно, model-only, без замены бинарника.
Установлен кандидат v27; best-known/reference по решению пользователя — v24.
Быстрый откат на v24 (в реальном запуске подставьте LAN-адрес HA из
`secrets/NETWORK.md`; deploy.py сам проверит alarm/idle/recorder, сохранит
backup пары и проверит hashes/HA/MA):

```bash
python3 projects/station/tools/wakeword-personalize/deploy.py \
  --ha-url http://192.0.2.28:8123 \
  --profile projects/station/configs/wakeword-v24.json
```

`wakeword-v22.json` — отдельный откат на v22. Установка не заменяет живую
голосовую приёмку: private `active.json` хранит `live_voice_acceptance=pending`.

После рестарта Pulsar MA переподключается сам: supervised `reload` завершает
Sendspin-сессию goodbye-причиной `restart`, после чего MA делает redial (на
`v0.5.2-3-gc57d521` измерено ~3 с) без рестарта add-on MA. Проверка: entity
`media_player.iandeks_stantsiia_midi` = `idle`, source `Music Assistant Queue`,
volume 0.3; в `/tmp/pulsar.log` — `sendspin: goodbye restart (daemon restarts)`,
`sendspin: connection trust unpaired` и `sendspin: server connected: Music
Assistant`. Необязательное восстановление при реальном сбое (entity остаётся
`unavailable`) — один рестарт только add-on MA через `hassio.addon_restart`
(`addon: d5369777_music_assistant`), без рестарта HA.

Проверка OTA/ADB:

```bash
projects/station/scripts/station-status.sh
projects/station/scripts/restore-ota-fortress.sh   # осторожно, откат
```


## Планировщик / HA bridge

Deploy, пауза и запуск исполнителя — `projects/home/kubernetes/scheduler/README.md`.
Короткие команды (приватные значения в `secrets/home/scheduler.env`):

```bash
# ключ ноды: secrets/proxmox-install/proxmox_admin (labuser@<node>);
# deploy.sh использует ssh/scp без -i — добавьте ключ в ssh-agent или ~/.ssh/config
# сборка + применение, режим исполнителя сохраняется
NODE_SSH=labuser@<node> projects/home/kubernetes/scheduler/deploy.sh

# осознанный переход в shadow (dry-run, 0 реплик)
NODE_SSH=root@<node> projects/home/kubernetes/scheduler/deploy.sh --shadow

# запуск исполнителя: preflight + execute
HA_TOKEN_FILE=secrets/home/ha.token NODE_SSH=root@<node> \
  projects/home/kubernetes/scheduler/deploy.sh --execute

# резервная копия SQLite (quiesce + checksums, private путь по умолчанию)
projects/home/kubernetes/scheduler/backup.sh
```

Проверки:

```bash
kubectl -n home-assistant exec deployment/scheduler -- \
  /opt/ricardo/bin/scheduler -healthcheck
kubectl -n home-assistant logs deploy/ha-bridge --tail=50
```

Сценарии Home control — это HA-метка `scenario` (`CONTROL_SCENARIO_LABEL`,
default `scenario`): назначьте её скриптам в Settings → Automations & scenes →
Scripts. Иконки/комнаты/имена — только отображение. `CONTROL_LABEL` удалена.

Home control (каталог устройств/сценариев):
`http://<node>:30099` (LAN) или `https://pl-control.<tailnet>.ts.net`
(tailnet), тот же bearer, что у планировщика; в приложении — поле
«Home control address» без `/v1/catalog`. Переключение исполнения:
`deploy.sh --control-execute|--control-catalog`.

Удалённый endpoint для Android: `https://pl-scheduler.<tailnet>.ts.net`
(tailnet-only, TLS оператора); LAN endpoint — `http://<node>:30098`.
Новые HTTPS-имена (LAN/VPN, Let's Encrypt): `https://alarms.home.example.com`
(scheduler) и `https://control.home.example.com` (каталог); переключение
Android на них — отдельный шаг. Все варианты требуют bearer. `pl-home` не
менять: `/` → HAOS, `/grafana` → Grafana.

Пауза исполнения — `deploy.sh --shadow` (0 реплик, данные scheduler
сохраняются); возврат — `--execute` после preflight. Legacy writers удалены и
повторно не включаются; два исполнителя одновременно не запускать. Бэкап/restore
БД — `backup.sh` и процедура в README scheduler.

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
Расписания и таймеры — canonical scheduler (см. `STATE.md`); старые
`todo`/`timer` helpers и legacy single-slot не использовать.

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

После reboot проверить HA, K3s pods, Grafana, HTTPS ingress, Tailscale,
Sendspin и Pulsar.
Аварийный доступ к PVE — LAN/Wi-Fi; host Tailscale не настроен.

## Backup

HA full backup — через HA UI/Supervisor. K3s recovery и private values —
`KUBERNETES-OPERATIONS.md`, `REBUILD.md`. Не удалять:
`/srv/k3s/volumes/*`, `/srv/k3s/recovery`, `secrets/`, `backups/`.
