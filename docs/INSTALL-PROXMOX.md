# Установка Proxmox на неттоп

Подготовлено 2026-10-27. Обновление 2026-10-28: Proxmox `9.2.24` установлен,
SSH проверен; VM `100 haos` и `101 lab-k3s` созданы Terraform, HAOS и
K3s runtime восстановлены, Tailscale Operator опубликован. Эта инструкция
сохранена для повторной установки; текущие факты — STATE.md.

Текущее состояние после установки (примеры LAN-адресов):

```text
PVE host         192.0.2.26   Ethernet vmbr0; Wi-Fi пока DOWN
VM 100 haos      192.0.2.28   2 vCPU / 3 GiB / 32 GiB
VM 101 lab-k3s   192.0.2.27   8 vCPU / 9 GiB / 200 GiB
HA compatibility 192.0.2.27:8123 -> HAOS:8123 (HAProxy hostNetwork)
```

Terraform plan — `No changes`; state остаётся root-only на PVE и копируется
в `secrets/`. Полный power-loss restore и переход host на Wi-Fi ещё не
приняты.

## Что уже сохранено

На отдельном Mac в игнорируемой папке `secrets/proxmox-install/`:

- официальный x86-64 ISO Proxmox VE 9.2-1, проверенный SHA-256;
- свежий cold snapshot всех 11 Bound PVC: приложения и telemetry;
- 11 SQLite databases прошли quick_check, K3s state — integrity_check;
- host recovery: конфигурация, SSH/Tailscale, MOK/DKMS/kernel, package list,
  partition/LVM metadata; EFI сохранён отдельным архивом;
- приватная памятка `install-settings.md`, отдельная SSH key pair и
  `authorize-proxmox.sh` для интерактивного добавления ключа после установки.

SHA-256 архивов на Mac совпадает с исходными на неттопе. Отдельно сохранены
предыдущие recovery archives и OCI archive собственных Hermes/proxy images
в `secrets/kubernetes-research/recovery/`. Не удалять эти файлы.

Это копия состояния и recovery-файлов, **не полный образ диска**. Полное
восстановление на запасном железе ещё не проверялось. Откат требует установки
Ubuntu/K3s и восстановления файлов; установка Proxmox сама приложения не восстановит.
При восстановлении в новые VM нельзя вслепую заменять их `/etc` или K3s database
старым архивом: новые сетевые адреса и пути нужно адаптировать.

## Физическая установка

1. Открыть приватную памятку `secrets/proxmox-install/install-settings.md` на Mac.
   Она содержит реальные IP, MAC и параметры; не копировать её в Git или чат.
