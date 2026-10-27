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
