# SECURITY — политики и риски

Факты — `STATE.md`; приватные пути — `SENSITIVE-DATA.md`.

## Границы

- Стенд только для своих устройств в своей сети.
- Не публиковать `secrets/`, ключи, архивы, HA auth, логи с токенами.
- Не пробрасывать HA/MA/ADB/Zigbee в интернет.
- Публичный доступ — только Tailscale Operator.

## Сеть

- HAOS `.28`: `8123`, `8095/8097/8927` — LAN/Tailscale, login/MFA.
- K3s `.27`: `30300` Grafana — LAN/Tailscale, login; `8643` — только
  HAOS/K3s/loopback; `6443/10250/9100/9116/8472` — firewall.
- Станция `.13`: `6053`, `8928`, `8099`, `5555`, `22` — только LAN.
  `8099` — bearer token; ADB root без пароля; SSH root по паролю.
- `6638` Zigbee — без auth; сейчас закрыт/не используется, наружу не выставлять.
- ONT/AX3: входящие порты не открывать; WAN `22/443` проверить/закрыть.

## VPN

- ocserv в CT 102: `443/tcp` + `443/udp`, TLS 1.2+, без compression,
  `isolate-workers`, встроенный бан.
- Два способа входа: пароль + TOTP (основной резервный путь) и клиентский
  сертификат частного CA (без пароля/OTP). Сертификат — `auth = "certificate"`,
  пароль + TOTP — `enable-auth = "plain[...]"`.
- Приватный ключ CA (RSA 4096, 10 лет) хранится только на рабочей машине в
  `secrets/home/vpn/client-ca/`; на CT попадают только `client-ca.pem` и
  `client-ca.crl`. Клиентские сертификаты — RSA 2048, 2 года, EKU clientAuth,
  CN = пользователь.
- Отзыв: `vpncerts.py revoke` обновляет CRL, кладёт его в CT и перезагружает
  ocserv; отозванный сертификат больше не проходит проверку.
- Профили `lan` (только домашняя LAN) и `full`; группы берутся из OU
  сертификата (`cert-group-oid`), по умолчанию `lan`. Пул `10.77.0.0/24`,
  masquerade в CT.
- Сейчас сервер доступен только из LAN. После проброса на ONT/AX3 наружу
  открыт только `443`; SSH в CT остаётся только из LAN.
- Серверный сертификат — публичный Let's Encrypt с автоматическим продлением.
  Приватные пароли/TOTP/QR и ключ клиентского CA — только в `secrets/home/vpn/`.

## Egress guard

- Разрешены LAN, loopback, DHCP, multicast, Google NTP; публичный WAN —
  `DROP`; Яндекс — отдельный block.
- Private LAN считается доверенной; для строгого режима сузить allowlist.
- Снятие guard не снимает Яндекс-блок/OTA.

## Секреты

`secrets/` — 700, файлы 600. Реестр — `SENSITIVE-DATA.md`.

- `secrets/NETWORK.md` — реальные IP/порты/Tailscale.
- `secrets/CREDENTIALS.md` — SSH/ADB/сервисные пароли.
- `secrets/certs/` — приватные сертификаты/ключи.
- QR/DPP отключён; прежний материал — только в private recovery archive.
- `secrets/kubernetes-research/` — kubeconfig, values, Grafana password,
  recovery OCI/SQLite.
- Hermes secrets — Kubernetes Secret + writable home.
- BLE token — `/data/quasar/ble-gateway/token`; HTTP control token —
  `/data/quasar/ble-gateway/control-token`; HA token — `secrets.yaml`.
- MOK private key на PVE — root-only; без него DKMS Wi-Fi не загрузится.
- Wi-Fi profile — 0600, вне Git.

Не коммитить, не синхронизировать без шифрования, не печатать в чат.

## HA / MCP / LLM

- MCP-токены — минимальные права; опасные действия — с подтверждением.
- Hermes видит распознанный текст; для приватности — локальная модель.
- Hermes не владеет alarm/timer state; при его падении локальные команды
  должны работать.
- MA MCP experimental; full access включён осознанно.

## Secure Boot / host

- Secure Boot включён; Wi-Fi DKMS подписан MOK.
- Secure Boot PK остаётся `DO NOT TRUST - AMI Test PK`; менять только с
  физдоступом и планом.
- `labuser` с sudo NOPASSWD — root-эквивалент; убрать при многопользовательском
  доступе.

## Логи / публикация

- Логи стека/станции могут содержать IP, SSID, device_id, URL.
- Перед публикацией: `make test`, `SENSITIVE-DATA.md`, проверка истории Git.
- Публичные docs используют `192.0.2.x`; реальные значения — только `secrets/`.
- История и старые research/plan документы удалены из рабочего дерева;
  при публикации проверять всю Git-историю, а не только HEAD.

## Обновления / recovery

- OTA на станции заблокирован: `updatesd` удалён, `ota_script.pub` подменён,
  `device_ota_update.sh` — no-op.
- Factory reset сносит `/data`, root/SSH/guard/OTA; проверенного recovery-образа
  Midi нет.
- Не считать OTA-блок аппаратной защитой; raw boot/recovery не сохранён.

## Что не делать

- Mini 2/Light downgrade и rollback — только private archive (`archive/legacy/mini2/`); не запускать на Midi.
- Не менять `ota_script.pub`, пока обновление ещё нужно.
- Не запускать p2p0-эксперимент при рабочей сети.
- Не делать `setenforce 0`, не оставлять rootfs `rw`, не запускать `curl | sh`.
- Не выставлять `31116` в LAN; stock `quasar-server` API без auth. Сейчас
  stock-стек остановлен (работает Pulsar); при возврате закрыть порт.
