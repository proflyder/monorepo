# packages/rust

Общий core-слой first-party Rust. Здесь живут крейты, у которых не меньше
двух потребителей (станция, BLE gateway, Windows logon provider, инструменты).

- Пакет не импортирует `projects/**` и не содержит проектных endpoint'ов, путей
  БД или deployment-специфики: один и тот же крейт подключается в разных
  проектах (см. `docs/MONOREPO.md`).
- Крейты — члены корневого Cargo workspace (`/Cargo.toml`), зависимости
  объявляются через `[workspace.dependencies]`.
- Пока каталог пуст: P0 добавил только workspace-фундамент, крейты появятся
  отдельными шагами рефакторинга.
