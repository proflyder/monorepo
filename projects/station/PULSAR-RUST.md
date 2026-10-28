# Pulsar Rust fork

**Активная станция:** Rust Pulsar `v0.5.0-1-g908a027`, установлен
2026-10-28. Device-откат — `pulsar.prev` на станции; legacy исходники,
patch и binaries заархивированы private в `secrets/local/recovery/`.

- Fork: <https://github.com/OWNER/pulsar>
- Ветка: `feat/ricardo-rust`, commit `908a027`
- База: upstream `v0.5.0` (`1934b68`)
- Локальный checkout: `projects/station/upstream/pulsar-rust/`
- Снимок изменений: `projects/station/patches/pulsar-rust/`
  (`snapshot-pulsar.py`)
- Бинарник armhf: `satellite/build/armhf/pulsar`; полный SHA-256 — в
  private HIL log.
- Модель: `/data/pulsar/wakewords/ru_ricardo_mww.tflite`; SHA-256
  совпадает с `artifacts/wakeword/ricardo-v1/model-sha256.txt`.

## Профиль

- on-device wake word «Рикардо» через C++ bridge, microfrontend и
  `/system/vendor/quasar/libs/libtflite.so`.
- Pre-roll 0.5 с; `stt-no-text-recognized` — тихая отмена.
- Локальный command spotter только во время playback; «Рикардо, стоп»
  уходит в HA для остановки будильника и восстановления музыки.
- `wake_word=ricardo`, `auto_update=0`, `ENABLE_ZIGBEE=false`.
- ESPHome, Sendspin, Spotify/MA, кнопки, mute, часы, громкость сохранены.
- `state.conf` совместим с C++; текущие значения (`nearest_answers=1`,
  `volume2=0.13`, `zigbee_role=off`) прочитаны без сброса.

## Проверено

- armhf ELF, `mww_create/feed/destroy` присутствуют; strip 3.8 MB.
- Реальная станция: `libtflite` и модель загружены; HA ESPHome подключён.
- Live wake `Рикардо` probability `0.904`; HA pipeline и TTS прошли.
- Sendspin/MA: после HAOS reboot MA переподключилась, player вернулся.
- Station reboot: Rust autostart, VLAN/HA/MA/Sendspin восстановились.
- `pulsar.prev --version` — legacy build; rollback доступен.
- Backup: `secrets/local/backups/pulsar-rust-migration-20261028/`.

## Осталось

- Живой «Рикардо, стоп» во время будильника, snooze/resume.
- Кнопки/mute/clock/volume физически после reboot.
- Spotify phone→speaker transfer.
- Полный откат на legacy Pulsar с проверкой HA/MA (бинарник и backup готовы).

## Rollback

```bash
adb -s 192.168.3.13:5555 shell \
  'cp /data/pulsar/pulsar.prev /data/pulsar/pulsar && \
   chmod 755 /data/pulsar/pulsar && /data/pulsar/ctl.sh restart'
```

Legacy backup на станции — в `/data/pulsar/backups/`; исходники, patch и
binaries — private в `secrets/local/recovery/`.
