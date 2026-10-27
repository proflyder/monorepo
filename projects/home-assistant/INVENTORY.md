# INVENTORY — неттоп

**Runtime обновлён 2026-10-28:** Proxmox VE `9.2.24`; VM `100 haos`
(HAOS `2026.10.4`, `.28`) и VM `101 lab-k3s` (Ubuntu 24.04.5, K3s
`v1.35.9+k3s1`, `.27`); Docker/Compose retired. Terraform state — на PVE,
private copy вне Git. [Эксплуатация](../../docs/KUBERNETES-OPERATIONS.md).
Старые Compose paths/таблицы ниже — историческая схема, не текущий writer.


> Публичная копия: serial/MAC, внешний адрес и tailnet-домен заменены примерами.
> Реальные значения и исходные локальные документы — только в `secrets/`.
> LAN-адреса и remote-пути описывают стенд; перед применением адаптировать.

## Железо

| Параметр | Значение |
|---|---|
| Модель | DEXP Jupiter JU01CY |
| CPU | Intel Core i7-12650H, 10 ядер / 16 потоков |
| RAM | 16 ГБ DDR4 (доступно ~14 GiB из-за iGPU), 1 модуль, 2 слота |
| SSD | NVMe 476.9 ГБ (`nvme0n1`) |
| LVM | root `ubuntu--vg-ubuntu--lv`, 100 ГБ (можно расширить) |
| Сеть | 1 Гбит Ethernet, Wi-Fi RTL8821CE (2.4/5 ГГц), Bluetooth 5.x |
| BIOS | `5.27`, 2026-02-03; VT-x, Secure Boot, UEFI |
| Форм-фактор | 150 × 75.5 × 183.2 мм, 120 Вт БП |

## Сеть

Текущая схема:

| Узел | Интерфейс | IP | Примечание |
|---|---|---|---|
| Proxmox host | `vmbr0` / Ethernet | `192.0.2.26/24` | management |
| HAOS VM 100 | `enp6s18` | `192.0.2.28/24` | static (Supervisor) |
| K3s VM 101 | `eth0` | `192.0.2.27/24` | static (netplan) |
| HA compatibility | K3s hostNetwork | `192.0.2.27:8123` | HAProxy → HAOS |

Wi-Fi RTL8821CE на Proxmox host сейчас DOWN; PCI mapping создан, но VM не
передан. Ethernet обязателен до отдельного Wi-Fi/routed-проекта.

Историческая схема прежнего Ubuntu (не текущий writer):

| Интерфейс | IP | MAC | Metric | Примечание |
|---|---|---:|---:|---|
| `enp3s0` | `192.0.2.26/24` | `02:00:00:00:00:04` | 100 | основной |
| `wlp4s0` | `192.0.2.25/24` | `02:00:00:00:00:03` | 200 | резерв, 5 ГГц |
| gateway/DNS | `192.0.2.1` | | | |

## ОС и сервисы

