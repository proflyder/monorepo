# SETUP — воспроизводимый сетап неттопа

Ниже — порядок, которым собран текущий сервер. Команды выполняются на самом
неттопе (или по SSH с Mac). Секреты (Wi-Fi PSK, HA-пароль) не входят в этот
документ.

## 0. Что получится

- Ubuntu Server 26.04.1 LTS, hostname `lab-host`;
- Ethernet `192.0.2.26`, Wi-Fi `192.0.2.25`;
- Docker + Home Assistant Container;
- Secure Boot включён, DKMS Wi-Fi подписан MOK;
- Wi-Fi 5 ГГц, failover Ethernet↔Wi-Fi.

## 1. Железо и BIOS

DEXP Jupiter JU01CY: i7-12650H, 16 ГБ DDR4, 512 ГБ NVMe.

В BIOS:

- VT-x — enabled (нужно для KVM/VM в будущем);
- UEFI — enabled;
- Secure Boot — **сначала можно выключить**, включим после MOK;
- Fast Boot — можно оставить, но для установки удобнее выключить;
- boot order — Ubuntu/SSD.

Вход в BIOS: **Del** или **F2** при включении; Boot Menu — **F11/F12**.

> Если ставится HAOS VM, Secure Boot обычно выключают. Для текущего
> HA Container Secure Boot можно включить после MOK.

## 2. Ubuntu Server

Установить Ubuntu Server 26.04.1 LTS на весь NVMe (LVM по умолчанию).
Пользователь: `labuser`. OpenSSH — да.

После установки:

```bash
sudo timedatectl set-timezone Asia/Almaty
sudo apt update
sudo apt install -y docker.io docker-compose-v2 git dkms build-essential bc iw wireless-regdb
sudo systemctl enable --now docker
sudo usermod -aG docker labuser
```

Перелогиниться (или новый SSH-сеанс), проверить:

```bash
docker ps
docker compose version
```

### 2.1. sudo без пароля (опционально)

Используется для удобства; это фактически root-доступ.

```bash
echo 'labuser ALL=(ALL:ALL) NOPASSWD:ALL' | sudo tee /etc/sudoers.d/90-labuser-nopasswd >/dev/null
sudo chmod 440 /etc/sudoers.d/90-labuser-nopasswd
sudo visudo -cf /etc/sudoers.d/90-labuser-nopasswd
```

Откат:

```bash
sudo rm /etc/sudoers.d/90-labuser-nopasswd
```

### 2.2. zsh + Oh My Zsh

```bash
sudo apt install -y zsh
git clone --depth=1 https://github.com/ohmyzsh/ohmyzsh.git "$HOME/.oh-my-zsh"
# если ~/.zshrc ещё нет:
cp "$HOME/.oh-my-zsh/templates/zshrc.zsh-template" "$HOME/.zshrc"
sudo chsh -s /usr/bin/zsh labuser
```

Проверка:

```bash
getent passwd labuser          # shell = /usr/bin/zsh
zsh -lic 'echo $ZSH; echo $ZSH_THEME; echo $plugins'
```

Ожидаем `~/.oh-my-zsh`, тему `robbyrussell`, плагин `git`. Изменения
применятся в новом SSH-сеансе.

## 3. Статические IP (netplan)

Проверить интерфейсы:

```bash
ip -br link
```

Ожидаем `enp3s0` (Ethernet) и `wlp4s0` (Wi-Fi). MAC-адреса — в
[`INVENTORY.md`](INVENTORY.md).

### 3.1. Ethernet

`/etc/netplan/00-installer-config.yaml`:

```yaml
network:
  version: 2
  ethernets:
    enp3s0:
      dhcp4: no
      dhcp6: no
      match:
        macaddress: "02:00:00:00:00:04"
      set-name: enp3s0
      addresses: ["192.0.2.26/24"]
      routes:
        - to: default
          via: 192.0.2.1
          metric: 100
      nameservers:
        addresses: ["192.0.2.1"]
```

### 3.2. Wi-Fi

`/etc/netplan/50-wifi.yaml` (шаблон — `projects/home-assistant/configs/50-wifi.yaml.example`):

```yaml
network:
  version: 2
  renderer: networkd
  wifis:
    wlp4s0:
      optional: true
      dhcp4: no
      addresses: ["192.0.2.25/24"]
      routes:
        - to: default
          via: 192.0.2.1
          metric: 200
      nameservers:
        addresses: ["192.0.2.1"]
      access-points:
        "Stanislav1_Wi-Fi5":
          password: "<WIFI_PSK>"
```

