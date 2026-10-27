# REMOTE-ACCESS — удалённый доступ к умному дому через HA

**Дата снимка:** 2026-10-26 20:30 (+05)  
**Статус:** исследование + план; **ничего не менялось** (только read-only
проверки `lab-host`, HA и сети).  
**Область:** HA UI/API/WebSocket/Assist снаружи дома.  
**Вне области:** Music Assistant/Spotify streaming, порты станции, Zigbee-
координатор, Yandex/Alice cloud, provisioning станции.  
**Канонические факты:** `STATE.md`/`projects/home-assistant/HA.md`; здесь — снимок для анализа
со ссылками, а не второй STATE.

> **Обновление 2026-10-26 (реализовано):** Tailscale на `lab-host` +
> `tailscale serve` работают; HA MFA, external URL
> `https://lab-host.example-tailnet.ts.net`, ACL только `tcp:443`; телефон
> `xiaomi-17` в tailnet; remote с cellular проверен. Публичного HA нет,
> роутеры/ONT/AX3 не менялись. План/детали:
> [`REMOTE-ACCESS-TAILSCALE-PLAN.md`](REMOTE-ACCESS-TAILSCALE-PLAN.md).
> WireGuard на статическом IP — альтернатива, **не выбрана**.

> **TL;DR.** Сейчас дом за **двойным NAT** (Huawei AX3 `192.0.2.1` →
> Huawei GPON ONT `192.168.1.1`), публичный IPv4 есть, но он **вероятно
> динамический**, глобального
> IPv6 нет, admin-доступ к обоим ISP-устройствам не документирован. Поэтому
> inbound port-forward — плохой и хрупкий путь. Рекомендация: **Tailscale на
> самом `lab-host` + `tailscale serve` (HTTPS `:443` → `127.0.0.1:8123`) + MFA в
> HA**, без единого изменения на роутерах и без входящих портов. Это бесплатно
> для личного tailnet, даёт валидный TLS `*.ts.net`, E2E WireGuard и работает
> за CGNAT/двойным NAT. Перед включением — обязательный этап hardening HA
> (MFA, trusted proxy, лимит логинов). Платные/альтернативные пути: Home
> Assistant Link (`$6.50/мес`, проще всего, push + offsite backup),
> Cloudflare Tunnel + Access (браузер без клиента, но TLS терминируется в
> Cloudflare), self-hosted WireGuard/Headscale на VPS (без SaaS, дороже по
> труду). Port-forward, Funnel и публикация `8123/8095/8099/6053/6638` —
> **нет**.

---

## 1. Что именно нужно понять

Запрос «удалённый доступ к умному дому через HA» может означать три разные
вещи. Ниже они разведены, чтобы не смешивать транспорт, устройства и голос.

| Сценарий | Что нужно | Входит в этот план |
|---|---|---|
| **S1. Телефон вне дома** | HA UI, управление, история, уведомления | ✅ да |
| **S2. Ноутбук вне дома** | HA UI/API | ✅ да |
| **S3. Голосовой Assist с телефона** | HA Assist text/voice; STT/TTS на `lab-host` | ✅ да |
| **S4. Семья/гость** | ограниченный доступ без админки | ⚠️ опция (Phase 5) |
| **S5. Админ/SSH** | SSH к серверу/станции | ⚠️ опция, отдельно от HA |
| **S6. Музыка/Spotify снаружи** | MA/Connect streaming | ❌ нет; L2 и latency |
| **S7. Zigbee-устройства** | ZHA/Z2M через координатор станции | ❌ нет; остаётся локально |
| **S8. Yandex Alice/Alexa/Google** | облачный голосовой навык | ❌ в A; ⚠️ Alexa/Google — через Link (B) |

**Ключевая мысль:** удалённый доступ — это **транспорт до HA**, а не
«включение умного дома». Он не добавляет устройства, не заменяет Zigbee/
Matter и не делает Music Assistant пригодным для WAN. Зато он резко
увеличивает blast radius: HA — это центр, который видит станцию, музыку,
будильник и (в будущем) замки/свет/датчики. Поэтому MFA и least privilege —
не «после», а **до** первого внешнего пакета.

---

## 2. Что есть сейчас (факты)

### 2.1. Home Assistant

| Параметр | Факт | Источник |
|---|---|---|
| Инсталляция | HA Container `2026.10.4`, `network_mode: host`, `privileged: true` | `docker ps`, compose |
| Порт | `0.0.0.0:8123`, HTTP (без TLS) | `ss -lntp`, `.storage/http` |
| URL | `internal_url: null`, `external_url: null` | `.storage/core.config` |
| HTTP-настройки | `server_port: 8123`, `login_attempts_threshold: -1` (без лимита), `ip_ban_enabled: true`, `trusted_proxies` нет, `use_x_forwarded_for` нет | `.storage/http` |
| Пользователи | owner `Proflyder` + системный `Home Assistant Content`; MFA не включён | `.storage/auth` |
| Токены | 7 refresh tokens, 1 long-lived (Hermes, локальный) | `.storage/auth` |
| Мобильное приложение | `mobile_app` → `Xiaomi 17`; `notify.xiaomi_17` есть | `core.config_entries` |
| Cloud | `.cloud` пуст, аккаунт Nabu Casa не привязан | `/config/.cloud` |
| Custom components | нет | `custom_components/` |
| Интеграции | ESPHome (Pulsar), Music Assistant, Wyoming Whisper/Piper/ONNX, Hermes/`llama_cpp`, MCP Server, mobile_app, met, sun, shopping_list | `core.config_entries` |
| Сущности | 102; из «умного дома» — Pulsar (33 entities), MA-плееры, helpers, скрипты, будильник | HA API |
| Zigbee | роль **Off**, `:6638` закрыт; реальных Zigbee-устройств в HA нет | HA API, `nc` |
| MA/Spotify | MA `2.10.8`, Spotify Connect работает; Liked Songs/MA — отдельная история | `STATE.md` |

**Что это значит.** Снаружи имеет смысл открывать только HA (`8123` →
Tailscale `443`). Всё остальное на `lab-host` — внутренние сервисы без
нормальной аутентификации или с L2-зависимостью: MA `8095/8097/8927`,
Whisper `10300`, Piper `10200`, Hermes `8642/8643` (loopback), ESPHome `6053`,
станция `8099` (bearer), Zigbee `6638` (без auth). Их **не** выставляем.

### 2.2. Сеть и NAT

