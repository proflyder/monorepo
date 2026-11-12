# NETWORK — публичная карта стенда

**Назначение:** обезличенное описание домашней сети, портов и удалённого
доступа. Документ можно публиковать.

> **Важно:** все адреса ниже — примеры из документационного диапазона
> `192.0.2.0/24` (RFC 5737), а не реальные адреса стенда. Реальная карта
> с LAN-IP и именами VPN/HTTPS хранится отдельно, в приватном
> `secrets/NETWORK.md`, и не публикуется.

Текущее состояние и версии — `STATE.md`. Этот файл описывает сетевую
архитектуру и классы доступа, а не конкретные текущие параметры.

## 1. Схема

```text
Интернет
   │
   ├── Cloudflare Tunnel ──► K3s cloudflared
   │        публично tg.example.com/telegram/webhook
   │        и ci-nexus.<public-zone>/repository/* (Nexus, read-only)
   │
   └── роутер / gateway / DNS       192.0.2.1
          │
          └── LAN 192.0.2.0/24, плоский L2, без VLAN
                 │
                 ├── Proxmox VE host               192.0.2.26
                 │      vmbr0, Wi-Fi 4addr / Ethernet
                 │      ├── VM 100  HAOS          192.0.2.28
                 │      │      HA + MA + Matter add-ons
                 │      ├── VM 101  Ubuntu/K3s    192.0.2.27
                 │      │      STT/TTS, Hermes, intercom, Nexus, observability
                 │      ├── CT 102  vpn           192.0.2.29
                 │      │      ocserv, OpenConnect, TOTP
                 │      └── CT 103  bb            192.0.2.30
                 │             bb server, SQLite, исходящий Connect
                 │
                 ├── Станция Midi                 192.0.2.13
                 │      Pulsar, ESPHome, Sendspin
                 │
                 └── Mac (DHCP, не фиксирован)    192.0.2.11
```

Компоненты:

- **Proxmox host** — гипервизор и management-точка; не публикует HA.
- **HAOS VM** — Home Assistant, Music Assistant и Matter Server add-ons.
- **K3s VM** — STT/TTS, Hermes, currency-bot, intercom, Nexus, наблюдаемость
  и Grafana.
- **VPN CT** — ocserv для входа в домашнюю LAN снаружи (пароль + TOTP).
- **BB CT** — сервер bb и SQLite; входящих портов не публикует, ходит
  исходящим HTTPS к bb Connect.
- **Cloudflare Tunnel** — единственный публичный HTTP-путь: webhook
  currency-bot и read-only `/repository/*` Nexus для CI.
- **Станция** — Pulsar/ESPHome-спутник и Sendspin-плеер.
- **Mac** — рабочая машина; адрес может меняться по DHCP.

## 2. Адреса и роли

| Узел | Пример адреса | Роль |
|---|---|---|
| Gateway/DNS | `192.0.2.1` | роутер, DHCP/DNS |
| Proxmox VE | `192.0.2.26` | `pve`, management `vmbr0` |
| HAOS VM 100 | `192.0.2.28` | HA Core/Supervisor |
| K3s VM 101 | `192.0.2.27` | Ubuntu, K3s node |
| VPN CT 102 | `192.0.2.29` | Debian 13, ocserv |
| BB CT 103 | `192.0.2.30` | Debian 13, bb server |
| Станция | `192.0.2.13` | Pulsar |
| Mac | `192.0.2.11` | DHCP |

`22222/tcp` на HAOS — host SSH (dropbear), не add-on. Ключ ha-config-sync
принимается только через `restrict,command="HA_CONFIG_AGENT_ALLOWED_FROM=<K3s node> /mnt/data/ha-config-sync/agent.sh"`;
dropbear не поддерживает ограничение по адресу в authorized_keys, поэтому
источник проверяет агент.

