//! `tracing` bootstrap shared by the daemons in this repository.
//!
//! The builder owns the subscriber layers so a service only decides where the
//! human-readable log goes and which level applies. Pulsar can attach its
//! ESPHome sink with [`TelemetryBuilder::with_layer`]; the station BLE gateway
//! uses the default fmt layer. Local time is captured through
//! [`local_offset`] **before** the Tokio runtime starts: `time` refuses
//! `current_local_offset` once the process is multi-threaded.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};
use tracing::Subscriber;
use tracing::span;
use tracing_subscriber::Registry;
pub use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

const TIMESTAMP: &[time::format_description::BorrowedFormatItem<'static>] = format_description!(
    "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6][offset_hour sign:mandatory]:[offset_minute]"
);

/// Local UTC offset for the log timestamp.
///
/// Call this on the main thread before building a Tokio runtime; afterwards the
/// platform may refuse to read `/etc/localtime`.
#[must_use]
pub fn local_offset() -> UtcOffset {
    UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC)
}

struct LocalTimer {
    offset: UtcOffset,
}

impl FormatTime for LocalTimer {
    fn format_time(&self, writer: &mut Writer<'_>) -> fmt::Result {
        let now = OffsetDateTime::now_utc().to_offset(self.offset);
        writer.write_str(&now.format(TIMESTAMP).map_err(|_| fmt::Error)?)
    }
}

#[derive(Clone)]
enum Sink {
    Stderr,
    File(Arc<Mutex<File>>),
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
    type Writer = SinkWriter;

    fn make_writer(&'a self) -> Self::Writer {
        match self {
            Self::Stderr => SinkWriter::Stderr,
            Self::File(file) => SinkWriter::File(file.clone()),
        }
    }
}

enum SinkWriter {
    Stderr,
    File(Arc<Mutex<File>>),
}

impl Write for SinkWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Stderr => io::stderr().write(buffer),
            Self::File(file) => file
                .lock()
                .map_or(Ok(buffer.len()), |mut file| file.write(buffer)),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Stderr => io::stderr().flush(),
            Self::File(file) => file.lock().map_or(Ok(()), |mut file| file.flush()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogTarget {
    Stderr,
    File(PathBuf),
}

pub struct TelemetryBuilder {
    level: LevelFilter,
    target: LogTarget,
    offset: UtcOffset,
    layers: Vec<Box<dyn Layer<Registry> + Send + Sync>>,
}

impl Default for TelemetryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            level: LevelFilter::INFO,
            target: LogTarget::Stderr,
            offset: local_offset(),
            layers: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_max_level(mut self, level: LevelFilter) -> Self {
        self.level = level;
        self
    }

    #[must_use]
    pub fn with_stderr(self) -> Self {
        Self {
            target: LogTarget::Stderr,
            ..self
        }
    }

    /// Append to `path`, creating parent directories.
    #[must_use]
    pub fn with_file(self, path: impl Into<PathBuf>) -> Self {
        Self {
            target: LogTarget::File(path.into()),
            ..self
        }
    }

    #[must_use]
    pub fn with_local_offset(mut self, offset: UtcOffset) -> Self {
        self.offset = offset;
        self
    }

    /// Attach an extra layer (for example Pulsar's ESPHome sink).
    #[must_use]
    pub fn with_layer<L>(mut self, layer: L) -> Self
    where
        L: Layer<Registry> + Send + Sync + 'static,
    {
        self.layers.push(Box::new(layer));
        self
    }

    /// Build the subscriber without installing it globally.
    pub fn build(self) -> io::Result<impl Subscriber + Send + Sync> {
        let sink = match &self.target {
            LogTarget::Stderr => Sink::Stderr,
            LogTarget::File(path) => {
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    std::fs::create_dir_all(parent)?;
                }
                Sink::File(Arc::new(Mutex::new(
                    OpenOptions::new().create(true).append(true).open(path)?,
                )))
            }
        };
        let fmt = tracing_subscriber::fmt::layer()
            .with_timer(LocalTimer {
                offset: self.offset,
            })
            .with_target(false)
            .with_level(true)
            .with_ansi(false)
            .with_writer(sink);
        let mut layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> = vec![Box::new(self.level)];
        layers.push(Box::new(fmt));
        layers.extend(self.layers);
        Ok(Registry::default().with(LayerSet(layers)))
    }

    /// Install the subscriber as the process-wide default.
    pub fn init(self) -> io::Result<()> {
        tracing::subscriber::set_global_default(self.build()?)
            .map_err(|error| io::Error::new(io::ErrorKind::AlreadyExists, error))
    }
}

