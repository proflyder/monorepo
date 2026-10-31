# SECURITY — политики и риски

Факты — `STATE.md`; приватные пути — `SENSITIVE-DATA.md`.

## Границы

- Стенд только для своих устройств в своей сети.
- Не публиковать `secrets/`, ключи, архивы, HA auth, логи с токенами.
- Не пробрасывать HA/MA/ADB/Zigbee в интернет.
- Публичный доступ — собственный VPN (ocserv) и единственный webhook
  currency-bot через Cloudflare Tunnel; больше наружу ничего не публикуется.

## Сеть

- HAOS `.28`: `8123`, `8095/8097/8927` — LAN/VPN, login/MFA.
- K3s `.27`: `30300` Grafana — LAN/VPN, login; `8643` — только
  HAOS/K3s/loopback; `6443/10250/9100/9116/8472` — firewall.
- currency-bot: `8081` webhook — только cloudflared, `8080` API/metrics —
  кластер/узел/LAN и Traefik; API `currency.home.example.com` — LAN/VPN,
  Bearer. Публично только `tg.example.com/telegram/webhook` через туннель.
- Станция `.13`: `6053`, `8928`, `8099`, `5555`, `22` — только LAN.
  `8099` — bearer token; ADB root без пароля; SSH root по паролю.
- `6638` Zigbee — без auth; сейчас закрыт/не используется, наружу не выставлять.
- ONT/AX3: проброшен только VPN `443/tcp+udp`; внешний скан 2026-10-31: из
  WAN открыт лишь `443/tcp` (`22`, `80`, `8006`, `8443` закрыты).

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
- Группа `ci` для GitHub Actions изолирована: пул `10.77.1.0/24`, маршрут
  только до K3s, без DNS; nftables `forward` разрешает только
  `tcp/{443,6443}` на K3s (443 — Traefik/OpenBao OIDC, 6443 — API) и `drop`
  остальное. Сертификат CI не даёт доступа к HA, PVE, станции или другим
  LAN-адресам.
- Сервер проброшен на ONT/AX3: снаружи открыт только `443/tcp+udp`; SSH в CT
  остаётся только из LAN. Лимит `max-same-clients = 10` на пользователя.
- Серверный сертификат — публичный Let's Encrypt с автоматическим продлением
  (certbot, DNS-01 Cloudflare). Приватные пароли/TOTP/QR и ключ клиентского
  CA — только в `secrets/home/vpn/`.

## Cloudflare Tunnel

- Remotely-managed tunnel `home-k3s` запущен как Deployment `cloudflared` в
  namespace `currency-bot`; он только исходящий: `7844/udp` (QUIC) +
  `443/tcp` fallback и DNS. Входящих портов на роутере не добавляет.
- Публичный маршрут один: `tg.example.com` путь `^/telegram/webhook$` →
  `currency-bot-webhook:8081`; остальные пути — 404. Туннель не даёт доступа
  к HA, API, Grafana, OpenBao или K3s API.
- Webhook проверяет `X-Telegram-Bot-Api-Secret-Token` (constant time) и
  allowlist `CHAT_ID`; без секрета — 401. После успешной аутентификации
  всегда 200, чтобы Telegram не ретраил и не повторял `/trigger`.
- `TUNNEL_TOKEN` — только OpenBao `k8s/cloudflared` → ESO Secret
  `cloudflared-tunnel`; Cloudflare API token и tunnel credentials — в
  `secrets/currency-bot/`. Токены не логировать.
- `cloudflared` egress ограничен NetworkPolicy; webhook ingress — только от
  подов cloudflared.

## Egress guard

- Разрешены LAN, loopback, DHCP, multicast, Google NTP; публичный WAN —
  `DROP`; Яндекс — отдельный block.
- Private LAN считается доверенной; для строгого режима сузить allowlist.
- Снятие guard не снимает Яндекс-блок/OTA.

## Секреты

`secrets/` — 700, файлы 600. Реестр — `SENSITIVE-DATA.md`.

- `secrets/NETWORK.md` — реальные IP/порты/VPN/HTTPS.
- `secrets/CREDENTIALS.md` — SSH/ADB/сервисные пароли.
- `secrets/certs/` — приватные сертификаты/ключи.
- QR/DPP отключён; прежний материал — только в private recovery archive.
- `secrets/kubernetes-research/` — kubeconfig, values, Grafana password,
  recovery OCI/SQLite.
- `secrets/currency-bot/` — VPS ssh key/config, экспорт H2 CSV/SQLite,
  tunnel token, Cloudflare tunnel API token, `k8s.env`;
  `secrets/home/currency-bot.env`.
- Hermes secrets — Kubernetes Secret + writable home.
- BLE token — `/data/quasar/ble-gateway/token`; HTTP control token —
  `/data/quasar/ble-gateway/control-token`; HA token — `secrets.yaml`.
