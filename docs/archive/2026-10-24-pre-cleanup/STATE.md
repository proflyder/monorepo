# Текущее состояние

Снимок на **2026-10-24 16:46**. Полный вывод — `logs/status-20261024-1646-dpp-sound.txt`; кратко — `docs/CONTEXT.md`.

**Активный режим:** Яндекс-карантин + station-side egress-guard. DNS-спуфинг,
Yandex-сети в `iptables` и allowlist `EGRESS_GUARD` режут Яндекс; станция ходит
только в LAN, `https-proxy` на `192.0.2.11` (если Mac включён). ADB/SSH/OTA-запрет
живы. Телеметрия Quasar удалена, OTA-предложения выключены, s3/cdn отдают `403`.

Пользователь выключил прокси на роутере (24.10). На проверке DNS роутера ещё
отвечал и отдавал реальные Яндекс-IP, но egress-guard их резал; ADB/SSH и
локальный API по IP работали. Если DNS роутера тоже выключат — доступ по IP
сохранится, guard не зависит от DNS.

## Колонка

| Параметр | Значение |
|---|---|
| Модель | Yandex Station Midi |
| Device ID | `STATION_SERIAL` |
| MAC | `02:00:00:00:00:02` |
| IP | `192.0.2.13/24` (wlan0, DHCP) |
| Прошивка | `0.369.1.46.4778262955.20261013.160` |
| Версия quasar | `3.21` |
| Ядро | `Linux 5.4.180 #2 SMP PREEMPT ... aarch64` |
| ОС | Buildroot 2020.02.1 |
| Корень | `/dev/root` (ext4, `ro`), 975.9 MB, занято 459 MB |
| `/ota` (`/bkupgrade`) | 1.4 GB, занято ~170 MB |
| ADB | `:5555`, `adbd` (pid динамический, `pidof adbd`), root без авторизации |
| SSH | `:22`, dropbear, `root` / см. `secrets/CREDENTIALS.md` |
| Маркеры ADB | `/data/quasar/activate_adb_quasmodrom`, `/data/quasar/activate_network_adb` (оба есть, keeper) |

> Маркеры пересоздаются keeper'ом; `deactivate` от облака блокируется
> обёрткой `activate_adb.sh`. Сетевой ADB переживает ребут и kill adbd.

## Mac

| Параметр | Значение |
|---|---|
| IP | `192.0.2.11` (en0) |
| macOS | 27.0.1 |
| DNS | `src/dns.ts`, запущен через `nohup`, UDP `:53` (pid в `logs/dns.pid`); LaunchAgent нет |
| Bun-прокси | остановлен (стек занимает 80/443) |
| Colima | запущен, VM aarch64, 8 CPU, 24 GiB, docker 28.4.0 |
| Docker-порты | `0.0.0.0:80`, `0.0.0.0:443`, `0.0.0.0:31116` — **API без auth** |

## Стек v3rt3p

| Сервис | Состояние | Порты |
|---|---|---|
| `https-proxy` | Up | 80/443 |
| `quasar-server` | Up | 31116 (API), 31115 внутри |
| `processor` | Up | 8080 внутри |
| `quasistreaming-gigam` | Up | 8080 внутри |
| `supertonic-tts` | Up | 9150 внутри |
| `postgres` | Up | 5432 внутри |

## DNS

Сейчас `DNS_NAMES` содержит полный список Яндекса (35 имён + корневые
`yandex.ru/.net/.com`, `ya.ru`, `yango.com`, `funtechservices.com`).
Локальный DNS на Mac отдаёт для них `192.0.2.11`, остальное форвардит.

На колонке:
- `/etc/unbound/unbound.conf` → единственный forwarder `192.0.2.11`;
- `/etc/hosts` → блок `YANDEX-BLOCK` с основными именами на `192.0.2.11`;
- `/etc/chrony.conf` → Google NTP (`216.239.35.0/4/8/12`), без `ntp.yantp.net`.

Конфиги в rootfs периодически перезаписываются, поэтому их переприменяет
`/data/quasar/yandex-block.sh` (из `onboot.sh` до Quasar и из keeper раз в 60 с).
Файлы сохранены в `configs/station-applied/`.

