# Station BLE gateway

Локальный канал настройки Wi-Fi для Yandex Station Midi. Без приложения и
облака Яндекса. Работает по Bluetooth Low Energy; Wi-Fi не выключает.

## Что делает

- Станция рекламируется как `Station-Gateway`.
- Телефон подключается и пишет SSID/PSK/token в GATT-характеристики.
- Станция добавляет новую сеть через нативный control-интерфейс
  `wpa_supplicant` (unix datagram `/var/run/wpa_supplicant/wlan0`), сохраняет
  старую и откатывается, если новая не поднялась.
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
6. В `Token` записать токен из `/data/quasar/ble-gateway/token`
   (6 цифр; файл `0600`, не публиковать).
7. В `Commit` записать `1`.
8. Читать `Status`:
   - `CONNECTING` — пробует;
   - `CONNECTED` — подключилась;
   - `ERROR:...` — ошибка, можно повторить.

Токен:

```bash
adb -s 192.0.2.13:5555 shell cat /data/quasar/ble-gateway/token
```

Если nRF Connect не делает длинную запись пароля, включите в настройках
характеристики `Write` с response / long write. Сервер поддерживает offset
(BlueZ присылает `uint16`).

## Кнопка

- **1 нажатие** — штатный hardware mute: `SW_MUTE_DEVICE` читает Pulsar,
  gateway только логирует нажатие.
- **3 коротких нажатия** — открыть/закрыть окно gateway на 15 минут.
- **4 нажатия** — включить/выключить Bluetooth A2DP-приёмник: adapter
  становится discoverable/pairable на **90 секунд**, телефон играет на
  станцию через `bluealsad`/`bluealsa-aplay`. После окна discoverable/
  pairable выключаются, `btMode` сбрасывается (следующие 4 нажатия снова
  включают), уже спаренный телефон сохраняет A2DP-связь, а Wi-Fi просит
  `wpa_supplicant reconnect`.
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
читать их одновременно. Gateway реагирует только на `EV_KEY`/`KEY_MUTE`
(значения 1/0); прочие события input-устройства игнорируются.

**Важно:** на этом combo-чипе discoverable Bluetooth может уронить Wi-Fi/HA.
Поэтому окно 4 нажатия ограничено 90 с, после него gateway просит Wi-Fi
reconnect. Если Wi-Fi всё равно пропал, `Station-Gateway` остаётся аварийным
каналом.

## Авто-режим

- Если `wlan0` не в `COMPLETED` больше 20 секунд — BLE включается сам.
- После успешного подключения и без ручного окна — выключается.
- После ошибки остаётся включённым для повтора.
- Пока идёт Wi-Fi apply, auto-тик не запускает второй apply.

## Запуск на станции

```bash
# один раз
adb -s 192.0.2.13:5555 push bazel-bin/projects/station/ble-gateway/crates/ble-gateway/ble-gateway /data/quasar/ble-gateway/ble-gateway
adb -s 192.0.2.13:5555 push station/start.sh /data/quasar/ble-gateway/start.sh
adb -s 192.0.2.13:5555 push station/stop.sh /data/quasar/ble-gateway/stop.sh
adb -s 192.0.2.13:5555 push station/status.sh /data/quasar/ble-gateway/status.sh
adb -s 192.0.2.13:5555 shell 'chmod 755 /data/quasar/ble-gateway/*.sh /data/quasar/ble-gateway/ble-gateway; /data/quasar/ble-gateway/start.sh'

# проверка
adb -s 192.0.2.13:5555 shell /data/quasar/ble-gateway/status.sh
```

`station/start.sh` запускает `ble-gateway --auto=true --log …`; этот синтаксис
сохранён.

## CLI

```
ble-gateway [OPTIONS]

  --auto[=<BOOL>]              авто-режим (по умолчанию true; --auto, --auto=false, --auto=1)
  --log <PATH>                 append-лог вместо stderr (каталог создаётся)
  --log-level <LEVEL>          уровень по умолчанию (RUST_LOG как уровень важнее), default info
  --token-file <PATH>          default /data/quasar/ble-gateway/token
  --button <PATH>              default /dev/input/event2
  --dry-run                    не менять Wi-Fi, Commit отвечает DRY_RUN_OK
  --no-button                  не открывать кнопку
  --force                      включить BLE сразу при старте
  --test-wpa                   добавить/заполнить/удалить временную сеть и выйти
  --control-addr <ADDR>        default 0.0.0.0:8099; пустая строка выключает HTTP
  --control-token-file <PATH>  default /data/quasar/ble-gateway/control-token
  --wpa-ctrl <PATH>            default /var/run/wpa_supplicant/wlan0
  -h, --help                   справка
  -V, --version                версия
```

