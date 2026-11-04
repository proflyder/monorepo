# Pulsar Rust fork

**Станция (установлено):** Rust Pulsar `v0.5.1005` —
armhf-сборка (SHA256 `89415c5a…71671b6`) с повторным wake, экспериментальным
локальным endpointing, Ricardo pre-roll 256 ms и записью wake-событий; база
upstream `v0.5.4` (`f1f5b17`), HEAD `427a8e3`. Процесс запущен с
`PULSAR_LOCAL_ENDPOINTING=1`. StationCancel без речи — HA-конфиг, не
бинарник. `pulsar.prev` — `v0.5.1004` (`fb0695b2…`); historical:
`3a544979…` (`v0.5.1002`), `80aea2ec…` (`v0.5.1001`, база `v0.5.2`),
`7081ef8e…` (`v0.5.2-4-g9b5e86a-dirty`),
`0b0c48e4…` (endpointing 512 ms), `pulsar.pre-endpoint-20261030` —
`v0.5.2-3-gc57d521-dirty`; `v0.5.2-2-g95ce568` и `v0.5.0-1-g908a027` —
только private backup вне Git.

**Код в монорепозитории:** исходники Pulsar — `projects/station/pulsar/`
(first-party, AGPL-3.0); база upstream `v0.5.4` (`f1f5b17`) сохранена в
history. Upstream `Morvex885/pulsar` вливается вручную патчем
(`git diff vA..vB` → `git apply -3 --directory=projects/station/pulsar`), затем
обычный squash PR. Legacy C++ исходники, patch и binaries в активном дереве
отсутствуют; private recovery archive — `secrets/local/recovery/`.

- Исходники: `projects/station/pulsar/` (вложенного checkout'а и snapshot нет).
- База: upstream `v0.5.4` (`f1f5b17`), ancestor HEAD.
- Сборка: `bazel build -c opt --platforms=//platform/bazel/zig:<arch>_linux
  //projects/station/pulsar/satellite:station_binary`; CI-юниты `pulsar-bazel`
  (хост-тесты) и `pulsar` (кросс-сборка/релиз).
- Релиз: изменение `projects/station/pulsar/VERSION` → общий `deploy-home`
  (`publish_when` по VERSION) → `publish-native` публикует OCI
  `ghcr.io/proflyder/pulsar-firmware:<version>` (+ `:latest`), затем
  `deploy-station` после approve в Environment `Station`. GitHub Releases и
  теги запрещены; подробности — `RICARDO.md`, «Релизы и деплой».
- armhf build: `satellite/build/armhf/pulsar` — stripped ARM EABI5
  hard-float ELF (`v0.5.1005`), установлен на станции. Makefile
  zig objcopy на macOS всё ещё падает; артефакт получен установленным LLVM
  objcopy (GNU-совместимый, PT_LOAD/NEEDED проверены), source/Makefile не
  менялись. Артефакт не версионируется.
- Модель: базовый профиль `/data/pulsar/wakewords/ru_ricardo_mww.tflite`
  (SHA-256 совпадает с `artifacts/wakeword/ricardo-v1/model-sha256.txt`);
  установленный персональный кандидат — v27, best-known — v24 (см.
  `docs/STATE.md`).

## Профиль

- on-device wake word «Рикардо» через C++ bridge, microfrontend и
  `/system/vendor/quasar/libs/libtflite.so`.
- Pre-roll: upstream `PrerollRing`/`wake_pending` race fix; для Ricardo
  `Model::wake_preroll` = Fixed(8) chunks = 256 ms
  (`v0.5.2-4-g9b5e86a-dirty`, SHA256 `7081ef8e…`; три реальные команды точно,
  n=3 — эмпирика, не гарантия). HA switch «Command right after wake word»
  управляет `wake_preroll`.
- `stt-no-text-recognized` — тихая отмена.
- Отклонение форка от upstream `v0.5.4` (STT stop-word fallback): в диалоге,
  открытом «Рикардо», распознанный текст всегда уходит в HA — локальной
  отмены/паузы нет, поэтому «Рикардо, стоп» доходит до HA. Для диалогов по
  кнопке, follow-up и `start_conversation` отдельное «стоп»/«хватит» отменяет
  диалог локально и останавливает звонящий таймер.
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

Проверки сборки `v0.5.2-3-gc57d521` (база текущей установленной `-4`):
`cargo fmt --check`, arm64 clippy `-D warnings`,
`cargo test --workspace --exclude pulsar-update` (включая pre-roll tests) —
pass; `pulsar-update` 19 pass / 7 fail (26 total) на arm64 из-за x86_64
asset name, тот же baseline failure у чистого `4cba44a`. armhf build
слинкован и stripped (Android NDK LLVM objcopy). Live: single daemon, HA
connected / voice subscribed, Noise, Ricardo model (cutoff 0.90, window 10),
volume 0.3 unmuted. Supervised `reload` объявляет goodbye-причину `restart`,
и MA переподключается сам за ~3 с — два подряд рестарта без рестарта MA,
player остаётся зарегистрированным.

Пользовательскую приёмку проходила предыдущая `v0.5.2-2-g95ce568`: «Рикардо»
просыпается и отвечает на команду сразу после имени («который час») без потери
начального слога; alarm/snooze/music/mute/кнопки в ней не проверялись.
Установленная `v0.5.2-3-gc57d521` такой пользовательской приёмки ещё не
проходила; последующие сборки форка (`v0.5.2-4` с pre-roll 256 ms и текущая
`v0.5.1002`) — тоже (см. `docs/PENDING.md`).

Сборка с повторным wake (`-dirty` от HEAD `c57d521`); StationCancel без
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