## Egress-guard

С 24.10.2026 на станции стоит allowlist `EGRESS_GUARD` / `EGRESS_GUARD6`:

- разрешены loopback, private LAN (`10/8`, `172.16/12`, `192.168/16`,
  `100.64/10`), `169.254.0.0/16`, `224.0.0.0/4`, `255.255.255.255`,
  DHCP (`68→67`, `546→547`) и Google NTP (`216.239.35.0/24:123`);
- весь остальной IPv4/IPv6 `OUTPUT` — `DROP`;
- `INPUT` не ограничен: ADB `:5555`, SSH `:22` и LAN-доступ работают;
- существующие соединения с Яндексом не спасает `ESTABLISHED`, потому что
  общего allow для WAN нет;
- Яндекс режется по IP, **DNS на роутере/Mac для этого не нужен**.

Скрипты:
`/data/quasar/egress-guard.sh`, `egress-guard-off.sh`,
`egress-guard-watchdog.sh`; локальные копии — `scripts/station-egress-guard*.sh`.
Вызов: `/system/vendor/quasar/onboot.sh` после `yio_iptables_init.sh` и
`/data/quasar/rootkeeper.sh` раз в 60 с.

Проверено: reboot, новое ADB/SSH, LAN API, `ping`/HTTP на Яндекс и `1.1.1.1`
блокируются, NTP Google идёт. Откат — `egress-guard-off.sh`; бэкап
`backups/egress-20261024-050346/`.

## DPP / sound provisioning

- DPP/Wi-Fi Easy Connect: `wpa_supplicant 2.10`; `dpp_config_processing=2`;
  DER-ключ `secrets/dpp/bootstrap.der` + `/data/quasar/dpp-bootstrap.hex`;
  QR `secrets/dpp/qr.png`, URI `secrets/dpp/uri.txt`.
- `/data/quasar/dpp-provision.sh` вызывается из onboot/keeper; bootstrap
  стабилен после reboot; `dpp_listen` включается только без default route
  (force: `/data/quasar/dpp-force-listen`).
- **DPP при активном Wi-Fi не работает:** `wlan0` драйвер даёт
  `vm_cfg80211_remain_on_channel ... vm_p2p_support==0`; телефон не завершает
  обмен. `p2p0` off-channel работает, но второй интерфейс уронил `wlan0`;
  восстановление — reboot. **p2p0 при рабочей сети не запускать.** DPP — только
  unassociated, после USB/fallback. Android 10+ вероятен, iPhone не гарантирован.
- Sound: `scripts/station-pairing-sound.py` — локальный WAV/OGG без Telegram;
  `resources/quasar-connect-bot` проверен 15/15. На живой Midi не проигрывали;
  нужен режим сопряжения; случайный токен только Wi-Fi.

## Что уже сделано

- Сброс колонки на заводскую 2024 (`0.7.1.24...factory-7`), восстановление
  доверия к eMudhra.
- Получен root ADB, установлен SSH (dropbear), переживает ребут.
- Обновление на безопасную `0.369.1.46...160` через штатный OTA стека
  (`updates-config.yml`, CRC `3103154577`).
- Развёрнут стек v3rt3p (сборка, модели, Postgres, STT/TTS).
- **Поставлены OTA-запрет и fortress** (23.10): обёртка `activate_adb.sh`,
  keeper v4, no-op `device_ota_update.sh`, dummy `ota_script.pub`,
  `updatesd` вырезан из `launch.json`. Проверено после ребута.
- Починен STT на aarch64: `ten-vad` не поддерживает платформу, добавлен
  fallback «всё аудио — одна реплика» (`repos/quasistreaming-gigam/.../vad_processor.py`).
- Включён **Яндекс-карантин**: DNS и `iptables` в обе стороны, chrony на Google.
  Проверено после ребута: реальных соединений с Яндексом нет, локальный трафик
  идёт в прокси, WAN ограничен egress-guard; root/ADB/SSH/OTA-запрет сохранились.
