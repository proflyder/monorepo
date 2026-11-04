# Bazel platform layer

Общие toolchain-настройки и платформы. Корневой `MODULE.bazel` (Bzlmod, без
WORKSPACE) подключает `rules_go` + Gazelle, `rules_rust`, `rules_oci` и
Android-правила; `zig/` — Zig cross-toolchain и платформы
`//platform/bazel/zig:<arch>_linux` для станционных сборок Pulsar.

Правило: BUILD-файлы приложений не должны содержать машинно-специфичные
пути и абсолютные toolchain-пути.
