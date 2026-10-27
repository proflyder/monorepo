# TROUBLESHOOTING — неттоп

## 1. `rtw88_8821ce: failed to send h2c command`

Симптомы:

```text
failed to send h2c command
firmware failed to leave lps state
```

Причина: встроенный `rtw88_8821ce` на Ubuntu 26.04/kernel 7.0 нестабилен с
RTL8821CE. Это не netplan и не роутер.

Лечение в этом сетапе:

1. `blacklist rtw88_8821ce` и `rtw88_8821c`;
2. DKMS-драйвер `rtl8821ce` (tomaspinho);
3. Bluetooth combo-чипа отключён;
4. Secure Boot + MOK (если Secure Boot включён).

Проверка:

```bash
lsmod | grep -E '8821|rtw88'
dkms status
sudo dmesg | grep -c 'failed to send h2c'
```

Ожидаем `8821ce`, `h2c=0`.

## 2. UBSAN `array-index-out-of-bounds`

Симптомы:

```text
UBSAN: array-index-out-of-bounds in core/rtw_wlan_util.c...
index 1 is out of range for type 'u8 [1]'
...
ioctl_cfg80211.c...
index 16 is out of range for type 'u8 [*]'
```

Это false-positive вендорского драйвера на flexible-array (`pIE->data`,
`crypt.key`). Патч:

```bash
patch -p1 < patches/rtl8821ce-ubsan.patch
sudo dkms remove rtl8821ce/v5.5.2_34066.20200325 --all
sudo dkms install rtl8821ce/v5.5.2_34066.20200325 --force
sudo update-initramfs -u
sudo reboot
```

**Важно:** без `update-initramfs -u` загрузится старый модуль из initramfs,
и UBSAN останется.

Проверка:

```bash
sudo dmesg | grep -c UBSAN
```

Ожидаем `0`.

## 3. Secure Boot: `Key was rejected by service`

Причина: DKMS-модуль подписан локальным MOK, ключ не зарегистрирован.

Проверка:

```bash
sudo mokutil --sb-state
sudo mokutil --test-key /var/lib/shim-signed/mok/MOK.der
```

Если `not enrolled`:

```bash
sudo mokutil --import /var/lib/shim-signed/mok/MOK.der
```

Задать временный пароль, перезагрузиться, в **MOK Manager** выполнить
`Enroll MOK → Continue → Yes → пароль → Reboot`.

Если MOK Manager не появляется через shim:

```bash
sudo efibootmgr -c -d /dev/nvme0n1 -p 1 -L "MOK Manager" -l '\EFI\ubuntu\mmx64.efi'
sudo efibootmgr -n <N>
```

Одноразово загрузиться в MOK Manager, зарегистрировать ключ, затем удалить entry:

```bash
sudo efibootmgr -b <N> -B
```

Не удалять `/var/lib/shim-signed/mok/`.

## 4. Wi-Fi не поднимается после reboot

Проверить:

```bash
ip -br addr
ip route
sudo wpa_cli -i wlp4s0 status
sudo iw dev wlp4s0 info
networkctl status wlp4s0
```

Если интерфейс DOWN:

```bash
sudo ip link set wlp4s0 up
sudo wpa_cli -i wlp4s0 reconnect
```

Если нет IP:

```bash
sudo netplan generate
sudo netplan apply
```

Если IP конфликтует с Ethernet (оба `.26` или `.25`), проверить netplan:
адреса должны быть статическими и разными (`projects/home-assistant/configs/`).

## 5. Ethernet/Wi-Fi failover

Ожидаемо:

```text
default via 192.0.2.1 dev enp3s0 metric 100
default via 192.0.2.1 dev wlp4s0 metric 200
```

При отключении Ethernet трафик уходит в Wi-Fi; после возврата кабеля —
обратно. Если нет — проверить `ip route` и `netplan`.

## 6. HA или Music Assistant не открываются

```bash
cd /home/labuser/nettop
./ctl.sh status
./ctl.sh logs ha
./ctl.sh logs ma
ss -lntp | grep -E ':(8123|8095|8097|8927|10200|10300)\b'
curl -v http://127.0.0.1:8123/
curl -v http://127.0.0.1:8095/
```

