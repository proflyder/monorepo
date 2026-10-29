# Pulsar Rust fork

**Станция (установлено):** Rust Pulsar `v0.5.2-3-gc57d521-dirty` —
сборка из рабочего дерева на HEAD `c57d521` с повторным wake (30.10);
StationCancel без речи — HA-конфиг, не бинарник. `pulsar.prev` —
`v0.5.2-3-gc57d521`; `v0.5.2-2-g95ce568` и `v0.5.0-1-g908a027` — только
private backup вне Git.

**Локальный форк:** upstream `v0.5.2` влит в `feat/ricardo-rust`; HEAD
`c57d521`, describe `v0.5.2-3-gc57d521` (не release). Safety ref
`ricardo-rust/pre-v0.5.2` = `908a027`.
Legacy C++ исходники, patch и binaries в активном дереве отсутствуют; private
recovery archive — `secrets/local/recovery/`.

- Fork: <https://github.com/OWNER/pulsar>
- База: upstream `v0.5.2` (`4cba44a`), ancestor HEAD; delta 74 files.
- Локальный checkout: `projects/station/upstream/pulsar-rust/`
- Снимок изменений: `projects/station/patches/pulsar-rust/`
  (`snapshot-pulsar.py`, base v0.5.2)
- armhf build: `satellite/build/armhf/pulsar` — stripped ARM EABI5
  hard-float ELF (`v0.5.2-3-gc57d521`), установлен на станции. Makefile zig
  objcopy на macOS всё ещё падает; артефакт получен установленным LLVM objcopy
  (GNU-совместимый, PT_LOAD/NEEDED проверены), source/Makefile не менялись.
  Артефакт не версионируется.
- Модель: `/data/pulsar/wakewords/ru_ricardo_mww.tflite`; SHA-256
  совпадает с `artifacts/wakeword/ricardo-v1/model-sha256.txt`.

## Профиль

- on-device wake word «Рикардо» через C++ bridge, microfrontend и
  `/system/vendor/quasar/libs/libtflite.so`.
- Pre-roll: upstream `PrerollRing`/`wake_pending` race fix; для Ricardo
  `Model::wake_preroll` = Fixed(16) chunks = 512 ms (deployed baseline ~0.5 с).
  HA switch «Command right after wake word» управляет `wake_preroll`.
- `stt-no-text-recognized` — тихая отмена.
- Локальный command spotter только во время playback; «Рикардо, стоп»
  уходит в HA для остановки будильника и восстановления музыки.
- `wake_word=ricardo`, `auto_update=0`, `ENABLE_ZIGBEE=false`.
- ESPHome, Sendspin, Spotify/MA, кнопки, mute, часы сохранены.
- Громкость (кнопки/голос) теперь шагает по сетке Yandex; ANNOUNCE buffering
  переписан. `pulsar-alsa::Pcm` сохраняет armhf ABI shim и добавляет второго
  клиента dmix — требует runtime-приёмки.
- `state.conf` совместим с C++; текущие значения (`nearest_answers=1`,
  `volume2=0.13`, `zigbee_role=off`) прочитаны без сброса.

## Проверено

Deployed baseline v0.5.0-1: armhf ELF, `mww_create/feed/destroy`; live wake
`0.904`, HA pipeline/TTS, HAOS reboot, Sendspin/MA reconnect, station reboot.

Установленная v0.5.2-3-gc57d521: `cargo fmt --check`, arm64 clippy `-D warnings`,
`cargo test --workspace --exclude pulsar-update` (включая pre-roll tests) —
pass; `pulsar-update` 19 pass / 7 fail (26 total) на arm64 из-за x86_64 asset name, тот же
baseline failure у чистого `4cba44a`. armhf build слинкован и stripped
(Android NDK LLVM objcopy). Live: single daemon, HA connected / voice
subscribed, Noise, Ricardo model (cutoff 0.90, window 10), volume 0.3
unmuted. Supervised `reload` объявляет goodbye-причину `restart`, и MA
переподключается сам за ~3 с — два подряд рестарта без рестарта MA, player
остаётся зарегистрированным.

Пользовательскую приёмку проходила предыдущая `v0.5.2-2-g95ce568`: «Рикардо»
просыпается и отвечает на команду сразу после имени («который час») без потери
начального слога; alarm/snooze/music/mute/кнопки в ней не проверялись.
Установленная `v0.5.2-3-gc57d521` такой пользовательской приёмки ещё не
проходила.

Сборка 30.10 с повторным wake (`-dirty` от HEAD `c57d521`); StationCancel без
речи — отдельное изменение HA-конфига. Проверки: `cargo fmt --check`, Linux
`cargo test -p pulsar` (31 pass), clippy
`-D warnings`, `test_ha_config.py` (50 pass) — pass в worker-сессии; armhf
ELF32 EABI5 собран и stripped LLVM objcopy из NDK, PT_LOAD/NEEDED совпадают с
предыдущей установленной сборкой. Live: single daemon, HA connected/voice
subscribed, Noise, Ricardo v22 (cutoff 0.90, window 10), Sendspin→MA reconnect
за ~1.6 с, `state.conf` и модель не менялись. HA: `StationCancel` без речи
применён reload-ом `intent_script`; `ha core check` ok, «отмена» через
`conversation.process` → `action_done` без речи. Живая голосовая приёмка
повторного wake и отмены ещё нужна. Backup и rollback — в `docs/STATE.md`.

## Осталось

- Runtime-приёмка v0.5.2: второй dmix client/ANNOUNCE, volume grid,
  stop/snooze, кнопки/mute/clock.
- Живой «Рикардо, стоп» во время будильника, snooze/resume;
  будильник/музыка/mute/кнопки не входили в подтверждённую приёмку.
- Spotify phone→speaker transfer.

## Recovery

Штатной команды возврата на legacy C++ в активном дереве нет. При
необходимости использовать private recovery archive и отдельный
согласованный план; текущие HA/MA/ESPHome настройки и `state.conf` при этом
не сбрасывать.