| Элемент | Факт |
|---|---|
| Станция | `192.0.2.13` (Wi-Fi), Pulsar `:6053`, `:8928`, `:8099` |
| `lab-host` | Ethernet `192.0.2.26` — **сейчас DOWN**; Wi-Fi `192.0.2.25` — UP |
| Роутер LAN | `192.0.2.1` — **HUAWEI WiFi AX3 (WS7100-25)**, ISP-профиль `TELECOM` |
| Второй NAT | `192.168.1.1` — **Huawei GPON Home Gateway** (ONT провайдера) |
| Публичный IPv4 | есть, `2.134.90.x` (не RFC6598 CGNAT); dedicated/shared не подтверждён; **динамический** `[INFERRED]` |
| IPv6 | глобального нет: только `fe80::/64`; AX3 `support_ipv6_enable: 0` |
| Трасса | `192.0.2.1` → `192.168.1.1` → `2.134.90.x` (ISP) → … |
| UPnP/IGD | на M-SEARCH не ответил; port-forward на роутерах не документирован |
| Admin-доступ | логинов AX3/ONT в `secrets/CREDENTIALS.md` нет |
| Firewall на `lab-host` | `ufw` inactive; `INPUT ACCEPT`; правила только Docker |

**Вывод по сети:** публичный IPv4 есть, но он за двумя NAT и, вероятно,
меняется. Чтобы сделать inbound, нужно: (1) пробросить порт на ONT
провайдера, (2) пробросить его на AX3, (3) иметь DDNS, (4) иметь static IP
или мириться с переездами, (5) открыть наружу хотя бы один порт. Это
**четыре** зависимости и один публичный listener ради HA. Для домашнего
умного дома это плохой trade-off, особенно когда есть outbound-overlay.

### 2.3. Безопасность сейчас

- MFA **нет**; пароль owner — единственный барьер.
- HTTPS **нет**; HA отдаёт HTTP.
- Внешнего URL **нет**; приложение ходит только по LAN.
- VPN/reverse proxy/Cloudflare/Tailscale **не установлены**.
- `ufw` выключен, хостовый firewall отсутствует; порты слушают `0.0.0.0`.
- HA-контейнер `privileged` + пользователь в группе `docker` = фактически
  root на хосте; компрометация HA — компрометация сервера.
- Долгоживущий HA-токен Hermes лежит на сервере (`~/.config/station/...`,
  600); remote access его не открывает, но при компрометации HA/сервера
  токен — это доступ к HA в обход MFA.
- `:8099` станции — bearer-token, LAN-only; `:6638` Zigbee — **без auth** и
  умеет стирать сеть; `:31116` на Mac — без auth (отдельный P0).
- Политика проекта: `8123/8095/8099/6053` не в интернет; удалённо — только
  VPN. Этот документ ей следует.

### 2.4. Почему текущее состояние не готово к remote

| Проблема | Почему блокер |
|---|---|
| Нет MFA | один украденный пароль = весь дом |
| Нет TLS | логин/токены по HTTP; браузер и приложение ругаются; mic/Assist в браузере ограничены |
| Нет trusted proxy | reverse proxy/Serve будут получать `400` или неверный IP |
| `login_attempts_threshold: -1` | brute force не блокируется |
| Нет external URL | приложение не знает, куда идти вне LAN; ссылки/уведомления ломаются |
| Ethernet DOWN | сервер живёт на Wi-Fi/DKMS-драйвере; remote зависит от стабильности Wi-Fi |
| Нет offsite backup | при поломке/краже сервера нечего восстанавливать |
| HA `privileged` | внешний доступ повышает цену любой HA-уязвимости |

---

## 3. Требования

### 3.1. Функциональные

- **R1.** С телефона вне дома открывается HA UI, работают WebSocket-обновления,
  сервис-вызовы, история, Assist (text/voice).
- **R2.** Домашний путь не меняется: `http://192.0.2.25:8123` (и `.26`,
  когда Ethernet вернётся), локальный голос, MA, Pulsar — как сейчас.
- **R3.** Уведомления на Android приходят вне дома (local push через VPN).
- **R4.** Вход защищён MFA; TLS-сертификат валидный, без ручного trust.
- **R5.** Rollback — за ≤10 минут, без выезда и без изменений на роутерах.
- **R6.** Семейный доступ (опционально) — без админки и без доступа к
  SSH/MA/Zigbee.

### 3.2. Нефункциональные

| Критерий | Требование |
|---|---|
| NAT | работает за double NAT и dynamic IP |
| Входящие порты | 0 |
| Изменения роутеров | 0 |
| Стоимость | 0 или ≤$7/мес |
| Приватность | трафик E2E; минимум metadata третьей стороне |
| Обслуживание | ≤15 мин/мес |
| Отказоустойчивость | LAN и локальный голос работают при падении WAN/VPN |
| Blast radius | снаружи виден только HA; SSH/MA/Zigbee/ESPHome — нет |

### 3.3. Non-goals

- Публичный HA без VPN (Funnel/port-forward/публичный reverse proxy).
- MA/Spotify streaming через WAN.
- Zigbee-координатор через WAN.
- Yandex Alice / навык «Дом с Алисой» (потребует публичный HTTPS + Yandex
  OAuth; конфликт с карантином).
- Замена HAOS/Supervisor; миграция на VM.

---

## 4. Варианты

### 4.1. Матрица

| Критерий | **A. Tailscale + Serve** | **B. HA Link (Cloud)** | **C. Cloudflare Tunnel + Access** | **D. VPS WireGuard/Headscale** | **E. Port-forward + DDNS** | **F. IPv6** |
|---|---|---|---|---|---|---|
| За double NAT | ✅ | ✅ | ✅ | ✅ | ❌/⚠️ | ❌ |
| Dynamic IP | ✅ | ✅ | ✅ | ✅ | ⚠️ DDNS | ⚠️ |
| Без входящих портов | ✅ | ✅ | ✅ | ⚠️ VPS слушает | ❌ | ❌ |
| Валидный TLS | ✅ `*.ts.net` | ✅ | ✅ | ⚠️ настроить | ✅ LE | ✅ |
| Клиент на телефоне | ✅ | ❌ | ❌ | ✅ | ✅ | ❌ |
| Android push вне дома | ✅ local push | ✅ | ⚠️ local push | ✅ local push | ✅ local push | — |
| iOS push вне дома | ❌ | ✅ | ❌ | ❌ | ❌ | — |
| Семья без клиента | ❌ | ✅ | ✅ | ❌ | ❌ | — |
| Стоимость | **0** | $6.50/мес | 0 + домен | VPS $3–5/мес | 0 + домен | 0 |
| Приватность/контроль | ⚠️ metadata Tailscale | ⚠️ Nabu Casa relay | ⚠️ TLS в Cloudflare | ✅ максимум | ⚠️ публичный порт | ⚠️ публичный порт |
| Трудозатраты | низкие | минимальные | средние | высокие | средние | высокие |
| Главный риск | Tailscale outage/block | Link outage/цена | CF/домен/ACL | VPS/ключи | exposure/двойной NAT | exposure |

