# OPERATIONS — ежедневные команды

Факты — `STATE.md`; K3s — `KUBERNETES-OPERATIONS.md`; сеть — `NETWORK.md`.

## Единая точка входа: `mise tasks`

`mise tasks` перечисляет локальные задачи. Деплой и откат не угадывают цель по
имени каталога: список деплоимых юнитов и документированная процедура отката
живут в `platform/ci/units.mjs` (`deploy` и `rollback` блоки).

```bash
# деплой затронутых юнитов через deploy-home.yml (ref develop)
mise run deploy scheduler --dry-run     # печатает gh-команду, ничего не запускает
mise run deploy scheduler --yes         # без --yes спрашивает подтверждение
mise run deploy scheduler,ha-bridge --yes --watch
mise run deploy all --yes
# режимы deploy-home: --mode preserve|execute|shadow|control-execute|control-catalog
# (не-preserve принимает только юнит ha-bridge); --simulate-hermes-failure — учения отката hermes

# последние запуски deploy-home и подсказка проверки в кластере
mise run deploy:status
mise run deploy:status scheduler
mise run deploy:status scheduler --live  # read-only kubectl get deploy; нужен admin kubeconfig

# откат по процедуре из units.mjs
mise run rollback scheduler --dry-run   # план: kind, state-dir, команда, источник в docs
mise run rollback scheduler --yes       # выполняет документированную команду
```

`deploy` вызывает `gh workflow run deploy-home.yml --ref develop -f units=…`
(+ существующие inputs workflow). `deploy:status` только читает `gh run list`;
`--live` делает read-only `kubectl get deploy` через kubeconfig из раздела
«Доступ» (`mise run secrets:file kubernetes-research/kubeconfig`). `rollback`
печатает план всегда, выполняет команду только с `--yes`; `forward-only`
завершается ненулевым кодом, а `pulsar` лишь печатает команду для станции —
устройство автоматически не трогается. Подробности отката — в разделах ниже.

Бэкапы тоже собраны в одну точку: цель — запись реестра
`platform/ops/backups.mjs` (расписание, где, read-only проверка, команда
ручного запуска, проверка и ссылка на процедуру restore). Команды — из docs, а
хосты — через алиасы `ssh -F ~/.ssh/home-lab/config pve|k3s|haos`:

```bash
mise run backup:list                      # таблица целей, без сети
mise run backup:status --dry-run          # печатает read-only команды проверки
mise run backup:status                    # сводка ok/fail/manual по всем целям
mise run backup:status offsite-restic
mise run backup:now offsite-restic --dry-run   # план: команда, note, restore doc
mise run backup:now offsite-restic --yes       # без --yes спрашивает подтверждение
mise run backup:verify --dry-run          # restic check / integrity_check и т.п.
mise run backup:verify offsite-restic --yes
```

`backup:now` выполняет только документированную команду выбранной цели; цель,
доступная только через UI (HA backup), печатает `manual` и ничего не запускает.
Restore намеренно не автоматизирован: вывод всегда содержит `restore_doc`.
Подробности и цели — раздел «Backup» ниже.


## Доступ

```bash
# станция
adb connect 192.0.2.13:5555
adb devices
ssh -i ~/.ssh/home-lab/station_ed25519 root@192.0.2.13   # ключ; пароль — fallback

# Proxmox / K3s / HAOS: ключ и config из `mise run secrets:pull`
ssh -F ~/.ssh/home-lab/config pve
ssh -F ~/.ssh/home-lab/config k3s 'sudo -n k3s kubectl get pods -A'
ssh -F ~/.ssh/home-lab/config haos

# admin kubeconfig для примеров ниже (API 6443 — через туннель, см.
# KUBERNETES-OPERATIONS.md); после работы — mise run secrets:clean …
mise run secrets:file kubernetes-research/kubeconfig
export KUBECONFIG=secrets/kubernetes-research/kubeconfig
```

HA: `http://192.0.2.28:8123`  
MA: `http://192.0.2.28:8095`  
Grafana LAN: `http://192.0.2.27:30300/`  
Grafana HTTPS: `https://grafana.home.example.com/`  
HTTPS вход (LAN/VPN): `https://{ha,grafana,alarms,control,currency,vault}.home.example.com/`

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
  --ssh-key ~/.ssh/home-lab/proxmox_admin \
  --known-hosts ~/.ssh/home-lab/known_hosts_pve
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
  --pve 192.0.2.26 --ssh-key ~/.ssh/home-lab/proxmox_admin \
  --known-hosts ~/.ssh/home-lab/known_hosts_pve

# выпуск: p12 + legacy p12 + пароль импорта рядом (600)
python3 projects/home/proxmox/vpn/vpncerts.py issue <user> --device phone

# отзыв по serial или .p12; обновляет CRL на CT и перезагружает ocserv
python3 projects/home/proxmox/vpn/vpncerts.py revoke <serial|file.p12> \
  --pve 192.0.2.26 --ssh-key ~/.ssh/home-lab/proxmox_admin \
  --known-hosts ~/.ssh/home-lab/known_hosts_pve

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
  --pve 192.0.2.26 --ssh-key ~/.ssh/home-lab/proxmox_admin \
  --known-hosts ~/.ssh/home-lab/known_hosts_pve

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

## BB server (CT 103)

Создание и установка — `projects/home/proxmox/bb/README.md`. Основные
команды (реальные IP/MAC — `secrets/NETWORK.md`):

```bash
# состояние CT, Node и user-сервиса bb
ssh root@192.0.2.26 'pct status 103'
ssh root@192.0.2.26 'pct exec 103 -- node --version'
ssh root@192.0.2.26 \
  "pct exec 103 -- bash -lc 'uid=\$(id -u bb); \
     runuser -u bb -- env XDG_RUNTIME_DIR=/run/user/\$uid \
       systemctl --user list-units \"bb-host-daemon-*\" --no-pager'"

# согласованный SQLite backup и ночной vzdump
ssh root@192.0.2.26 'pct exec 103 -- systemctl list-timers bb-db-backup.timer'
ssh root@192.0.2.26 'pct exec 103 -- systemctl start bb-db-backup.service'
ssh root@192.0.2.26 \
  'pct exec 103 -- sqlite3 /var/backups/bb/bb.db "PRAGMA integrity_check;"'
ssh root@192.0.2.26 'pvesh get /cluster/backup/nightly'

# перенос сервера: только после подтверждения владельца
bb settings experiment serverMove true
bb server move --to bb --check
```

`/etc/bb-backup/env` содержит `BB_DATA_DIR`; после enrollment путь берётся из
`bb machine show bb`. Если файла базы нет, таймер завершается успешно с
предупреждением. Enrollment-команда из UI одноразовая: выполнять её в shell
пользователя `bb`, не сохранять в репозитории и логах.

Плагины `profiles` и `home-dispatcher` — path-источники на CT; команды
выполнять в shell пользователя `bb` (см. выше):

```bash
# плагины и их настройки
bb plugin list
bb plugin config home-dispatcher
bb profiles list
bb profiles show          # профиль, провайдеры и машины проекта

# сжатие рабочего диспетчера: расписание и статус
bb plugin list | grep -i home-dispatcher

# проверка моста с K3s-узла (ключ шлюза; вывод — только JSON плагина)
printf '%s' '{"method":"targets","params":{}}' |
  ssh -i /etc/mcp-gateway/bb_dispatch_key -o BatchMode=yes bb@192.0.2.30
```

