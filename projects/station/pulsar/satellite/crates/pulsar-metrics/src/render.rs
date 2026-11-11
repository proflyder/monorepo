//! Prometheus text exposition (version 0.0.4) of a [`Metrics`] snapshot.

use std::fmt::Write as _;

use crate::catalog;
use crate::metrics::{Histogram, Metrics, PipelineResult, PlaybackKind};
use crate::system::SystemSample;

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('"', "\\\"")
}

fn labels(pairs: &[(&str, &str)]) -> String {
    if pairs.is_empty() {
        return String::new();
    }
    let body = pairs
        .iter()
        .map(|(key, value)| format!("{key}=\"{}\"", escape(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{body}}}")
}

fn header(out: &mut String, name: &str, kind: &str, help: &str) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} {kind}");
}

fn scalar(out: &mut String, name: &str, kind: &str, help: &str, value: f64) {
    header(out, name, kind, help);
    let _ = writeln!(out, "{name} {value}");
}

fn counter(out: &mut String, name: &str, help: &str, value: u64) {
    scalar(out, name, "counter", help, value as f64);
}

fn gauge(out: &mut String, name: &str, help: &str, value: f64) {
    scalar(out, name, "gauge", help, value);
}

fn info(out: &mut String, name: &str, pairs: &[(&str, &str)]) {
    header(out, name, "gauge", "Build information.");
    let _ = writeln!(out, "{name}{} 1", labels(pairs));
}

fn histogram(out: &mut String, name: &str, help: &str, h: &Histogram) {
    header(out, name, "histogram", help);
    for (bound, count) in h.buckets() {
        let le = if bound.is_infinite() {
            "+Inf".to_owned()
        } else {
            bound.to_string()
        };
        let _ = writeln!(out, "{name}_bucket{} {count}", labels(&[("le", &le)]));
    }
    let _ = writeln!(out, "{name}_sum {}", h.sum());
    let _ = writeln!(out, "{name}_count {}", h.count());
}

