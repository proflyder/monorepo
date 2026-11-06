# SENSITIVE-DATA — приватные данные

`secrets/` — 700; файлы — 600. Значений в документе нет.

## Высокий риск

Все секреты — в OpenBao: `secret/k8s/*` (ESO), `secret/ci/*` (CI),
`secret/infra/*` (доступ владельца: SSH-ключи, CA, VPN, бэкап-пароли, admin
kubeconfig, Terraform, Wi-Fi/BLE станции). Локальный `secrets/` хранит только
описание и bootstrap:

| Путь | Что | Почему |
|---|---|---|
| `secrets/README.md`, `secrets/catalog.json` | описание и карта «старый файл → путь#поле» | без значений; IP/MAC |
| `secrets/NETWORK.md` | реальные LAN IP, порты, VPN/HTTPS | карта сети |
| `secrets/openbao/` | operator token, адрес, TOTP method id | постоянный KV-доступ агентов; recovery keys и seal key — только у владельца (Bitwarden `Home recovery`, Vaultwarden `openbao:bootstrap`) |
| `~/.ssh/home-lab/` | ключ PVE/K3s/HAOS, Комп, VPS + known_hosts | рабочий SSH; копии в `secret/infra/ssh/*` |
| `local-data/local/backups/`, `local-data/proxmox-install/*.tar*` | бэкапы станции/HA/Wi-Fi | внутри исторические токены/ключи |
| `/data/quasar/ble-gateway/token` | BLE token | смена Wi-Fi |
| `/data/quasar/ble-gateway/control-token` | HTTP control | gateway/Bluetooth |
| `/var/lib/shim-signed/mok/MOK.priv` | MOK private key | DKMS при Secure Boot |
| Hermes `auth.json`/`.env` | HA/MA tokens, OAuth | доступ к HA/MA/LLM |

Файл, который ещё читает локальный скрипт (`secrets/home/*.env` и т.п.),
временно восстанавливает `mise run secrets:file` (`secrets:clean` убирает). Источник правды — OpenBao; ручные правки Kubernetes
Secret перезапишет ESO.

## Средний риск

| Путь | Что | Почему |
|---|---|---|
| `local-data/local/logs/`, `archive/logs/` | статусы/диагностика | IP, URL, иногда токены |
| `local-data/local/artifacts/` | архивы, pcap, прошивки | метаданные/ключи внутри |
| `local-data/local/snapshots/` | снапшоты станции и `git archive` проекта | сеть/конфиги |
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
