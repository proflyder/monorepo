# REPLICATION GUIDE — Yandex Station Midi: карантин, телеметрия, OTA-блок

**Дата:** 2026-10-24  
**Модель:** Yandex Station Midi, `cucumber`, `STATION_SERIAL`  
**Прошивка:** `0.369.1.46.4778262955.20261013.160`  
**Роль:** воспроизводит **именно нашу** текущую конфигурацию, а не универсальную «панель».

> ⚠️ Это только для **своей** колонки и только для **Midi `cucumber`** на этой прошивке.
> На Station 1 / Lite / других моделях другие пути, другие ключи, другой OTA. Не запускай
> эти файлы вслепую на другой модели.
>
> ⚠️ Это не аппаратная защита. Recovery/factory можно перезаписать вручную или будущим
> OTA-пакетом. OTA-блок в рабочей системе — не immutable boot.
>
> ⚠️ В гайде нет паролей, приватных ключей и токенов. Они лежат в
> `~/Desktop/yandex-station-lab/secrets/` и в бэкапах; не публикуй их.

## 0. Переменные и файлы

```bash
export STATION_IP=192.0.2.13
export MAC_IP=192.0.2.11
export LAB="$HOME/Desktop/yandex-station-lab"
export STACK="$HOME/Desktop/v3rt3p-stack"
export MITM="$HOME/Desktop/alice-mitm-bootstrap"
```

Точные применённые файлы:

- `$LAB/scripts/station-yandex-block.sh` — вся станционная защита;
- `$LAB/scripts/station-egress-guard.sh` — station-side allowlist WAN;
- `$LAB/scripts/station-egress-guard-off.sh` — снять только гвард;
- `$LAB/scripts/station-egress-guard-watchdog.sh` — страховка первого запуска;
- `$LAB/scripts/station-dpp-provision.sh` — DPP bootstrap/listen;
- `$LAB/scripts/station-pairing-sound.py` — локальный WAV/OGG sound-код;
- `$LAB/configs/station-applied/launch.json.notel` — `launch.json` без `telemetry`;
- `$LAB/configs/station-applied/onboot.sh` — onboot с вызовом блока и гварда;
- `$LAB/configs/station-applied/rootkeeper.sh` — keeper с вызовом блока и гварда;
- `$LAB/configs/station-applied/{activate_adb.sh,device_ota_update.sh,ota_script.pub,launch.json}` — OTA/ADB fortress;
- `$LAB/configs/stack/docker-compose.yml` — `ALLOW_UPDATES=false`;
- `$LAB/configs/stack/https-proxy-generate.sh` — `403` для s3/cdn и `204` для телеметрии;
- `$LAB/backups/emergency-20261023-2137/` — полный бэкап станции;
- `$LAB/backups/pre-telemetry-20261023-2223/` — старые compose/generate/yandex.conf;
- `$LAB/backups/egress-20261024-050346/` — iptables-save и старые onboot/keeper.

## 1. Pre-flight

```bash
adb connect "$STATION_IP:5555"
adb -s "$STATION_IP:5555" shell 'id; cat /proc/device-tree/model; cat /system/vendor/quasar/version_full'
```

Ожидаемо:

```text
uid=0(root) ...
Yandex Cucumber DVT2
0.369.1.46.4778262955.20261013.160
```

Если модель не `Cucumber` или версия другая — **остановись**. Не применяй наши
`launch.json`/`onboot.sh`/`quasar.cfg`-правки к другой модели.

Сделай бэкап **до** любых изменений:

