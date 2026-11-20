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
| Профили wake word: `personal` — установленная v28, `v27`/`v24`/`v22`/`baseline` — откаты | `wakeword-{baseline,personal,v22,v24,v27}.json` |

Приватные env (не в Git): `*.env` — символические ссылки на
`local-data/local/projects/station/configs/`; не публиковать и не переносить в
исходники.

Восстановление — `docs/REBUILD.md`; OTA/ADB — `docs/OTA-FORTRESS.md`.

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `dump-station.sh` — Dumps the Yandex Station Mini firmware and filesystem for offline study.
- `find-station.sh` — Resolve the station's current LAN address via mDNS (the hostname embeds the device id and survives DHCP changes and factory resets).
- `station-applied/activate_adb.sh` — FORTRESS-WRAPPER v2: 'deactivate' -> no-op (maind не может убить adbd), 'activate' -> всегда с --network (иначе оригинал снял бы network-маркер).
- `station-applied/rootkeeper.sh` — FORTRESS-KEEPER v5: markers + network adbd; start adbd explicitly because yandex-debugctl says "already configured" and does not spawn it again.
- `station-applied/yandex-block.sh` — FORTRESS-YANDEX-BLOCK v1 Blocks real Yandex at the packet level and pins DNS to the local Mac. Lives in /data (rw) and is re-applied by rootkeeper every 60s.
