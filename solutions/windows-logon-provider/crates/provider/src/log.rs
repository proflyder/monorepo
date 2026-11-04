//! Append-only diagnostic log for the logon provider.
//!
//! Every line is `<UTC ISO8601> pid=<pid> tid=<tid> <event> key=value...`,
//! as documented in `README.md`. Secrets never reach this module; every field
//! is sanitised (control characters replaced by spaces, at most 240 bytes) so
//! remote data cannot forge log lines.
//!
//! Rotation: one generation, at 256 KiB, `logon.log` -> `logon.log.1`. A failure
//! to write is silently ignored: logging must never break logon.

/// Directory and files are created by the installer; the path is fixed.
pub const DATA_DIR: &str = r"C:\ProgramData\Ricardo\KompLogon";
/// Current log file.
pub const LOG_PATH: &str = r"C:\ProgramData\Ricardo\KompLogon\logon.log";
/// Single rotated generation.
pub const ROTATED_PATH: &str = r"C:\ProgramData\Ricardo\KompLogon\logon.log.1";
/// Rotation threshold (256 KiB).
pub const MAX_LOG_BYTES: u64 = 256 * 1024;
/// Cap for the event name and the field string, in bytes.
pub const MAX_FIELD_BYTES: usize = 240;

/// Replaces control characters with spaces and caps the result at
/// [`MAX_FIELD_BYTES`] bytes, matching `SanitizeForLog`.
pub fn sanitize(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(MAX_FIELD_BYTES));
    for c in text.chars() {
        if out.len() + c.len_utf8() > MAX_FIELD_BYTES {
            break;
        }
        if (c as u32) < 0x20 || c == '\u{7f}' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` from already extracted UTC fields.
#[allow(clippy::too_many_arguments)]
pub fn timestamp_string(
    year: u16,
    month: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
) -> String {
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milliseconds:03}Z")
}

/// One complete log line, including the trailing `\n`.
pub fn format_line(timestamp: &str, pid: u32, tid: u32, event: &str, fields: &str) -> String {
    let mut line = String::with_capacity(160 + fields.len());
    line.push_str(timestamp);
    line.push_str(" pid=");
    line.push_str(&pid.to_string());
    line.push_str(" tid=");
    line.push_str(&tid.to_string());
    line.push(' ');
    line.push_str(&sanitize(event));
    if !fields.is_empty() {
        line.push(' ');
        line.push_str(&sanitize(fields));
    }
    line.push('\n');
    line
}

/// True when the current log must be rotated before appending.
pub fn should_rotate(current_size: u64) -> bool {
    current_size >= MAX_LOG_BYTES
}

#[cfg(windows)]
pub use imp::log_event;

#[cfg(windows)]
mod imp {
    use std::sync::{Mutex, Once};

    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::{
        CreateDirectoryW, CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ,
        FILE_SHARE_WRITE, GetFileAttributesExW, GetFileExInfoStandard, MOVEFILE_REPLACE_EXISTING,
        MoveFileExW, OPEN_ALWAYS, WIN32_FILE_ATTRIBUTE_DATA, WriteFile,
    };
    use windows::Win32::System::SystemInformation::GetSystemTime;
    use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
    use windows::core::PCWSTR;

    use super::{DATA_DIR, LOG_PATH, ROTATED_PATH, format_line, should_rotate, timestamp_string};
    use crate::provider::widen_nul;

    static LOG_LOCK: Mutex<()> = Mutex::new(());
    static LOG_INIT: Once = Once::new();

    fn ensure_data_dir() {
        LOG_INIT.call_once(|| {
            // Idempotent: ERROR_ALREADY_EXISTS is expected. The installer owns
            // the ACL; if the directory is missing we simply cannot log.
            // SAFETY: a static NUL-terminated path and no security attributes.
            let _ = unsafe { CreateDirectoryW(PCWSTR(widen_nul(DATA_DIR).as_ptr()), None) };
        });
    }

    fn rotate_if_needed() {
        let mut data = WIN32_FILE_ATTRIBUTE_DATA::default();
        // SAFETY: `data` is a valid, correctly sized out-parameter for
        // GetFileExInfoStandard, and the path literal outlives the call.
        let attributes = unsafe {
            GetFileAttributesExW(
                PCWSTR(widen_nul(LOG_PATH).as_ptr()),
                GetFileExInfoStandard,
                (&raw mut data).cast(),
            )
        };
        if attributes.is_err() {
            return;
        }
        let size = (u64::from(data.nFileSizeHigh) << 32) | u64::from(data.nFileSizeLow);
        if !should_rotate(size) {
            return;
        }
        // One generation only: the current log always describes the latest run.
        // SAFETY: both arguments are static NUL-terminated paths.
        unsafe {
            let _ = windows::Win32::Storage::FileSystem::DeleteFileW(PCWSTR(
                widen_nul(ROTATED_PATH).as_ptr(),
            ));
            let _ = MoveFileExW(
                PCWSTR(widen_nul(LOG_PATH).as_ptr()),
                PCWSTR(widen_nul(ROTATED_PATH).as_ptr()),
                MOVEFILE_REPLACE_EXISTING,
            );
        }
    }

    fn timestamp() -> String {
        // SAFETY: GetSystemTime has no preconditions and writes the whole struct.
        let now = unsafe { GetSystemTime() };
        timestamp_string(
            now.wYear,
            now.wMonth,
            now.wDay,
            now.wHour,
            now.wMinute,
            now.wSecond,
            now.wMilliseconds,
        )
    }

    /// Appends one line. Never panics for I/O reasons and never blocks the
    /// caller longer than a single small `WriteFile`.
    pub fn log_event(event: &str, fields: &str) {
        ensure_data_dir();
        // SAFETY: both getters take no arguments and return plain integers.
        let (pid, tid) = unsafe { (GetCurrentProcessId(), GetCurrentThreadId()) };
        let line = format_line(&timestamp(), pid, tid, event, fields);

        let _unpoisoned = LOG_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        rotate_if_needed();

        // SAFETY: OPEN_ALWAYS creates the file if needed; no security
        // attributes and no template file are passed. The handle is closed on
        // every path.
        match unsafe {
            CreateFileW(
                PCWSTR(widen_nul(LOG_PATH).as_ptr()),
                FILE_APPEND_DATA.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_ALWAYS,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        } {
            Ok(handle) => {
                let mut written = 0u32;
                // SAFETY: `handle` is an open append handle and the slice stays
                // alive for the duration of the call.
                unsafe {
                    let _ = WriteFile(handle, Some(line.as_bytes()), Some(&raw mut written), None);
                    let _ = CloseHandle(handle);
                }
            }
            Err(_) => {
                // Logging must never break logon.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_control_characters() {
        assert_eq!(sanitize("a\nb\tc\r\0d"), "a b c  d");
        assert_eq!(sanitize("plain text"), "plain text");
        assert_eq!(sanitize("del\u{7f}ete"), "del ete");
    }

    #[test]
    fn sanitize_caps_at_the_field_limit() {
        let long = "x".repeat(1000);
        assert_eq!(sanitize(&long).len(), MAX_FIELD_BYTES);
        // Exactly at the limit is not truncated.
        let exact = "y".repeat(MAX_FIELD_BYTES);
        assert_eq!(sanitize(&exact), exact);
    }

    #[test]
    fn timestamp_matches_the_cxx_format() {
        assert_eq!(
            timestamp_string(2026, 11, 3, 21, 44, 12, 123),
            "2026-11-03T21:44:12.123Z"
        );
        assert_eq!(
            timestamp_string(2026, 1, 2, 3, 4, 5, 6),
            "2026-01-02T03:04:05.006Z"
        );
    }

    #[test]
    fn format_line_matches_the_documented_shape() {
        assert_eq!(
            format_line("2026-11-03T21:44:12.123Z", 912, 913, "provider_load", ""),
            "2026-11-03T21:44:12.123Z pid=912 tid=913 provider_load\n"
        );
        assert_eq!(
            format_line(
                "2026-11-03T21:44:12.130Z",
                912,
                1204,
                "worker",
                "phase=start budget_ms=90000"
            ),
            "2026-11-03T21:44:12.130Z pid=912 tid=1204 worker phase=start budget_ms=90000\n"
        );
    }

    #[test]
    fn format_line_sanitizes_both_fields() {
        assert_eq!(
            format_line("t", 1, 2, "e\nv", "a=1\nb=2"),
            "t pid=1 tid=2 e v a=1 b=2\n"
        );
    }

    #[test]
    fn rotation_happens_at_the_threshold() {
        assert!(!should_rotate(0));
        assert!(!should_rotate(MAX_LOG_BYTES - 1));
        assert!(should_rotate(MAX_LOG_BYTES));
        assert!(should_rotate(MAX_LOG_BYTES + 1));
    }
}
