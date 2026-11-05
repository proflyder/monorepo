//! Logging with the same shape as the Go daemon: local time with microseconds,
//! either on stderr or appended to `--log`.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};

const FORMAT: &[time::format_description::BorrowedFormatItem<'static>] =
    format_description!("[year]/[month]/[day] [hour]:[minute]:[second].[subsecond digits:6]");

enum Sink {
    Stderr,
    File(Mutex<File>),
}

/// Shared logger. Cloning the handle is cheap and every task may log.
///
/// The local offset is resolved by `main` before any thread is created:
/// `time` refuses `now_local()` once the process is multi-threaded, so the
/// offset is captured once and reused for every line.
#[derive(Clone)]
pub struct Logger {
    sink: Arc<Sink>,
    offset: UtcOffset,
}

impl Logger {
    pub fn stderr(offset: UtcOffset) -> Self {
        Self {
            sink: Arc::new(Sink::Stderr),
            offset,
        }
    }

    /// Open `path` for appending, creating its parent directory like the Go
    /// `--log` flag does.
    pub fn file(path: &Path, offset: UtcOffset) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            sink: Arc::new(Sink::File(Mutex::new(file))),
            offset,
        })
    }

    pub fn log(&self, args: fmt::Arguments<'_>) {
        let stamp = format_timestamp(OffsetDateTime::now_utc(), self.offset);
        let line = format!("{stamp} {args}\n");
        match &*self.sink {
            Sink::Stderr => {
                let _ = std::io::stderr().write_all(line.as_bytes());
            }
            Sink::File(file) => {
                if let Ok(mut file) = file.lock() {
                    let _ = file.write_all(line.as_bytes());
                }
            }
        }
    }
}

fn format_timestamp(now: OffsetDateTime, offset: UtcOffset) -> String {
    now.to_offset(offset)
        .format(FORMAT)
        .unwrap_or_else(|_| "0000/00/00 00:00:00.000000".to_string())
}

#[macro_export]
macro_rules! logf {
    ($logger:expr, $($arg:tt)*) => {
        $logger.log(format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_uses_configured_offset() {
        let epoch = OffsetDateTime::from_unix_timestamp(0).expect("epoch");
        let plus_three = UtcOffset::from_hms(3, 0, 0).expect("offset");
        assert_eq!(
            format_timestamp(epoch, plus_three),
            "1970/01/01 03:00:00.000000"
        );
        let minus_five_thirty = UtcOffset::from_hms(-5, -30, 0).expect("offset");
        assert_eq!(
            format_timestamp(epoch, minus_five_thirty),
            "1969/12/31 18:30:00.000000"
        );
    }
}
