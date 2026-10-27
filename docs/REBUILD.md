# REBUILD — воссоздание стенда с нуля

**При восстановлении текущего неттопа использовать также
[KUBERNETES-OPERATIONS.md](KUBERNETES-OPERATIONS.md) и
[INSTALL-PROXMOX.md](INSTALL-PROXMOX.md).** С 2026-10-28 runtime — Proxmox +
HAOS VM + Ubuntu/K3s VM; Terraform state и VM recovery — отдельный слой.
Kubernetes cluster/PV/auth восстанавливаются из migration recovery-комплекта;
Docker/Ubuntu/HA Container-разделы ниже описывают **прежний** вариант.
Не запускать одновременно Kubernetes/native services на одних данных.

**Дата:** 2026-10-27  
**Статус:** канонический recovery-гайд. Текущие факты — `STATE.md`; команды —
`OPERATIONS.md`; решения — `DECISIONS.md`; секреты — `SENSITIVE-DATA.md`.  
**Цель:** если умрёт SSD неттопа, будет сброшена колонка или потерян Mac с
Quasar-стендом — восстановить рабочую среду по этому файлу, а не по
транскриптам сессий.

> **Главное правило:** сначала восстанови **секреты и бэкапы**, потом
> инфраструктуру. Без `secrets/` и свежего бэкапа HA восстановление будет
> дольше и может потерять Zigbee/токены/сертификаты.

---

## 0. Что должно быть в recovery-комплекте

| Компонент | Где | Зачем |
|---|---|---|
| Этот репозиторий | Mac/внешний диск | docs, configs, scripts, tools, resources |
| `secrets/` | отдельно, шифрованно | SSH/ADB/роутер, сертификаты, DPP, legacy env |
| Свежий backup неттопа | `backups/maintenance-20261027/…` (предыдущий — `remote-access-20261026/`) | HA config, MA, Whisper/Piper, `.env` |
| Station backups | `backups/emergency-*`, `station-data-*`, `station-originals-*` | `/data`, оригиналы OTA/ADB |
| Quasar kit | `archive/legacy/alice-mitm-bootstrap/`, `archive/legacy/v3rt3p-stack/` | вернуть ADB после factory reset |
| Root kit | `projects/station/tools/quasar-root-kit/`, `artifacts/ssh/` | dropbear/ADB/активация |
| Firmware | `artifacts/firmware/` | **не для Midi**; см. предупреждение |
| Tailscale аккаунт + MFA | у владельца | remote HA |
| HA owner + TOTP secret | password manager + офлайн | вход в HA |
| ONT/AX3 admin | у владельца | сеть/port-forward (если понадобится) |

Подробный список чувствительных путей — `SENSITIVE-DATA.md`. Значения секретов
в docs не хранятся.

---

## 1. Инвентарь (что восстанавливаем)

| Компонент | Значение |
|---|---|
| Станция | Yandex Station Midi `cucumber`, `STATION_SERIAL`, `192.0.2.13`, прошивка `.160` |
| Станция: софт | Pulsar `v0.4.1`, HA ESPHome `:6053`, Sendspin `:8928`, OTA/ADB fortress, egress-guard, BLE gateway |
| Неттоп | DEXP Jupiter JU01CY, Ubuntu Server 26.04.1, `lab-host`, `.25` Wi-Fi / `.26` Ethernet |
| HA | Container `2026.10.4`, host network, `:8123`, MFA, Tailscale Serve |
| MA | `2.10.8`, `:8095`, stream `:8097`, Sendspin `:8927` |
| Matter | `matterjs-server`, `:5580` loopback, HA integration `Matter` |
| STT/TTS | Wyoming Whisper/Piper + ONNX/GigaAM; актуальные версии — `STATE.md` |
| Hermes | `v0.22.5`, loopback `:8642`, voice proxy `:8643`, HA MCP + MA MCP |
| Mac | `192.0.2.11`; Quasar/MITM kit (не активен, нужен для factory reset) |
| Сеть | AX3 `192.0.2.1` → ONT `192.168.1.1`; public static `203.0.113.10`; публичный IPv6 off, link-local IPv6 on для Matter |
| Remote | `https://lab-host.example-tailnet.ts.net` → `127.0.0.1:8123` |

