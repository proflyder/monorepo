# Configs — applied protection and recovery sources

Активные источники для станции. Это снимки/референсы, не живой сервис.

| Что | Где |
|---|---|
| OTA/ADB/egress protection, применённое на станции | `station-applied/` |
| Root/ADB helper | `station-applied/activate_adb.sh` |
| OTA no-op + rootkeeper | `station-applied/device_ota_update.sh`, `rootkeeper.sh` |
| Яндекс/telemetry block | `station-applied/yandex-block.sh`, `hosts`, `unbound.conf` |
| Applied boot hooks | `station-applied/onboot.sh`, `launch.json`, `ota_script.pub` |
| SSH init (копия с устройства) | `station-applied/S99dropbear` |
| Station dump/find/ADB diagnostics | `dump-station.sh`, `find-station.sh`, `inspect-adb-activation.sh` |
| Профили wake word (откат/база) | `wakeword-{baseline,personal,v22,v24}.json` |

Приватные env (не в Git): `*.env` — символические ссылки на
`local-data/local/projects/station/configs/`; не публиковать и не переносить в
исходники.

Восстановление — `docs/REBUILD.md`; OTA/ADB — `docs/OTA-FORTRESS.md`.