/// Type-erased collection of layers evaluated against [`Registry`].
struct LayerSet(Vec<Box<dyn Layer<Registry> + Send + Sync>>);

impl Layer<Registry> for LayerSet {
    fn register_callsite(
        &self,
        metadata: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        let mut sometimes = false;
        for layer in &self.0 {
            let interest = layer.register_callsite(metadata);
            if interest.is_never() {
                return tracing::subscriber::Interest::never();
            }
            sometimes |= interest.is_sometimes();
        }
        if sometimes {
            tracing::subscriber::Interest::sometimes()
        } else {
            tracing::subscriber::Interest::always()
        }
    }

    fn enabled(&self, metadata: &tracing::Metadata<'_>, context: Context<'_, Registry>) -> bool {
        self.0
            .iter()
            .all(|layer| layer.enabled(metadata, context.clone()))
    }

    fn on_new_span(
        &self,
        attributes: &span::Attributes<'_>,
        id: &span::Id,
        context: Context<'_, Registry>,
    ) {
        for layer in &self.0 {
            layer.on_new_span(attributes, id, context.clone());
        }
    }

    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, context: Context<'_, Registry>) {
        for layer in &self.0 {
            layer.on_record(id, values, context.clone());
        }
    }

    fn on_follows_from(&self, id: &span::Id, follows: &span::Id, context: Context<'_, Registry>) {
        for layer in &self.0 {
            layer.on_follows_from(id, follows, context.clone());
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, context: Context<'_, Registry>) {
        for layer in &self.0 {
            layer.on_event(event, context.clone());
        }
    }

    fn on_enter(&self, id: &span::Id, context: Context<'_, Registry>) {
        for layer in &self.0 {
            layer.on_enter(id, context.clone());
        }
    }

    fn on_exit(&self, id: &span::Id, context: Context<'_, Registry>) {
        for layer in &self.0 {
            layer.on_exit(id, context.clone());
        }
    }

    fn on_close(&self, id: span::Id, context: Context<'_, Registry>) {
        for layer in &self.0 {
            layer.on_close(id.clone(), context.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tracing_subscriber::Layer;
    use tracing_subscriber::registry::LookupSpan;

    use super::*;

    #[derive(Clone, Default)]
    struct Counter(Arc<AtomicUsize>);

    impl<L: for<'a> LookupSpan<'a> + Subscriber> Layer<L> for Counter {
        fn on_event(&self, _event: &tracing::Event<'_>, _context: Context<'_, L>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ricardo-telemetry-{tag}-{}-{}.log",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn file_output_filter_and_extra_layer() {
        let path = temp_path("file");
        let counter = Counter::default();
        let subscriber = TelemetryBuilder::new()
            .with_max_level(LevelFilter::WARN)
            .with_file(&path)
            .with_local_offset(UtcOffset::from_hms(3, 0, 0).expect("offset"))
            .with_layer(counter.clone())
            .build()
            .expect("build subscriber");

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("hidden by the filter");
            tracing::warn!(answer = 42, "visible");
        });

        let contents = std::fs::read_to_string(&path).expect("read log");
        assert!(contents.contains("WARN"), "{contents}");
        assert!(contents.contains("visible"), "{contents}");
        assert!(contents.contains("answer=42"), "{contents}");
        assert!(!contents.contains("hidden"), "{contents}");
        assert!(contents.contains("+03:00"), "{contents}");
        assert_eq!(counter.0.load(Ordering::SeqCst), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn builder_attaches_multiple_layers_without_global_installation() {
        let first = Counter::default();
        let second = Counter::default();
        let subscriber = TelemetryBuilder::new()
            .with_max_level(LevelFilter::TRACE)
            .with_stderr()
            .with_layer(first.clone())
            .with_layer(second.clone())
            .build()
            .expect("build");
        tracing::subscriber::with_default(subscriber, || {
            tracing::trace!("one event");
            tracing::debug!("another event");
        });
        assert_eq!(first.0.load(Ordering::SeqCst), 2);
        assert_eq!(second.0.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn local_offset_is_stable() {
        assert!(local_offset().whole_hours().abs() <= 14);
    }
}