pub fn render(metrics: &Metrics, sample: &SystemSample) -> String {
    let mut out = String::with_capacity(4096);

    let (version, model) = metrics
        .info
        .get()
        .map_or(("", ""), |(v, m)| (v.as_str(), m.as_str()));
    info(
        &mut out,
        "pulsar_info",
        &[("version", version), ("model", model)],
    );

    // ---- hardware ----
    gauge(
        &mut out,
        "pulsar_cpu_usage_ratio",
        "CPU busy ratio over the interval since the previous scrape.",
        sample.cpu_usage,
    );
    header(
        &mut out,
        "pulsar_cpu_seconds_total",
        "counter",
        "CPU time per mode since boot.",
    );
    for (name, seconds) in crate::system::mode_names().iter().zip(sample.cpu.seconds) {
        let _ = writeln!(
            &mut out,
            "pulsar_cpu_seconds_total{} {seconds}",
            labels(&[("mode", name)])
        );
    }
    labeled_temp(&mut out, sample);
    gauge(
        &mut out,
        "pulsar_load1",
        "1-minute load average.",
        sample.loads[0],
    );
    gauge(
        &mut out,
        "pulsar_load5",
        "5-minute load average.",
        sample.loads[1],
    );
    gauge(
        &mut out,
        "pulsar_load15",
        "15-minute load average.",
        sample.loads[2],
    );
    gauge(
        &mut out,
        "pulsar_memory_total_bytes",
        "Total RAM.",
        sample.memory_total,
    );
    gauge(
        &mut out,
        "pulsar_memory_available_bytes",
        "RAM available for new workloads.",
        sample.memory_available,
    );
    header(
        &mut out,
        "pulsar_filesystem_total_bytes",
        "gauge",
        "Filesystem size.",
    );
    for fs in &sample.filesystems {
        let _ = writeln!(
            &mut out,
            "pulsar_filesystem_total_bytes{} {}",
            labels(&[("mount", fs.mount)]),
            fs.total_bytes
        );
    }
    header(
        &mut out,
        "pulsar_filesystem_free_bytes",
        "gauge",
        "Filesystem bytes available to the user.",
    );
    for fs in &sample.filesystems {
        let _ = writeln!(
            &mut out,
            "pulsar_filesystem_free_bytes{} {}",
            labels(&[("mount", fs.mount)]),
            fs.free_bytes
        );
    }
    gauge(
        &mut out,
        "pulsar_uptime_seconds",
        "Seconds since boot.",
        sample.uptime,
    );

    // ---- Home Assistant ----
    gauge(
        &mut out,
        "pulsar_ha_connected",
        "1 while at least one ESPHome API client completed the handshake.",
        metrics.ha_connected.get(),
    );
    counter(
        &mut out,
        "pulsar_ha_reconnects_total",
        "ESPHome API client handshakes since start.",
        metrics.ha_reconnects.get(),
    );

    // ---- assistant pipeline ----
    header(
        &mut out,
        "pulsar_assistant_requests_total",
        "counter",
        "Voice pipeline runs by result.",
    );
    for (i, label) in PipelineResult::LABELS.iter().enumerate() {
        let _ = writeln!(
            &mut out,
            "pulsar_assistant_requests_total{} {}",
            labels(&[("result", label)]),
            metrics.pipeline[i].get()
        );
    }
    histogram(
        &mut out,
        "pulsar_assistant_wake_to_stt_seconds",
        "Wake word to the start of STT.",
        &metrics.wake_to_stt,
    );
    histogram(
        &mut out,
        "pulsar_assistant_wake_to_answer_seconds",
        "Wake word to the start of TTS playback (playback duration excluded).",
        &metrics.wake_to_answer,
    );

    // ---- playback ----
    header(
        &mut out,
        "pulsar_playback_started_total",
        "counter",
        "Playbacks started by kind.",
    );
    for (i, label) in PlaybackKind::LABELS.iter().enumerate() {
        let _ = writeln!(
            &mut out,
            "pulsar_playback_started_total{} {}",
            labels(&[("kind", label)]),
            metrics.playback_started[i].get()
        );
    }
    header(
        &mut out,
        "pulsar_playback_finished_total",
        "counter",
        "Playbacks finished by kind and result.",
    );
    for (i, label) in PlaybackKind::LABELS.iter().enumerate() {
        for (j, result) in ["ok", "error"].iter().enumerate() {
            let _ = writeln!(
                &mut out,
                "pulsar_playback_finished_total{} {}",
                labels(&[("kind", label), ("result", result)]),
                metrics.playback_finished[i][j].get()
            );
        }
    }
    counter(
        &mut out,
        "pulsar_alsa_xruns_total",
        "ALSA underruns and suspend recoveries.",
        metrics.alsa_xruns.get(),
    );
    header(&mut out, "pulsar_mute", "gauge", "Microphone mute state.");
    for (i, label) in ["hw", "sw"].iter().enumerate() {
        let _ = writeln!(
            &mut out,
            "pulsar_mute{} {}",
            labels(&[("kind", label)]),
            metrics.mute[i].get()
        );
    }
    gauge(
        &mut out,
        "pulsar_volume_ratio",
        "Master volume, 0..1.",
        metrics.volume.get(),
    );

    // ---- calls ----
    gauge(
        &mut out,
        "pulsar_call_active",
        "1 while a call is in progress.",
        metrics.call_active.get(),
    );
    counter(
        &mut out,
        "pulsar_call_incoming_total",
        "Incoming calls.",
        metrics.call_incoming.get(),
    );
    counter(
        &mut out,
        "pulsar_call_ended_total",
        "Finished calls.",
        metrics.call_ended.get(),
    );
    histogram(
        &mut out,
        "pulsar_call_duration_seconds",
        "Call duration.",
        &metrics.call_duration,
    );
    counter(
        &mut out,
        "pulsar_call_rx_packets_total",
        "Received RTP audio packets.",
        metrics.call_rx_packets.get(),
    );
    counter(
        &mut out,
        "pulsar_call_packets_lost_total",
        "RTP sequence numbers missing.",
        metrics.call_packets_lost.get(),
    );
    counter(
        &mut out,
        "pulsar_call_fec_total",
        "Lost frames recovered from in-band FEC.",
        metrics.call_fec.get(),
    );
    counter(
        &mut out,
        "pulsar_call_plc_total",
        "Lost frames concealed by PLC.",
        metrics.call_plc.get(),
    );
    counter(
        &mut out,
        "pulsar_call_decode_errors_total",
        "Undecodable Opus packets.",
        metrics.call_decode_errors.get(),
    );
    counter(
        &mut out,
        "pulsar_call_underrun_total",
        "Downlink ALSA underruns.",
        metrics.call_underrun.get(),
    );
    counter(
        &mut out,
        "pulsar_call_overflow_total",
        "Downlink frames dropped by drift control.",
        metrics.call_overflow.get(),
    );

    // ---- wake word ----
    counter(
        &mut out,
        "pulsar_wake_detections_total",
        "Ricardo wake-word detections.",
        metrics.wake_detections.get(),
    );
    counter(
        &mut out,
        "pulsar_wake_near_total",
        "Rejected near-misses below the threshold.",
        metrics.wake_near.get(),
    );
    counter(
        &mut out,
        "pulsar_wake_miss_total",
        "User-reported missed wake words.",
        metrics.wake_miss.get(),
    );
    header(
        &mut out,
        "pulsar_wake_labels_total",
        "counter",
        "Wake-word feedback labels.",
    );
    for (i, label) in ["false", "missed"].iter().enumerate() {
        let _ = writeln!(
            &mut out,
            "pulsar_wake_labels_total{} {}",
            labels(&[("label", label)]),
            metrics.wake_labels[i].get()
        );
    }
    histogram(
        &mut out,
        "pulsar_wake_score",
        "Windowed detector probability per processed chunk.",
        &metrics.wake_score,
    );
    gauge(
        &mut out,
        "pulsar_wake_threshold",
        "Detector cutoff in use.",
        metrics.wake_threshold.get(),
    );
    let (wake_model, wake_version) = metrics
        .wake_model
        .get()
        .map_or(("", ""), |(m, v)| (m.as_str(), v.as_str()));
    info(
        &mut out,
        "pulsar_wake_model_info",
        &[("model", wake_model), ("version", wake_version)],
    );

    debug_assert!(
        out.lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split(['{', ' ']).next())
            .all(|name| catalog::contains(name.strip_suffix("_bucket").unwrap_or(name))),
        "rendered a metric outside the catalog"
    );
    out
}

