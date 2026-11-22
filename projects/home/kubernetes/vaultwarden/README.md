# Vaultwarden — менеджер паролей

Vaultwarden — сервер, совместимый с Bitwarden (официальные приложения,
расширения браузера и `bw` CLI), на K3s: `https://passwords.<home-domain>/`,
только LAN/VPN через Traefik и `home-allowlist`.

Роль: удобный доступ человека к паролям и копия секретов OpenBao на всех
устройствах. Клиенты Bitwarden держат зашифрованный офлайн-кэш хранилища,
поэтому при падении домашнего сервера данные остаются на телефоне и Mac
(без синхронизации). Источник правды для машинных секретов — по-прежнему
OpenBao (`secret/{k8s,ci,infra}/*`).

## Устройство

Flux Kustomization `vaultwarden` (`kustomization.yaml` → `manifests/`):
namespace, ExternalSecret, NetworkPolicies (ingress 8080 только с узла/LAN,
egress только DNS), PVC `vaultwarden-data` (`local-retain` 2Gi, `keep` +
`prune: disabled`), Deployment (`Recreate`, один SQLite writer), Service,
IngressRoute.

- Образ `docker.io/vaultwarden/server` по digest (`versions.sh`), non-root
  (65534), read-only rootfs, порт 8080, WebSocket-уведомления на том же порту.
- Регистрация закрыта (`SIGNUPS_ALLOWED=false`); аккаунты — по приглашению из
  `/admin`. SMTP нет: приглашённый адрес регистрируется сам на странице входа.
- Исходящего трафика нет: иконки сайтов не скачиваются, push-relay не
  настроен (мобильные клиенты синхронизируются при открытии и по pull).
- Настройки `/admin` не сохраняются (`CONFIG_FILE` в `/tmp`): конфигурация —
  только env в манифесте.
- Данные (`db.sqlite3`, `attachments/`, `sends/`, `rsa_key*`) — PVC
  `vaultwarden-data`. Ежедневный offsite-бэкап (`projects/home/backup/offsite`):
  online-копия SQLite + tar остального.

## Секреты

| OpenBao | Что |
|---|---|
| `secret/k8s/vaultwarden/vaultwarden-admin#ADMIN_TOKEN` | Argon2id PHC-хэш пароля `/admin` (ESO → env) |
| `secret/infra/vaultwarden/admin#admin_password` | сам пароль `/admin` для входа человека |

Смена пароля `/admin`: новый случайный пароль → Argon2id
(`m=65540,t=3,p=4`, как `vaultwarden hash`) → оба поля в OpenBao →
`kubectl -n vaultwarden annotate externalsecret vaultwarden-admin force-sync="$(date +%s)" --overwrite`
→ `kubectl -n vaultwarden rollout restart deployment/vaultwarden`.

## Деплой

Namespace, ExternalSecret, NetworkPolicies, workload и маршрут reconciled Flux
(`vaultwarden` Kustomization, dependsOn `platform`). `HOME_DOMAIN`,
`NODE_LAN_IP` и `LAN_CIDR` подставляются из `secret/k8s/flux-system/settings`.

```bash
# после merge в develop
mise run kubectl -- -n flux-system annotate kustomization/vaultwarden \
  reconcile.fluxcd.io/requestedAt="$(date +%s)" --overwrite
mise run kubectl -- -n vaultwarden get pod,pvc,externalsecret
```

Обновление версии — новый digest в `versions.sh` **и** в
`manifests/vaultwarden.yaml` (тест сверяет их), затем merge.

DNS-запись `passwords.<home-domain>` — вручную (`cloudflare_dns.py`, `../README.md`).

## Первый вход

1. `https://passwords.<home-domain>/admin` → пароль из
   `secret/infra/vaultwarden/admin` → Users → Invite (email владельца).
2. `https://passwords.<home-domain>/` → Create account с этим email; мастер-пароль
   хранится только в голове/на бумаге — его нельзя восстановить.
3. Приложения Bitwarden: при входе выбрать Self-hosted, Server URL
   `https://passwords.<home-domain>`. Вне дома — через VPN.

## Копия секретов OpenBao

`sync_openbao.py` копирует все пути `secret/{ci,k8s,infra}/*` в папку `OpenBao`:
Secure Note `openbao:<path>` (короткие значения — скрытые поля, длинные —
вложения), отдельные SSH key items `ssh:infra/ssh/...` для SSH-агента
Bitwarden и `openbao:bootstrap` (operator token; recovery keys и seal key
обновляются, только если лежат на диске — иначе item не трогается). Идемпотентно (поле `_digest`), удалённые в OpenBao записи
только отмечаются. Править — в OpenBao, затем перезапуск:

```bash
bw config server https://passwords.<home-domain> && bw login   # один раз
export BW_SESSION=$(bw unlock --raw)
mise run vault:sync -- --dry-run
mise run vault:sync
bw lock
```

## Восстановление

Из offsite (`restic restore`): `sqlite/vaultwarden.sqlite3` → `db.sqlite3`,
`vaultwarden/files.tar` распаковать рядом, в корень PVC `vaultwarden-data`
при остановленном Deployment (`kubectl scale --replicas=0`), затем
`--replicas=1`.
