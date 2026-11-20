pub fn set_tz(posix_tz: &str) {
    crate::sys::set_tz(posix_tz);
}
