# PROVISIONING — network recovery

Дизайн и варианты. Что уже внедрено/сломано — `STATE.md`; задачи — `PENDING.md`.

## Требование

После переезда/смены Wi-Fi всегда иметь доступ к станции без приложения Яндекса.
Guard должен разрешать private LAN; публичный WAN/Яндекс остаётся закрыт.

## Ограничения текущей платформы

- `wpa_supplicant`/`wpa_cli` есть; профили — в
  `/bkupgrade/data/wpa_supplicant.conf`.
- Чип поддерживает AP, но `hostapd`/DHCP-сервера в системе нет.
- UDC/configfs есть; функции `rndis`/`ncm`/`ffs.adb` создаются, `ecm` нет;
  data-порт USB-C не проверен.
- BLE 5.0 peripheral; BlueZ 5.83; `GattManager1`/`LEAdvertisingManager1`
  есть. В `maind` есть `BleSetupCapability`/`BluezBLEImpl`, но app колонку
  не видит.
- **DPP на `wlan0` при активном Wi-Fi не работает**: драйвер даёт
  `vm_cfg80211_remain_on_channel ... vm_p2p_support==0`.
- `p2p0` off-channel работает, но роняет `wlan0`; при рабочей сети не
  использовать.

## Каталог способов

| # | Способ | Суть | Плюсы | Минусы |
|---|---|---|---|---|
| 0 | **Штатный BLE setup** | mute 5 с + приложение «Дом с Алисой» | уже в прошивке | **app не видит колонку** — не используется |
| 1 | **DPP QR** | bootstrap + телефон-конфигуратор | штатно, без app/облака | при активном Wi-Fi драйвер блокирует; iPhone не гарантирован |
| 2 | **Sound pairing** | FSK-аудио SSID/PSK, микрофон | любой телефон/ноутбук, офлайн | нужен режим сопряжения; токен только Wi-Fi |
| 3 | **Свой BLE GATT** | GATT service + nRF Connect | **реализован и проверен**; без облака | token; клиент nRF/Web Bluetooth |
| 4 | **Fallback SSID** | профиль + phone hotspot | просто, без прошивки | travel router исключён; нужен хотспот |
| 5 | **USB NCM/RNDIS** | gadget, статический IP, ADB/SSH | без Wi-Fi/app | **исключён**: data не подтверждён, 5 В не хватает |
| 6 | **AP + captive portal** | hostapd + DHCP + web | без app/travel router | нет hostapd/DHCP; сборка; риск |
| 7 | **WPS/P2P GO** | WPS/P2P | минимум софта | WPS небезопасен; не обычный AP |
| 8 | **UART** | 3.3V TTL root shell | максимально надёжно | нужен доступ к плате |
| 9 | **Factory reset** | стоковый setup | просто | сносит root/ADB/SSH/guard/OTA; нет образа |
| 10 | **Aux** | Ethernet/4G/NFC/Thread/overlay | — | на Midi нет готового железа |

## 0. Штатный BLE setup — не используется

Формально `maind` умеет setup по mute 5 с и `BleSetupCapability`, но
приложение «Дом с Алисой» колонку не видит. Штатный путь вычеркнут.

## 1. DPP / Wi-Fi Easy Connect

`wpa_supplicant` Enrollee; постоянный DER-ключ; QR:

```text
dpp_bootstrap_gen type=qrcode mac=<MAC> chan=81/1 key=<DER-hex>
dpp_listen 2412
```

Телефон-конфигуратор передаёт SSID/PSK; `dpp_config_processing=2` сохраняет
сеть. Приложение/облако не нужны.

Ограничение: на `wlan0` при активной ассоциации драйвер не даёт off-channel.
Проверять только **unassociated**, после BLE/AP. `p2p0` не использовать
при рабочей сети. Android 10+ обычно; iPhone не гарантирован.

## 2. Sound pairing

