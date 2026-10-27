# Операции

## Статус

```bash
# колонка
adb connect 192.0.2.13:5555
adb devices
adb -s 192.0.2.13:5555 shell 'id; cat /system/vendor/quasar/version_full; uptime'
ssh root@192.0.2.13          # пароль — см. secrets/CREDENTIALS.md

# Mac / DNS
cd ~/Desktop/alice-mitm-bootstrap && ./scripts/proxy-ctl.sh status
lsof -nP -iUDP:53 -iTCP:80 -iTCP:443 -iTCP:22 -iTCP:5555

# стек
cd ~/Desktop/v3rt3p-stack && docker compose ps
curl -s http://127.0.0.1:31116/devices
```

## SSH

```bash
ssh root@192.0.2.13
# пароль — см. secrets/CREDENTIALS.md
```

Первый раз подтвердить host key. Host key:
`/etc/dropbear/dropbear_ed25519_host_key`,
fingerprint `SHA256:fh/84futNAiAzSJL/JcinbBC99obg6t+VxEhIjjhL44`.

Управление:

```bash
/etc/init.d/S99dropbear start|stop|restart
```

## ADB

```bash
adb connect 192.0.2.13:5555
adb -s 192.0.2.13:5555 shell
```

ADB без авторизации, root по умолчанию. Не выставлять `5555` наружу.

## API 31116 (открыто)

`quasar-server` публикует API на `0.0.0.0:31116` **без авторизации**:

- `POST /devices/:duid/push-directive` — произвольная директива;
- `PATCH /devices/:duid` — `quasarConfig` + `pushUpdateConfig`.

Это локальный push-канал; любой в LAN может отправить станции директиву.
Предложенное исправление (требует согласования): в `docker-compose.yml`
`ports: "127.0.0.1:31116:31116"` или убрать публикацию, затем
`docker compose up -d quasar-server`. Станция/прокси используют внутренний
`quasar-server:31115`, так что это не сломает Алису.

## OTA/fortress

```bash
~/Desktop/yandex-station-lab/scripts/station-status.sh

# откат защиты (вернуть штатные OTA/ADB-файлы, reboot)
~/Desktop/yandex-station-lab/scripts/restore-ota-fortress.sh
```

Что стоит: обёртка `activate_adb.sh` (deactivate → no-op), keeper
`/data/quasar/rootkeeper.sh`, no-op `device_ota_update.sh`, dummy
`ota_script.pub`, `updatesd` вырезан из `launch.json`.
Подробности — `docs/OTA-FORTRESS.md`.

## Яндекс-карантин (текущий режим)

- DNS на Mac: `DNS_NAMES` = полный список Яндекса → `192.0.2.11`.
- На колонке: `unbound` только на Mac, `/etc/hosts` с блоком, chrony на Google.
- `iptables`/`ip6tables`: `INPUT DROP` и `OUTPUT REJECT` для Яндексовых сетей.
- Локальный трафик идёт в `https-proxy` (`/uni.ws`, `/gproxy/...`); WAN
  ограничен egress-guard (LAN/DHCP/Google NTP).
- Переживает ребут: `onboot.sh` вызывает `/data/quasar/yandex-block.sh` до
  Quasar, keeper — раз в 60 с.
- Яндексовые сервисы (Музыка, Лента, облако) не работают — это цена карантина.
- Root/ADB/SSH/OTA-запрет живут на колонке и от DNS не зависят.

### Если нужен только карантин колонки

Верни Static DNS роутера на `192.0.2.1`: колонка всё равно ходит в Mac
напрямую через свой `unbound`. Так остальные устройства не пострадают.
Если оставить DNS роутера на Mac — карантин будет для всей LAN, а при сне/
выключении Mac сломается DNS у всех.

### Файлы

- `scripts/station-yandex-block.sh` — применяет DNS/hosts/chrony/iptables.
- `configs/station-applied/yandex-block.sh` — копия с колонки.
- `configs/station-applied/{rootkeeper.sh,onboot.sh,unbound.conf,hosts,chrony.conf}`.
- Лог на колонке: `/data/quasar/yandex-block.log`.

## OTA / телеметрия (закрыто 23.10)

- `ALLOW_UPDATES=false` в `docker-compose.yml`; `quasar-server` отдаёт
  `/check_updates` → `{"hasUpdate":false}`.
- `https-proxy`: `*.s3.yandex.net` и `*.svc.cdn.yandex.net` → `403`
  (в `generate.sh`, не только в сгенерированном конфиге).
