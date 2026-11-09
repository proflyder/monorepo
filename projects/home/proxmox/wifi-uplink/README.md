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

`home-uplink.service` управляет только двумя physical bridge ports:

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
systemctl status home-uplink wpa_supplicant@wlp4s0
journalctl -u home-uplink -n 20
cat /sys/class/net/nic0/carrier
readlink /sys/class/net/wlp4s0/master
wpa_cli -i wlp4s0 status | grep '^wpa_state='
```

При Wi-Fi: carrier Ethernet = 0, Wi-Fi master = vmbr0,
wpa_state = COMPLETED. Не публиковать полный wireless status: он содержит
SSID/MAC. Временный режим задаётся root-only `/run/home-uplink.mode`
со значением `wifi`, `ethernet` или `auto`; после удаления файла и reboot
используется `MODE` из `/etc/default/home-uplink` (обычно auto).

Нужны тесты: доступ по SSH/PVE, HA/MA/Grafana, K3s acceptance через tunnel,
HTTPS-имена, связь HAOS→STT/TTS/Hermes, IPv6 multicast и прямое чтение
Matter-устройства. Затем физически вынуть Ethernet и проверить reboot.
Смена uplink — короткий разрыв связи, не бесшовная агрегация каналов.

## Восстановление

При проблемах Wi-Fi подключить Ethernet: auto-policy выберет его по carrier,
VM и host сохранят прежние адреса. При ручном force-wifi сначала удалить
`/run/home-uplink.mode` с консоли. Для полного отката с консоли остановить
и disable `home-uplink`, остановить Wi-Fi supplicant, отсоединить Wi-Fi,
подключить Ethernet к vmbr0 и вернуть сохранённый `/etc/network/interfaces`.
Сам daemon при остановке не разбирает мост.

## Ограничения стенда

- Netplan гостя K3s сохраняет MAC match/set-name `eth0`: без этих полей Linux
  вернёт predictable NIC name, а node-ip/flannel-iface/firewall ожидают `eth0`.
- Sendspin станции после reboot не находится по IPv4 mDNS через AP: в MA
  Sendspin provider **Manual IP addresses for discovery** задан адрес станции
  (пример: `192.0.2.13`). При восстановлении MA вернуть эту настройку.

Источники: [Linux Wireless 4-address](https://wireless.docs.kernel.org/en/latest/en/users/documentation/iw.html),
[Music Assistant Sendspin settings](https://www.music-assistant.io/player-support/sendspin/).
