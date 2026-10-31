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
   └── роутер / gateway / DNS       192.0.2.1
          │
          └── LAN 192.0.2.0/24, плоский L2, без VLAN
                 │
                 ├── Proxmox VE host               192.0.2.26
                 │      vmbr0, Wi-Fi 4addr / Ethernet
                 │      ├── VM 100  HAOS          192.0.2.28
                 │      │      HA + MA + Matter add-ons
                 │      ├── VM 101  Ubuntu/K3s    192.0.2.27
                 │      │      STT/TTS, Hermes, Grafana, observability
                 │      └── CT 102  vpn           192.0.2.29
                 │             ocserv, OpenConnect, TOTP
                 │
                 ├── Станция Midi                 192.0.2.13
                 │      Pulsar, ESPHome, Sendspin
                 │
                 └── Mac (DHCP, не фиксирован)    192.0.2.11
```

Компоненты:

- **Proxmox host** — гипервизор и management-точка; не публикует HA.
- **HAOS VM** — Home Assistant, Music Assistant и Matter Server add-ons.
- **K3s VM** — STT/TTS, Hermes, наблюдаемость и Grafana.
- **VPN CT** — ocserv для входа в домашнюю LAN снаружи (пароль + TOTP).
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
| Станция | `192.0.2.13` | Pulsar |
| Mac | `192.0.2.11` | DHCP |

HAOS и K3s используют статические адреса внутри guest. Сеть плоская;
отдельных VLAN для гостей/IoT нет. Proxmox host использует Wi-Fi 4-address/WDS uplink с сохранением L2.
Сервис `ricardo-uplink` выбирает Ethernet при наличии carrier, иначе Wi-Fi;
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

### K3s VM

| Порт | Назначение | Класс доступа |
|---|---|---|
| `22/tcp` | SSH | LAN |
| `80/tcp` | Traefik, редирект на HTTPS | LAN/VPN, IP allow-list |
| `443/tcp` | Traefik, TLS для `*.home.example.com` | LAN/VPN, IP allow-list |
| `30300/tcp` | Grafana NodePort | LAN, login required |
| `10200/tcp` | Piper TTS | LAN/HA |
| `10300/tcp` | Whisper fallback | LAN/HA |
| `10305/tcp` | GigaAM STT | LAN/HA |
| `8643/tcp` | Hermes voice proxy | только HAOS/K3s/loopback |
| `8642/tcp` | Hermes API | loopback |
| `6443/tcp` | K3s API | localhost/pod/node; не обычный LAN |
| `10250/tcp` | kubelet | localhost/pod/node; не обычный LAN |
| `9100/tcp` | node-exporter | не обычный LAN |
| `9116/tcp` | runtime health exporter | не обычный LAN |
| `8472/udp` | Flannel VXLAN | cluster-only |

K3s management/metrics-порты закрыты firewall от обычных LAN-клиентов.
Администрирование — через SSH и локальный туннель к API, а не через
публикацию `6443` в LAN.

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

### Станция

| Порт | Назначение | Класс доступа |
|---|---|---|
| `6053/tcp` | ESPHome/Pulsar API | LAN |
| `8928/tcp` | Sendspin | LAN |
| `8099/tcp` | HTTP control | LAN, bearer auth |
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
  `https://vault.home.example.com/` (OpenBao UI/API)
- Grafana требует login; анонимный доступ выключен.
- Matter не имеет отдельного LAN UI: используется HA add-on ingress.
- Старый compatibility endpoint `http://192.0.2.27:8123` не используется;
  HA открывается напрямую через HAOS.

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
- NAT/masquerade делает CT, маршруты на LAN-устройствах не нужны;
- сертификат Let's Encrypt на `vpn.example.com`, продление certbot-ом
  (DNS-01 Cloudflare);
- встроенный бан неудачных попыток, SSH в CT — только из LAN.

### HTTPS-имена в LAN/VPN

В K3s работают Traefik (`hostNetwork`, порты `80/443` на узле) и
cert-manager с ACME DNS-01 Cloudflare. Wildcard-сертификат
`*.home.example.com` выпускается автоматически и продлевается cert-manager.

- A-записи `ha`, `grafana`, `alarms`, `control`, `vault` → `192.0.2.27`,
  Cloudflare DNS only (не proxied), TTL auto; apex и прочие записи не
  меняются.
- `80` редиректит на `443`; вход разрешён только LAN `192.0.2.0/24` (VPN
  приходит NAT-ом с адреса CT), pod CIDR и loopback.
- HA видит источником адрес узла `192.0.2.27`; `trusted_proxies` не
  расширяется.
- WebSocket (`/api/websocket`, Grafana `/api/live/ws`) проходит через прокси.
- Публикации в интернет нет: роутер по-прежнему не пробрасывает `80/443`;
  имена резолвятся в приватный адрес и работают только из LAN/VPN.
- Группа VPN `ci` (CI) маршрутизируется только до `192.0.2.27`; nft
  `forward` разрешает `tcp/{443,6443}`: `443` — Traefik (OpenBao API/UI для
  GitHub OIDC), `6443` — K3s API. Остальные адреса и порты для пула
  `10.77.1.0/24` закрыты.

Управление и проверки — `projects/home/kubernetes/ingress/README.md` и
`KUBERNETES-OPERATIONS.md`. Прямые LAN-адреса и NodePort остаются аварийным
путём.

## 6. Политика доступа

- Административные интерфейсы — только LAN или VPN.
- `6443/10250/9100/9116/8472` не публикуются в обычный LAN.
- `8643` voice proxy доступен только HAOS, K3s и loopback.
- `8123/8095/8099/6053/6638` не пробрасываются в интернет.
- `80/443` на узле K3s обслуживает только Traefik с IP allow-list; роутер их
  не пробрасывает, имена `*.home.example.com` работают только из LAN/VPN.
- VPN `443/tcp+udp` — единственный входящий порт из интернета; SSH в
  VPN-CT остаётся LAN-only.
- `6638` (Zigbee) не имеет auth; при включении не выставлять наружу.
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
