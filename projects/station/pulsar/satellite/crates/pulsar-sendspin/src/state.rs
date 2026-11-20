use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pulsar_core::Timers;
use tracing::info;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Tick,
    StreamEnd,
}

pub const STREAM_END_HOLD_MS: u64 = 1000;

pub struct Status {
    streaming: AtomicBool,
    enabled: AtomicBool,
}

impl Status {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            streaming: AtomicBool::new(false),
            enabled: AtomicBool::new(true),
        })
    }

    pub fn streaming(&self) -> bool {
        self.streaming.load(Ordering::Relaxed)
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Relaxed);
    }
}

pub struct StreamHold {
    status: Arc<Status>,
    group_stopped: bool,
}

impl StreamHold {
    pub fn new(status: Arc<Status>) -> Self {
        Self {
            status,
            group_stopped: false,
        }
    }

    pub fn start(&mut self, timers: &mut Timers<Key>) -> Option<bool> {
        self.group_stopped = false;
        timers.cancel(Key::StreamEnd);
        if self.status.streaming() {
            return None;
        }
        self.status.streaming.store(true, Ordering::Relaxed);
        Some(true)
    }

    pub fn end(&mut self, timers: &mut Timers<Key>, hold: bool) -> Option<bool> {
        if !self.status.streaming() || timers.is_set(Key::StreamEnd) {
            return None;
        }
        if self.group_stopped || !hold {
            return self.finish(timers);
        }
        timers.set(Key::StreamEnd, STREAM_END_HOLD_MS);
        None
    }

    pub fn group(&mut self, playing: bool, timers: &mut Timers<Key>) -> Option<bool> {
        self.group_stopped = !playing;
        if self.group_stopped && timers.is_set(Key::StreamEnd) {
            return self.finish(timers);
        }
        None
    }

    pub fn finish(&mut self, timers: &mut Timers<Key>) -> Option<bool> {
        timers.cancel(Key::StreamEnd);
        if !self.status.streaming() {
            return None;
        }
        info!("sendspin: stream end");
        self.status.streaming.store(false, Ordering::Relaxed);
        Some(false)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::{Instant, advance, timeout};

    use super::*;

    struct Rig {
        timers: Timers<Key>,
        hold: StreamHold,
        status: Arc<Status>,
    }

    impl Rig {
        fn new() -> Self {
            let status = Status::new();
            Self {
                timers: Timers::new(),
                hold: StreamHold::new(Arc::clone(&status)),
                status,
            }
        }

        fn streaming(&self) -> bool {
            self.status.streaming()
        }

        async fn fire(&mut self, within: Duration) -> Option<Option<bool>> {
            match timeout(within, self.timers.next()).await {
                Ok(Key::StreamEnd) => Some(self.hold.finish(&mut self.timers)),
                Ok(Key::Tick) => unreachable!(),
                Err(_) => None,
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn end_without_stopped_is_held_one_second() {
        let mut r = Rig::new();
        assert_eq!(r.hold.start(&mut r.timers), Some(true));
        assert!(r.streaming());
        let t0 = Instant::now();
        assert_eq!(r.hold.end(&mut r.timers, true), None);
        assert!(r.streaming());
        assert_eq!(r.fire(Duration::from_secs(5)).await, Some(Some(false)));
        assert_eq!(t0.elapsed(), Duration::from_millis(STREAM_END_HOLD_MS));
        assert!(!r.streaming());
        assert_eq!(r.hold.end(&mut r.timers, true), None);
        assert_eq!(r.fire(Duration::from_secs(5)).await, None);
    }

    #[tokio::test(start_paused = true)]
    async fn restart_within_10_ms_keeps_streaming() {
        let mut r = Rig::new();
        assert_eq!(r.hold.start(&mut r.timers), Some(true));
        assert_eq!(r.hold.end(&mut r.timers, true), None);
        advance(Duration::from_millis(10)).await;
        assert_eq!(r.hold.start(&mut r.timers), None);
        assert!(r.streaming());
        assert_eq!(r.fire(Duration::from_secs(5)).await, None);
        assert!(r.streaming());
        let t0 = Instant::now();
        assert_eq!(r.hold.end(&mut r.timers, true), None);
        assert_eq!(r.fire(Duration::from_secs(5)).await, Some(Some(false)));
        assert_eq!(t0.elapsed(), Duration::from_millis(STREAM_END_HOLD_MS));
    }

    #[tokio::test(start_paused = true)]
    async fn stopped_before_end_ends_at_once() {
        let mut r = Rig::new();
        assert_eq!(r.hold.start(&mut r.timers), Some(true));
        assert_eq!(r.hold.group(true, &mut r.timers), None);
        assert_eq!(r.hold.group(false, &mut r.timers), None);
        assert!(r.streaming());
        assert_eq!(r.hold.end(&mut r.timers, true), Some(false));
        assert!(!r.streaming());
        assert_eq!(r.fire(Duration::from_secs(5)).await, None);
    }

    #[tokio::test(start_paused = true)]
    async fn stopped_after_end_finishes_the_hold() {
        let mut r = Rig::new();
        assert_eq!(r.hold.start(&mut r.timers), Some(true));
        assert_eq!(r.hold.end(&mut r.timers, true), None);
        advance(Duration::from_millis(300)).await;
        assert!(r.streaming());
        assert_eq!(r.hold.group(false, &mut r.timers), Some(false));
        assert!(!r.streaming());
        assert_eq!(r.fire(Duration::from_secs(5)).await, None);
    }

    #[tokio::test(start_paused = true)]
    async fn new_stream_clears_stopped() {
        let mut r = Rig::new();
        assert_eq!(r.hold.group(false, &mut r.timers), None);
        assert_eq!(r.hold.start(&mut r.timers), Some(true));
        assert_eq!(r.hold.end(&mut r.timers, true), None);
        assert!(r.streaming());
        assert_eq!(r.fire(Duration::from_secs(5)).await, Some(Some(false)));
    }

    #[tokio::test(start_paused = true)]
    async fn disconnect_ends_a_held_stream_at_once() {
        let mut r = Rig::new();
        assert_eq!(r.hold.start(&mut r.timers), Some(true));
        assert_eq!(r.hold.end(&mut r.timers, true), None);
        advance(Duration::from_millis(100)).await;
        assert_eq!(r.hold.finish(&mut r.timers), Some(false));
        assert!(!r.streaming());
        assert_eq!(r.fire(Duration::from_secs(5)).await, None);
        assert_eq!(r.hold.start(&mut r.timers), Some(true));
        assert_eq!(r.hold.finish(&mut r.timers), Some(false));
        assert_eq!(r.hold.finish(&mut r.timers), None);
    }

    #[tokio::test(start_paused = true)]
    async fn teardown_does_not_hold() {
        let mut r = Rig::new();
        assert_eq!(r.hold.start(&mut r.timers), Some(true));
        assert_eq!(r.hold.end(&mut r.timers, false), Some(false));
        assert!(!r.streaming());
    }
}