```bash
adb -s "$STATION_IP:5555" shell '
  mkdir -p /data/opt/backup
  cp /system/vendor/quasar/launch.json /data/opt/backup/launch.json.before-replication
  cp /system/vendor/quasar/onboot.sh /data/opt/backup/onboot.sh.before-replication
  cp /data/quasar/rootkeeper.sh /data/opt/backup/rootkeeper.sh.before-replication 2>/dev/null || true
  cp /etc/hosts /data/opt/backup/hosts.before-replication
  cp /etc/unbound/unbound.conf /data/opt/backup/unbound.conf.before-replication
  cp /etc/chrony.conf /data/opt/backup/chrony.conf.before-replication
  cp /system/vendor/quasar/ota_script.pub /data/opt/backup/ota_script.pub.before-replication
  cp /system/vendor/quasar/device_ota_update.sh /data/opt/backup/device_ota_update.sh.before-replication
  cp /system/vendor/quasar/activate_adb.sh /data/opt/backup/activate_adb.sh.before-replication
'
adb -s "$STATION_IP:5555" pull /data/opt/backup "$LAB/backups/replication-originals"
```

## 2. OTA/ADB fortress (как было сделано ранее)

Файлы берутся **готовые** из `$LAB/configs/station-applied/`; не переписывай их вручную.

```bash
cd "$LAB"

# оригиналы уже в /data/opt/backup; дополнительно кладём копию
adb -s "$STATION_IP:5555" shell 'cp /system/vendor/quasar/ota_script.pub /data/opt/backup/ota_script.pub.orig 2>/dev/null || true'

# wrapper + no-op OTA + dummy key + launch без updatesd
# если remount вернул ошибку — остановись и не продолжай
adb -s "$STATION_IP:5555" shell 'mount -o remount,rw /'
adb -s "$STATION_IP:5555" push configs/station-applied/activate_adb.sh /system/vendor/quasar/activate_adb.sh
adb -s "$STATION_IP:5555" push configs/station-applied/device_ota_update.sh /system/vendor/quasar/device_ota_update.sh
adb -s "$STATION_IP:5555" push configs/station-applied/ota_script.pub /system/vendor/quasar/ota_script.pub
adb -s "$STATION_IP:5555" push configs/station-applied/launch.json /system/vendor/quasar/launch.json
adb -s "$STATION_IP:5555" shell 'chmod 755 /system/vendor/quasar/activate_adb.sh /system/vendor/quasar/device_ota_update.sh; chmod 644 /system/vendor/quasar/ota_script.pub /system/vendor/quasar/launch.json; sync; mount -o remount,ro /'

# keeper + onboot
adb -s "$STATION_IP:5555" push configs/station-applied/rootkeeper.sh /data/quasar/rootkeeper.sh
adb -s "$STATION_IP:5555" shell 'chmod 755 /data/quasar/rootkeeper.sh'
adb -s "$STATION_IP:5555" shell 'mount -o remount,rw /'
adb -s "$STATION_IP:5555" push configs/station-applied/onboot.sh /system/vendor/quasar/onboot.sh
adb -s "$STATION_IP:5555" shell 'chmod 755 /system/vendor/quasar/onboot.sh; sync; mount -o remount,ro /'
```

`launch.json` в `configs/station-applied/` уже **без** `telemetry` и **без** `updatesd`.
`onboot.sh` вызывает `/data/quasar/yandex-block.sh` до старта Quasar.
`rootkeeper.sh` вызывает его каждые 60 секунд и держит ADB.

Проверка:

```bash
adb -s "$STATION_IP:5555" shell 'grep -c updatesd /system/vendor/quasar/launch.json; grep -c telemetry /system/vendor/quasar/launch.json; ps | grep updatesd || echo "updatesd: none"'
```

## 3. Локальный стек и DNS на Mac

```bash
cd "$STACK"
grep -n 'ALLOW_UPDATES' docker-compose.yml
grep -n 'return 403\|return 204\|quasar_metrics' repos/https-proxy/generate.sh | head
```

Должно быть:

```yaml
ALLOW_UPDATES: "false"
```

и в `generate.sh`:

```sh
*.s3.yandex.net|*.svc.cdn.yandex.net) proxy_pass_line='return 403;'
clck.yandex.net|report.appmetrica.yandex.net|startup.mobile.yandex.net) proxy_pass_line='return 204;'
quasar.yandex.net|quasar.yandex.ru) extra_location='location /quasar_metrics/ { return 204; }'
```

