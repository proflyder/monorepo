# Home VPN (ocserv)

Собственный VPN-сервер для доступа к домашней LAN снаружи: OpenConnect /
Cisco Secure Client. Вход — пароль + TOTP (Google Authenticator) либо
клиентский сертификат (без пароля и кода). Работает в unprivileged Debian 13
LXC на Proxmox, слушает `443/tcp` и `443/udp` (DTLS).
Снаружи проброшен только `443/tcp+udp`; серверный сертификат — публичный
Let's Encrypt, который продлевается автоматически через DNS-01 Cloudflare.
Самоподписанная пара остаётся запасным вариантом (`selfsigned-cert.pem` /
`selfsigned-key.pem`) и не используется в штатном входе.

Реальные адреса и порты — `secrets/NETWORK.md`; здесь только примеры
`192.0.2.x`. Состояние — `docs/STATE.md`, команды — `docs/OPERATIONS.md`.

## Что внутри

| Файл | Назначение |
|---|---|
| `create-lxc.sh` | создаёт unprivileged CT (Debian 13, 1 vCPU / 512 MiB / 4 GiB), статический IP, onboot, `/dev/net/tun` |
| `install-ocserv.sh` | ставит и настраивает ocserv, сертификат, nftables, forwarding; при наличии CA включает вход по клиентскому сертификату |
| `setup-certbot.sh` | ставит certbot + dns-cloudflare, выпускает/переводит сертификат, ставит deploy hook |
| `vpnusers.py` | добавляет пользователя: пароль + TOTP, QR для Google Authenticator |
| `test_vpnusers.py` | unit-тесты чистой логики |
| `vpncerts.py` | частный CA и клиентские сертификаты: `init-ca`, `issue`, `revoke`, `list` |
| `test_vpncerts.py` | unit/локальные интеграционные тесты CA (без сети) |

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
`full` (весь трафик через дом). Отдельная группа `ci` (пул `10.77.1.0/24`)
предназначена только для GitHub Actions и описана ниже. NAT делает сам CT,
поэтому маршруты на LAN-устройствах не нужны.

Безопасность ocserv: TLS 1.2+, без compression, `isolate-workers = true`,
встроенный бан по очкам (`max-ban-score`), `max-same-clients = 10` на
пользователя, `rate-limit-ms`, TCP и UDP на одном порту. Вход в CT по SSH
разрешён только из LAN; остальное в INPUT — `drop`.

Сертификат — публичный Let's Encrypt, продлевается автоматически
(`certbot.timer`, DNS-01 Cloudflare, deploy hook перезапускает ocserv).
Самоподписанная пара сохранена как `selfsigned-cert.pem` /
`selfsigned-key.pem` только для аварийного отката.

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

# 4. Настроить автоматическое продление Let's Encrypt (см. ниже)
ssh root@192.0.2.26 \
  'pct push 102 /root/vpn-setup/vpn/setup-certbot.sh /root/setup-certbot.sh --perms 755'
ssh root@192.0.2.26 \
  'pct exec 102 -- /root/setup-certbot.sh \
     --domain vpn.example.com --credentials /etc/letsencrypt/cloudflare.ini'
```

`create-lxc.sh` сам проверяет, что CTID свободен, а IP не отвечает на
ping/ARP, и скачивает шаблон Debian 13, если его нет. Он не меняет `vmbr0`,
uplink и firewall хоста.

`install-ocserv.sh` идемпотентен: существующие `ocpasswd`, `users.oath` и
сертификат сохраняются. Если передан `--client-ca` (или файл CA уже лежит в
`/etc/ocserv/ssl/client-ca.pem`), включается вход по клиентскому
сертификату; дополнительно нужен `--client-crl`. Без CA остаётся только
пароль + TOTP. Изменение методов аутентификации требует restart ocserv;
остальные правки применяются reload без разрыва активных сессий.

## Сертификат Let's Encrypt

DNS зоны делегирован Cloudflare, поэтому DNS-01 решается плагином
`certbot-dns-cloudflare`. Токен нужен только с правом `Zone:DNS:Edit` на эту
зону. В репозиторий он не попадает: файл готовится внутри CT.

```bash
# внутри CT: 600, root
printf 'dns_cloudflare_api_token = %s\n' '<token>' \
  > /etc/letsencrypt/cloudflare.ini
