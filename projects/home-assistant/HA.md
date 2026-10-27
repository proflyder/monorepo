# HA — Home Assistant Container на неттопе

**Текущий статус:** onboarding пройден, HA `2026.10.4`, контейнер `homeassistant`
работает; `http://192.0.2.26:8123` и `http://192.0.2.25:8123` отвечают.
Music Assistant `2.10.8` с Spotify готов; HA Assist base (Whisper/Piper/pipeline `Рикардо`) развёрнут; Pulsar `v0.4.1-ricardo6` подключён как satellite и Sendspin-плеер.

## 1. Почему Container, а не HAOS

На сервере уже Ubuntu Server, поэтому выбран официальный **HA Container**:

- работает как Docker-контейнер, обновляется вручную;
- **нет add-ons/Supervisor**;
- Music Assistant/Whisper/Piper ставятся отдельными контейнерами;
- если позже понадобятся add-ons — можно перейти на HAOS VM (KVM есть).

Текущий путь не «хуже»: он даёт полный HA Core и нормальную сеть, просто без
магазина приложений.

## 2. Файлы и пути

Всё управляется из `/home/labuser/nettop`:

```text
/home/labuser/nettop/docker-compose.yml
/home/labuser/nettop/ctl.sh
/home/labuser/nettop/homeassistant/config/   # /config в контейнере
/home/labuser/nettop/data/music-assistant/   # /data MA
```

Compose (фрагмент HA):

```yaml
services:
  homeassistant:
    image: ${HA_IMAGE:-ghcr.io/home-assistant/home-assistant:2026.10.4}
    container_name: homeassistant
    restart: unless-stopped
    privileged: true
    network_mode: host
    environment:
      TZ: ${TZ:-Asia/Almaty}
    volumes:
      - ./homeassistant/config:/config
      - /run/dbus:/run/dbus:ro
      - /etc/localtime:/etc/localtime:ro
```

`network_mode: host` нужен для discovery (mDNS/ESPHome) и доступа к локальной
сети. `privileged` нужен для части интеграций/DBus.

## 3. Ежедневные команды

```bash
cd /home/labuser/nettop

./ctl.sh status
./ctl.sh logs ha
./ctl.sh restart ha
./ctl.sh stop ha
./ctl.sh up
```

Проверка:

```bash
curl -sSL -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8123/
docker inspect -f '{{.State.Status}}' homeassistant
```

## 4. Обновление

Версии фиксируются в `.env`. Обновление — осознанное:

```bash
cd /home/labuser/nettop
./ctl.sh backup
# изменить HA_IMAGE/MA_IMAGE в .env на нужные теги
./ctl.sh pull
./ctl.sh up
```

Не использовать `stable` вслепую: перед обновлением проверить release notes,
сделать backup и убедиться, что UI/интеграции живы.

## 5. Backup

### 5.1. Штатный backup HA

HA UI → **Settings → System → Backups** → создать backup. Хранить вне сервера
(зашифрованно), не только на неттопе.

### 5.2. Backup всего стека

```bash
cd /home/labuser/nettop
./ctl.sh backup
```

Создаёт `backups/nettop-YYYYmmdd-HHMMSS.tar.gz` с:

- `docker-compose.yml`;
- `.env`;
- `homeassistant/config`;
- `data/music-assistant`.

На время backup стек останавливается, затем запускается снова.

### 5.3. Что ещё бэкапить

- `/var/lib/shim-signed/mok/` (MOK private key) — **обязательно**;
- `/etc/netplan/`;
- `/etc/modprobe.d/`, `/etc/modules-load.d/`;
- `/usr/src/rtl8821ce-...` или патч;
- `secrets/` лаборатории (Wi-Fi/SSH/DPP) — отдельно, зашифрованно.

## 6. Восстановление

```bash
cd /home/labuser/nettop
./ctl.sh down
sudo tar -xzf backups/nettop-YYYYmmdd-HHMMSS.tar.gz
./ctl.sh up
```

Если нужен только HA config — восстановить `homeassistant/config` из архива.
Старый compose до миграции: `archive/homeassistant-compose.pre-stack.yml`.

## 7. Сеть и доступ

