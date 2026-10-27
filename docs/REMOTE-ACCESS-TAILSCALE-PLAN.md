# REMOTE-ACCESS / TAILSCALE — план на аппрув

**Дата:** 2026-10-26  
**Статус:** **DRAFT FOR APPROVAL**; в системе ничего не менялось.  
**Решение:** удалённый доступ к HA через **Tailscale на `lab-host`** +
`tailscale serve` (HTTPS `:443` → `127.0.0.1:8123`) + **MFA в HA**.  
**Не выбираем:** публичный HA, port-forward `8123`, WireGuard-ключи вручную,
Funnel, Cloudflare/Link (остаются альтернативами).

**Связанные документы:**
- [`REMOTE-ACCESS.md`](REMOTE-ACCESS.md) — полное исследование и сравнение.
- [`REMOTE-ACCESS-STATIC-IP.md`](REMOTE-ACCESS-STATIC-IP.md) — альтернатива
  WireGuard на статическом IP (не выбрана).
- [`../projects/home-assistant/scripts/verify-remote-access.sh`](../projects/home-assistant/scripts/verify-remote-access.sh)
  — read-only проверка после внедрения.

**Progress 2026-10-26 22:47:**
| Phase | Статус |
|---|---|
| 0 | ✅ backup `nettop-20261026-224336.tar.gz` (1.5G, копия на Mac), HA check RC=0 |
| 1 | ✅ HTTP settings + internal URL; ✅ MFA (`auth_module.totp`, owner `Proflyder`); пароль/refresh-tokens — за владельцем |
| 2 | ✅ Tailscale `1.104.2` online; Serve `443 → 127.0.0.1:8123`; cert Let's Encrypt; external URL `https://lab-host.example-tailnet.ts.net` |
| 3 | ✅ tag `tag:homeassistant` + ACL: netmap разрешает только `TCP 443`; `xiaomi-17` в tailnet |
| 4 | ✅ remote HA с cellular и HA app работают; URL исправлен на `https://` |
| 5 | ✅ verify 0/0; reboot `lab-host` 23:46 прошёл, Serve/ACL/HA поднялись; `xiaomi-17` переподключился active/direct |

---

## 1. Что именно делаем

```text
Xiaomi 17 / Mac / ноут
  Tailscale app (login, без ручных ключей)
        │  WireGuard-туннель (Tailscale)
        ▼
lab-host (tailscaled, 100.x)
  └─ tailscale serve :443  (TLS Let's Encrypt *.ts.net)
       └─ http://127.0.0.1:8123
            └─ Home Assistant Container (host network)
```

- HA **не публикуется** в интернет.
- Роутеры **не трогаем**, порты **не пробрасываем**.
- Статический IP/двойной NAT не важны: Tailscale всегда исходящий.
- Наружу виден только tailnet; на tailnet — только `443` (ACL/tag).
- HA защищён паролем + TOTP; Tailscale-аккаунт — MFA у IdP.

---

## 2. Scope

### Входит

| # | Что |
|---|---|
| 1 | Backup HA/стека + config check |
| 2 | HA hardening: смена пароля, TOTP, trusted proxy `127.0.0.1`, лимит логинов, IP ban |
| 3 | Установка Tailscale на хост `lab-host` (Ubuntu 26.04) |
| 4 | Вход в tailnet, имя `lab-host`, отключение key expiry для сервера |
| 5 | MagicDNS + HTTPS Certificates + `tailscale serve` 443→8123 |
| 6 | Tag `tag:homeassistant` + ACL: только `443` |
| 7 | HA URL: internal `http://192.0.2.25:8123`, external `https://lab-host.<tailnet>.ts.net` |
| 8 | Xiaomi 17: Tailscale always-on, HA app internal/external URL, local push |
| 9 | Приёмка: cellular/LAN/reboot/no-Tailscale/Funnel/ACL |
| 10 | Rollback-процедура |

### Не входит

- Публичный HA, Caddy, Cloudflare, Link, Funnel.
- WireGuard на статическом IP, bridge ONT, mini-PC как роутер.
- MA/Spotify/SSH/Zigbee/ESPHome наружу.
- Семейный доступ (можно добавить позже).
- iOS remote push (для iOS — Link).
- Offsite backup (отдельная задача).
- De-privilege HA (отдельный проект).

---

## 3. Assumptions / prerequisites

- [ ] HA healthy: `./ctl.sh status`, `hass --script check_config` RC=0.
- [ ] Есть backup **вне** `lab-host`.
- [ ] Есть Tailscale-аккаунт (или создаём) и MFA у IdP.
- [ ] Xiaomi 17: Android + Tailscale app + HA app; Google Play или APK.
- [ ] HA owner-пароль известен; можно сменить.
- [ ] Есть TOTP-приложение и место для секрета (password manager).
- [ ] Ethernet `.26` желательно вернуть; если нет — работаем на `.25` (Wi-Fi).
- [ ] Окно 2–2.5 часа; HA HTTP settings требуют restart + confirm ≤5 мин.
- [ ] Согласие на установку пакетов и изменение HA-настроек.