Пересборка/запуск:

```bash
cd "$STACK"
docker compose build https-proxy
docker compose up -d https-proxy
docker compose up -d quasar-server
```

Проверка:

```bash
curl -k -s -o /dev/null -w '%{http_code}\n' --resolve quasar.s3.yandex.net:443:127.0.0.1 https://quasar.s3.yandex.net/
# 403
curl -k -s -o /dev/null -w '%{http_code}\n' --resolve clck.yandex.net:443:127.0.0.1 https://clck.yandex.net/
# 204
curl -k -s -o /dev/null -w '%{http_code}\n' --resolve quasar.yandex.net:443:127.0.0.1 -X POST -d '{}' https://quasar.yandex.net/quasar_metrics/write/batch
# 204
curl -k -s --resolve quasar.yandex.net:443:127.0.0.1 \
  'https://quasar.yandex.net/check_updates?device_id=STATION_SERIAL&version=0.369.1.46.4778262955.20261013.160&platform=cucumber'
# {"hasUpdate":false}
```

DNS на Mac:

```bash
cd "$MITM"
grep '^DNS_NAMES=' .env
```

Список — полный, как в нашей `.env` (35 имён + корневые).
В `.env` он должен быть **одной строкой** без переносов:

```text
quasar.yandex.net,quasar.yandex.ru,iot.quasar.yandex.net,scbh.yandex.net,
uniproxy.alice.yandex.net,uniproxy-quic.alice.yandex.net,rpc.alice.yandex.net,
alice.yandex.ru,alice.yandex.net,alice-ats.yandex.net,alice-ats.yandex.ru,
music.yandex.ru,music.yandex.net,api.music.yandex.net,ws.api.music.yandex.net,
passport.yandex.ru,passport.yandex.net,oauth.yandex.ru,login.yandex.ru,
mobile.yandex.net,storage.mds.yandex.net,avatars.mds.yandex.net,
strm.yandex.ru,strm.yandex.net,push.yandex.ru,clck.yandex.net,
report.appmetrica.yandex.net,startup.mobile.yandex.net,
mobileproxy.passport.yandex.net,yandex.ru,yandex.net,yandex.com,ya.ru,
yango.com,funtechservices.com
```

Запуск DNS (ровно как у нас — **без** `switch-to-stack`, он убивает процесс при выходе):

```bash
cd "$MITM"
set -e
OLD=$(cat logs/dns.pid 2>/dev/null || true)
[ -n "$OLD" ] && kill "$OLD" 2>/dev/null || true
sleep 1
rm -f logs/dns.pid
nohup .tools/bun-darwin-aarch64/bun run src/dns.ts >> logs/dns.log 2>&1 </dev/null &
echo $! > logs/dns.pid
disown || true
sleep 1
ps -p "$(cat logs/dns.pid)" -o pid,command
```

## 4. Станция: DNS/hosts/chrony/iptables

Всё это делает `$LAB/scripts/station-yandex-block.sh`. Скопируй его как есть:

```bash
adb -s "$STATION_IP:5555" push "$LAB/scripts/station-yandex-block.sh" /data/quasar/yandex-block.sh
adb -s "$STATION_IP:5555" shell 'chmod 755 /data/quasar/yandex-block.sh; /data/quasar/yandex-block.sh; echo rc=$?'
```

Что он делает (точные значения):

- **unbound** → единственный forwarder `192.0.2.11`:

```text
server:
	msg-cache-size: 2m
	rrset-cache-size: 2m
	username: "nobody"
forward-zone:
	name: "."
	forward-addr: "192.0.2.11"
```

- **`/etc/hosts`** → блок `YANDEX-BLOCK` со всеми основными именами на `192.0.2.11`;
- **chrony** → Google NTP `216.239.35.0/4/8/12`, без `ntp.yantp.net`;
- **iptables** → `INPUT DROP` + `OUTPUT REJECT` для:

