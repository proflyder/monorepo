# REMOTE-ACCESS / STATIC-IP — WireGuard на `lab-host` и routing на мини-ПК

**Дата:** 2026-10-26  
**Триггер:** владелец сообщил, что внешний IP **статический**, и рассматривает
вариант с routing/входом через мини-ПК вместо Tailscale.  
**Статус:** **альтернатива; НЕ выбрана** (решение 2026-10-26 — Tailscale, см.
[`REMOTE-ACCESS-TAILSCALE-PLAN.md`](REMOTE-ACCESS-TAILSCALE-PLAN.md)).
Документ сохранён как план B для случая, если понадобится self-hosted
WireGuard без Tailscale. Владелец подтвердил статический IP `203.0.113.10` и
admin-доступ к ONT/AX3; это не отменяет Tailscale-план.  
**Связанный документ:** [`REMOTE-ACCESS.md`](REMOTE-ACCESS.md) — общий разбор
вариантов, HA hardening, threat model.

> **TL;DR.** Если внешний IP **действительно статический/редко меняется** и
> есть admin-доступ к ONT и AX3, Tailscale **не обязателен**. Рабочая схема:
> **WireGuard на `lab-host` + один порт UDP 51820** (два правила: ONT → AX3 →
> `lab-host`, либо bridge ONT и одно правило на AX3). HA остаётся внутри VPN,
> наружу не публикуется; MFA обязательна. Если нужен
> браузер/семья без VPN-клиента — добавляем **Caddy + домен + 443** (HA MFA,
> trusted proxy `127.0.0.1`). «Routing на мини-ПК» в смысле **замены AX3
> роутером** — отдельный проект: нужен второй NIC, ONT bridge, DHCP/DNS/NAT,
> AX3 в AP; высокий риск, не делать одновременно с remote access.
>
> **Красный флаг:** whois текущего `2.134.90.x` — `GPON_FTTH_SERVICES` /
> `Dynamic GPON-FTTH` (Kazakhtelecom). Владелец подтвердил, что IP
> статический; whois-имя пула не доказывает обратное (провайдер может выдать
> статику из общего пула). Остаётся проверить inbound и admin на ONT (см. §3).

---

## 0. Термины (чтобы не путаться)

| Термин | Что это | Адрес/пример |
|---|---|---|
| **ONT** | Оптический терминал (GPON-модем) от Kazakhtelecom; делает WAN, NAT, DHCP, VoIP/IPTV | `192.168.1.1` |
| **AX3** | Твой Wi-Fi-роутер Huawei WiFi AX3; делает LAN `192.0.2.0/24`, Wi-Fi, NAT | `192.0.2.1` |
| **Admin на ONT/AX3** | Логин/пароль в **веб-панель** устройства, чтобы менять WAN, port mapping, DMZ, bridge. Не Linux-пользователь | `http://192.168.1.1`, `http://192.0.2.1` |
| **ONT route mode** | ONT — роутер: держит публичный IP, делает NAT; AX3 подключён к нему и делает **второй** NAT | текущая схема |
| **ONT bridge mode** | ONT — «прозрачный» модем: публичный IP уходит на AX3; NAT только на AX3 | нужно переключить |
| **WAN IP** | Адрес на внешнем интерфейсе устройства | у ONT — `203.0.113.10`; у AX3 — `192.168.1.x` |
| **Port mapping / forward** | Правило «внешний порт → внутренний IP:порт» | ONT/AX3 |
| **DMZ** | «Все входящие — на этот внутренний IP»; грубее, чем port forward | ONT/AX3 |
| **PPPoE** | Тип подключения, где логин/пароль выдаёт ISP; нужен при bridge | ISP |

**Как понять, где статический IP.** Если AX3 WAN = `203.0.113.10` — ONT уже
в bridge (или отдаёт публичный IP). Если AX3 WAN = `192.168.1.x` — ONT в
route mode, публичный IP на ONT, и без admin на ONT inbound не сделать.
По текущей трассе (`192.0.2.1 → 192.168.1.1 → ISP`) видно, что ONT
**сейчас в route mode**.

---

## 1. Что меняет статический IP

