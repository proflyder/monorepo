//! Passive `/dev/input/eventN` reader and the short-press/hold gesture machine.
//!
//! Events are read without `EVIOCGRAB`, so Pulsar keeps receiving the hardware
//! mute key. Only `EV_KEY`/`KEY_MUTE` transitions reach the tracker: reacting
//! to any `input_event` (brightness, jack detect, ...) could open the setup
//! window by accident.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::time::Instant;
use tracing::{info, warn};

use crate::gateway::{Gateway, Timings};

pub const EV_KEY: u16 = 1;
pub const KEY_MUTE: u16 = 113;
pub const KEY_REPEAT: i32 = 2;

/// 32-bit armhf `struct input_event`: timeval(i32,i32), type u16, code u16,
/// value i32. Parsed explicitly without `unsafe`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEvent {
    pub sec: i32,
    pub usec: i32,
    pub event_type: u16,
    pub code: u16,
    pub value: i32,
}

pub fn parse_input_event(buffer: &[u8]) -> Option<InputEvent> {
    if buffer.len() < 16 {
        return None;
    }
    Some(InputEvent {
        sec: i32::from_le_bytes(buffer[0..4].try_into().ok()?),
        usec: i32::from_le_bytes(buffer[4..8].try_into().ok()?),
        event_type: u16::from_le_bytes(buffer[8..10].try_into().ok()?),
        code: u16::from_le_bytes(buffer[10..12].try_into().ok()?),
        value: i32::from_le_bytes(buffer[12..16].try_into().ok()?),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ButtonEvent {
    Hold,
    Decision(usize),
}

/// Pure press grouping. All timers are deadlines, so tests advance the paused
/// Tokio clock instead of sleeping for production intervals.
pub struct ButtonTracker {
    press_at: Option<Instant>,
    presses: Vec<Instant>,
    hold_fired: bool,
    hold_at: Option<Instant>,
    decision_at: Option<Instant>,
    timings: Timings,
}

impl ButtonTracker {
    pub fn new(timings: Timings) -> Self {
        Self {
            press_at: None,
            presses: Vec::new(),
            hold_fired: false,
            hold_at: None,
            decision_at: None,
            timings,
        }
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        match (self.hold_at, self.decision_at) {
            (Some(first), Some(second)) => Some(first.min(second)),
            (deadline, None) | (None, deadline) => deadline,
        }
    }

    /// Non-key events, other keys and auto-repeat are ignored.
    pub fn on_event(&mut self, event: InputEvent, now: Instant) {
        if event.event_type != EV_KEY || event.code != KEY_MUTE {
            return;
        }
        match event.value {
            1 => {
                self.press_at = Some(now);
                self.hold_fired = false;
                self.decision_at = None;
                self.hold_at = Some(now + self.timings.hold_window);
            }
            0 => self.release(now),
            _ => {}
        }
    }

    fn release(&mut self, now: Instant) {
        let Some(press_at) = self.press_at.take() else {
            return;
        };
        let duration = now.saturating_duration_since(press_at);
        self.hold_at = None;
        let hold_fired = std::mem::replace(&mut self.hold_fired, false);
        if hold_fired || duration > self.timings.press_max {
            self.presses.clear();
            self.decision_at = None;
            return;
        }
        self.presses.push(now);
        let cutoff = now.checked_sub(self.timings.press_group).unwrap_or(now);
        self.presses.retain(|pressed| *pressed > cutoff);
        self.decision_at = Some(now + self.timings.press_group);
    }

    pub fn on_timer(&mut self, now: Instant) -> Option<ButtonEvent> {
        if let (Some(hold_at), Some(_)) = (self.hold_at, self.press_at)
            && !self.hold_fired
            && now >= hold_at
        {
            self.hold_fired = true;
            self.hold_at = None;
            return Some(ButtonEvent::Hold);
        }
        if self.decision_at.is_some_and(|deadline| now >= deadline) {
            self.decision_at = None;
            let count = self.presses.len();
            self.presses.clear();
            if count > 0 {
                return Some(ButtonEvent::Decision(count));
            }
        }
        None
    }
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

impl Gateway {
    /// Read the button device until shutdown; a missing device disables the
    /// button without stopping the gateway.
    pub async fn run_button(
        self: Arc<Self>,
        path: &Path,
        shutdown: tokio_util::sync::CancellationToken,
    ) {
        let mut file = match tokio::fs::File::open(path).await {
            Ok(file) => file,
            Err(error) => return warn!(%error, path = %path.display(), "button disabled"),
        };
        info!(path = %path.display(), "button monitor started");
        let mut tracker = ButtonTracker::new(self.timings.clone());
        let mut buffer = [0u8; 16];
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => return,
                () = sleep_until(tracker.next_deadline()) => {
                    if let Some(event) = tracker.on_timer(Instant::now()) {
                        self.button_event(event).await;
                    }
                }
                result = file.read(&mut buffer) => match result {
                    Ok(16) => {
                        if let Some(event) = parse_input_event(&buffer) {
                            tracker.on_event(event, Instant::now());
                        }
                    }
                    Ok(0) => {
                        warn!("button read: end of file");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        warn!(%error, "button read");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                },
            }
        }
    }

    async fn button_event(self: &Arc<Self>, event: ButtonEvent) {
        match event {
            ButtonEvent::Hold => {
                info!("button: hold 5s -> open Wi-Fi setup window");
                self.open_manual().await;
            }
            ButtonEvent::Decision(1) => {
                info!("button: 1 short press (mute is handled by Pulsar)");
            }
            ButtonEvent::Decision(3) => {
                info!("button: 3 short presses -> toggle Wi-Fi gateway");
                self.toggle_manual().await;
            }
            ButtonEvent::Decision(4) => {
                info!("button: 4 short presses -> toggle Bluetooth");
                self.toggle_bluetooth().await;
            }
            ButtonEvent::Decision(count) => info!(presses = count, "button: ignored"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event_bytes(sec: u32, usec: u32, event_type: u16, code: u16, value: i32) -> [u8; 16] {
        let mut buffer = [0u8; 16];
        buffer[0..4].copy_from_slice(&sec.to_le_bytes());
        buffer[4..8].copy_from_slice(&usec.to_le_bytes());
        buffer[8..10].copy_from_slice(&event_type.to_le_bytes());
        buffer[10..12].copy_from_slice(&code.to_le_bytes());
        buffer[12..16].copy_from_slice(&value.to_le_bytes());
        buffer
    }

    fn key(value: i32) -> InputEvent {
        InputEvent {
            sec: 0,
            usec: 0,
            event_type: EV_KEY,
            code: KEY_MUTE,
            value,
        }
    }

    fn tap(tracker: &mut ButtonTracker, at: Instant) {
        tracker.on_event(key(1), at);
        tracker.on_event(key(0), at + Duration::from_millis(50));
    }

    #[test]
    fn parse_input_event_layout() {
        let buffer = event_bytes(123, 456, 1, 113, -1);
        assert_eq!(
            parse_input_event(&buffer),
            Some(InputEvent {
                sec: 123,
                usec: 456,
                event_type: 1,
                code: 113,
                value: -1,
            })
        );
        assert!(parse_input_event(&buffer[..15]).is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn unrelated_events_are_ignored() {
        let mut tracker = ButtonTracker::new(Timings::default());
        let now = Instant::now();
        let other_type = InputEvent {
            event_type: 2,
            ..key(1)
        };
        let other_code = InputEvent {
            event_type: EV_KEY,
            code: 115,
            ..key(1)
        };
        tracker.on_event(other_type, now);
        tracker.on_event(other_code, now);
        tracker.on_event(key(KEY_REPEAT), now);
        assert_eq!(tracker.next_deadline(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn one_three_and_four_presses_group() {
        let timings = Timings::default();
        let base = Instant::now();
        let mut one = ButtonTracker::new(timings.clone());
        tap(&mut one, base);
        assert_eq!(
            one.on_timer(one.next_deadline().expect("deadline")),
            Some(ButtonEvent::Decision(1))
        );

        let mut three = ButtonTracker::new(timings.clone());
        for at in [0, 200, 400] {
            tap(&mut three, base + Duration::from_millis(at));
        }
        assert_eq!(
            three.on_timer(three.next_deadline().expect("deadline")),
            Some(ButtonEvent::Decision(3))
        );

        let mut four = ButtonTracker::new(timings);
        for at in [0, 200, 400] {
            tap(&mut four, base + Duration::from_millis(at));
        }
        let early = four.next_deadline().expect("deadline");
        tap(&mut four, base + Duration::from_millis(600));
        assert_eq!(four.on_timer(early), None);
        assert_eq!(
            four.on_timer(four.next_deadline().expect("deadline")),
            Some(ButtonEvent::Decision(4))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn hold_five_seconds_opens_the_window_once() {
        let timings = Timings::default();
        let base = Instant::now();
        let mut tracker = ButtonTracker::new(timings.clone());
        tracker.on_event(key(1), base);
        assert_eq!(tracker.next_deadline(), Some(base + timings.hold_window));
        assert_eq!(
            tracker.on_timer(base + timings.hold_window),
            Some(ButtonEvent::Hold)
        );
        assert_eq!(tracker.on_timer(base + timings.hold_window), None);
        tracker.on_event(key(0), base + Duration::from_secs(6));
        assert_eq!(tracker.on_timer(base + Duration::from_secs(10)), None);
    }

    #[tokio::test(start_paused = true)]
    async fn long_press_clears_the_group() {
        let timings = Timings::default();
        let base = Instant::now();
        let mut tracker = ButtonTracker::new(timings);
        tap(&mut tracker, base);
        tracker.on_event(key(1), base + Duration::from_millis(200));
        tracker.on_event(key(0), base + Duration::from_millis(800));
        assert_eq!(tracker.next_deadline(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn old_presses_expire() {
        let mut tracker = ButtonTracker::new(Timings::default());
        let base = Instant::now();
        tap(&mut tracker, base);
        tap(&mut tracker, base + Duration::from_secs(2));
        assert_eq!(
            tracker.on_timer(tracker.next_deadline().expect("deadline")),
            Some(ButtonEvent::Decision(1))
        );
    }
}