---

## 2. Сценарии и порядок

| Сценарий | Что потеряно | Порядок |
|---|---|---|
| **A. SSD неттопа** | HA/MA/STT/TTS/Hermes/Tailscale | секреты → новый Ubuntu → restore nettop backup → Tailscale → проверка. Станция/Mac не трогаем |
| **B. Factory reset станции** | `/data`, ADB, SSH, Pulsar, guard, BLE token | Mac/Quasar stage 1 → ADB → CA/root → OTA fortress → egress/quarantine → Pulsar → BLE → проверка |
| **C. Mac/Quasar потерян** | DNS/proxy/сертификаты/Quasar backend | восстановить Mac из `archive/legacy/...` + `secrets/` + v3rt3p upstream/архив; проверить stage 1 |
| **D. Полная потеря** | всё | секреты/бэкапы → Mac/Quasar (если нужен root станции) → неттоп → станция → проверка |

**Не делать factory reset станции**, пока не готов Quasar kit и не проверен
stage 1: без него можно остаться без ADB на стоковой прошивке.

---

## 3. Сценарий A — умер SSD неттопа

### 3.1. Базовая ОС и сеть

1. Установить Ubuntu Server 26.04.1 LTS, пользователь `labuser`, OpenSSH.
2. Вернуть сеть по `projects/home-assistant/SETUP.md`:
   - Ethernet `enp3s0` `.26/24`, Wi-Fi `wlp4s0` `.25/24`;
   - gateway/DNS `192.0.2.1`; статические адреса;
   - `systemd-networkd-wait-online --any --timeout=30`.
3. Docker/Compose:
   ```bash
   sudo apt update
   sudo apt install -y docker.io docker-compose-v2 git curl jq
   sudo systemctl enable --now docker
   sudo usermod -aG docker labuser
   ```
4. Secure Boot/MOK — только если сохранился MOK private key; иначе выключить
   Secure Boot или пересобрать DKMS. Детали — `projects/home-assistant/TROUBLESHOOTING.md`.

### 3.2. Восстановить стек из backup

```bash
sudo mkdir -p /home/labuser/nettop
cd /home/labuser
sudo tar -xzf /path/to/nettop-YYYYmmdd-HHMMSS.tar.gz -C /home/labuser/nettop
cd /home/labuser/nettop
ls -la
./ctl.sh up
```

В архиве: `docker-compose.yml`, `.env`, `homeassistant/config`, `data/music-assistant`,
`data/whisper`, `data/piper`, возможно `data/onnx-asr`. Это самый быстрый путь.

Проверить:
```bash
./ctl.sh status
curl -sS -o /dev/null -w 'HA=%{http_code}\n' http://127.0.0.1:8123/
curl -sS -o /dev/null -w 'MA=%{http_code}\n' http://127.0.0.1:8095/
```

### 3.3. Если backup нет

Собрать заново по `projects/home-assistant/stack/` и `projects/home-assistant/HA.md`:

1. `docker-compose.yml` + `.env` из `projects/home-assistant/stack/`;
2. HA config — из последнего бэкапа/архива; иначе onboarding заново;
3. MA — setup, Spotify/Soloist; см. `docs/STATE.md`, `projects/home-assistant/stack/README.md`;
4. Whisper/Piper/ONNX — compose-сервисы и модели; см. `projects/home-assistant/stack/`;
   Пользовательские веса Piper не входят в стандартный каталог: восстановить ONNX + config до выбора пользовательского PIPER_VOICE. Для закреплённого Puriso — [установка/хеши/откат](../projects/home-assistant/tts/puriso/README.md), скрипт `projects/home-assistant/tools/tts-lab/install_puriso.py`. Затем проверить TTS voice/language в pipeline и отдельные alarm options. Не полагаться на один `.env` без весов.
5. Matter — `matter-server` из compose, data `data/matter-server`, HA URL
   `ws://127.0.0.1:5580/ws`; сертификаты/fabric восстановить из data/backup;