- Телеметрия: task `telemetry` удалён из `launch.json`; `clck.yandex.net`,
  `report.appmetrica.yandex.net`, `startup.mobile.yandex.net` и
  `/quasar_metrics/` → `204`.
- `/data/quasar/launch.json.notel` — эталон без telemetry; `yandex-block.sh`
  возвращает его в rootfs, если тот откатился.
- Проверено после ребута: `maind --telemetry` отсутствует, в proxy-логах нет
  `report/clck/quasar_metrics`, ADB/SSH/OTA живы.
- Бэкап старых файлов: `backups/pre-telemetry-20261023-2223/`.

## Переключение режимов

```bash
cd ~/Desktop/v3rt3p-stack

# на кастомный стек (все домены Яндекса → Mac, nginx 80/443)
./switch-to-stack.sh

# legacy: bootstrap quasar-only; НЕ снимает карантин/iptables
./switch-back.sh
```

Скрипты меняют `DNS_NAMES` в `~/Desktop/alice-mitm-bootstrap/.env` и
перезапускают DNS. Осторожно: `switch-to-stack`/`switch-back` могут убить
DNS-процесс при выходе — после них проверь `:53` и при необходимости
запусти DNS вручную через `nohup` (как в `docs/CONTEXT.md`).

## DNS / Bun-прокси

```bash
cd ~/Desktop/alice-mitm-bootstrap
./scripts/proxy-ctl.sh status
./scripts/proxy-ctl.sh start|stop|restart
./scripts/proxy-ctl.sh proxy-start|proxy-stop
./scripts/proxy-ctl.sh dns-start|dns-stop
./scripts/proxy-ctl.sh stage1|stage2
```

Логи: `logs/proxy.log`, `logs/dns.log`, `logs/requests.jsonl`.
Конфиг: `.env` (активный), `.env.stage1`, `.env.stage2`.

### Egress-guard (station-side)

С 24.10.2026 на станции стоит `EGRESS_GUARD`: разрешены loopback,
private LAN (`10/8`, `172.16/12`, `192.168/16`, `100.64/10`),
link-local/multicast, DHCP и Google NTP
(`216.239.35.0/24:123`); **весь остальной IPv4/IPv6 — DROP**. Это не зависит
от DNS, роутера или Mac.

```bash
# статус
adb -s 192.0.2.13:5555 shell 'iptables -S EGRESS_GUARD; iptables -L EGRESS_GUARD -v -n'

# применить/снять (снимает только guard, Yandex-блок и OTA остаются)
adb -s 192.0.2.13:5555 shell /data/quasar/egress-guard.sh
adb -s 192.0.2.13:5555 shell /data/quasar/egress-guard-off.sh

# проверить, что Яндекс режется, а LAN/ADB живы
adb -s 192.0.2.13:5555 shell 'ping -c 1 -W 2 77.88.8.8; ping -c 1 -W 2 192.0.2.11'
```

- **Если выключить Mac DNS/прокси**: локальной Алисы и DNS-имён не будет, но
  egress-guard продолжит блокировать Яндекс. Доступ к станции — по IP
  `192.0.2.13` из LAN.
- **Если выключить DNS на роутере**: то же; для ADB/SSH DNS не нужен.
- **Если выключить Mac полностью**: доступ с телефона/другого LAN-устройства;
  если Mac — единственный админ, включи его снова. Сам guard на станции жив.
- Единственные WAN-исключения: Google NTP. Чтобы убрать, удалить правило
  `216.239.35.0/24:123` или пересобрать guard с `ALLOW_NTP=0`.
  LAN `192.0.2.0/24` считается доверенной.
- Проверено: reboot, новое ADB/SSH, LAN API, блокировка Яндекса и `1.1.1.1`,
  Google NTP. Порядок: `onboot.sh` после `yio_iptables_init.sh`, затем keeper.

## DPP / Wi-Fi Easy Connect

QR и ключ: `secrets/dpp/` (`qr.png`, `uri.txt`, `bootstrap.der`).
На станции: `/data/quasar/dpp-bootstrap.hex`, `/data/quasar/dpp-provision.sh`;
`dpp_config_processing=2` в `/bkupgrade/data/wpa_supplicant.conf`.

