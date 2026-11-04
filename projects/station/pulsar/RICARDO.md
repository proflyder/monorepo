# Ricardo profile of the Rust Pulsar fork

This fork is the Ricardo Rust profile of Pulsar for the Station Midi
installation. The code lives in the monorepo at `projects/station/pulsar/`; releases are
versions `v0.5.10NN` published to GHCR (see «Релизы и деплой»), and the station runs the
latest such release, the current one is recorded in the monorepo `docs/STATE.md`.

Base: `Morvex885/pulsar` `v0.5.4`, merged with the earlier upstream merges kept in history.
Deliberate deviations from upstream are listed below.

## What this profile keeps

- ESPHome Native API satellite and Sendspin / Music Assistant endpoint.
- Spotify Connect and phone → speaker transfer through the existing HA/MA
  routing.
- Native Yandex VQE, stock command spotter (`стоп`, `громче`, `тише`,
  `пауза`, `продолжи`, `дальше`, `назад`).
- The stock «Алиса» / «Яндекс» spotters as a rollback path; the HA API still
  advertises them, but the default is `ricardo`.
- Hardware mute, action button, ring, clock, volume curve, timer/alarm
  announcements and duck/resume.

## What is changed for this installation

| Area | Change |
|---|---|
| Wake word | New on-device «Рикардо» detector, same microWakeWord model as the deployed fork build; `wake_word=ricardo` is the default. |
| Pre-roll | The wake event carries 0.5 s (16 chunks, 512 ms) of VQE-cleaned audio taken from the shared 1.024 s ring (upstream `wake_preroll` rule `Fixed(16)` for the `ricardo` id), so «Рикардо, включи свет» does not lose its first syllables. The upstream `WakePreroll` switch and `wake_preroll=` key control it. |
| Silent cancel | `stt-no-text-recognized` is treated as a cancel, not as an error ring. |
| Repeated wake | A wake word heard during LISTENING / PROCESSING ends that dialog (`voice_request_stop`) and starts a new one with the wake earcon; during SPEAKING the answer is cut and the new dialog starts. A station that loses wake arbitration does not cut its answer. Late events of a replaced started run are dropped (its RUN_END is treated as stale). |
| `стоп` | The local command spotter is off during LISTENING/PROCESSING/arbitration. «Рикардо, стоп» reaches HA, which stops the alarm and restores the music. |
| `отмена` | Recognized after the wake word as the request, it is an HA custom intent (`StationCancel`) with no speech and no side effects: no TTS answer and no Hermes fallback. The stock command model has no «отмена», so a mid-call «отмена» without a repeated wake word is not detected locally. |
| Local commands | The command spotter runs only while something plays and outside a voice request, matching the deployed fork profile rather than upstream's always-on behavior. |
| Nearest station | Default `nearest_answers=false`: this installation has one station and the 250 ms arbitration window only adds latency. It can be turned back on in HA. |
| Zigbee | Runtime initialization is disabled (`ENABLE_ZIGBEE=false`); the radio and the `zigbee-*.ybl` assets are left untouched. |
| Updates | `UPDATE_REPO=ghcr.io/proflyder/pulsar-firmware` (public OCI package in GHCR, published by the monorepo), `auto_update=false` by default. The upstream repository is never an update source. |
| Hardware | Midi remains the tested target. Lite 2 / Mini 2 code is still compiled by upstream but is not part of this profile's acceptance. |

### Отличие от upstream #44 (стоп-слово в тексте STT)

Upstream разбирает готовый текст STT: если в нём только стоп-слова, диалог, открытый
wake word, завершается, а музыка ставится на паузу. В форке для такого диалога
локального действия нет: «Рикардо, стоп» уходит в Home Assistant, который
останавливает будильник и возвращает музыку (см. строку «`стоп`» выше). Для диалогов,
открытых кнопкой, продолжением разговора или `start_conversation`, поведение upstream
сохранено: выключить звонок таймера, иначе отменить диалог. Решение — `logic::stt_stop_action`.

The custom detector lives in `satellite/crates/pulsar-audio/ricardo/` and is
linked through a small C ABI. It uses the firmware's `libtflite.so` and the
vendored TensorFlow Lite microfrontend; the model is not committed (see
`satellite/wakewords/README.md`).

## Запись wake-событий