Права:

```bash
sudo chmod 600 /etc/netplan/00-installer-config.yaml /etc/netplan/50-wifi.yaml
sudo netplan generate
sudo netplan apply
```

Если SSH-сессия идёт через Ethernet, применять можно так:

```bash
sudo systemd-run --unit=netplan-apply --collect /usr/sbin/netplan apply
```

Проверка:

```bash
ip -br addr
ip route
ping -c 2 1.1.1.1
```

Должно быть два default route: Ethernet metric 100, Wi-Fi metric 200.

### 3.3. Boot при вынутом Ethernet

`wlp4s0` помечен `optional: true`, поэтому netplan исключает Wi-Fi из
`systemd-networkd-wait-online`; без кабеля сервис ждёт `enp3s0` 120 с и
загружает HA только после таймаута. В текущем сетапе стоит drop-in, который
ждёт **любой** из интерфейсов максимум 30 с:

```bash
sudo mkdir -p /etc/systemd/system/systemd-networkd-wait-online.service.d
sudo cp projects/home-assistant/configs/systemd-networkd-wait-online.service.d/10-netplan.conf \
  /etc/systemd/system/systemd-networkd-wait-online.service.d/10-netplan.conf
sudo systemctl daemon-reload
sudo systemctl restart systemd-networkd-wait-online
```

Файл:

```ini
[Service]
ExecStart=
ExecStart=/lib/systemd/systemd-networkd-wait-online --any --timeout=30 -i enp3s0 -i wlp4s0
```

Проверка:

```bash
systemctl cat systemd-networkd-wait-online.service | grep ExecStart
systemctl is-system-running
systemctl --failed
```

После reboot с вынутым Ethernet SSH должен вернуться примерно за 20 с, а
`wait-online` — завершиться успешно (не `failed`).

## 4. Wi-Fi RTL8821CE: почему не mainline

На Ubuntu 26.04 / kernel 7.0 встроенный `rtw88_8821ce` падает:

```text
failed to send h2c command
firmware failed to leave lps state
```

Симптомы: загрузка/сканирование нестабильны, консоль засыпает h2c-ошибками.
Лечение в этом сетапе — вендорский DKMS-драйвер.

### 4.1. Blacklist mainline

`/etc/modprobe.d/blacklist-rtw88.conf`:

```text
blacklist rtw88_8821ce
blacklist rtw88_8821c
```

`/etc/modprobe.d/rtw88.conf` (на случай возврата mainline):

```text
options rtw88_core disable_lps_deep=y
options rtw88_pci disable_aspm=y
options rtw88_pci disable_msi=y
```

`/etc/modprobe.d/blacklist-bt.conf` (combo-чип; Bluetooth не нужен):

```text
blacklist btusb
blacklist bluetooth
blacklist btmtk
blacklist btrtl
blacklist btbcm
blacklist btintel
```

`/etc/modules-load.d/8821ce.conf`:

```text
8821ce
```

### 4.2. Сборка DKMS

```bash
git clone --depth=1 https://github.com/tomaspinho/rtl8821ce.git /tmp/rtl8821ce
cd /tmp/rtl8821ce
# применить патч из этой папки:
patch -p1 < /path/to/projects/home-assistant/patches/rtl8821ce-ubsan.patch
sudo ./dkms-install.sh
sudo update-initramfs -u
```

Патч убирает UBSAN-предупреждения для flexible-array `pIE->data` и
`crypt.key` (см. `patches/rtl8821ce-ubsan.patch`).

После сборки:

```bash
dkms status
modinfo 8821ce | grep -E 'filename|version'
```

### 4.3. Secure Boot и MOK

Если Secure Boot включён, модуль должен быть подписан MOK и ключ зарегистрирован:

```bash
sudo mokutil --import /var/lib/shim-signed/mok/MOK.der
```

Задать временный пароль, перезагрузиться, в **MOK Manager**:

```text
Enroll MOK → Continue → Yes → <пароль> → Reboot
```

Если MOK Manager не появляется через shim, можно однократно загрузить
`mmx64.efi`:

```bash
sudo efibootmgr -c -d /dev/nvme0n1 -p 1 -L "MOK Manager" -l '\EFI\ubuntu\mmx64.efi'
sudo efibootmgr -n <N>
```

