# PROVISIONING — восстановление Wi-Fi станции

Текущий путь — BLE gateway. Факты — `STATE.md`; команды — `OPERATIONS.md`.

## Требование

После смены/потери Wi-Fi нужен доступ к станции без приложения Яндекса.
Guard разрешает private LAN; публичный WAN/Яндекс закрыт.

## BLE gateway

- Сервис `Station-Gateway`, GATT service
  `a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d`.
- SSID `…4c01`, PSK `…4c02`, token `…4c03`, commit `…4c04`,
  status `…4c05`.
- Клиент — nRF Connect: писать UTF-8, **SEND**, затем читать `Status`;
  commit = `1`. При ошибке возвращается прежняя сеть.
- `/dev/input/event2` пассивно:
  - 1 — hardware mute;
  - 3 — окно gateway 15 минут;
  - 4 — Bluetooth A2DP 90 с, затем reconnect Wi-Fi;
  - 5 секунд — окно gateway вместо Yandex setup.
- Авто: нет Wi-Fi >20 с → gateway включается.
- Токен — `/data/quasar/ble-gateway/token`; HTTP control —
  `/data/quasar/ble-gateway/control-token`; не публиковать.

## Fallback

- Старый профиль Wi-Fi сохраняется и включается при ошибке.
- При потере Wi-Fi после Bluetooth — BLE gateway остаётся аварийным каналом.
- QR/DPP провижининг удалён 2026-10-28: нет автозапуска, bootstrap ключей
  или DPP-listen. Настройка Wi-Fi выполняется через BLE gateway.

## Проверка

1. 3 нажатия → nRF Connect → SSID/PSK/token/commit → `CONNECTED`.
2. Перезагрузка → профиль сохраняется.
3. Неверный SSID → `ERROR:TIMEOUT`, старый профиль не теряется.
4. Нет Wi-Fi → авто-BLE через 20 с; reconnect → BLE off.
5. Guard: LAN/ADB/SSH живы, Яндекс/WAN закрыты.

Factory reset сносит `/data`, root/SSH/OTA/guard; проверенного Midi-образа
нет. Не считать factory reset unbrick.
