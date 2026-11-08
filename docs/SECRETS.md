# SECRETS — где лежат секреты

Все секреты — только в OpenBao (KV v2, mount `secret`), `https://vault.<home-domain>/`
(LAN/VPN). В Git — только имена: машинный каталог
`platform/workstation/catalog.json`, эта таблица его отражает (тест
`platform/workstation/test_workstation.py`). Значения не публикуются никогда.

- `secret/k8s/<ns>/<name>` — Secrets приложений; Kubernetes Secret создаёт ESO.
- `secret/ci/<name>` — CI по GitHub OIDC.
- `secret/infra/<group>/<name>` — доступ владельца и агентов (operator token).

Доступ с рабочего места — `docs/WORKSTATION.md`: `mise run secrets:login` выдаёт
машине periodic operator token (политика `operator-kv`: read/write
`secret/{ci,k8s,infra}/*`, без delete). Скрипты получают секреты через
`mise run admin:*`: fnox (`fnox.toml`, профиль на скрипт) передаёт их во
временных файлах и переменных; `mise run secrets:file <file>` — запасной путь
для файла вручную; `mise run secrets:check` сверяет каталог и ссылки
`fnox.toml` с OpenBao. Recovery keys и static seal key OpenBao хранятся
только у владельца (Bitwarden `Home recovery`, Vaultwarden `openbao:bootstrap`).
Копия всего KV — Vaultwarden (`mise run vault:sync`).

Запись: GET → merge → POST с `options.cas` (PATCH admin-политике запрещён).
Новый путь — сразу в OpenBao и строкой в каталог (+ эта таблица).

## Пути

| Путь | Что | Кто читает |
|---|---|---|
| `ci/android-distribute` | Android: APK release signing key, passwords, alias and certificate; Firebase App Distribution | CI android-distribute (OIDC) |
| `ci/deploy-home` | deploy-home: ci-deploy kubeconfig, service env files, runtime values | CI deploy-home (OIDC) |
| `ci/deploy-station` | Pulsar rollout through HA (url, token, firmware entity) | CI deploy-station (OIDC) |
| `ci/test` | smoke probe, not secret | CI auth smoke |
| `infra/backup/ha` | HA backup encryption key | owner (restore) |
| `infra/backup/offsite` | restic offsite: env, password, rclone/GDrive OAuth, HA token | owner (offsite install/restore) |
| `infra/certs/station-tls` | station TLS: eMudhra CA, quasar leaf and legacy MITM keys | owner |
| `infra/ci/github-environment` | values of GitHub Environments Home/Station (VPN p12, server, runtime values) | owner (GitHub settings) |
| `infra/cloudflare/currency-bot-tunnel` | Cloudflare tunnel API token and tunnel metadata | owner |
| `infra/cloudflare/vpn-dns` | Cloudflare DNS token (home records, OpenBao/Vaultwarden deploy) | owner deploy scripts |
| `infra/cloudflare/workers` | Cloudflare Workers/KV token and heartbeat Worker env | owner deploy scripts |
| `infra/currency-bot/vps` | legacy VPS env of the currency bot | owner (legacy) |
| `infra/docs/network` | private network map and station identifiers (secrets/NETWORK.md) | secrets:pull |
| `infra/komp/wakegrant` | wakegrant env for the Windows PC | owner deploy scripts |
| `infra/kubernetes/admin` | admin kubeconfigs of K3s (current and stale) | owner deploy scripts |
| `infra/kubernetes/secret-dumps` | historical Kubernetes Secret dumps (2026-10-31, no helm releases) | owner (recovery) |
| `infra/kubernetes/values-private` | private Helm values and ingress env | owner deploy scripts |
| `infra/legacy/llm` | legacy v3rt3p LLM env | owner (legacy) |
| `infra/proxmox/terraform` | Proxmox API token, Terraform state/vars, install settings | owner (Terraform) |
| `infra/ssh/ci-deploy` | CI deploy SSH key and known_hosts | owner |
| `infra/ssh/currency-bot-vps` | SSH to the currency bot VPS | secrets:pull |
| `infra/ssh/ha-addon` | SSH key of the HA SSH add-on | secrets:pull |
| `infra/ssh/komp` | SSH to the Windows PC (dev key) and its host key | secrets:pull |
| `infra/ssh/mcp-gateway-bb-dispatch` | public key of the MCP gateway bb dispatch key | owner |
| `infra/ssh/offsite-bb` | offsite pull key for the bb server export | owner (offsite install) |
| `infra/ssh/proxmox-admin` | SSH to PVE, K3s, HAOS: key, known_hosts, ~/.ssh/home-lab/config | secrets:pull |
| `infra/ssh/station` | SSH key for the station (root) | secrets:pull |
| `infra/ssh/wsl` | SSH key for WSL on the Windows PC | secrets:pull |
| `infra/station/access` | station emergency root password, BLE recovery token, Wi-Fi profiles | owner |
| `infra/station/pulsar-backup-keys` | API/sendspin keys from old station backups | owner (recovery) |
| `infra/station/retired-dpp` | archive of the removed DPP/QR provisioning | owner (recovery) |
| `infra/test` | smoke probe, not secret | operator token check |
| `infra/vaultwarden/admin` | Vaultwarden /admin password | owner |
| `infra/vpn/client-ca` | ocserv client CA (key, cert, CRL, OpenSSL dir) | owner (vpncerts.py) |
| `infra/vpn/clients/ci-github` | VPN client certificate for CI deploy-home | owner |
| `infra/vpn/clients/ci-pulsar` | VPN client certificate for CI deploy-station | owner |
| `infra/vpn/clients/owner-phone` | VPN client certificate of the owner's phone | owner |
| `infra/vpn/owner` | owner VPN login: password, TOTP seed and QR | owner |
| `k8s/cert-manager/cloudflare-api-token` | Cloudflare DNS-01 token for the wildcard certificate | ESO → cert-manager |
| `k8s/currency-bot/cloudflared-tunnel` | Cloudflare Tunnel token for the Telegram webhook | ESO → currency-bot |
| `k8s/currency-bot/currency-bot` | currency bot Telegram/API credentials | ESO → currency-bot |
| `k8s/home-assistant/ha-bridge-ha` | HA long-lived token for ha-bridge | ESO → home-assistant |
| `k8s/home-assistant/hermes-environment` | Hermes agent environment (HA/MA/Telegram/API keys) | ESO → home-assistant |
| `k8s/home-assistant/mcp-gateway-auth` | MCP gateway bearer | ESO → home-assistant |
| `k8s/home-assistant/mcp-gateway-bb-ssh` | SSH key + known_hosts for bb dispatch from the MCP gateway | ESO → home-assistant |
| `k8s/home-assistant/mcp-gateway-voice` | voice tool bearer of the MCP gateway | ESO → home-assistant |
| `k8s/home-assistant/scheduler-auth` | scheduler API bearer | ESO → home-assistant |
| `k8s/home-assistant/wakegrant` | wakegrant issuer token and per-machine keys | ESO → home-assistant |
| `k8s/monitoring/grafana-admin` | Grafana admin login | ESO → monitoring |
| `k8s/monitoring/heartbeat` | dead-man switch Worker URL and token | ESO → monitoring |
| `k8s/test` | smoke probe, not secret | Kubernetes auth smoke |
| `k8s/vaultwarden/vaultwarden-admin` | Argon2id hash of the Vaultwarden /admin token | ESO → vaultwarden |

