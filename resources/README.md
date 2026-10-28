# Resources — внешние проекты/артефакты

Снимки сторонних проектов и артефактов. **Ничего из этого не запускать на
рабочей станции без отдельной задачи/бэкапа.** Совместимость и текущее состояние — [`../docs/STATE.md`](../docs/STATE.md);
recovery — [`../docs/REBUILD.md`](../docs/REBUILD.md).

| Путь | Что | Статус |
|---|---|---|
| `yandex-station-repo/` | `apkg` + aarch64 packages (340 МБ) | reference/кандидат |
| `yaff/` | YAM/`.yap`/Midi support | reference |
| `spotiya/` | Spotify Connect Mini 2/armv7 | несовместимо с Midi |
| `ya-station-root-shell/` | eMudhra MITM root (Mini 2/Lite) | reference |
| `airplay2-station/` | AirPlay 2 (тест Mini 3) | reference |
| `airplay-fix/` | AirPlay patch Mini 3 | несовместимо |
| `spotify-autostart/spotify.sh` | обрывок librespot | битый, не запускать |
| `yse/` | пакетный менеджер Midi | ждёт исходник |
| `telemetry-yandex-station-privacy.sh` | vibe-скрипт телеметрии | reference, не запускать |

Pulsar и sound pairing перенесены в `projects/station/`:
`upstream/pulsar-rust/` и `resources/quasar-connect-bot/`.

## v3rt3p / Quasar backend

Не здесь: source/compose — `archive/legacy/v3rt3p-stack/` +
`repos/` (`UPSTREAM.md`). Это recovery-only, не активный runtime.

## Правило

Ничего не ставить на карантинную станцию без согласования. Любой тест — на
запасной колонке, с бэкапом. Mini 2 rollback/guide — `archive/legacy/mini2/`,
**не для Midi**.
