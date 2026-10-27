# Proxmox bootstrap

Физическая установка и этапы восстановления: [runbook](../../../docs/INSTALL-PROXMOX.md).

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

Рабочий Terraform directory/state/plan/token находятся только в
`secrets/proxmox-install/terraform-state/`. Конфигурация подключается оттуда
symlink на tracked main.tf; provider lock копируется обратно в source.
TLS проверяется CA, полученным по проверенному SSH; `insecure=false`.
Provider endpoint/token берутся из environment, не из публичных tfvars.
API user имеет read-only audit, VM admin только для 100/101, выделение места
на local-lvm и использование vmbr0. Bootstrap SSH root — отдельный канал.
State пока локальный с файловой блокировкой; регулярное шифрованное внешнее
резервирование ещё нужно настроить. Не запускать параллельный apply.