```text
5.255.0.0/16
37.9.0.0/16
77.88.0.0/16
87.250.0.0/16
93.158.0.0/16
95.108.0.0/16
213.180.0.0/16
178.248.232.0/21
185.32.187.0/24
141.8.128.0/18
```

- **IPv6** → `2a02:6b8::/32` (`DROP`/`REJECT`);
- **`launch.json`** → если rootfs откатил, копирует `/data/quasar/launch.json.notel`.

Проверка:

```bash
adb -s "$STATION_IP:5555" shell '
  nslookup -type=a quasar.yandex.ru 2>/dev/null | awk "/^Address: /{print \$2}" | tail -1
  nslookup -type=a cp.cloudflare.com 2>/dev/null | awk "/^Address: /{print \$2}" | tail -1
  iptables -S INPUT | grep -c DROP
  iptables -S OUTPUT | grep -c REJECT
  ip6tables -S INPUT | grep -c DROP
  ip6tables -S OUTPUT | grep -c REJECT
  netstat -tn 2>/dev/null | grep -E "213\.180\.|93\.158\.|5\.255\.|77\.88\." | head
  netstat -tn 2>/dev/null | grep "192.0.2.11:443" | head
'
```

Ожидаемо:

```text
quasar.yandex.ru -> 192.0.2.11
cp.cloudflare.com -> реальный IP
INPUT DROP: 10
OUTPUT REJECT: 10
IPv6 INPUT DROP: 1
IPv6 OUTPUT REJECT: 1
реальных Яндекс-соединений: нет
соединения к 192.0.2.11:443: есть
```

## 4a. Egress-guard (главная гарантия)

DNS-блокировку можно обойти сменой резолвера, поэтому ставим allowlist на
`OUTPUT` самой станции. Он не зависит от роутера и Mac.

```bash
for f in station-egress-guard.sh station-egress-guard-off.sh station-egress-guard-watchdog.sh; do
  adb -s "$STATION_IP:5555" push "$LAB/scripts/$f" "/data/quasar/${f#station-}"
done
adb -s "$STATION_IP:5555" shell '
  chmod 755 /data/quasar/egress-guard.sh /data/quasar/egress-guard-off.sh /data/quasar/egress-guard-watchdog.sh
  rm -f /data/quasar/egress-guard.confirmed
  start-stop-daemon -S -b -x /data/quasar/egress-guard-watchdog.sh
  /data/quasar/egress-guard.sh
  iptables -S EGRESS_GUARD
'
```

Разрешено: loopback, private LAN (`10/8`, `172.16/12`, `192.168/16`,
`100.64/10`), `169.254.0.0/16`, multicast/broadcast, DHCP, Google NTP
`216.239.35.0/24:123`. Остальной IPv4/IPv6 — `DROP`. `INPUT` не
ограничивается, поэтому ADB/SSH и LAN-доступ живы.

Проверка **сразу**, пока watchdog ждёт:

```bash
# access must survive
adb -s "$STATION_IP:5555" disconnect; adb connect "$STATION_IP:5555"
adb -s "$STATION_IP:5555" shell 'id; ping -c 1 -W 2 192.0.2.11; wget -q -t 1 -T 2 -O /dev/null http://192.0.2.11:31116/devices; echo lan_api=$?'
# Yandex/non-LAN must fail
adb -s "$STATION_IP:5555" shell 'ping -c 1 -W 2 77.88.8.8; wget -q -t 1 -T 2 -O /dev/null http://1.1.1.1/; echo wan=$?; iptables -L EGRESS_GUARD -v -n | tail -2'
```

Если всё хорошо — подтвердить и закрепить:

