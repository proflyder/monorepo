# Home VPN (ocserv)

Собственный VPN-сервер для доступа к домашней LAN снаружи: OpenConnect /
Cisco Secure Client, пароль + TOTP (Google Authenticator). Работает в
unprivileged Debian 13 LXC на Proxmox, слушает `443/tcp` и `443/udp` (DTLS).
Этап 1 — только LAN и самоподписанный сертификат; пробросы на ONT/AX3 и
Let's Encrypt добавляются позже.

Реальные адреса и порты — `secrets/NETWORK.md`; здесь только примеры
`192.0.2.x`. Состояние — `docs/STATE.md`, команды — `docs/OPERATIONS.md`.

## Что внутри

| Файл | Назначение |
|---|---|
| `create-lxc.sh` | создаёт unprivileged CT (Debian 13, 1 vCPU / 512 MiB / 4 GiB), статический IP, onboot, `/dev/net/tun` |
| `install-ocserv.sh` | ставит и настраивает ocserv, сертификат, nftables, forwarding |
| `vpnusers.py` | добавляет пользователя: пароль + TOTP, QR для Google Authenticator |
| `test_vpnusers.py` | unit-тесты чистой логики |

Схема:

```text
клиент ── 443/tcp (TLS) ─┐
        └ 443/udp (DTLS) ┴─ ocserv в CT
                              ├─ профиль lan:  route 192.0.2.0/24, DNS 192.0.2.1
                              └─ профиль full: route default
                              nftables: masquerade VPN → LAN/WAN
```

Пул VPN — `10.77.0.0/24`. Профиль выбирается при входе в клиенте:
`lan` (по умолчанию, split tunnel — только домашняя LAN) или
`full` (весь трафик через дом). NAT делает сам CT, поэтому маршруты на
LAN-устройствах не нужны.

Безопасность ocserv: TLS 1.2+, без compression, `isolate-workers = true`,
встроенный бан по очкам (`max-ban-score`), лимит одинаковых клиентов,
`rate-limit-ms`, TCP и UDP на одном порту. Вход в CT по SSH разрешён только
из LAN; остальное в INPUT — `drop`. Сертификат на этапе 1 самоподписанный,
клиент проверяет `pin-sha256`.

## Установка

Выполнять с Mac, реальные значения брать из `secrets/NETWORK.md`.

```bash
# 1. Скопировать каталог на PVE
scp -r projects/home/proxmox/vpn \
  root@192.0.2.26:/root/vpn-setup

# 2. Создать CT (пример адресов; подставить реальные)
ssh root@192.0.2.26 \
  '/root/vpn-setup/vpn/create-lxc.sh \
     --ip 192.0.2.29 --gateway 192.0.2.1 --dns 192.0.2.1 \
     --ssh-pubkey /root/.ssh/authorized_keys'

# 3. Установить ocserv внутрь CT
ssh root@192.0.2.26 \
  'pct push 102 /root/vpn-setup/vpn/install-ocserv.sh /root/install-ocserv.sh --perms 755'
ssh root@192.0.2.26 \
  'pct exec 102 -- /root/install-ocserv.sh \
     --lan-cidr 192.0.2.0/24 --dns 192.0.2.1 --server-ip 192.0.2.29'
```

`create-lxc.sh` сам проверяет, что CTID свободен, а IP не отвечает на
ping/ARP, и скачивает шаблон Debian 13, если его нет. Он не меняет `vmbr0`,
uplink и firewall хоста.

`install-ocserv.sh` идемпотентен: существующие `ocpasswd`, `users.oath` и
сертификат сохраняются.

## Пользователь

```bash
python3 projects/home/proxmox/vpn/vpnusers.py stas \
  --pve 192.0.2.26 \
  --ssh-key secrets/proxmox-install/proxmox_admin \
  --known-hosts secrets/proxmox-install/known_hosts
```

Скрипт генерирует пароль и TOTP-секрет, обновляет
`/etc/ocserv/ocpasswd` и `/etc/ocserv/users.oath` в CT, кладёт приватную
копию и PNG QR в `secrets/home/vpn/` (каталог `700`, файлы `600`). Значения
секретов не печатаются. QR добавляется в Google Authenticator (issuer
`home VPN`, задаётся `--issuer`).

Повторная выдача для того же пользователя (ротация) — с `--force`.
Пользователь получает группы `lan,full`; клиент показывает выбор профиля.

Пароль и TOTP вводятся в клиенте последовательно: сначала пароль, затем
6-значный код. В `users.oath` хранится тот же ключ в hex, в QR — в base32.

## Проверки

На PVE:

```bash
pct status 102                 # running, onboot: 1
qm list                        # VM 100 и 101 не затронуты
pct exec 102 -- systemctl is-active ocserv
pct exec 102 -- ss -lntup | grep ':443'
pct exec 102 -- nft list ruleset
pct exec 102 -- systemctl restart ocserv
pct exec 102 -- nft list ruleset   # правила на месте
```

Проверка только аутентификации, без поднятия туннеля на Mac:

```bash
openconnect --authenticate --user=stas --authgroup=lan \
  --servercert pin-sha256:<PIN> \
  https://192.0.2.29/
```

Пароль + актуальный TOTP — успех; неверный TOTP — отказ. `--authenticate`
печатает cookie сессии, поэтому вывод не публиковать. Повторять проверку с
тем же TOTP нельзя: ocserv отклоняет повторно использованный код, а несколько
неудач подряд временно банят IP.

`pin-sha256` — SHA-256 публичного ключа сервера (то, что проверяет
`--servercert`), не хеш всего сертификата:

```bash
pct exec 102 -- sh -c \
  'openssl x509 -in /etc/ocserv/ssl/server-cert.pem -pubkey -noout \
   | openssl pkey -pubin -outform DER \
   | openssl dgst -sha256 -binary | openssl base64'
```

## Откат

```bash
# остановить и удалить только этот CT (пользователи и ключи внутри)
pct stop 102
pct destroy 102

# локальная копия секретов пользователя
rm -rf secrets/home/vpn
```

Сертификат и пользователи живут внутри CT; после destroy они удаляются.
Приватные копии в `secrets/home/vpn/` — единственный вне-хостовый архив
учётных данных. При пересоздании CT пользователей нужно добавить заново.
