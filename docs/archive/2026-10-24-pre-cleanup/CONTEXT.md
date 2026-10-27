# CONTEXT — краткая память для /compact

**Дата:** 2026-10-24 17:15 MSK  
**Лаборатория:** `~/Desktop/yandex-station-lab`  
**Колонка:** Yandex Station Midi, `cucumber`, `STATION_SERIAL`, MAC `02:00:00:00:00:02`, `192.0.2.13`  
**Mac:** `192.0.2.11`, macOS 27.0.1

## Главное

- Прошивка: `0.369.1.46.4778262955.20261013.160` (13.10.2026, до патча).
- Root/ADB: `adb connect 192.0.2.13:5555`, `uid=0(root)`, без авторизации.
- SSH: `ssh root@192.0.2.13`, пароль — см. `secrets/CREDENTIALS.md` (dropbear, `S99dropbear`).
- OTA/ADB fortress: wrapper `activate_adb.sh` (deactivate → no-op), keeper v4
  `/data/quasar/rootkeeper.sh`, no-op `device_ota_update.sh`, dummy
  `ota_script.pub`, `updatesd` вырезан из `launch.json`. Проверено ребутом.
- **Текущий режим — Яндекс-карантин**: DNS + `iptables`/`ip6tables` режут
  Яндекс в обе стороны; `unbound` на колонке → только `192.0.2.11`;
  локальный трафик идёт в `https-proxy`, WAN разрешён только для Google NTP
  через egress-guard. Облачная Алиса, Музыка, Лента, умный дом Яндекса
  недоступны. ADB/SSH/OTA-запрет живы.
- **Роутерный прокси выключен (24.10, пользователь):** DNS на роутере ещё
  отвечает и отдаёт реальные Яндекс-IP, но egress-guard их режет; доступ к
  станции по IP сохраняется. Отключение DNS на роутере тоже не влияет на guard.
- **Egress-guard (24.10):** на станции стоит allowlist `EGRESS_GUARD`:
  разрешены loopback, private LAN (`10/8`, `172.16/12`, `192.168/16`,
  `100.64/10`), link-local/multicast, DHCP и Google NTP
  (`216.239.35.0/24:123`); **весь остальной IPv4/IPv6 — DROP**. Яндекс режется
  на уровне IP, DNS уже не важен. ADB/SSH и доступ из LAN сохранены.
  Скрипты: `/data/quasar/egress-guard.sh`, `-off.sh`, `-watchdog.sh`;
  вызывается из `onboot.sh` после `yio_iptables_init.sh` и из keeper раз в 60 с.
  Проверено ребутом и при выключенных Mac DNS/proxy.
- **DPP/Wi-Fi Easy Connect (24.10):** ключ/QR/скрипт стоят, QR стабилен.
  **Но DPP при активном Wi-Fi не работает**: драйвер `wlan0` даёт
  `vm_cfg80211_remain_on_channel ... vm_p2p_support==0`; телефон не может
  завершить DPP. На `p2p0` off-channel работает (`remain_on_channel` без
  ошибки), но второй интерфейс **уронил wlan0**; восстановление — reboot.
  **Больше не запускать p2p0 при рабочей сети.** DPP тестировать только
  unassociated, после USB/fallback. Force: `/data/quasar/dpp-force-listen`.
- **Sound pairing (24.10):** локальный генератор
  `scripts/station-pairing-sound.py` (без Telegram/облака; WAV/OGG);
  `resources/quasar-connect-bot` (MIT) проверен, 15/15 тестов. На живой Midi
  не проигрывали: нужен режим сопряжения. Рандомный токен — только Wi-Fi.
- Защита: `/data/quasar/yandex-block.sh` (DNS/hosts/chrony/iptables + возврат
  `launch.json` без telemetry), вызывается из `onboot.sh` до Quasar и из keeper
  раз в 60 с; переживает ребут.
