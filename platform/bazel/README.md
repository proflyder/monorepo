# Bazel platform layer

Общие toolchain-настройки и платформы. Корневой `MODULE.bazel` (Bzlmod, без
WORKSPACE) подключает `rules_rust`, `rules_oci` и Android-правила; `zig/` —
Zig cross-toolchain и платформы `//platform/bazel/zig:<arch>_linux` для
станционных сборок Pulsar. `//platform/bazel:linux_amd64` — целевая
платформа образов (os:linux + cpu:x86_64).

Правило: BUILD-файлы приложений не должны содержать машинно-специфичные
пути и абсолютные toolchain-пути.