| Параметр | Значение |
|---|---|
| Hostname | `lab-host` |
| OS | Ubuntu Server 26.04.1 LTS |
| Kernel | `7.0.0-38-generic` |
| Boot wait-online | drop-in `--any --timeout=30 -i enp3s0 -i wlp4s0` |
| Timezone | `Asia/Almaty` (+05) |
| Пользователь | `labuser`; shell `zsh` + Oh My Zsh; sudo NOPASSWD |
| Shell | zsh 5.9 + Oh My Zsh; тема `robbyrussell`, плагин `git` |
| OMZ path | `/home/labuser/.oh-my-zsh` |
| Docker | retired после перехода на K3s |
| Compose | retired; текущий deploy — Helm |
| HA | HAOS `2026.10.4` VM (was `ghcr.io/home-assistant/home-assistant:2026.10.4` Container) |
| HA URL | HAOS `http://192.0.2.28:8123`; compatibility `http://192.0.2.27:8123` |
| Music Assistant | HAOS add-on `2.10.8`; UI `.28:8095`, stream `:8097`, Sendspin `:8927` |
| Matter Server | HAOS add-on `core_matter_server 9.2.0`; `127.0.0.1:5580`; HA integration `Matter`; devices pending |
| Xiaomi Home | custom integration `v0.5.0`; HA custom_components; China login/device `MJGJD02YL` pending |
| MA URL | `http://192.0.2.28:8095`; add-on data перенесены из `runtime-music`, Spotify playback проверен |
| Stack root | `/home/labuser/nettop` |
| Stack entry | `/home/labuser/nettop/ctl.sh` |
| HA config | `/home/labuser/nettop/homeassistant/config` |
| HA compose | `/home/labuser/nettop/docker-compose.yml` |
| MA data | HAOS `/mnt/data/supervisor/apps/data/d5369777_music_assistant` |
| Matter data | HAOS `/mnt/data/supervisor/apps/data/core_matter_server` |
| GigaAM STT | `lmo3/wyoming-onnx-stt:latest`, `:10305`, `gigaam-v3-ctc` int8, CPU, ru |
| Whisper fallback | `rhasspy/wyoming-whisper:3.8.1`, `:10300`, `medium-int8`, ru |
| Piper | `rhasspy/wyoming-piper:2.5.2`, `:10200`, `ru_puriso2-medium` (Puriso; Irina сохранена) |
| Assist | pipeline `Рикардо`; Pulsar `v0.4.1-ricardo6`; wake word `Рикардо`; voice commands |
| Whisper data | `/home/labuser/nettop/data/whisper` |
| Piper data | `/home/labuser/nettop/data/piper` |

## Безопасность загрузки

| Параметр | Значение |
|---|---|
| Secure Boot | **enabled** |
| MOK | `lab-host Secure Boot Module Signature key` — enrolled |
| MOK files | `/var/lib/shim-signed/mok/MOK.{priv,der}` (root-only) |
| Boot entry | Ubuntu → `\EFI\ubuntu\shimx64.efi`; временный `MOK Manager` удалён |
| Kernel lockdown | включён из-за Secure Boot |

## Wi-Fi драйвер

| Параметр | Значение |
|---|---|
| Встроенный | `rtw88_8821ce` — blacklisted (h2c firmware bug) |
| Рабочий | DKMS `rtl8821ce` v5.5.2_34066.20200325 (tomaspinho) |
| Патч | `patches/rtl8821ce-ubsan.patch` (UBSAN false-positive) |
| Модуль | `/lib/modules/7.0.0-38-generic/updates/dkms/8821ce.ko.zst` |
| Bluetooth | `btusb`/`bluetooth`/`btmtk`/`btrtl`/`btbcm`/`btintel` blacklisted |

## Полезные пути

```text
/home/labuser/nettop/docker-compose.yml
/home/labuser/nettop/ctl.sh
/home/labuser/nettop/homeassistant/config/
/home/labuser/nettop/data/music-assistant/
/etc/netplan/00-installer-config.yaml
/etc/netplan/50-wifi.yaml
/etc/modprobe.d/blacklist-rtw88.conf
/etc/modprobe.d/blacklist-bt.conf
/etc/modprobe.d/rtw88.conf
/etc/modules-load.d/8821ce.conf
/etc/systemd/system/systemd-networkd-wait-online.service.d/10-netplan.conf
/usr/src/rtl8821ce-v5.5.2_34066.20200325/
/var/lib/dkms/rtl8821ce/
```

## Что ещё не настроено

- Spotify: 429 снят 2026-10-27; Connect search/play проверены; Liked Songs
  в MA отдельно не перепроверялся.
- Matter: сервер и HA integration готовы; добавить ленту/лампу Яндекса
  через HA Companion (QR/11-значный код, 2.4 ГГц Wi-Fi).
- Xiaomi Home component установлен; войти в Xiaomi China и импортировать
  light bar `MJGJD02YL`; если модели нет — ESPHome/Tasmota (пайка).
- Личность Hermes/Рикардо: `SOUL.md` + `voice` personality + HA prompt;
  проверено «как тебя зовут» → «Я Рикардо…».
- Custom wake word «Рикардо»; голосовой запуск Spotify — после sync.
- Будильник/таймер голосом и phone→speaker/transfer — ручной retest.
- offsite backup HA/MOK/secrets.
