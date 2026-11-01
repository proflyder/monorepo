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
HTTPS вход (LAN/VPN): `https://{ha,grafana,alarms,control}.home.example.com/`

Только LAN/VPN. Не пробрасывать `8123/8095/8099/6053/6638` и
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
`max-same-clients = 10` — не больше 10 одновременных сессий одного
пользователя.

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

### Доступ CI (`ci-deploy`)

GitHub Actions ходит в дом только сертификатом группы `ci` (свой пул, маршрут
только до K3s, nft разрешает лишь `tcp/{443,6443}` — Traefik для OpenBao и
K3s API) и работает kubectl под ServiceAccount `ci-deploy` в
`home-assistant`. SSH и root-helper на узле отсутствуют; узел принимает
`6443` только с адреса VPN-CT (пул `ci` маскируется в него).

```bash
# 1. отозвать сертификат CI (CRL обновится на CT, ocserv перезагрузится)
python3 projects/home/proxmox/vpn/vpncerts.py revoke \
  secrets/home/vpn/clients/ci-github.p12 \
  --pve 192.0.2.26 --ssh-key secrets/proxmox-install/proxmox_admin \
  --known-hosts secrets/proxmox-install/known_hosts

# 2. отозвать Kubernetes-доступ: удалить долгоживущий token Secret
kubectl -n home-assistant delete secret ci-deploy-token
# rbac.yaml создаёт его заново; после этого пересобрать kubeconfig
# (server https://<K3S_NODE>:6443, token из этого Secret).

# полный отзыв роли и аккаунта
kubectl delete -f projects/home/kubernetes/scheduler/ci/rbac.yaml
```

Ротация без отзыва: `vpncerts.py issue ci --device github --groups ci --force`.
Обновить файлы `secrets/ci/github/` и секреты Environment `home`
(`VPN_CERT_P12_B64`, `VPN_CERT_PASSWORD`).
Проверка: `kubectl auth can-i` с kubeconfig CI не должен видеть другие
namespace и cluster-scoped объекты.

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

Реальные имена и LAN_CIDR — `secrets/home/ingress.env`; токен Cloudflare
хранится в OpenBao (`secret/k8s/cert-manager/cloudflare-api-token`) и
материализуется ESO (не печатать).

## OpenBao

UI/API — `https://vault.<home-domain>/` (LAN/VPN), файлы —
`projects/home/kubernetes/openbao/`, подробности — его `README.md`.

```bash
export KUBECONFIG=secrets/kubernetes-research/kubeconfig

# состояние и логи
kubectl -n openbao get pod,svc,pvc
kubectl -n openbao exec openbao-0 -- /bin/sh -ec \
  'BAO_ADDR=http://127.0.0.1:8200 bao status'
kubectl -n openbao logs openbao-0 --tail=50

# раскатать конфигурацию (chart/values меняются — только через deploy.sh)
./projects/home/kubernetes/openbao/deploy.sh
./projects/home/kubernetes/openbao/bootstrap.sh   # идемпотентно; нужен admin-токен
./projects/home/kubernetes/openbao/verify.sh --kubernetes

# рестарт (auto-unseal, без человека)
kubectl -n openbao delete pod openbao-0

# snapshot вручную
kubectl -n openbao create job --from=cronjob/openbao-raft-snapshot snapshot-manual
kubectl -n openbao logs job/snapshot-manual
```

Пароль человека задаёт сам пользователь и меняет через UI
(`Access → Authentication methods → userpass → <user> → Edit`) или API
`bao write auth/userpass/users/<user> password=…`; файла `human.pass` в
рабочем дереве нет. В bootstrap-выводе может остаться путь к нему, но
источник правды — значение в OpenBao.

Recovery keys — `secrets/openbao/init.json` (600); root уже отозван, новый
выпускается `bao operator generate-root` из recovery keys. GitHub Environment
`Home` хранит только VPN-креды, `OPENBAO_ADDR` и (опционально)
`OPENBAO_ROLE`; `kubeconfig`, `scheduler.env` и runtime values лежат в
`secret/ci/deploy-home` и материализуются job'ом в файлы 600 из outputs
`hashicorp/vault-action` после OIDC-входа (схема и команда обновления —
README OpenBao).

