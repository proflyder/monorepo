# NETTOP STACK — единая точка управления

**Legacy Docker deployment.** Production runtime перенесён в Kubernetes.
[Новая точка управления](../../../docs/KUBERNETES-OPERATIONS.md),
[chart приложений](../deploy/README.md). Команды ниже относятся к прежнему стеку.
Рабочие каталоги данных нельзя удалять: Kubernetes использует их через PV.

Это рабочая папка инфраструктуры внутри неттопа `lab-host`:

```text
/home/labuser/nettop/
  docker-compose.yml
  ctl.sh
  .env
  homeassistant/config/     # HA config
  data/music-assistant/     # MA database/cache/credentials
  data/matter-server/       # Matter fabric/controller storage
  data/whisper/             # Whisper model/cache
  data/piper/               # Piper voice/model
  backups/
```

Одна команда запускает всё:

```bash
cd /home/labuser/nettop
./ctl.sh up
```

Одна команда показывает состояние:

```bash
./ctl.sh status
```

## Миграция 2026-10-25

HA config перенесён из `/home/labuser/homeassistant/config` в
`/home/labuser/nettop/homeassistant/config`. Старый compose сохранён в
`archive/homeassistant-compose.pre-stack.yml`, а рабочий файл переименован в
`/home/labuser/homeassistant/docker-compose.yml.disabled`. С этого момента
рабочая точка входа — только этот `ctl.sh`.

## Что здесь работает

| Сервис | Контейнер | Порт | Роль |
|---|---|---:|---|
| Home Assistant | `homeassistant` | `8123` | Автоматизация, Assist, интеграции |
| Music Assistant | `music-assistant-server` | `8095` UI, `8097` stream, `8927` Sendspin | Музыка, Spotify, плееры |
| Matter Server | `matterjs-server` | `5580` WebSocket/dashboard (loopback) | Matter over Wi-Fi controller для HA |
| GigaAM STT | `wyoming-onnx-stt` | `10305` | primary STT: GigaAM v3, ru |
| Whisper | `wyoming-whisper` | `10300` | fallback STT: речь → текст |
| Piper | `wyoming-piper` | `10200` | TTS: текст → речь |

Все сервисы работают в `network_mode: host`. Для MA это обязательно:
mDNS/uPnP discovery и стриминг на AirPlay/Chromecast/DLNA/Sonos/Sendspin не
работают через Docker bridge. Для Matter host network обязателен для IPv6/mDNS;
WebSocket слушает только `127.0.0.1:5580`, HA подключается к
`ws://127.0.0.1:5580/ws` (HA тоже host-network).

## Первый запуск

```bash
cd /home/labuser/nettop
cp .env.example .env
./ctl.sh up
```

Проверить:

```bash
./ctl.sh status
curl -I http://127.0.0.1:8123/
curl -I http://127.0.0.1:8095/
curl -I http://127.0.0.1:5580/
ss -lntp | grep -E ':(10200|10300|10305|5580)\b'
```

Текущие адреса после Proxmox cutover:

- HAOS: `http://192.0.2.28:8123`
- MA: HAOS add-on `http://192.0.2.28:8095`
- Whisper/Piper: TCP `192.0.2.27:10300`/`10200`, UI нет

Старые `.26/.25` адреса ниже относятся к прежнему Ubuntu/Container runtime.

**Автозапуск проверен:** после reboot 2026-10-26 контейнеры
поднялись сами (`restart: unless-stopped`), HA/MA ответили `200`,
Whisper/Piper — `healthy`. Matter Server добавлен 2026-10-27.

## Управление

| Команда | Что делает |
|---|---|
| `./ctl.sh up` | Создаёт `.env`/директории и поднимает HA + MA + Matter + Whisper + Piper |
| `./ctl.sh status` | Контейнеры, HTTP, systemd, failed units |
| `./ctl.sh logs` | Логи всех сервисов |
| `./ctl.sh logs ma` | Логи Music Assistant |
| `./ctl.sh logs matter` | Логи Matter Server |
| `./ctl.sh logs ha` | Логи Home Assistant |
| `./ctl.sh logs whisper` | Логи Whisper |
| `./ctl.sh logs piper` | Логи Piper |
| `./ctl.sh restart` | Перезапустить всё |
| `./ctl.sh restart ma` | Перезапустить MA |
| `./ctl.sh stop` | Остановить всё |
| `./ctl.sh stop ma` | Остановить MA |
| `./ctl.sh down` | Удалить контейнеры, данные остаются |
| `./ctl.sh pull` | Скачать образы из `.env` |
| `./ctl.sh update` | Скачать образы и пересоздать контейнеры |
| `./ctl.sh backup` | Остановить стек, сделать tar, запустить обратно |
| `./ctl.sh shell ma` | Shell внутри MA |
| `./ctl.sh shell matter` | Shell внутри Matter Server |
| `./ctl.sh shell ha` | Shell внутри HA |
| `./ctl.sh doctor` | Диагностика без изменений |