- HA слушает `0.0.0.0:8123` (host network).
- Доступ из LAN `192.0.2.0/24`; удалённо — Tailscale:
  `https://lab-host.example-tailnet.ts.net` → `127.0.0.1:8123` через
  `tailscale serve`. HA MFA, external URL, trusted proxy `127.0.0.1`;
  tag `tag:homeassistant` + ACL только `tcp:443`.
- **Не делать port-forward 8123 на роутере.** Публичного HA нет; ONT/AX3
  не менялись. План/история: [`../../docs/REMOTE-ACCESS-TAILSCALE-PLAN.md`](../../docs/REMOTE-ACCESS-TAILSCALE-PLAN.md).
- MFA в HA включена. `trusted_networks`/auto-login не настраивались.

## 8. Голосовые команды Pulsar

«включи/выключи блютуз», «включи/выключи гейтвей», «громче/тише»,
«громкость 1…10», «пауза/продолжи/останови музыку», «включи любимое»
(+ варианты) добавлены через custom sentences + `intent_script`
(+ `rest_command`/MA media_player). Голая «продолжи» (а также
«продолжить», «возобнови») входит в `StationMusicResume`; next/previous —
локальные `StationMusicNext/Previous`; generic «включи музыку» — локальный
`StationMusicPlay`. Лампы: «выключи/включи лампу» (ночник/лампа у кровати),
«светильник», «ленту» — локальные `StationLightOn/Off`, без Hermes.
Сценарий «я сел работать» / «включи рабочий режим» → `script.work_mode`:
лента, лампа у кровати и светильник на мониторе. Обратное действие —
«black out» / «blackout» / «блэкаут» / «блэк аут» → `script.blackout`:
выключает те же три светильника. ASR-варианты «блок аут»/«блак аут» тоже
локальные. После wake можно сказать «отмена/передумал/не надо» —
`StationCancel`
просто закрывает вызов, без изменения музыки/source.

Таймеры — встроенные Assist-таймеры Pulsar (не `timer.*` entities):
«поставь таймер на 5 минут», «что с таймером», «отмени таймер». После
restart HA не сохраняются.

Будильник — один слот, helpers/scripts/automation/custom sentences:
«поставь будильник на 7:30», «на завтра», «по будням», «какой будильник»,
«отмени/отложи/останови/стоп». Повторы, snooze 9 мин, непрерывный рингтон, restore
source/volume; если музыка играла, после stop она продолжается; остановка
также коротким action-нажатием Pulsar. Подробности,
файлы и токен — [`ha-voice/README.md`](ha-voice/README.md).
Токен не публиковать.

## 9. Интеграции, которые понадобятся

| Что | Как |
|---|---|
| Pulsar (Midi) | ✅ ESPHome integration по IP `192.0.2.13:6053`; 33 entities, Noise key |
| Music Assistant | сервер и Spotify готовы; Pulsar Sendspin виден; HA integration **добавлена**; Spotify Connect plugin **включён**; 429 снят, search/play проверены |
| Whisper/Piper | ✅ `wyoming-whisper:3.8.1` / `wyoming-piper:2.5.2`; pipeline `Рикардо` |
| Spotify | Music Assistant + Spotify Soloist (новый аккаунт) |
| Hermes | ✅ `llama_cpp` conversation agent; личность **Рикардо**, личный ассистент |
| Matter | ✅ Matter Server `matterjs-server` :5580 loopback; HA integration `Matter`; лента Яндекса — pairing pending |
| Xiaomi Home | ✅ custom integration `v0.5.0`; China login; imported light bar `MJGJD02YL` and Mi Bedside Lamp 2; HomeKit duplicate removed |

## 10. Ограничения текущего HA

- Нет add-ons (by design для Container).
- Нет MFA/HTTPS (local-only).
- Нет внешнего backup.
- Music Assistant развёрнут, Spotify playback проверен; HA integration добавлена.
- Whisper: RU prompt для громкости/Spotify/будильника/таймера/«стоп»; ASR aliases `грокость`/`громкасть`.
  Spotify 429 снят 2026-10-27; Connect search/play проверены, MA `play_media`
  sync отдельно не перепроверялся. Spotify Connect plugin включён;
  voice-роутер MA/Connect; volume sync работает. Открыто: phone transfer и ASR retest.