chmod 600 /etc/letsencrypt/cloudflare.ini
```

Токен передаётся в CT только файлом (не в argv): создать временный файл с
правами `600`, `pct push` в CT, затем удалить временные копии на PVE и Mac.

`setup-certbot.sh` идемпотентен:

- ставит `certbot` и `python3-certbot-dns-cloudflare`;
- если lineage уже есть, переводит renewal-конфиг на `dns-cloudflare`
  (сохраняет backup), убирает `manual`-хук и `renew_hook`;
- если сертификата ещё нет — выпускает новый;
- ставит deploy hook `/etc/letsencrypt/renewal-hooks/deploy/ocserv-ssl.sh`,
  который проверяет пару ключ/сертификат, копирует `fullchain`/`privkey` в
  `/etc/ocserv/ssl/server-{cert,key}.pem` и перезапускает ocserv;
- включает `certbot.timer` и проверяет `certbot renew --dry-run`.

`dns_cloudflare_propagation_seconds` — 30 секунд. Продление выполняет
systemd-таймер (`certbot.timer`), ручной запуск не нужен. Deploy hook
срабатывает только при реальном обновлении сертификата, поэтому ocserv не
перезапускается без необходимости.

Проверка:

```bash
pct exec 102 -- certbot certificates
pct exec 102 -- certbot renew --dry-run --no-random-sleep-on-renew
pct exec 102 -- systemctl list-timers certbot.timer
pct exec 102 -- ls -l /etc/letsencrypt/renewal-hooks/deploy/
```

## Пользователь

```bash
python3 projects/home/proxmox/vpn/vpnusers.py <user> \
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

## Клиентские сертификаты (вход без пароля)

Дополнительный способ входа: частный CA выпускает клиентские сертификаты.
Клиент с валидным сертификатом входит без пароля и TOTP; без сертификата
продолжает работать пароль + TOTP. Серверный сертификат Let's Encrypt и
certbot не затрагиваются.

- CA: RSA 4096, 10 лет. Приватный ключ CA хранится только на рабочей машине
  в `secrets/home/vpn/client-ca/` (каталог `700`, ключ `600`) и никогда не
  копируется на сервер.
- На CT попадают только `client-ca.pem` и `client-ca.crl` в
  `/etc/ocserv/ssl/`.
- Клиентский сертификат: RSA 2048, 2 года, EKU clientAuth, CN = имя
  пользователя, OU = группы (`lan`, `full`), уникальный serial.
- ocserv: `auth = "certificate"` (основной), `enable-auth = "plain[...]"` —
  альтернатива для входа паролем + TOTP, `cert-user-oid = 2.5.4.3`,
  `cert-group-oid = 2.5.4.11`. В ocserv 1.3 сертификат должен быть основным
  методом: как `enable-auth` он не разрешается security-модулем.

### CA и выпуск

```bash
# один раз: создать CA и положить на CT client-ca.pem + CRL
python3 projects/home/proxmox/vpn/vpncerts.py init-ca \
  --pve 192.0.2.26 \
  --ssh-key secrets/proxmox-install/proxmox_admin \
  --known-hosts secrets/proxmox-install/known_hosts

# выпустить сертификат; --device задаёт только имя файлов
python3 projects/home/proxmox/vpn/vpncerts.py issue <user> --device phone
# => secrets/home/vpn/clients/<user>-phone.p12
#    <user>-phone.legacy.p12  (запасной вариант для старого импорта)
#    <user>-phone.password    (пароль импорта, 600)

# отозвать по serial или по файлу .p12 и обновить CRL на CT
python3 projects/home/proxmox/vpn/vpncerts.py revoke <serial|file.p12> \
  --pve 192.0.2.26 --ssh-key ... --known-hosts ...

# список выданных сертификатов
python3 projects/home/proxmox/vpn/vpncerts.py list
```

`issue` и `revoke` не печатают пароли и ключи. `revoke` обновляет CRL,
кладёт его в CT, проверяет `ocserv --test-config` и перезагружает ocserv.
`issue --groups lan` ограничивает сертификат одной группой.

### Android: Cisco Secure Client

1. Скопировать `<user>-phone.p12` на телефон.
2. Cisco Secure Client → Diagnostics → Certificate Management → Import,
   выбрать файл, ввести пароль из `<user>-phone.password`.
3. В профиле подключения: сервер `vpn.example.com`, сертификат —
   импортированный; поле Group — `lan` (по умолчанию) или `full`.
4. Подключение: сертификат выбирается автоматически, пароль и код не нужны.