6. Xiaomi Home — optional custom component `v0.5.0`:
   clone/install из `XiaoMi/ha_xiaomi_home` в `custom_components`, затем HA
   restart; вход Xiaomi China; импортировать `MJGJD02YL` и Mi Bedside Lamp 2.
   OAuth redirect жёстко `http://homeassistant.local:8123`; при NXDOMAIN
   заменить host на `192.0.2.25`, сохранив путь/query;
7. Hermes — `projects/home-assistant/hermes/README.md` (OAuth, voice proxy, full MCP):
   HA MCP без `tools.include`; MA FastMCP plugin `/mcp/v1` + `HERMES_MA_TOKEN`;
   `platform_toolsets.api_server=[mcp-homeassistant, mcp-music-assistant, web]`;
8. HA MFA, пользователи, интеграции, ESPHome Pulsar, MA, Wyoming;
9. Tailscale — §3.4.

### 3.4. Tailscale / remote HA

```bash
curl -fsSL https://pkgs.tailscale.com/stable/ubuntu/resolute.noarmor.gpg \
  | sudo tee /usr/share/keyrings/tailscale-archive-keyring.gpg >/dev/null
curl -fsSL https://pkgs.tailscale.com/stable/ubuntu/resolute.tailscale-keyring.list \
  | sudo tee /etc/apt/sources.list.d/tailscale.list
sudo apt update && sudo apt install -y tailscale
sudo systemctl enable --now tailscaled
sudo tailscale up --accept-routes=false --ssh=false
```

В admin: `lab-host`, **Disable key expiry**, MagicDNS + HTTPS Certificates,
tailnet name; затем:
```bash
sudo tailscale serve --bg --https=443 http://127.0.0.1:8123
```
HA: external URL `https://lab-host.<tailnet>.ts.net`, trusted proxy
`127.0.0.1`, MFA, TOTP. ACL: `tag:homeassistant` → только `tcp:443`
(снапшот: `backups/tailscale-acl-20261026/`). Проверка:
`projects/home-assistant/scripts/verify-remote-access.sh`.

> Публичный HA/port-forward не использовать. Полный ACL-снапшот и откат —
> `backups/tailscale-acl-20261026/`.

### 3.5. Проверка A

- HA/MA/Wyoming healthy; HA UI + TOTP;
- Pulsar ESPHome online; Assist/TTS работают;
- Tailscale remote с cellular;
- `make test` (hermetic) проходит.

---

## 4. Сценарий B — factory reset колонки (самый важный)

Factory reset сносит `/data`: ADB-маркеры, keeper, SSH, Pulsar, egress-guard,
BLE token. Колонка возвращается к стоковому Quasar/Alice. **Проверенного
factory/recovery-образа Midi нет**; `artifacts/firmware/yandexmidi…` — это
Station 2, не Midi. Factory reset не считать unbrick.

### 4.1. Что понадобится

- Mac `192.0.2.11` с LAN-доступом к колонке;
- `archive/legacy/alice-mitm-bootstrap/` — Bun DNS/HTTPS-прокси, stage 1/2;
- `secrets/legacy-alice-mitm/.env*` — env для stage 1/2;
- сертификаты: bootstrap leaf+intermediate, local CA/leaf — `secrets/certs/`,
  `archive/legacy/alice-mitm-bootstrap/certs/`;
- `projects/station/configs/` — снимки stage1/stage2 env, DNS, скрипты, `station-applied/`;
- `projects/station/tools/quasar-root-kit/` + `artifacts/ssh/` — root kit/dropbear;
- для полного fake backend — `archive/legacy/v3rt3p-stack/` (compose/prompts/
  switch) + `repos/` (исходники уже архивированы) + `secrets/legacy-v3rt3p/.env`;
- роутер: static DNS `quasar.yandex.net` → Mac; вернуть обратно после.

**Юридически/этически:** только своя колонка в своей сети. Bootstrap-комплект
содержит приватный leaf-ключ из публичной утечки CA; не публиковать, не
применять к чужим устройствам. Подробно — `archive/legacy/alice-mitm-bootstrap/NOTICE.md`,
`SECURITY.md`.

