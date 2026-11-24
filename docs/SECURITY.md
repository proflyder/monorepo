# SECURITY — политики и риски

Факты — `STATE.md`; сеть — `NETWORK.md`; пути OpenBao — `SECRETS.md`.
Реестр приватных данных — раздел «Приватные данные» ниже.

## Границы

- Стенд только для своих устройств в своей сети.
- Не публиковать `secrets/`, ключи, архивы, HA auth, логи с токенами.
- Не пробрасывать HA/MA/ADB/Zigbee в интернет.
- Публичный доступ — собственный VPN (WireGuard, ocserv), webhook currency-bot и
  read-only `/repository/*` Nexus через Cloudflare Tunnel; больше наружу
  ничего не публикуется.

## Сеть

- HAOS `.28`: `8123`, `8095/8097/8927` — LAN/VPN, login/MFA.
- K3s `.27`: `30300` Grafana — LAN/VPN, login; `8643` — только
  HAOS/K3s/loopback; `6443/10250/9100/9116/9118/8472` — firewall.
- currency-bot: `8081` webhook — только cloudflared, `8080` API/metrics —
  кластер/узел/LAN и Traefik; API `currency.home.example.com` — LAN/VPN,
  Bearer. Публично только `tg.example.com/telegram/webhook` и
  `ci-nexus.<public-zone>/repository/*` через туннель.
- Станция `.13`: `6053`, `8928`, `8099`, `9101`, `5555`, `22` — только LAN.
  `8099` — bearer token; `9101` — метрики, доступ только с адреса
  Prometheus; ADB root без пароля; SSH root по паролю.
- `6638` Zigbee — без auth; сейчас закрыт/не используется, наружу не выставлять.
- ONT/AX3: пробрасываются только порты VPN; `22`, `80`, `8006`, `8443` из WAN
  закрыты.

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
  только до K3s, без DNS; nftables `forward` разрешает только `tcp/443`
  (Traefik → HA для установки прошивки станции) и `drop` остальное. K3s API
  (`6443`), SSH, PVE, станция и другие LAN-адреса недоступны. Порт `443` и
  группа `ci` нужны сертификатам `ci-pulsar` (`deploy-station`) и
  `ci-android` (`android-distribute`).
- SSH в CT — только из LAN. Лимит `max-same-clients = 10` на пользователя.
- Серверный сертификат — публичный Let's Encrypt с автоматическим продлением
  (certbot, DNS-01 Cloudflare). Приватные пароли/TOTP/QR и ключ клиентского
  CA — только в `secrets/home/vpn/`.

## Cloudflare Tunnel

- Remotely-managed tunnel `home-k3s` запущен как Deployment `cloudflared` в
  namespace `currency-bot`; он только исходящий: `7844/udp` (QUIC) +
  `443/tcp` fallback и DNS. Входящих портов на роутере не добавляет.
- Публичных маршрута два: `tg.example.com` путь `^/telegram/webhook$` →
  `currency-bot-webhook:8081` и `ci-nexus.<public-zone>` путь `^/repository/`
  → `nexus.nexus.svc:8081` (read-only `ci-reader`); остальные пути — 404.
  Туннель не даёт доступа к HA, API, Grafana, OpenBao, K3s API или админке
  Nexus.
- Webhook проверяет `X-Telegram-Bot-Api-Secret-Token` (constant time) и
  allowlist `CHAT_ID`; без секрета — 401. После успешной аутентификации
  всегда 200, чтобы Telegram не ретраил и не повторял `/trigger`.
- `TUNNEL_TOKEN` — только OpenBao `k8s/currency-bot/cloudflared-tunnel` → ESO Secret
  `cloudflared-tunnel`; Cloudflare API token и tunnel credentials — в
  `secrets/currency-bot/`. Токены не логировать.
- `cloudflared` egress ограничен NetworkPolicy: webhook — `currency-bot-webhook:8081`,
  CI-кэш — `nexus.nexus.svc:8081`; webhook ingress — только от подов
  cloudflared.

## Egress guard

- Разрешены LAN, loopback, DHCP, multicast, Google NTP и `tcp/443` к
  публичным IPv4-диапазонам GitHub (updater, `ALLOW_GITHUB=1`); остальной
  публичный WAN — `DROP`; Яндекс — отдельный block.
