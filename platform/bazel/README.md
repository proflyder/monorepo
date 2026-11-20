# Bazel platform layer

Общие toolchain-настройки и платформы. Корневой `MODULE.bazel` (Bzlmod, без
WORKSPACE) подключает `rules_rust`, `rules_oci` и Android-правила; `zig/` —
Zig cross-toolchain и платформы `//platform/bazel/zig:<arch>_linux` для
станционных сборок Pulsar. `//platform/bazel:linux_amd64` — целевая
платформа образов (os:linux + cpu:x86_64).

Правило: BUILD-файлы приложений не должны содержать машинно-специфичные
пути и абсолютные toolchain-пути.

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `downloader.cfg` — Bazel downloader rewrites for CI.
  - Maven Central and Google Maven each go to their own proxy, never through the maven-public group: some coordinates (com.android.tools.build:transform-api) exist in both with different bytes, the group answers with the Central copy and maven_install.json pins …
- `rust/musl_binary.bzl` — Builds one Rust binary for the static musl image target.
- `windows/BUILD.bazel` — Windows cross-build platform for the Home Komp Logon credential provider (solutions/windows-logon-provider).
- `zig/repositories.bzl` — Hermetic zig 0.14.1 SDK repository and the cc_toolchain_config macro.
  - Bazel reports the host OS as `ctx.os.name`, which is not the zig archive spelling: macOS is "mac os x" (older Bazel also "macos"/"darwin"), Linux is "linux".
  - One wrapper set per target: the triple, CPU and float ABI are baked into the wrapper instead of a per-action flag, matching the former Makefile/cargo-zigbuild invocation.
  - `nm`/`gcov` are never on the code path we build (Rust and static C/C++), but Bazel expects a valid tool path for them.
- `zig/toolchain.bzl` — Hermetic zig cc_toolchain config.