> **До factory reset:** подготовь Mac/Quasar и переключи DNS
> `quasar.yandex.net` на Mac. Если сброшенная колонка сначала увидит настоящий
> Яндекс, она может уйти в OTA/закрыть ADB. Не включай её без готового
> stage 1. Проверь срок сертификатов (`notBefore/notAfter`); просроченный
> leaf = новый/обновлённый kit.

### 4.2. Stage 1 — вернуть ADB

1. Развернуть прокси на Mac:
   ```bash
   cd archive/legacy/alice-mitm-bootstrap
   cp .env.example .env
   # stage 1: INTERCEPT_HOSTS=quasar.yandex.net,
   # MITM_CERT=./certs/quasar.yandex.net.crt, MITM_KEY=...key,
   # BOOTSTRAP_ENABLED=true
   bun install
   bun run start
   ```
   Env можно взять из `projects/station/configs/alice-mitm.stage1.env` (значения IP/портов;
   секреты — в `secrets/legacy-alice-mitm/`).
2. DNS на роутере/AX3: `quasar.yandex.net → 192.0.2.11`; AAAA — NODATA.
3. Убедиться, что сервер отдаёт **цепочку leaf + intermediate**:
   ```bash
   openssl s_client -connect 192.0.2.11:443 \
     -servername quasar.yandex.net -showcerts </dev/null
   openssl verify -CAfile /path/to/station-ca-bundle.crt \
     -untrusted certs/em-dv-tls-ca-g3b.pem certs/quasar.yandex.net.crt
   ```
4. Перезагрузить/передёрнуть питание колонки; она запросит
   `/get_sync_info`; проверить `"adbd":{"remoteActivation":true,"network":true}`.
5. Дождаться ADB:
   ```bash
   ./scripts/wait-for-adb.sh
   # или
   adb connect 192.0.2.13:5555
   adb devices
   ```

Если TLS `unknown_ca` — сервер отдал только leaf; нужна цепочка. Если
`bad_certificate` — SAN/ключ/порядок. Если TLS ок, но нет sync — DNS/SNI/hosts.

### 4.3. Stage 2 — local CA, uniproxy, fake Quasar

1. Установить local CA (после ADB):
   ```bash
   ADB_TARGET=192.0.2.13:5555 ./scripts/install-ca.sh certs/ca.crt
   ```
   Скрипт добавляет CA в `/system/vendor/quasar/certs/ca-certificates.crt`,
   remount-ro, reboot.
2. Переключить `.env` на `quasar-uniproxy.crt/key`, добавить
   `uniproxy.alice.yandex.net → 192.0.2.11`; перезапустить proxy.
3. Full fake backend (если нужен стоковый Quasar/voice, а не Pulsar):
   ```bash
   cd archive/legacy/v3rt3p-stack
   # repos/ уже содержит: quasar-server, https-proxy, processor,
   # quasistreaming-gigam, supertonic-tts
   # .env в архиве нет: значения — secrets/legacy-v3rt3p/.env (600);
   # скопируй в .env, не публикуй. Старый Postgres data не восстанавливать.
   docker compose build
   docker compose up -d
   ```
   В compose уже: `ALLOW_UPDATES=false`, `https-proxy` 403 для s3/cdn,
   204 для телеметрии, `processor`/Postgres/TTS/STT.
   **Gap:** модели GigaAM/Supertonic/ассеты не входят в git-репозитории; их
   надо восстановить из `resources/`/бэкапа или upstream. Для возврата ADB
   stage 1 v3rt3p не обязателен.
4. Переключение bootstrap ↔ stack: `switch-to-stack.sh` / `switch-back.sh`
   (ожидают `MITM_DIR`; проверь пути).

### 4.4. После ADB — вернуть станцию в рабочее состояние

Порядок (подробные команды — `REPLICATION-GUIDE.md`, `OTA-FORTRESS.md`,
`PROVISIONING.md`):

