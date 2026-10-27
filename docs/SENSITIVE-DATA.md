# SENSITIVE-DATA — реестр чувствительных данных

**Дата:** 2026-10-27  
**Статус:** локальные приватные данные собраны под `secrets/` (2026-10-27).
Физические пути перечислены ниже; старые пути сохранены игнорируемыми symlink.
`secrets/` имеет режим 700, приватные файлы — 600 (исполняемые — 700).
Ниже — пути, типы и риски; секретов в этом файле нет.

> Правило: значения хранятся только в `secrets/`/HMAC-хранилище; в docs/чатах/
> git — имена переменных, не значения. Перед публикацией репозитория/логов
> вычищать. При утечке — ротация, а не «удалить файл».

## Хранение после подготовки к публикации

- `secrets/local/backups/`, `logs/`, `archive/`, `snapshots/` — recovery, логи, история стенда.
- `secrets/local/artifacts/` — локальные архивы, аудио, pcap, прошивки и сборки;
  в Git остаются README, компактная модель и разрешённые метаданные wakeword.
- `secrets/local/projects/station/configs/*.env` — env станции.
- `secrets/local/projects/station/tools/quasar-root-kit/leaf.key` — ключ root kit.
- `secrets/local/projects/home-assistant/ha-voice/www/alarm_ringtone.wav` — пользовательский трек;
  исключён из текущего дерева Git, живая HA-конфигурация не менялась.
- `secrets/relocation-20261027.json` — локальный список переносов без содержимого файлов.

Прежние пути в таблицах ниже — совместимые локальные ссылки. Их нет в чистом
checkout: для восстановления нужны отдельная приватная копия и секреты.
Вложенные upstream checkout’ы не публикуются; их индекс и история не менялись.

## 1. Высокий риск

| Путь | Что | Почему | Режим |
|---|---|---|---|
| `secrets/CREDENTIALS.md` | SSH/ADB/сервисные пароли, IP/MAC, fingerprint | прямой доступ к станции/сети | 600 |
| `secrets/certs/emudhra-ca.key` | приватный ключ intermediate-CA eMudhra | подпись leaf/доверие станции | 600 |
| `secrets/certs/quasar-uniproxy.key` | приватный ключ stage 2 | MITM Quasar/uniproxy | 600 |
| `secrets/certs/quasar.yandex.net.key` | приватный ключ bootstrap leaf | stage 1 ADB | 600 |
| `secrets/dpp/bootstrap.der`, `.der.hex`, `uri.txt`, `qr*.png`, `p2p0-*` | DPP bootstrap/QR | доступ к Wi-Fi-провижинингу | часть 644 |
| `secrets/legacy-alice-mitm/.env*` | env Quasar proxy/DNS | IP/порты/пути, возможно токены | 600 |
| `secrets/legacy-v3rt3p/.env` | LLM base/key/model, TTS voice | API-ключ/аккаунт | 600 |
| `projects/station/configs/stack.env` | `LLM_API_KEY`, `LLM_BASE_URL`, model | API-ключ | **644** |
| `~/.hermes/.env` (lab-host) | Hermes secrets | `HERMES_HA_TOKEN`, `HERMES_MA_TOKEN`, `API_SERVER_KEY` | 600 |
| `archive/legacy/alice-mitm-bootstrap/certs/*.key` | bootstrap/local leaf keys | MITM/ADB | 600 |
| `archive/legacy/v3rt3p-stack/repos/https-proxy/certs/ca.key` | local CA private key (из upstream) | подпись leaf | **644** |
| `projects/station/tools/quasar-root-kit/leaf.key` | leaf private key | root kit | 600 |
| `artifacts/desktop-originals/alice-mitm-bootstrap-20261026.tar.gz` | архив Quasar kit | внутри ключи/env | 600 |
| `artifacts/alice-mitm-bootstrap-2026-10-21.tar.gz` | bootstrap kit | ключи/сертификаты | 600 |
| `artifacts/prior-session/codex-session-*.md` | транскрипт прошлой сессии | может содержать OAuth/токены/device_id | 600 |
| `backups/remote-access-20261026/nettop-*.tar.gz` | полный backup неттопа | HA `secrets.yaml`, auth, MA | **644** |
| `backups/maintenance-20261027/` | pre-update backup неттопа + `/etc` | HA secrets, `.env`, SSH host keys/shadow | 600 |
| `backups/emergency-20261023-2137/` | полный lab/station snapshot | secrets/ключи/конфиги | каталог |
| `backups/station-data-pre-pulsar-*.tar.gz` | `/data` станции | SSH/токены/маркеры | 600 |
| `backups/station-etc-pre-pulsar-*.tar.gz` | `/etc` станции | hosts/chrony/unbound | 600 |
| `archive/logs/2026-10/status-20261025-0455-token.txt` | лог с токеном в имени/содержимом | утечка токена | 644 |

