# Разработка

Справочник по сборке, CI и релизам, установке на станцию, супервизору, обновлениям из HA, правилам
`installer/` и crash-отчёту: пины, контракты между компонентами и версиями, внешнее поведение. Чего
здесь нет: команд проверки и правил репозитория (AGENTS.md), протокола работы на живых станциях
([stations.md](stations.md)), фактов железа (вики и docs моделей, указатель —
[hardware.md, «Модели»](hardware.md#модели)); поведение кода — в коде, обоснования — в
комментариях у кода.

## Сборка

Сборка — на Linux или WSL2. Демон и его C/C++-зависимости собираются Bazel
(Bazelisk, Bazel 9.3.1); версии закреплены в корневом `MODULE.bazel`:
Rust 1.99.1 (совпадает с корневым `/rust-toolchain.toml`), zig 0.14.1 с
cc-toolchain для `aarch64-linux-gnu` (glibc 2.36) и
`arm-linux-gnueabihf` (glibc 2.23, cortex-a53/VFPv4), sendspin-cpp и его
зависимости — `http_archive` с теми же ревизиями, что раньше качал
`third_party/fetch.sh` (BUILD-overlay'и — `satellite/third_party/bazel/`).
Rust, zig и C/C++ скачиваются Bazel'ом; rustup, cargo-zigbuild, cmake и ninja
не нужны. Установщик — тот же Rust workspace; для установки — adb из platform-tools.

Крейты Pulsar — члены общего корневого Cargo workspace: пины зависимостей
лежат в `/Cargo.toml` (`[workspace.dependencies]`), один `Cargo.lock` и один
`rust-toolchain.toml` на весь монорепозиторий, `crate_universe` читает их как
`@crates//`. Общие линты — `[workspace.lints]` там же; Bazel передаёт их в
`rust_clippy_test` через `extract_cargo_lints`. Upstream-патч, меняющий
манифесты upstream, переносится вручную: версии из upstream
`[workspace.dependencies]` добавляются/обновляются в корневом файле, а
манифесты крейтов сохраняют `{ workspace = true }` (см. `RICARDO.md`).

Хост-проверки (fmt, clippy с `-D warnings`, юнит/интеграционные тесты,
golden-файлы) — те же, что в CI-юните `pulsar-bazel`:

```sh
bazel test //projects/station/pulsar/satellite/... //projects/station/pulsar/installer/...
```

Кросс-сборка на одну архитектуру; release передаёт версию и дату коммита:

```sh
version=$(tr -d '[:space:]' < projects/station/pulsar/VERSION)
build_time=$(TZ=UTC0 LC_ALL=C git log -1 --format=%cd --date=format-local:'%b %e %Y, %H:%M:%S')
bazel build -c opt \
  --platforms=//platform/bazel/zig:aarch64_linux \
  --define=pulsar_version="$version" \
  --define=pulsar_build_time="$build_time" \
  //projects/station/pulsar/satellite:station_binary
```

(`armhf_linux` вместо `aarch64_linux` для 32-битного userland.) `-c opt`
даёт `opt-level=3`, thin LTO, `debuginfo=1` и `panic=abort` как Cargo-профиль
release. `panic=abort` включён только у финального `pulsar` для `:station`
(host-тесты того же triple собираются с unwind и не затрагиваются): паника в
задаче завершает процесс, и supervisor перезапускает демон, а не оставляет его
полурабочим. Без `--define` версия и время — `unknown`; host-сборка значения
не использует.

Выходы `satellite:station_binary`:

- `bazel-bin/projects/station/pulsar/satellite/pulsar` — стрипнутый, именно он
  ставится на станцию;
- `pulsar.debug` — нестрипнутый бинарник (символы и debug info); в
  `pulsar` записан `.gnu_debuglink`, поэтому `addr2line -f -e pulsar.debug
  <addr>` разбирает crash-отчёт (non-PIE, адреса абсолютные).

CI копирует их в `satellite/build/<arch>/`; локально можно сделать то же
`cp` (см. workflow) или передать путь из `bazel-bin`.

`ssh/build.sh` собирает dropbear и sftp-server для `pulsar-install --ssh` и
архивов релиза.

Установщик при запуске из чекаута собирает демон сам: ищет `bazel`, затем
`bazelisk` (mise ставит только его), запускает найденное из корня Git с
`--disk_cache=` и берёт каталог вывода из `bazel info -c opt
--platforms=//platform/bazel/zig:<arch>_linux bazel-bin`. Это осознанное отличие
от прежнего Go-кода, который звал `bazel` из каталога `projects/station/pulsar`
и читал локальный `bazel-bin`; в монорепозитории симлинк лежит в корне.

Версия бинарника — содержимое `VERSION`, переданное в
`--define=pulsar_version`; апдейтер разбирает её как `vX.Y.Z[-N-g<hash>]` и
сверяет `--version` нового бинарника с версией OCI-тега. Время сборки —
дата HEAD-коммита в формате C++ `__DATE__ ", " __TIME__`, поэтому пересборка
того же коммита не меняет `option_env!` и не вызывает relink.

## CI и релизы

CI Pulsar разделён на два юнита. `pulsar-bazel` (`.github/workflows/_bazel.yml`) — хост-сборка,
`rustfmt`, `clippy` и тесты: `bazel build //projects/station/pulsar/...` + `bazel test
//projects/station/pulsar/satellite/... //projects/station/pulsar/installer/...`. `pulsar` (`.github/workflows/_native.yml`) — Bazel-кросс-сборка
обеих архитектур и установщика (`pulsar-install`), `radio/build.sh` и `ssh/build.sh`.
Версии инструментов — как в «Сборке» выше; `make check` больше не дублируется.

Релиз — изменение `projects/station/pulsar/VERSION` (`v<major.minor>.<1000+N>`) в PR в
`develop`; merge проводит юнит `pulsar` через общий `.github/workflows/deploy-home.yml`
(`native.publish_when` по VERSION). `_native.yml` собирает обе архитектуры с этой версией и
radio, проверяет `--version` под qemu, складывает `pulsar-<arch>.bin`, `SHA256SUMS` и
`zigbee-*.ybl` в OCI-артефакт `ghcr.io/proflyder/pulsar-firmware:<version>`
(`publish-oci.sh`) и переносит `:latest`; затем `deploy-station` после approve в Environment
`Station` катит его на станцию. GitHub Releases и теги не используются. Прочие изменения
Pulsar — только PR CI. Имена слоёв — постоянный контракт; от них зависят:

| Слой OCI | Кто зависит |
|---|---|
| `pulsar-<arch>.bin` (`aarch64`, `armhf`) | апдейтер: сверяет digest слоя, `SHA256SUMS` и `--version` |
| `zigbee-coordinator.ybl`, `zigbee-router.ybl` | апдейтер (только Миди; релиз без образа обновляется без него); архивы — `radio/out/zigbee-*.ybl` для установщика |
| `SHA256SUMS` | апдейтер сверяет по нему `.bin` и `.ybl` |
| `pulsar-<arch>.debug` | не в OCI: workflow-артефакт для разбора crash-отчёта |

Версия из `VERSION` передаётся сборке (`--define=pulsar_version=...`) и должна совпадать с
`pulsar --version`; апдейтер предлагает только строго большие `MAJOR.MINOR.PATCH`.
`pulsar-windows-amd64.zip`, `pulsar-macos-*.tar.gz`, `pulsar-linux-*.tar.gz` и
`pulsar-install` больше не публикуются из монорепозитория; установщик собирается в CI как
проверка (`installer/dist/`); macOS-бинарь собирается нативно на Mac.

## Установка и станция

```sh
cd projects/station/pulsar
cargo run -p pulsar-installer -- --list         # найти станции и определить модель, ничего не ставить
cargo run -p pulsar-installer -- <ip>[:port]    # установить или обновить
```

`cwd` должен быть `projects/station/pulsar` (или его потомок): установщик ищет payload
`satellite/deploy/boot.sh` рядом с собой и в родителях рабочего каталога. В чекауте он сначала сам
вызывает `bazel build` (с `--ssh` — и `ssh/build.sh`). Флаги — usage в
[`../installer/testdata/usage.txt`](../installer/testdata/usage.txt) (снят с Go и вкомпилирован
в [`../installer/src/cli.rs`](../installer/src/cli.rs)).

**Детект модели — три места, которые должны совпадать:** демон (`detect_profile()` в
[`../satellite/crates/pulsar-model/src/detect.rs`](../satellite/crates/pulsar-model/src/detect.rs)),
установщик (`modelName()` / `abis()` / `stationCode()` и особый маркер Мини в `probe()` в
[`../installer/src/probe.rs`](../installer/src/probe.rs)) и
[`../satellite/deploy/boot.sh`](../satellite/deploy/boot.sh) (список кодовых имён `yandex,<codename>`
из `/proc/device-tree/compatible`). У Мини записи `yandex,*` нет, и все три места требуют ОБА
признака: `/proc/device-tree/amlogic-dt-id` = `axg_s420_v03` и `deviceType=yandexmini` в
`/system/vendor/quasar/quasar.cfg` (почему одного id мало —
[mini.md, «Userland и хранилище»](mini.md#userland-и-хранилище)).
Новая модель добавляется во все три места.

**Выбор сборки** (aarch64 или armhf) — комментарий у `abis()` в
[`../installer/src/probe.rs`](../installer/src/probe.rs). Правило: вторая ABI в `abis()` — только для
userspace, действительно встреченного на этой модели.

**Раскладка `/data/pulsar`:** `pulsar`, `pulsar.prev`, `pulsar.bad`, `boot.sh`, `supervise.sh`,
`ctl.sh`, `state.conf`, `api_key`, `enabled`, `sendspin/`, `zigbee-coordinator.ybl`,
`zigbee-router.ybl` (Миди), `asound.conf` (только Мини).

- `.prev` — цель отката супервизора; `.bad` — бинарник, заменённый откатом после crash-цикла.
- `state.conf` — формат и совместимость с `.prev`: [architecture.md, «state.conf и откат на
  .prev»](architecture.md#stateconf-и-откат-на-prev).
- `api_key` — Noise PSK ESPHome: base64 от 32 байт, режим 0600. Пишут автонастройка HA (по API) или
  `ctl.sh api-key`. Нет файла — API без шифрования; повреждённый файл — сервер работает, но не
  пускает ни одного клиента (fail closed, без отката на plaintext); детали —
  [`../satellite/crates/pulsar-esphome/src/key.rs`](../satellite/crates/pulsar-esphome/src/key.rs).
- `enabled` — пока файл есть, `boot.sh` и супервизор держат pulsar вместо стека Яндекса.
- `asound.conf` (Мини) пишет только установщик. Обновление из HA меняет лишь бинарник и образы
  радио, поэтому правка `satellite/deploy/asound.conf` доходит до станции только при повторном
  запуске `pulsar-install`.
- Вне `/data` — только `/etc/init.d/S99zpulsar` (автозапуск; пишет установщик, правила —
  AGENTS.md, «Безопасность станций»).
- Лог: `/tmp/pulsar.log` (`ctl.sh log N`; ротация — заголовок `supervise.sh`). Путь — общий
  контракт `supervise.sh`, `ctl.sh`, установщика (health check) и `tools/station.sh`. `/tmp` —
  tmpfs: [hardware.md, «RAM и tmpfs»](hardware.md#ram-и-tmpfs).

**Цепочка загрузки:** busybox init → `rcS` (запускает `/etc/init.d/S??*` по порядку с `start` и
ждёт каждый) → скрипт прошивки: `S97quasar` (Миди), `S99quasar` (Лайт 2; Мини 2 — по данным
контрибьюторов), `S99onboot.sh` (Мини, по данным контрибьюторов) → `onboot.sh` (adbd через
`activate_adb.sh check`, quasar в фоне; возвращается) → `S99zpulsar` → `boot.sh` в фоне →
ожидание quasar, `/system/vendor/quasar/q stop` (`q` есть на Лайт 2; Мини 2 — по данным
контрибьюторов) → `supervise.sh`. Мини 3 (`bergamot`) поддерживают `boot.sh` и
установщик, но скрипт прошивки Мини 3 не записан. Условия, при которых `boot.sh` не трогает стек
Яндекса, — в его заголовке и комментариях.

**Ручная замена бинарника** (ssh — после `pulsar-install --ssh`; через adb — `adb push`):

```sh
scp satellite/build/<arch>/pulsar root@<ip>:/data/pulsar/pulsar.new
ssh root@<ip> 'cd /data/pulsar && cp pulsar pulsar.prev &&
  mv pulsar.new pulsar && ./ctl.sh restart'
```

## Супервизор

Контракт — заголовок [`../satellite/deploy/supervise.sh`](../satellite/deploy/supervise.sh)
(коды выхода, короткие сбои, откат на `.prev`, возврат стека Яндекса, ротация лога, очистка
зависшего dmix lock); здесь он не пересказывается. Механизм зависшего dmix lock —
[hardware.md, «Зависший dmix lock»](hardware.md#зависший-dmix-lock).

Контракт строк лога (`supervise.sh`, crash-отчёт демона, маркер установщика) с health check
установщика (`health_check()` в [`../installer/src/deploy.rs`](../installer/src/deploy.rs)): после маркера
установки в логе есть `=== start`, и начиная с первого из них — ровно одна такая строка (новый
демон), процесс `pulsar` жив, а сбоем считаются `=== CRASH` (пишет демон), `=== exit` и
`=== supervise`. Строка `=== alsa: removed stale dmix lock (…)`, которую `supervise.sh` пишет перед
запуском, если что-то удалил, — штатная очистка и сбоем считаться не должна.

## Обновления из HA

`pulsar-update`, сущности «Firmware» и «Auto update»; поведение для пользователя — README,
[«Обновление из Home Assistant»](../README.md#обновление-из-home-assistant). Что нельзя сломать:

1. **Имена ассетов** — постоянны, см. [«CI и релизы»](#ci-и-релизы).
2. **Поиск последнего релиза** — OCI-манифест `https://ghcr.io/v2/proflyder/pulsar-firmware/manifests/latest`
   (анонимный pull-token), аннотация `org.opencontainers.image.version` и слои по
   `org.opencontainers.image.title`. GitHub REST API и GitHub Releases не используются: лимит
   60 запросов в час на публичный IP исчерпывается станциями за общим выходом VPN / CGNAT.
   Клиент — `reqwest` (HTTP/1, rustls с провайдером ring) с корнями из CA-бандла прошивки.
3. **Маркер `update_attempt=<tag>` в `state.conf`** записывается на диск до подмены бинарника и
   стирается новым бинарником, если его версия равна `<tag>`. Маркер, оставшийся под другой
   версией, значит, что релиз не встал: откат `supervise.sh` или фатальная ошибка установки
   (`UpdateError::fatal`, `pulsar-update/src/error.rs`; после нефатальной маркер стирается).
   Автообновление этот тег больше не ставит (вручную — можно). Ключ читают и пишут разные версии
   демона.

## Правила installer/

- `installer/` — крейт `pulsar-installer` в корневом Cargo workspace, бинарь `pulsar-install`.
  Это единственный установщик на стороне хоста.
- Зависимости — только `[workspace.dependencies]` (плюс `zip`/`ureq` для архива platform-tools);
  из инструментов хоста — только adb (плюс `bazel` и `ssh/build.sh` в чекауте).
- Команды на станции — строки busybox sh через `Adb::sh` в
  [`../installer/src/adb.rs`](../installer/src/adb.rs) (adb shell + маркер `__rc=`); одна
  команда — не больше 4 КБ на этих прошивках, лимит проверяет `ProcessAdb`.
- `adb shell` на прошивках станций — pty: в выводе `\r`, канал не binary-safe (бинарные данные —
  только `adb push` / `adb pull`), код выхода всегда 0 (настоящий передаётся маркером `__rc=`).
- Чистые части (текст init-скрипта, проверка защищённости корневой ФС, чистка `rootkeeper.sh`)
  покрыты тестами [`../installer/src/scripts.rs`](../installer/src/scripts.rs); фикстуры
  `installer/testdata/` сняты с Go-версии до её удаления, последовательность adb-команд деплоя
  сверяется с ними же.

## Диагностика

При падении демон пишет в лог (`/tmp/pulsar.log`) отчёт, начинающийся с `=== CRASH`. Контракт:
бинарник собран non-PIE (`-C relocation-model=static`), поэтому адреса в отчёте абсолютные и
разбираются `addr2line -f -e` только по `pulsar.debug` той же сборки
(`satellite/build/<arch>/pulsar.debug`; для релиза — ассет `pulsar-<arch>.debug`). Формат отчёта —
[`../satellite/crates/pulsar/src/crash.rs`](../satellite/crates/pulsar/src/crash.rs) (заголовок и
запись полей в обработчике); процедура разбора — skill `crash-decode`.
