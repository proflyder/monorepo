# RESOURCES — совместимость с нашей Midi

**Дата:** 2026-10-24  
**Станция:** Yandex Station Midi, `cucumber`, `STATION_SERIAL`  
**Прошивка:** `0.369.1.46.4778262955.20261013.160`  
**Платформа:** aarch64, Buildroot, root ADB, карантин + OTA-блок  
**Правило:** ничего из этого **не устанавливалось** на станцию. Только клонировано/сохранено в `resources/` для изучения.

## Матрица

| # | Ресурс | Где лежит | Что это | Совместимость с Midi `.160` | Решение |
|---|---|---|---|---|---|
| 1 | **pulsar** | `resources/pulsar/` | Локальный голосовой спутник HA (ESPHome API) + Sendspin/Music Assistant, заменяет `maind` | **Да по модели** (`cucumber`), но проверено на `.351`; наш `.160` новее. Нужен Home Assistant, Assist STT/TTS; заменяет стек Яндекса | **Reference / не сейчас** |
| 2 | **yandex-station-repo / apkg** | `resources/yandex-station-repo/` | Пакетный менеджер `apkg` + пакеты aarch64 для Android/buildroot | **Возможно**: buildroot/aarch64 поддержан; на `cucumber .160` не проверялся. Ставит в `/usr/local`/`/opt`, rootfs remount | **Кандидат на тест на запасной** |
| 3 | **yse** | `resources/yse/` | Пакетный менеджер для quasar/Buildroot, ставит в `/data/opt`, Entware | **Да по описанию** (Миди в списке), но исходник/ссылка не переданы | **Ждёт исходник** |
| 4 | **spotiya** | `resources/spotiya/` | Spotify Connect + голос Алисы, Rust | **Нет**: только Mini 2, armv7/32-bit; наша Midi aarch64 | **Отклонить** |
| 5 | **yaff / YAM** | `resources/yaff/` | Модульная Lua-система `.yap`, LED/часы/Bluetooth, web UI | **Да по железу**: в docs есть Midi `cucumber`, aarch64. Нужна сборка Rust/URL бинарника; web UI без auth | **Кандидат на тест** |
| 6 | **ya-station-root-shell** | `resources/ya-station-root-shell/` | Root через eMudhra CA/MITM на Mini 2/Lite | **Не нужен**: наша Midi уже рутован; эксплойт для других SoC | **Reference** |
| 7 | **AirPlay 2 station v1.0** | `resources/airplay2-station/airplay2-station-v1.0.zip` | AirPlay 2 через shairport-sync, отключает Алису | **Модель `cucumber` есть в таблице**, но проверено только на Mini 3; звук вероятен, LED/часы — только Mini 3; пишет rootfs | **Кандидат на запасную** |
| 8 | **yandex-station-airplay-fix.sh** | `resources/airplay-fix/` | Патч `airplayd` + хуки для Mini 3 | **Нет**: тестировано на Mini 3 `bergamot`, патч по смещению | **Отклонить** |
| 9 | **spotify.sh** | `resources/spotify-autostart/spotify.sh` | Обёртка/librespot для Spotify | **Нет в текущем виде**: скрипт битый/неполный, переменные не заданы, нет `setup.sh` и `librespot-static` | **Отклонить до исходника** |
| 10 | **apkg_repo Telegram** | ссылка | Канал обновлений пакетов apkg | Не установка; полезно для apkg | **Reference** |
| 11 | **yandex-station-1-panel** | (ранее изучено) | Веб-панель Station 1 | **Нет**: Station 1 `tulip-d1`, не Midi; web-terminal root на 8080 | **Отклонить** |
| 12 | **OTA decrypt script** | (текст, без репо) | Расшифровка `update.zip` Mini 2/Lite/Station 2 | **Нет**: ключ Mini 2, наша Midi `cucumber` не в списке | **Отклонить** |
| 13 | **yandex-station-privacy.sh** | `resources/telemetry-yandex-station-privacy.sh` | Vibe-скрипт блокировки `clck`/DNS | **Частично**: у нас уже сильнее; скрипт патчит `quasar.cfg`/`unbound.conf.head`, у нас `unbound.conf.head` нет; меняет DNS на публичный | **Reference, не запускать** |
| 14 | **v3rt3p-stack** (наш) | `~/Desktop/v3rt3p-stack` | Локальный фейковый backend | Работает; не обновлять без причины, наши патчи `403/204` | **Оставить** |
| 15 | **quasar-connect-bot** | `resources/quasar-connect-bot/` | Звуковой код сопряжения Yandex Station с Wi-Fi (FSK 16 кГц), Telegram-бот + Python-генератор | **Протокол от app 26.18.0**; на Midi `.160` не проверен. Wi-Fi-токен рандомный, привязка к аккаунту не работает; нужен режим сопряжения | **Offline fallback / тест после USB/fallback** |