## External Secrets Operator

Источник правды для приложенческих Secrets — OpenBao `secret/k8s/*`;
Kubernetes Secrets создаёт ESO. Пути, миграция и ротация —
`projects/home/kubernetes/external-secrets/README.md`.

```bash
export KUBECONFIG=secrets/kubernetes-research/kubeconfig

# установить/обновить оператор, store и все ExternalSecret
./projects/home/kubernetes/external-secrets/deploy.sh

# состояние
kubectl -n external-secrets get pods
kubectl get clustersecretstore openbao
kubectl get externalsecret -A

# немедленная синхронизация после смены значения в OpenBao
kubectl -n home-assistant annotate externalsecret scheduler-auth \
  force-sync="$(date +%s)" --overwrite
```

`deploy-home` проверяет `Ready` ExternalSecret перед выкаткой; deploy-скрипты
не создают и не перезаписывают эти Secrets. `--bootstrap-secrets` у
`ingress/deploy.sh` — только для первой установки без OpenBao/ESO.

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

Приватный runtime values (путь в `RUNTIME_PRIVATE_VALUES`, файл в `secrets/`)
обязан давать рендер, совпадающий с живым релизом `runtime`; иначе
`apply-runtime.sh` заденет несвязанные workloads/PV. Проверка (пустой diff,
код 0); `helm get values` storage не содержит — он применяется отдельно:

```bash
export KUBECONFIG=secrets/kubernetes-research/kubeconfig
helm template runtime projects/home-assistant/deploy/chart \
  -n home-assistant -f "$RUNTIME_PRIVATE_VALUES" \
  | kubectl diff -n home-assistant -f -
```

Полный `apply-runtime.sh` применяет storage (PV/PVC) через `kubectl`, а
workloads — через Helm; CI катит только workloads. Автодеплой Hermes
(build+push → digest в релиз → голосовая проверка → откат) описан в
`KUBERNETES-OPERATIONS.md`; ручной прогон:

```bash
KUBECONFIG=secrets/kubernetes-research/kubeconfig \
  python3 projects/home-assistant/deploy/hermes_rollout.py \
    --values-file "$RUNTIME_PRIVATE_VALUES" \
    --hermes-digest sha256:… --voice-proxy-digest sha256:… \
    --state-dir /tmp/hermes-state
```

`--force` — повторить upgrade с теми же digest'ами, `--simulate-smoke-failure`
— проверить откат.

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

Установлен релиз форка `v0.5.1002` (armhf, SHA256 `3a544979…`, база upstream
`v0.5.3`): Ricardo pre-roll 256 ms (`Fixed(8)`), локальный endpointing и
upstream-функции `v0.5.3` (switch часов «Clock auto brightness», Mini gen1 и
Mini 3 experimental, переработанные volume/mixer/player/pcm_out/sendspin).
`--version` теперь показывает тег релиза. Процесс запущен с
`PULSAR_LOCAL_ENDPOINTING=1`. `pulsar.prev` — предыдущая сборка `80aea2ec…`
(`v0.5.1001`, база upstream `v0.5.2`, тот же Ricardo-код).
Historical старше в `/data/pulsar/backups/` и private: `7081ef8e…`
(`v0.5.2-4-g9b5e86a-dirty`), `0b0c48e4…` (endpointing 512 ms),
`d2197f6a…` (`v0.5.2-3-gc57d521-dirty`, `pulsar.pre-endpoint-20261030`),
`69310a37…` (`pulsar.prev.pre-endpoint-20261030`); off-device private
`secrets/local/backups/pulsar-preroll8-20261030T185157Z/`.

Откат на предыдущую сборку — только на станции, не через CI: updater
предлагает лишь более новые версии. Команда ниже возвращает `pulsar.prev`
(сейчас — `v0.5.1001`, `80aea2ec…`; более старые сборки лежат в
`/data/pulsar/backups/`):

