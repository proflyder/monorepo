# TROUBLESHOOTING — неттоп

Текущая сеть: PVE Wi-Fi 4addr/WDS, Ethernet fallback. Факты — `../../docs/STATE.md`.

## Wi-Fi на PVE

```bash
systemctl status ricardo-uplink wpa_supplicant@wlp4s0
journalctl -u ricardo-uplink -n 50
cat /sys/class/net/nic0/carrier
ip -br addr
bridge link
```

- Должен быть ровно один physical uplink в `vmbr0`: `nic0` или `wlp4s0`.
- Оба одновременно не подключать — петля.
- `wlp4s0` в 4addr; без поддержки AP/драйвера VM потеряют L2.
- После reboot проверить VM `.27/.28`, HA/MA/Grafana.

## `rtw88_8821ce: failed to send h2c command`

Обычно firmware/regulatory или power-save. Проверить:

```bash
dmesg | grep -Ei 'rtw|8821|firmware'
iw dev wlp4s0 link
iw dev wlp4s0 set power_save off
```

Не переустанавливать PVE; сначала проверить профиль/AP/канал.

## Secure Boot / DKMS

```bash
mokutil --sb-state
dkms status
```

Wi-Fi-модуль должен быть подписан MOK. Не удалять
`/var/lib/shim-signed/mok/`. После обновления ядра:

```bash
sudo dkms autoinstall
sudo update-initramfs -u
```

## UBSAN / Wi-Fi

Старый `rtl8821ce` мог давать `array-index-out-of-bounds`. Текущий штатный
`rtw88_8821ce`; патч — `patches/rtl8821ce-ubsan.patch` (legacy).
Не менять драйвер без canary на отдельном MAC.

## HA/MA/Grafana не открываются

```bash
curl -fsS -o /dev/null -w 'HA %{http_code}\n' http://192.0.2.28:8123/
curl -fsS -o /dev/null -w 'MA %{http_code}\n' http://192.0.2.28:8095/
curl -fsS -o /dev/null -w 'Grafana %{http_code}\n' \
  http://192.0.2.27:30300/grafana/api/health
```

- HAOS VM running? `qm status 100` на PVE.
- K3s VM running? `qm status 101`; `sudo k3s kubectl get pods -A`.
- Tailscale/operator: `tailscale status` в K3s VM; проверить ingress.
- Не путать `.27:8123` (закрыт) с `.28:8123`.

## Voice / Whisper / Piper

```bash
ssh labuser@192.0.2.27
sudo k3s kubectl -n home-assistant get pods
curl -fsS http://127.0.0.1:10305/  # Wyoming
curl -fsS http://127.0.0.1:10200/
curl -fsS http://127.0.0.1:8643/v1/models
```

Проверить HA pipeline, `stt.onnx_asr`, `tts.piper`, voice
`ru_puriso2-medium`. Hermes `8642` — loopback; `8643` — только HAOS/K3s.

## Boot degraded / нет Ethernet

`systemd-networkd-wait-online` не должен блокировать boot при Wi-Fi.
Если boot долгий — проверить `ricardo-uplink`, `wpa_supplicant@wlp4s0`,
`nic0` carrier. Ethernet не обязателен, но остаётся fallback.

## Rollback

- PVE network: вернуть предыдущий `/etc/network/interfaces` из
  `/root/wifi-cutover/*`.
- Wi-Fi uplink: остановить `ricardo-uplink`, вернуть Ethernet в `vmbr0`.
- HA/K3s: `../../docs/REBUILD.md`; не запускать второй writer.

## Не делать

- Не подключать Ethernet и Wi-Fi одновременно в мост.
- Не менять AP/драйвер без проверки отдельным MAC/DHCP/IPv6.
- Не переустанавливать PVE при потере Wi-Fi — сначала консоль/профиль.
- Не публиковать SSID/PSK/ключи.
