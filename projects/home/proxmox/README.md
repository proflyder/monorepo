# Proxmox bootstrap

Физическая установка и этапы восстановления: [runbook](../../../docs/INSTALL-PROXMOX.md).

## Текущее состояние 2026-10-28

PVE `9.2.24`; host management `192.0.2.26`; VM `100 haos` (`192.0.2.28`,
2 vCPU/3 GiB/32 GiB) и VM `101 lab-k3s` (`192.0.2.27`,
8 vCPU/9 GiB/200 GiB) running, static IP в guests, startup order 1/2.
Terraform plan — no changes;
state root-only на PVE и копия в `secrets/`. HAOS восстановлен, K3s runtime
и Tailscale Operator приняты read-only E2E. HA-совместимый endpoint
`192.0.2.27:8123` обслуживает HAProxy chart `haos-proxy`.

Wi-Fi RTL8821CE на host DOWN; `bootstrap-wifi-mapping.sh` создаёт PCI mapping,
но **не передаёт** карту VM и не меняет Ethernet. Это подготовка, не готовая
Wi-Fi архитектура: до routed/NAT или внешнего Wi-Fi bridge оставлять Ethernet.

`backup-before-install.py --output /private/new-directory` запускается root на
текущем K3s host. Останавливает writers, копирует PVC/host recovery, восстанавливает
replica counts, сохраняет cluster metadata и проверяет SQLite. Output содержит
секреты; хранить вне Git. Путь output не должен лежать внутри копируемого PVC.
Скрипт рассчитан на текущую Ubuntu/LVM/K3s композицию, не универсальный backup tool.

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
3 GiB / 32 GiB) и 101 (Ubuntu 24.04 LTS, 9 GiB / 200 GiB). `prevent_destroy`
защищает от случайного удаления через Terraform. Образы и private cloud-init
snippet загружаются bootstrap-этапом; Terraform использует их storage IDs.
`cloud-init.yaml` — публичная часть, приватная добавляет SSH public key.

Авторитетный Terraform directory/state находится на PVE в root-only
`/var/lib/ricardo-terraform/`; после apply state копируется на Mac в
`secrets/proxmox-install/terraform-state-pve/`. Первый macOS apply не создал
ресурсов: Go TLS на macOS не использует SSL_CERT_FILE как Linux. Terraform
запускается на Linux с проверкой CA, системное trust store Mac не меняется.
Конфигурация синхронизируется из tracked main.tf; lock копируется обратно в source.
TLS проверяется CA, полученным по проверенному SSH; `insecure=false`.
Provider endpoint/token берутся из environment, не из публичных tfvars.
API user имеет read-only audit, VM admin только для 100/101, выделение места
на local и local-lvm и использование vmbr0. Порядок старта задаётся отдельно
root bootstrap `set-startup-order.sh`: его PVE API требует глобального Sys.Modify,
которое API user не получает. Terraform игнорирует только startup policy. Bootstrap SSH root — отдельный канал.
State пока локальный с файловой блокировкой; регулярное шифрованное внешнее
резервирование ещё нужно настроить. Не запускать параллельный apply.