Поля `ci/android-distribute` для release-подписи APK: `signing_keystore_b64`
(base64 PKCS12-ключа), `signing_store_password`, `signing_key_password`,
`signing_key_alias`, `signing_cert_sha256` (обязательный ожидаемый отпечаток
сертификата), плюс `google_services_json` и `firebase_service_account_json`.
Локально они восстанавливаются как `secrets/android/home-release.keystore`,
`home-release.store.password`, `home-release.key.password`, `home-release.alias`,
`home-release.cert.sha256` (`mise run secrets:file android/<имя>`), но обычно
их передаёт fnox: `mise run admin:android-distribute` (профиль `android`); старые
`keystore_b64`/`keystore_password` больше не читаются. Процедура — Android
README, раздел Signing.

## Что `mise run secrets:pull` кладёт на рабочее место

| Файл | Источник |
|---|---|
| `~/.ssh/home-lab/proxmox_admin` | `infra/ssh/proxmox-admin#proxmox_admin` |
| `~/.ssh/home-lab/proxmox_admin.pub` | `infra/ssh/proxmox-admin#proxmox_admin_pub` |
| `~/.ssh/home-lab/known_hosts_pve` | `infra/ssh/proxmox-admin#known_hosts` |
| `~/.ssh/home-lab/known_hosts_k3s` | `infra/ssh/proxmox-admin#guest_known_hosts` |
| `~/.ssh/home-lab/known_hosts_haos` | `infra/ssh/proxmox-admin#haos_known_hosts` |
| `~/.ssh/home-lab/config` | `infra/ssh/proxmox-admin#home_lab_ssh_config` |
| `~/.ssh/home-lab/dev_ed25519` | `infra/ssh/komp#dev_ed25519` |
| `~/.ssh/home-lab/dev_ed25519.pub` | `infra/ssh/komp#dev_ed25519_pub` |
| `~/.ssh/home-lab/known_hosts_komp` | `infra/ssh/komp#known_hosts` |
| `~/.ssh/home-lab/vps_ed25519` | `infra/ssh/currency-bot-vps#vps_ed25519` |
| `~/.ssh/home-lab/vps_ed25519.pub` | `infra/ssh/currency-bot-vps#vps_ed25519_pub` |
| `~/.ssh/home-lab/station_ed25519` | `infra/ssh/station#private_key` |
| `~/.ssh/home-lab/station_ed25519.pub` | `infra/ssh/station#public_key` |
| `~/.ssh/home-lab/wsl_ed25519` | `infra/ssh/wsl#private_key` |
| `~/.ssh/home-lab/wsl_ed25519.pub` | `infra/ssh/wsl#public_key` |
| `~/.ssh/home-lab/ha_addon_ed25519` | `infra/ssh/ha-addon#private_key` |
| `~/.ssh/home-lab/ha_addon_ed25519.pub` | `infra/ssh/ha-addon#public_key` |
| `secrets/NETWORK.md` | `infra/docs/network#network_md` |

`~/.ssh/home-lab/config` даёт алиасы `pve`, `k3s`, `haos`:
`ssh -F ~/.ssh/home-lab/config k3s 'sudo -n k3s kubectl get nodes'`.