После enrollment удалить временный entry:

```bash
sudo efibootmgr -b <N> -B
```

Проверка:

```bash
sudo mokutil --sb-state
sudo mokutil --test-key /var/lib/shim-signed/mok/MOK.der
```

Ожидаем `SecureBoot enabled` и `is already enrolled`.

> Не удалять `/var/lib/shim-signed/mok/` — иначе будущие DKMS-сборки не
> загрузятся при Secure Boot.

## 5. Единый стек HA + Music Assistant + Assist base

Рабочая папка:

```text
/home/labuser/nettop/
  docker-compose.yml
  ctl.sh
  homeassistant/config/
  data/music-assistant/
  data/whisper/
  data/piper/
```

Скопировать `projects/home-assistant/stack/` на сервер и запустить:

```bash
cd /home/labuser/nettop
cp .env.example .env
./ctl.sh up
```

`docker-compose.yml` содержит HA, MA, Whisper и Piper; все в
`network_mode: host`.

Версии фиксируются в `.env`:

```dotenv
HA_IMAGE=ghcr.io/home-assistant/home-assistant:2026.10.4
MA_IMAGE=ghcr.io/music-assistant/server:2.10.8
WHISPER_IMAGE=rhasspy/wyoming-whisper:3.8.1
WHISPER_MODEL=medium-int8
WHISPER_LANGUAGE=ru
PIPER_IMAGE=rhasspy/wyoming-piper:2.5.2
PIPER_VOICE=ru_RU-irina-medium
```

Проверка:

```bash
./ctl.sh status
curl -sSL -o /dev/null -w 'HA=%{http_code}\n' http://127.0.0.1:8123/
curl -sSL -o /dev/null -w 'MA=%{http_code}\n' http://127.0.0.1:8095/
```

Открыть:

- HA: `http://192.0.2.26:8123` / `http://192.0.2.25:8123`
- MA: `http://192.0.2.26:8095` / `http://192.0.2.25:8095`

MA setup завершён, Spotify подключён. HA Assist base: Wyoming integrations
`faster-whisper`/`piper`, pipeline `Рикардо` (RU, HA intents). HA integration MA
добавлена 2026-10-26; Spotify 429 снят 2026-10-27, search/play проверены.
и Pulsar — следующие этапы, см. [`stack/README.md`](stack/README.md).

Подробности HA — [`HA.md`](HA.md); единая точка входа — `stack/ctl.sh`.

## 6. Проверка

```bash
sudo ./scripts/verify.sh
```

Скрипт проверяет: IP/маршруты, Secure Boot/MOK, DKMS, модуль, Wi-Fi, HA, MA,
Whisper/Piper и контейнеры.

## 7. Обновление ядра / драйвера

При обновлении ядра DKMS должен пересобрать `8821ce`. Проверить:

```bash
dkms status
sudo update-initramfs -u
```

Если после обновления Wi-Fi не поднялся — сначала Ethernet, затем:

```bash
sudo dkms install rtl8821ce/v5.5.2_34066.20200325 --force
sudo update-initramfs -u
sudo reboot
```

## 8. Откат

- **Wi-Fi**: вернуть mainline — удалить `blacklist-rtw88.conf`, удалить DKMS
  `rtl8821ce`, `sudo update-initramfs -u`, reboot. Но mainline h2c-баг вернётся.
- **HA/MA**: `cd /home/labuser/nettop && ./ctl.sh down`; конфиг и данные
  останутся в `homeassistant/config/` и `data/music-assistant/`.
- **Сеть**: вернуть DHCP в netplan или временно подключить Ethernet.
- **Secure Boot**: выключить в BIOS; MOK-ключ можно оставить.
- **sudo NOPASSWD**: удалить `/etc/sudoers.d/90-labuser-nopasswd`.

## 9. Безопасность

- 8123 — только LAN, не пробрасывать на роутере.
- Если нужен удалённый доступ — WireGuard/Tailscale, не прямой port-forward.
- HA-аккаунт — сильный пароль + MFA, когда дойдём.
- MOK private key — root-only, забэкапить в зашифрованное хранилище.
- Wi-Fi PSK — в `secrets/`, не в этой папке.
- `labuser` в группе `docker` = root-эквивалент; NOPASSWD добавляет удобство,
  но не новую степень доступа.