- **Телеметрия вырезана**: task `telemetry` убран из `launch.json`; `clck`,
  `report`, `startup.mobile` и `/quasar_metrics` отдают `204`; `maind --telemetry`
  после ребута не стартует.
- **OTA-предложения выключены**: `ALLOW_UPDATES=false` в compose;
  `/check_updates` → `{"hasUpdate":false}`. `*.s3.yandex.net` и
  `*.svc.cdn.yandex.net` в `https-proxy` → `403` (раньше шли в настоящий Яндекс).
- Проверены внешние репозитории/ресурсы; клонированы в `resources/`,
  матрица — `docs/RESOURCES.md`. Ничего не установлено.
- Проверены recovery/factory: factory-образа Midi `cucumber` нет; в
  `artifacts/firmware/` лежит `yandexmidi-0.372...298.zip` — это **Station 2**,
  не Midi. Raw-дампа `recovery`/`aml_sysrecovery` нет.
- **Поставлен station-side egress-guard**: allowlist private LAN/DHCP/Google
  NTP, остальной WAN DROP; закреплён в rootfs `onboot.sh` и keeper. Проверено
  reboot и выключением Mac DNS/proxy: ADB/SSH/LAN API живы, Яндекс недоступен.
- **DPP/Wi-Fi Easy Connect**: сохранён DER-ключ, QR, `dpp_config_processing=2`,
  boot/keeper-скрипт; проверено reboot, QR/pkhash стабильны. Тест с телефоном
  ещё нужен.
- **Sound pairing**: локальный генератор WAV/OGG; 15/15 тестов; режим
  сопряжения/живая Midi ещё не проверены.

## Ресурсы

`resources/` содержит `pulsar`, `yandex-station-repo` (apkg), `yaff`, `spotiya`,
`ya-station-root-shell`, архивы AirPlay/Spotify и заметки. Статусы:
`pulsar` — Midi, но требует HA и заменяет стек; `apkg` — возможен buildroot;
`yaff` — Midi support, нужна сборка; `yse` — исходник не передан;
`spotiya` — Mini 2/armv7; AirPlay — тест только Mini 3; `spotify.sh` — битый.
Подробно — `docs/RESOURCES.md`.

## Известные проблемы / ограничения

1. **API `quasar-server` `:31116` без авторизации**: `push-directive`/`PATCH`
   могут отправить станции произвольную директиву/конфиг. Нужно привязать к
   `127.0.0.1` или убрать публикацию. **Не исправлено.**
2. Кастомный бэкенд v3rt3p без LLM-ключа; полноценных ответов Алисы нет.
3. **Factory/recovery:** проверенного factory-образа Midi нет; `yandexmidi`
   `.298` — Station 2; raw-бэкапа `recovery`/`aml_sysrecovery` нет. Factory
   reset снесёт `/data`, root/SSH/OTA; от серверного лока не гарантирует.
4. OTA-запрет/fortress стоят; снять — `scripts/restore-ota-fortress.sh`.
5. Mac DNS/прокси без автозапуска; сон/ребут Mac ломает локальную Алису и
   DNS-имена. На карантин это больше не влияет: egress-guard блокирует Яндекс
   независимо от Mac. Доступ к станции при выключенном Mac — по IP из LAN.
   Роутер сейчас Static DNS → Mac; для карантина только колонки его можно
   вернуть на `192.0.2.1`.
6. Ресурсы из `resources/` не установлены; кандидаты тестировать только на
   запасной Midi.
7. Egress-guard доверяет private LAN; устройство в такой сети может быть
   прокси. NTP Google — единственное разрешённое WAN-исключение.
   `ALLOW_NTP=0`/удаление правила убирает и его.
8. **Мобильный provisioning наполовину:** guard private-LAN; DPP на `wlan0`
   не работает при активной сети (`vm_p2p_support==0`), а `p2p0`-обход роняет
   Wi-Fi. Sound готов, но режим сопряжения не тестирован; fallback SSID, USB
   NCM и AP не внедрены. DPP — только unassociated после USB/fallback. План —
   `docs/PROVISIONING.md`; до завершения не переезжать/не менять Wi-Fi.