Детектор «Рикардо» пишет реальные срабатывания, near-miss и сообщённые промахи, чтобы считать
FP на час прослушивания и добавлять hard negatives в обучение (контракт v1, общий с
инструментами обучения).

- Каталог: `/data/pulsar/wake-events/`. Файлы `<id>.wav` (PCM16 mono 16 кГц, ровно тот
  `VQE_OMNI_WITH_GAIN`, который слышит детектор, без усиления) и `<id>.json` (сайдкар);
  `stats.json` — счётчики (переписывается не чаще раза в 60 с и при остановке демона).
  `id` = `<UTC yyyymmddTHHMMSSZ>-<kind>-<seq>`, `seq` — счётчик запуска.
- Виды: `hit` — подтверждение детектора (3 с до + 1 с после), `near` — локальный максимум
  среднего вероятности в полосе 0.5..cutoff (3 с до пика + 1 с после, не чаще раза в 3 с и не
  раньше 2 с после hit), `miss` — кнопка «Wake: не услышала» (последние 30 с кольца; кольцо 31 с, ~1 МБ).
- Сайдкар: `schema`, `id`, `kind`, `utc`, `sample_rate`, `pre_ms`, `post_ms`, `score_peak`,
  `cutoff`, `window`, `model_sha256`, `pulsar_version`, `dry_run`, `outcome`, `stt_text`,
  `label`, `label_source`, `labeled_utc`. `outcome` заполняется, когда диалог, начатый hit,
  заканчивается: `answered`, `empty_stt`, `cancelled`, `error`, `arbiter_lost`; `dry_run` —
  сразу при записи. `stt_text` — текст STT_END этого диалога. Метки ставит только pulsar:
  `miss` → `label=missed`, `label_source=user`; кнопка «Wake: ложное срабатывание» помечает
  `label=false` у самого нового `hit` за последние 10 минут (если такого нет — предупреждение
  в логе, файлы не меняются).
- Порог и окно берутся из манифеста модели `<stem>.json` рядом с `.tflite`
  (`micro.probability_cutoff` в (0,1), `micro.sliding_window_size` 1..50), иначе 0.9 / 10;
  источник пишется в лог при старте (`wake events: … cutoff … window …`).
- Ретеншн: не больше 300 МБ или 3000 событий. Сначала удаляются самые старые непомеченные
  `near`, затем непомеченные `hit`, затем самые старые вообще; помеченные — последними.
- `stats.json`: `since_utc`, `listening_seconds` (время, реально поданное детектору, без
  мьюта), `hits`, `near`, `miss`, `dry_run_listening_seconds`, `dropped`,
  `model_sha256`. Счётчики продолжаются между перезапусками.

Сущности Home Assistant (ESPHome):

- кнопка `wake_false_alarm` «Wake: ложное срабатывание»;
- кнопка `wake_missed` «Wake: не услышала»;
- переключатель `wake_event_recording` «Wake: запись событий» (по умолчанию включён,
  `state.conf` `wake_event_recording=`);
- переключатель `wake_dry_run` «Wake: тестовый режим (без ответа)» (по умолчанию выключен,
  `wake_dry_run=`): подтверждённый hit записывается с `outcome=dry_run`, но не начинает
  диалог, не участвует в арбитраже и не приглушает музыку. При выключенной записи hit всё
  равно не отвечает, но ничего не сохраняется.

Запись работает только для кастомного детектора «Рикардо» (`wake_word=ricardo`); нативный
спотер не пишет. При мьюте микрофона кольцо не ведётся и время прослушивания не считается.
Поток обработки только копирует аудио в ограниченный канал: при отставании диска окно
теряется и увеличивает `dropped`, микрофон не блокируется.

Как забрать файлы:

```bash
adb -s <serial> pull /data/pulsar/wake-events/ ./wake-events/
# или по ssh:
scp -r root@<ip>:/data/pulsar/wake-events/ ./wake-events/
```

Проверки: `bazel test //projects/station/pulsar/satellite/...` (fmt, clippy с `-D warnings`,
юнит/интеграционные тесты, golden-файлы; Linux). На станции не проверено: реальная запись с
микрофона, загрузка манифеста рядом с моделью, поведение кнопок в HA и заполнение `outcome`
живым пайплайном.

## Build