### 4.2. A — Tailscale на хосте + Serve (рекомендация)

**Как работает.** `tailscaled` на `lab-host` поднимает WireGuard-интерфейс и
получает адрес `100.x` в личном tailnet. Телефон/ноут — тоже узлы tailnet.
`tailscale serve` слушает `:443` **только на tailscale-интерфейсе**, получает
Let's Encrypt-сертификат на `lab-host.<tailnet>.ts.net` и проксирует в
`http://127.0.0.1:8123`. Роутеры не трогаем: соединение всегда исходящее
(UDP 41641, при блокировке — DERP по 443/TCP).

**Почему подходит:**
- double NAT/dynamic IP не важны; CGNAT тоже не важен;
- **0** входящих портов и **0** изменений на AX3/ONT;
- валидный TLS, HA login по домену (HA не любит IP для внешнего входа);
- E2E WireGuard; Tailscale coordination видит metadata, но не payload;
- бесплатный Personal: 6 users, unlimited devices, ACL;
- Android local push работает через тот же WebSocket (проверить на Xiaomi;
  `[INFERRED]` по docs Companion, тест обязателен);
- ACL/tag позволяют оставить на tailnet только `443`;
- LAN-путь и локальный голос не меняются; WAN упал — LAN жив.

**Минусы/риски:**
- зависимость от Tailscale coordination/DERP; при outage новые соединения
  могут не установиться (LAN работает);
- metadata (имена устройств, IP, время) у Tailscale; для параноидального
  режима — Headscale на VPS;
- iOS remote push всё равно требует Link (local push на iOS только на
  Internal URL/SSID);
- нужен клиент Tailscale на каждом устройстве; «гостю с браузера» не подойдёт;
- при direct-недоступности трафик идёт через DERP — latency для UI ок, для
  голоса может быть заметна;
- Tailscale может быть заблокирован в некоторых сетях/странах; проверить
  `tailscale netcheck` из нужных мест.

**Не роутер.** AX3/ONT как VPN-сервер не рассматриваем: ISP-прошивки,
double NAT, admin-доступ не документирован. Tailscale на хосте проще и не
зависит от роутера. Если позже перейдёте на HAOS/Supervised — Tailscale
ставится app-ом, архитектура Serve/ACL та же.

### 4.3. B — Home Assistant Link (ex-Cloud)

**Как работает.** Штатная интеграция `cloud` в HA открывает исходящий
туннель к Nabu Casa; в UI включается Remote access, появляется
`https://<id>.ui.nabu.casa`. Никаких портов/клиентов. С 2 декабря 2026
Cloud переименовывается в **Link**; сейчас $6.50/мес или $65/год, 31-дневный
trial без карты.

**Плюсы:** самый простой путь; push на iOS/Android; Alexa/Google; offsite
backup (encrypted, <5 ГБ, один бэкап); voice STT/TTS; не нужно ставить
клиент; работает за double NAT.

**Минусы:** платно; трафик проходит через relay Nabu Casa (доверие к
провайдеру; end-to-end не гарантируется); меньше контроля/ACL; для Container
Cloud backup надо проверить в триале (документация требует HA 2025.1+ и
automatic backups, про Supervisor не сказано — `[GAP]`).

**Когда выбирать:** если приоритет — «ноль обслуживания», iOS-устройства в
семье, offsite backup и голосовые ассистенты; или как дополнение к A только
для push/backup (Remote UI можно не включать).

### 4.4. C — Cloudflare Tunnel + Access

**Как работает.** `cloudflared` на `lab-host` открывает исходящий туннель к
Cloudflare; домен `ha.example.com` → туннель → `127.0.0.1:8123`. Access
(Zero Trust) добавляет email OTP/SSO перед HA. Портов на роутере нет.

**Плюсы:** бесплатно (нужен домен); браузер без клиента; Access-политики;
WebSocket работает; хорош для семьи/гостей.

**Минусы:** TLS терминируется в Cloudflare (они видят трафик); домен/DNS/
Access — новые зависимости; HA всё равно требует свой login (двойная
аутентификация); push вне дома на iOS нет, на Android — только local push;
нужно аккуратно не открыть HA публично при ошибке Access; Cloudflare может
блокировать/менять условия.

**Когда выбирать:** нужен именно браузерный доступ без установки клиента
(семья/гость), и вы готовы доверять Cloudflare.

### 4.5. D — Self-hosted WireGuard/Headscale на VPS

**Как работает.** Домашний `lab-host` — WireGuard-клиент, инициирует туннель
к VPS; телефон — второй peer; VPS роутит к `192.0.2.0/24` или только к HA.
Double NAT не мешает (исходящее соединение). Для TLS — Caddy/HA self-signed
внутри VPN; для «Tailscale-подобного» UX — Headscale + tailscale-клиенты.

**Плюсы:** никакого SaaS; полный контроль; можно ограничить только HA;
E2E.

**Минусы:** VPS ($3–5/мес) + администрирование; ключи/peer-конфиги; TLS;
Headscale не является официальным Tailscale и имеет ограничения; VPS —
новая точка отказа/атаки; нужен DNS.

**Когда выбирать:** принципиальный отказ от Tailscale/Nabu Casa/Cloudflare
и готовность поддерживать VPS.

### 4.6. E — Port-forward + DDNS + Let's Encrypt

**Почему нет:**
- два NAT (ONT + AX3), оба ISP-managed; логины в репозитории не
  документированы, доступ не проверялся;
- публичный IP динамический; нужен DDNS;
- открытый `8123`/`443` — публичная поверхность атаки; MFA обязательна, но
  HA всё равно светится;
- даже если пробросить только WireGuard UDP, это сложнее и хрупче, чем A;
- политика проекта прямо запрещает `8123` в интернет.

**Когда:** только если провайдер даст static IP + bridge ONT + admin AX3, и
вы сознательно хотите свой публичный вход. Для HA это не оправдано.

### 4.7. F — IPv6

