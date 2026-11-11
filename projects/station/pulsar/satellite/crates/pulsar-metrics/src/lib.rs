//! Prometheus metrics of the Pulsar daemon on the station.
//!
//! The daemon exports `GET /metrics` (Prometheus text format) on the LAN
//! interface, port [`DEFAULT_PORT`] by default (plus `PULSAR_PORT_OFFSET`).
//! Hardware metrics are read from `/proc` and `/sys` at scrape time; daemon
//! metrics are relaxed atomics updated on the existing paths, so the audio
//! thread and the call loop never block or allocate.
//!
//! Names form the contract with the Prometheus rules and Grafana dashboards in
//! `projects/station/observability`; see [`catalog`].
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod catalog;
mod http;
mod metrics;
mod render;
pub mod system;

pub use metrics::{Histogram, Metrics, PipelineResult, PlaybackKind};

/// Prometheus scrape port before `PULSAR_PORT_OFFSET`.
pub const DEFAULT_PORT: u16 = 9101;

static GLOBAL: Metrics = Metrics::new();

/// The process-wide registry: every service records here.
#[must_use]
pub fn global() -> &'static Metrics {
    &GLOBAL
}

/// Starts the HTTP endpoint; a bind failure is logged, not fatal.
pub fn start_http(port: u16) {
    if let Err(e) = http::spawn(port, global()) {
        tracing::warn!("metrics: cannot serve on port {port}: {e}");
    }
}

pub fn init(version: &str, model: &str) {
    global().set_info(version, model);
}

pub fn set_wake_model(model: &str, version: &str) {
    global().set_wake_model(model, version);
}

pub fn set_wake_threshold(threshold: f64) {
    global().set_wake_threshold(threshold);
}

pub fn ha_connected(connected: bool) {
    global().set_ha_connected(connected);
}

pub fn ha_reconnect() {
    global().ha_reconnect();
}

pub fn pipeline_finished(result: PipelineResult) {
    global().pipeline_finished(result);
}

pub fn observe_wake_to_stt(seconds: f64) {
    global().observe_wake_to_stt(seconds);
}

pub fn observe_wake_to_answer(seconds: f64) {
    global().observe_wake_to_answer(seconds);
}

pub fn playback_started(kind: PlaybackKind) {
    global().playback_started(kind);
}

pub fn playback_finished(kind: PlaybackKind, ok: bool) {
    global().playback_finished(kind, ok);
}

pub fn alsa_xrun() {
    global().alsa_xrun();
}

pub fn set_mute(hardware: bool, software: bool) {
    global().set_mute(hardware, software);
}

pub fn set_volume(ratio: f64) {
    global().set_volume(ratio);
}

pub fn set_call_active(active: bool) {
    global().set_call_active(active);
}

pub fn call_incoming() {
    global().call_incoming();
}

pub fn call_ended(seconds: f64) {
    global().call_ended(seconds);
}

pub fn call_rx_packet() {
    global().call_rx_packet();
}

pub fn call_packet_lost(n: u64) {
    global().call_packet_lost(n);
}

pub fn call_fec() {
    global().call_fec();
}

pub fn call_plc() {
    global().call_plc();
}

pub fn call_decode_error() {
    global().call_decode_error();
}

pub fn call_underrun() {
    global().call_underrun();
}

pub fn call_overflow() {
    global().call_overflow();
}

pub fn wake_detected() {
    global().wake_detected();
}

pub fn wake_near() {
    global().wake_near();
}

pub fn wake_miss() {
    global().wake_miss();
}

pub fn wake_label_false_alarm() {
    global().wake_label(true);
}

pub fn wake_label_missed() {
    global().wake_label(false);
}

pub fn observe_wake_score(score: f64) {
    global().observe_wake_score(score);
}