1. **SSH/dropbear**: `artifacts/ssh/dropbear*`, пароль — `secrets/CREDENTIALS.md`.
2. **OTA/ADB fortress**: `projects/station/configs/station-applied/` — `activate_adb.sh`,
   `device_ota_update.sh`, `ota_script.pub`, `launch.json`, `onboot.sh`,
   `rootkeeper.sh`; remount rw/ro, бэкап оригиналов в `/data/opt/backup`.
3. **Egress/quarantine**: `projects/station/scripts/station-egress-guard.sh`,
   `projects/station/scripts/station-yandex-block.sh`; проверить `EGRESS_GUARD`,
   `updatesd` отсутствует, `launch.json` без telemetry.
4. **Pulsar**: `projects/station/upstream/pulsar/` + `pulsar-install --no-enable`, затем
   `ctl.sh enable`; HA ESPHome integration, Assist pipeline.
5. **BLE gateway**: `projects/station/ble-gateway/` (Go armhf), service
   `Station-Gateway`, token в `/data/quasar/ble-gateway/token`; 1/3/4/5.
6. **Проверка**: ADB/SSH reboot, OTA no-op, egress, Pulsar HA entities,
   Assist/TTS, BLE GATT.

### 4.5. После восстановления

- Вернуть DNS на роутере с Mac на `192.0.2.1` (или как было), остановить
  Quasar DNS/proxy; иначе Mac — единая точка отказа DNS.
- Не оставлять `alice-mitm`/`v3rt3p` в автозапуске.
- Записать новый BLE token/SSH host key; обновить `secrets/`.
- Обновить `STATE.md`/`HISTORY.md`.

---

## 5. Сценарий C — потерян Mac/Quasar

Нужен только для factory reset/root станции; HA/станция работают без него.

1. macOS + Bun + Docker/Colima.
2. Восстановить `archive/legacy/alice-mitm-bootstrap/` (полный source) и
   `secrets/legacy-alice-mitm/.env*`, `secrets/certs/`.
3. Восстановить `archive/legacy/v3rt3p-stack/` (compose/prompts/switch +
   `repos/` уже в архиве) и `secrets/legacy-v3rt3p/.env`.
4. Static IP Mac `192.0.2.11` (DHCP reservation), DNS/прокси 53/80/443.
5. Проверить stage 1 `get_sync_info`, затем stage 2/CA.
6. Root kit: `projects/station/tools/quasar-root-kit/`, `artifacts/ssh/`.

**Gap:** модели/ассеты v3rt3p и локальный CA private key могут быть неполными;
проверь `https-proxy/certs/ca.key` и `secrets/certs/` до factory reset.
Если key утерян — сгенерировать новый local CA и заново установить на станцию
(только при наличии ADB; иначе stage 1 bootstrap не зависит от local CA).

---

## 6. Сценарий D — полная потеря

1. Расшифровать `secrets/` и бэкапы.
2. Восстановить Mac/Quasar kit (если нужен root станции).
3. Восстановить неттоп (§3).
4. Восстановить станцию (§4) или, если она жива, проверить ADB/SSH/Pulsar.
5. Проверить HA/MA/STT/TTS/Tailscale/BLE.
6. Сделать новый backup и offsite-копию.

---

## 7. Что бэкапить и где

| Что | Как часто | Куда | Секретность |
|---|---|---|---|
| `homeassistant/config` (вкл. `secrets.yaml`, `.storage/auth`) | перед изменениями | `ctl.sh backup` + offsite | высокая |
| MA data / Whisper/Piper | перед обновлением | backup | средняя |
| `secrets/` (SSH/ADB/сертификаты/DPP) | при изменении | шифрованный внешний диск | высокая |
| Station `/data`/оригиналы | после изменений | `backups/station-*` | высокая |
| Quasar kit/configs | при изменении | repo + archive | высокая (certs) |
| Tailscale ACL | при изменении | `backups/tailscale-acl-*` | низкая |
| Router/ONT config | при изменении | экспорт/скриншоты | средняя |
| Mac Quasar env | при изменении | `secrets/legacy-*` | высокая |