Глобального IPv6 нет; AX3 devcap `support_ipv6_enable: 0`. Даже если
включить, нужен firewall + DDNS + TLS + MFA, и HA всё равно публичен.
Не путь.

### 4.8. G — ZeroTier / Netbird

Похожие overlay-сети. ZeroTier — L2, free 25 devices, но нет своего
HTTPS-Serve; нужен reverse proxy/TLS. Netbird — open source, self-hostable,
WireGuard; ближе к D. Не дают выигрыша против A для этого кейса; держим как
резерв, если Tailscale не подходит по политике.

### 4.9. Решение

**Основной путь — A.** Он единственный, который одновременно: не требует
изменений на двух NAT-роутерах, не зависит от dynamic IP, не открывает
входящих портов, даёт валидный TLS и E2E, бесплатен и сохраняет LAN/голос.

**Если статический IP подтверждён и есть admin на ONT/AX3** — см.
[`REMOTE-ACCESS-STATIC-IP.md`](REMOTE-ACCESS-STATIC-IP.md): **WireGuard на
`lab-host` + UDP 51820** становится основным, Tailscale не обязателен; W2
добавляет Caddy/443 для семьи/браузера. A остаётся fallback, если inbound
не работает.

**B** — если нужен «официальный» zero-maintenance, iOS push, offsite backup
или Alexa/Google. **C** — если нужен браузерный доступ без клиента для семьи.
**D** — если SaaS неприемлем. **E/F** — не использовать.

---

## 5. Целевая архитектура

```text
Телефон / ноут вне дома
  Tailscale app (WireGuard, 100.x)
        │  исходящее: UDP 41641; fallback DERP 443/TCP
        ▼
  Tailscale coordination (только metadata; payload не видит)
        │
        ▼
  lab-host (tailnet, 100.x)
    tailscaled
      └─ tailscale serve :443  (TLS Let's Encrypt *.ts.net)
           └─ http://127.0.0.1:8123
                └─ HA Container (host network)
                     ├─ ESPHome  → Pulsar 192.0.2.13:6053   (LAN)
                     ├─ MA       → 192.0.2.25:8095         (LAN)
                     ├─ Wyoming  → 127.0.0.1:10300/10200     (local)
                     └─ Hermes   → 127.0.0.1:8642/8643       (local)

Домашний путь (не меняется):
  телефон/ноут в Wi-Fi → http://192.0.2.25:8123
  Mac/станция          → LAN как сейчас
  роутеры              → без port-forward, без изменений
```

**Границы доверия:**
- Публичный интернет → только tailnet-клиенты с валидным Tailscale-ключом.
- Tailscale-клиент → только `443` на `lab-host` (ACL/tag), не весь LAN.
- Serve → только `127.0.0.1:8123`; TCP-peer для HA — `127.0.0.1`,
  реальный клиент — через `X-Forwarded-For`.
- HA → MFA + пароль; long-lived токены остаются локальными.
- LAN → как раньше; `8123` доступен из `192.0.2.0/24`.
- Станция/MA/Zigbee/SSH — **не** на tailnet.

---

## 6. План реализации

Принципы: **одно изменение за шаг**, gate после каждого, rollback заранее,
никаких изменений на роутерах, `tailscale serve` **не** Funnel.

### Phase 0 — Preflight, backup, решение (≈30 мин)

1. Зафиксировать состояние и здоровье:
   ```bash
   ssh labuser@192.0.2.25
   cd /home/labuser/nettop
   ./ctl.sh status
   docker exec homeassistant hass --script check_config -c /config
   curl -sS -o /dev/null -w 'HA=%{http_code}\n' http://127.0.0.1:8123/
   ```
2. Сделать backup **до** любых изменений:
   ```bash
   ./ctl.sh backup
   ls -lh backups/ | tail -5
   ```
   Плюс HA UI → **Settings → System → Backups → Create backup**. Один архив
   увезти с сервера (Mac/внешний диск); backup содержит `secrets.yaml` и
   `.storage/auth` — считать его секретом. Проверить restore хотя бы
   частично: распаковать в `/tmp` и убедиться, что `homeassistant/config`,
   `data/music-assistant` и `.env` на месте; полный restore — на тестовом
   контейнере, не на живом.
3. Записать текущие настройки (без секретов):
   ```bash
   sudo jq '.data.stable' /home/labuser/nettop/homeassistant/config/.storage/http
   sudo jq '.data | {internal_url, external_url}' \
     /home/labuser/nettop/homeassistant/config/.storage/core.config
   sudo jq '.data.users[] | {name, is_owner}' \
     /home/labuser/nettop/homeassistant/config/.storage/auth
   ```
4. Вернуть Ethernet, если кабель доступен: `ip -br addr`, `ip route`; `.26`
   должен быть UP, default metric 100. Wi-Fi `.25` оставить как failover.
   Причина: remote access не должен висеть на одном DKMS-драйвере.
5. Решить: личный tailnet или семья; Android/iOS; нужен ли Link/VPS; готовы
   ли к Tailscale metadata. Если семья/iOS — сразу закладывать B или C.

**Gate 0:** backup вне сервера; HA healthy; LAN с Mac; известны текущие URL;
Ethernet/Wi-Fi понятны.

**Rollback:** не требуется (read-only + backup).

### Phase 1 — Hardening HA до первого внешнего пакета (≈45 мин)

Порядок важен: сначала MFA/лимиты, потом proxy, потом Tailscale.

1. **Пароль owner.** HA UI → аватар → **Profile → Security → Change
   password**. Уникальный, из password manager; «sign out other sessions» —
   да, кроме текущей. Если пароль давно не менялся — менять обязательно.
2. **MFA (TOTP).** Profile → Security → **Multi-factor authentication
   modules → Enable** (Authenticator app). Сканировать QR, ввести код.
   - HA TOTP **не выдаёт recovery codes**. Сохранить секрет (не только QR) в
     password manager + офлайн-копию. Это и есть break-glass.
   - Проверить в приватном окне: logout → login → код.
   - Часы `lab-host` синхронизированы (`timedatectl`: `System clock
     synchronized: yes`); TOTP от этого зависит.