| Вопрос | Без статики / с CGNAT | Со статикой |
|---|---|---|
| Inbound port-forward | хрупко/невозможно | **возможен** |
| DDNS | нужен | не нужен |
| WireGuard endpoint | hostname + обновление | `IP:51820` |
| Tailscale | почти обязателен | **не обязателен** |
| Публичный HTTPS | сложно | Caddy + домен + 443 |
| Риск | меньше (нет входа) | выше: появился публичный listener |
| Сложность | низкая | средняя (два NAT + firewall) |

**Что не меняется:**
- MFA и сильный пароль в HA — обязательны.
- `8123` наружу **не** публикуем; только WireGuard UDP 51820 (и, при
  необходимости, Caddy 443).
- Double NAT остаётся: ONT `192.168.1.1` → AX3 `192.0.2.1` → `lab-host`.
  Нужны правила на **двух** устройствах (или bridge/DMZ).
- HA `privileged` + docker group = root; публичный вход повышает цену
  компрометации.
- LAN/локальный голос/MA/Pulsar не должны меняться.
- iOS remote push без Link не появится; Android local push работает через
  WireGuard, если VPN always-on.

---

## 2. Снимок ONT/AX3 (2026-10-26)

| Параметр | ONT | AX3 |
|---|---|---|
| Модель | GPON Home Gateway (Huawei) | AX3000 / WS7100-25 |
| ПО | — | `2.0.0.115(SP3C956)`, HarmonyOS 3.0.3 |
| WAN | `1_INTERNET_R`, **Dynamic DHCP** | `192.168.1.64` |
| WAN MAC | — | `02:00:00:00:00:01` |
| Публичный IP | **`203.0.113.10` /26**, GW `2.134.90.189` | — |
| LAN | `192.168.1.1` | `192.0.2.1` |
| DNS | `82.200.159.82`, `82.200.160.100` | `192.168.1.1` |
| IPv6 | Down / off | off |

**Вывод:** ONT — route mode, публичный IP на ONT; AX3 — второй NAT. Для
WireGuard нужны два правила: ONT `UDP 51820 → 192.168.1.64:51820`, AX3
`UDP 51820 → 192.0.2.26` (Ethernet) или `.25` (Wi-Fi).

**Про «статический» IP:** ONT показывает `Connection Mode: Dynamic DHCP`, но
провайдер может закреплять адрес за MAC ONT — тогда DHCP-клиент всегда
получает `203.0.113.10`. Это нормально. Если закрепления нет, адрес может
смениться после reboot/lease; тогда DDNS + watchdog или Tailscale. Уточнить
у ISP; фактическая проверка — reboot ONT/наблюдение.

---

## 3. Проверить ONT, inbound и admin (30–60 мин)

### 3.1. Проверки

1. **ISP/inbound.** Статический IP подтверждён владельцем. Уточнить у ISP:
   разрешены ли входящие соединения, не блокируются ли UDP 51820/TCP 443,
   можно ли сделать port mapping/DMZ/bridge. Это главный внешний риск.
2. **WAN-режим ONT.** Войти в `http://192.168.1.1` (ISP-учётка) и посмотреть
   WAN: `Static IP` / `DHCP` / `PPPoE`. По трассе ONT сейчас **route mode**
   (второй NAT); подтвердить в UI.
3. **AX3 WAN IP.** AX3 UI → `http://192.0.2.1/html/index.html#/wan`
   (или Home/Internet/WAN status): если WAN IP = `203.0.113.10` — ONT bridge;
   если `192.168.1.x` — ONT route, и публичный IP на ONT. Записать AX3 WAN
   IP — он нужен для правила на ONT.
4. **Admin на ONT.** Проверить, пускает ли `192.168.1.1` в настройки
   (sticker/ISP-аккаунт/звонок в поддержку). Без этого inbound невозможен
   (кроме Tailscale/Cloudflare).
5. **Admin на AX3.** Проверить `http://192.0.2.1`; нужен для port forwarding
   (`#/nat`) и/или DMZ (`#/dmz`). Пароль — тот, что задавали при настройке
   Wi-Fi; если забыли — сброс AX3, но это отдельная процедура.
6. **Проверка inbound.** После настройки listener/forward — handshake
   WireGuard с телефона на cellular. Если handshake есть, inbound работает.
