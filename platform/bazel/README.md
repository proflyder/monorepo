# Bazel platform layer

Здесь будут общие toolchain-настройки, constraints и платформы
(`linux_amd64`, `linux_arm64`, `darwin_arm64`, будущие mobile).

Сейчас корневой `MODULE.bazel` уже подключает:

- `rules_go` + Gazelle;
- `rules_rust` + Rust toolchain;
- Bzlmod (без WORKSPACE).

Правило: BUILD-файлы приложений не должны содержать машинно-специфичные
пути и абсолютные toolchain-пути.