## 2. Средний риск

| Путь | Что | Почему |
|---|---|---|
| `backups/station-originals-20261023/` | оригиналы station-файлов | могут содержать маркеры/пути, не ключи |
| `backups/pre-telemetry-20261023-2223/` | старые compose/nginx/launch | конфиги/пути; проверить токены |
| `backups/egress-20261024-050346/` | iptables/onboot/keeper | карта сети/правил |
| `logs/*.txt`, `archive/logs/2026-10/*` | статусы/диагностика | IP, URL, иногда токены; не публиковать как есть |
| `artifacts/pcap/station.pcap` | TLS-захват | metadata/SNI; не содержит plaintext, но чувствителен |
| `projects/station/configs/alice-mitm*.env` | stage env | IP/порты/пути; не секрет, но карта стенда |
| `projects/station/configs/station-applied/` | launch/keeper/OTA | карта защиты; не ключи |
| `artifacts/desktop-originals/TTS-voices-20261026.zip` | TTS-сэмплы | не секрет; голосовые данные |
| `artifacts/firmware/yandexmidi…zip` | официальная прошивка Station 2 | не секрет; юридические/риски прошивки |

## 3. Низкий риск / публичное

- `projects/station/configs/*.crt` — публичные сертификаты (leaf/intermediate/CA cert).
- `projects/station/configs/station-applied/ota_script.pub` — публичный ключ.
- `backups/tailscale-acl-20261026/` — ACL-политика, не секрет.
- `resources/` — сторонний open-source; лицензии/атрибуция.
- `artifacts/ssh/dropbear*` — бинарники, не ключи.
- `docs/` — проверить на токены перед публикацией; сейчас значения не найдены.

## 4. Замечания по режимам

На 2026-10-27 перечисленные локальные данные перенесены под закрытый `secrets/`.
Числа 644 в таблице выше описывают состояние до переноса. Режимы исправлены.
Исторические замечания:

- `projects/station/configs/stack.env` — **644**, содержит `LLM_API_KEY` → `600` или перенести
  значение в `secrets/`/HMAC.
- `backups/remote-access-20261026/*.tar.gz` — **644**, содержит HA secrets →
  `600` + шифрование.
- `secrets/dpp/*.png`/`uri.txt` — **644**; это доступ к провижинингу → `600`.
- `archive/legacy/v3rt3p-stack/repos/https-proxy/certs/ca.key` — **644**;
  это ключ из upstream, но обращаться как с ключом → `600`.
- `logs/*` — **644**; перед передачей вычищать токены/URL.
- `backups/remote-access` — не offsite: нужна шифрованная копия вне Mac.

## 5. Правила

1. Не публиковать `secrets/`, `backups/`, `artifacts/prior-session`,
   `artifacts/*alice-mitm*`, `projects/station/tools/quasar-root-kit/*.key`,
   `archive/legacy/**/certs/*.key`.
2. Не вставлять значения в docs/чат/issue; только имена.
3. Для offsite — шифрованный архив (age/GPG/Borg) + отдельный пароль.
4. При потере устройства/утечке: ротация SSH/ADB/BLE/DPP/LLM/Tailscale-ключей;
   отзыв HA refresh tokens; смена паролей.
5. Перед публикацией репозитория — прогнать `platform/quality/test_static.py`
   (secret patterns) и глазами проверить `logs`/`artifacts`.