3. **HTTP server.** Settings → **System → Network → HTTP server**:
   | Параметр | Значение | Зачем |
   |---|---|---|
   | Server port | `8123` | не меняем |
   | Trust X-Forwarded-For | **on** | Serve приходит с `127.0.0.1` |
   | Trusted proxies | **`127.0.0.1`** | доверяем только локальному Serve |
   | Enable IP banning | **on** | brute force |
   | Login attempts before ban | **5** | баланс |
   | Unlimited login attempts | **off** | иначе лимит не работает |
   - **Save** → HA перезапустит HTTP и попросит подтвердить новые настройки
     в течение **5 минут**. Делать с LAN, не удалённо.
   - После подтверждения проверить `http://192.0.2.25:8123`.
   - `configuration.yaml` **не** трогать: с 2026.8 HTTP настраивается в UI;
     YAML-блок устарел/игнорируется.
4. **URL.** Settings → System → Network → **Home Assistant URL**:
   - Internal: `http://192.0.2.25:8123` (текущий Wi-Fi; если Ethernet
     вернулся — `.26` как основной, но Internal URL один).
   - Internet: пока пусто; заполним после Serve.
   - «Use Home Assistant Cloud» — **off** (если не выбран B).
5. **Токены/сессии.** Profile → Security → Refresh tokens: отозвать старые/
   незнакомые (сейчас 7), оставить телефон/Mac. Long-lived токен Hermes —
   оставить, файл `600`; не печатать.
6. **Пользователи.** Пока только owner + system. Семейного пользователя
   создавать в Phase 5 (не админ).

**Gate 1:** MFA реально спрашивается; LAN UI работает; `.storage/http`
показывает `use_x_forwarded_for: true`, `trusted_proxies: ["127.0.0.1"]`,
`login_attempts_threshold: 5`; HA config check RC=0.

**Rollback:** выключить MFA в Profile; вернуть HTTP-настройки (proxy off,
threshold `-1`), HA сам перезапустится; при полной поломке — restore backup
(HA остановить, распаковать, поднять).

### Phase 2 — Tailscale на `lab-host` (≈45 мин)

> Tailscale ставим **на хост**, не в контейнер: HA уже `network_mode: host`,
> а `tailscaled` должен переживать перезапуски Docker. Add-on в Container
> недоступен by design.

1. **Установка из официального apt-репозитория** (Ubuntu 26.04 `resolute`;
   репозиторий Tailscale для неё есть):
   ```bash
   curl -fsSL https://pkgs.tailscale.com/stable/ubuntu/resolute.noarmor.gpg \
     | sudo tee /usr/share/keyrings/tailscale-archive-keyring.gpg >/dev/null
   curl -fsSL https://pkgs.tailscale.com/stable/ubuntu/resolute.tailscale-keyring.list \
     | sudo tee /etc/apt/sources.list.d/tailscale.list
   sudo apt-get update
   sudo apt-get install -y tailscale
   sudo systemctl enable --now tailscaled
   sudo modprobe tun
   ls -l /dev/net/tun
   ```
   Если `tun` не поднимется после reboot — `echo tun | sudo tee
   /etc/modules-load.d/tun.conf`.
2. **Вход в tailnet:**
   ```bash
   sudo tailscale up --accept-routes=false --ssh=false
   ```
   Открыть напечатанный URL, войти, подтвердить устройство. `--ssh=false` и
   `--accept-routes=false` — минимальные права; SSH/подсети не нужны для HA.
   Если Tailscale DNS конфликтует с `systemd-resolved` (`resolvectl status`),
   можно `sudo tailscale set --accept-dns=false`: для HA/Serve DNS на самом
   сервере не обязателен, а клиенты (телефон/ноут) продолжают резолвить
   MagicDNS. Проверять remote тогда с телефона; скрипт ниже использует
   `curl --resolve`, чтобы не зависеть от DNS сервера.
3. **Admin console** (console.tailscale.com):
   - Machines → переименовать в `lab-host` (или `ha-lab`);
   - **Disable key expiry** для сервера (иначе через ~180 дней remote
     умрёт); для телефона/ноута expiry оставить;
   - DNS → включить **MagicDNS** и **HTTPS Certificates**; tailnet name
     выбрать **до** первого Serve (переименование позже меняет URL);
   - записать `lab-host.<tailnet>.ts.net`.
4. **Serve:**
   ```bash
   sudo tailscale serve --bg --https=443 http://127.0.0.1:8123
   sudo tailscale serve status
   tailscale funnel status        # должно быть пусто/off
   ```
   Ожидаемо: `https://lab-host.<tailnet>.ts.net` → `http://127.0.0.1:8123`.
   `--bg` — конфиг переживает reboot/`tailscale up`.
   **Funnel не включать.**
5. **HA Internet URL:** Settings → System → Network → Home Assistant URL →
   Internet = `https://lab-host.<tailnet>.ts.net` (без порта). Save.
6. **Первый внешний тест:** телефон на cellular, Tailscale **on**:
   - открыть `https://lab-host.<tailnet>.ts.net`;
   - сертификат валиден (браузер без предупреждения);
   - login + TOTP; dashboard обновляется (WebSocket);
   - если HA отдаёт `400`/`Received request from 127.0.0.1` — не включён
     trusted proxy; вернуться в Phase 1;
   - если `000`/cert error — не включены HTTPS Certificates/MagicDNS или
     tailnet name не совпал; `tailscale cert`/повторный Serve.
7. **Least privilege (сразу после успешного теста):**
   - admin console → Machines → `lab-host` → Edit ACL tags →
     `tag:homeassistant`;
   - ACL (пример в Приложении C): `autogroup:member` → только
     `tag:homeassistant:443`;
   - проверить: HA через `https://...` работает; `http://100.x:8123`,
     `:22`, `:8095` с телефона — **не** работают (это ожидаемо; LAN SSH
     остаётся).
   - Если ACL сломал доступ — вернуть прежний ACL в admin console; LAN
     остаётся страховкой.

**Gate 2:** с cellular+Tailscale HA открывается по HTTPS, TOTP, live updates;
LAN работает; `tailscale serve status` только 443→8123; Funnel off; ACL
пускает только 443; после reboot сервера `tailscaled` и Serve поднимаются
сами.

**Rollback:**
```bash
sudo tailscale serve reset
sudo tailscale down
sudo systemctl disable --now tailscaled
# опционально:
sudo apt-get remove -y tailscale
```
В HA: Internet URL очистить. На роутерах ничего не менялось.

### Phase 3 — HA Companion App и уведомления (≈30 мин)

1. **Телефон:** установить Tailscale (Play Store; для Xiaomi без GMS — APK с
   tailscale.com/download/android), войти, включить VPN.