HAOS и K3s используют статические адреса внутри guest. Сеть плоская;
отдельных VLAN для гостей/IoT нет. Proxmox host использует Wi-Fi 4-address/WDS uplink с сохранением L2.
Сервис `home-uplink` выбирает Ethernet при наличии carrier, иначе Wi-Fi;
оба physical uplink одновременно к мосту не подключаются. Адреса VM не
меняются, NAT нет. Адреса Proxmox, VM и станции дополнительно зарезервированы на роутере. Профиль Wi-Fi хранится только приватно.

## 3. Порты

Порты сгруппированы по смыслу. Конкретные firewall-правила и source-адреса
не публикуются; в реальной карте они есть.

### Proxmox host

| Порт | Назначение | Класс доступа |
|---|---|---|
| `22/tcp` | SSH | LAN |
| `8006/tcp` | PVE web UI, HTTPS | LAN |
| `3128/tcp` | SPICE proxy | LAN |
| `111/tcp,udp` | rpcbind | LAN, не management-путь |

### HAOS

| Порт | Назначение | Класс доступа |
|---|---|---|
| `8123/tcp` | Home Assistant | LAN |
| `8095/tcp` | Music Assistant UI/API | LAN |
| `8097/tcp` | MA stream | LAN |
| `8927/tcp` | MA Sendspin | LAN |
| `5580/tcp` | Matter Server | только loopback внутри HAOS |
| `22222/tcp` | SSH хоста HAOS (dropbear) для ha-config-sync | LAN, только K3s-узел, ключ с forced command |

### K3s VM

| Порт | Назначение | Класс доступа |
|---|---|---|
| `22/tcp` | SSH | LAN |
| `80/tcp` | Traefik, редирект на HTTPS | LAN/VPN, IP allow-list |
| `443/tcp` | Traefik, TLS для `*.home.example.com` | LAN/VPN, IP allow-list |
| `30300/tcp` | Grafana NodePort | LAN, login required |
| `30100/tcp` | currency-bot API NodePort | LAN, Bearer, nft LAN/pod allow-list |
| `30097/tcp` | intercom signaling NodePort (аварийный путь) | LAN/VPN, Bearer, nft LAN/pod allow-list |
| `10200/tcp` | Piper TTS | LAN/HA |
| `10300/tcp` | Whisper fallback | LAN/HA |
| `10305/tcp` | GigaAM STT | LAN/HA |
| `8643/tcp` | Hermes voice proxy | только HAOS/K3s/loopback |
| `8642/tcp` | Hermes API | loopback |
| `6443/tcp` | K3s API | localhost/pod/node; не обычный LAN |
| `10250/tcp` | kubelet | localhost/pod/node; не обычный LAN |
| `9100/tcp` | node-exporter | не обычный LAN |
| `9116/tcp` | runtime health exporter | не обычный LAN |
| `9118/tcp` | Hermes config-drift exporter | не обычный LAN (SSH-туннель) |
| `8472/udp` | Flannel VXLAN | cluster-only |

K3s management/metrics-порты закрыты firewall от обычных LAN-клиентов.
Администрирование — через SSH и локальный туннель к API, а не через
публикацию `6443` в LAN.

### Currency bot

`currency-bot` webhook Service `:8081` доступен только подам
`cloudflared`; API и `/metrics` `:8080` — из кластера, узла, LAN CIDR через
Traefik и Prometheus. Для голосового intent «курс валют» тот же API Service
дополнительно публикуется как NodePort (пример `30100`, реальное значение —
`CURRENCY_BOT_NODE_PORT` в `secrets/home/currency-bot.env`) только в LAN:
host-nftables фильтр `table inet home_currency_bot` разрешает источник
`LAN_CIDR` и pod CIDR и дропает IPv6/остальные источники до kube-proxy
DNAT. Запросы всё равно требуют Bearer. Наружу API выходит только как
`https://currency.home.example.com/api/*` (LAN/VPN, Bearer); публичный
webhook — `tg.example.com/telegram/webhook` через Cloudflare Tunnel.

### Nexus

Nexus (`home/kubernetes/nexus`) слушает `8081` только внутри кластера. UI и
`/service/rest/*` доступны как `https://nexus.home.example.com/` через Traefik
с `home-allowlist` (LAN/VPN). Для CI существующий туннель `home-k3s`
публикует только `ci-nexus.<public-zone>/repository/*` с read-only учёткой
`ci-reader`; остальные пути на публичном имени отдают 404. Отдельный входящий
порт на роутере не открывается.

