# Proxmox Wi-Fi uplink с сохранением L2

Wi-Fi RTL8821CE остаётся на Proxmox и работает в 4-address/WDS client mode.
`vmbr0` сохраняет management IPv4/MAC и виртуальные NIC обеих VM; адреса
HAOS/K3s и application endpoints не меняются. NAT, PCI passthrough и
внутренняя routed сеть не нужны. IPv6 link-local/multicast остаются в LAN.

**Не универсальная возможность любого Wi-Fi:** и драйвер, и AP должны
передавать кадры 4-address. На текущем оборудовании это проверено отдельным
network namespace с другим MAC: DHCP, gateway ping и IPv6 multicast.
После смены роутера/firmware/драйвера проверку повторять. Обычный ping самого
Proxmox не доказывает передачу трафика VM.

## Политика uplink

`ricardo-uplink.service` управляет только двумя physical bridge ports:

- `auto`: Ethernet с carrier приоритетен; без carrier используется Wi-Fi;
- прежде чем подключить один uplink, другой отсоединяется;
- guest tap-интерфейсы сохраняются;
- Ethernet держится UP даже вне моста, чтобы обнаружить подключение кабеля;
- Wi-Fi powersave выключен; wpa_supplicant получает `-b vmbr0` для EAPOL;
- `bridge-ports none` оставляет выбор uplink сервису; MAC моста закреплён,
  multicast snooping выключен для discovery.

**Нельзя одновременно вручную добавлять Ethernet и Wi-Fi в мост:** это два
пути в одну LAN, которые без отдельной STP-схемы создадут петлю.

## Установка

Выполнять на PVE после успешной отдельной canary-проверки WDS, с Ethernet
и локальной консолью для отката. Не переносить карту в VM параллельно.
Нужны `python3`, `iw`, `wpasupplicant`, firmware для адаптера.

Приватный профиль wpa_supplicant (0600) содержит SSID/PSK и хранится вне Git:

```bash
sudo python3 install.py --profile /root/private/wifi.conf \
  --bridge vmbr0 --wired nic0 --wifi wlp4s0
```

Installer сохраняет previous config в root-only timestamp directory,
закрепляет существующий MAC/IPv4 моста, устанавливает daemon/systemd units
и закрытый профиль. Перед первым live-переключением подготовить timed
rollback на Ethernet. Installer не создаёт профиль и не печатает credentials.

Исходный public source и private профиль — разные артефакты: профиль нужен
при disaster restore, Git его не содержит. На текущем PVE профиль находится
в `/etc/wpa_supplicant/wpa_supplicant-wlp4s0.conf`; закрытая копия сетевой
конфигурации сохранена отдельно от хоста.

## Диагностика и приёмка

```bash
systemctl status ricardo-uplink wpa_supplicant@wlp4s0
journalctl -u ricardo-uplink -n 20
cat /sys/class/net/nic0/carrier
readlink /sys/class/net/wlp4s0/master
wpa_cli -i wlp4s0 status | grep '^wpa_state='
```

При Wi-Fi: carrier Ethernet = 0, Wi-Fi master = vmbr0,
wpa_state = COMPLETED. Не публиковать полный wireless status: он содержит
SSID/MAC. Временный режим задаётся root-only `/run/ricardo-uplink.mode`
со значением `wifi`, `ethernet` или `auto`; после удаления файла и reboot
используется `MODE` из `/etc/default/ricardo-uplink` (обычно auto).

Нужны тесты: доступ по SSH/PVE, HA/MA/Grafana, K3s acceptance через tunnel,
Tailscale HTTPS, связь HAOS→STT/TTS/Hermes, IPv6 multicast и прямое чтение
Matter-устройства. Затем физически вынуть Ethernet и проверить reboot.
Смена uplink — короткий разрыв связи, не бесшовная агрегация каналов.

## Восстановление

При проблемах Wi-Fi подключить Ethernet: auto-policy выберет его по carrier,
VM и host сохранят прежние адреса. При ручном force-wifi сначала удалить
`/run/ricardo-uplink.mode` с консоли. Для полного отката с консоли остановить
и disable `ricardo-uplink`, остановить Wi-Fi supplicant, отсоединить Wi-Fi,
подключить Ethernet к vmbr0 и вернуть сохранённый `/etc/network/interfaces`.
На текущем PVE сохранён `/root/wifi-cutover/rollback.sh`; выполнять его
только с подключённым Ethernet. Сам daemon при остановке не разбирает мост.

## Результат приёмки на текущем стенде, 2026-10-28

Физически отключён Ethernet, выполнены две полные штатные перезагрузки PVE.
Host uplink и VM возвращаются автоматически, прежние IP/MAC сохраняются.
K3s network CIDR/gateway заданы в private Terraform variables; cloud-init
сохраняет host keys, netplan сохраняет MAC match/set-name eth0. Нельзя
удалять эти поля из guest netplan: иначе Linux вернёт predictable NIC name,
а node-ip/flannel-iface/firewall ожидают eth0.

Пройдены PVE TLS, cluster 5/5, HA 3/3, Hermes 4/4, STT/TTS smoke,
IPv6 multicast и физический Matter attribute read. Firewall пережил reboot.
Router reservations созданы для host/VM и станции без изменения DHCP-пула.

Sendspin станции после reboot не находился по IPv4 mDNS. В штатной настройке
MA Sendspin provider **Manual IP addresses for discovery** добавлен закрытый
адрес станции; повторный restart MA подтвердил reconnect. Pairing не сбрасывали,
station firmware не меняли, воспроизведение не запускали. При восстановлении
MA вернуть эту настройку из backup либо добавить фиксированный адрес станции
через UI/API. Пример адреса в публичном стенде: `192.0.2.13`.
Подтверждение IPv6 multicast не означает, что все IPv4 mDNS service types
будут автоматически обнаруживаться через конкретный AP.

Private network/IaC recovery и новый native partial MA backup с SHA-256
хранятся отдельно на Mac; full disaster restore не выполнялся.

Источники: [Linux Wireless 4-address](https://wireless.docs.kernel.org/en/latest/en/users/documentation/iw.html),
[Music Assistant Sendspin settings](https://www.music-assistant.io/player-support/sendspin/).