7. **Whois.** `2.134.90.x` — `Dynamic GPON-FTTH` (Kazakhtelecom). Это имя
   пула, а не приговор: статический адрес могли выдать из него. Доверяем
   договору/ISP и фактическому handshake, а не netname.

### 3.2. Если admin на ONT/AX3 есть

Переходим к §5 (WireGuard) и, при необходимости, §6 (Caddy).

### 3.3. Если IP окажется динамическим

Даже при подтверждённой статике полезно знать план B: если ISP сменит адрес
или handshake не пойдёт, есть три пути:

| Путь | Как | Минусы |
|---|---|---|
| **DDNS + WireGuard** | DuckDNS/Cloudflare DDNS; `Endpoint = hostname`; watchdog обновляет endpoint при смене IP | `wg-quick` резолвит endpoint при старте; при смене IP нужен `wg set`/restart; дополнительный moving part |
| **Tailscale** | см. `REMOTE-ACCESS.md` | SaaS metadata; зато ноль роутерных настроек |
| **VPS relay** | home-initiated WireGuard к VPS; телефон к VPS | VPS + обслуживание |

Если IP меняется редко (раз в недели/месяцы), DDNS + watchdog приемлем.
Если меняется часто/после каждого PPPoE-сеанса — Tailscale/VPS проще.

### 3.4. Если admin на ONT нет

Это не «конец», но inbound через ONT нельзя. Варианты:

1. **Попросить ISP.** «Нужен проброс UDP 51820 на `192.168.1.<AX3>`» или
   «переведите ONT в bridge, дайте PPPoE/static-параметры». Некоторые
   провайдеры делают это удалённо/по заявке.
2. **Sticker/аккаунт.** На ONT обычно есть наклейка с моделью, SSID и
   паролем пользователя; в договоре/приложении ISP — доступ. Не сбрасывать
   ONT: сброс может снести PPPoE/VLAN/IPTV/VoIP и оставить дом без интернета.
3. **Tailscale/Cloudflare/Link.** Outbound-туннель не требует ONT admin;
   это самый быстрый fallback. Tailscale — если нужен VPN; Cloudflare/Link —
   если нужен браузер без клиента.
4. **AX3-only forward бесполезен** при ONT route mode: пакет не дойдёт до
   AX3. Сначала ONT (forward/DMZ/bridge), потом AX3.

---

## 4. Топологии

### 4.1. W1 — WireGuard-only (рекомендуется первым)

```text
Internet (static IP)
  │  UDP 51820
  ▼
ONT 192.168.1.1  ── port map 51820 → AX3 WAN IP:51820
  │
AX3 192.0.2.1  ── port forward 51820 → 192.0.2.26:51820
  │
lab-host 192.0.2.26
  └─ wg0 10.10.0.1/24
       └─ phone 10.10.0.2
            └─ http://192.0.2.26:8123  (HA, внутри VPN)
```

- Наружу открыт **только UDP 51820**; HA `8123` не проброшен.
- HA login по приватному IP разрешён; MFA.
- Браузерный микрофон/Assist на `http://` может быть заблокирован
  (secure context); **Android HA app** использует native mic — Assist
  работает. Для браузерного голоса — W2.
- Android local push — при always-on VPN. iOS push — нет (Link).

> В примерах ниже `.26` — Ethernet. Сейчас `enp3s0`/`.26` **DOWN**; если
> кабель не вернули, замените `.26` на `.25` (Wi-Fi) во всех forward, URL и
> тестах. Один forward смотрит на один IP; failover Ethernet↔Wi-Fi inbound
> не покрывает.

### 4.2. W2 — WireGuard + Caddy HTTPS

```text
Internet (static IP)
  ├─ UDP 51820 → lab-host WireGuard
  └─ TCP 443  → lab-host Caddy → 127.0.0.1:8123
```

