const SOUNDS: &str = "/system/vendor/quasar/sounds/";

pub fn path(sound: &str) -> String {
    format!("{SOUNDS}{sound}")
}
