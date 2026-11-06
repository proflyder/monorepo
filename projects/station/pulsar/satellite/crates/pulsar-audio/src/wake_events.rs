//! Публичный фасад контракта v1: реализация живёт в `pulsar-wake-events`, здесь — прежние пути
//! для контроллера и утилит.

pub use pulsar_wake_events::{
    Capture, Counters, HIT_PRE_MS, Job, Kind, MISS_MS, Meta, Outcome, POST_MS, RING_MS, Recording,
    Sidecar, Stats, Thresholds, WakeEventOptions, WakeEventsHandle, WakeEventsRuntime, WakeInput,
    WakeStoreError, logic, sha256_hex, start,
};