Ротация: `bb plugin token home-dispatcher --rotate` + перезаписать
`/home/bb/.config/home-dispatcher/token` (0600); смена `hmacSecret` требует
одновременно обновить `/home/bb/.config/home-dispatcher/hmac`, иначе плагин
отвечает 401. Значения не печатать.

## Проверка

```bash
cd <repo>
mise run test                # hermetic; не трогает стенд
mise run test:e2e            # live HA/BLE read-only; нужны токены
STATION_E2E_MUTATE=1 mise run test:e2e
STATION_E2E_AUDIO=1 mise run test:e2e
```

`mise run test:e2e` требует `HA_URL` (реальный LAN-адрес HA, не пример из
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

Операторский токен для KV (без входа человека): `secrets/openbao/operator.token`,
политика `operator-kv` (`configure.py`: `secret/{ci,k8s,infra}/*`, без
delete), orphan, period 30 дней.

Секреты владельца (SSH-ключи, CA, бэкап-пароли, admin kubeconfig) — в
`secret/infra/*`; каталог путей — `docs/SECRETS.md`, рабочее место и задачи
mise — `docs/WORKSTATION.md`. Скрипт, читающий файл вида
`secrets/home/ingress.env`, запускать после временного восстановления:

```bash
mise run secrets:file home/ingress.env    # или -- --all
mise run secrets:clean home/ingress.env
mise run secrets:check                    # каталог ↔ OpenBao
```

Не-секретные локальные данные (бэкапы, датасеты, логи) — `local-data/`
(git-ignored); инструменты — `mise install`.

```bash
# BAO_ADDR уже в окружении mise; токен — только на время команды
BAO_TOKEN=$(cat secrets/openbao/operator.token) bao token lookup
BAO_TOKEN=$(cat secrets/openbao/operator.token) bao kv get secret/ci/deploy-home
# новый токен машины: mise run secrets:login (пароль + TOTP); каждая задача
# secrets:* продлевает period 30 дней
```

Пароль человека задаёт сам пользователь и меняет через UI
(`Access → Authentication methods → userpass → <user> → Edit`) или API
`bao write auth/userpass/users/<user> password=…`; файла `human.pass` в
рабочем дереве нет. В bootstrap-выводе может остаться путь к нему, но
источник правды — значение в OpenBao.

Recovery keys и seal key — у владельца в Bitwarden (`Home recovery`) и в Vaultwarden (`openbao:bootstrap`), не на диске; root уже отозван, новый
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

## Vaultwarden (менеджер паролей)

`https://passwords.<home-domain>/` (LAN/VPN), клиенты Bitwarden (Self-hosted,
Server URL — этот адрес). Файлы, первый вход, смена пароля `/admin` и
восстановление — `projects/home/kubernetes/vaultwarden/README.md`.

```bash
KUBECONFIG=<admin kubeconfig> projects/home/kubernetes/vaultwarden/deploy.sh
kubectl -n vaultwarden get pod,pvc,externalsecret
kubectl -n vaultwarden logs deploy/vaultwarden --tail=50
```

Пароль `/admin` — `secret/infra/vaultwarden/admin`; в под попадает только
Argon2id-хэш (`secret/k8s/vaultwarden/vaultwarden-admin`).

## Внешний heartbeat (dead-man switch)

`Watchdog` уходит на Cloudflare Worker `solutions/heartbeat`: ping раз в
5 минут, при тишине дольше `GRACE_SECONDS` Worker пишет в Telegram через тот
же инфра-бот, при восстановлении — recovery. Развёрнут как `home-heartbeat`
на `workers.dev`; Cloudflare API token (Workers Scripts + KV edit) —
`secrets/home/cloudflare-workers.env`, остальное — `secrets/home/heartbeat.env`.

```bash
# тесты без сети и без Cloudflare
node --test solutions/heartbeat/src/*.test.js

# приватные значения: secrets/home/heartbeat.env (в .gitignore)
# CLOUDFLARE_ACCOUNT_ID, CLOUDFLARE_API_TOKEN, KV_NAMESPACE_ID,
# PING_TOKEN, TELEGRAM_BOT_TOKEN, TELEGRAM_CHAT_ID
./projects/home/cloudflare/heartbeat/deploy.sh
./projects/home/cloudflare/heartbeat/deploy.sh --secrets

# liveness, затем ping без токена и с токеном (значение не печатать)
curl -fsS "https://<worker>.<account-subdomain>.workers.dev/health"
curl -sS -o /dev/null -w '%{http_code}\n' -X POST \
  "https://<worker>.<account-subdomain>.workers.dev/ping"        # 401
set -a; . secrets/home/heartbeat.env; set +a
curl -sS -o /dev/null -w '%{http_code}\n' -X POST \
  -H "Authorization: Bearer $PING_TOKEN" -H 'content-type: application/json' \
  -d '{"status":"firing"}' \
  "https://<worker>.<account-subdomain>.workers.dev/ping"        # 200

# OpenBao: те же URL и PING_TOKEN для Alertmanager
bao kv put secret/k8s/monitoring/heartbeat \
  url="https://<worker>.<account-subdomain>.workers.dev/ping" \
  token="$PING_TOKEN"
unset PING_TOKEN

# применить Alertmanager (ESO рендерит webhook_configs из этой записи)
./projects/home/kubernetes/deploy-observability.sh
kubectl -n monitoring get externalsecret alertmanager-telegram
```

Ротация: новый `PING_TOKEN` — обновить private env, выполнить
`deploy.sh --secrets`, затем `bao kv put` с новым значением; ESO обновит
Secret, Alertmanager перечитает config. Смена Telegram-бота — обновить
`TELEGRAM_BOT_TOKEN`/`TELEGRAM_CHAT_ID` в env Worker, выполнить
`--secrets` и обновить `k8s/home-assistant/hermes-environment`, чтобы
Alertmanager и Hermes остались на том же боте. Cloudflare API token нужен
только deploy.sh и в OpenBao/Kubernetes не попадает.

Outage-тест без остановки остальных алертов — silence только на Watchdog:

```bash
kubectl -n monitoring exec alertmanager-monitoring-alertmanager-0 -c alertmanager -- \
  amtool silence add alertname=Watchdog --duration=25m \
  --comment="heartbeat outage test" --alertmanager.url=http://127.0.0.1:9093
```

Через `GRACE_SECONDS` + cron-тик (~16–20 мин) приходит «дом молчит»; после
истечения silence первый ping даёт recovery.

## MCP-шлюз (голосовая диагностика)

Голосом: «проверь сервисы», «что с гермесом», «почему не работает X»,
«перезапусти планировщик» (два шага с подтверждением), «пришли отчёт».
Установка, RBAC и ротация — `KUBERNETES-OPERATIONS.md`.

```bash
export KUBECONFIG=secrets/kubernetes-research/kubeconfig

# состояние и метрики
kubectl -n home-assistant get deploy,pod -l app.kubernetes.io/name=mcp-gateway
kubectl -n home-assistant logs deploy/mcp-gateway --tail=100
kubectl -n home-assistant get --raw \
  "/api/v1/namespaces/home-assistant/services/http:mcp-gateway:8080/proxy/readyz"

# каталог, который реально смонтирован (сверка с manifests/catalog.yaml)
kubectl -n home-assistant get configmap mcp-gateway-catalog \
  -o jsonpath='{.data.catalog\.yaml}' | head -40

# секреты синхронизированы
kubectl -n home-assistant get externalsecret mcp-gateway-auth mcp-gateway-telegram
```

Шлюз не публикуется наружу: порт-форвард нужен только для отладки MCP-клиента
и требует Bearer-токен из `Secret/mcp-gateway-auth`.

```bash
kubectl -n home-assistant port-forward svc/mcp-gateway 18080:8080
```

Голос и агенты bb:

```bash
# маршруты включены и токены на месте
kubectl -n home-assistant get externalsecret mcp-gateway-bb-ssh mcp-gateway-voice
kubectl -n home-assistant logs deploy/mcp-gateway --tail=100 | grep -E '"bb"|"voice"'

# самодиагностика distroless-образа: shell/curl/python внутри нет
kubectl -n home-assistant exec deploy/hermes -c voice-proxy -- \
  /usr/local/bin/voice-proxy -healthcheck
kubectl -n home-assistant exec deploy/hermes -c voice-proxy -- \
  /usr/local/bin/voice-proxy -status

# состояние голосового маршрута и bootstrap обновлений диспетчера:
# kubectl-side port-forward вместо exec в контейнер, токен не печатается
kubectl -n home-assistant port-forward svc/mcp-gateway 18081:8080 &
VOICE_TOKEN=$(kubectl -n home-assistant get secret mcp-gateway-voice \
  -o jsonpath='{.data.token}' | base64 -d)
curl -fsS -H "Authorization: Bearer $VOICE_TOKEN" \
  http://127.0.0.1:18081/v1/voice/state
# рабочий профиль: только cursor, без истории
curl -fsS -H "Authorization: Bearer $VOICE_TOKEN" \
  -H "Content-Type: application/json" -d "{}" \
  http://127.0.0.1:18081/v1/voice/updates
kill %1 2>/dev/null || true
unset VOICE_TOKEN
```

Голосовой прокси сам опрашивает `/v1/voice/updates` и озвучивает завершённые
ходы диспетчера, которые начал не владелец (`system`/`agent`), только пока
профиль «Рабочий». Выключается `HERMES_VOICE_UPDATES=0`; подробности —
`solutions/voice-proxy/README.md`.

Профиль и цвет кольца станции: HA `input_select.ricardo_voice_profile`,
`switch.ricardo_work_profile`; станция —
`select.iandeks_stantsiia_midi_voice_color` (Blue = рабочий).

Проверка моста и плагинов — в разделе «BB server (CT 103)». Ручной эквивалент
для диагностики (обходит аудит шлюза): `printf` JSON в
`ssh -i /etc/mcp-gateway/bb_dispatch_key bb@<BB_CT_IP>` с K3s-узла.

Ротация: `mcp-gateway-auth` — перезапустить `mcp-gateway` и `hermes`;
`mcp-gateway-voice` — перезапустить `mcp-gateway` и `hermes` (voice-proxy);
`mcp-gateway-bb-ssh` — заменить ключ на CT (строка `authorized_keys`) и в
OpenBao одновременно. Список личных целей — `manifests/bb-targets.yaml`,
рендерится `deploy.sh` из `BB_PROJECT_*`.

## Комп (Windows-ПК)

Адрес, MAC и пользователь — только в HA `secrets.yaml`
(`komp_ping_command`, `komp_mac`, `komp_ssh_target`); не печатать их.

```bash
# из HA: Terminal & SSH add-on — состояние по ping
ping -c 1 -W 1 <host>

# из HA Terminal & SSH — ограниченный SSH (только shutdown|cancel)
ssh -i /config/.ssh/komp_ed25519 \
  -o BatchMode=yes -o ConnectTimeout=5 \
  -o UserKnownHostsFile=/config/.ssh/komp_known_hosts \
  -o StrictHostKeyChecking=accept-new <user>@<host> shutdown

# ключ и known_hosts вне Git, но персистентны внутри HAOS
ls -l /config/.ssh/komp_ed25519 /config/.ssh/komp_known_hosts
```

В HA: Actions `shell_command.komp_shutdown` / `shell_command.komp_cancel_shutdown`,
`wake_on_lan.send_magic_packet` (mac из секрета), сущность `switch.komp`.
Голосом: «включи комп», «выключи комп», «отмени выключение компа»,
«комп включён». Выключение откладывается на 60 с, отмена — в этом окне.

### wakegrant — пропуск входа компа

`switch.komp.turn_on` сначала выдаёт одноразовый пропуск
(`rest_command.wakegrant_issue`), затем отправляет WoL. Сервис —
`projects/home/kubernetes/wakegrant`, LAN NodePort `30130`
(`WAKE_GRANT_NODE_PORT`), протокол —
`packages/contracts/wakegrant/PROTOCOL.md`. Секреты: OpenBao
`secret/k8s/home-assistant/wakegrant` (`issuer_token`, `machines`), в HA —
`wakegrant_endpoint` и `wakegrant_bearer` в `secrets.yaml`.

```bash
# здоровье
curl -sS http://<k3s-node>:30130/healthz

# журнал: только machine/outcome/grantId, без nonce/MAC/токенов
kubectl -n home-assistant logs deploy/wakegrant --tail=50

# тестовый пропуск; токен читается из приватного файла, не печатается
curl -sS -X POST http://<k3s-node>:30130/v1/grants \
  -H "Authorization: Bearer $(<secrets/home/wakegrant.token)" \
  -H 'Content-Type: application/json' \
  -d '{"machine":"komp","ttlSeconds":60}'
# ожидаемо: 201 {"grantId":"…","expiresAt":"…"};
# 401 — не совпал issuer_token, 400 — нет machine в Secret `machines`
```

Если сервис недоступен, включение компа продолжает работать:
`rest_command.wakegrant_issue` помечен `continue_on_error: true`, и ПК
показывает обычный вход. Пропуск одноразовый, живёт 180 с, повторная выдача
заменяет предыдущий. Перезапуск пода не теряет выданный пропуск (SQLite на
PVC), но сбрасывает выданные nonce — ПК просто запросит новый.

### Модуль входа (logon provider)

Провайдер входа — переиспользуемое решение `solutions/windows-logon-provider`
(Rust, Bazel/rules_rust + zig под `x86_64-pc-windows-gnu`, без Visual Studio).
Значения `ServiceUrl`, `Machine` и
`UserName` в решении не хранятся: их передаёт проект при установке (для этого
ПК — параметры ниже).

Распространение — публичный OCI-артефакт GHCR, а не artifact Actions. Любое
изменение путей провайдера (и общих Rust/Cargo-входов) собирается, проверяется
и публикуется PR CI как `tree-<hash>` с provenance (build once). На push в
`develop` `native-release` продвигает проверенный `tree-<hash>` на релизные
теги, ничего не собирая (нет артефакта или provenance — job падает);
`publish_when` по `solutions/windows-logon-provider/VERSION` ограничивает
только релиз:

```text
ghcr.io/proflyder/windows-logon-provider:<VERSION>
ghcr.io/proflyder/windows-logon-provider:sha-<12 hex коммита>
```

Файлы артефакта: `RicardoKompLogon.dll`, `cp-selftest.exe`, `SHA256SUMS`.
Тег `<VERSION>` неизменяем: повторная публикация с тем же содержимым — no-op,
с другим — ошибка, поэтому релиз
делается сменой `VERSION`. Provenance attestation создаётся для digest
артефакта и для DLL. PR из этого репозитория публикует тот же артефакт под
`tree-<hash>` с provenance (build once); форки и `workflow_dispatch` только
собирают.

Пакет публичный (унаследовал видимость публичного репозитория при первом
publish), анонимный `oras pull` работает без токена. Если новый пакет другого
решения окажется приватным: **профиль владельца → Packages → `<пакет>` →
Package settings → Danger Zone → Change visibility → Public**.

Получение (Mac/Linux оператора; `oras` и `gh` — из `mise install`, `gh` авторизован):

```bash
cd solutions/windows-logon-provider
./fetch.sh <VERSION>                 # или
./fetch.sh sha256:<digest>           # рекомендуемый pin
```

`fetch.sh` тянет артефакт в временный каталог, проверяет `SHA256SUMS` и
`gh attestation verify` для DLL, печатает разрешённый digest. Любое
расхождение — ошибка. Затем скопировать файлы на ПК по SSH, запустить
self-test, установить/заменить DLL и записать digest в `docs/STATE.md`
(там же — текущая установленная версия).

Установка (PowerShell от администратора; `out` — распакованный артефакт):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1 `
  -ServiceUrl 'http://192.0.2.27:30130' `
  -Machine 'komp' -UserName '<user>' -SourceDir 'C:\path\to\out'

# пароль — SecureString (интерактивно), ключ — одной строкой в stdin
'BASE64KEY' | powershell -NoProfile -ExecutionPolicy Bypass -File set-secret.ps1

powershell -NoProfile -ExecutionPolicy Bypass -File status.ps1
```

`install.ps1` требует `-ServiceUrl`, `-Machine` и `-UserName` (домашние
значения — только в команде проекта, не в решении) и сбрасывает
`FailCount=0` и `Disabled=0`. Обновление DLL:
LogonUI держит файл, поэтому `Copy-Item -Force` падает — сначала переименовать
старый, затем скопировать новый, старый удалить после выхода из системы или
перезагрузки:

```powershell
$dir = 'C:\Program Files\Ricardo\KompLogon'
Move-Item "$dir\RicardoKompLogon.dll" "$dir\RicardoKompLogon.dll.old" -Force
Copy-Item .\RicardoKompLogon.dll "$dir\RicardoKompLogon.dll" -Force
# .old удалить позже, когда файл больше не занят
```

`set-secret.ps1` не печатает значения: пароль запрашивается как SecureString,
ключ читается одной строкой из stdin; echo/историю не использовать.
`status.ps1` показывает конфигурацию, регистрацию, DLL, `FailCount`/`Disabled`
и последние строки журнала.

Журнал: `C:\ProgramData\Ricardo\KompLogon\logon.log` (ротация в
`logon.log.1` при 256 КиБ). Читать административной SSH-сессией; ключ HA
ограничен только `shutdown`/`cancel` и журнал не покажет:

```bash
ssh <user>@<host> \
  "powershell -NoProfile -Command \"Get-Content 'C:\\ProgramData\\Ricardo\\KompLogon\\logon.log' -Tail 50\""
```

Self-test (запускать, когда грант не ожидается — `cp-selftest.exe` на
мгновение поднимает фоновый поток):

```powershell
.\cp-selftest.exe     # загрузка провайдера по CLSID вне LogonUI
```

После трёх неудачных входов провайдер ставит `Disabled=1` и больше ничего не
делает; снять latch — переустановить (`install.ps1` обнуляет
`FailCount`/`Disabled`) или поправить значения в реестре.

Удаление — без перезагрузки, регистрации снимаются сразу; журнал остаётся:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File uninstall.ps1
# -KeepSecret — не удалять secret.bin
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
workloads — через Helm; CI катит только workloads. Скрипту нужен digest
Health exporter'а: `HEALTH_EXPORTER_IMAGE=ghcr.io/proflyder/health-exporter@sha256:…`
(взять текущий: `kubectl -n home-assistant get deployment
runtime-health-exporter -o jsonpath='{.spec.template.spec.containers[0].image}'`);
он подставляет image, command и пустые volume-монтирования вместо удалённого
Python-скрипта/ConfigMap. Автодеплой Hermes
(build+push → digest в релиз → голосовая проверка → откат) описан в
`KUBERNETES-OPERATIONS.md`; ручной прогон:

```bash
KUBECONFIG=secrets/kubernetes-research/kubeconfig \
  home-deploy hermes rollout \
    --values-file "$RUNTIME_PRIVATE_VALUES" \
    --hermes-digest sha256:… --voice-proxy-digest sha256:… \
    --exporter-digest sha256:… \
    --smoke-image ghcr.io/proflyder/home-deploy@sha256:… \
    --state-dir /tmp/hermes-state
```

`--force` — повторить upgrade с теми же digest'ами, `--simulate-smoke-failure`
— проверить откат. План отката и документированную команду печатает
`mise run rollback hermes --dry-run`.

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

Цвет анимации слушания по профилю — HA select
`select.iandeks_stantsiia_midi_voice_color` (Violet = личный, Blue = рабочий);
его выставляет автоматизация `voice_profile_station_color`; если сущности
нет, автоматизация ничего не делает.

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

Установленная версия и состав — `docs/STATE.md`; `--version` показывает тег
релиза. Откат на предыдущую сборку — только на станции, не через CI
(updater предлагает лишь более новые версии): команда выше возвращает
`pulsar.prev`. Напомнить команду и источник: `mise run rollback pulsar`
(только печать — станция автоматически не трогается).

### Релиз и деплой Pulsar

Исходники Pulsar — `projects/station/pulsar/` в монорепозитории. GitHub Releases и теги
запрещены; релиз публикуется в публичный OCI-пакет
`ghcr.io/proflyder/pulsar-firmware` (его читает `pulsar_update::UPDATE_REPO`).
Полная процедура — `projects/station/pulsar/RICARDO.md`, «Релизы и деплой». Кратко:

```bash
# 1. в рабочей ветке изменить версию (v<major.minor>.<1000+N>)
$EDITOR projects/station/pulsar/VERSION
# 2. обычный PR в develop, squash после зелёного ci-ok;
#    PR уже собрал release-shaped артефакт (обе архитектуры с версией из
#    VERSION, radio .ybl, SHA256SUMS, проверка --version под qemu) и
#    опубликовал его как tree-<git tree hash> с build provenance;
#    merge проводит pulsar через deploy-home (publish_when по VERSION):
#    native-release проверяет tree-тег и provenance и ставит на тот же
#    digest ghcr.io/proflyder/pulsar-firmware:<version> и :latest.
#    Если tree-тега нет или provenance не сходится — job падает:
#    deploy-home ничего не собирает и не публикует. Если develop ушёл
#    вперёд до squash-merge, берётся head PR, но только когда дельта
#    head→merge не затрагивает pulsar; иначе — подтянуть develop в PR,
#    дождаться CI и смержить заново.
# 3. approve в GitHub Environment `Station` (job deploy-station)
#    → VPN группа ci → OIDC в OpenBao (deploy-station)
#    → HA update_entity + update.install
# 4. повторный запуск уже опубликованной версии — workflow_dispatch
#    deploy-home с units=pulsar: катится только deploy-station, текущий
#    VERSION без сборки и push; units=all тоже включает deploy-station.
#    Если OCI-тег версии уже существует с другим digest, promotion/publish
#    падает: выпускается следующая версия; тот же digest — no-op
# 5. прочие изменения Pulsar — только PR CI, релизные теги не двигаются
```

После approve job `deploy-station` входит в VPN группой `ci` (отдельный клиентский сертификат
`ci-pulsar`, выпускается `vpncerts.py`) и по GitHub OIDC в OpenBao роль
`deploy-station`. Политика `ci-station` читает
`secret/ci/deploy-station` (`ha_url`, `ha_token`, `entity_id`; HA long-lived
token `ci-pulsar-deploy`, 2 года). Деплой — HA `update_entity` +
`update.install` на Firmware-сущность станции: колонка скачивает слои OCI,
проверяет `SHA256SUMS` и `--version`, меняет бинарник (старый — `.prev`) и
перезапускается. Job ждёт `installed_version == version` и 90 с стабильности;
откат `supervise.sh` на `.prev` или недоступность сущности дольше 30 с валят
job. Если версия уже стоит, скрипт пишет `already installed` и выходит
успешно.

Ручной sync upstream:

```bash
# в checkout upstream (Morvex885/pulsar) получить диапазон и применить патчем
git -C <upstream-checkout> diff vOLD..vNEW > /tmp/pulsar-upstream.patch
git apply -3 --directory=projects/station/pulsar /tmp/pulsar-upstream.patch
# дальше обычные проверки и PR; конфликты разрешать вручную
```

Локальные приватные копии CI-значений — `secrets/ci/pulsar/` (`VPN_*`,
`ha_url`, `ha_token`, `entity_id`); значения не печатать и не коммитить.

Полное выключение endpointing — по-прежнему reload без переменной, как выше.
Повторное включение endpointing — reload с переменной; длина pre-roll при
этом не меняется.

StationCancel без речи — HA-конфиг, а не бинарник.
Штатный `quasar-server`
остановлен вместе со stock-стеком; при возврате на него (`ctl.sh disable` или
crash-loop) снова откроется `:31116` без auth — перед возвратом закрыть порт
правилом INPUT.

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
  --host 192.0.2.13 --ssh-key ~/.ssh/home-lab/station_ed25519 \
  --dest local-data/datasets/station-events

# разбор и метки (локальный labels.json, оригиналы не меняются)
python3 projects/station/tools/wakeword-personalize/station_events.py report \
  --events local-data/datasets/station-events
python3 projects/station/tools/wakeword-personalize/station_events.py review \
  --events local-data/datasets/station-events
python3 projects/station/tools/wakeword-personalize/station_events.py label \
  --events local-data/datasets/station-events --id EVENT_ID --label false

# manifest для finetune.py --extra-manifest
python3 projects/station/tools/wakeword-personalize/station_events.py to-manifest \
  --events local-data/datasets/station-events \
  --output PRIVATE_RUN/station-manifest.json

# калибровка cutoff/window по replay и запись micro.* в манифест модели
python3 projects/station/tools/wakeword-personalize/calibrate.py \
  --model PRIVATE_MODEL/ru_ricardo_mww.tflite \
  --positives PRIVATE_RUN/recordings --events local-data/datasets/station-events \
  --manifest PRIVATE_MODEL/ru_ricardo_mww.json
```

`collect --prune-labeled` удаляет с устройства только локально проверенные и
размеченные файлы; без флага устройство не меняется.

После рестарта Pulsar MA переподключается сам: supervised `reload` завершает
Sendspin-сессию goodbye-причиной `restart`, после чего MA делает redial
(~3 с) без рестарта add-on MA. Проверка: entity
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


## Артефакты GHCR: сборка на PR и продвижение

Образ деплой-юнита и native-артефакт (Pulsar, Windows logon provider)
собираются один раз на PR и после merge не пересобираются (решение 36). PR
публикует их как `ghcr.io/proflyder/<image>:tree-<git tree hash>` с build
provenance; `deploy-home` проверяет тег и provenance:

* образы — ставит на тот же digest теги `<commit sha>` и `develop`;
* native — ставит `native.publish_tags`: для Pulsar `<version>` и `latest`,
  для logon provider `<version>` и `sha-<12>`. Версия неизменяема: тот же
  digest — no-op, другой — отказ; двигается только `latest`.

`deploy-home` — только доставка: он не запускает `bazel build`/`bazel run`
и не собирает Docker-образы. Native-юниты (Pulsar, logon provider) тоже
только продвигаются: если `tree-<hash>` не опубликован или provenance не
сходится, job `native-release` падает с подсказкой перезапустить PR CI; история
для них не используется, потому что релизный артефакт должен соответствовать
текущему tree и `VERSION`. `deploy-station` только устанавливает уже
опубликованный `<version>` через HA и ничего не собирает. Для каждого
затронутого деплой-юнита
`_images.yml` дополнительно публикует `ghcr.io/proflyder/home-deploy`
(если юнит не собирает его сам); job достаёт из промоутнутого образа
бинарник `home-deploy` (`crane export`) и передаёт его скриптам через
`HOME_DEPLOY`. Бинарник собирает `rust_service_image` из
`//platform/deploy-rs:home-deploy`; Go-реализация удалена.

Ручной `workflow_dispatch` (смена режима ha-bridge, `units=all`) выкатывает
текущий `develop`, для которого у нетронутых последним PR юнитов нет
`tree-<hash>`-тега. В этом случае promotion идёт по first-parent истории
(~200 коммитов) и берёт самый свежий коммит с существующим и проверенным
`tree-<hash>`; коммит-источник виден в summary. Так же разрешаются
образы-соседи, которые общий deploy-скрипт рендерит, но не применяет
(`deploy.optional_images` в `units.mjs`, например ha-bridge/ha-control при
деплое юнита scheduler). Зависимости от живого кластера нет, поэтому
механизм работает и при первой установке; аварийное восстановление с нуля всё
равно удобнее делать локальным запуском deploy-скрипта (там сборка разрешена).

### Squash-merge на уехавший `develop`

Squash-коммит сохраняет tree PR только пока `develop` не ушёл вперёд. Если
между прогоном PR CI и merge влился другой PR, у merge-коммита другой tree и
`tree-<hash>` для него нет (PR #204: проверен `310954ce`, влит `fe392157`
после docs-merge #205). На push в этом случае работает PR-head fallback:

1. `promote.mjs` находит PR по коммиту (`gh api
   repos/{repo}/commits/{sha}/pulls`, затем `(#N)` из заголовка squash-коммита)
   и берёт его head `H`;
2. `git fetch origin refs/pull/N/head`, `H` сверяется с ответом API;
3. дельта `git diff H <merge-commit>` уходит в `affected.compute`; если юнит в
   дельту не попал, его входы в merge-коммите идентичны проверенным на `H`,
   и продвигается `tree-<tree(H)>` (после обычной проверки provenance). В
   summary: `from PR head <H12>, delta to <merge12> does not affect <unit>`;
4. если дельта затрагивает юнит — job падает: проверенные и влитые байты
   разные. Лечение: подтянуть `develop` в PR, дождаться зелёного CI и
   смержить заново (или `workflow_dispatch` после следующего merged PR).

Та же логика применяется к `home-deploy` и образам-соседям (у них сначала
история, потом PR head) и к native-юнитам (`_native.yml`,
`runNativePromotion`): релиз, собранный на head PR, продвигается, только если
дельта не затрагивает native-юнит; иначе `native-release` падает. Ошибка
routing'а дельты — отказ (fail closed), а не разрешение.

Профилактика: merge выполнять через `mise run pr:merge <N>` — он подтягивает
`develop` в PR и ждёт `ci-ok` на итоговом head до merge, поэтому уехавшая база
не возникает (docs/GIT.md, раздел «Merge»). Fallback выше остаётся страховкой
`deploy-home`, а не штатным путём.

Pulsar собирается release-shaped на каждом PR, затрагивающем его код, — с
версией из `VERSION`, radio и `SHA256SUMS`; тег версии создаётся только при
merge с изменением `VERSION`.

### Воспроизводимая сборка Hermes

`hermes` — единственный образ деплоя, собираемый Dockerfile'ом. Его собирает
общий скрипт `projects/home-assistant/deploy/images/hermes/build.sh`, который
вызывает PR CI (`_images.yml`). Сборка запинена по digest базовых образов:
apt только из snapshot Debian, `SOURCE_DATE_EPOCH` = время upstream-коммита, а
Buildx переписывает mtime слоёв (`rewrite-timestamp=true`). Поэтому две сборки
одной ревизии дают один digest и неподвижный образ не вызывает rollout. В
`deploy-home` `build-and-push.sh` только принимает промоутнутые digest'ы
(`DEPLOY_NO_BUILD=1`) и ничего не собирает.

```bash
# две сборки без кэша в разные каталоги → сравнение digest'ов
projects/home-assistant/deploy/images/hermes/verify-reproducible.sh
# ожидается «reproducible: sha256:…»
```

PR CI (`_images.yml`) читает общий кэш слоёв Buildx
`ghcr.io/proflyder/hermes:buildcache`; пишут его (`mode=max`) только job'ы,
которые уже залогинены в GHCR с правом записи. Кэш переиспользует слои с теми
же входами (apt, `uv`), поэтому ускоряет сборку, но не меняет digest. Пакет
должен оставаться публичным, иначе чтение кэша из PR без логина будет
пропущено.

Обновление Hermes — bump `projects/home-assistant/deploy/images/hermes/revision`
(и при необходимости digest'ов баз и `DEBIAN_SNAPSHOT` в `Dockerfile`); путь
`projects/home-assistant/deploy/images/hermes/**` привязан к деплой-юниту
`hermes`, так что изменение доезжает до кластера.

Как понять, что использовал деплой:

* вкладка Actions → прогон `deploy-home` → summary job'а
  `deploy <unit>`: строка `image path: prebuilt` (взят проверенный PR-образ);
  `build` означает, что promotion отказал и job упал до выкатки (причина — в
  блоке `image promotion`);
* там же видны digest'ы образов и `runtime revision` Helm-релиза.

Если для образа нет ни текущего tree, ни верифицированной сборки в истории,
job падает с понятной ошибкой; автосборки на деплой-раннере нет. Частая
причина — `develop` ушёл вперёд между PR и squash-merge: тогда promotion
пробует head PR (см. «Squash-merge на уехавший `develop`»). Если и это
отказано, в summary будет `from PR head <H12> … changes <unit>`: нужно
подтянуть `develop` в PR, дождаться CI и смержить заново. Локально сборка
остаётся поведением по умолчанию; в CI её запрещает `DEPLOY_NO_BUILD=1`.

Локально путь продвижения можно повторить, передав проверенные digest'ы
(без них скрипты собирают и пушат как обычно):

```bash
PREBUILT_DIGESTS="scheduler=sha256:…,ha-bridge=sha256:…,ha-control=sha256:…" \
  NODE_SSH=labuser@<node> projects/home/kubernetes/scheduler/deploy.sh
```

Чистка GHCR — `.github/workflows/ghcr-cleanup.yml`, ежедневно по расписанию.
Пакеты — образы деплой-юнитов и `native.publish_repository` из
`units.mjs`, включая `pulsar-firmware` и `windows-logon-provider`. Политика по
каждому пакету: не удаляются новейшая версия, теги `develop`, `latest`, semver
и native `sha-<12>`, digest'ы из `PROTECTED_DIGESTS` (сейчас пусто) и версии
моложе 7 дней. Deploy-версии (40-hex commit-sha тег) сохраняют пять новейших, более
старые старше 7 дней удаляются. Устаревшие версии только с `tree-*` или без
тегов удаляются. Referrer-версии с тегами `sha256-<hex>` удаляются вместе со
своим digest'ом или если его уже нет в пакете; в окно пяти deploy-версий они
не входят. Каждое решение с причиной пишется в job summary.

```bash
# локальный dry-run политики на фикстуре (в GitHub не ходит, ничего не удаляет)
node platform/ci/ghcr_cleanup.mjs \
  --fixture platform/ci/testdata/ghcr-versions.json --now 2026-11-04T00:00:00Z
```

Реальный ручной прогон: Actions → `ghcr-cleanup` → Run workflow,
`dry_run` по умолчанию `true` (снять галочку — удалять). Если `GITHUB_TOKEN`
не может удалять версии user-owned пакетов, job падает с подсказкой: нужен
секрет `GHCR_CLEANUP_TOKEN` (токен с правом удаления пакетов и доступом к
репозиторию).

## Планировщик / HA bridge

Deploy, пауза и запуск исполнителя — `projects/home/kubernetes/scheduler/README.md`.
Короткие команды (приватные значения в `secrets/home/scheduler.env`):

```bash
# ключ ноды: ~/.ssh/home-lab/proxmox_admin (labuser@<node>);
# образы собираются Bazel и импортируются в containerd по SSH, применяются kubectl.
# сборка + применение, режим исполнителя сохраняется
NODE_SSH=labuser@<node> SSH_KEY=~/.ssh/home-lab/proxmox_admin \
  projects/home/kubernetes/scheduler/deploy.sh

# только один юнит (scheduler | ha-bridge | ha-control; ha-bridge катит и ha-scenario)
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

Очередь озвучивания агента: `-status` печатает raw GetStatus, в `queues`
смотрите `voice-work` / `voice-personal` (`pending`, `leased`) и consumer
`voice-proxy-home` в `consumers` (`alive` после первого цикла). Растущий
`leased` без `pending` означает, что станция занята или ответ не подтверждён;
`FAILED` после `SCHEDULER_EVENT_MAX_ATTEMPTS` пишется в `detail` (без текста).
Логи — `kubectl -n home-assistant logs deploy/hermes -c voice-proxy`: строки
`queue enqueued|delivered|stale|announce failed` содержат только структуру.
`HERMES_VOICE_QUEUE=0` возвращает прямой announce без consumer'а.

Сценарии Home control — это HA-метка `scenario` (`CONTROL_SCENARIO_LABEL`,
default `scenario`): назначьте её скриптам в Settings → Automations & scenes →
Scripts. Иконки/комнаты/имена — только отображение. `CONTROL_LABEL` удалена.
Нативные сценарии scheduler (`home.v1.ScenarioService`) в этот каталог не
входят: каталог строится только из сущностей HA.

Home control (каталог устройств/сценариев):
`http://<node>:30099` (LAN) или `https://control.home.example.com` (LAN/VPN),
тот же bearer, что у планировщика; в приложении — поле «Home control address»
без `/v1/catalog`. Переключение исполнения:
`deploy.sh --control-execute|--control-catalog`.

«Сказать на колонке» (`action.station_speak`) требует HA-скрипт
`script.speak_text`. Проверка каталога (bearer — тот же, что у control):

```bash
curl -fsS -H "Authorization: Bearer $CONTROL_TOKEN" \
  https://control.home.example.com/v1/catalog \
  | jq '.entries[] | select(.kind == "action") | {id, title, device, room, actions}'
```

Записи нет — значит `script.speak_text` отсутствует или скрыт в HA. Команда
идёт обычной задачей `home.control`/очередь `control`:
`{"entry":"action.station_speak","action":"speak","text":"…"}` (1–500
символов, без управляющих). Результат — `ACK_DELIVERED` после 2xx HA;
`ACK_FAILED` означает неверный текст/неизвестный entry, detail текста не
содержит. Скрипт молчит при `input_boolean.alarm_ringing = on` и при
неготовом сателлите откатывается на `schedule_announce`. Текст не пишется в
логи, ledger, `detail` и отчёты — не добавляйте его в диагностические команды.

### Исполнитель сценариев (ha-scenario)

Определения хранит scheduler (`home.v1.ScenarioService`), исполняет отдельный
Deployment `ha-scenario` на очереди `scenario`. Он катится вместе с юнитом
`ha-bridge` (`deploy.sh --only ha-bridge`), 1 реплика, `SCENARIO_EXECUTE=true`;
ledger — `/data/scenario-ledger.json`. Проверка:

```bash
kubectl -n home-assistant exec deployment/scheduler -- \
  /usr/local/bin/scheduler -status \
  | jq '{scenario_store_supported, scenario_execution_supported, queues}'
kubectl -n home-assistant logs deploy/ha-scenario --tail=50
kubectl -n home-assistant exec deploy/ha-scenario -- \
  /usr/local/bin/ha-scenario -healthcheck
```

`scenario_execution_supported=true` — это флаг scheduler
`SCENARIO_EXECUTION=true` в манифесте, а не догадка о наличии пода: при
выключенном исполнителе флаг снимается тем же PR/деплоем.

Создать сценарий и запустить вручную (`$SCHEDULER_TOKEN` — тот же bearer, что
у планировщика; адрес — `https://alarms.home.example.com` или LAN NodePort):

```bash
curl -fsS -X POST -H "Authorization: Bearer $SCHEDULER_TOKEN" \
  -H 'Content-Type: application/json' \
  https://alarms.home.example.com/home.v1.ScenarioService/CreateScenario \
  -d '{
    "label": "Evening light",
    "enabled": true,
    "triggers": [{"manual": {}}],
    "conditions": [{"entityState": {"entityId": "sun.sun", "state": "below_horizon"}}],
    "steps": [
      {"catalog": {"entryId": "scene.focus", "action": "run"}},
      {"delay": {"seconds": 2}},
      {"speak": {"text": "Добрый вечер."}}
    ],
    "continueOnError": false
  }' | jq -r '.scenario.id'

# manual-запуск: событие scenario.fire встаёт в очередь scenario
curl -fsS -X POST -H "Authorization: Bearer $SCHEDULER_TOKEN" \
  -H 'Content-Type: application/json' \
  https://alarms.home.example.com/home.v1.ScenarioService/RunScenario \
  -d '{"id":"<scenario-id>"}' | jq '{id: .event.id, queue: .event.queue}'
```

Time-триггер добавляется так же: `{"time":{"timezone":"Asia/Almaty",
"at":{"hour":7,"minute":30},"repeat":"REPEAT_KIND_DAILY"}}`; scheduler сам
материализует расписание и ставит `scenario.fire`. Окно условий
`{"timeWindow":{"start":{"hour":22},"end":{"hour":6},"weekdays":[1,2,3,4,5]}}`
поддерживает переход через полночь; timezone условий — у первого
time-триггера, иначе `SCENARIO_TIMEZONE`.

### Сид бытовых сценариев

«Блэкаут» и «Сел работать» засеваются идемпотентно из
`projects/home/kubernetes/scheduler/scenarios/` (`blackout.json`,
`work_mode.json`, `seed.mjs`). Сид читает `ListScenarios`, создаёт только
сценарии, чьей метки ещё нет (`idempotencyKey=seed-<slug>`), и никогда не
перезаписывает существующие: правки владельца в приложении сохраняются.
Токен передаётся файлом, а не аргументом; в выводе только метки и id.

```bash
SCHEDULER_URL=https://alarms.home.example.com \
SCHEDULER_TOKEN_FILE=secrets/home/scheduler.token \
  node projects/home/kubernetes/scheduler/scenarios/seed.mjs --dry-run   # план
SCHEDULER_URL=https://alarms.home.example.com \
SCHEDULER_TOKEN_FILE=secrets/home/scheduler.token \
  node projects/home/kubernetes/scheduler/scenarios/seed.mjs             # создать
```

`keep` — сценарий с такой меткой уже есть, `created` — создан,
`would` — попал бы в план при `--dry-run`. Проверка после сида:

```bash
curl -fsS -X POST -H "Authorization: Bearer $SCHEDULER_TOKEN" \
  -H 'Content-Type: application/json' \
  https://alarms.home.example.com/home.v1.ScenarioService/ListScenarios \
  -d '{"includeDisabled":true}' | jq -r '.scenarios[].label'
```

Голос запускает их через `script.scenario_run` по точной метке; ручная
правка метки в приложении сломает голосовую команду, поэтому метки —
часть контракта. Тест сида:
`node --test projects/home/kubernetes/scheduler/scenarios/seed.test.mjs`.

Выключить исполнение (манифест при следующем `deploy.sh` сохранит
env/реплики, поэтому ручное выключение не откатывается само):

```bash
kubectl -n home-assistant scale deployment/ha-scenario --replicas=0
# либо оставить под, но не забирать очередь:
kubectl -n home-assistant set env deployment/ha-scenario SCENARIO_EXECUTE=false
```

Перед снятием флага исполнения снять `SCENARIO_EXECUTION` с scheduler не
обязательно, но честнее: `kubectl -n home-assistant set env deployment/scheduler
SCENARIO_EXECUTION=false` (манифест вернёт `true` при следующем деплое).

Диагностика: `ACK_SKIPPED` с detail `condition not met` / `scenario is
disabled` — ожидаемое поведение; `FAILED` `step N: …` — шаг не выполнился,
повторно он не запускается. `FAILED interrupted` означает, что процесс упал
после начала шагов; проверьте ledger и выполните сценарий вручную. Текст
`speak` не попадает в логи, ledger, `detail` и отчёты — не добавляйте его в
диагностические команды.

Endpoint для Android: `https://alarms.home.example.com` (scheduler) и
`https://control.home.example.com` (каталог) через Traefik + Let's Encrypt;
LAN endpoint — `http://<node>:30098`. Переключение Android на HTTPS-имена —
отдельный шаг. Все варианты требуют bearer.

Пауза исполнения — `deploy.sh --shadow` (0 реплик, данные scheduler
сохраняются); возврат — `--execute` после preflight. Legacy writers удалены и
повторно не включаются; два исполнителя одновременно не запускать. Бэкап/restore
БД — `backup.sh` и процедура в README scheduler.

### Android / App Distribution

Каждый PR в `develop`, затрагивающий Android-приложение (изменился
`projects/mobile/android/**`, общая Kotlin-библиотека `packages/kotlin/**` или
`platform/android/**`; routing выставляет `android_distribute` = true), после
успешной сборки `android` запускает job `android-distribute` в
`.github/workflows/ci.yml`: Environment `Android`, VPN группа `ci`, роль OpenBao
`android-distribute` → `secret/ci/android-distribute`. Глобальная инвалидация
(например `MODULE.bazel`) по-прежнему прогоняет проверки `android`, но APK не
публикует. Исключение — правка `MODULE.bazel` только внутри Maven-расширения
(`maven.install(...)`, `use_repo(maven, ...)`): она меняет состав APK, поэтому
маршрутизируется как `MODULE.bazel#maven` только на `android` и публикует APK;
scoped-изменения native-входов (`MODULE.bazel#native`, `.bazelrc#cache`) не
выбирают `android` и APK не публикуют. Job собирает `//projects/mobile/android:app`, проверяет подпись личным ключом
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

# LAN NodePort для голосового intent «курс доллара/рубля» (HA Assist).
# CURRENCY_BOT_NODE_PORT — приватное значение из secrets/home/currency-bot.env;
# наружу порт не пробрасывается, на узле его фильтрует nft.
curl -fsS "http://192.0.2.27:<CURRENCY_BOT_NODE_PORT>/api/latest" \
  -H "Authorization: Bearer <API_TOKEN>"

# однократно на K3s-узле: тот же LAN/pod allowlist, что у scheduler NodePort
ssh root@192.0.2.27 \
  env PRIVATE_ENV=/path/to/currency-bot.env \
  bash projects/home/kubernetes/currency-bot/nft/apply.sh

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

Откат Deployment из state последнего деплоя (для всех юнитов с записанным
state есть общая точка входа `mise run rollback <unit> --dry-run`):

```bash
home-deploy state rollback \
  --state-dir secrets/local/rendered/currency-bot/state \
  --namespace currency-bot
```

Старый VPS-деплой снесён, откат только внутри K3s (выше) или fix forward;
H2-оригинал и конфиги — `local-data/currency-bot/vps-backup/`. Удаление
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

Профиль голоса (Личный/Рабочий) — `input_select.ricardo_voice_profile`;
менять только вручную: голосом («рабочий профиль», «личный профиль», «какой
профиль»), на дашборде «Помощник» → «Голос» или карточкой «Личный | Рабочий»
в Android (`switch.ricardo_work_profile`, метка `voice_profile`). Проверка без
колонки — тихим `conversation.process`:

```text
conversation.process: "рабочий профиль"
conversation.process: "какой профиль"
conversation.process: "личный профиль"
```

Свободная речь идёт по профилю: рабочий → постоянный тред-диспетчер через
`mcp-gateway /v1/voice/utterance` (ответ озвучивает `script.schedule_announce`);
личный + режим агента → активный тред bb; иначе Hermes. Профиль не читается —
«Не могу подтвердить профиль», в Hermes ничего не уходит. Маршруты и мост
bb — раздел «MCP-шлюз (голосовая диагностика)».

Курс валют: «курс доллара», «курс рубля», «курс валют» — локальный intent
`StationCurrencyRate`; HA читает `GET /api/latest` с LAN NodePort
currency-bot. В `secrets.yaml` — `currency_bot_endpoint` (base URL без `/api`)
и `currency_bot_bearer` (`Bearer <API_TOKEN>` — тот же токен, что у API).
Проверка без колонки:

```bash
python3 -m unittest discover -s projects/home-assistant/ha-voice -p 'test_*.py' -v
```

После применения ha-voice в HA: `conversation.process: "курс доллара"`.

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

Все цели, их расписание, read-only проверки и команды ручного запуска —
`mise run backup:list` (реестр `platform/ops/backups.mjs`, `status`/`verify` —
из этого раздела). Proxmox: ночной vzdump `nightly` (04:00) на SATA SSD,
storage `backup`.

```bash
ssh pve 'pvesh get /cluster/backup; pvesm list backup | tail'
ssh pve 'vzdump 101 --storage backup --mode snapshot --compress zstd'  # вручную
ssh pve 'qmrestore /mnt/pve/backup/dump/<archive> <new-vmid> --storage local-lvm'
```

Проверочный restore — в новый VMID без сети (`--unique`, отключить `net0`),
не поверх живой VM. HA full backup — через HA UI/Supervisor (automatic
encrypted backup 03:30, 3 копии, агент `hassio.local`). K3s recovery и private
values — `KUBERNETES-OPERATIONS.md`, `REBUILD.md`. Не удалять:
`/srv/k3s/volumes/*`, `/srv/k3s/recovery`, `secrets/`, `backups/`.

### Offsite backup (Google Drive + restic)

Live с 2026-11-02: K3s node, таймер `offsite-backup.timer` 04:45 →
restic-репозиторий `rclone:gdrive:home-offsite-v2` (личный Google Drive, scope
`drive.file`: rclone видит только файлы своего OAuth client; свой client
«Desktop app» в Google Cloud project `home-offsite-backup`). Установка/обновление с Mac:

```bash
# Private inputs (вне Git):
#   secrets/home/offsite-backup.env      RESTIC_REPOSITORY, RCLONE_CONFIG, HA_URL, лимиты
#   secrets/home/offsite-restic.password restic password (копия — password manager)
#   secrets/home/offsite-rclone.conf     rclone remote `gdrive` (OAuth token)
#   secrets/home/offsite-ha.token        отдельный HA long-lived token
SSH_CONFIG=~/.ssh/home-lab/config projects/home/backup/offsite/install.sh k3s
SSH_CONFIG=~/.ssh/home-lab/config projects/home/backup/pve/install.sh pve
# PVE scrape target и Grafana server — secrets/home/observability-values.yaml
KUBECONFIG=secrets/kubernetes-research/kubeconfig projects/home/kubernetes/deploy-observability.sh
```

Новый OAuth-токен Google: `rclone authorize drive <base64 JSON
{"client_id","client_secret","scope":"drive.file"}>` с client из
`secrets/home/offsite-gdrive-client.env`; вывод — base64-JSON с полем `token`;
собрать `[gdrive] type=drive client_id=… client_secret=… scope=drive.file
token=…` в `offsite-rclone.conf` и переустановить. Смена client = новый
репозиторий (drive.file не видит файлы другого client): новая папка, `restic
init` выполнит установщик. Лимиты: `MAX_STAGING_BYTES` (2 GiB) и `MAX_REPO_BYTES`
(5 GiB) — превышение = неуспешный запуск и алерт, не загрузка.

Проверки и обслуживание:

```bash
ssh root@192.0.2.27 'systemctl list-timers offsite-backup.timer'
ssh root@192.0.2.27 'systemctl start offsite-backup.service'
ssh root@192.0.2.27 'journalctl -u offsite-backup.service -n 100 --no-pager'
ssh root@192.0.2.27 'cat /var/lib/node-exporter-textfile/offsite_backup.prom'
ssh root@192.0.2.27 'set -a; . /etc/offsite-backup/env; set +a; restic snapshots'  # нужен rclone
ssh root@192.0.2.26 'cat /var/lib/prometheus/node-exporter/vzdump.prom'
```

Retention: daily `restic forget --keep-daily 7 --keep-weekly 4
--keep-monthly 6`; prune и `restic check --read-data-subset=5%` — по
воскресеньям. Смена restic password: `restic key add`, затем удалить старый
ключ, обновить `/etc/offsite-backup/restic.password` и копию вне дома.
Восстановление — `projects/home/backup/offsite/README.md` и `REBUILD.md`.
Отзыв доступа к Drive — Google Account → Безопасность → сторонние приложения
→ rclone (отзывает и рабочий токен: затем новый `rclone authorize`).
