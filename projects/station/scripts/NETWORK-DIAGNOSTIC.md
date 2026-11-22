# Диагностика потери сети станции

`station-network-diagnostic.sh` (на станции — `/data/quasar/station-network-diagnostic.sh`)
только читает состояние: не меняет Wi-Fi профили, не перезапускает радио,
DHCP, Bluetooth или Pulsar, не ставит startup hook и не делает reboot.

```sh
DIAG_MAX_SAMPLES=2 DIAG_INTERVAL=1 /data/quasar/station-network-diagnostic.sh run
/data/quasar/station-network-diagnostic.sh start   # фоновый сбор
/data/quasar/station-network-diagnostic.sh stop
```

По умолчанию до 120 samples с интервалом 5 с (`DIAG_MAX_SAMPLES`,
`DIAG_INTERVAL`, корень `DIAG_ROOT`). Каждый boot — отдельная папка
`/data/quasar/network-diag/<boot-id>/` (каталог 0700, файлы 0600): uptime,
WPA state, адреса/маршруты/соседи, доступность gateway, сетевые процессы,
хвосты kernel/syslog. Логи раскрывают LAN — в Git не добавлять, хранить в
приватной папке проекта.

## Сбор через reboot

Kernel/syslog живут в `/tmp` и теряются при снятии питания, поэтому для
отказа после reboot сбор нужно запускать и после загрузки — временным hook в
существующем rootkeeper. Порядок: сохранить оригинал rootkeeper, проверить
hook на текущем boot, согласовать присутствие человека у питания; после
теста удалить hook/arming flag и вернуть оригинал со сверкой хэша.

## Известное

- Симптом: станция не отвечает на ARP/IPv4/IPv6/ADB/SSH/ESPHome, хотя роутер
  видит её online; BLE provisioning даёт TIMEOUT; cold power cycle
  восстанавливает. Контролируемый warm reboot отказ не воспроизвёл —
  он не на каждом reboot.
- Wi-Fi/BT станции — Amlogic W1 (`vlsicomm`/`aml_sdio`, `sdio_bt`), общий SDIO;
  гипотеза — состояние SDIO после warm reboot.
- Драйвер не менять, защиту не отключать и Pulsar не откатывать только на
  основании гипотез. Открытая задача — `docs/PENDING.md`.