- Pulsar `v0.4.1-ricardo6` активен; pipeline `Рикардо`, wake word `Рикардо`; TTS/reboot проверены. «Отмена/передумал» после wake — `StationCancel`, без side effects.
- Hermes/`llama_cpp` отвечает как **Рикардо**, личный ассистент; `SOUL.md`,
  `voice` personality и HA prompt согласованы. «включи музыку» и «громкость
  пять» проверены текстом/пайплайном; открыт живой микрофонный retest.
- Matter Server подключён к HA (`ws://127.0.0.1:5580/ws`), fabric пустой.
  Добавить Yandex LED strip через HA Companion: Matter → Add device → QR/код,
  телефон на 2.4 ГГц, Bluetooth и location permission; Multi-admin.
- Xiaomi Home `v0.5.0` loaded; вход Xiaomi China выполнен, config entry
  `01M5Y1M47A9VKA6Z98BEJT9FR8`. Импортированы Monitor Light Bar 1S
  (`light.yeelink_cn_919937191_lamp22_s_2`) и Mi Bedside Lamp 2
  (`light.yeelink_cn_827036414_bslamp2_s_2_light`). Дубликат HomeKit
  Controller `MiBedsideLamp2-F723` удалён; backup
  `~/backups/ha-duplicate-lamp-20261027-114648/`. OAuth redirect жёстко
  `http://homeassistant.local:8123`; при NXDOMAIN после «Agree and link»
  заменить host в адресной строке на `192.0.2.25`, сохранив путь/query.
  Осталось проверить on/off, brightness и color temperature.
- Голос «Алиса» → HA → TTS и голосовые команды проверены;
  `todo.shopping_list` aliases `список`/`покупки`; таймеры и будильник
  проверены text/API. Открыто: фактический голосовой retest таймера/
  будильника (включая «стоп»/resume музыки), Bluetooth-пара,
  phone→speaker transfer и volume retest.
- Wi-Fi driver — DKMS; зависит от обновлений ядра.
- HA container `privileged` — компромисс для DBus/интеграций; не выставлять
  сервер в недоверенную сеть.

## 11. Быстрая диагностика

```bash
cd /home/labuser/nettop
./ctl.sh status
./ctl.sh logs ha
ss -lntp | grep 8123
curl -v http://127.0.0.1:8123/
```

Если HA не открывается:

1. `./ctl.sh status` — контейнер Up?
2. `./ctl.sh logs ha` — ошибка конфига/порта?
3. `ss -lntp | grep 8123` — слушает?
4. `ip -br addr` — сеть/маршрут?
5. firewall/роутер — не блокирует LAN?

## 12. Связанные документы

- [`SETUP.md`](SETUP.md) — с нуля.
- [`stack/README.md`](stack/README.md) — единая точка входа HA + MA + Whisper/Piper.
- [`TROUBLESHOOTING.md`](TROUBLESHOOTING.md) — Wi-Fi/Secure Boot/сеть.
- [`../../docs/REMOTE-ACCESS.md`](../../docs/REMOTE-ACCESS.md) — удалённый доступ:
  исследование, варианты, план, security/rollback.
- [`../../docs/REBUILD.md`](../../docs/REBUILD.md) — восстановление неттопа/станции
  с нуля (SSD, factory reset, Quasar/Mac).
- [`../../docs/REMOTE-ACCESS-TAILSCALE-PLAN.md`](../../docs/REMOTE-ACCESS-TAILSCALE-PLAN.md)
  — **выбранный план на аппрув**: Tailscale на `lab-host` + Serve + MFA.
- [`../../docs/REMOTE-ACCESS-STATIC-IP.md`](../../docs/REMOTE-ACCESS-STATIC-IP.md) —
  альтернатива для статического IP: WireGuard на `lab-host`, ONT/AX3 forward,
  Caddy/HTTPS, mini-PC router.
- [`scripts/verify-remote-access.sh`](scripts/verify-remote-access.sh) —
  read-only проверка.
- `../docs/OPERATIONS.md` — общий runbook.
- `../docs/RESEARCH-OWN-ASSISTANT.md` — Pulsar/Spotify/MCP/wake word.