`--test-wpa` теперь завершается кодом 0, когда временная сеть добавлена,
заполнена и удалена; ошибка `add_network`/`set_network`/`remove_network`
даёт ненулевой код.

## Архитектура

- `cli` — clap derive, единая `Config`.
- `gateway` — состояние и общие сценарии. Статус — `tokio::sync::watch`
  (его читают файл `status`, GATT notify и HTTP). Commit'ы приходят в
  `mpsc` и разбираются одним worker'ом; `tokio::sync::Mutex` дополнительно
  сериализует apply, поэтому два параллельных Commit не запускают два
  `SELECT_NETWORK`. BLE-регистрация и Bluetooth-переключения — короткие
  async-мьютексы; таймстемпы и флаги — атомики/`std::sync::Mutex` на одно
  поле.
- `wifi` — типизированный клиент control-интерфейса `wpa_supplicant`
  (`STATUS`, `LIST_NETWORKS`, `ADD_NETWORK`, `SET_NETWORK` с квотированием,
  `ENABLE_NETWORK`, `SELECT_NETWORK`, `REMOVE_NETWORK`, `SAVE_CONFIG`,
  `RECONNECT`) и последовательность apply/rollback.
- `gatt` — `#[zbus::interface]` для GATT-объектов и `#[zbus::proxy]` для
  `GattManager1`/`LEAdvertisingManager1`; `Connectable` ставится через
  `org.freedesktop.DBus.Properties`. `zbus` выбран
  сознательно: он pure-Rust и не линкует libdbus; `bluer` требует libdbus и
  поэтому не используется.
- `bluetooth` — `Adapter1` proxy (`Powered`/`Pairable`/`Discoverable`),
  отключение устройств через `ObjectManager` + `Device1`; `bluealsad`,
  `bluealsa-aplay` и `bt-agent` остаются внешними процессами (у них нет
  D-Bus-эквивалента запуска ALSA-моста/агента).
- `button` — разбор 32-bit `input_event` без `unsafe` и чистый автомат
  жестов на `tokio::time::Instant` (тесты на `tokio::time::pause`).
- `control` — axum + `home-http` bearer; маршруты/поля те же, что читает
  `projects/home-assistant/ha-voice`.
- Логи — `tracing` через `home-telemetry`: локальное время, stderr или
  append-файл. Формат строк отличается от старого Go/ручного логгера.

## HTTP control (для HA)

Gateway поднимает LAN-only HTTP на `0.0.0.0:8099`; авторизация — bearer
токен из `/data/quasar/ble-gateway/control-token` (0600, создаётся при
первом старте):

```bash
TOKEN=$(cat /data/quasar/ble-gateway/control-token)
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8099/status
curl -X POST -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8099/gateway/on
curl -X POST -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8099/gateway/off
curl -X POST -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8099/bluetooth/on
curl -X POST -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8099/bluetooth/off
```

HA использует `rest_command` + `intent_script`; файлы —
`../../home-assistant/ha-voice/`. Токен не публиковать; порт 8099 не выставлять
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
bazel build -c opt --platforms=//platform/bazel/zig:armhf_linux \
  //projects/station/ble-gateway:ble-gateway
# бинарник: bazel-bin/projects/station/ble-gateway/crates/ble-gateway/ble-gateway

bazel test //packages/rust/... //projects/station/ble-gateway/...
```

`bazel test` запускает unit-тесты, `clippy` (warnings = errors) и `rustfmt`.
Кросс-сборка использует тот же hermetic zig toolchain и Rust-тулчейн, что и
Pulsar (`rules_rust` в корневом `MODULE.bazel`), поэтому отдельный
rustup-тулчейн не нужен.

Новый бинарник — armhf ELF. На станции: остановить gateway,
заменить `/data/quasar/ble-gateway/ble-gateway`, запустить `start.sh`.
Старый бинарник сохраняется как `ble-gateway.prev`.

## `bletest`

`bletest` — маленький D-Bus клиент (clap):

```bash
./bletest --destination :1.4 3 1          # записать Commit
./bletest --destination :1.4 --status 4   # прочитать Status
```

## Откат

```bash
/data/quasar/ble-gateway/stop.sh
# убрать хуки из /data/quasar/onboot.sh и /data/quasar/rootkeeper.sh
rm -rf /data/quasar/ble-gateway
```

Wi-Fi-профили не удаляются. Rootfs/OTA не меняются.