```bash
adb -s "$STATION_IP:5555" shell 'touch /data/quasar/egress-guard.confirmed'
# onboot: вызов после yio_iptables_init.sh
adb -s "$STATION_IP:5555" push "$LAB/configs/station-applied/onboot.sh" /data/quasar/onboot.sh.new
adb -s "$STATION_IP:5555" shell '
  sh -n /data/quasar/onboot.sh.new
  mount -o remount,rw /
  cp /data/quasar/onboot.sh.new /system/vendor/quasar/onboot.sh
  chmod 755 /system/vendor/quasar/onboot.sh
  sync
  mount -o remount,ro /
'
# keeper
adb -s "$STATION_IP:5555" push "$LAB/configs/station-applied/rootkeeper.sh" /data/quasar/rootkeeper.sh
adb -s "$STATION_IP:5555" shell 'chmod 755 /data/quasar/rootkeeper.sh'
```

Затем reboot и повторить проверки. Откат только гварда:
`adb shell /data/quasar/egress-guard-off.sh`; Yandex-блок и OTA остаются.
Ограничения: LAN `192.0.2.0/24` считается доверенной; NTP — единственное
WAN-исключение; при выключенном Mac локальной Алисы нет, доступ — по IP.

## 4b. DPP / Wi-Fi Easy Connect + sound

### DPP

Нужен постоянный DER-ключ; QR не меняется между reboot:

```bash
# на Mac, один раз
openssl ecparam -name prime256v1 -genkey -noout -out /tmp/dpp.pem
openssl ec -in /tmp/dpp.pem -outform DER -out /tmp/dpp.der
xxd -p /tmp/dpp.der | tr -d '\n' > /tmp/dpp.hex
# сохранить /tmp/dpp.der и /tmp/dpp.hex в secrets/dpp/, не публиковать
```

На станции:

```bash
adb push /tmp/dpp.hex /data/quasar/dpp-bootstrap.hex
adb shell 'chmod 600 /data/quasar/dpp-bootstrap.hex'
adb push "$LAB/scripts/station-dpp-provision.sh" /data/quasar/dpp-provision.sh
adb shell '
  chmod 755 /data/quasar/dpp-provision.sh
  KEY=$(cat /data/quasar/dpp-bootstrap.hex)
  wpa_cli -i wlan0 set dpp_config_processing 2
  wpa_cli -i wlan0 dpp_bootstrap_remove "*"
  ID=$(wpa_cli -i wlan0 dpp_bootstrap_gen type=qrcode mac=acbac0797774 chan=81/1 key="$KEY" | tail -1)
  wpa_cli -i wlan0 dpp_bootstrap_get_uri "$ID" > /data/quasar/dpp-uri.txt
'
```

`dpp_config_processing=2` добавить в `/bkupgrade/data/wpa_supplicant.conf`.
QR сгенерировать из URI (`qrcode`/`qrencode`) и напечатать. Скрипт
`dpp-provision.sh` держит bootstrap и включает `dpp_listen 2412` только без
default route; для теста — `touch /data/quasar/dpp-force-listen`, снять —
`rm`. Вызов из `onboot.sh`/keeper. Android 10+ обычно; iPhone не гарантирован.

**Ограничение:** DPP на `wlan0` при активном Wi-Fi не работает
(`vm_p2p_support==0`); `p2p0` off-channel работает, но роняет `wlan0`.
DPP тестировать только unassociated, после USB/fallback; p2p0 при рабочей
сети не запускать.

### Sound

```bash
python3 "$LAB/scripts/station-pairing-sound.py" \
  --ssid 'MyWiFi' --password 'secret' --format wav --out /tmp/pair.wav
```

Локально, без Telegram. Играть рядом с колонкой в режиме сопряжения. Случайный
токен — только Wi-Fi, не аккаунт. На живой Midi пока не проверено.

## 5. Телеметрия — вырезать полностью

1. `launch.json` без task `telemetry`:

```bash
adb -s "$STATION_IP:5555" pull /system/vendor/quasar/launch.json /tmp/launch.json
python3 - <<'PY'
import json
p='/tmp/launch.json'
d=json.load(open(p))
for n in d.get('nodes', []):
    n['tasks']=[t for t in n.get('tasks', []) if t.get('name') != 'telemetry']
with open('/tmp/launch.json.notel','w') as f:
    json.dump(d, f, indent=2, ensure_ascii=False); f.write('\n')
print([t['name'] for n in d['nodes'] for t in n['tasks']])
PY
```

