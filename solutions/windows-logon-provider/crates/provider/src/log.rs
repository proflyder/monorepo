//! Append-only diagnostic log for the logon provider.
//!
//! Line format, rotation (one generation at 256 KiB) and the event set are the
//! contract in `README.md`. Calls go through `tracing`; the provider-local
//! layer below renders the documented line shape through a Win32 append
//! writer, because `LogonUI` has no stderr and the file must be rotated before
//! every write. Every field is sanitised (control characters replaced, 240
//! bytes max) so remote data cannot forge log lines. A write failure is
//! ignored: logging must never break logon.

/// Directory and files are created by the installer; the path is fixed.
pub const DATA_DIR: &str = r"C:\ProgramData\Ricardo\KompLogon";
pub const LOG_PATH: &str = r"C:\ProgramData\Ricardo\KompLogon\logon.log";
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

pub fn should_rotate(current_size: u64) -> bool {
    current_size >= MAX_LOG_BYTES
}

#[cfg(windows)]
pub use imp::log_event;

#[cfg(windows)]
mod imp {
    use std::fmt;
    use std::sync::{Mutex, OnceLock};

    use tracing::dispatcher::Dispatch;
    use tracing::field::{Field, Visit};
    use tracing::{Event, Subscriber};
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::Context;
    use tracing_subscriber::layer::SubscriberExt;

    use crate::bindings::Windows::Win32::Foundation::CloseHandle;
    use crate::bindings::Windows::Win32::Storage::FileSystem::{
        CreateDirectoryW, CreateFileW, DeleteFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL,
        FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileAttributesExW, GetFileExInfoStandard,
        MOVEFILE_REPLACE_EXISTING, MoveFileExW, OPEN_ALWAYS, WIN32_FILE_ATTRIBUTE_DATA, WriteFile,
    };
    use crate::bindings::Windows::Win32::System::SystemInformation::GetSystemTime;
    use crate::bindings::Windows::Win32::System::Threading::{
        GetCurrentProcessId, GetCurrentThreadId,
    };
    use crate::wide::WideNul;

    use super::{DATA_DIR, LOG_PATH, ROTATED_PATH, format_line, should_rotate, timestamp_string};

    static LOG_LOCK: Mutex<()> = Mutex::new(());

    fn ensure_data_dir() {
        // Idempotent: ERROR_ALREADY_EXISTS is expected. The installer owns the
        // ACL; if the directory is missing we simply cannot log.
        let data_dir = WideNul::new(DATA_DIR);
        // SAFETY: `data_dir` is NUL-terminated; no security attributes.
        let _ = unsafe { CreateDirectoryW(data_dir.as_pcwstr(), None) };
    }

    fn rotate_if_needed() {
        let mut data = WIN32_FILE_ATTRIBUTE_DATA::default();
        let log_path = WideNul::new(LOG_PATH);
        // SAFETY: `data` is a valid, correctly sized out-parameter for
        // GetFileExInfoStandard, and `log_path` is NUL-terminated.
        let attributes = unsafe {
            GetFileAttributesExW(
                log_path.as_pcwstr(),
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
        let rotated_path = WideNul::new(ROTATED_PATH);
        // One generation only: the current log always describes the latest run.
        // SAFETY: both paths are NUL-terminated and live for the call.
        unsafe {
            let _ = DeleteFileW(rotated_path.as_pcwstr());
            let _ = MoveFileExW(
                log_path.as_pcwstr(),
                rotated_path.as_pcwstr(),
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
    fn write_line(line: &str) {
        let _serialized = crate::sync::lock(&LOG_LOCK);
        rotate_if_needed();
        let log_path = WideNul::new(LOG_PATH);
        // SAFETY: OPEN_ALWAYS creates the file if needed; no security
        // attributes and no template file are passed, and `log_path` is
        // NUL-terminated. The handle is closed on every path.
        match unsafe {
            CreateFileW(
                log_path.as_pcwstr(),
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

    /// The two fields every event carries; tracing also records callsite
    /// metadata, which this fixed log format does not include.
    #[derive(Default)]
    struct LineFields {
        event: String,
        fields: String,
    }

    impl Visit for LineFields {
        fn record_str(&mut self, field: &Field, value: &str) {
            match field.name() {
                "event" => self.event.push_str(value),
                "fields" => self.fields.push_str(value),
                _ => {}
            }
        }

        fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
            self.record_str(field, &format!("{value:?}"));
        }
    }

    struct LineLayer;

    impl<S: Subscriber> Layer<S> for LineLayer {
        fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
            let mut fields = LineFields::default();
            event.record(&mut fields);
            // SAFETY: both getters take no arguments and return plain integers.
            let (pid, tid) = unsafe { (GetCurrentProcessId(), GetCurrentThreadId()) };
            write_line(&format_line(
                &timestamp(),
                pid,
                tid,
                &fields.event,
                &fields.fields,
            ));
        }
    }

    /// Installed once per DLL lifetime. `Dispatch::with_default` scopes it to
    /// the call instead of `set_global_default`, which would leave the global
    /// dispatcher pointing into this DLL after LogonUI unloads it.
    fn dispatch() -> &'static Dispatch {
        static DISPATCH: OnceLock<Dispatch> = OnceLock::new();
        DISPATCH.get_or_init(|| {
            Dispatch::new(
                tracing_subscriber::registry()
                    .with(tracing_subscriber::filter::LevelFilter::INFO)
                    .with(LineLayer),
            )
        })
    }

    pub fn log_event(event: &str, fields: &str) {
        ensure_data_dir();
        tracing::dispatcher::with_default(dispatch(), || {
            tracing::info!(target: "ricardo_logon", event = event, fields = fields);
        });
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
