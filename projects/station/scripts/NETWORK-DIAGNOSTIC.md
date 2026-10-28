# Диагностика потери сети станции

`station-network-diagnostic.sh` только читает состояние. Он не меняет
Wi-Fi профили, не перезапускает радио, DHCP, Bluetooth или Pulsar.

Разовая проверка на станции:

```sh
DIAG_MAX_SAMPLES=2 DIAG_INTERVAL=1 /data/quasar/station-network-diagnostic.sh run
```

Для ограниченного фонового сбора — `start`, остановка — `stop`.
По умолчанию до 120 samples с интервалом 5 с. Каждый boot имеет отдельную
папку в `/data/quasar/network-diag/<boot-id>/`, права каталога 0700 и
файлов 0600. Сохраняются uptime (часы могут скачком установиться через NTP),
WPA state, адреса/маршруты/соседи, доступность gateway, сетевые процессы и
ограниченные хвосты kernel/syslog. Команда wpa_cli ограничена по времени.
Логи содержат сведения о LAN; не добавлять их в Git и не публиковать.

Для теста reboot сбор должен запускаться и после загрузки, например через
временный hook в существующем rootkeeper. Перед этим сохранить оригинал,
проверить hook на текущем boot и согласовать наличие человека рядом с
питанием. Сам скрипт **не устанавливает** startup hook и не делает reboot.
После теста удалить hook/arming flag, забрать логи в приватную папку проекта.

## Что установлено 2026-10-28

- При отказе HA/Hermes были доступны; сама станция не отвечала на ARP,
  IPv4/IPv6, ADB/SSH/ESPHome с Mac и K3s VM. Другие LAN-клиенты отвечали.
- Роутер показывал MAC/IP станции online. BLE после ручного окна работал.
- Provisioning через BLE дал TIMEOUT; старый профиль восстановлен.
- Cold power cycle восстановил Rust Pulsar, сеть, HA, голос и Sendspin.
- На станции Amlogic W1: vlsicomm/aml_sdio и sdio_bt, не адаптер RTL8821CE
  неттопа. Два основных сохранённых WPA профиля эквивалентны и соответствуют
  рабочим credentials; постоянного force-DPP флага нет.
- До удаления работал старый dpp-provision.sh: при отсутствии default route каждые
  3 с включает DPP listen, не проверяя обычное подключение WPA/DHCP или
  параллельное BLE provisioning. Это кандидат на гонку, а не доказанная
  первопричина наблюдавшегося отказа.
- Старые kernel/syslog были в /tmp и утрачены при снятии питания. Поэтому
  программный reboot с persistent диагностикой нужен для подтверждения.
  Также остаётся гипотеза состояния общего Wi-Fi/BT SDIO после warm reboot.

Не менять драйвер, не отключать защиту и не откатывать Rust на C++ только
на основании этих гипотез.


## Контролируемый warm reboot

Тест согласован и выполнен 2026-10-28 с временным startup hook в rootkeeper.
Новый boot-id подтверждён; ADB вернулся примерно за 18 секунд, WPA COMPLETED,
IP прежний, HA voice subscription и Sendspin восстановились. В сохранённом
фрагменте нового boot: 52 samples / 52 успешных gateway ping, отказов нет.
Следовательно, зависание **не воспроизводится каждым reboot**.

Логи обоих boot скопированы в private
`secrets/proxmox-install/station-network-diag-warm-test.tar.gz`.
Временный startup hook/arming flag удалены; оригинал rootkeeper восстановлен
с проверкой хэша. Драйвер, DPP policy и Wi-Fi profiles не менялись.

## QR/DPP выведен из эксплуатации

По решению пользователя QR/DPP provisioning удалён 2026-10-28. Предложенный
DPP fault test отменён. Удалены загрузочный запуск и respawn из rootkeeper,
скрипты, URI/bootstrap keys и runtime bootstrap state; обработка DPP = 0.
Сохранённые WPA network blocks оставлены без изменений. BLE gateway —
действующий способ настройки. Происхождение прежнего LAN-отказа этим не
доказано. Recovery archive скопирован на Mac, ключи не публикуются.