Свежий полный backup неттопа: `backups/maintenance-20261027/` (1.5 ГБ,
pre-update); предыдущий — `backups/remote-access-20261026/` (1.4 ГБ).
Это **не offsite**: нужен шифрованный внешний диск/облако.

---

## 8. Проверка после восстановления

- [ ] `./ctl.sh status`; HA/MA 200; Whisper/Piper healthy.
- [ ] HA UI + TOTP; MA player; Pulsar ESPHome online.
- [ ] Assist text/voice; TTS; alarm/timer; `PLAN-SCHEDULES.md` (local_todo, 8 timer helpers, dashboard).
- [ ] Tailscale `verify-remote-access.sh` 0/0; remote с cellular.
- [ ] ADB/SSH станции; OTA fortress; egress guard; BLE 1/3/4/5.
- [ ] `make test`; `make test-e2e` (с токенами).
- [ ] Новый backup + offsite; `secrets/` не в публичном месте.

---

## 9. Чего в проекте нет / известные gap’ы

- **Нет проверенного factory-образа Midi `cucumber`**; `artifacts/firmware/`
  — Station 2 (`yandexmidi`), **не прошивать в Midi**.
- **Нет raw bootloader/recovery/boot/dtb**; factory reset не равен unbrick;
  серверный lock/`stolen` может остаться.
- Zigbee-сеть в модуле станции может пережить factory reset или нет —
  неизвестно; после reset проверить `:6638`/ZHA/Z2M, при необходимости
  пересоздать сеть (устройства могут потребовать re-pair).
- v3rt3p **source** архивирован; модели/ассеты/Postgres data — нет.
- `alice-mitm-bootstrap`/v3rt3p `node_modules`/Docker images не архивированы;
  `bun install`/`docker compose build` требуют сеть. Для ADB stage 1 это
  допустимо; для полного offline — заранее собрать образы.
- Локальный CA private key: в архиве upstream есть
  `repos/https-proxy/certs/ca.key`; проверить fingerprint с `ca.crt` и
  станцией. Если не тот — сгенерировать новый local CA и заново установить
  (только при наличии ADB); stage 1 bootstrap от local CA не зависит.
- Offsite backup пока не настроен; текущая копия на Mac — не offsite.
- ONT WAN `22/443` — отдельная security-задача.

---

## 10. Ссылки

- `STATE.md`, `ARCHITECTURE.md`, `OPERATIONS.md`, `SECURITY.md`,
  `DECISIONS.md`, `SENSITIVE-DATA.md`.
- `OTA-FORTRESS.md`, `PROVISIONING.md`, `REPLICATION-GUIDE.md`.
- `projects/home-assistant/SETUP.md`, `projects/home-assistant/HA.md`, `projects/home-assistant/stack/README.md`,
  `projects/home-assistant/hermes/README.md`.
- `archive/legacy/alice-mitm-bootstrap/README.md`,
  `archive/legacy/v3rt3p-stack/ARCHIVE.md`/`UPSTREAM.md`.
- `projects/station/configs/README.md`, `artifacts/README.md`, `backups/README.md`.

## Свой wake word после восстановления Pulsar

После штатного восстановления доступа и базового Pulsar: модель и manifest
брать из `artifacts/wakeword/ricardo-v1/`, совместимый бинарник и source archive —
из `artifacts/wakeword/ricardo-station/`. Сверить SHA-256; копии исходного
бинарника на станции перед заменой обязательны. Полная карта файлов и
сборка — `WAKE-WORD.md`, команды отката — `OPERATIONS.md`.

## Приватная часть локального стенда

После аудита 2026-10-27 env/ключи, backups/logs/archive/snapshots и локальные
artifacts физически находятся под `secrets/local/`. Старые пути — игнорируемые
ссылки. Чистый Git checkout их не содержит: восстановление требует отдельно
сохранённой приватной копии. Реестр — `SENSITIVE-DATA.md`. Пользовательский
`www/alarm_ringtone.wav` также приватный: перед HA deploy предоставить свой
аудиофайл или восстановить локальную копию.