Типовые причины:

- контейнер не Up;
- ошибка в `configuration.yaml` / MA config;
- порт 8123/8095/10200/10300 занят;
- сеть/маршрут;
- сервис ещё стартует (первые 10–30 секунд);
- MA запущен не с `network_mode: host`;
- MA UI отвечает `302` на `/setup` — это нормально до завершения setup.

### 6.1. Whisper/Piper не работают

```bash
cd /home/labuser/nettop
./ctl.sh logs whisper
./ctl.sh logs piper
docker compose ps
ss -lntp | grep -E ':(10200|10300)\b'
```

- Whisper должен быть `healthy`, порт `10300`.
- Piper должен быть `healthy`, порт `10200`.
- В HA: Settings → Devices & Services → Wyoming Protocol; сервисы
  `faster-whisper` и `piper`.
- В pipeline `Рикардо`: STT `faster-whisper`, TTS `piper`,
  голос `ru_RU-irina-medium`.
- Если Piper не видит голос — проверить `data/piper` и лог загрузки voice.

## 7. HA Container и add-ons

Если нужен add-on — его нет в Container. Варианты:

- заменить на отдельный контейнер;
- перейти на HAOS VM (KVM есть).

## 8. Обновление ядра сломало Wi-Fi

```bash
dkms status
sudo dkms install rtl8821ce/v5.5.2_34066.20200325 --force
sudo update-initramfs -u
sudo reboot
```

Если DKMS-исходник потерян — клонировать `tomaspinho/rtl8821ce`, применить
`patches/rtl8821ce-ubsan.patch`, `./dkms-install.sh`.

## 9. Boot 120 s / `degraded` при вынутом Ethernet

Симптом:

```text
systemd-networkd-wait-online.service: Timeout occurred while waiting for network connectivity.
systemd-networkd-wait-online.service: Failed with result 'exit-code'.
systemd is-system-running: degraded
```

Причина: `wlp4s0` в netplan помечен `optional: true`, поэтому netplan исключает
Wi-Fi из wait-online; сервис ждёт только `enp3s0`. Если кабель вынут, он ждёт
120 с, и HA стартует только после таймаута. Доступ по Wi-Fi при этом работает.

**Статус:** в текущем сетапе применён вариант **B**. После reboot с вынутым
Ethernet: SSH вернулся через ~20 с, `wait-online` — `active (exited)`,
`systemctl is-system-running` — `running`, HA — `200`.

Варианты фикса:

**A. Простой (официальный netplan):** Ethernet — optional, Wi-Fi — required.

```yaml
# 00-installer-config.yaml
    enp3s0:
      optional: true

# 50-wifi.yaml
    wlp4s0:
      # optional: true  ← убрать
```

Тогда boot ждёт Wi-Fi. Если Ethernet подключён, а Wi-Fi нет — будет ждать
Wi-Fi; это цена простоты.

**B. Универсальный (systemd drop-in, применён):** ждать любой из двух
интерфейсов, максимум 30 с. Файл уже лежит в
`projects/home-assistant/configs/systemd-networkd-wait-online.service.d/10-netplan.conf`.

```bash
sudo mkdir -p /etc/systemd/system/systemd-networkd-wait-online.service.d
sudo cp projects/home-assistant/configs/systemd-networkd-wait-online.service.d/10-netplan.conf \
  /etc/systemd/system/systemd-networkd-wait-online.service.d/10-netplan.conf
sudo systemctl daemon-reload
sudo systemctl restart systemd-networkd-wait-online
```

Проверка:

```bash
systemctl cat systemd-networkd-wait-online.service | grep ExecStart
systemctl status systemd-networkd-wait-online.service --no-pager
systemctl is-system-running
```

## 10. Rollback

- Wi-Fi: удалить `blacklist-rtw88.conf`, удалить DKMS `rtl8821ce`, reboot
  (вернётся h2c-баг).
- Secure Boot: выключить в BIOS; MOK можно оставить.
- HA: `docker compose down`; конфиг остаётся.
- Сеть: вернуть DHCP в netplan.
- sudo NOPASSWD: удалить `/etc/sudoers.d/90-labuser-nopasswd`.