- Private LAN считается доверенной; для строгого режима сузить allowlist.
- Снятие guard не снимает Яндекс-блок/OTA.

## Секреты

`secrets/` — 700, файлы 600. Реестр — «Приватные данные» ниже.

- `secrets/NETWORK.md` — реальные IP/порты/VPN/HTTPS (копия в OpenBao
  `secret/infra/docs/network`).
- Пароли, ключи и сертификаты — только OpenBao (`docs/SECRETS.md`); локально
  `secrets/openbao/{addr,operator.token}` и `~/.ssh/home-lab/`.
- currency-bot: VPS key/config, tunnel/API tokens, env — OpenBao
  `secret/infra/{ssh/currency-bot-vps,currency-bot/vps,cloudflare/currency-bot-tunnel}`;
  экспорт SQLite/CSV — `local-data/currency-bot/`.
- Hermes secrets — Kubernetes Secret + writable home.
- BLE token — `/data/quasar/ble-gateway/token`; HTTP control token —
  `/data/quasar/ble-gateway/control-token`; HA token — `secrets.yaml`.
- MOK private key на PVE — root-only; без него DKMS Wi-Fi не загрузится.
- Wi-Fi profile — 0600, вне Git.

Не коммитить, не синхронизировать без шифрования, не печатать в чат.

## Vaultwarden

- `https://passwords.<home-suffix>/`, только LAN/VPN (`home-allowlist`).
- Регистрация закрыта, вход — мастер-пароль владельца (+2FA в аккаунте).
  Хранилище шифруется на клиенте; сервер видит только шифротекст.
- `/admin` включён; в поде только Argon2id-хэш токена, пароль — в OpenBao
  `secret/infra/vaultwarden/admin`.
- NetworkPolicy: вход только с узла/LAN на 8080, выход только DNS — сервер не
  может ничего отправить наружу.

## OpenBao

- UI/API — `https://vault.<home-suffix>/`, только LAN/VPN через Traefik и
  `home-allowlist`; listener внутри кластера без TLS.
- Auto-unseal — встроенный `static` seal: 32-байтный ключ в Kubernetes Secret
  `openbao-seal` на том же узле, что и raft-данные. Это компромисс ради
  перезагрузки без человека: компромисс узла даёт и ключ, и данные. Ключ не
  копируется в snapshot. Риск принят владельцем, KMS не
  планируется; в offsite-бэкапе ключ и raft-снапшоты хранятся раздельно.
- Recovery keys (5/3) и seal key — у владельца в Bitwarden (`Home recovery`) и в Vaultwarden (`openbao:bootstrap`), не на диске.
  Root-токен после bootstrap отозван; новый выпускается процедурой
  `generate-root` из recovery keys.
- Человек: `userpass` (имя — `secrets/openbao/username`) → политика `admin` с запретом
  `sys/seal`, `sys/step-down`, `sys/raw/*`, `sys/replication/*`,
  `sys/rotate`, `sys/rekey*`. Вход человека — пароль + обязательный
  TOTP (enforcement на `auth/userpass`).
- Операторская автоматизация (агент на Mac владельца): orphan periodic token
  (period 30 дней, продлевается при использовании) с политикой `operator-kv` —
  только create/read/update/patch KV `secret/{ci,k8s,infra}/*`, без
  delete/destroy, `sys/*`, auth и политик. Файл
  `secrets/openbao/operator.token` (600). Компромисс рабочей машины даёт
  чтение/изменение этих секретов — риск принят владельцем; отзыв — `bao token revoke` или UI.
- CI: `jwt` mount `github`, только GitHub OIDC (без статических токенов):
  issuer `https://token.actions.githubusercontent.com`, audience = адрес
  OpenBao, TTL 10/15 мин. Остались роли `deploy-station` (Environment
  `Station`, политика `ci-station` — только `secret/ci/deploy-station`),
  `android-distribute` (Environment `Android` на PR, политика `ci-android` —
  только `secret/ci/android-distribute`) и `android-cleanup` (та же политика и
  Environment, но для ежедневной чистки App Distribution из `develop`) и
  `android-release` (ручной релиз). Home-сервисы применяет Flux, CI в OpenBao
  за ними не ходит.
