# Proxmox bootstrap

Физическая установка и этапы восстановления: [runbook](../../../docs/INSTALL-PROXMOX.md).

## Текущее состояние

PVE `9.2.24`; host management `192.0.2.26`; VM `100 haos` (`192.0.2.28`,
2 vCPU/4 GiB/32 GiB), VM `101 lab-k3s` (`192.0.2.27`, 8 vCPU/12 GiB/200 GiB),
CT `102 vpn` (`192.0.2.29`, Debian 13, ocserv) и CT `103 bb`
(`192.0.2.30`, Debian 13, 2 vCPU/2 GiB/16 GiB). Static IP в guests,
startup order 1/2/3. HAOS add-ons: Matter `9.2.0`, Music Assistant `2.10.8`.
Terraform plan — no changes; state root-only на PVE и копия в `secrets/`.
HAOS восстановлен, K3s runtime и HTTPS ingress приняты read-only E2E.
HA-совместимый endpoint HAOS доступен напрямую `.28:8123`; compatibility
HAProxy удалён.

Wi-Fi RTL8821CE работает на host как 4-address/WDS uplink `vmbr0`,
VM IP и L2/multicast сохраняются. `ricardo-uplink` выбирает один uplink:
Ethernet при carrier, иначе Wi-Fi. NAT и PCI passthrough не требуются.
Исходники/установка/откат: [wifi-uplink](wifi-uplink/).
`bootstrap-wifi-mapping.sh` и `backup-before-install.py` — помощники разовой
миграции на PVE; в текущей эксплуатации не используются.

VPN-сервер (ocserv) разворачивается отдельным unprivileged CT 102 `vpn`
скриптами [vpn](vpn/): создание LXC, установка ocserv с nftables,
пользователи и клиентские сертификаты. Публичный сертификат Let's Encrypt
продлевается автоматически (DNS-01 Cloudflare); группа `ci` — только до K3s.
Проброс на роутере — только `443/tcp+udp`; подробности и откат — в
[vpn/README.md](vpn/README.md).

Сервер bb разворачивается отдельным unprivileged CT 103 `bb` (Debian 13,
2 vCPU / 2 GiB / 16 GiB) скриптами [bb](bb/): фиксированный MAC и static IP,
Node.js по checksum, пользователь `bb` с linger и проверкой systemd user bus.
Внутри CT работает согласованный SQLite backup перед ночным vzdump; при
недоступном user bus enrollment запрещён и нужна VM. Подробности и перенос —
[bb/README.md](bb/README.md). `set-startup-order.sh` задаёт order 1/2/3 для
HAOS, K3s и bb.

Статический IP K3s задаётся приватными Terraform variables
`k3s_ipv4_address` (CIDR) и `k3s_ipv4_gateway`; default DHCP — только для
новых универсальных инсталляций. `cloud-init.yaml` сохраняет существующие
SSH host keys при смене config-drive instance ID.

`authorize.sh HOST KEY KNOWN_HOSTS` запускается интерактивно на Mac после установки
PVE: добавляет отдельный ключ через ssh-copy-id и проверяет доступ. Fingerprint
обязательно сравнить с физической консолью. Приватный wrapper с параметрами
находится в `secrets/proxmox-install/authorize-proxmox.sh`.

Проверки: `python3 -m unittest discover -s projects/home/proxmox -v` и
`bash -n projects/home/proxmox/authorize.sh`.

`bootstrap-host.sh` включает подписанный no-subscription PVE repository, отключает
неиспользуемые enterprise/Ceph feeds, сохраняет исходные sources в host backup.
Показывает upgrade simulation; само обновление и reboot выполняются отдельно.

`terraform/main.tf` закрепляет версии CLI/provider и описывает VM 100 (HAOS,
4 GiB / 32 GiB) и 101 (Ubuntu 24.04 LTS, 12 GiB / 200 GiB). `prevent_destroy`
защищает от случайного удаления через Terraform. Образы и private cloud-init
snippet загружаются bootstrap-этапом; Terraform использует их storage IDs.
`cloud-init.yaml` — публичная часть, приватная добавляет SSH public key.

Авторитетный Terraform directory/state находится на PVE в root-only
`/var/lib/ricardo-terraform/`; после apply state копируется на Mac в
`secrets/proxmox-install/terraform-state-pve/`. Terraform запускается на Linux
с проверкой CA (Go TLS на macOS игнорирует `SSL_CERT_FILE`); системное trust
store Mac не меняется.
Конфигурация синхронизируется из tracked main.tf; lock копируется обратно в source.
TLS проверяется CA, полученным по проверенному SSH; `insecure=false`.
Provider endpoint/token берутся из environment, не из публичных tfvars.
API user имеет read-only audit, VM admin только для 100/101, выделение места
на local и local-lvm и использование vmbr0. Порядок старта задаётся отдельно
root bootstrap `set-startup-order.sh`: его PVE API требует глобального Sys.Modify,
которое API user не получает. Terraform игнорирует только startup policy. Bootstrap SSH root — отдельный канал.
State пока локальный с файловой блокировкой; регулярное шифрованное внешнее
резервирование ещё нужно настроить. Не запускать параллельный apply.