Должно быть: `['mediad', 'maind', 'fluent-bitd', 'bulbik']`.

2. Положить эталон в `/data` и rootfs:

```bash
adb -s "$STATION_IP:5555" push /tmp/launch.json.notel /data/quasar/launch.json.notel
adb -s "$STATION_IP:5555" shell 'chmod 644 /data/quasar/launch.json.notel'
adb -s "$STATION_IP:5555" shell 'mount -o remount,rw /'
adb -s "$STATION_IP:5555" push /tmp/launch.json.notel /system/vendor/quasar/launch.json
adb -s "$STATION_IP:5555" shell 'chmod 644 /system/vendor/quasar/launch.json; sync; mount -o remount,ro /'
```

3. Перезапустить Quasar (или ребут):

```bash
adb -s "$STATION_IP:5555" shell 'q restart' || adb -s "$STATION_IP:5555" shell 'reboot'
```

4. Проверка:

```bash
adb -s "$STATION_IP:5555" shell 'ps | grep "[m]aind --telemetry"'    # пусто
adb -s "$STATION_IP:5555" shell 'grep -c telemetry /system/vendor/quasar/launch.json'  # 0
```

`clck.yandex.net`, `report.appmetrica.yandex.net`, `startup.mobile.yandex.net`
и путь `/quasar_metrics/` на прокси отдают `204`.
`clck.yasmina.yango.com` резолвится в Mac и уходит в локальный `quasar-server`,
наружу не идёт (это не `204`, но и не настоящий Яндекс).
`maind --telemetry` больше не запускается.

> `fluent-bitd` остаётся — это локальный логгер, не телеметрия. Не удаляй его
> «за компанию», если не понимаешь, зачем он нужен.

## 6. Финальная проверка после ребута

```bash
adb -s "$STATION_IP:5555" shell 'sync; reboot'
sleep 85

adb -s "$STATION_IP:5555" shell '
  echo -n "ssh/adb: "; pidof adbd
  echo -n "keeper: "; ps | grep rootkeeper | grep -v grep | awk "{print \$1}"
  echo -n "telemetry: "; ps | grep "[m]aind --telemetry" || echo none
  echo -n "updatesd: "; ps | grep updatesd | grep -v grep || echo none
  echo -n "version: "; cat /system/vendor/quasar/version_full
  echo -n "dns: "; nslookup -type=a quasar.yandex.ru 2>/dev/null | awk "/^Address: /{print \$2}" | tail -1
  echo -n "iptables INPUT DROP: "; iptables -S INPUT | grep -c DROP
  echo -n "iptables OUTPUT REJECT: "; iptables -S OUTPUT | grep -c REJECT
  echo -n "egress guard: "; iptables -C OUTPUT -j EGRESS_GUARD 2>/dev/null && echo ON || echo OFF
  echo -n "egress drop: "; iptables -L EGRESS_GUARD -v -n 2>/dev/null | awk "/DROP/ {print \$1}"
  echo -n "dpp config: "; wpa_cli -i wlan0 get dpp_config_processing 2>/dev/null | tail -1
  echo -n "dpp bootstrap: "; wpa_cli -i wlan0 dpp_bootstrap_get_uri 1 >/dev/null 2>&1 && echo ON || echo OFF
  echo "real conns:"; netstat -tn 2>/dev/null | grep -E "213\.180\.|93\.158\.|5\.255\.|77\.88\." | head
'
```

Ожидаемо: ADB/SSH открыты, keeper жив, telemetry нет, updatesd нет, DNS → Mac
(если Mac включён), iptables 10/10, `egress guard: ON`, реальных
Яндекс-соединений нет.

Логи прокси:

```bash
cd "$STACK"
docker compose logs --tail=200 https-proxy 2>&1 | grep -E "uni.ws|gproxy|clck|report|quasar_metrics"
```