```bash
# статус/bootstrap
adb -s 192.0.2.13:5555 shell \
  'wpa_cli -i wlan0 get dpp_config_processing; wpa_cli -i wlan0 dpp_bootstrap_get_uri 1'

# принудительно слушать QR (для теста, даже при текущей сети)
adb -s 192.0.2.13:5555 shell 'touch /data/quasar/dpp-force-listen'
# снять
adb -s 192.0.2.13:5555 shell 'rm -f /data/quasar/dpp-force-listen'
```

Штатно `dpp_listen 2412` включается только когда нет default route.

**Важно:** DPP на `wlan0` при активном Wi-Fi не работает: драйвер даёт
`vm_cfg80211_remain_on_channel ... vm_p2p_support==0`. `p2p0` off-channel
работает, но роняет `wlan0`; **не запускать p2p0 при рабочей сети.** DPP
тестировать только unassociated, после USB/fallback.
 После
успешного DPP сеть сохраняется и станция подключается. QR стабилен, пока
хранится тот же DER-ключ. Android 10+ обычно поддерживает; iPhone — не
гарантирован.

## Sound pairing

```bash
python3 scripts/station-pairing-sound.py \
  --ssid 'MyWiFi' --password 'secret' --format wav --out /tmp/pair.wav
```

Локально, без Telegram/облака. Проиграть рядом с колонкой **в режиме
сопряжения**; случайный токен подходит только для Wi-Fi, не для привязки
аккаунта. Пароль звучит в эфире.

## Стек Docker

```bash
# Colima (после ребута Mac)
colima start

cd ~/Desktop/v3rt3p-stack
docker compose ps
docker compose up -d                 # всё, включая https-proxy
docker compose up -d postgres quasar-server processor quasistreaming-gigam supertonic-tts
docker compose stop https-proxy      # освободить 80/443
docker compose logs -f processor
docker compose logs -f https-proxy
docker compose build
```

Файлы: `docker-compose.yml`, `.env` (LLM/TTS), `repos/` (исходники),
`quasistreaming-models/`, `supertonic-tts/assets/`.

## После перезагрузки Mac

1. `colima start` (если Docker не поднялся).
2. `cd ~/Desktop/v3rt3p-stack && docker compose up -d`.
3. DNS: запустить **вручную через nohup** (см. `docs/CONTEXT.md`), потому что
   `proxy-ctl.sh dns-start`/`switch-to-stack` могут убить процесс при выходе.
4. Проверить `:53`, `:443`, `:31116` (API без auth), `adb connect`.
5. Если роутер смотрит на Mac, без DNS интернет-имена не резолвятся;
   egress-guard при этом продолжает блокировать Яндекс, а ADB/SSH работают по IP.

## Роутер

Static DNS сейчас указывает на `192.0.2.11`. Если вернуть `192.0.2.1`
или вообще выключить DNS на роутере, это затронет **остальные устройства**, но
**не** вернёт колонку в настоящий Яндекс: egress-guard режет WAN, `iptables`
режет Яндекс, а `unbound` смотрит на Mac. Чтобы вернуть реальный Яндекс, нужно
отдельно снять `egress-guard-off.sh`, затем `yandex-block`/iptables/hosts/unbound —
это осознанный откат, не просто смена DNS на роутере.

## Логи и диагностика

```bash
# последние запросы колонки к стеку
cd ~/Desktop/v3rt3p-stack && docker compose logs --tail=100 https-proxy

# DNS-запросы
tail -50 ~/Desktop/alice-mitm-bootstrap/logs/dns.log

# ADB/SSH
adb -s 192.0.2.13:5555 shell 'logread | tail -50'

# pcap (если понадобится снова)
sudo tcpdump -ni en0 -s0 -w ~/Desktop/station.pcap 'host 192.0.2.13 and tcp port 443'
```

## Восстановление / откат

- **Настоящая Алиса:** `switch-back.sh` переключает DNS на bootstrap/quasar
  и поднимает Bun-прокси, но **не снимает** наш карантин/iptables. Для
  реального Яндекса нужен отдельный откат `yandex-block.sh`.
- **Прошивка:** factory reset/recovery на Midi **не проверен**; снесёт
  `/data`, root/SSH/OTA/telemetry. Не использовать как unbrick без
  raw-дампа recovery/aml_sysrecovery.
- **OTA-запрет/fortress:** см. `docs/PENDING.md` и `tools/quasar-root-kit`.
- **SSH:** при factory reset `/etc` снесётся; ставить заново.
- **ADB:** root ADB поднимается keeper'ом из маркеров `/data/quasar/activate_adb_*`; после factory reset — заново.
