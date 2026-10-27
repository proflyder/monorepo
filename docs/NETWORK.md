# NETWORK — публичная карта стенда

**Назначение:** обезличенное описание домашней сети, портов и удалённого
доступа. Документ можно публиковать.

> **Важно:** все адреса ниже — примеры из документационного диапазона
> `192.0.2.0/24` (RFC 5737), а не реальные адреса стенда. Реальная карта
> с LAN-IP и Tailscale-именем хранится отдельно, в приватном
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
                 │      vmbr0, только Ethernet
                 │      ├── VM 100  HAOS          192.0.2.28
                 │      │      HA + MA + Matter add-ons
                 │      └── VM 101  Ubuntu/K3s    192.0.2.27
                 │             STT/TTS, Hermes, Grafana, observability
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
- **Станция** — Pulsar/ESPHome-спутник и Sendspin-плеер.
- **Mac** — рабочая машина; адрес может меняться по DHCP.

## 2. Адреса и роли

| Узел | Пример адреса | Роль |
|---|---|---|
| Gateway/DNS | `192.0.2.1` | роутер, DHCP/DNS |
| Proxmox VE | `192.0.2.26` | `pve`, management `vmbr0` |
| HAOS VM 100 | `192.0.2.28` | HA Core/Supervisor |
| K3s VM 101 | `192.0.2.27` | Ubuntu, K3s node |
| Станция | `192.0.2.13` | Pulsar |
| Mac | `192.0.2.11` | DHCP |

HAOS и K3s используют статические адреса внутри guest. Сеть плоская;
отдельных VLAN для гостей/IoT нет. Proxmox host подключён Ethernet;
Wi-Fi на хосте не является рабочим uplink.

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
| `41641/udp` | Tailscale | outbound/NAT traversal |

K3s management/metrics-порты закрыты firewall от обычных LAN-клиентов.
Администрирование — через SSH и локальный туннель к API, а не через
публикацию `6443` в LAN.

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
- Grafana доступна по LAN: `http://192.0.2.27:30300/grafana/`
- Grafana требует login; анонимный доступ выключен.
- Matter не имеет отдельного LAN UI: используется HA add-on ingress.
- Старый compatibility endpoint `http://192.0.2.27:8123` не используется;
  HA открывается напрямую через HAOS.

## 5. Удалённый доступ

Удалённый доступ построен на **Tailscale**, без входящих портов на роутере:

```text
https://pl-home.<tailnet>.ts.net/          -> HAOS
https://pl-home.<tailnet>.ts.net/grafana/  -> Grafana
```

Свойства схемы:

- Tailscale Operator работает в K3s и публикует Kubernetes Ingress;
- `pl-home` указывает на operator proxy с тегом `k8s`;
- старый host-level Serve не используется;
- Funnel и port-forward на ONT/AX3 не используются;
- публичного HA, MA, ADB или Zigbee-порта нет;
- LAN-доступ и локальный голосовой контур работают независимо от WAN.

## 6. Политика доступа

- Административные интерфейсы — только LAN или Tailscale.
- `6443/10250/9100/9116/8472` не публикуются в обычный LAN.
- `8643` voice proxy доступен только HAOS, K3s и loopback.
- `8123/8095/8099/6053/6638` не пробрасываются в интернет.
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
  http://<K3S_IP>:30300/grafana/api/health

curl -fsS -o /dev/null -w 'MA %{http_code}\n' \
  http://<HAOS_IP>:8095/
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
- `INSTALL-PROXMOX.md` — VM и восстановление;
- `REMOTE-ACCESS.md` — Tailscale и альтернативы;
- `SENSITIVE-DATA.md` — реестр приватных данных (без значений).

Реальная карта с LAN-IP, Tailscale-именем и точными firewall source —
`secrets/NETWORK.md`; она не входит в публичный Git.