2. **Android always-on:** Settings → Network → VPN → Tailscale →
   **Always-on VPN**. «Block connections without VPN» **off** (иначе при
   падении Tailscale пропадёт и LAN). Разрешить фоновую работу:
   Xiaomi/HyperOS → Apps → Tailscale/HA → Battery saver **No restrictions**,
   Autostart **on**.
3. **HA app:** Settings → Companion app → сервер:
   - Internal URL: `http://192.0.2.25:8123`;
   - External URL: `https://lab-host.<tailnet>.ts.net`;
   - Home SSID: `Stanislav1_Wi-Fi5` (и 2.4-ГГц SSID, если есть);
   - проверить переключение: Wi-Fi off → external; Wi-Fi on → internal.
   - Android 13+ может попросить Location permission для SSID — разрешить,
     иначе авто-переключение не сработает (external всё равно работает).
4. **Local push:** HA app → Settings → Companion app → сервер →
   **Persistent connection** (Android) — включить; постоянное уведомление
   можно свернуть. Проверить:
   ```yaml
   action: notify.mobile_app_xiaomi_17
   data:
     title: remote test
     message: cellular + tailscale
   ```
   Телефон на cellular, Tailscale on, приложение в фоне → уведомление.
   **Без always-on VPN Android local push вне дома не работает** — это
   следствие local push, а не баг. Если always-on неприемлем, для push
   вне дома нужен Link (Firebase/APNs через Cloud).
5. **iOS (если появится):** local push на iOS работает только на Internal
   URL/домашнем SSID; вне дома push без Link не будет. Это ограничение
   платформы, не HA.

**Gate 3:** приложение само переключает internal/external; уведомление
приходит на cellular; сенсоры/состояния обновляются; после reboot телефона
Tailscale и приложение восстанавливаются.

**Rollback:** вернуть URL в приложении; выключить always-on; удалить
Tailscale.

### Phase 4 — Приёмка (≈45 мин)

| # | Тест | Как | Ожидание |
|---|---|---|---|
| T1 | LAN UI | Mac/телефон в Wi-Fi → `http://192.0.2.25:8123` | 200, login+TOTP |
| T2 | Remote UI | cellular, Tailscale on → `https://...ts.net` | 200, валидный cert, login+TOTP |
| T3 | WebSocket | открыть dashboard, изменить состояние | live-обновление без refresh |
| T4 | Service call | Developer Tools → `light.toggle`/`script.alarm_status` | выполняется |
| T5 | Assist text | HA → Assist → «какой будильник» | ответ |
| T6 | Assist voice | Assist → микрофон → «поставь таймер на 1 минуту» | STT/TTS работают |
| T7 | Notify | `notify.mobile_app_xiaomi_17` на cellular | приходит |
| T8 | Без Tailscale | cellular, Tailscale off → `curl -m 5 http://2.134.90.x:8123/` | timeout/refused |
| T9 | Funnel | `tailscale funnel status` | пусто/off |
| T10 | Reboot server | `sudo reboot`; ждать 2–3 мин | Tailscale online, Serve active, HA remote |
| T11 | HA restart | `./ctl.sh restart ha` | Tailscale не падает; remote возвращается |
| T12 | Phone reboot | reboot Xiaomi | always-on VPN + app |
| T13 | WAN down | вынуть WAN/выключить Wi-Fi на роутере (осторожно) | LAN HA/голос живы; remote ожидаемо нет |
| T14 | Brute force | 5 неверных паролей с tailnet | бан; `ip_bans.yaml`; очистить |
| T15 | Cert | `openssl s_client -connect lab-host...:443 -servername ...` | валидная цепочка |
| T16 | ACL | с телефона `nc -vz 100.x 22/8095/8123` | закрыто (кроме 443) |
| T17 | Backup | `./ctl.sh backup`; HA backup | архив создаётся; restore проверен на тесте |

**Gate 4:** T1–T12, T14–T16 PASS; T13 — поведение понятно; T17 — backup
восстановим.

### Phase 5 — Опции (после приёмки)

- **5.1. Семья.** HA: Settings → People → Add person (non-admin). Tailscale:
  invite user (Personal до 6) + ACL `group:family` → только
  `tag:homeassistant:443`, или node sharing. HA dashboard скрыть лишнее;
  помнить, что HA UI не даёт per-entity ACL для non-admin — считайте семью
  доверенной. Альтернатива без клиента — Link/Cloudflare.
- **5.2. Host firewall.** `ufw` сейчас off; включать только после карты
  потоков MA/ESPHome/Sendspin. Для remote-пути достаточно Tailscale ACL;
  firewall — defense-in-depth, но может сломать MA/стриминг. Если включать:
  dead-man switch (`systemd-run --on-active=600 ufw disable`), затем
  разрешить LAN + `tailscale0:443`, проверить и отменить таймер.
- **5.3. Tailscale SSH (опционально).** `sudo tailscale set --ssh`; ACL
  только для admin-устройств; лучше `check mode`; LAN SSH с ключами
  оставить. Не включать, если не нужно: это root-эквивалент при NOPASSWD.
- **5.4. Offsite backup.** Link (encrypted, <5 ГБ) или Borg/rclone с
  шифрованием на внешний диск/облако. Remote access не заменяет backup.
- **5.5. Мониторинг.** Внешний uptime-check на `https://...ts.net` (с
  Tailscale-клиента), `systemctl status tailscaled`, HA `ip_bans.yaml`,
  срок сертификата (Serve renew автоматически), `timedatectl`.
- **5.6. MA/Zigbee.** Не выставлять. MA UI без нормальной auth; Zigbee
  `:6638` без auth и умеет стереть сеть. Если когда-то понадобится MA
  снаружи — отдельный reverse proxy с auth, не Serve «как есть».
- **5.7. De-privilege HA.** `privileged: true` + docker group = root. Это
  отдельный проект: убрать privileged, добавить нужные devices/caps,
  проверить ESPHome/Bluetooth/DBus. Не делать в один день с remote.
- **5.8. Tailnet Lock (advanced).** Защита от вредоносного coordination
  plane; требует управления signing keys; включать только после стабилизации.

### Таймлайн

| Phase | Время | Риск | Кто |
|---|---|---|---|
| 0 | 30 мин | низкий (backup) | владелец |
| 1 | 45 мин | средний (MFA/HTTP restart) | владелец |
| 2 | 45 мин | средний (ACL/сертификат) | владелец |
| 3 | 30 мин | низкий | владелец |
| 4 | 45 мин | низкий | владелец |
| 5 | по желанию | зависит | владелец |

---

## 7. Безопасность и threat model

