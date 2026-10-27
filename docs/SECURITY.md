# SECURITY — политики и риски

Политики, а не текущий статус. Факты — `STATE.md`; задачи — `PENDING.md`.

## Юридическое / этическое

- Стенд использует приватный ключ intermediate-CA eMudhra из публичной утечки;
  применять только к **своей** колонке в **своей** сети.
- Не публиковать `secrets/`, приватные ключи, архивы и логи с токенами.
- Не направлять на стенд чужие устройства и не выставлять порты в интернет.

## Сеть

| Порт | Где | Доступ | Риск |
|---|---|---|---|
| `53/udp` | Mac | LAN | при падении DNS ломается резолв |
| `80/443` | Mac | LAN | MITM-стенд, eMudhra |
| `31116` | Mac | LAN | API push/config без auth |
| `22` | станция | LAN | root по паролю |
| `5555` | станция | LAN | ADB root без пароля |

- `22/5555` не пробрасывать наружу, не держать в гостевой сети.
- `31116` закрыть bind `127.0.0.1`/auth — P0.
- SSH — ключи вместо пароля; пароль только в `secrets/CREDENTIALS.md`.

## Egress-guard

- Разрешены private LAN, loopback, DHCP, multicast и Google NTP; публичный
  IPv4/IPv6 WAN — `DROP`.
- Яндекс блокируется по IP, а не по DNS; DNS-off не ослабляет карантин.
- Private LAN считается доверенной: устройство в ней может быть прокси.
  Для строгого режима сузить allowlist до Mac + admin-IP.
- Google NTP — единственное WAN-исключение; можно убрать.
- Откат guard не снимает Яндекс-блок/OTA.

## Секреты

Реестр путей/типов (без значений) — [`SENSITIVE-DATA.md`](SENSITIVE-DATA.md).
Особое внимание: `projects/station/configs/stack.env` (`LLM_API_KEY`), `secrets/dpp` QR,
`backups/*` (HA `secrets.yaml`/auth), `artifacts/prior-session`.

`secrets/` (chmod 700), внутри:

- `dpp/` — DER-ключ, bootstrap URI/QR (доступ к Wi-Fi-провижинингу);
- `certs/*.key`, `emudhra-ca.key` — ключи eMudhra/leaf;
- `CREDENTIALS.md` — SSH/сервисные пароли.

Не коммитить, не синхронизировать в облако без шифрования, не пересылать.
`secrets/dpp` и sound-код нельзя публиковать: это доступ к сети/провижинингу.
Токен BLE gateway — `/data/quasar/ble-gateway/token` (600): это доступ к
смене Wi-Fi; не публиковать. BLE-окно 15 минут после физического нажатия;
авто-режим только когда Wi-Fi нет.

## Голос / LLM / HA / MCP

- Старый `LLM_API_KEY` из v3rt3p — в `secrets/legacy-v3rt3p/.env` (600),
  не использовать и не публиковать; стек удалён.
  Облачный LLM видит распознанный текст; для приватности — локальная модель.
- HA/MCP/Spotify — только LAN, токены в `secrets/`; MCP-инструменты давать
  минимально необходимые, опасные действия (замки, сигнализация) — с
  подтверждением.
- Официальный HA MCP Server — OAuth; неофициальные MCP — long-lived token,
  ограничить ACL/сеть.
- Pulsar ESPHome API `:6053`: HA уже записала Noise-ключ; не удалять
  `/data/pulsar/api_key` без переавторизации устройства. Без ключа API был
  бы открыт всей LAN.
- HTTP control станции `:8099` (gateway/Bluetooth): bearer token 64 hex в
  `/data/quasar/ble-gateway/control-token` (0600) и HA `secrets.yaml`;
  endpoint только LAN, не публиковать. Без токена — 401.
- Zigbee-координатор Pulsar :6638 без auth (как сетевые координаторы);
  не выставлять в недоверенную сеть.
- Custom wake word модель — это код/данные, проверять источник и ложные
  срабатывания; не путать с заменой `words.txt` (акустику это не меняет).

## Неттоп / HA / Secure Boot

- Неттоп `lab-host`: HA `:8123`, Music Assistant `:8095`, Whisper `:10300`,
  Piper `:10200` — только LAN; не пробрасывать. MA/STT/TTS в host network.
  `network_mode: host`, `privileged` — компромисс для DBus/интеграций.
- Secure Boot включён; DKMS Wi-Fi подписан MOK. Приватный MOK-ключ
  `/var/lib/shim-signed/mok/MOK.priv` — root-only, забэкапить зашифрованно.
  Без него будущие DKMS-модули не загрузятся при Secure Boot.
- `labuser` в группе `docker` = root-эквивалент; `sudo` NOPASSWD — удобство,
  не новая степень доступа. Убрать, если сервер станет многопользовательским.
- HA MFA/Tailscale remote access работает; публичного HA нет, `8123` не
  проброшен. Удалённо — только Tailscale/Serve; не port-forward 8123.
  ONT WAN `22/443` — проверить/закрыть remote management.
- Wi-Fi PSK — в `secrets/`; не в `projects/home-assistant/`.

## Логи

- Старый `alice-mitm-bootstrap` архивирован без `logs/` и `.env`
  (`archive/legacy/alice-mitm-bootstrap/`); если восстанавливать его
  отдельно, старые `logs/requests.jsonl` могли содержать OAuth-токены,
  device_id и пути — считать чувствительными. Recovery — `REBUILD.md` §4.
- Логи стека содержат трафик станции; DPP event-логи — SSID/BSSID/IP.
- Не публиковать; при передаче вычищать.

## Обновления и recovery

- OTA-запрет: `updatesd` вырезан, `ota_script.pub` подменён,
  `device_ota_update.sh` no-op, s3/cdn `403`.
- Factory/recovery на Midi не проверен; проверенного factory-образа нет.
  Сброс сносит `/data` (keeper, SSH, guard, ключи) и не гарантирует снятие
  серверного лока/`stolen`.
- Raw recovery не сохранён; не считать OTA-блок аппаратной защитой.

## Целостность

- Корень `/` — `ro`; изменения требуют `remount,rw`.
- `verifiedbootstate=green`; модификация system tolerated, но не злоупотреблять.
- Перед изменениями — бэкап оригиналов в `/data/opt/backup`.

## Что не делать

- Не запускать `rollback.sh` от Mini 2: другие разделы/SoC, риск кирпича.
- Не применять `yandex-micro-kit.sh` без сверки маркеров.
- Не менять `ota_script.pub`, пока обновление ещё нужно.
- Не запускать `curl | sh`, не делать `setenforce 0`, не оставлять rootfs `rw`.
- Не запускать p2p0-эксперимент при рабочей сети: он роняет `wlan0`.