- MOK private key на PVE — root-only; без него DKMS Wi-Fi не загрузится.
- Wi-Fi profile — 0600, вне Git.

Не коммитить, не синхронизировать без шифрования, не печатать в чат.

## OpenBao

- UI/API — `https://vault.<home-suffix>/`, только LAN/VPN через Traefik и
  `home-allowlist`; listener внутри кластера без TLS.
- Auto-unseal — встроенный `static` seal: 32-байтный ключ в Kubernetes Secret
  `openbao-seal` на том же узле, что и raft-данные. Это компромисс ради
  перезагрузки без человека: компромисс узла даёт и ключ, и данные. Ключ не
  копируется в snapshot; при появлении внешнего KMS/HSM/transit перейти на
  него (`docs/PENDING.md`).
- Recovery keys (5/3) и начальный root-токен — только
  `secrets/openbao/init.json` (600). Root-токен после bootstrap отозван;
  новый выпускается процедурой `generate-root` из recovery keys. Recovery
  keys хранить офлайн/в менеджере паролей, не на узле.
- Человек: `userpass` (имя — `secrets/openbao/username`) → политика `admin` с запретом
  `sys/seal`, `sys/step-down`, `sys/raw/*`, `sys/replication/*`,
  `sys/rotate`, `sys/rekey*`. Вход человека — пароль + обязательный
  TOTP (enforcement на `auth/userpass` с 2026-10-31).
- CI: `jwt` mount `github`, только GitHub OIDC (без статических токенов):
  issuer `https://token.actions.githubusercontent.com`, audience = адрес
  OpenBao, claims `repository`, `environment=Home`, `ref=refs/heads/develop`,
  TTL 10/15 мин, политика `ci-deploy` — read только `secret/ci/test` и
  `secret/ci/deploy-home`.
- Поды: `kubernetes` auth через TokenReview (`system:auth-delegator` на SA
  `openbao`); политика `k8s-read` — read только `secret/k8s/*`. Роль
  `external-secrets` используется External Secrets Operator (namespace
  `external-secrets`), который создаёт Secrets приложений из OpenBao.
- Audit — декларативный файловый девиз на отдельном PV; лог не публикуется и
  может содержать пути/metadata запросов.
- Backup — raft snapshot в локальный PV на том же узле; offsite-копия пока
  открытый пункт, восстановление на отдельном стенде не проверялось.
- Приложенческие Secrets (`scheduler-auth`, `ha-bridge-ha`,
  `hermes-environment`, `grafana-admin`, `cloudflare-api-token`,
  `currency-bot`, `cloudflared-tunnel`) перенесены в
  `secret/k8s/*`; источник правды — OpenBao, Kubernetes Secrets создаёт ESO.
  Локальные копии в `secrets/` — бэкап/bootstrap, не используются деплоем.
- Не переносятся: `openbao-seal` (нужен до старта OpenBao), helm release
  Secrets, SA token Secrets, TLS, выпускаемые cert-manager, служебные Secrets
  контроллеров.

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
- CI не имеет SSH и root на узлах: группа `ci` маршрутизируется только до
  `K3S_NODE`, VPN nft разрешает только `tcp/{443,6443}` (443 — Traefik для
  OpenBao OIDC, 6443 — API), а узел принимает `6443` лишь с адреса VPN-CT
  (пул `ci` маскируется в него). Пользователь `ci-deploy` и root-helper
  удалены с узла.
- Kubeconfig CI — ServiceAccount `ci-deploy` только в `home-assistant`
  (Role, не ClusterRole): нет других namespace и cluster-scoped объектов;
  добавлены только `batch/jobs` для in-cluster preflight. API доступен
  напрямую по `https://<K3S_NODE>:6443` (IP узла в SAN сертификата).
- Образы неизменяемы и адресуются по digest; GHCR-пакеты должны быть
  публичными (pull без секрета), push выполняет job-scoped `GITHUB_TOKEN`
  с `packages: write` только в deploy-джобе.
- Секреты Environment `Home` (VPN p12/пароль, `OPENBAO_ADDR`, `OPENBAO_ROLE`)
  загружаются primary вручную через `gh`; в Git их нет. Всё, что нужно только
  после входа в OpenBao (`kubeconfig`, `scheduler.env`, runtime values),
  хранится в `secret/ci/deploy-home`, читается по GitHub OIDC ролью
  `deploy-home` (политика `ci-deploy`: read только `secret/ci/test` и
  `secret/ci/deploy-home`) и материализуется в файлы 600 с маскированием
  значений; токен отзывается в `if: always()`. HA/scheduler токены CI больше
  не нужны: preflight выполняется Job-ом в кластере.

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
