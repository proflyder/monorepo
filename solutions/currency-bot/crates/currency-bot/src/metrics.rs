//! Prometheus exposition: only the `currency_bot_*` families.
//!
//! The Go service registered the default Go/process collectors as well; the
//! Rust service exports exactly what the dashboard and ServiceMonitor use, with
//! the same names, labels and `DefBuckets`. The registry is local, so tests can
//! build independent instances.

use std::sync::atomic::AtomicU64;

use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::Histogram;
use prometheus_client::registry::{Registry, Unit};

/// Go's `prometheus.DefBuckets`.
pub const DEF_BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

type Labels = Vec<(String, String)>;

fn labels(pairs: &[(&str, &str)]) -> Labels {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

/// The registry and every instrument.
pub struct Metrics {
    registry: Registry,
    rates: Family<Labels, Gauge<f64, AtomicU64>>,
    job_runs: Family<Labels, Counter>,
    job_failures: Family<Labels, Counter>,
    parse_seconds: Family<Labels, Histogram>,
    telegram_failures: Counter,
    http_requests: Family<Labels, Counter>,
}

impl std::fmt::Debug for Metrics {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Metrics").finish_non_exhaustive()
    }
}

impl Metrics {
    /// Build an isolated registry.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Registry::default();
        let rates = Family::<Labels, Gauge<f64, AtomicU64>>::default();
        let job_runs = Family::<Labels, Counter>::default();
        let job_failures = Family::<Labels, Counter>::default();
        let parse_seconds =
            Family::<Labels, Histogram>::new_with_constructor(|| Histogram::new(DEF_BUCKETS));
        let telegram_failures = Counter::default();
        let http_requests = Family::<Labels, Counter>::default();

        registry.register(
            "currency_bot_rate",
            "Latest aggregated rate by pair and side",
            rates.clone(),
        );
        registry.register(
            "currency_bot_job_runs",
            "Scheduled or manual job runs",
            job_runs.clone(),
        );
        registry.register(
            "currency_bot_job_failures",
            "Failed job runs",
            job_failures.clone(),
        );
        registry.register_with_unit(
            "currency_bot_parse_duration",
            "Time spent fetching and parsing kurs.kz",
            Unit::Seconds,
            parse_seconds.clone(),
        );
        registry.register(
            "currency_bot_telegram_send_failures",
            "Failed Telegram sendMessage calls",
            telegram_failures.clone(),
        );
        registry.register(
            "currency_bot_http_requests",
            "HTTP requests by route and status",
            http_requests.clone(),
        );
        Self {
            registry,
            rates,
            job_runs,
            job_failures,
            parse_seconds,
            telegram_failures,
            http_requests,
        }
    }

    /// Update the latest-rate gauges.
    pub fn set_rates(&self, usd_buy: f64, usd_sell: f64, rub_buy: f64, rub_sell: f64) {
        self.rates
            .get_or_create(&labels(&[("pair", "USD_TO_KZT"), ("type", "BUY")]))
            .set(usd_buy);
        self.rates
            .get_or_create(&labels(&[("pair", "USD_TO_KZT"), ("type", "SELL")]))
            .set(usd_sell);
        self.rates
            .get_or_create(&labels(&[("pair", "RUB_TO_KZT"), ("type", "BUY")]))
            .set(rub_buy);
        self.rates
            .get_or_create(&labels(&[("pair", "RUB_TO_KZT"), ("type", "SELL")]))
            .set(rub_sell);
    }

    /// Count a finished run.
    pub fn inc_job_run(&self, job: &str) {
        self.job_runs.get_or_create(&labels(&[("job", job)])).inc();
    }

    /// Count a failed run.
    pub fn inc_job_failure(&self, job: &str) {
        self.job_failures
            .get_or_create(&labels(&[("job", job)]))
            .inc();
    }

    /// Record a parse duration.
    pub fn observe_parse(&self, job: &str, seconds: f64) {
        self.parse_seconds
            .get_or_create(&labels(&[("job", job)]))
            .observe(seconds);
    }

    /// Count a failed Telegram send.
    pub fn inc_telegram_failure(&self) {
        self.telegram_failures.inc();
    }

    /// Count one HTTP request by matched route and status.
    pub fn observe_http(&self, route: &str, status: u16) {
        self.http_requests
            .get_or_create(&labels(&[
                ("route", route),
                ("status", &status.to_string()),
            ]))
            .inc();
    }

    /// Encode the whole registry in the OpenMetrics text format.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut out = String::new();
        encode(&mut out, &self.registry).expect("encoding metrics to a String cannot fail");
        out
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_only_the_documented_families_and_labels() {
        let metrics = Metrics::new();
        metrics.set_rates(485.5, 487.2, 4.85, 4.92);
        metrics.inc_job_run("hourly");
        metrics.inc_job_failure("trigger");
        metrics.observe_parse("hourly", 0.007);
        metrics.inc_telegram_failure();
        metrics.observe_http("/api/history", 401);
        let text = metrics.encode();

        for line in [
            "currency_bot_rate{pair=\"USD_TO_KZT\",type=\"BUY\"} 485.5",
            "currency_bot_rate{pair=\"RUB_TO_KZT\",type=\"SELL\"} 4.92",
            "currency_bot_job_runs_total{job=\"hourly\"} 1",
            "currency_bot_job_failures_total{job=\"trigger\"} 1",
            "currency_bot_telegram_send_failures_total 1",
            "currency_bot_http_requests_total{route=\"/api/history\",status=\"401\"} 1",
        ] {
            assert!(text.contains(line), "missing {line:?} in\n{text}");
        }
        assert!(
            text.contains(
                "currency_bot_parse_duration_seconds_bucket{le=\"0.005\",job=\"hourly\"}"
            ),
            "{text}"
        );
        assert!(text.contains("# TYPE currency_bot_rate gauge"), "{text}");
        assert!(
            text.contains("# TYPE currency_bot_job_runs counter"),
            "{text}"
        );
        assert!(
            text.contains("# TYPE currency_bot_parse_duration_seconds histogram"),
            "{text}"
        );
        // No Go/process collectors: only currency_bot_* sample lines.
        for line in text.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            assert!(
                line.starts_with("currency_bot_"),
                "unexpected series {line}"
            );
        }
    }

    #[test]
    fn buckets_are_go_def_buckets() {
        let metrics = Metrics::new();
        metrics.observe_parse("hourly", 0.004);
        metrics.observe_parse("hourly", 100.0);
        let text = metrics.encode();
        for bucket in DEF_BUCKETS {
            assert!(
                text.contains(&format!("le=\"{bucket:?}\"")),
                "missing bucket {bucket} in\n{text}"
            );
        }
        assert!(text.contains("le=\"+Inf\""), "{text}");
    }
}