Linux or WSL2 with Bazelisk (Bazel 9.3.1) and a host C/C++ compiler. Zig 0.14.1,
Rust 1.99.1 and the sendspin C/C++ libraries come from the root `MODULE.bazel`:

```bash
bazel test //projects/station/pulsar/satellite/...

# Deployable daemon for one station ABI (glibc 2.36 / 2.23).
bazel build -c opt \
  --platforms=//platform/bazel/zig:armhf_linux \
  --define=pulsar_version="$(cat projects/station/pulsar/VERSION)" \
  //projects/station/pulsar/satellite:station_binary
```

The outputs are `bazel-bin/projects/station/pulsar/satellite/pulsar` (stripped)
and `pulsar.debug` (unstripped; `pulsar` carries a `.gnu_debuglink`), copied by CI to
`satellite/build/<arch>/`. `PULSAR_BUILD_TIME` defaults to `unknown`; the release
workflow passes HEAD's commit date with `--define=pulsar_build_time=...`.
Targets are Linux-only and skipped on macOS.

The active Midi uses the armhf userland (`//platform/bazel/zig:armhf_linux`).

Put the model here before an install:

```bash
cp /path/to/ru_ricardo_mww.tflite satellite/wakewords/
# необязательно: манифест с micro.probability_cutoff / micro.sliding_window_size
cp /path/to/ru_ricardo_mww.json satellite/wakewords/
```

`pulsar-install` copies it to `/data/pulsar/wakewords/` with a SHA-256 check.
Without the model the daemon starts but logs that the Ricardo detector is
missing; it does not silently fall back to «Алиса».

## Релизы и деплой

Код и релиз Pulsar живут в монорепозитории `proflyder/monorepo`
(`projects/station/pulsar/`). GitHub Releases и теги в монорепозитории запрещены; источник
обновлений станции — OCI-пакет `ghcr.io/proflyder/pulsar-firmware`.

Версия продолжает нумерацию upstream: `v<upstream major.minor>.<1000+N>` — например
`v0.5.1008`; после merge upstream `v0.6.x` — `v0.6.1001`. Патч-номер 1000+ больше любого
upstream-патча, поэтому сравнение версий (`vMAJOR.MINOR.PATCH`) считает релиз новее.

1. Изменить `projects/station/pulsar/VERSION` в рабочей ветке и открыть обычный PR в `develop`.
2. Каждый PR, затрагивающий Pulsar, уже собирает release-shaped артефакт: обе архитектуры с
   версией из `VERSION` (`--define`), radio `.ybl`, `SHA256SUMS`, проверка `--version` под qemu.
   Из этого репозитория PR публикует его как `tree-<git tree hash>` с build provenance.
3. Merge PR (squash) проводит юнит `pulsar` через общий `.github/workflows/deploy-home.yml`:
   `native.publish_when` пускает его в план только при изменении VERSION. `native-release`
   сначала считает tree деплоимого коммита, проверяет `tree-<hash>` и provenance и ставит на
   тот же digest `ghcr.io/proflyder/pulsar-firmware:<version>` и `:latest` — байты, прошедшие
   PR, не пересобираются. Если tree-тега нет или provenance не сходится, workflow собирает и
   публикует `publish-oci.sh` как раньше, с пометкой fallback в summary. Тег версии неизменяем:
   другой digest — отказ, тот же — no-op; двигается только `latest`. Прочие изменения Pulsar
   идут только через PR CI и релизные теги не двигают.
4. Job `deploy-station` ждёт одобрения в Environment `Station` (required reviewer).
5. После одобрения job поднимает VPN (группа `ci`), затем `hashicorp/vault-action` по
   GitHub OIDC (роль `deploy-station`) читает `secret/data/ci/deploy-station` и вызывает
   `tools/ha_firmware_deploy.py --tag <version>`: HA `homeassistant.update_entity` → сущность
   «Firmware» видит версию → `update.install` → колонка скачивает слои, проверяет `SHA256SUMS`
   и `--version`, меняет бинарник (старый остаётся `.prev`) и перезапускается. Скрипт ждёт
   `installed_version == version` и ещё 90 с следит за стабильностью; откат `supervise.sh` на
   `.prev` или недоступность сущности дольше 30 с валят job.
6. Проверить в HA: «Firmware» показывает установленную версию, колонка онлайн; при сомнениях —
   `ctl.sh log` на станции.