---

## 4. Решения, которые нужны до старта

| # | Решение | Вариант | Мой выбор |
|---|---|---|---|
| D1 | Tailnet name | `plhome-lab` / свой | `plhome-lab` |
| D2 | Аккаунт Tailscale | Google / GitHub / email | твой |
| D3 | Internal URL | `.25` (Wi-Fi) / `.26` (Ethernet) | `.26`, если вернём кабель; иначе `.25` |
| D4 | ACL только 443 | да / нет | **да** |
| D5 | SSH через Tailscale | сейчас / позже | **позже** |
| D6 | Семья | сейчас / позже | **позже** |
| D7 | MFA | TOTP | **да** |
| D8 | Смена owner-пароля | да / нет | **да** |
| D9 | Принять metadata Tailscale | да / нет | нужно твоё «да» |
| D10 | Окно | дата/время | нужно твоё |

Если D9 = «нет» — Tailscale не подходит; тогда Link/Cloudflare/VPS или
WireGuard.

---

## 5. Что изменится

| Компонент | Изменение | Обратимо |
|---|---|---|
| Пакеты | `tailscale`, `tailscaled` | `apt remove` |
| Tailscale state | `/var/lib/tailscale/` (node key) | re-auth/remove |
| Serve | `443 → 127.0.0.1:8123`, `--bg` | `tailscale serve reset` |
| Tailscale admin | имя, key expiry, MagicDNS/HTTPS, tag/ACL | в admin console |
| HA `.storage/http` | XFF, trusted `127.0.0.1`, threshold 5, ban on | UI/backup |
| HA `.storage/core.config` | internal/external URL | UI/backup |
| HA `.storage/auth` | TOTP-credential, отзыв refresh tokens | Profile/backup |
| HA app | internal/external URL, always-on | app settings |
| Docker/stack | **без изменений** | — |
| Роутеры/ONT/AX3 | **без изменений** | — |
| Станция/Pulsar/MA | **без изменений** | — |

---

## 6. План выполнения

### Phase 0 — Preflight и backup (20 мин)

**Кто:** я по SSH.

```bash
ssh labuser@192.0.2.25
cd /home/labuser/nettop
./ctl.sh status
docker exec homeassistant hass --script check_config -c /config
curl -sS -o /dev/null -w 'HA=%{http_code}\n' http://127.0.0.1:8123/
./ctl.sh backup
ls -lh backups/ | tail -5
```

Плюс HA UI → **Settings → System → Backups → Create backup**; один архив
увезти с сервера.

Проверить активный IP:
```bash
ip -br addr
```
- `.26` UP → internal URL `.26`;
- `.26` DOWN → `.25` (Wi-Fi), зафиксировать риск.

**Gate 0:** backup вне сервера; HA check RC=0; выбран internal URL.

---

### Phase 1 — HA hardening (40 мин)

**Кто:** ты в HA UI (я подсказываю/проверяю). Делать с LAN, не удалённо.

1. **Пароль owner.** Profile → Security → Change password; уникальный,
   из password manager; «sign out other sessions» — да.
2. **MFA.** Profile → Security → Multi-factor authentication → **Enable**
   (Authenticator app). Сканировать QR, ввести код.
   - TOTP **не даёт recovery codes**; сохранить секрет в password manager +
     офлайн. Это break-glass.
   - Проверить в приватном окне: logout → login → код.
3. **HTTP server.** Settings → **System → Network → HTTP server**:
   | Параметр | Значение |
   |---|---|
   | Server port | `8123` |
   | Trust X-Forwarded-For | **on** |
   | Trusted proxies | **`127.0.0.1`** |
   | Enable IP banning | **on** |
   | Login attempts before ban | **5** |
   | Unlimited login attempts | **off** |
   - Save → HA перезапустит HTTP; **подтвердить новые настройки в течение
     5 минут**. Делать с LAN.
   - `configuration.yaml` не трогать: с 2026.8 HTTP настраивается в UI.
4. **URL.** Settings → System → Network → Home Assistant URL:
   - Internal: `http://192.0.2.26:8123` (если Ethernet) или
     `http://192.0.2.25:8123` (Wi-Fi);
   - Internet: пока пусто, заполним после Serve.
5. **Refresh tokens.** Profile → Security → Refresh tokens: отозвать старые/
   незнакомые (сейчас 7), оставить Xiaomi 17/Mac. Long-lived Hermes — оставить
   (локальный, 600).

**Gate 1:** MFA спрашивается; LAN UI работает; `.storage/http`:
`use_x_forwarded_for: true`, `trusted_proxies: ["127.0.0.1"]`,
`login_attempts_threshold: 5`; HA check RC=0.