После ребута должны быть только `/uni.ws 101` и `/gproxy/...`; телеметрии нет.

## 7. Откат

### 7.1 Вернуть станцию

```bash
adb -s "$STATION_IP:5555" shell '
  cp /data/opt/backup/launch.json.before-replication /system/vendor/quasar/launch.json
  cp /data/opt/backup/onboot.sh.before-replication /system/vendor/quasar/onboot.sh
  cp /data/opt/backup/rootkeeper.sh.before-replication /data/quasar/rootkeeper.sh
  cp /data/opt/backup/hosts.before-replication /etc/hosts
  cp /data/opt/backup/unbound.conf.before-replication /etc/unbound/unbound.conf
  cp /data/opt/backup/chrony.conf.before-replication /etc/chrony.conf
  /data/quasar/egress-guard-off.sh 2>/dev/null || true
  rm -f /data/quasar/egress-guard.sh /data/quasar/egress-guard-off.sh /data/quasar/egress-guard-watchdog.sh /data/quasar/egress-guard.confirmed
  rm -f /data/quasar/yandex-block.sh /data/quasar/launch.json.notel
  sync
'
adb -s "$STATION_IP:5555" shell 'reboot'
```

Если нужно вернуть **OTA/fortress** к заводскому состоянию:

```bash
"$LAB/scripts/restore-ota-fortress.sh"
```

### 7.2 Вернуть Mac/стек

```bash
cd "$STACK"
docker compose stop https-proxy
# ALLOW_UPDATES/generate.sh — из backups/pre-telemetry-.../ при необходимости
```

DNS на Mac:

```bash
cd "$MITM"
./scripts/proxy-ctl.sh dns-stop
```

Роутер: Static DNS можно вернуть на `192.0.2.1` (если он указывал на Mac).

## 8. Чего не делать

- Не применять `launch.json`/`onboot.sh`/`quasar.cfg` от Midi на Station 1/Lite.
- Не менять `unbound` на публичные DNS, если нужен контроль: наша схема — только Mac.
- Не запускать `curl -k | sh` и не делать `setenforce 0`.
- Наша схема блокирует **весь** Яндекс: облачная Алиса, Музыка и Лента не работают. Если нужна облачная Алиса — это не тот гайд.
- Не трогать `fluent-bitd` без понимания.
- Не считать OTA-блок аппаратной защитой recovery.
- Не оставлять rootfs в `rw`.
- Не публиковать `secrets/`, `ota_script.pub.orig`, приватные ключи и токены.
- Не запускать второй агент/скрипт на этой же колонке одновременно: конфликт правок.

## 9. Текущее состояние (снимок)

- Модель: `Yandex Cucumber DVT2`, `cucumber`, `.160`;
- root ADB + SSH;
- `updatesd` нет, `device_ota_update.sh` — no-op, `ota_script.pub` — dummy;
- `activate_adb.sh` wrapper + keeper v4;
- `unbound` → `192.0.2.11`; `/etc/hosts` YANDEX-BLOCK; chrony Google;
- `iptables` INPUT/OUTPUT 10/10; IPv6 1/1;
- `EGRESS_GUARD`: private LAN/DHCP/Google NTP, остальной WAN DROP; ADB/SSH/LAN живы;
- DPP ключ/QR/скрипт установлены; `dpp_config_processing=2`; тест с телефоном
  ещё нужен; sound-генератор готов, режим сопряжения не проверен;
- `launch.json`: `mediad/maind/fluent-bitd/bulbik`, без telemetry;
- прокси: s3/cdn `403`, clck/report/startup/quasar_metrics `204`, `/check_updates` → `hasUpdate:false`;
- реальных соединений с Яндексом нет; egress-guard блокирует WAN даже без DNS;
- локальная Алиса — только при включённом Mac; доступ к станции — по IP из LAN;
- полный бэкап: `$LAB/backups/emergency-20261023-2137`;
- бэкап перед гвардом: `$LAB/backups/egress-20261024-050346`.