- Телеметрия: task `telemetry` удалён; `maind --telemetry` не стартует;
  `clck.yandex.net`, `report.appmetrica`, `startup.mobile`, `/quasar_metrics` → `204`.
  `clck.yasmina.yango.com` → локальный `quasar-server` (наружу не идёт).
- OTA: `ALLOW_UPDATES=false`; `/check_updates` → `hasUpdate:false`;
  `*.s3.yandex.net` и `*.svc.cdn.yandex.net` в `https-proxy` → `403`.
- Кастомный бэкенд v3rt3p поднят, но **без LLM-ключа**; полноценных ответов нет.

## КРИТИЧНО / открыто

1. **API `quasar-server` на `0.0.0.0:31116` без авторизации.**
   `POST /devices/:duid/push-directive` и `PATCH /devices/:duid` могут отправить
   станции произвольную директиву/конфиг. Предложено привязать к
   `127.0.0.1:31116:31116` или убрать публикацию; **не применено**.
2. **Factory/recovery:** проверенного factory-образа для Midi `cucumber` нет.
   В `artifacts/firmware/` лежит `yandexmidi-0.372...298.zip` — это **Station 2**,
   не Midi; флешить нельзя. Raw-бэкапа `recovery` (192 МиБ) / `aml_sysrecovery`
   (1.5 ГиБ) / bootloader нет; дамп не делали. Factory reset снесёт `/data`,
   root/SSH/OTA-скрипты; от серверного лока/`stolen` не гарантирует.
3. **Мобильный provisioning наполовину готов:** guard расширен на private
   LAN; DPP на `wlan0` **не работает при активной сети** (драйвер
   `p2p_support==0`), а `p2p0`-обход роняет Wi-Fi. Sound-генератор готов, но
   не проверен в режиме сопряжения. Fallback SSID, USB NCM и AP не внедрены.
   DPP тестировать только unassociated, после USB/fallback.
4. **Mac остаётся единой точкой для локального стека, но уже не для карантина:**
   DNS запущен через `nohup`, LaunchAgent нет; при сне/ребуте Mac станция теряет
   DNS/локальную Алису, но egress-guard продолжает блокировать Яндекс.
   Роутер сейчас Static DNS → Mac; при выключенном Mac DNS имён нет, доступ к
   станции — по IP из LAN.

## Ресурсы (не установлены)

В `resources/` сложены внешние проекты; матрица — `docs/RESOURCES.md`.

- `pulsar/` — Midi поддержан, но требует Home Assistant/ESPHome, **заменяет**
  стек Яндекса; проверен на `.351`, у нас `.160`; не ставить сейчас.
- `yandex-station-repo/` — `apkg` aarch64/buildroot; возможно совместим,
  но ставит в rootfs `/usr/local`/`/opt`; только тест на запасной.
- `yaff/` — YAM `.yap`, поддержка Midi в docs; нужна сборка; web UI без auth;
  кандидат на запасную.
- `yse/` — по описанию лучший вариант (Миди, `/data/opt`, Entware), но
  **исходник/ссылка не переданы**; ждёт.
- `spotiya/` — только Mini 2/armv7; несовместим.
- `airplay2-station/` — модель `cucumber` есть, но тест только Mini 3;
  отключает Алису, пишет rootfs; только запасная.
- `airplay-fix/` — патч Mini 3; не Midi.
- `spotify-autostart/spotify.sh` — **битый/неполный**, нет `setup.sh` и
  `librespot-static`; не запускать.
- `yandex-station-1-panel` — Station 1, не Midi.
- OTA decrypt — Mini 2/Lite/Station 2, ключ Mini 2; к Midi не применимо.
- `yandex-station-privacy.sh` — reference; не запускать (конфликтует с нашей схемой).

## Доступ / пути

- Lab: `~/Desktop/yandex-station-lab`
- Живые проекты: `~/Desktop/alice-mitm-bootstrap`, `~/Desktop/v3rt3p-stack`
  (симлинки в `lab/services/`).
