# Proxmox bootstrap

Хост Proxmox и его гости: VM 100 `haos`, VM 101 K3s, CT 102 `vpn`, CT 103
`bb`. Факты (версии, адреса, ресурсы) — `docs/STATE.md`, реальные адреса —
`secrets/NETWORK.md`, физическая установка и восстановление —
`docs/INSTALL-PROXMOX.md` и `docs/REBUILD.md`.

| Путь | Что |
|---|---|
| [vpn/](vpn/README.md) | CT 102: ocserv + WireGuard |
| [bb/](bb/README.md) | CT 103: bb server |
| [wifi-uplink/](wifi-uplink/) | Wi-Fi 4-address/WDS uplink `vmbr0` с fallback на Ethernet |
| `terraform/` | VM 100 и 101 (`prevent_destroy`) |
| `cloud-init.yaml` | публичная часть cloud-init K3s (приватная добавляет SSH key); сохраняет SSH host keys при смене instance ID |
| `authorize.sh HOST KEY KNOWN_HOSTS` | интерактивно с Mac после установки PVE: ssh-copy-id отдельного ключа; fingerprint сверить с физической консолью |
| `bootstrap-host.sh` | no-subscription repository, отключение enterprise/Ceph feeds, upgrade simulation (само обновление и reboot — отдельно) |
| `set-startup-order.sh` | startup order 1/2/3 для HAOS, K3s, bb (root: PVE API требует `Sys.Modify`, которого у API user нет) |
| `versions.sh` | версии шаблонов и Node.js для CT |
| `bootstrap-wifi-mapping.sh`, `backup-before-install.py` | разовые помощники миграции; в эксплуатации не используются |

## Terraform

- Каталог и state — на PVE в root-only `/var/lib/home-terraform/`; после apply
  state сохраняется в OpenBao
  `secret/infra/proxmox/terraform#pve_terraform_tfstate`. Конфигурация
  синхронизируется из tracked `main.tf`, lock копируется обратно.
- Запуск на Linux с проверкой CA (Go TLS на macOS игнорирует
  `SSL_CERT_FILE`), `insecure=false`, CA получен по проверенному SSH.
  Endpoint/token — из environment, не из tfvars.
- API user: read-only audit, VM admin только для 100/101, место на `local` и
  `local-lvm`, `vmbr0`. Startup policy Terraform игнорирует.
- Статический IP K3s — приватные переменные `k3s_ipv4_address` (CIDR) и
  `k3s_ipv4_gateway`; DHCP по умолчанию — только для новых инсталляций.
- State с локальной файловой блокировкой: не запускать параллельный apply.

## Проверки

```bash
python3 -m unittest discover -s projects/home/proxmox -v
bash -n projects/home/proxmox/authorize.sh
```