```bash
adb -s 192.0.2.13:5555 shell 'cd /data/pulsar && cp -p pulsar.prev pulsar.rollback && mv pulsar.rollback pulsar && /data/pulsar/ctl.sh reload'
```

### Релиз и деплой Pulsar

Pulsar собирается и релизится в форке (origin вложенного checkout
`projects/station/upstream/pulsar-rust`), а не в монорепо. Полная процедура —
RICARDO.md «Релизы и деплой форка» в форке. Кратко:

```bash
# 1. в форке: аннотированный тег и push (v<major.minor>.<1000+N>)
git tag -a v0.5.1003 -m 'pulsar v0.5.1003'
git push origin v0.5.1003
# build.yml собирает и публикует GitHub release (latest),
# затем запускает deploy-station.yml

# 2. approve в GitHub Environment `Station` (только теги v*)

# 3. повторный деплой уже опубликованного релиза — вручную
gh workflow run deploy-station.yml --ref v0.5.1003 -f tag=v0.5.1003
```

После approve job входит в VPN группой `ci` (отдельный клиентский сертификат
`ci-pulsar`, выпускается `vpncerts.py`) и по GitHub OIDC в OpenBao роль
`deploy-station`. Политика `ci-station` читает
`secret/ci/deploy-station` (`ha_url`, `ha_token`, `entity_id`; HA long-lived
token `ci-pulsar-deploy`, 2 года). Деплой — HA `update_entity` +
`update.install` на Firmware-сущность станции: колонка скачивает релиз,
проверяет `SHA256SUMS` и `--version`, меняет бинарник (старый — `.prev`) и
перезапускается. Job ждёт `installed_version == tag` и 90 с стабильности;
откат `supervise.sh` на `.prev` или недоступность сущности дольше 30 с валят
job. Если версия уже стоит, скрипт пишет `already installed` и выходит
успешно.

Локальные приватные копии CI-значений — `secrets/ci/pulsar/` (`VPN_*`,
`ha_url`, `ha_token`, `entity_id`); значения не печатать и не коммитить.

Полное выключение endpointing — по-прежнему reload без переменной, как выше.
Повторное включение endpointing — reload с переменной; длина pre-roll при
этом не меняется.

Установленная версия — Rust `v0.5.1002` (armhf, релиз форка на базе upstream
`v0.5.3`; повторный wake + endpointing + Ricardo pre-roll 256 ms, SHA256
`3a544979…`); `pulsar.prev` — `80aea2ec…` (`v0.5.1001`, база `v0.5.2`, тот же
Ricardo-код). Historical known-good: `7081ef8e…` (`v0.5.2-4-g9b5e86a-dirty`),
`0b0c48e4…` (512 ms endpointing), `pulsar.pre-endpoint-20261030` — `d2197f6a…`
(`v0.5.2-3-gc57d521-dirty`); `pulsar.prev.pre-endpoint-20261030` — более
старый pre-endpoint backup `69310a37…`; `v0.5.2-2-g95ce568` и
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

### Wake-события

Pulsar пишет `/data/pulsar/wake-events/`; сбор и разметка — `station_events.py`,
цикл и контракт — `../projects/station/tools/wakeword-personalize/README.md`.

```bash
# снимок новых событий + stats.json (busybox tar по ssh, без rsync)
python3 projects/station/tools/wakeword-personalize/station_events.py collect \
  --host 192.0.2.13 --ssh-key ~/.ssh/ricardo_station_ed25519 \
  --dest secrets/datasets/station-events

# разбор и метки (локальный labels.json, оригиналы не меняются)
python3 projects/station/tools/wakeword-personalize/station_events.py report \
  --events secrets/datasets/station-events
python3 projects/station/tools/wakeword-personalize/station_events.py review \
  --events secrets/datasets/station-events
python3 projects/station/tools/wakeword-personalize/station_events.py label \
  --events secrets/datasets/station-events --id EVENT_ID --label false

# manifest для finetune.py --extra-manifest
python3 projects/station/tools/wakeword-personalize/station_events.py to-manifest \
  --events secrets/datasets/station-events \
  --output PRIVATE_RUN/station-manifest.json

# калибровка cutoff/window по replay и запись micro.* в манифест модели
python3 projects/station/tools/wakeword-personalize/calibrate.py \
  --model PRIVATE_MODEL/ru_ricardo_mww.tflite \
  --positives PRIVATE_RUN/recordings --events secrets/datasets/station-events \
  --manifest PRIVATE_MODEL/ru_ricardo_mww.json
```

