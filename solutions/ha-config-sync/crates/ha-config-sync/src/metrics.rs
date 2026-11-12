//! Prometheus text exposition for one sync worker.
//!
//! The families are rendered by hand: the drift family must contain only the
//! currently drifted files (a `Family` would keep stale label sets), and the
//! exact series names are part of the alert contract.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use crate::config::Mode;

/// The four values of `ha_config_sync_apply_total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyResult {
    /// Applied, validated, reloaded and verified.
    Success,
    /// Applied, then rolled back after a failure.
    RolledBack,
    /// The rollback itself (or the apply) failed; manual attention needed.
    Failed,
    /// Restart was postponed because the guard entity was not `off`.
    Deferred,
}

impl ApplyResult {
    /// The metric label value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::RolledBack => "rolled_back",
            Self::Failed => "failed",
            Self::Deferred => "deferred",
        }
    }
}

#[derive(Debug, Clone)]
struct Labels {
    drift: BTreeSet<String>,
    agent_version: String,
    desired_digest: String,
}

/// All instruments of one worker.
#[derive(Debug)]
pub struct Metrics {
    labels: Mutex<Labels>,
    managed: AtomicU64,
    last_check_success: AtomicU64,
    last_check_timestamp: AtomicI64,
    last_apply_timestamp: AtomicI64,
    apply_success: AtomicU64,
    apply_rolled_back: AtomicU64,
    apply_failed: AtomicU64,
    apply_deferred: AtomicU64,
    restart_pending: AtomicU64,
    mode: Mode,
}

impl Metrics {
    /// Build instruments for one mode.
    #[must_use]
    pub fn new(mode: Mode) -> Self {
        Self {
            labels: Mutex::new(Labels {
                drift: BTreeSet::new(),
                agent_version: String::new(),
                desired_digest: String::new(),
            }),
            managed: AtomicU64::new(0),
            last_check_success: AtomicU64::new(0),
            last_check_timestamp: AtomicI64::new(0),
            last_apply_timestamp: AtomicI64::new(0),
            apply_success: AtomicU64::new(0),
            apply_rolled_back: AtomicU64::new(0),
            apply_failed: AtomicU64::new(0),
            apply_deferred: AtomicU64::new(0),
            restart_pending: AtomicU64::new(0),
            mode,
        }
    }

    /// Replace the drifted-file set.
    pub fn set_drift(&self, files: &BTreeSet<String>) {
        if let Ok(mut labels) = self.labels.lock() {
            labels.drift.clone_from(files);
        }
    }

    /// Set the managed-file count.
    pub fn set_managed(&self, count: usize) {
        self.managed.store(count as u64, Ordering::Relaxed);
    }

    /// Record the agent's reported protocol version.
    pub fn set_agent_version(&self, version: &str) {
        if let Ok(mut labels) = self.labels.lock() {
            version.clone_into(&mut labels.agent_version);
        }
    }

    /// Record the desired-set digest.
    pub fn set_desired_digest(&self, digest: &str) {
        if let Ok(mut labels) = self.labels.lock() {
            digest.clone_into(&mut labels.desired_digest);
        }
    }

    /// Record one check outcome and its Unix timestamp.
    pub fn record_check(&self, success: bool, timestamp: i64) {
        self.last_check_success
            .store(u64::from(success), Ordering::Relaxed);
        self.last_check_timestamp
            .store(timestamp, Ordering::Relaxed);
    }

    /// Record that an apply attempt finished at `timestamp`.
    pub fn record_apply_timestamp(&self, timestamp: i64) {
        self.last_apply_timestamp
            .store(timestamp, Ordering::Relaxed);
    }