## Детали

### 1. pulsar

- Автор: `Morvex885`, https://github.com/Morvex885/pulsar
- Последний стабильный релиз: **v0.3.0** (22.10.2026), в `main` уже есть коммит 23.10 про автозапуск через `/etc/init.d/S99zpulsar`.
- Модели: Midi `cucumber`, Lite 2 `plum` (Lite 2 экспериментально).
- Требуется: root ADB, Home Assistant + ESPHome + Assist STT/TTS; для музыки — Music Assistant/Sendspin.
- Важно: **заменяет** штатный `maind`/Quasar, а не работает параллельно (issue #12 — гибридного режима нет). Наша схема карантина/keeper/OTA при этом конфликтует.
- Открытый issue #13: ABI ALSA на 32-битном ARM; на нашей aarch64 Midi надо проверять отдельно.
- **Не устанавливать на текущую станцию.** Если делать — отдельная тестовая Midi, бэкап, релиз с фиксом автозапуска, проверка HA.

### 2. yandex-station-repo / apkg

- Автор: `initialfox`, https://github.com/initialfox/yandex-station-repo
- `apkg` — статический aarch64-бинарь; поддерживает Android (bionic) и **buildroot**.
- На buildroot ставит в `/usr/local/bin` / `/opt`, на Android — `/data/local/*`.
- Наша Midi — buildroot/aarch64, поэтому **теоретически совместима**, но конкретно `cucumber .160` не проверялась.
- Минусы: запись в rootfs с remount; пакеты идут с GitHub; для нашей hardened-схемы это лишний канал. Клон весит ~340 МБ (пакеты).
- **Кандидат только для запасной станции.** На текущей не ставить.

### 3. yse

- По описанию: лёгкий шелл-скрипт, ставит в `/data/opt`, разворачивает Entware (htop, tmux, python, git…), поддерживает Миди.
- **Исходник/архив не передан**, в GitHub поиске не нашёлся. Без ссылки/архива проверить нельзя.
- Это самый безопасный по идее вариант для Midi (не трогает rootfs), но **нужен исходник**. Положи в `resources/yse/` архив/ссылку — проверю.

### 4. spotiya

- Автор: `klockky`, https://github.com/klockky/spotiya
- Spotify Connect + голосовое управление, Rust.
- README: проверено только на **Mini 2**, Buildroot, glibc 2.30, **32-битный ARM**; для других моделей нужна своя сборка и проверка ALSA/Алисы.
- Наша Midi — aarch64; готовый armv7-бинарь не подойдёт.
- **Отклонить** в текущем виде; порт возможен, но не заявлен.

### 5. yaff / YAM

- Автор: `nyakokitsu`, https://github.com/nyakokitsu/yaff
- YAM — менеджер `.yap`-пакетов на Lua, Rust-демон, web UI на `:8099`.
- В `docs/PLATFORM_HARDWARE.md` есть **Midi `cucumber`**: LED-кольцо, 45 сегментов часов, датчик света, кнопки.
- Установка: нужен готовый `yamd` aarch64 (musl) по URL; `install.sh` кладёт в `/data/quazar/yam` (в репо опечатка `quazar`; лучше `/data/quasar/yam`).
- Web UI **без аутентификации**; пакетам можно давать бинарники; не даёт OTA/ADB/factory.
- **Кандидат на запасную станцию**; на текущей не ставить.

### 6. ya-station-root-shell

- Автор: `miilv`, https://github.com/miilv/ya-station-root-shell
- Репродукция eMudhra CA → MITM → root ADB на **Mini 2** и **Lite**.
- Наша Midi уже рутован; для нас это **reference** по MITM/сертификатам, не инструмент установки.
- В репозитории есть PoC-сертификаты/ключи; не публиковать, не смешивать с `secrets/`.

### 7. AirPlay 2 station v1.0

- Локальный архив: `resources/airplay2-station/airplay2-station-v1.0.zip`.
- README: shairport-sync 4.3.7 AirPlay 2 + nqptp + avahi + `airplayd`.
- В таблице моделей есть **`cucumber` — Станция Миди**; но проверено **только на Mini 3** (`bergamot`). На других моделях звук/AirPlay должны работать, LED/часы — только Mini 3.
- **Отключает Алису**; установщик пишет в rootfs (автозапуск, avahi, dbus).
- Требует Windows PowerShell (`deploy.ps1`) или ручных команд; ~120 МБ на `/data`.
- **Кандидат только на запасную Midi**; на текущей не ставить: сломает наш карантин/Алису.

### 8. yandex-station-airplay-fix.sh

- Локальный скрипт: `resources/airplay-fix/yandex-station-airplay-fix.sh`.
- Патчит `airplayd` по смещению 7344 и ставит хуки shairport/quasar.
- В шапке: проверено на **Mini 3 `bergamot`**; на других моделях патч применяется только при совпадении байтов.
- Для нашей Midi **не подходит**; не запускать.

### 9. spotify.sh

- Локальный скрипт: `resources/spotify-autostart/spotify.sh`.
- В текущем виде **битый**: не заданы `INSTALL_DIR`, `CACHE_DIR`, `BIN_PATH`, `PID_FILE`, `LOG_FILE`, `SERVICE_SCRIPT`; в середине висит `EOF`; нет `setup.sh`; нет `librespot-static`.
- Это не рабочий установщик, а обрывок. **Не запускать.**
- Если нужен Spotify — проще `spotiya` (но Mini 2) или собрать librespot под aarch64 отдельно; нужен нормальный исходник/бинарь.

### 10. apkg_repo

- Telegram: https://t.me/apkg_repo
- Канал обновлений/новых пакетов для `apkg`; не самостоятельный установщик.
- Держим как reference к apkg; на станцию ничего не ставит.

### 11. yandex-station-1-panel

- https://github.com/initialfox/yandex-station-1-panel
- Station 1 (`tulip-d1`), не Midi; web-terminal root на `0.0.0.0:8080`, `curl -k | sh`, `setenforce 0`.
- **Не подходит**; ранее уже разбирали.

### 12. OTA decrypt script

- Не репозиторий; текст/ключ для Mini 2 (`bl2`/`tpl`), поддержка Mini 2 / Lite 2021 / Station 2.
- Наша Midi `cucumber` не в списке; ключ Mini 2 не применим.
- **Не использовать** на нашей станции.

### 13. yandex-station-privacy.sh

- Локальный vibe-скрипт: `resources/telemetry-yandex-station-privacy.sh`.
- Патчит `quasar.cfg` и `/bkupgrade/data/unbound.conf.head`, ставит `S100privacy`.
- У нас `unbound.conf.head` **нет**; `quasar.cfg` не патчим; DNS уже на Mac; скрипт заменил бы форвардеры на Cloudflare/Google/`192.168.0.1` и ослабил карантин.
- **Reference, не запускать.**

## Что уже добавлено в проект

- `resources/pulsar/` — git clone;
- `resources/yandex-station-repo/` — git clone (340 МБ);
- `resources/ya-station-root-shell/` — git clone;
- `resources/yaff/` — git clone;
- `resources/spotiya/` — git clone;
- `resources/spotify-autostart/spotify.sh` — копия;
- `resources/airplay2-station/airplay2-station-v1.0.zip` — копия;
- `resources/airplay-fix/yandex-station-airplay-fix.sh` — копия;
- `resources/telemetry-yandex-station-privacy.sh` — копия;
- `resources/yse/` — пока только README, ждёт исходник.

## Правила на будущее

- Ничего из этого не ставить на текущую станцию без отдельного «да».
- Любой кандидат сначала проверять на **запасной** Midi/Lite, с бэкапом.
- Не запускать `curl | sh`, не делать `setenforce 0`, не оставлять rootfs в `rw`.
- Не публиковать `secrets/`, PoC-ключи, `ota_script.pub.orig`, токены.
- После установки любого пакета проверять: ADB/SSH, `launch.json`, telemetry, `iptables`, OTA-блок.

### 15. quasar-connect-bot

- Репо: https://github.com/sevennnoff/quasar-connect-bot (MIT, 23.10.2026).
- Генерирует звуковой код Wi-Fi; `pairing.py` офлайновый, к Яндексу не ходит.
- Тесты 15/15; реальной проверки на станции нет.
- Рандомный токен — только Wi-Fi; привязка к аккаунту Яндекса не сработает.
- Нужен режим сопряжения; способ входа без приложения не описан.
- Использовать локальную генерацию, не публичный Telegram-бот для реальных паролей.
- Решение: резервный offline-способ; сначала DPP/QR + USB/fallback.