`collect --prune-labeled` удаляет с устройства только локально проверенные и
размеченные файлы; без флага устройство не меняется.

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
# образы собираются Bazel и импортируются в containerd по SSH, применяются kubectl.
# сборка + применение, режим исполнителя сохраняется
NODE_SSH=labuser@<node> SSH_KEY=secrets/proxmox-install/proxmox_admin \
  projects/home/kubernetes/scheduler/deploy.sh

# только один юнит (scheduler | ha-bridge | ha-control)
NODE_SSH=labuser@<node> projects/home/kubernetes/scheduler/deploy.sh --only scheduler

# осознанный переход в shadow (dry-run, 0 реплик)
NODE_SSH=labuser@<node> projects/home/kubernetes/scheduler/deploy.sh --shadow

# запуск исполнителя: preflight (в кластере или локально) + execute
HA_TOKEN_FILE=secrets/home/ha.token NODE_SSH=labuser@<node> \
  projects/home/kubernetes/scheduler/deploy.sh --execute

# NodePort-фильтр — host-конфигурация, вручную/root на узле
sudo PRIVATE_ENV=/path/scheduler.env \
  projects/home/kubernetes/scheduler/nft/apply.sh

# резервная копия SQLite (quiesce + checksums, private путь по умолчанию)
projects/home/kubernetes/scheduler/backup.sh
```

Проверки:

```bash
kubectl -n home-assistant exec deployment/scheduler -- \
  /usr/local/bin/scheduler -healthcheck
kubectl -n home-assistant exec deployment/scheduler -- \
  /usr/local/bin/scheduler -status
kubectl -n home-assistant logs deploy/ha-bridge --tail=50
```

Сценарии Home control — это HA-метка `scenario` (`CONTROL_SCENARIO_LABEL`,
default `scenario`): назначьте её скриптам в Settings → Automations & scenes →
Scripts. Иконки/комнаты/имена — только отображение. `CONTROL_LABEL` удалена.

Home control (каталог устройств/сценариев):
`http://<node>:30099` (LAN) или `https://control.home.example.com` (LAN/VPN),
тот же bearer, что у планировщика; в приложении — поле «Home control address»
без `/v1/catalog`. Переключение исполнения:
`deploy.sh --control-execute|--control-catalog`.

Endpoint для Android: `https://alarms.home.example.com` (scheduler) и
`https://control.home.example.com` (каталог) через Traefik + Let's Encrypt;
LAN endpoint — `http://<node>:30098`. Переключение Android на HTTPS-имена —
отдельный шаг. Все варианты требуют bearer.

Пауза исполнения — `deploy.sh --shadow` (0 реплик, данные scheduler
сохраняются); возврат — `--execute` после preflight. Legacy writers удалены и
повторно не включаются; два исполнителя одновременно не запускать. Бэкап/restore
БД — `backup.sh` и процедура в README scheduler.

### Android / App Distribution

Каждый PR в `develop`, затрагивающий Android-приложение (`projects/mobile/android/`
или общие Kotlin/Bazel-зависимости, из-за которых `android-app` попадает в
`android_units`), после успешной сборки `android` запускает job
`android-distribute` в `.github/workflows/ci.yml`: Environment `Android`, VPN
группа `ci`, роль OpenBao `android-distribute` → `secret/ci/android-distribute`.
Job собирает `//projects/mobile/android:app`, проверяет подпись личным ключом
(`88ef5d61…04ad`), загружает APK в Firebase App Distribution в группу `admins` («Admins»)
и удаляет ключи. PR из форков пропускается: у них нет ни OIDC-токена, ни
секретов Environment. Job не входит в `ci-ok` — неудачная загрузка видна на PR,
но не блокирует merge; новый push в тот же PR отменяет предыдущую загрузку.
Локально — `projects/mobile/android/distribute.sh` (`--dry-run` печатает
команду без загрузки). Секреты и запись KV — README
`projects/home/kubernetes/openbao`; статус — вкладка Actions прогона PR
(`android-distribute`).