FSK 16 кГц, 40 мс/символ, `1024 + 256*nibble`, payload SSID/PSK/token + CRC,
RS(15,11), WAV/OGG. Играть рядом с колонкой в режиме сопряжения.
`projects/station/resources/quasar-connect-bot` — MIT; `pairing.py` офлайновый; 15/15 тестов.
Случайный токен только Wi-Fi; аккаунт не привяжет. Пароль звучит в эфире.

## 3. Свой BLE GATT — реализован

Детали, кнопки, HTTP control и пересборка — `../projects/station/ble-gateway/README.md`.

`/data/quasar/ble-gateway/ble-gateway` (Go, armhf, статический), BlueZ 5.83,
`GattManager1`/`LEAdvertisingManager1`. Сервис `Station-Gateway`;
SSID/PSK/token/commit/status. Клиент — nRF Connect. event2 читается
пассивно: 1 — hardware mute (Pulsar), 3 — gateway, 4 — Bluetooth, 5 с —
gateway setup. Авто при отсутствии Wi-Fi >20 с. `maind` при Pulsar уже
остановлен, но gateway работает независимо. Старая сеть сохраняется и
включается как fallback; `save_config` только после успеха.

## 4. Fallback SSID

Профиль `Station-Recovery` (WPA2, priority ниже дома) + phone hotspot с тем же
SSID; DHCP reservation по MAC → фиксированный IP. **Travel router исключён**;
это опциональный путь, не основной. Профиль можно восстанавливать скриптом из
`/data`, если конфиг потерян.

## 5. USB NCM/RNDIS (исключён)

configfs gadget:

- `ncm.usb0` для macOS/Linux, `rndis.usb0` для Windows;
- станция `usb0` = `192.168.42.1/24`, ноутбук `.2`;
- `adbd :5555`/dropbear уже слушают;
- `ffs.adb` доступен для ADB over USB.

**Питание:** Midi ждёт **15 В / 1.5 А (22.5 Вт)**; Mac USB-C даёт 5 В, и
станция сообщает «не знакома с этим блоком питания». Для USB-теста нужен
одновременно 15-В PD и data-хост. UDC `fdd00000.crgudc2` есть, но сейчас
`not attached`, Mac USB-устройство не видит; data-режим порта не подтверждён.
Если порт power-only, USB NCM невозможен; основной путь — штатный BLE.
USB не считать резервом.

## 6. AP + captive portal

`hostapd` + DHCP + мини-веб: `Station-Recovery`, `192.168.4.1`, ввод
SSID/PSK → переключение в STA. Нужны static-бинарники и тест AP+STA.

## 7. WPS / P2P

WPS PBC/PIN небезопасен; P2P GO не обычный AP. Только как эксперимент.

## 8. UART

3.3V TTL, root shell, `wpa_cli`. Нужно открыть корпус/найти pads. Последний
аппаратный резерв.

## 9. Factory / recovery

Сброс восстанавливает стоковый setup, но сносит `/data`, root/ADB/SSH, guard и
OTA/telemetry-защиту. Проверенного Midi-образа нет. Не считать unbrick.

## Рекомендуемая архитектура

1. private-LAN guard (обязательно);
2. **свой BLE gateway** (3 нажатия/5 с + авто) — основной, уже работает;
3. sound pairing — офлайн-запас;
4. AP + captive portal — без travel router, если BLE не выйдет;
5. DPP только unassociated; USB/travel router исключены;
6. UART/factory — последний.

## Тест

1. Guard private LAN; ADB/SSH/Яндекс — проверено.
2. Свой BLE: 3 нажатия → nRF → SSID/PSK/token/commit → CONNECTED — проверено.
3. Reboot — проверено; текущий id=5, старая id=0 enabled.
4. Failure-rollback — проверено: фейковый SSID → `ERROR:TIMEOUT`, старый id сохраняется.
5. Авто-режим — проверено: disconnect → BLE ON через 20 с → reconnect → OFF.
   Sound/AP — только если BLE мало.