### VPN CT 102

| Порт | Назначение | Класс доступа |
|---|---|---|
| `443/tcp` | ocserv, TLS/OpenConnect | LAN + проброс ONT/AX3 |
| `443/udp` | ocserv, DTLS | LAN + проброс ONT/AX3 |
| `22/tcp` | SSH | только LAN `192.0.2.0/24` |

Внутри CT — пул `10.77.0.0/24`, nftables `drop` по умолчанию, masquerade
VPN → LAN/WAN. Профили `lan`/`full`; отдельная группа `ci` получает пул
`10.77.1.0/24` и маршрут только до K3s. На пользователя — не больше 10
одновременных сессий. Наружу проброшен только `443/tcp+udp`; сертификат
Let's Encrypt для `vpn.example.com` продлевается certbot-ом (DNS-01
Cloudflare) автоматически.

### BB CT 103

| Порт | Назначение | Класс доступа |
|---|---|---|
| публичные входящие | не публикуются | — |
| `22/tcp` | SSH; forced command `bb-dispatch` для моста | LAN; из кластера — только mcp-gateway с ограниченным ключом |
| `443/tcp` + DNS | исходящий bb Connect и enrollment | egress из CT |

Сервер bb слушает loopback внутри CT; удалённый доступ идёт через
исходящий канал `bb connect`, входящие порты на роутере не открываются.
SSH в CT — только LAN, как у остальных гостей. SQLite-копия лежит в
`/var/backups/bb`, ночной vzdump включает CT 103.

Голосовой путь не открывает в CT HTTP-порт: `mcp-gateway` в K3s ходит
`ssh` на `CT 103:22` отдельным ed25519-ключом с pinned `known_hosts` и
forced command `/usr/local/bin/bb-dispatch`; мост уже на CT обращается к
loopback-маршруту плагина `home-dispatcher`. NetworkPolicy шлюза разрешает
egress ровно на `${BB_CT_IP}:22`, публикации API bb наружу нет. Токен
плагина и HMAC-секрет остаются на CT.

### Станция

| Порт | Назначение | Класс доступа |
|---|---|---|
| `6053/tcp` | ESPHome/Pulsar API | LAN |
| `8928/tcp` | Sendspin | LAN |
| `8099/tcp` | HTTP control | LAN, bearer auth |
| `9101/tcp` | Prometheus-метрики Pulsar | LAN; только с адреса Prometheus |
| `5555/tcp` | ADB | LAN |
| `22/tcp` | SSH | LAN |
| `6638/tcp` | Zigbee coordinator | закрыт/не используется; без auth, не публиковать |

Станция использует egress guard: публичный WAN закрыт, разрешены LAN,
loopback, DHCP, multicast и NTP. Это часть голосового контура и не должно
меняться без отдельной проверки Pulsar/HA.

## 4. Grafana и HA

- HA доступен по LAN: `http://192.0.2.28:8123`
- Grafana доступна по LAN: `http://192.0.2.27:30300/` (корень)
- HTTPS (LAN/VPN): `https://ha.home.example.com/`,
  `https://grafana.home.example.com/`,
  `https://alarms.home.example.com/` (scheduler),
  `https://control.home.example.com/` (ha-control),
  `https://currency.home.example.com/` (currency-bot API, Bearer),
  `https://intercom.home.example.com/` (intercom signaling, Bearer;
  аварийный путь — NodePort `30097`),
  `https://vault.home.example.com/` (OpenBao UI/API),
  `https://passwords.home.example.com/` (Vaultwarden, клиенты Bitwarden),
  `https://nexus.home.example.com/` (Nexus UI/admin, LAN/VPN)
- Публично (Cloudflare Tunnel): `https://tg.example.com/telegram/webhook`
  (currency-bot) и `https://ci-nexus.<public-zone>/repository/*` (Nexus,
  read-only, `ci-reader`); остальные пути — 404.