**Rollback:** выключить MFA; вернуть HTTP-настройки; restore backup.

---

### Phase 2 — Tailscale на `lab-host` (30 мин + твой login)

**Кто:** я ставлю; ты логинишься.

1. Установка (Ubuntu 26.04 `resolute`; репозиторий Tailscale есть):
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
2. Вход:
   ```bash
   sudo tailscale up --accept-routes=false --ssh=false
   ```
   Открыть URL, войти, подтвердить устройство. `--ssh=false` и
   `--accept-routes=false` — минимальные права; SSH/подсети не нужны.
3. Admin console (console.tailscale.com):
   - Machines → переименовать в `lab-host`;
   - **Disable key expiry** для `lab-host` (иначе remote умрёт через ~180 дней);
   - DNS → **MagicDNS** + **HTTPS Certificates**;
   - tailnet name выбрать **до** первого Serve;
   - записать `lab-host.<tailnet>.ts.net`.
4. Serve:
   ```bash
   sudo tailscale serve --bg --https=443 http://127.0.0.1:8123
   sudo tailscale serve status
   tailscale funnel status        # должно быть пусто/off
   ```
   **Funnel не включать.**
5. HA Internet URL: `https://lab-host.<tailnet>.ts.net`; Save.

**Gate 2:** с cellular + Tailscale HA открывается по HTTPS, TOTP, WebSocket;
LAN работает; Serve только 443→8123; Funnel off; после reboot сервера
`tailscaled` и Serve поднимаются сами.

**Rollback:** `tailscale serve reset`; `tailscale down`;
`systemctl disable --now tailscaled`; HA Internet URL очистить.

---

### Phase 3 — Least privilege: tag + ACL (10 мин)

**Кто:** я подсказываю; ты в admin console.

1. Machines → `lab-host` → Edit ACL tags → `tag:homeassistant`.
2. ACL:
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
3. Проверить с телефона:
   - `https://lab-host...` работает;
   - `http://100.x:8123`, `:22`, `:8095` — **не** работают (ожидаемо;
     LAN SSH остаётся).
4. Если ACL сломал доступ — вернуть прежний ACL; LAN — страховка.

**Gate 3:** tailnet видит только 443; HA работает; SSH/MA/Zigbee снаружи
недоступны.

**Rollback:** убрать ACL/tag в admin console.

---

### Phase 4 — Xiaomi 17 и HA app (20 мин)

**Кто:** ты на телефоне; я проверяю.

1. **Tailscale app:** установить (Play Store; если нет GMS — APK
   tailscale.com/download/android), войти, включить VPN.
2. **Android always-on:** Settings → Network → VPN → Tailscale →
   **Always-on VPN**; «Block connections without VPN» **off**.
3. **HyperOS:** Apps → Tailscale/HA → Battery saver **No restrictions**;
   Autostart **on**; закрепить в недавних.
4. **Private DNS:** если `ts.net` не резолвится — Automatic/Off.
5. **HA app:** Settings → Companion app → сервер:
   - Internal URL: `http://192.0.2.26:8123` или `.25`;
   - External URL: `https://lab-host.<tailnet>.ts.net`;
   - Home SSID: `Stanislav1_Wi-Fi5` (+ 2.4 ГГц, если есть).
6. **Local push:** Persistent connection (Android) — on; проверить:
   ```yaml
   action: notify.mobile_app_xiaomi_17
   data:
     title: remote test
     message: cellular + tailscale
   ```
   Телефон на cellular, Tailscale on, HA в фоне → уведомление.
7. **Assist:** HA app → Assist → текст/голос; native mic.

**Gate 4:** приложение переключает internal/external; уведомление приходит;
Assist работает; после reboot телефона Tailscale/HA восстанавливаются.

**Rollback:** вернуть URL; выключить always-on; удалить Tailscale.

---

### Phase 5 — Приёмка (30 мин)

| # | Тест | Ожидание |
|---|---|---|
| T1 | LAN → `http://192.0.2.25:8123` | 200, login+TOTP |
| T2 | cellular + Tailscale → `https://...ts.net` | 200, валидный cert, login+TOTP |
| T3 | WebSocket | live-обновления без refresh |
| T4 | Service call | выполняется |
| T5 | Assist text/voice | ответ |
| T6 | `notify.mobile_app_xiaomi_17` | приходит на cellular |
| T7 | cellular **без** Tailscale → `http://203.0.113.10:8123` | timeout/refused |
| T8 | `tailscale funnel status` | пусто/off |
| T9 | `nc -vz 100.x 22/8123/8095` с телефона | закрыто (кроме 443) |
| T10 | reboot `lab-host` | Tailscale online, Serve active, HA remote |
| T11 | HA restart | remote возвращается |
| T12 | phone reboot | always-on VPN + app |
| T13 | `verify-remote-access.sh` | PASS (WARN — только если обосновано) |
| T14 | HA backup | создаётся/восстановим |