- VPN для владельца; HTTPS-домен для семьи/браузера.
- HA MFA; Caddy TLS (Let's Encrypt); HA trusted proxy `127.0.0.1`.
- Публичный HA — выше риск; только с MFA, обновлениями, логами.
- Caddy может выпустить сертификат по TLS-ALPN-01 через 443; 80 не обязателен,
  но полезен для HTTP-01/redirect.

### 4.3. W3 — Caddy public only (без VPN)

- Только `443` (и `80`), без WireGuard.
- Проще для семьи/гостей, но HA публичен; MFA + rate limit + обновления.
- Не рекомендую как первый шаг; W1 безопаснее.

### 4.4. R — мини-ПК как главный роутер (отдельный проект)

- Мини-ПК заменяет AX3: WAN → ONT (bridge/DMZ), LAN → AX3 в AP-режиме.
- Нужен **второй NIC** (USB 2.5GbE), PPPoE/static, DHCP/DNS, NAT, firewall,
  mDNS, IPv6 off, DHCP reservations, backup, console access.
- Меняются IP/HA/Pulsar/DNS; риск сломать весь дом; rtl8821ce как AP —
  плохая идея, Wi-Fi оставить на AX3.
- **Не делать одновременно с remote access.** Сначала W1/W2; потом отдельный
  проект с окном обслуживания и rollback на AX3.

### 4.5. Tailscale — когда всё же нужен

Tailscale не нужен, если W1/W2 работает. Он остаётся лучшим fallback, если:
- статика не подтвердилась / IP динамический;
- нет admin-доступа к ONT/AX3 или ISP блокирует inbound;
- сеть сервера нестабильна: forward привязан к одному IP, а Tailscale
  переживает смену интерфейса;
- не хочется трогать роутеры и нужен MagicDNS/TLS/ACL «из коробки».

Можно поставить Tailscale **позже** и не менять HA: хост + `tailscale serve`.
Постоянно держать оба пути (WireGuard + Tailscale) не обязательно: это две
поверхности и два источника истины. Выберите один основной, второй — на
случай аварии.

---

## 5. План W1 — WireGuard на `lab-host`

### Phase W0 — Preflight (30 мин)

1. Backup HA/стека (см. `REMOTE-ACCESS.md` Phase 0):
   ```bash
   cd /home/labuser/nettop && ./ctl.sh backup
   ```
   HA UI → Settings → System → Backups → Create backup. Увезти с сервера.
2. Зафиксировать текущую сеть:
   ```bash
   ip -br addr; ip route
   curl -s https://ifconfig.me/ip
   ss -lntup | grep -E ':(8123|22|51820)\b'
   ```
3. Получить admin-доступ к ONT `192.168.1.1` и AX3 `192.0.2.1`.
   Если доступа нет — W1 невозможен; Tailscale остаётся fallback.
4. Узнать **AX3 WAN IP** (в LAN `192.168.1.0/24`): AX3 UI → Home/Internet/
   WAN status, или ONT → DHCP client list. Он нужен для правила на ONT.
   **Зарезервировать его** на ONT (DHCP reservation по MAC AX3 WAN) или
   поставить AX3 WAN статическим, иначе port mapping на ONT сломается при
   смене lease. Если ONT не даёт reservation — рассматривать bridge/DMZ.
5. Проверить, **какой IP сервера активен** для forward:
   ```bash
   ip -br addr
   ```
   Сейчас `enp3s0`/`.26` **DOWN**, работает `wlp4s0`/`.25`. Правило на AX3
   должно указывать на **живой** IP (`.25`), либо сначала вернуть Ethernet и
   forward на `.26`. Один forward — один IP; Ethernet↔Wi-Fi failover не
   спасает inbound, если правило смотрит на упавший интерфейс. Это важный
   аргумент за Tailscale, если сеть сервера нестабильна.
6. Решить: W1 (VPN only) или W2 (+Caddy/домен). Для семьи/iOS — W2/Link.

**Gate W0:** статика подтверждена или принято решение жить с DDNS; есть
admin на оба роутера; известен AX3 WAN IP; backup есть.

### Phase W1 — WireGuard на сервере (45 мин)

1. Установка:
   ```bash
   sudo apt-get update
   sudo apt-get install -y wireguard wireguard-tools qrencode
   sudo modprobe wireguard
   wg --version
   ```
   На Ubuntu 26.04 `wireguard` — kernel module; `wireguard-tools` — `wg`.
2. Ключи (на сервере, `umask 077`):
   ```bash
   install -d -m 700 /etc/wireguard/keys
   cd /etc/wireguard/keys
   umask 077
   wg genkey | tee server.key | wg pubkey > server.pub
   wg genpsk > phone.psk
   wg genkey | tee phone.key | wg pubkey > phone.pub
   ```
   Ключи не коммитить/не печатать. Для второго устройства — отдельная пара и
   PSK.
3. `/etc/wireguard/wg0.conf` (owner root, 600):
   ```ini
   [Interface]
   Address = 10.10.0.1/24
   ListenPort = 51820
   PrivateKey = <содержимое server.key>

   # Только если телефону нужен весь LAN (192.0.2.0/24), а не один HA:
   PostUp = sysctl -w net.ipv4.ip_forward=1
   PostUp = iptables -A FORWARD -i wg0 -j ACCEPT
   PostUp = iptables -A FORWARD -o wg0 -j ACCEPT
   PostUp = iptables -t nat -A POSTROUTING -s 10.10.0.0/24 ! -o wg0 -j MASQUERADE
   PostDown = iptables -D FORWARD -i wg0 -j ACCEPT
   PostDown = iptables -D FORWARD -o wg0 -j ACCEPT
   PostDown = iptables -t nat -D POSTROUTING -s 10.10.0.0/24 ! -o wg0 -j MASQUERADE

   [Peer]
   # phone
   PublicKey = <phone.pub>
   PresharedKey = <phone.psk>
   AllowedIPs = 10.10.0.2/32
   ```
   Если нужен **только HA** (`192.0.2.26/32`), NAT/forwarding можно не
   включать — меньше поверхности. Если нужен MA/SSH/станция — включить.
4. Включить:
   ```bash
   sudo chmod 600 /etc/wireguard/wg0.conf
   sudo systemctl enable --now wg-quick@wg0
   sudo wg show
   sudo systemctl status wg-quick@wg0 --no-pager
   ```
5. Проверить локально (до роутеров): с телефона в домашнем Wi-Fi подключить
   конфиг с `Endpoint = 192.0.2.26:51820` (временный) и проверить handshake.
   Это исключает ошибки ключей/конфига до проброса.

**Gate W1:** `wg show` показывает peer; handshake идёт по LAN; `ping
192.0.2.26` с телефона; HA открывается по `http://192.0.2.26:8123`;
LAN-доступ не сломан.

### Phase W2 — Проброс на роутерах (30–45 мин)

Сначала **AX3**, потом **ONT** (или наоборот; главное — оба).

#### AX3 (192.0.2.1)

UI: `http://192.0.2.1/html/index.html#/nat` после логина
(«More Functions → Security → NAT Service»; в этой прошивке есть страница
`#/nat`, `#/dmz`).

Добавить Port Forwarding:
| Поле | Значение |
|---|---|
| Protocol | UDP |
| External port | 51820 |
| Internal IP | `192.0.2.26` (Ethernet) или `192.0.2.25` (Wi-Fi) — **только живой** |
| Internal port | 51820 |
| Enabled | yes |

Если нужен W2 (Caddy): добавить TCP 80 и TCP 443 → `192.0.2.26`
(или `.25`, если Ethernet не вернули).

#### ONT (192.168.1.1)

UI: `Forward Rules → Port Mapping Configuration` (Huawei GPON; названия
могут отличаться).

Добавить:
| Поле | Значение |
|---|---|
| WAN interface | интернет-WAN (не IPTV/TR-069) |
| Protocol | UDP |
| External port | 51820 |
| Internal host | **AX3 WAN IP** (`192.168.1.x`) |
| Internal port | 51820 |

Альтернативы:
- **DMZ на ONT** → AX3 WAN IP: все входящие идут на AX3, дальше AX3
  forward/NAT. Проще, но ONT перестаёт быть вторым фильтром; AX3 всё равно
  NAT. Приемлемо, если AX3 admin доверенный.
- **Bridge ONT** → AX3 PPPoE/static: один NAT, только AX3 forward. Чище, но
  требует PPPoE/ISP-креды и переконфигурацию; риск сломать интернет. Делать
  только с бэкапом конфигов и окном.
- **Mini-PC вместо AX3 (R)** — отдельный проект, не сейчас.

#### Проверка

1. С телефона **выключить Wi-Fi**, включить WireGuard с `Endpoint =
   <public-ip>:51820`.
2. `wg show` на сервере: `latest handshake` и transfer > 0.
3. `ping 192.0.2.26`, открыть `http://192.0.2.26:8123`, login + MFA.
4. На сервере `sudo tcpdump -ni any udp port 51820` видит пакеты.
5. С cellular **без** WireGuard: `http://<public-ip>:8123` — timeout;
   `443/80` — closed (если не W2); UDP 51820 не отвечает (WireGuard silent).
6. Перезагрузить ONT/AX3/сервер — handshake восстанавливается.

**Gate W2:** handshake с cellular; HA через VPN; наружу только UDP 51820
(и 443/80 для W2); LAN/локальный голос живы.

### Phase W3 — HA и приложение (30 мин)

1. HA hardening из `REMOTE-ACCESS.md` Phase 1: MFA, пароль, лимит логинов,
   backup. Для W1 trusted proxy **не** нужен (нет reverse proxy); для W2 —
   `127.0.0.1`.
2. W1:
   - В HA app External URL: `http://192.0.2.26:8123`.
   - Internal URL: `http://192.0.2.25:8123` (Wi-Fi) или `.26`.
   - HA `external_url` можно оставить пустым или поставить тот же приватный
     URL; login по приватному IP разрешён. Для ссылок/уведомлений лучше
     домен (W2).
   - Android: WireGuard always-on; battery no restrictions; HA local push.
   - **iOS:** external URL по `http://` может блокироваться ATS/политикой;
     для iOS используйте W2 (HTTPS-домен) или Link.
3. W2:
   - HA `external_url = https://ha.example.com`; trusted proxy `127.0.0.1`;
     Caddy reverse proxy на `127.0.0.1:8123`.
   - HA app External URL = `https://ha.example.com`.
4. Проверить Assist: Android app native mic — работает; браузер на `http://` —
   mic может быть заблокирован; для браузера — W2.

**Gate W3:** HA login + TOTP с cellular; dashboard WebSocket; Assist text/voice;
Android notify; LAN internal URL.

### Phase W4 — Caddy HTTPS (опционально, 45 мин)

Нужен, если:
- семья/гость без VPN;
- браузерный микрофон/Assist;
- хотите нормальный URL/сертификат.

1. Домен: A-запись `ha.example.com` → статический IP. Если IP окажется
   динамическим — DDNS-имя.
2. Проброс TCP 80/443 на `.26` (W2).
3. Caddy:
   ```bash
   sudo apt-get install -y debian-keyring debian-archive-keyring apt-transport-https curl
   # официальный репозиторий Caddy — по docs.caddyserver.com/docs/install
   sudo apt-get install -y caddy
   ```
   `/etc/caddy/Caddyfile`:
   ```caddyfile
   ha.example.com {
       encode zstd gzip
       reverse_proxy 127.0.0.1:8123
   }
   ```
   `sudo systemctl reload caddy`; `sudo systemctl status caddy`.
4. HA: HTTP server → Trust X-Forwarded-For on, trusted proxy `127.0.0.1`;
   external URL `https://ha.example.com`; MFA; IP ban.
5. Проверить: `curl -I https://ha.example.com`; браузер; HA app; WebSocket;
   сертификат; HSTS (Caddy по умолчанию? проверить); логи.
6. Безопасность: публичный 443 — только HA; Caddy не проксирует MA/ESPHome;
   HA MFA; обновления; rate limit/CrowdSec — опционально; не открывать 8123.

**Gate W4:** HTTPS 200, валидный cert, HA login+TOTP, WebSocket, Assist mic
в браузере; наружу 443/80 только Caddy.

### Phase W5 — мини-ПК как роутер (отдельный проект)

Если всё же хотите заменить AX3:

**Требования:**
- второй NIC (USB 2.5GbE RTL8156 или USB GbE RTL8153) — WAN;
- ONT bridge (или DMZ на мини-ПК);
- PPPoE/static на WAN;
- DHCP/DNS (Kea/dnsmasq/AdGuard Home), NAT, nftables;
- AX3 в AP-режиме (Wi-Fi остаётся на AX3; rtl8821ce как AP не использовать);
- mDNS/IGMP, IPv6 off (или настроить), DHCP reservations для `.13/.25/.26`;
- console access (клавиатура/монитор), backup конфигов, окно обслуживания;
- rollback: вернуть AX3 как роутер, вынуть USB NIC.

**Почему не сейчас:** single point of failure; меняются IP/DNS/HA/Pulsar;
риск сломать станцию/MA; rtl8821ce; double NAT проще убрать bridge/DMZ, чем
менять роутер. Сначала W1/W2; R — потом, если действительно нужен
router/firewall на мини-ПК.

---

## 6. Конфиги

### 6.1. WireGuard phone (W1)

```ini
[Interface]
PrivateKey = <phone.key>
Address = 10.10.0.2/32
# DNS не нужен, если ходим по IP; иначе 192.0.2.1
# DNS = 192.0.2.1

[Peer]
PublicKey = <server.pub>
PresharedKey = <phone.psk>
Endpoint = <static-ip>:51820
AllowedIPs = 192.0.2.26/32
PersistentKeepalive = 25
```

Если нужен весь LAN:
```ini
AllowedIPs = 192.0.2.0/24, 10.10.0.0/24
```
и включить NAT/forwarding на сервере (см. wg0.conf). Для HA-only `/32`
безопаснее.

### 6.2. WireGuard Mac

То же, но отдельные ключи/PSK и `Address = 10.10.0.3/32`; в `[Peer]`
`AllowedIPs = 192.0.2.26/32` (или `192.0.2.0/24`). Не забудьте добавить
на сервере отдельный `[Peer]` с `PublicKey` Mac и
`AllowedIPs = 10.10.0.3/32`. Импортировать в официальный WireGuard app; не
хранить ключи в репозитории.

### 6.3. Caddyfile (W2)

```caddyfile
ha.example.com {
    encode zstd gzip
    reverse_proxy 127.0.0.1:8123
}
```

Caddy сам получит сертификат (HTTP-01/TLS-ALPN-01). Нужны 80/443 наружу.
HA trusted proxy `127.0.0.1`; `use_x_forwarded_for: true`.

### 6.4. Firewall/периметр

- Наружу на AX3/ONT: **только UDP 51820** (W1) или **UDP 51820 + TCP 80/443**
  (W2). `8123/8095/8099/6053/6638/22` — **нет**.
- HA `privileged`: обновления; не ставить untrusted custom components.
- ONT/AX3: сменить admin-пароли; отключить WAN-remote-management/UPnP, если
  не нужны; не трогать TR-069 без ISP.
- WireGuard: один peer на устройство; PSK; отзывать потерянные; не печатать
  ключи; `600`.
- Caddy: только HA upstream; MFA; IP ban; логи; обновления; при возможности
  CrowdSec/fail2ban.
- Резервный путь: если сломали forward — LAN; если сломали AX3 — консоль/
  сброс; хранить конфиги.

---

## 7. Проверка/приёмка

| # | Тест | Ожидание |
|---|---|---|
| S1 | `wg show` после старта | peer, handshake, transfer |
| S2 | cellular + WG → `ping 192.0.2.26` | ok |
| S3 | `http://192.0.2.26:8123` | login + TOTP |
| S4 | HA app external URL | dashboard/WebSocket |
| S5 | Assist text/voice (app) | работает |
| S6 | Android notify | local push |
| S7 | cellular без WG → `http://<public-ip>:8123` | timeout |
| S8 | `nmap`/scan public IP | только 51820/udp (и 443/80 для W2) |
| S9 | reboot ONT/AX3/сервер | handshake/HA восстанавливаются |
| S10 | LAN/локальный голос | не сломаны |
| S11 | Caddy (W2) | `https://ha.example.com` 200, cert valid |
| S12 | backup/restore | HA backup восстановим |

---

## 8. Rollback

**W1/W2:**
```bash
sudo systemctl disable --now wg-quick@wg0
sudo rm /etc/wireguard/wg0.conf   # после backup
```
- Удалить port-forward на AX3 и ONT (или вернуть прежние правила).
- HA: external URL очистить; trusted proxy off (если W2); MFA оставить.
- Caddy: `systemctl disable --now caddy`; удалить 80/443 forward.
- Роутеры: сохранить скриншоты/конфиги до изменений; откат в UI.

**R (мини-ПК роутер):** вернуть AX3 как роутер, вынуть USB NIC, восстановить
netplan/DHCP; это отдельный rollback-план.

---

## 9. Риски/gaps

| # | Риск | Влияние | Мера |
|---|---|---|---|
| W1 | «Статический» IP окажется динамическим | WireGuard/URL ломаются | проверить ISP/ONT; DDNS+watchdog или Tailscale |
| W2 | Double NAT: нужны два forward (W1/W2) | ошибка в одном — нет связи | оба правила; AX3 WAN IP; тест handshake |
| W3 | Нет admin на ONT/AX3 | inbound невозможен | Tailscale fallback |
| W4 | ISP блокирует inbound | handshake не идёт | Tailscale/VPS |
| W5 | ONT bridge ломает интернет | весь дом offline | бэкап конфигов, окно, ISP |
| W6 | Публичный 443/HA | атаки/сканеры | MFA, обновления, rate limit, только HA |
| W7 | Ключи WireGuard утекли | доступ в LAN | PSK, ротация, revoke, 600 |
| W8 | Мини-ПК как роутер | single point/сложность | отдельный проект, второй NIC, AX3 AP |
| W9 | Браузер mic на HTTP | Assist в браузере не работает | W2/Caddy |
| W10 | iOS push | нет без Link | Link или смириться |
| W11 | Динамический IP + `wg-quick` endpoint | туннель не переразрешается | watchdog/`wg set` или Tailscale |
| W12 | Forward на `.26`/`.25` | при падении этого интерфейса remote умирает, хотя второй IP жив | forward на стабильный Ethernet; Tailscale fallback; virtual IP/bond — сложно |
| W13 | Смена AX3 WAN IP на ONT | port mapping перестаёт работать | DHCP reservation/static WAN или bridge |
| W14 | HA privileged | root при компрометации | de-privilege отдельно |

---

## 10. Что решить

1. Статика подтверждена (`203.0.113.10`). Уточнить у ISP: **разрешён ли
   inbound** (UDP 51820/TCP 443) и можно ли сделать port mapping/DMZ/bridge?
2. Есть admin на **ONT** и **AX3**? Модель/режим ONT (route/bridge)?
3. Какой **AX3 WAN IP** (для правила на ONT)? Виден в AX3 UI или ONT DHCP.
4. W1 (VPN only) или W2 (+Caddy/домен)? Нужна ли семья/iOS?
5. «Routing на мини-ПК» — это **VPN/ingress за AX3** или **замена AX3
   роутером**? Если второе — есть второй NIC и окно обслуживания?
6. Готовы ли открыть UDP 51820 (и 80/443 для W2)?
7. Есть ли домен для W2? Если нет — DuckDNS/Cloudflare.
8. Если admin на ONT нет — можно ли запросить ISP или согласны на
   Tailscale/Cloudflare как fallback?
9. Готовы ли к DDNS+watchdog, если ISP всё же сменит адрес/заблокирует?

---

## 11. Источники

- Huawei: port mapping — <https://consumer.huawei.com/en/support/content/en-us15806329/>
  (More Functions → Security Settings → NAT Services → Port Forwarding).
- Huawei ONT: Forward Rules → Port Mapping Configuration (HG8245H и др.).
- Huawei AX3: PPPoE/DHCP WAN; bridge/AP ограничения —
  <https://consumer.huawei.com/en/community/details/topicId-129361/>.
- WireGuard: <https://www.wireguard.com/quickstart/>,
  <https://www.wireguard.com/install/>.
- Caddy reverse proxy / automatic HTTPS —
  <https://caddyserver.com/docs/quick-starts/reverse-proxy>,
  <https://caddyserver.com/docs/automatic-https>.
- HA HTTP/trusted proxy — <https://www.home-assistant.io/integrations/http/>.
- HA remote access — <https://www.home-assistant.io/docs/configuration/remote/>.
- Локальные факты: `REMOTE-ACCESS.md`, `projects/home-assistant/INVENTORY.md`, `STATE.md`,
  whois/RDAP `2.134.90.x` (Kazakhtelecom, `Dynamic GPON-FTTH`).