- Grafana требует login; анонимный доступ выключен.
- Matter не имеет отдельного LAN UI: используется HA add-on ingress.

## 5. Удалённый доступ

Удалённый доступ — собственный VPN на ocserv (CT 102). На роутере проброшен
только VPN (`443/tcp+udp`); остальные сервисы в интернет не публикуются.

VPN доступен из LAN по `https://192.0.2.29/` и снаружи по
`https://vpn.example.com/`. Клиент — Cisco Secure Client / OpenConnect,
аутентификация — пароль + TOTP (6 цифр, 30 с, Google Authenticator) или
клиентский сертификат.

- `443/tcp` и `443/udp` (DTLS), TLS 1.2+, без compression;
- профили `lan` (split tunnel, маршрут только на домашнюю LAN, DNS
  `192.0.2.1`) и `full` (весь трафик через дом); отдельная группа `ci` — только
  для GitHub Actions и только до K3s;
- при входе по сертификату клиент не показывает выбор группы: по умолчанию
  `lan`, а `full` выбирается адресом сервера с суффиксом группы
  (`vpn.example.com/full`) — на телефоне это отдельное подключение
  «Home full» с тем же сертификатом. Проверено с мобильной сети 2026-11-01:
  внешний IP — домашний, сайты и HTTPS-имена дома открываются;
- NAT/masquerade делает CT, маршруты на LAN-устройствах не нужны;
- сертификат Let's Encrypt на `vpn.example.com`, продление certbot-ом
  (DNS-01 Cloudflare);
- встроенный бан неудачных попыток, SSH в CT — только из LAN.

### HTTPS-имена в LAN/VPN

В K3s работают Traefik (`hostNetwork`, порты `80/443` на узле) и
cert-manager с ACME DNS-01 Cloudflare. Wildcard-сертификат
`*.home.example.com` выпускается автоматически и продлевается cert-manager.

- A-записи `ha`, `grafana`, `alarms`, `control`, `currency`, `intercom`,
  `passwords`, `vault`, `nexus` → `192.0.2.27`, Cloudflare DNS only (не
  proxied), TTL auto; apex и прочие записи не меняются. Записи создаются
  вручную `projects/home/kubernetes/cloudflare_dns.py` (Flux их не трогает);
  `nexus` — Flux/README Nexus.
- `80` редиректит на `443`; вход разрешён только LAN `192.0.2.0/24` (VPN
  приходит NAT-ом с адреса CT), pod CIDR и loopback.
- HA видит источником адрес узла `192.0.2.27`; `trusted_proxies` не
  расширяется.
- WebSocket (`/api/websocket`, Grafana `/api/live/ws`) проходит через прокси.
- Роутер по-прежнему не пробрасывает `80/443`: имена резолвятся в приватный
  адрес и работают только из LAN/VPN. Публичный HTTP идёт только исходящим
  Cloudflare Tunnel (webhook currency-bot и read-only Nexus `/repository/*`).
- `intercom` — долгоживущий server-streaming HTTP/1.1 (`Watch`): Traefik
  маршрут задаёт `flushInterval: 100ms`, а entrypoint `websecure` —
  `writeTimeout: 0s` / `idleTimeout: 1800s`, поэтому кадры не буферизуются
  и стрим не обрывается по таймауту. NetworkPolicy `intercom-ingress`
  допускает LAN/VPN (включая узел с Traefik в `hostNetwork`) и pod CIDR.
- Группа VPN `ci` (CI) маршрутизируется только до `192.0.2.27`; nft
  `forward` разрешает лишь `tcp/443`: Traefik → HA (`deploy-station`) и
  OpenBao API/UI для GitHub OIDC (`deploy-station`, `android-distribute`).
  Остальные адреса и порты для пула `10.77.1.0/24` закрыты; правило
  `6443` снято вместе с CI-деплоем.

### Публичные маршруты Cloudflare Tunnel

