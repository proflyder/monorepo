# Home VPN (ocserv + WireGuard, CT 102)

VPN-сервер для доступа к домашней LAN снаружи в unprivileged Debian 13 LXC на
Proxmox. ocserv (OpenConnect / Cisco Secure Client) на `443/tcp+udp` (DTLS):
вход паролем + TOTP либо клиентским сертификатом. WireGuard (`wg0`,
`51820/udp`) — рядом, peer'ы из scheduler. Серверный сертификат — Let's Encrypt
через DNS-01 Cloudflare; самоподписанная пара `selfsigned-{cert,key}.pem`
остаётся только для аварийного отката.

Реальные адреса и порты — `secrets/NETWORK.md`; здесь примеры `192.0.2.x`.

| Файл | Назначение |
|---|---|
| `create-lxc.sh` | CT (1 vCPU / 512 MiB / 4 GiB), статический IP, onboot, `/dev/net/tun`; проверяет свободный CTID/IP, хост не меняет |
| `install-ocserv.sh` | ocserv, сертификат, nftables (единая точка правды для firewall CT, включая WireGuard и группу `ci`), forwarding |
| `setup-certbot.sh` | certbot + dns-cloudflare, deploy hook в ocserv |
| `vpnusers.py` | пользователь: пароль + TOTP, QR |
| `vpncerts.py` | частный CA и клиентские сертификаты: `init-ca`, `issue`, `revoke`, `list` |
| `install-wireguard.sh` | `wg0`, ключ сервера, токен синхронизации, `wg-peer-sync` + timer |
| `wg-peer-sync.sh` | `GET /v1/vpn/peers` из scheduler → валидация → `wg syncconf` |

## Сеть

| Пул | Кто | Маршруты |
|---|---|---|
| `10.77.0.0/24` | ocserv, группы `lan` (по умолчанию, split: LAN + DNS роутера) и `full` (default route) | NAT в адрес CT |
| `10.77.1.0/24` | ocserv, группа `ci` (только сертификат с OU `ci`) | один маршрут `<k3s-node>/32`, без DNS; `forward` разрешает только `<k3s-node> tcp/443`, правило до общего accept |
| `10.77.2.0/24` | WireGuard, сервер `10.77.2.1` | как `full` (или split — выбор приложения) |

ocserv: TLS 1.2+, без compression, `isolate-workers`, бан по очкам,
`max-same-clients = 10`, `rate-limit-ms`. SSH в CT только из LAN, остальной
INPUT — `drop`. MSS clamp в forward chain должен стоять до `accept`, иначе
клиент оставляет слишком большой MTU.

## Установка

С Mac; реальные значения — из `secrets/NETWORK.md`. Команды, которые меняют
nftables или перезапускают ocserv, запускать в CT через `systemd-run` и
смотреть `journalctl`: обрыв сессии через VPN иначе убьёт скрипт.

```bash
scp -r projects/home/proxmox/vpn root@192.0.2.26:/root/vpn-setup
ssh root@192.0.2.26 '/root/vpn-setup/vpn/create-lxc.sh \
  --ip 192.0.2.29 --gateway 192.0.2.1 --dns 192.0.2.1 --ssh-pubkey /root/.ssh/authorized_keys'
ssh root@192.0.2.26 'pct push 102 /root/vpn-setup/vpn/install-ocserv.sh /root/install-ocserv.sh --perms 755'
ssh root@192.0.2.26 'pct exec 102 -- systemd-run --unit=ocserv-install /root/install-ocserv.sh \
  --lan-cidr 192.0.2.0/24 --dns 192.0.2.1 --server-ip 192.0.2.29 \
  --server-name vpn.example.com --ci-cidr 10.77.1.0/24 --ci-host 192.0.2.27 \
  --wg-cidr 10.77.2.0/24 --wg-port 51820'
```

`install-ocserv.sh` идемпотентен: `ocpasswd`, `users.oath`, сертификат и CA
сохраняются. С `--client-ca` (+ `--client-crl`) или уже лежащим
`/etc/ocserv/ssl/client-ca.pem` включается вход по сертификату. Смена методов
аутентификации требует restart ocserv; остальное — reload без разрыва сессий.

## Let's Encrypt

Токен Cloudflare с `Zone:DNS:Edit` только на эту зону кладётся в CT файлом
`/etc/letsencrypt/cloudflare.ini` (`600`, через временный файл и `pct push`,
временные копии удалить), не в argv.

```bash
pct exec 102 -- /root/setup-certbot.sh \
  --domain vpn.example.com --credentials /etc/letsencrypt/cloudflare.ini
```

Скрипт идемпотентен: ставит пакеты, переводит существующий lineage на
`dns-cloudflare` или выпускает новый, ставит deploy hook
`/etc/letsencrypt/renewal-hooks/deploy/ocserv-ssl.sh` (проверка пары, копия в
`/etc/ocserv/ssl/server-{cert,key}.pem`, restart ocserv только при реальном
обновлении), включает `certbot.timer`, проверяет `renew --dry-run`.

## Пользователи (пароль + TOTP)

```bash
python3 projects/home/proxmox/vpn/vpnusers.py <user> \
  --pve 192.0.2.26 --ssh-key ~/.ssh/home-lab/proxmox_admin \
  --known-hosts ~/.ssh/home-lab/known_hosts_pve
```