### 7.1. Что меняется в поверхности атаки

| До | После A |
|---|---|
| HA доступен только из LAN | HA доступен из tailnet по HTTPS |
| Пароль | Пароль + TOTP |
| Нет TLS | Валидный TLS `*.ts.net` |
| Нет входящих портов | Нет входящих портов (outbound overlay) |
| Роутеры не трогаем | Роутеры не трогаем |
| SSH/MA/Zigbee только LAN | остаются только LAN (ACL/tag) |

### 7.2. Угрозы и меры

| Угроза | Путь | Мера |
|---|---|---|
| Украденный телефон с активной сессией | Tailscale + HA refresh token | lock screen; revoke device в Tailscale; revoke refresh token в HA; MFA на новом входе |
| Угон Tailscale-аккаунта | добавление своего узла в tailnet | MFA у IdP (Google/GitHub/…); device approval при наличии; Tailnet Lock; audit |
| Brute force HA | tailnet-клиент | сильный пароль; TOTP; `login_attempts_threshold=5`; ACL только 443 |
| Вредоносный член семьи | tailnet user | ACL `group:family` → 443; HA non-admin; никакого LAN-route; firewall |
| Tailscale outage/block | coordination/DERP | LAN работает; existing connections могут жить; fallback Link/CF для критичного |
| DERP relay | metadata/latency | payload encrypted; `tailscale ping`; при необходимости UDP 41641 forward (optional) |
| Компрометация HA (privileged) | HA vuln/integration | обновления; минимум интеграций; no custom components; de-privilege; Tailscale-only; MFA |
| Утечка long-lived токена | Hermes/файл | 600; не публиковать; ротация; не давать наружу; оба MCP loopback-only и full access |
| Потеря TOTP | телефон потерян | секрет в password manager + офлайн; backup admin/HA backup |
| Бан из-за proxy | brute force через Serve | с корректным XFF HA видит `100.x` и банит per-client; без XFF все выглядят как `127.0.0.1`, и бан блокирует весь remote; чистить `ip_bans.yaml` по LAN/SSH |
| Утечка backup | `secrets.yaml`, auth | шифровать; offsite; не в git/облако без шифрования |
| Zigbee `:6638` | LAN/tailnet | не выставлять; ACL; Pulsar README: порт без auth |
| Station `:8099` | LAN | bearer; не выставлять; ACL |
| Yandex/Alice | cloud | не подключать; отдельный проект |

### 7.3. Приватность

- **Tailscale:** WireGuard E2E; coordination/DERP видят metadata (кто, когда,
  какие узлы), не payload. Для строгого режима — Headscale на VPS (D).
- **Link:** TLS до relay Nabu Casa; доверие к провайдеру; зато encrypted
  backup и push.
- **Cloudflare:** TLS терминируется на edge; Cloudflare видит HTTP; Access
  логирует.
- **Port-forward:** HA виден всему интернету; худший вариант.

### 7.4. Что не делать

- Не включать `tailscale funnel` и не пробрасывать `8123`.
- Не давать tailnet-доступ к `192.0.2.0/24` «на всякий случай».
- Не выставлять MA/ESPHome/Zigbee/SSH на tailnet без ACL.
- Не отключать MFA «на время теста» и не оставлять `threshold: -1`.
- Не хранить TOTP-секрет/пароль/токены в docs/git.
- Не делать remote access и de-privilege HA в один день.

---

## 8. Риски, неизвестности, gaps

| # | Риск/неизвестность | Влияние | Как снять |
|---|---|---|---|
| G1 | Публичный IP динамический; ISP/ONT admin | port-forward невозможен/хрупок | Tailscale; у ISP не уточнять, пока не нужен E |
| G2 | Double NAT | direct-соединение может уйти в DERP | `tailscale netcheck`/`tailscale ping`; при необходимости UDP 41641 forward (optional) |
| G3 | Нет глобального IPv6 | IPv6-путь недоступен | не использовать |
| G4 | Tailscale может быть заблокирован в сети | remote не работает | тест из cellular/public Wi-Fi; fallback Link/CF |
| G5 | Xiaomi/HyperOS убивает фон | push/always-on нестабильны | battery no restrictions, autostart, persistent connection; тест |
| G6 | iOS remote push | нет без Link | Link или смириться |
| G7 | HA 2026.10 UI-лейблы HTTP | шаг может отличаться | docs 2026.10; делать с LAN |
| G8 | Link + Container cloud backup | backup может не появиться | trial; проверить; иначе Borg/rclone |
| G9 | Ethernet сейчас DOWN | Wi-Fi/DKMS — единая точка | вернуть кабель; UPS/мониторинг |
| G10 | HA privileged | root при компрометации | de-privilege отдельным проектом |
| G11 | TOTP без recovery codes | lockout | секрет в password manager + offline |
| G12 | `ip_ban` при неверном XFF | remote DoS после brute force | XFF + trusted proxy; threshold 5; чистить `ip_bans.yaml`; LAN |
| G13 | Смена tailnet name | URL/приложение ломаются | выбрать имя до Serve; не менять |
| G14 | Key expiry сервера | remote умирает через ~180 дней | Disable key expiry для `lab-host` |
| G15 | ACL/tag ошибка | remote 403/timeout | делать с LAN; rollback ACL |
| G16 | Tailscale metadata | приватность | принять или D (Headscale) |
| G17 | MA/Spotify L2 | remote streaming не работает | не обещать; HA media_player only |
| G18 | Zigbee coordinator без auth | стирание сети | не выставлять; ACL |
| G19 | Сертификат Serve | первый выпуск до ~60 с | подождать/повторить; HTTPS on |
| G20 | HA login по IP | `invalid client id` | использовать ts.net домен, не 100.x |

---

## 9. Что нужно решить (вопросы)

1. Доступ нужен **только вам** или семье? Android, iOS, оба?
2. Tailscale (metadata у Tailscale) приемлем или только self-hosted (D)?
3. Готовы ли платить $6.50/мес за Link ради iOS push/offsite backup/Alexa?
4. Нужен ли SSH/MA снаружи или **только HA**?
5. Нужен ли голосовой доступ через Alexa/Google/Yandex?
6. Можно ли вернуть Ethernet на `lab-host` (сейчас `.26` DOWN)?
7. Делать offsite backup сразу или отдельным этапом?
8. Готовы ли к always-on VPN на телефоне (батарея)?
9. Какой tailnet name/домен хотите (выбрать до Serve)?
10. Какой password manager/authenticator для TOTP и пароля owner?

