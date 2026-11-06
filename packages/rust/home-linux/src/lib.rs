//! Small Unix helpers shared by the first-party daemons.
//!
//! Covers what used to be copied into each Pulsar crate: C `strerror` text for
//! `io::Error` and the sysfs attribute writes. `unsafe` stays here, in the
//! single place that has to turn a raw descriptor into a rustix borrow.
#![cfg(unix)]
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod linux;
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod tty;

#[cfg(target_os = "linux")]
pub use linux::{
    EMPTY_EVENT, Evdev, Framebuffer, I2c, KeyBits, RawInputEvent, SwBits, WallTime, wall_time,
};
#[cfg(target_os = "linux")]
pub use tty::{AsyncTty, Tty};
#[cfg(all(target_os = "linux", feature = "testing"))]
#[allow(unsafe_code)]
pub mod testing;

use std::fs::OpenOptions;
use std::io::Write as _;
use std::os::fd::RawFd;

/// C `strerror` text of an errno value, without Rust's " (os error N)" suffix.
pub fn errno_text(errno: i32) -> String {
    strip_os_error_suffix(&rustix::io::Errno::from_raw_os_error(errno).to_string())
}

/// C `strerror` text of an I/O error, without Rust's " (os error N)" suffix.
///
/// Errors that do not wrap an OS errno keep their own message.
pub fn io_error_text(error: &std::io::Error) -> String {
    error
        .raw_os_error()
        .map_or_else(|| error.to_string(), errno_text)
}

fn strip_os_error_suffix(text: &str) -> String {
    text.find(" (os error")
        .map_or_else(|| text.to_owned(), |end| text[..end].to_owned())
}

/// One write(2) of `val` to an existing file (no create, no truncate): a sysfs
/// attribute takes a value per write call, so a short write is a failure rather
/// than something to resume.
pub fn write_file(path: impl AsRef<std::path::Path>, val: &str) -> bool {
    let Ok(mut file) = OpenOptions::new().write(true).open(path) else {
        return false;
    };
    matches!(file.write(val.as_bytes()), Ok(n) if n == val.len())
}

/// Writes `value` in decimal to an attribute kept open (LED brightness, GPIO
/// value) at offset 0, which sysfs needs for every store on a reused fd.
#[allow(unsafe_code)]
pub fn write_int_fd(fd: RawFd, value: i32) -> bool {
    let mut buffer = [0u8; 12]; // "-2147483648"
    let mut rest = &mut buffer[..];
    if write!(rest, "{value}").is_err() {
        return false;
    }
    let len = 12 - rest.len();
    // SAFETY: `fd` belongs to the caller's open attribute file and stays open
    // for this call; `BorrowedFd::borrow_raw` only borrows it for the duration of
    // the write (no close, no ownership transfer).
    let fd = unsafe { rustix::fd::BorrowedFd::borrow_raw(fd) };
    rustix::io::pwrite(fd, &buffer[..len], 0).is_ok_and(|written| written == len)
}

/// Drops the trailing newline (and spaces / CR) sysfs values end with.
pub fn trim(value: &str) -> &str {
    value.trim_end_matches(['\n', ' ', '\r'])
}

#[cfg(test)]
mod tests {
    use std::os::fd::AsRawFd as _;

    use super::*;

    #[test]
    fn trim_only_trailing_newline_space_cr() {
        assert_eq!(trim("255\n"), "255");
        assert_eq!(trim(" 12 \r\n"), " 12");
        assert_eq!(trim("a\tb\t"), "a\tb\t");
    }

    #[test]
    fn errno_text_is_strerror_without_rust_suffix() {
        let text = errno_text(2);
        assert!(!text.contains("(os error"), "{text}");
        let text = io_error_text(&std::io::Error::from_raw_os_error(2));
        assert!(!text.contains("(os error"), "{text}");
        let custom = io_error_text(&std::io::Error::other("custom"));
        assert_eq!(custom, "custom");
    }

    #[test]
    fn write_int_fd_and_write_file() {
        let dir = std::env::temp_dir().join(format!("home-linux-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("value");
        std::fs::write(&path, b"0").unwrap();
        assert!(write_file(&path, "42"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "42");

        let file = OpenOptions::new().write(true).open(&path).unwrap();
        assert!(write_int_fd(file.as_raw_fd(), -7));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "-7");
        drop(file);
        let _ = std::fs::remove_dir_all(dir);
    }
}