`down` **не удаляет** `homeassistant/config`, `data/music-assistant`,
`data/matter-server`, `data/whisper` и `data/piper`.

## Версии

Версии зафиксированы в `.env`:

```dotenv
HA_IMAGE=ghcr.io/home-assistant/home-assistant:2026.10.4
MA_IMAGE=ghcr.io/music-assistant/server:2.10.8
MATTER_SERVER_IMAGE=ghcr.io/matter-js/matterjs-server:stable
```

Обновление — осознанно: изменить тег, затем

```bash
./ctl.sh backup
./ctl.sh pull
./ctl.sh up
```

Не переключать MA stable/beta простой сменой тега и не переносить базу между
stable и beta.

## HA Assist и голосовой контур

Текущие STT/conversation/TTS, preferred pipeline и wake word — в [STATE.md](../../../docs/STATE.md). Этот раздел не является вторым снимком состояния.

Установка пользовательского Piper-голоса, параметры `language/options.voice` и точечный откат описаны в [runbook Puriso](../tts/puriso/README.md). Отдельные `tts.speak` в scripts, в частности `alarm_announce`, проверяются независимо от выбора голоса Assist. Сохранить ESPHome endpoint будильника и local intents.

## Matter Server

Matter-контроллер HA живёт отдельным контейнером `matterjs-server`:
`ghcr.io/matter-js/matterjs-server:stable`, host network, данные —
`data/matter-server`. WebSocket: `ws://127.0.0.1:5580/ws`; dashboard:
`http://127.0.0.1:5580/` (только локально, при необходимости — SSH tunnel).
`PRIMARY_INTERFACE=wlp4s0` (Wi-Fi .25), `LISTEN_ADDRESS=127.0.0.1`.
Проверка: `./ctl.sh status`, `curl -I http://127.0.0.1:5580/`,
`docker logs --tail 50 matterjs-server`.

В HA: Settings → Devices & services → Add integration → Matter → URL
`ws://127.0.0.1:5580/ws`. Сама лента/лампа добавляется через HA Companion
(Android/iOS) по QR/11-значному коду; нужен 2.4 ГГц Wi-Fi, Bluetooth и
разрешение геолокации на Android. Если Matter Server перезапускается, fabric
сохраняется в `data/matter-server`.

## Music Assistant: статус

MA setup завершён: admin создан, Spotify provider подключён, Soloist
`1.3.10.13` установлен, web player спарен. Spotify playback проверен через
Web (Chrome on Mac): трек играл, прогресс шёл, пауза сработала. Pulsar уже
виден как Sendspin-плеер; HA integration MA добавлена. Spotify API 429
снят 2026-10-27, Connect search/play проверены; Liked Songs в MA
отдельно не перепроверялся. MA не перестраивать без причины.

## Music Assistant: что уже настроено

- admin-аккаунт MA;
- Spotify provider через **Soloist**;
- web player (Chrome) как тестовый Sendspin-плеер;
- MA UI и streamserver на правильном IP.

## Music Assistant: если переустанавливать

1. Открыть `http://192.0.2.25:8095` (или `.26`, если Ethernet подключён).
2. Создать admin-аккаунт MA.
3. Добавить **music provider**:
   - Spotify: для нового аккаунта — **Spotify Soloist**, не librespot;
   - локальная музыка: при необходимости примонтировать `/media` в compose.
4. Добавить **player provider**:
   - Pulsar/Sendspin — уже зарегистрирован;
   - для проверки без Pulsar — web player/desktop app MA.
5. Убедиться, что MA видит правильный локальный IP и плееры.
6. HA integration уже добавлена; при переустановке — добавить заново.

### Spotify

- Нужен Spotify Premium.
- Новый аккаунт (после декабря 2024): **Soloist**.
- librespot — только для старых аккаунтов; новый, скорее всего, не заиграет.
- Soloist: API key + pairing, lossless, но старт очереди медленнее; один стрим
  на аккаунт.
- Использование Soloist через MA — на свой риск с точки зрения Spotify ToS.
- Spotify Connect plugin — отдельная штука: делает плееры MA видимыми в Spotify
  app; для голосового «включи Spotify» нужен music provider.

## HA integration

**Статус:** добавлена 2026-10-26 (`music_assistant`, entry loaded).
Music Assistant integration — официальный компонент HA Core:

1. MA server должен быть уже настроен (музыка + плееры).
2. HA → **Settings → Devices & Services → Add Integration**.
3. Найти **Music Assistant**.
4. Указать `192.0.2.26:8095`.
5. Войти **admin-аккаунтом MA**.

После этого HA создаст `media_player`-сущности и actions:

- `music_assistant.play_media`
- `music_assistant.play_announcement`
- `music_assistant.transfer_queue`
- `music_assistant.search`
- `music_assistant.get_library`
- `music_assistant.get_queue`

Голосовой сценарий:

```text
Pulsar → HA Assist → LLM/script → music_assistant.play_media → MA → Pulsar
```

MA не делает wake word/STT/TTS. Это отдельные слои.

## Pulsar

Pulsar — это Sendspin-плеер, а не источник музыки.

```text
Music Assistant → Sendspin :8927 → Pulsar → динамики
```

Pulsar `v0.4.1-ricardo6` активен на `.160`; ESPHome + Sendspin, голос, TTS,
кнопки и голосовые команды проверены; HA integration MA добавлена,
Spotify Connect plugin включён; source router и volume sync работают.
Осталось: phone↔speaker test и Bluetooth-пара. `ctl.sh disable`
возвращает штатный стек. Детали — `../../docs/STATE.md`, команды —
`../../docs/OPERATIONS.md`.

## Backup и restore

```bash
./ctl.sh backup
```

Создаёт `backups/nettop-YYYYmmdd-HHMMSS.tar.gz` с:

- `docker-compose.yml`
- `.env` (если есть)
- `homeassistant/config`
- `data/music-assistant`
- `data/whisper`
- `data/piper`

Восстановление:

```bash
./ctl.sh down
sudo tar -xzf backups/nettop-YYYYmmdd-HHMMSS.tar.gz
./ctl.sh up
```

Для HA дополнительно делать штатные backups через UI и хранить вне сервера.
Для MOK/`/etc` — отдельный backup, см. `../TROUBLESHOOTING.md`.

## Сеть и безопасность

- HA, MA, Whisper и Piper — только LAN. Не пробрасывать `8123`/`8095`
  в интернет; `10200`/`10300`/`10305` тоже не публиковать.
- MA требует flat L2: без VLAN между MA и плеерами.
- MA нельзя за Docker bridge, если нужны discovery/streaming.
- HA Container работает с `privileged: true`; MA — без лишних привилегий.
- SMB/NFS лучше монтировать на хосте и пробрасывать read-only, а не давать MA
  `SYS_ADMIN`.
- `.env` может содержать только версии/TZ; секреты Spotify/HA — в данных MA/HA,
  не в git.
- Не публиковать `data/music-assistant` и `homeassistant/config` в git.

## Диагностика

```bash
./ctl.sh status
./ctl.sh doctor
./ctl.sh logs ma
docker compose ps
ss -lntp | grep -E ':(8123|8095|8097|8927)\b'
```

Если MA UI не открывается:

1. `./ctl.sh ps` / `status` — контейнер Up?
2. `./ctl.sh logs ma` — ошибка старта?
3. `ss -lntp | grep 8095` — слушает?
4. `ip -br addr` — сеть/маршрут?
5. Проверить, что контейнер запущен с `network_mode: host`.

Если HA не открывается:

1. `./ctl.sh logs ha`
2. `curl -v http://127.0.0.1:8123/`
3. Проверить `homeassistant/config/configuration.yaml`
4. При необходимости восстановить config из backup.

## Дальше

1. Bluetooth-пара телефона (4 нажатия/голос; возможен Wi-Fi drop).
2. Spotify Connect уже играет; MA-ветка Liked Songs ждёт sync.
3. Custom wake word «Рикардо».
4. Spotify sync и voice → любимый плейлист.
5. LLM-ключ — параллельно, для свободной речи/старого emulator.

Полный план — `../../docs/PENDING.md`; контекст — `../../docs/CONTEXT.md`.

## Границы ответственности

- **HA** — автоматизация, Assist, интеграции.
- **MA** — музыка, Spotify, очередь, плееры (2.10.8, Spotify подключён).
- **Pulsar** — уши/рот колонки.
- **Whisper/Piper** — STT/TTS (активны).
- **LLM** — свободная речь (позже).

Эта папка — единая точка входа для HA и MA. Новые сервисы добавляются сюда же:
`docker-compose.yml` + команда в `ctl.sh` + раздел в README.

## Пользовательский голос Piper

Установка закреплённых весов Puriso, выбор голоса и откат: `../tts/puriso/README.md`. Текущий voice/status — `../../docs/STATE.md`. Веса нужно установить до выбора пользовательского PIPER_VOICE.