- Статус: `scripts/station-status.sh`; последний снимок — `logs/status-20261024-1715-dpp-p2p0-recovery.txt`
- Откат OTA/fortress: `scripts/restore-ota-fortress.sh`
- Секреты: `lab/secrets/` (chmod 700/600).
- Бэкапы: `/data/opt/backup` на колонке;
  `lab/backups/emergency-20261023-2137/` — полный бэкап;
  `lab/backups/pre-telemetry-20261023-2223/` — старые compose/generate;
  `lab/backups/egress-20261024-050346/` — iptables-save и старые onboot/keeper.
- Точный гайд: `docs/REPLICATION-GUIDE.md`.
- Матрица ресурсов: `docs/RESOURCES.md`.

## Что было сделано (сжато)

1. Сброс на заводскую 2024 → доверие к eMudhra → root ADB.
2. OTA на безопасную `.160` через `updates-config.yml`.
3. Стек v3rt3p: quasar-server, https-proxy, processor, GigaAM STT, TTS, Postgres;
   STT пропатчен под aarch64.
4. SSH/dropbear, OTA-запрет/fortress, keeper v4.
5. Полный реальный Яндекс (Music/Lenta работали) → затем карантин из-за угрозы
   удалённого сброса/закрытия ADB.
6. Карантин: DNS на Mac, `iptables` 10/10, unbound Mac, chrony Google,
   `/etc/hosts`; проверено ребутом.
7. Телеметрия вырезана; OTA-предложения и s3/cdn закрыты; проверено ребутом.
8. Полный emergency-бэкап станции.
9. Проверены внешние репозитории/ресурсы; ничего не установлено.
10. Проверены recovery/factory: factory-образа Midi нет; дамп не делали.
11. Egress-guard (24.10): allowlist LAN/DHCP/NTP, весь остальной WAN DROP;
    закреплён в onboot/keeper, проверен reboot и выключением Mac DNS/proxy.
    ADB/SSH/локальный API при этом живы, Яндекс — нет.
12. 24.10: guard расширен на private LAN; DPP ключ/QR/скрипт установлены.
    DPP с телефоном на `wlan0` не завершился из-за `vm_p2p_support==0`;
    `p2p0` off-channel сработал, но уронил `wlan0` — восстановили reboot.
    Локальный sound-генератор без Telegram проверен 15/15.

## Что осталось

- **P0:** закрыть API `31116` (bind localhost/remove ports) — с согласованием.
- **P1:** read-only дамп `bootloader/boot/dtb/recovery/aml_sysrecovery` — с согласованием.
- **P1:** DPP — только unassociated, после USB/fallback; не запускать p2p0
  при рабочей сети. Проверить sound в режиме сопряжения; затем fallback SSID
  и USB NCM.
- **P1:** LaunchAgent/автозапуск DNS+прокси на Mac (нужен только для локальной
  Алисы; на карантин больше не влияет).
- **P2:** LLM для v3rt3p; функции/музыка; SSH по ключам.
- **P2:** тесты кандидатов (`pulsar`, `apkg`, `yaff`, AirPlay) только на запасной Midi;
  получить исходник `yse`.

## Риски

- `:5555` root без пароля; `:22` root по слабому паролю; только LAN.
- API `31116` без auth — локальный push-канал.
- Factory reset сносит `/data`, keeper, SSH, пароль, OTA/telemetry-скрипты.
- Нет raw recovery/factory backup; серверный лок factory reset не гарантирует снять.
- Egress-guard доверяет private LAN: устройство в такой сети может теоретически
  быть прокси. NTP Google — единственное разрешённое WAN-исключение.
- DPP QR/ключ = доступ к провижинингу; хранить в `secrets/dpp`, не публиковать.
- Sound-код содержит Wi-Fi-пароль в звуке; не проигрывать в людном месте.
- При выключенном Mac локальной Алисы нет; доступ к станции — только из LAN
  по IP (`adb`/`ssh`), не из интернета.
- DNS-off сам по себе не блокировка; блокировка — egress-guard/iptables.
- `secrets/` и PoC-ключи не публиковать.
- Не запускать второй агент/скрипт на станции одновременно.