Обновляет `/etc/ocserv/ocpasswd` и `users.oath` в CT, приватную копию и QR
кладёт в `secrets/home/vpn/` (`700`/`600`), значения не печатает. Группы
`lan,full`; ротация — `--force`. В клиенте сначала пароль, затем код; ocserv
отклоняет повторно использованный код, серия неудач временно банит IP.

## Клиентские сертификаты

CA RSA 4096 / 10 лет; ключ CA только на рабочей машине в
`secrets/home/vpn/client-ca/`, на CT — только `client-ca.pem` и `client-ca.crl`.
Клиентский сертификат: RSA 2048 / 2 года, clientAuth, CN = пользователь,
OU = группы. В ocserv 1.3 сертификат работает только как основной метод
(`auth = "certificate"`), пароль + TOTP — `enable-auth` для клиентов без
сертификата.

```bash
python3 projects/home/proxmox/vpn/vpncerts.py init-ca --pve 192.0.2.26 --ssh-key … --known-hosts …
python3 projects/home/proxmox/vpn/vpncerts.py issue <user> --device phone [--groups lan]
python3 projects/home/proxmox/vpn/vpncerts.py revoke <serial|file.p12> --pve 192.0.2.26 --ssh-key … --known-hosts …
python3 projects/home/proxmox/vpn/vpncerts.py list
```

`issue` пишет `secrets/home/vpn/clients/<user>-<device>.p12`, `.legacy.p12`
(для клиентов, не принимающих AES-256/SHA-256) и `.password`. `revoke`
обновляет CRL на CT, проверяет `ocserv --test-config` и делает reload.
Сертификаты CI: `issue ci --device pulsar --groups ci` и
`issue ci --device android --groups ci` (они же в OpenBao
`secret/infra/vpn/clients/*`, см. `docs/SECRETS.md`).

Cisco Secure Client: Diagnostics → Certificate Management → Import `.p12`;
в профиле указать группу (`lan`/`full`) один раз.

## WireGuard

Peer'ы руками не редактируются: источник правды — scheduler (`vpn_peers`).
Приложение присылает публичный ключ в `POST /v1/vpn/profile`, scheduler
выделяет адрес. `wg-peer-sync` каждые 30 с читает `GET /v1/vpn/peers`
отдельным read-only bearer'ом и применяет `wg syncconf` без разрыва сессий.
Невалидный ответ (ключ, не `/32`, дубли, HTTP-ошибка) отбрасывается, остаётся
последний валидный `/etc/wireguard/peers.json` — падение scheduler не
отключает телефоны. Отзыв — `POST /v1/vpn/profile/remove`, peer исчезает за
≤30 с; аварийно — остановить `wg-quick@wg0`.

Модуль `wireguard` должен быть загружен на PVE
(`/etc/modules-load.d/wireguard.conf`): из unprivileged CT он не подгружается,
`install-wireguard.sh` проверяет это пробным интерфейсом. Снаружи нужен
проброс `51820/udp` на CT.

Секреты — `secret/infra/vpn/wireguard#private_key` и
`secret/k8s/home-assistant/scheduler-auth#vpn_sync_token`, только временными
файлами `600`, удалить после установки:

```bash
pct push 102 /root/vpn-setup/vpn/wg-peer-sync.sh /root/wg-peer-sync.sh --perms 755
pct push 102 /root/vpn-setup/vpn/install-wireguard.sh /root/install-wireguard.sh --perms 755
pct push 102 <tmp>/wg.key /root/wg.key --perms 600
pct push 102 <tmp>/sync.token /root/sync.token --perms 600
pct exec 102 -- systemd-run --unit=wg-install /root/install-wireguard.sh --address 10.77.2.1/24 \
  --sync-url https://alarms.home.example.com/v1/vpn/peers \
  --private-key-file /root/wg.key --sync-token-file /root/sync.token
pct exec 102 -- rm -f /root/wg.key /root/sync.token
```

Повторный запуск без ключевых файлов сохраняет установленные ключ и токен.
Тест без телефона: клиент `wgt0` в отдельном network namespace на PVE с
адресом `10.77.2.250/32` и default route через туннель.

## Проверки

```bash
pct exec 102 -- systemctl is-active ocserv
pct exec 102 -- nft list chain inet vpn forward
pct exec 102 -- certbot renew --dry-run --no-random-sleep-on-renew
pct exec 102 -- wg show wg0
pct exec 102 -- journalctl -u wg-peer-sync -n 20
openconnect --authenticate --user=<user> --authgroup=lan \
  --resolve vpn.example.com:192.0.2.29 https://vpn.example.com/
python3 -m unittest discover -s projects/home/proxmox/vpn -p 'test_*.py'
```

`--authenticate` печатает cookie сессии — вывод не публиковать. Для
сертификата: `--certificate <file>.p12 --passwd-on-stdin < <file>.password`;
запрос пароля VPN значит, что сертификат не принят. При аварийном
самоподписанном сертификате клиент проверяет `pin-sha256` публичного ключа.

## Откат

`pct stop 102 && pct destroy 102`. Пользователи, сертификат и ключ WireGuard
живут внутри CT; вне хоста — только `secrets/home/vpn/` и OpenBao. При
пересоздании пользователей добавить заново; WireGuard-peer'ы вернутся из
scheduler.