Если импорт современного `.p12` (AES-256/SHA-256) не принимается, повторить
с `<user>-phone.legacy.p12` и тем же паролем. Группу нужно указать один раз
в профиле: ocserv берёт профиль из сертификата, а без группы в запросе
просит выбрать её при подключении.

### Проверка без пароля

```bash
openconnect --authenticate \
  --certificate <user>-phone.p12 --passwd-on-stdin \
  --authgroup=lan \
  --resolve vpn.example.com:192.0.2.29 \
  https://vpn.example.com/ < <user>-phone.password
```

`--authenticate` печатает cookie — вывод не публиковать. Успех — код 0 и
строка `COOKIE`. Серверный сертификат — публичный Let's Encrypt, поэтому
`--servercert` не нужен. Пароль из stdin нужен только для расшифровки
`.p12`; если сервер попросит пароль VPN, значит сертификат не принят.

Отзыв: `revoke` добавляет serial в CRL, и следующий вход с этим
сертификатом отклоняется (клиенту остаётся только пароль + TOTP). Файлы
отозванного сертификата можно удалить из `secrets/home/vpn/clients/`.

## Группа `ci` (доступ GitHub Actions)

Для автоматического деплоя на K3s заведена отдельная группа сертификата `ci`.
Она не пересекается с `lan`/`full`:

- собственный пул `10.77.1.0/24` (`ipv4-network` в
  `config-per-group/ci`), общий пул `10.77.0.0/24` в него не входит;
- клиенту выдаётся единственный маршрут `<k3s-node>/32`, DNS не пушится;
- nftables `forward` разрешает из `10.77.1.0/24` только
  `<k3s-node> tcp/{443,6443}` (K3s API и HTTPS/Traefik для OpenBao; SSH из
  этой группы закрыт — правило стоит до общего `accept` для `10.77.0.0/24`,
  затем `drop`),
  `postrouting` маскирует пул в адрес CT;
- сертификат выпускается только с OU `ci`:
  `vpncerts.py issue ci --device github --groups ci`. В профиле клиента
  выбирать группу не нужно: ocserv берёт её из OU сертификата.

Установка/обновление (идемпотентно, активные сессии `lan`/`full` не рвутся —
меняются только файлы групп и nftables, ocserv делает reload):

```bash
pct push 102 /root/vpn-setup/vpn/install-ocserv.sh /root/install-ocserv.sh --perms 755
pct exec 102 -- /root/install-ocserv.sh \
  --lan-cidr 192.0.2.0/24 --dns 192.0.2.1 --server-ip 192.0.2.29 \
  --server-name vpn.example.com \
  --ci-cidr 10.77.1.0/24 --ci-host 192.0.2.27
```

Проверка без поднятия туннеля на рабочей машине:

```bash
openconnect --authenticate --certificate ci.p12 --passwd-on-stdin \
  --authgroup ci --resolve vpn.example.com:192.0.2.29 \
  https://vpn.example.com/ < ci.password
```

Отзыв доступа — обычный `vpncerts.py revoke <ci-github.p12>`; см.
`docs/OPERATIONS.md`.

## Проверки

На PVE:

```bash
pct status 102                 # running, onboot: 1
qm list                        # VM 100 и 101 не затронуты
pct exec 102 -- systemctl is-active ocserv
pct exec 102 -- ss -lntup | grep ':443'
pct exec 102 -- nft list ruleset
pct exec 102 -- nft list chain inet vpn forward   # MSS clamp первым
pct exec 102 -- systemctl restart ocserv
pct exec 102 -- nft list ruleset   # правила на месте
```

Правило MSS clamp в forward chain должно стоять до `accept`: SYN, принятый
раньше, до clamp уже не доходит, и клиент сохраняет слишком большой MTU.

Проверка только аутентификации, без поднятия туннеля на Mac (серверный
сертификат публичный, `--servercert` не нужен):

```bash
openconnect --authenticate --user=<user> --authgroup=lan \
  --resolve vpn.example.com:192.0.2.29 \
  https://vpn.example.com/
```

Пароль + актуальный TOTP — успех; неверный TOTP — отказ. `--authenticate`
печатает cookie сессии, поэтому вывод не публиковать. Повторять проверку с
тем же TOTP нельзя: ocserv отклоняет повторно использованный код, а несколько
неудач подряд временно банят IP.

Если сервер временно работает с самоподписанным сертификатом (аварийный
откат), клиент проверяет публичный ключ через `pin-sha256`:

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