    /// Count one apply result.
    pub fn count_apply(&self, result: ApplyResult) {
        let counter = match result {
            ApplyResult::Success => &self.apply_success,
            ApplyResult::RolledBack => &self.apply_rolled_back,
            ApplyResult::Failed => &self.apply_failed,
            ApplyResult::Deferred => &self.apply_deferred,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Record whether a deferred restart is still outstanding.
    pub fn set_restart_pending(&self, pending: bool) {
        self.restart_pending
            .store(u64::from(pending), Ordering::Relaxed);
    }

    /// Render the Prometheus text body.
    #[must_use]
    pub fn render(&self) -> String {
        let labels = match self.labels.lock() {
            Ok(labels) => (*labels).clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let mut out = String::new();
        let drift = labels.drift.len();
        let _ = writeln!(out, "# TYPE ha_config_sync_drift_files gauge");
        let _ = writeln!(out, "ha_config_sync_drift_files {drift}");
        let _ = writeln!(out, "# TYPE ha_config_sync_drift gauge");
        for file in &labels.drift {
            let _ = writeln!(
                out,
                "ha_config_sync_drift{{file=\"{}\"}} 1",
                escape_label(file)
            );
        }
        let _ = writeln!(out, "# TYPE ha_config_sync_managed_files gauge");
        let _ = writeln!(
            out,
            "ha_config_sync_managed_files {}",
            self.managed.load(Ordering::Relaxed)
        );
        let _ = writeln!(out, "# TYPE ha_config_sync_last_check_success gauge");
        let _ = writeln!(
            out,
            "ha_config_sync_last_check_success {}",
            self.last_check_success.load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "# TYPE ha_config_sync_last_check_timestamp_seconds gauge"
        );
        let _ = writeln!(
            out,
            "ha_config_sync_last_check_timestamp_seconds {}",
            self.last_check_timestamp.load(Ordering::Relaxed)
        );
        let _ = writeln!(
            out,
            "# TYPE ha_config_sync_last_apply_timestamp_seconds gauge"
        );
        let _ = writeln!(
            out,
            "ha_config_sync_last_apply_timestamp_seconds {}",
            self.last_apply_timestamp.load(Ordering::Relaxed)
        );
        let _ = writeln!(out, "# TYPE ha_config_sync_apply_total counter");
        for (result, counter) in [
            (ApplyResult::Success, &self.apply_success),
            (ApplyResult::RolledBack, &self.apply_rolled_back),
            (ApplyResult::Failed, &self.apply_failed),
            (ApplyResult::Deferred, &self.apply_deferred),
        ] {
            let _ = writeln!(
                out,
                "ha_config_sync_apply_total{{result=\"{}\"}} {}",
                result.as_str(),
                counter.load(Ordering::Relaxed)
            );
        }
        if !labels.agent_version.is_empty() {
            let _ = writeln!(out, "# TYPE ha_config_sync_agent_info gauge");
            let _ = writeln!(
                out,
                "ha_config_sync_agent_info{{version=\"{}\"}} 1",
                escape_label(&labels.agent_version)
            );
        }
        let _ = writeln!(out, "# TYPE ha_config_sync_desired_info gauge");
        let _ = writeln!(
            out,
            "ha_config_sync_desired_info{{digest=\"{}\"}} 1",
            escape_label(&labels.desired_digest)
        );
        let _ = writeln!(out, "# TYPE ha_config_sync_restart_pending gauge");
        let _ = writeln!(
            out,
            "ha_config_sync_restart_pending {}",
            self.restart_pending.load(Ordering::Relaxed)
        );
        let _ = writeln!(out, "# TYPE ha_config_sync_mode gauge");
        let _ = writeln!(
            out,
            "ha_config_sync_mode{{mode=\"{}\"}} 1",
            self.mode.as_str()
        );
        out
    }
}

fn escape_label(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drift_lists_only_drifted_files() {
        let metrics = Metrics::new(Mode::Apply);
        metrics.set_managed(3);
        metrics.set_drift(&BTreeSet::from([
            "a.yaml".to_owned(),
            "custom_sentences/ru/b.yaml".to_owned(),
        ]));
        let text = metrics.render();
        assert!(text.contains("ha_config_sync_drift_files 2\n"));
        assert!(text.contains("ha_config_sync_drift{file=\"a.yaml\"} 1\n"));
        assert!(text.contains("ha_config_sync_drift{file=\"custom_sentences/ru/b.yaml\"} 1\n"));
        assert!(text.contains("ha_config_sync_managed_files 3\n"));

        metrics.set_drift(&BTreeSet::new());
        let text = metrics.render();
        assert!(text.contains("ha_config_sync_drift_files 0\n"));
        assert!(!text.contains("ha_config_sync_drift{file="));
    }

    #[test]
    fn counters_modes_and_info_are_rendered() {
        let metrics = Metrics::new(Mode::Observe);
        metrics.set_agent_version("1");
        metrics.set_desired_digest("abc");
        metrics.record_check(true, 1_700_000_000);
        metrics.record_apply_timestamp(1_700_000_100);
        metrics.count_apply(ApplyResult::Success);
        metrics.count_apply(ApplyResult::RolledBack);
        metrics.count_apply(ApplyResult::Failed);
        metrics.count_apply(ApplyResult::Deferred);
        metrics.set_restart_pending(true);
        let text = metrics.render();
        assert!(text.contains("ha_config_sync_last_check_success 1\n"));
        assert!(text.contains("ha_config_sync_last_check_timestamp_seconds 1700000000\n"));
        assert!(text.contains("ha_config_sync_last_apply_timestamp_seconds 1700000100\n"));
        assert!(text.contains("ha_config_sync_apply_total{result=\"success\"} 1\n"));
        assert!(text.contains("ha_config_sync_apply_total{result=\"rolled_back\"} 1\n"));
        assert!(text.contains("ha_config_sync_apply_total{result=\"failed\"} 1\n"));
        assert!(text.contains("ha_config_sync_apply_total{result=\"deferred\"} 1\n"));
        assert!(text.contains("ha_config_sync_agent_info{version=\"1\"} 1\n"));
        assert!(text.contains("ha_config_sync_desired_info{digest=\"abc\"} 1\n"));
        assert!(text.contains("ha_config_sync_restart_pending 1\n"));
        assert!(text.contains("ha_config_sync_mode{mode=\"observe\"} 1\n"));
        metrics.set_restart_pending(false);
        assert!(
            metrics
                .render()
                .contains("ha_config_sync_restart_pending 0\n")
        );
    }

    #[test]
    fn label_values_are_escaped() {
        assert_eq!(escape_label("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }
}
