//! Звуки прошивки: единственное место, где собирается путь (`format!` было в пяти местах).

/// Каталог звуков в прошивке станции.
const SOUNDS: &str = "/system/vendor/quasar/sounds/";

/// Путь к звуку прошивки по имени из профиля модели.
pub fn path(sound: &str) -> String {
    format!("{SOUNDS}{sound}")
}