fn labeled_temp(out: &mut String, sample: &SystemSample) {
    header(
        out,
        "pulsar_cpu_temperature_celsius",
        "gauge",
        "SoC thermal zone temperature.",
    );
    for zone in &sample.temperatures {
        let _ = writeln!(
            out,
            "pulsar_cpu_temperature_celsius{} {}",
            labels(&[("zone", &zone.zone), ("type", &zone.kind)]),
            zone.celsius
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::Metrics;
    use crate::system::{CpuTimes, Filesystem, Temperature};

    fn sample() -> SystemSample {
        SystemSample {
            cpu: CpuTimes {
                seconds: [1.0, 0.0, 2.0, 7.0, 0.0, 0.0, 0.0, 0.0],
            },
            cpu_usage: 0.25,
            loads: [0.5, 0.4, 0.3],
            temperatures: vec![Temperature {
                zone: "thermal_zone0".into(),
                kind: "soc".into(),
                celsius: 55.5,
            }],
            memory_total: 2_000_000.0,
            memory_available: 500_000.0,
            filesystems: vec![
                Filesystem {
                    mount: "/data",
                    total_bytes: 1000.0,
                    free_bytes: 100.0,
                },
                Filesystem {
                    mount: "/",
                    total_bytes: 2000.0,
                    free_bytes: 200.0,
                },
            ],
            uptime: 42.0,
        }
    }

    #[test]
    fn every_catalog_name_is_rendered() {
        let metrics = Metrics::new();
        metrics.set_info("1.2.3", "cucumber");
        metrics.set_wake_model("ru_ricardo_mww", "1.2.3");
        let text = render(&metrics, &sample());
        for name in catalog::NAMES {
            assert!(
                text.contains(&format!("# TYPE {name} ")),
                "catalog name {name} is not rendered"
            );
        }
    }

    #[test]
    fn text_is_prometheus_shaped() {
        let metrics = Metrics::new();
        metrics.pipeline_finished(PipelineResult::Ok);
        metrics.pipeline_finished(PipelineResult::Timeout);
        metrics.observe_wake_score(0.95);
        metrics.set_mute(true, false);
        let text = render(&metrics, &sample());
        assert!(text.contains("pulsar_assistant_requests_total{result=\"ok\"} 1"));
        assert!(text.contains("pulsar_assistant_requests_total{result=\"timeout\"} 1"));
        assert!(text.contains("pulsar_assistant_requests_total{result=\"error\"} 0"));
        assert!(
            text.contains(
                "pulsar_cpu_temperature_celsius{zone=\"thermal_zone0\",type=\"soc\"} 55.5"
            )
        );
        assert!(text.contains("pulsar_filesystem_free_bytes{mount=\"/data\"} 100"));
        assert!(text.contains("pulsar_mute{kind=\"hw\"} 1"));
        assert!(text.contains("pulsar_mute{kind=\"sw\"} 0"));
        assert!(text.contains("pulsar_wake_score_bucket{le=\"+Inf\"} 1"));
        assert!(text.contains("pulsar_wake_score_count 1"));
        for line in text.lines().filter(|l| !l.starts_with('#')) {
            assert!(!line.contains("NaN"), "NaN in {line}");
        }
    }

    #[test]
    fn histogram_buckets_are_cumulative() {
        let metrics = Metrics::new();
        for value in [0.05, 0.15, 0.95] {
            metrics.observe_wake_score(value);
        }
        let text = render(&metrics, &sample());
        assert!(text.contains("pulsar_wake_score_bucket{le=\"0.1\"} 1"));
        assert!(text.contains("pulsar_wake_score_bucket{le=\"0.2\"} 2"));
        assert!(text.contains("pulsar_wake_score_bucket{le=\"0.99\"} 3"));
        assert!(text.contains("pulsar_wake_score_count 3"));
        assert!(text.contains("pulsar_wake_score_sum 1.15"));
    }

    #[test]
    fn label_values_are_escaped() {
        assert_eq!(escape("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }
}
