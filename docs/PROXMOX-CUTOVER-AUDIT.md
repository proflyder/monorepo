# Проверка миграции Proxmox / HAOS / K3s — 2026-10-28

Это протокол первоначальной проверки. Последующее закрытие Wi-Fi описано
в `STATE.md` и `projects/home/proxmox/wifi-uplink/README.md`; ниже сохранены
находки на момент начала аудита.

Проверено текущее оборудование по SSH, а не только отчёты предыдущей сессии.
Итог: сервисная миграция работает на Ethernet; исходное требование работы
неттопа по Wi-Fi ещё не выполнено. Отключать кабель пока нельзя.

## Подтверждено

- PVE 9.2.24/kernel 7.0.14-26-pve; обе VM running, onboot/protection включены.
  HAOS: 2 CPU/4 GiB/32 GiB; K3s: 8 CPU/8 GiB/200 GiB.
- Terraform plan с актуальным root-only state на PVE: No changes.
- HAOS Core/Supervisor 2026.10.4 healthy/supported; MA 2.10.8 и Matter 9.2.0
  add-ons started. Перенос этих двух сервисов из K3s в HAOS согласуется с
  целью использования HAOS; возвращать их в K3s оснований нет.
- Все 18 pod'ов Running/Ready, 9 PVC Bound. Kubernetes acceptance 5/5:
  workloads, retained volumes, четыре dashboard UID, семь endpoint metrics,
  наличие infrastructure logs в Loki. Dashboard ConfigMap управляется chart.
- HA read-only 3/3; Hermes 4/4, включая реальный chat turn.
- Прежний HTTPS HA URL и Grafana subpath работают; DNS принадлежит Operator
  proxy с tag:k8s. Старый host Serve не является serving path.
- Failed systemd units отсутствуют на PVE/Ubuntu. На PVE около 2 GiB
  available, swap не используется; Ubuntu около 3.2 GiB available,
  memory PSI avg10/60/300 = 0. На диске VM около 156 GiB свободно.
- 16 файлов текущего локального nettop snapshot прошли SHA-256.
  Это snapshot конфигурации/selected recovery, не полный backup дисков VM.
- Компонентные тесты Proxmox 4/4, deploy 5/5; Markdown audit без ошибок;
  Gitleaks истории develop без находок. Приватные архивы не входят в эту
  проверку публичной истории. Архивные refs не являются публикационной веткой.

## Найдено и исправлено

Документы утверждали, что management firewall уже установлен, однако live
6443/10250/9100/9116 были доступны с Mac из LAN. На Ubuntu отсутствовал
ricardo-k3s-firewall.service; работало только ограничение Hermes 8643.
Добавлены source-controlled правила и service в
`projects/home/kubernetes/management-firewall/`, установлены на VM.
Правила разрешают localhost/node/pod CIDR, закрывают указанные TCP-порты
и UDP 8472 для остальных источников, включая IPv6. SSH и Grafana NodePort
сохранены. Повторный LAN TCP probe подтвердил блокировку; acceptance 5/5
и HA read-only 3/3 повторно прошли через SSH tunnel. Reboot persistence
настроена через enabled service, фактический reboot в этой проверке не делался.

Исправлены сломанная ссылка NETTOP и неверные утверждения о сохранённых
MA/Matter PV и достаточности статических адресов для предотвращения
DHCP-конфликта.

## Что осталось до полной приёмки

1. **Wi-Fi.** На PVE wlp4s0 DOWN; PCI mapping подготовлен, passthrough нет,
   vmbr1/routed network нет. Обе VM используют vmbr0/Ethernet.
   Нельзя считать этот этап завершённым. Перенос Matter в HAOS меняет
   прежний вариант «Wi-Fi напрямую K3s»: HAOS также нужен рабочий multicast/
   IPv6 discovery. До исполнения следует уточнить сетевую схему для add-on,
   затем проверить Matter, MA/Sendspin, host management и холодный старт
   с физически отключённым Ethernet. Сам по себе IPv4 NAT не доказывает
   работоспособность Matter discovery.
2. **DHCP.** Известный LAN DHCP-пул включает статические адреса host/VM.
   Static в guest не запрещает роутеру выдать тот же адрес другому клиенту.
   Нужны reservations/exclusions с проверкой текущего состояния роутера.
3. **Воспроизводимость.** Terraform описывает VM hardware, но K3s cloud-init
   всё ещё задаёт DHCP; реальные static netplan/HAOS network, firewall и
   add-on data restore — отдельные операции. No changes не проверяет эти
   настройки. Нужен единый последовательный runbook/automation для них;
   Grafana LAN Service также применяется отдельным manifest, не Helm chart.
4. **Восстановление.** Проверены checksum snapshot и наличие private recovery,
   но чистое восстановление на запасной VM/железе не выполнялось. Нужны
   регулярные актуальные off-host backups PV/HA и проверка restore;
   cold archive до установки не заменяет backup новых данных.
5. **Приёмка.** Общий host reboot/power-loss, физический микрофон/wake,
   alarm stop/resume и MA/Spotify transfer в этой проверке не запускались.
   Три mutating/audio HA теста явно skipped; read-only тесты их не заменяют.
   TCP health checks не доказывают управление Matter-устройством.
6. **Аварийный доступ.** Proxmox не имеет собственного работающего Tailscale
   management path; сейчас аварийный путь — Ethernet/локальная консоль.

Рабочие сервисы, station firmware, музыка и будильники ради аудита не
перезапускались и не менялись. Сетевой Wi-Fi cutover требует отдельной
проверяемой реализации с сохранением Ethernet rollback.