Remotely-managed Cloudflare Tunnel `home-k3s` работает как Deployment
`cloudflared` в namespace `currency-bot` и ходит только наружу
(`7844/udp` QUIC + `443/tcp` fallback и DNS). На стороне Cloudflare два
маршрута:

- `tg.example.com` путь `^/telegram/webhook$` →
  `http://currency-bot-webhook.currency-bot.svc.cluster.local:8081`; webhook
  принимает только запросы с Telegram `secret_token` (иначе 401) и только из
  чатов allowlist.
- `ci-nexus.<public-zone>` путь `^/repository/` →
  `http://nexus.nexus.svc.cluster.local:8081`, read-only учётка `ci-reader`.

Остальные пути на этих именах отдают 404; входящих портов на роутере это не
открывает.

Управление и проверки — `projects/home/kubernetes/ingress/README.md` и
`KUBERNETES-OPERATIONS.md`. Прямые LAN-адреса и NodePort остаются аварийным
путём.

## 6. Политика доступа

- Административные интерфейсы — только LAN или VPN.
- `6443/10250/9100/9116/9118/8472` не публикуются в обычный LAN.
- `8643` voice proxy доступен только HAOS, K3s и loopback.
- `8123/8095/8099/6053/6638` не пробрасываются в интернет; `9101` — только
  LAN и только для Prometheus.
- Currency API NodePort (пример `30100`) — только LAN, Bearer и nft
  allow-list; в интернет не пробрасывается.
- `80/443` на узле K3s обслуживает только Traefik с IP allow-list; роутер их
  не пробрасывает, имена `*.home.example.com` работают только из LAN/VPN.
- VPN `443/tcp+udp` — единственный входящий порт из интернета; SSH в
  VPN-CT остаётся LAN-only. HTTP наружу идёт только исходящим Cloudflare
  Tunnel: `tg.example.com/telegram/webhook` и read-only
  `ci-nexus.<public-zone>/repository/*`.
- `6638` (Zigbee) не имеет auth; при включении не выставлять наружу.
- Сервер bb (CT 103) не публикует входящих портов: доступ идёт через
  исходящий Connect, SSH — только LAN, а из кластера — только `mcp-gateway`
  по SSH с ограниченным ключом и forced command.
- Станция не должна получать публичный WAN: egress guard — часть дизайна.
- Секреты и реальные адреса хранятся только в `secrets/`; в публичных
  документах — примеры и имена переменных.

## 7. Проверки

Проверки выполняются с подстановкой реальных адресов из приватной карты:

```bash
curl -fsS -o /dev/null -w 'HA %{http_code}\n' \
  http://<HAOS_IP>:8123/

curl -fsS -o /dev/null -w 'Grafana %{http_code}\n' \
  http://<K3S_IP>:30300/api/health

curl -fsS -o /dev/null -w 'MA %{http_code}\n' \
  http://<HAOS_IP>:8095/

curl -fsS -o /dev/null -w 'HA https %{http_code}\n' \
  https://ha.home.example.com/

curl -fsS -o /dev/null -w 'OpenBao %{http_code}\n' \
  https://vault.home.example.com/v1/sys/health
curl -fsS https://grafana.home.example.com/api/health

# webhook currency-bot: без secret token ожидается 401
curl -sS -o /dev/null -w 'webhook %{http_code}\n' \
  -X POST https://tg.example.com/telegram/webhook
```

K3s API не публикуется в LAN; для административного доступа используется
SSH-туннель:

```bash
ssh -N -L 127.0.0.1:16443:127.0.0.1:6443 <user>@<K3S_IP>
```

## 8. Связанные документы

- `STATE.md` — текущее состояние;
- `ARCHITECTURE.md` — устройство стенда и потоки;
- `KUBERNETES-OPERATIONS.md` — K3s, Helm и firewall;
- `INSTALL-PROXMOX.md` — VM, Terraform и Wi-Fi uplink;
- `SECURITY.md` — сетевые политики;
- `SENSITIVE-DATA.md` — реестр приватных данных (без значений).

Реальная карта с LAN-IP, именами VPN/HTTPS и точными firewall source —
`secrets/NETWORK.md`; она не входит в публичный Git.
