# Station BLE gateway

Локальный канал настройки Wi-Fi для Yandex Station Midi. Без приложения и
облака Яндекса. Работает по Bluetooth Low Energy; Wi-Fi не выключает.

## Что делает

- Станция рекламируется как `Station-Gateway`.
- Телефон подключается и пишет SSID/PSK/token в GATT-характеристики.
- Станция добавляет новую сеть через `wpa_cli`, сохраняет старую и
  откатывается, если новая не поднялась.
- Автоматически включает BLE, если Wi-Fi нет больше 20 секунд.
- Три коротких нажатия кнопки mute открывают/закрывают ручное окно на 15 минут.

## UUID

Service:

```text
a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d
```

| Характеристика | UUID | Доступ |
|---|---|---|
| SSID | `a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c01` | read/write |
| PSK | `a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c02` | write |
| Token | `a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c03` | write |
| Commit | `a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c04` | write |
| Status | `a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c05` | read/notify |

## Клиент: nRF Connect (Android)

1. Открыть nRF Connect → Scan.
2. Найти `Station-Gateway` → Connect.
3. Открыть service `a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d`.
4. В `SSID` записать имя Wi-Fi (UTF-8).
5. В `PSK` записать пароль (UTF-8).
6. В `Token` записать токен (6–12 цифр; сейчас 8).
7. В `Commit` записать `1`.
8. Читать `Status`:
   - `CONNECTING` — пробует;
   - `CONNECTED` — подключилась;
   - `ERROR:...` — ошибка, можно повторить.

Токен (не публиковать):

```bash
adb -s 192.0.2.13:5555 shell cat /data/quasar/ble-gateway/token
```

Если nRF Connect не делает длинную запись пароля, включите в настройках
характеристики `Write` с response / long write. Сервер поддерживает offset.

## Кнопка

- **1 нажатие** — штатный hardware mute: `SW_MUTE_DEVICE` читает Pulsar,
  gateway только логирует нажатие.
- **3 коротких нажатия** — открыть/закрыть окно gateway на 15 минут.
- **4 нажатия** — включить/выключить Bluetooth A2DP-приёмник: adapter
  становится discoverable/pairable на **90 секунд**, телефон играет на
  станцию через `bluealsad`/`bluealsa-aplay`. После окна discoverable/
  pairable выключаются, уже спаренный телефон сохраняет A2DP-связь, а
  Wi-Fi просит `wpa_cli reconnect`.
- **Удержание 5 секунд** — открыть окно gateway на 15 минут (замена
  штатного Yandex setup, который без `maind` недоступен). Аудио/LED
  подтверждения нет: смотреть `Station-Gateway` в nRF Connect или
  `/data/quasar/ble-gateway/status`; BlueZ может показывать alias
  `My Favourite Yandex Station No. 18921`, ориентироваться на service
  UUID `a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d`.
- **Удержание при питании** — factory reset, не делать.

Короткие нажатия группируются 1,5 секунды, поэтому 3-е нажатие из
четырёх не открывает gateway раньше времени. События читаются из
`/dev/input/event2` пассивно, без `EVIOCGRAB`: Pulsar и gateway могут
читать их одновременно. 1/4/5 по смыслу сохранены; 4 и 5 теперь
обслуживает наш gateway, а не `maind`.

2026-10-26: логика 3/4/5 проверена синтетическими `KEY_MUTE` через FIFO
(`4` включала/выключала discoverable, `5s` открывала окно, `3` закрывала).
Живой тест: 1 нажатие реально переключает hardware mute (на HA
`Hardware mic mute` on/off; красная волна — one-shot, постоянный красный
только на логотипе), 3 открывает gateway, 4 включает Bluetooth.

**Важно:** на этом combo-чипе discoverable Bluetooth может уронить Wi-Fi/HA.
В тесте 2026-10-26 станция ушла с Wi-Fi после 4 нажатий; восстановили через
BLE gateway (запись той же сети). Поэтому окно 4 нажатия ограничено 90 с,
после него gateway просит Wi-Fi reconnect. Если Wi-Fi всё равно пропал,
`Station-Gateway` остаётся аварийным каналом. 5 секунд физически ещё не
проверяли.

## Авто-режим

- Если `wlan0` не в `COMPLETED` больше 20 секунд — BLE включается сам.
- После успешного подключения и без ручного окна — выключается.
- После ошибки остаётся включённым для повтора.

## Запуск на станции

```bash
# один раз
adb -s 192.0.2.13:5555 push ble-gateway-armhf /data/quasar/ble-gateway/ble-gateway
adb -s 192.0.2.13:5555 push station/start.sh /data/quasar/ble-gateway/start.sh
adb -s 192.0.2.13:5555 push station/stop.sh /data/quasar/ble-gateway/stop.sh
adb -s 192.0.2.13:5555 push station/status.sh /data/quasar/ble-gateway/status.sh
adb -s 192.0.2.13:5555 shell 'chmod 755 /data/quasar/ble-gateway/*.sh /data/quasar/ble-gateway/ble-gateway; /data/quasar/ble-gateway/start.sh'

# проверка
adb -s 192.0.2.13:5555 shell /data/quasar/ble-gateway/status.sh
```

## HTTP control (для HA)

Gateway поднимает LAN-only HTTP на `0.0.0.0:8099`; авторизация — bearer
токен из `/data/quasar/ble-gateway/control-token` (0600):

```bash
TOKEN=$(cat /data/quasar/ble-gateway/control-token)
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8099/status
curl -X POST -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8099/gateway/on
curl -X POST -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8099/gateway/off
curl -X POST -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8099/bluetooth/on
curl -X POST -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8099/bluetooth/off
```

HA использует `rest_command` + `intent_script`; файлы —
`../../projects/home-assistant/ha-voice/`. Токен не публиковать; порт 8099 не выставлять
в интернет.

## Dry-run

Для проверки GATT без изменения Wi-Fi:

```bash
/data/quasar/ble-gateway/ble-gateway --dry-run --auto=false --no-button --force \
  --log /data/quasar/ble-gateway/test.log
```

Commit в этом режиме вернёт `DRY_RUN_OK`.

## Пересборка

```bash
cd projects/station/ble-gateway
CGO_ENABLED=0 GOOS=linux GOARCH=arm GOARM=7 \
  go build -trimpath -ldflags='-s -w' -o ble-gateway-armhf .
```

Новый бинарник — статический armhf. На станции: остановить gateway,
заменить `/data/quasar/ble-gateway/ble-gateway`, запустить `start.sh`.
Старый бинарник сохраняется как `ble-gateway.prev`.

## Откат

```bash
/data/quasar/ble-gateway/stop.sh
# убрать хуки из /data/quasar/onboot.sh и /data/quasar/rootkeeper.sh
rm -rf /data/quasar/ble-gateway
```

Wi-Fi-профили не удаляются. Rootfs/OTA не меняются.
