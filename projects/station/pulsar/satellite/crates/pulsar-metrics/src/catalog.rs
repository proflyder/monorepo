//! Metric catalog of `pulsar_*`: the names the daemon exports.
//!
//! The list is the contract shared by the Prometheus rules, the Grafana dashboards
//! and `observability/test_prometheusrule.py`. `render` panics in tests if a metric
//! is rendered under a name missing here, and every catalog name must appear in a
//! rendered sample ([`crate::render::tests`]), so a rename cannot silently break
//! an alert or a panel.

/// Base metric names (histograms without `_bucket`/`_sum`/`_count`).
pub const NAMES: &[&str] = &[
    "pulsar_info",
    "pulsar_cpu_usage_ratio",
    "pulsar_cpu_seconds_total",
    "pulsar_cpu_temperature_celsius",
    "pulsar_load1",
    "pulsar_load5",
    "pulsar_load15",
    "pulsar_memory_total_bytes",
    "pulsar_memory_available_bytes",
    "pulsar_filesystem_total_bytes",
    "pulsar_filesystem_free_bytes",
    "pulsar_uptime_seconds",
    "pulsar_ha_connected",
    "pulsar_ha_reconnects_total",
    "pulsar_assistant_requests_total",
    "pulsar_assistant_wake_to_stt_seconds",
    "pulsar_assistant_wake_to_answer_seconds",
    "pulsar_playback_started_total",
    "pulsar_playback_finished_total",
    "pulsar_alsa_xruns_total",
    "pulsar_mute",
    "pulsar_volume_ratio",
    "pulsar_call_active",
    "pulsar_call_incoming_total",
    "pulsar_call_ended_total",
    "pulsar_call_duration_seconds",
    "pulsar_call_rx_packets_total",
    "pulsar_call_packets_lost_total",
    "pulsar_call_fec_total",
    "pulsar_call_plc_total",
    "pulsar_call_decode_errors_total",
    "pulsar_call_underrun_total",
    "pulsar_call_overflow_total",
    "pulsar_wake_detections_total",
    "pulsar_wake_near_total",
    "pulsar_wake_miss_total",
    "pulsar_wake_labels_total",
    "pulsar_wake_score",
    "pulsar_wake_threshold",
    "pulsar_wake_model_info",
];

/// Whether `name` is a catalog base name or one of the derived histogram series.
#[must_use]
pub fn contains(name: &str) -> bool {
    if NAMES.contains(&name) {
        return true;
    }
    ["_bucket", "_sum", "_count"].iter().any(|suffix| {
        name.strip_suffix(suffix)
            .is_some_and(|base| NAMES.contains(&base))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_has_the_pulsar_prefix_and_no_duplicates() {
        let mut seen = std::collections::BTreeSet::new();
        for name in NAMES {
            assert!(name.starts_with("pulsar_"), "{name}");
            assert!(seen.insert(*name), "duplicate {name}");
        }
    }

    #[test]
    fn derived_histogram_series_are_known() {
        assert!(contains("pulsar_wake_score_bucket"));
        assert!(contains("pulsar_wake_score_sum"));
        assert!(contains("pulsar_wake_score_count"));
        assert!(!contains("pulsar_wake_score_unknown"));
        assert!(!contains("pulsar_other"));
    }
}