### Источник обновлений — GHCR

Апдейтер станции читает публичный OCI-артефакт `ghcr.io/proflyder/pulsar-firmware` (тег `latest`;
аннотация `org.opencontainers.image.version`, слои `pulsar-<arch>.bin`, `SHA256SUMS`,
`zigbee-*.ybl`) и проверяет digest слоя, `SHA256SUMS` и `--version`. Этот пакет публикует
монорепозиторий; константа — `pulsar_update::UPDATE_REPO`.

Переходный момент: установленная прошивка `v0.5.1005` читает GitHub Releases репозитория
`proflyder/pulsar`. Читать `pulsar-firmware` умеет только `v0.5.1007` и новее, поэтому первый
релиз из монорепозитория встанет через HA только на станцию, где уже стоит `v0.5.1007+`.
Пока это не так, бинарник ставится вручную (`pulsar-install` или замена `pulsar` + `.prev` и
`ctl.sh restart`).

Повторный деплой уже опубликованной версии — `workflow_dispatch` для `deploy-home`
с `units=pulsar`: job `deploy-station` катит текущий VERSION, ничего не собирая и не
перезаписывая OCI-тег, либо повторный approve существующего run. Если версия уже стоит,
скрипт печатает `already installed` и выходит успешно.

Откат через dispatch **невозможен**: обновлятор предлагает только более новые версии, поэтому
старая версия не появится в `latest_version`. Откат делается на станции: `.prev` (или резервная
копия бинарника) и `ctl.sh restart`, подробности и команды — в корневом `docs/OPERATIONS.md`
(раздел про станцию). После отката автообновление не поставит сломанную версию повторно
(маркер `update_attempt`).

Что должно быть настроено в GitHub (значения не хранятся в репозитории):

- Environment `Station`: required reviewer;
- его secrets: `VPN_SERVER`, `VPN_CERT_P12_B64`, `VPN_CERT_PASSWORD`, `OPENBAO_ADDR`,
  при необходимости `OPENBAO_ROLE` (по умолчанию `deploy-station`);
- в OpenBao роль `deploy-station` для `auth/github` и секрет `secret/data/ci/deploy-station`
  с ключами `ha_url`, `ha_token`, `entity_id` (сущность `update.<station>_firmware`).

## Verification performed

All checks were host-side; no binary was installed and no station was touched.

- `cargo fmt --all --check` (macOS host, Rust 1.99.1): pass.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` in
  `rust:1.99.1-bookworm`, `linux/arm64`: pass.
- `cargo test --workspace --exclude pulsar-update` in the same container: all
  suites pass, including the new Ricardo pre-roll tests
  (`ricardo_preroll_rule_uses_the_custom_id`,
  `ricardo_preroll_is_the_last_512_ms_of_the_ring`,
  `preroll_off_is_empty_but_the_wake_hold_is_separate`).
- `cargo test -p pulsar-update`: 27 passed, 7 failed on arm64. The tests in
  upstream hard-code the x86_64 asset name `pulsar-host.bin` while the binary
  reports `pulsar-aarch64.bin`; an export of the unmodified upstream `v0.5.2`
  (`4cba44a`) shows the same 7 failures on arm64, so this is a pre-existing
  upstream test limitation, not a merge regression. The new GHCR tests (token,
  manifest, layer digest, bearer only to ghcr.io) pass on arm64.
- `make ARCH=armhf ZIG=…/zig-0.14.1 binary`: the daemon compiles and links as
  an ARM EABI5 armhf ELF (`build/armhf/pulsar.debug`); the last
  `zig objcopy --strip-all` step fails on the macOS host (undefined symbols in
  zig's objcopy), so no stripped `build/armhf/pulsar` was produced here. The
  Linux CI job performs the same strip step.

Not performed: live microphone/wake-word acceptance, real `libtflite.so`
loading on the station, ESPHome entity regression, Spotify/MA playback, or a
rollback test. Those remain a separate deployment gate.

## Rollback

The previous Rust binary and its model are untouched. If this merge is ever
installed, keep:

```text
/data/pulsar/pulsar.prev
/data/pulsar/state.conf
/data/pulsar/wakewords/ru_ricardo_mww.tflite
```

and restore `pulsar.prev` with `ctl.sh restart`. The previous Rust build also
accepts the same `wake_word=ricardo` state and model path.