- Поды: `kubernetes` auth через TokenReview (`system:auth-delegator` на SA
  `openbao`); политика `k8s-read` — read только `secret/k8s/*`. Роль
  `external-secrets` используется External Secrets Operator (namespace
  `external-secrets`), который создаёт Secrets приложений из OpenBao.
  Роль `secrets-gateway` (SA `secrets-gateway`, namespace `home-assistant`,
  audience `secrets-gateway`) даёт сервису секретов приложения
  read/update/delete на `secret/data/*` (delete — только soft-delete версии) и
  read/list на `secret/metadata/*`. Destroy, metadata-delete, undelete,
  sys/auth/identity запрещены. По решению владельца из телефона достижим весь
  каталог, кроме ядра: `openbao/*`, `infra/vpn/client-ca`,
  `k8s/flux-system/*`, `k8s/home-assistant/scheduler-auth`,
  `k8s/home-assistant/secrets-gateway`. Остальные `infra/*` и `ci/*` открыты;
  список один для кода и политики
  (`manifests/deny.json` → `policy.hcl`). Root и operator token в под не
  попадают. Step-up — подпись EC P-256 ключа в Android Keystore: публичный
  ключ, device id и хеш одноразового кода лежат в
  `secret/k8s/home-assistant/secrets-gateway-device`; путь скрыт от HTTP API.
  `admin:secrets-device-enroll` создаёт код на 15 минут и пишет его в файл
  0600, `--revoke` удаляет ключ; сервис читает ключ на каждый запрос. Цепочка
  key attestation проверяется до закреплённых Google Hardware Attestation
  корней, challenge/nonce, TEE/StrongBox, публичный ключ и verified boot;
  непроверенный verified boot логируется.
- Audit — декларативный девайс `stdout/`: kubelet ротирует лог, Alloy
  отправляет его в Loki (7 дней). Значения в записях HMAC-хэшированы, но пути
  и metadata запросов видны — Loki доступен только из LAN/VPN.
- Backup — raft snapshot в локальный PV, ночной vzdump на отдельный SSD и
  зашифрованная offsite-копия (restic → Google Drive). Ключ auto-unseal и
  recovery keys в offsite-архив не входят; restore drill — `REBUILD.md` §6.
- Секреты приложений живут в `secret/k8s/*`; источник правды — OpenBao,
  Kubernetes Secrets создаёт ESO. Ручные правки Kubernetes Secret ESO
  перезапишет.
- Не переносятся: `openbao-seal` (нужен до старта OpenBao), helm release
  Secrets, SA token Secrets, TLS, выпускаемые cert-manager, служебные Secrets
  контроллеров.

## HA / MCP / LLM

- MCP-токены — минимальные права; опасные действия — с подтверждением.
- Hermes видит распознанный текст; для приватности — локальная модель.
- Hermes не владеет alarm/timer state; при его падении локальные команды
  должны работать.
- MA MCP experimental; full access включён осознанно.
- bb наружу не публикуется: API слушает loopback на CT 103, доступ — только
  плагин `home-dispatcher` (токен плагина + HMAC) за SSH forced command с
  отдельным ключом и pinned `known_hosts`. Токен и HMAC — только на CT;
  шлюз ключа плагина не знает и shell не получает. Маршрут плагина через bb
  Connect закрыт (401).
- Голосовой профиль (Личный/Рабочий) переключается только вручную и читается
  из HA как источник истины; ошибка чтения — отказ, а не догадка. В рабочем
  профиле свободная речь не уходит в Hermes (fail-closed): только в рабочий
  тред Claude Code. Hermes и `bb_status` видят только личные проекты;
  рабочие неотличимы от несуществующих.
- Текст речи не пишется в логи voice-proxy и шлюза (только маршрут, профиль,
  id треда и длина); prompt'ы и тексты сообщений bb-диспетчера не логируются.

## Secure Boot / host

- Secure Boot включён; Wi-Fi DKMS подписан MOK.
- Secure Boot PK остаётся `DO NOT TRUST - AMI Test PK`; менять только с
  физдоступом и планом.
- `labuser` с sudo NOPASSWD — root-эквивалент; убрать при многопользовательском
  доступе.
- CI не имеет SSH, root и доступа к K3s API: группа `ci` маршрутизируется
  только до `K3S_NODE`, VPN nft разрешает только `tcp/443` (Traefik → HA для
  `deploy-station`, OpenBao для OIDC `deploy-station`/`android-distribute`);
  `6443` закрыт. CI-пользователи VPN — `ci-pulsar` (станция) и `ci-android`
  (App Distribution).