2. Подготовить USB от 4 GB (удобнее 8 GB), записать локальный
   `secrets/proxmox-install/proxmox-ve_9.2-1.iso` через
   [balenaEtcher](https://etcher.balena.io/): Flash from file → USB → Flash.
   Проверить выбранный USB: запись стирает его. Etcher должен завершить validation.
   Нужен x86-64 ISO для Intel неттопа, даже если Mac на ARM.
3. Подключить неттоп к роутеру Ethernet, экран и клавиатуру. Вставить USB,
   открыть Boot Menu и загрузить UEFI USB. Виртуализацию Intel VT-x оставить
   включённой; Secure Boot заранее отключать не требуется для PVE 9.2.
4. Выбрать Install Proxmox VE (Graphical); при проблемах с графикой — Terminal UI.
5. Выбрать внутренний NVMe `/dev/nvme0n1`, около 512 GB / 476.9 GiB.
   **Установщик сотрёт весь этот диск: Ubuntu, K3s и существующие LVM volumes.**
   Свободная часть текущего VG также переразмечается.
6. В Options выбрать ext4: `maxroot=64`, `swapsize=8`, `minfree=16` (GiB),
   `hdsize` — весь диск, `maxvz` оставить default/auto. Остаток будет LVM-thin
   storage для VM. Это практичный вариант для одного NVMe и 16 GB RAM.
7. Country Kazakhstan, timezone Asia/Almaty, keyboard English (US).
   Задать свой сильный root password и email; пароль хранить в менеджере паролей,
   не отправлять в чат.
8. Network: выбрать проводной Realtek с MAC из приватной памятки и подключённым
   кабелем. Имя интерфейса может отличаться от Ubuntu. FQDN `pve.home.arpa`,
   IP/prefix/gateway/DNS — точно из памятки. Не принимать случайный DHCP address.
   Installer создаст management bridge `vmbr0` с этим Ethernet uplink.
9. На итоговом экране перепроверить диск и сеть, затем Install.
   После завершения убрать USB и загрузиться с внутреннего NVMe.
10. Открыть с Mac `https://<адрес-из-памятки>:8006`, войти `root`, realm
    Linux PAM. Начальный сертификат самоподписанный; сверить адрес с консолью.
11. На консоли Proxmox выполнить:

    ```sh
    ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub
    ```

    На Mac в Terminal запустить локальный wrapper:

    ```sh
    ./secrets/proxmox-install/authorize-proxmox.sh
    ```

    Сравнить fingerprint с консолью и только затем подтвердить SSH host key.
    Root password вводится в Terminal. Wrapper добавит отдельный публичный
    ключ и проверит версию PVE, `vmbr0` и storage; пароль не сохраняется.
12. Оставить Ethernet подключённым и сообщить, что установлен Proxmox и ключ добавлен.
    Если сеть не поднялась, использовать локальную консоль, не переустанавливать
    диск повторно. Кабель нужен до проверки VM и отдельного Wi-Fi проекта.

Сервисы будут недоступны с момента стирания диска до восстановления VM.
На этом этапе не создавать вручную HAOS/K3s VM: их ресурсы опишем в IaC.

## После установки: последовательность работ

1. Проверить PVE version/storage/bridge, обновления, время, firewall и доступ по
   выделенному ключу; получить CA certificate доверенным SSH-каналом.
2. Подготовить ограниченный API token для Terraform, закрепить provider,
   закрытый state и его backup. Проверить plan до apply.
3. Создать HAOS VM (предварительно 2 vCPU / 3 GiB / 32 GiB) и Linux/K3s VM
   (8 vCPU / 9 GiB / 200 GiB). Оставить около 3 GiB гипервизору; окончательно
   проверить бюджет памяти и диска перед apply. IP guests выбрать после проверки
   DHCP/reservations, не угадывать свободные адреса. HAOS guest UEFI без Secure Boot
   настраивается отдельно от Secure Boot физического хоста.
4. Восстановить HA с учётом supported backup/import path в HAOS; текущий cold
   archive содержит config, но не является готовым Supervisor backup.
   Восстановить остальные PVC в новую Linux VM, импортировать собственные images,
   развернуть K3s/Helm charts и dashboard JSON из репозитория.
5. Заменить localhost dependencies между HAOS и K3s на адреса VM; проверить
   Wyoming, Matter/multicast, MA, Hermes/MCP, callback/media URLs станции.
6. Создать Tailscale OAuth client/tags в Admin, сохранить credentials только
   приватно, развернуть pinned Operator и проверить тестовый Service/ACL/TLS.
   Затем перенести внешние точки входа HA и Grafana.
7. Проверить полный voice/media pipeline, dashboards/metrics/logs, reboot обеих VM
   и хоста, резервное копирование. Только после приёмки убирать временные настройки.

Переход обратно на Wi-Fi — отдельная проверяемая задача: client Wi-Fi нельзя
автоматически считать эквивалентом Ethernet bridge. Нужны проверка драйвера PVE
и выбранная routed/NAT схема с тестами discovery и media URLs.

## Источники

- [Официальный ISO и checksum](https://www.proxmox.com/en/downloads/proxmox-virtual-environment/iso/proxmox-ve-9-2-iso-installer)
- [Установка и параметры диска](https://github.com/proxmox/pve-docs/blob/master/pve-installation.adoc)
- [Установочный USB](https://github.com/proxmox/pve-docs/blob/master/pve-installation-media.adoc)
- [Сетевые схемы PVE](https://github.com/proxmox/pve-docs/blob/master/pve-network.adoc)
- [HAOS VM](https://www.home-assistant.io/installation/alternative)
- [Границы IaC и Operator](PROXMOX-HAOS-IAC.md)