## Currency bot

`solutions/currency-bot` в namespace `currency-bot`; устройство и импорт
истории — `projects/home/kubernetes/currency-bot/README.md`. Приватные
значения — `secrets/home/currency-bot.env` и `secrets/currency-bot/`.

```bash
export KUBECONFIG=secrets/kubernetes-research/kubeconfig

# деплой вручную: Bazel-образ импортируется на узел по SSH
NODE_SSH=labuser@192.0.2.27 \
  ./projects/home/kubernetes/currency-bot/deploy.sh

# состояние; ExternalSecrets должны быть SecretSynced=True
kubectl -n currency-bot get deploy,pod,svc,pvc,externalsecret
kubectl -n currency-bot get --raw \
  "/api/v1/namespaces/currency-bot/services/http:currency-bot:8080/proxy/readyz"

# логи: JSON в stdout; Alloy шипит их в Loki
kubectl -n currency-bot logs deploy/currency-bot --tail=100
kubectl -n currency-bot logs deploy/cloudflared --tail=50

# ручной прогон через LAN/VPN API (Bearer не печатать)
curl -fsS -X POST https://currency.home.example.com/api/trigger \
  -H "Authorization: Bearer <API_TOKEN>"

# webhook, который видит Telegram (токен не печатать)
curl -fsS "https://api.telegram.org/bot<BOT_TOKEN>/getWebhookInfo"
```

Webhook — `https://tg.example.com/telegram/webhook`: без secret token — 401,
с ним — 200. Метрики скрейпит `ServiceMonitor/currency-bot`; Grafana —
dashboard `currency-bot` (uid `currency-bot`).

Откат Deployment из state последнего деплоя:

```bash
python3 projects/home/kubernetes/scheduler/rollback.py \
  --state-dir secrets/local/rendered/currency-bot/state \
  --namespace currency-bot
```

Старый VPS-деплой снесён, откат только внутри K3s (выше) или fix forward;
H2-оригинал и конфиги — `secrets/currency-bot/vps-backup/`. Удаление
PVC/namespace — только после backup SQLite.

## Egress / карантин

```bash
adb -s 192.0.2.13:5555 shell \
  'iptables -S EGRESS_GUARD; ip6tables -S EGRESS_GUARD6'
adb -s 192.0.2.13:5555 shell /data/quasar/egress-guard.sh
adb -s 192.0.2.13:5555 shell /data/quasar/egress-guard-off.sh
adb -s 192.0.2.13:5555 shell /data/quasar/yandex-block.sh
```

Guard разрешает LAN/DHCP/NTP и — при `ALLOW_GITHUB=1` (по умолчанию) —
`tcp/443` только к публичным IPv4-диапазонам GitHub
(`140.82.112.0/20`, `143.55.64.0/20`, `185.199.108.0/22`, `192.30.252.0/22`)
для updater'а; публичный WAN и Яндекс — DROP. При смене диапазонов GitHub
обновить `GITHUB_NETS` в скрипте. Снятие guard не снимает Яндекс-блок/OTA.
Прежний скрипт без GitHub-разрешения — `egress-guard.sh.bak-20261031-github`.

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

После reboot проверить HA, K3s pods, Grafana, HTTPS ingress, Sendspin и
Pulsar. Аварийный доступ к PVE — LAN/Wi-Fi.

## Backup

HA full backup — через HA UI/Supervisor. K3s recovery и private values —
`KUBERNETES-OPERATIONS.md`, `REBUILD.md`. Не удалять:
`/srv/k3s/volumes/*`, `/srv/k3s/recovery`, `secrets/`, `backups/`.