- Kubeconfig у CI нет: применение в кластере делает Flux in-cluster.
- Образы неизменяемы и адресуются по digest; GHCR-пакеты должны быть
  публичными (pull без секрета), push выполняет job-scoped `GITHUB_TOKEN` в
  CI-джобах, собирающих образ (unit/native), с `packages: write`; job `pins`
  пишет только pins-ветку и имеет `contents: write`.
- Секреты Environment `Station` (VPN p12/пароль,
  `OPENBAO_ADDR`, `OPENBAO_ROLE`) загружаются primary вручную через `gh`; в Git
  их нет. Приватные env приложений лежат в `secret/k8s/flux-system/app-settings`,
  values Hermes — в `secret/k8s/flux-system/runtime-values`; их читает ESO
  (Kubernetes auth, только `secret/k8s/*`), а не job CI. HA/scheduler токены
  CI не нужны.

## Логи / публикация

- Логи стека/станции могут содержать IP, SSID, device_id, URL.
- Перед публикацией: `mise run test`, реестр «Приватные данные», проверка истории Git.
- Публичные docs используют `192.0.2.x`; реальные значения — только `secrets/`.
- При публикации проверять всю Git-историю, а не только HEAD.

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

## Приватные данные

Реестр мест, где лежат приватные данные (значений здесь нет). Все секреты —
в OpenBao: `secret/k8s/*` (ESO), `secret/ci/*` (CI), `secret/infra/*` (доступ
владельца: SSH-ключи, CA, VPN, бэкап-пароли, admin kubeconfig, Terraform,
Wi-Fi/BLE станции). Файл, который читает локальный скрипт, выдаёт fnox через
`mise run admin:*` во временных файлах; вручную — `mise run secrets:file` /
`secrets:clean`.

### Высокий риск

| Путь | Что | Почему |
|---|---|---|
| `secrets/NETWORK.md` | реальные LAN IP, порты, VPN/HTTPS | карта сети |
| `secrets/openbao/` | operator token, адрес, TOTP method id | постоянный KV-доступ агентов |
| `secrets/local/secrets-gateway/` | одноразовые коды привязки телефона (0600, 15 минут) | доступ к секретам до истечения кода |
| `~/.ssh/home-lab/` | ключи PVE/K3s/HAOS, Комп, VPS + known_hosts | рабочий SSH; копии в `secret/infra/ssh/*` |
| `local-data/local/backups/`, `local-data/proxmox-install/*.tar*` | бэкапы станции/HA/Wi-Fi | внутри исторические токены/ключи |
| `/data/quasar/ble-gateway/token`, `control-token` | BLE и HTTP control токены | смена Wi-Fi, управление gateway |
| `/var/lib/shim-signed/mok/MOK.priv` на PVE | MOK private key | DKMS при Secure Boot |
| Hermes `auth.json`/`.env` | HA/MA tokens, OAuth | доступ к HA/MA/LLM |

### Средний риск

| Путь | Что | Почему |
|---|---|---|
| `local-data/local/logs/`, `archive/logs/` | статусы/диагностика | IP, URL, иногда токены |
| `local-data/local/artifacts/` | архивы, pcap, прошивки | метаданные/ключи внутри |
| `local-data/local/snapshots/` | снапшоты станции и проекта | сеть/конфиги |
| `projects/station/configs/*.env` | station env | IP/URL, возможно ключи |
| `artifacts/firmware/*.zip` | прошивка Station 2 | юридический риск, риск кирпича |
| `artifacts/alice-mitm-*`, `artifacts/prior-session/` | bootstrap kit, транскрипт сессии | ключи и секреты внутри |

Низкий риск: `projects/station/configs/*.crt` (публичные сертификаты),
`artifacts/ssh/dropbear*` (бинарники, не ключи).

### Правила

1. Не публиковать `secrets/`, бэкапы, `artifacts/prior-session`,
   `archive/legacy/**/certs/*.key`.
2. В docs и чатах — имена переменных, не значения.
3. Offsite — только шифрованный архив с отдельным паролем.
4. При утечке — ротация SSH/ADB/BLE/LLM/VPN, отзыв HA tokens.
5. Значение секрета приложения менять в `secret/k8s/<ns>/<name>` и
   запускать `force-sync` ESO.
