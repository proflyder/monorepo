# Pulsar Rust fork

**Станция (установлено):** Rust Pulsar `v0.5.0-1-g908a027`. Это единственный
развёрнутый бинарник; обновление до локальной интеграции v0.5.2 не выполнялось.

**Локальный форк (интегрировано, не установлено):** upstream `v0.5.2` влит в
`feat/ricardo-rust`; HEAD `95ce568`, describe `v0.5.2-2-g95ce568` (не release,
build candidate). Safety ref `ricardo-rust/pre-v0.5.2` = `908a027`.
Legacy C++ исходники, patch и binaries в активном дереве отсутствуют; private
recovery archive — `secrets/local/recovery/`.

- Fork: <https://github.com/OWNER/pulsar>
- База: upstream `v0.5.2` (`4cba44a`), ancestor HEAD; delta 68 files.
- Локальный checkout: `projects/station/upstream/pulsar-rust/`
- Снимок изменений: `projects/station/patches/pulsar-rust/`
  (`snapshot-pulsar.py`, base v0.5.2)
- armhf build candidate: `satellite/build/armhf/pulsar` — stripped ARM EABI5
  hard-float ELF (`v0.5.2-2-g95ce568`). Makefile zig objcopy на macOS всё ещё
  падает; артефакт получен установленным LLVM objcopy (GNU-совместимый,
  PT_LOAD/NEEDED проверены), source/Makefile не менялись. Не развёрнуто
  (артефакт не версионируется).
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

Deployed v0.5.0-1: armhf ELF, `mww_create/feed/destroy`; live wake `0.904`,
HA pipeline/TTS, HAOS reboot, Sendspin/MA reconnect, station reboot.

Локальная интеграция v0.5.2: `cargo fmt --check`, arm64 clippy `-D warnings`,
`cargo test --workspace --exclude pulsar-update` (включая pre-roll tests) —
pass; `pulsar-update` 19 pass / 7 fail (26 total) на arm64 из-за x86_64 asset name, тот же
baseline failure у чистого `4cba44a`. armhf build слинкован и stripped
(Android NDK LLVM objcopy). Runtime/audible приёмка не выполнялась.

## Осталось

- Установка/приёмка v0.5.2 на станции (не выполнялась): pre-roll 512 ms,
  второй dmix client, volume grid, stop/snooze, кнопки/mute/clock.
- Живой «Рикардо, стоп» во время будильника, snooze/resume.
- Spotify phone→speaker transfer.

## Recovery

Штатной команды возврата на legacy C++ в активном дереве нет. При
необходимости использовать private recovery archive и отдельный
согласованный план; текущие HA/MA/ESPHome настройки и `state.conf` при этом
не сбрасывать.
