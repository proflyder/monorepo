# SENSITIVE-DATA — приватные данные

`secrets/` — 700; файлы — 600. Значений в документе нет.

## Высокий риск

| Путь | Что | Почему |
|---|---|---|
| `secrets/CREDENTIALS.md` | SSH/ADB/сервисные пароли, IP/MAC | доступ к станции/сети |
| `secrets/NETWORK.md` | реальные LAN IP, порты, VPN/HTTPS | карта сети |
| `secrets/certs/*.key` | eMudhra/leaf ключи | подпись/доверие станции |
| `secrets/proxmox-install/retired-dpp-*/` | архив удалённого QR/DPP, старые config/keys | приватный recovery; не активный provisioning |
| `secrets/openbao/` | seal key, recovery keys, admin password, TOTP method id | доступ к секретам/root recovery |
| `secrets/kubernetes-research/` | kubeconfig, values, Grafana password, recovery | admin/state |
| `secrets/proxmox-install/` | SSH keys, TF state, Wi-Fi profile, install settings | host/VM recovery |
| `secrets/local/backups/` | HA config/DB, MA/STT/TTS, станция | токены/auth/ключи |
| `/data/quasar/ble-gateway/token` | BLE token | смена Wi-Fi |
| `/data/quasar/ble-gateway/control-token` | HTTP control | gateway/Bluetooth |
| `/var/lib/shim-signed/mok/MOK.priv` | MOK private key | DKMS при Secure Boot |
| Hermes `auth.json`/`.env` | HA/MA tokens, OAuth | доступ к HA/MA/LLM |

Приложенческие Secrets (`scheduler-auth`, `ha-bridge-ha`,
`hermes-environment`, `grafana-admin`, `cloudflare-api-token`) хранятся в
OpenBao `secret/k8s/*`; Kubernetes Secrets создаёт External Secrets Operator.
Локальные файлы в `secrets/` (токены, values, `grafana-password`) остаются
бэкапом и для первой установки, но **не** являются источником правды и не
перезаписывают live Secrets при обычном деплое.

## Средний риск

| Путь | Что | Почему |
|---|---|---|
| `secrets/local/logs/`, `archive/logs/` | статусы/диагностика | IP, URL, иногда токены |
| `secrets/local/artifacts/` | архивы, pcap, прошивки | метаданные/ключи внутри |
| `secrets/proxmox-install/migration-*` | HA/K3s migration archives | auth/state |
| `secrets/local/snapshots/` | PVE/VM snapshots | сеть/конфиги |
| `projects/station/configs/*.env` | station env | IP/URL, возможно ключи |
| `artifacts/firmware/*.zip` | прошивка Station | юридический/риск кирпича |

## Публичное / низкий риск

- `projects/station/configs/*.crt` — публичные сертификаты.
- `artifacts/ssh/dropbear*` — бинарники, не ключи.
- `docs/` — только примеры `192.0.2.x`; реальные IP — в `secrets/`.

## Правила

1. Не публиковать `secrets/`, `backups/`, `artifacts/prior-session`,
   `archive/legacy/**/certs/*.key`.
2. В docs/чатах — имена переменных, не значения.
3. Offsite — только шифрованный архив + отдельный пароль.
4. При утечке — ротация SSH/ADB/BLE/LLM/VPN, отзыв HA tokens.
5. Перед публикацией — `make test` + проверка Git-истории.
6. Источник правды для приложенческих секретов — OpenBao; правки в
   Kubernetes Secret вручную будут перезаписаны ESO. Менять значение в
   `secret/k8s/<ns>/<name>` и запускать `force-sync`.
