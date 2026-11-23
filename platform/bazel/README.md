# Bazel platform layer

Общие toolchain-настройки и платформы для корневого `MODULE.bazel`.

- `zig/` — герметичный zig 0.14.1 cc-toolchain и платформы
  `//platform/bazel/zig:<arch>_linux` для станции (Pulsar, BLE gateway).
- `windows/` — платформа `x86_64_windows` для Windows logon provider.
- `rust/musl_binary.bzl`, `oci/` — статические musl-бинарники и сервисные
  образы; `//platform/bazel:linux_amd64` — целевая платформа образов.
- `downloader.cfg` — rewrite-правила загрузок через Nexus в CI.

BUILD-файлы приложений не содержат машинно-специфичных и абсолютных
toolchain-путей. Правка `zig/**` и `windows/**` выбирает только native-юниты,
остальное в `platform/bazel/**` — все юниты.

## Заметки к коду

- `downloader.cfg`: Maven Central и Google Maven идут каждый в свой прокси, а
  не через группу `maven-public` — часть координат есть в обоих с разными
  байтами, и группа отдала бы не ту копию, что пинует `maven_install.json`.
  Правила `maven.google.com` разведены по префиксам: в Google Maven идут
  только Google-группы (`androidx`, `com/android`, `com/google/android`,
  `com/google/firebase`, `com/google/gms`, ...), остальные URL, которые
  модули (rules_android, rules_robolectric) запрашивают у Google Maven первым,
  переписываются в Maven Central — иначе каждый такой артефакт давал 404 в
  `google-maven` (~230 на Android-job). Новая Google-группа вне списка
  упадёт загрузкой — её нужно добавить в оба правила Google Maven.
- `zig/repositories.bzl`: `ctx.os.name` для macOS — `mac os x`, а не написание
  архива zig; `nm`/`gcov` не используются, но Bazel требует валидный путь.