**Gate 5:** T1–T12, T14 PASS; T13 без FAIL; T7/T8/T9 подтверждают отсутствие
публичного доступа.

---

## 7. Acceptance criteria (что считаем успехом)

- [ ] HA открывается с cellular по `https://lab-host.<tailnet>.ts.net`.
- [ ] Сертификат валидный, без предупреждений.
- [ ] TOTP спрашивается; без кода не пускает.
- [ ] HA **не** доступен по публичному IP `203.0.113.10:8123`.
- [ ] Funnel off; на tailnet только 443.
- [ ] LAN, локальный голос, MA, Pulsar не сломаны.
- [ ] Android local push работает на cellular.
- [ ] После reboot сервера/телефона remote восстанавливается.
- [ ] Rollback проверен/понятен.

---

## 8. Rollback

```bash
# Tailscale
sudo tailscale serve reset
sudo tailscale down
sudo systemctl disable --now tailscaled
# опционально:
sudo apt-get remove -y tailscale
```

HA:
- Settings → System → Network → Home Assistant URL → Internet: очистить;
- HTTP server: trusted proxy выключить / threshold `-1` (если нужно);
- MFA: Profile → Security → выключить (если нужно);
- при полной поломке — restore backup (HA остановить, распаковать, поднять).

Tailscale admin: удалить `lab-host`/tag/ACL, revoke устройство.
**Роутеры не менялись — откатывать нечего.**

---

## 9. Риски и меры

| Риск | Влияние | Мера |
|---|---|---|
| Угон Tailscale-аккаунта | чужое устройство в tailnet | MFA у IdP; device list; revoke; Tailnet Lock (опция) |
| Tailscale outage/block | remote не работает | LAN жив; fallback Link/Cloudflare |
| Tailscale metadata | приватность | принять или Headscale/VPS |
| HA privileged | root при RCE | MFA; обновления; de-privilege позже |
| Потеря TOTP | lockout | секрет в password manager + офлайн |
| `ip_ban`/XFF | remote DoS | trusted proxy; threshold 5; чистить `ip_bans.yaml`; LAN |
| HyperOS убивает фон | push/always-on | No restrictions, Autostart, lock |
| iOS push | нет без Link | Link, если появится iPhone |
| Смена tailnet name | URL ломается | выбрать имя до Serve |
| Key expiry сервера | remote умирает | Disable key expiry |
| Serve cert | первый выпуск до ~60 с | подождать/повторить |
| Публичный Funnel | HA в интернете | не включать; проверять `funnel status` |

---

## 10. Кто что делает

| Шаг | Я | Ты |
|---|---|---|
| Backup/config check | ✅ SSH | подтвердить |
| HA password/MFA | подсказать/проверить | ✅ UI + телефон |
| HA HTTP settings | подсказать/проверить | ✅ UI (LAN) |
| Tailscale install | ✅ SSH | — |
| Tailscale login | дать URL | ✅ аккаунт |
| Admin console | подсказать | ✅ (или я по твоему согласию) |
| Serve/ACL | ✅ SSH/подсказать | подтвердить |
| HA URLs | подсказать | ✅ UI |
| Xiaomi 17 app | чек-лист | ✅ телефон |
| Приёмка | ✅ проверки | ✅ cellular-тест |
| Rollback | ✅ SSH | подтвердить |

---

## 11. Аппрув

Поставь галочки/ответь:

- [ ] **D1.** Tailnet name: `____________` (предлагаю `plhome-lab`).
- [ ] **D2.** Аккаунт Tailscale: `____________` (Google/GitHub/email).
- [ ] **D3.** Internal URL: `.26` (Ethernet) / `.25` (Wi-Fi).
- [ ] **D4.** ACL только `443`: да / нет.
- [ ] **D5.** SSH через Tailscale: сейчас / позже.
- [ ] **D6.** Семья: сейчас / позже.
- [ ] **D7.** MFA TOTP: да.
- [ ] **D8.** Сменить owner-пароль: да.
- [ ] **D9.** Принимаю metadata Tailscale: да / нет.
- [ ] **D10.** Окно: `____________`.
- [ ] **Разрешаю** установить `tailscale`/`tailscaled` на `lab-host`.
- [ ] **Разрешаю** изменить HA HTTP settings/URL/MFA.
- [ ] **Понимаю**, что HA не будет публичным, роутеры не меняются.
- [ ] **Понимаю** rollback и где лежит TOTP-секрет.

После аппрува — Phase 0→5 по этому документу.

**Approved by:** ____________________  **Date:** ____________  
**Окно:** ____________________  **Tailnet:** ____________