---

## 10. Приложения

### A. Чек-лист HA UI

- [ ] Profile → Security → Change password; sign out other sessions.
- [ ] Profile → Security → MFA → Enable TOTP; сохранить секрет offline.
- [ ] Settings → System → Network → HTTP server: XFF on, trusted
      `127.0.0.1`, ban on, attempts 5, unlimited off; Save + Confirm.
- [ ] Settings → System → Network → Home Assistant URL: Internal
      `http://192.0.2.25:8123`; Internet — после Serve.
- [ ] Profile → Security → Refresh tokens: отозвать лишние.
- [ ] Settings → System → Backups: создать backup; увезти.
- [ ] (Phase 5) Settings → People: non-admin для семьи.

### B. Команды (cheat sheet)

```bash
# HA health
cd /home/labuser/nettop && ./ctl.sh status
curl -sS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8123/

# Tailscale
tailscale status
tailscale ip -4
tailscale serve status
tailscale funnel status
sudo tailscale serve --bg --https=443 http://127.0.0.1:8123
sudo tailscale serve reset

# Проверка HA-настроек (без секретов)
sudo jq '.data.stable' /home/labuser/nettop/homeassistant/config/.storage/http
sudo jq '.data | {internal_url, external_url}' \
  /home/labuser/nettop/homeassistant/config/.storage/core.config

# Проверка remote-URL с сервера
curl -sS -o /dev/null -w '%{http_code}\n' \
  "https://$(tailscale status --json | jq -r '.Self.DNSName' | sed 's/\.$//')/"

# Что слушает
ss -lntp | grep -E ':(8123|8095|8099|6053|6638|22|443)\b'

# Backup
./ctl.sh backup
```

### C. Пример Tailscale ACL

```json
{
  "tagOwners": {
    "tag:homeassistant": ["autogroup:admin"]
  },
  "acls": [
    {
      "action": "accept",
      "src": ["autogroup:member"],
      "dst": ["tag:homeassistant:443"]
    }
  ]
}
```

- Тег вешается на `lab-host` в admin console.
- `autogroup:member` — все пользователи tailnet; если семья — заменить на
  `group:family`.
- SSH/8123/8095 с tailnet после этого закрыты; LAN не затронут.
- Если нужен Tailscale SSH — добавить отдельное правило для admin-устройств,
  не открывать 22 всем.

### D. Источники

- HA: Remote access — <https://www.home-assistant.io/docs/configuration/remote/>
  (2026.10.4).
- HA: HTTP (UI, trusted proxies, IP ban) —
  <https://www.home-assistant.io/integrations/http/> (2026.10.4).
- HA: MFA — <https://www.home-assistant.io/docs/authentication/multi-factor-auth/>.
- HA: Authentication — <https://www.home-assistant.io/docs/authentication/>.
- Companion: Local Push —
  <https://companion.home-assistant.io/docs/notifications/notification-local/>.
- Tailscale: Remote HA — <https://tailscale.com/blog/remotely-access-home-assistant>
  (обновлён 2026-07-16).
- Tailscale: Serve — <https://tailscale.com/kb/1242/tailscale-serve>
  (2026-01-26).
- Tailscale: Install Linux — <https://tailscale.com/kb/1031/install-linux>
  (2026-01-05).
- Tailscale: Pricing — <https://tailscale.com/pricing> (2026-10-26).
- Nabu Casa: Pricing/Link — <https://www.nabucasa.com/pricing/> (2026-10-26;
  переименование 2026-12-02).
- Nabu Casa: Remote access —
  <https://support.nabucasa.com/hc/en-us/articles/26474279202973> (2026-09-10).
- Nabu Casa: Cloud backup —
  <https://support.nabucasa.com/hc/en-us/articles/26294320337181> (2026-01-20).
- Локальные факты: `docs/STATE.md`, `projects/home-assistant/HA.md`, `projects/home-assistant/NETTOP.md`,
  `projects/home-assistant/INVENTORY.md`, `SECURITY.md`, `PENDING.md`.

### E. Evidence log (read-only, 2026-10-26)

```text
$ hostname; ip -br addr; ip route
lab-host
lo               127.0.0.1/8
wlp4s0           192.0.2.25/24
enp3s0           DOWN
default via 192.0.2.1 dev wlp4s0 metric 200

$ curl -s https://ifconfig.me/ip
2.134.90.x            # публичный IPv4, полный адрес не фиксируем в docs

$ tracepath -n 1.1.1.1
1: 192.0.2.1     # HUAWEI WiFi AX3
2: 192.168.1.1     # Huawei GPON ONT (double NAT)
3: 2.134.90.x      # ISP

$ curl -s http://192.0.2.1/api/system/deviceinfo | jq ...
FriendlyName: HUAWEI WiFi AX3; CustDeviceType: WS7100-25;
OperatorName: TELECOM; support_ipv6_enable: 0

$ curl -s http://192.168.1.1/ | grep title
<title>GPON Home Gateway</title>

$ ip -6 route
fe80::/64 dev wlp4s0 ...

$ ufw status
Status: inactive
$ ss -lntp | grep 8123
0.0.0.0:8123

$ jq '.data.stable' .../.storage/http
server_port 8123; login_attempts_threshold -1; ip_ban_enabled true;
(no trusted_proxies / use_x_forwarded_for)

$ jq '.data | {internal_url, external_url}' .../.storage/core.config
null, null

$ jq '.data.users[]' .../.storage/auth
Proflyder (owner), Home Assistant Content (system)
# MFA-credential отсутствует; 7 refresh tokens; 1 long-lived

$ nc -z 192.0.2.13 6638
closed (Zigbee role Off)
```

### F. Read-only проверка

```bash
# с Mac — выполнить на lab-host, ничего не копируя:
ssh labuser@192.0.2.25 'bash -s' \
  < ~/Desktop/yandex-station-lab/projects/home-assistant/scripts/verify-remote-access.sh

# или скопировать и запустить там:
scp ~/Desktop/yandex-station-lab/projects/home-assistant/scripts/verify-remote-access.sh \
  labuser@192.0.2.25:/tmp/
ssh labuser@192.0.2.25 'bash /tmp/verify-remote-access.sh'
```

Скрипт ничего не меняет: проверяет HA local, `tailscaled`, Serve/Funnel,
URL/HTTP-настройки, NTP, порты и remote-URL (если Tailscale уже настроен).
Он печатает PASS/WARN/FAIL и не выводит секреты.
