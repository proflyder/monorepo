//! Passive `/dev/input/eventN` reader and the short-press/hold state machine.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;

use crate::gateway::{Gateway, Timings};
use crate::logf;

const EV_KEY: u16 = 1;
const KEY_MUTE: u16 = 113;

/// 32-bit armhf `struct input_event`: timeval(i32,i32), type u16, code u16, value i32.
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
    Down,
    Up {
        duration: Duration,
        hold_fired: bool,
    },
    Hold,
    Decision(usize),
}

/// Pure press grouping; all timers are deadlines so tests can advance the clock
/// without sleeping.
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
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    /// Feed an `input_event.value` byte, exactly like the Go loop which only
    /// looks at `value` and ignores type/code.
    pub fn on_value(&mut self, value: i32, now: Instant) -> Option<ButtonEvent> {
        match value {
            1 => {
                self.press_at = Some(now);
                self.hold_fired = false;
                self.decision_at = None;
                self.hold_at = Some(now + self.timings.hold_window);
                Some(ButtonEvent::Down)
            }
            0 => {
                let press_at = self.press_at?;
                let duration = now.saturating_duration_since(press_at);
                self.press_at = None;
                self.hold_at = None;
                let hold_fired = std::mem::replace(&mut self.hold_fired, false);
                if hold_fired {
                    self.presses.clear();
                    self.decision_at = None;
                    return Some(ButtonEvent::Up {
                        duration,
                        hold_fired: true,
                    });
                }
                if duration > self.timings.press_max {
                    self.presses.clear();
                    self.decision_at = None;
                    return Some(ButtonEvent::Up {
                        duration,
                        hold_fired: false,
                    });
                }
                self.presses.push(now);
                let cutoff = now.checked_sub(self.timings.press_group).unwrap_or(now);
                self.presses.retain(|pressed| *pressed > cutoff);
                self.decision_at = Some(now + self.timings.press_group);
                Some(ButtonEvent::Up {
                    duration,
                    hold_fired: false,
                })
            }
            _ => None,
        }
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
        if let Some(decision_at) = self.decision_at
            && now >= decision_at
        {
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
        Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
        None => std::future::pending::<()>().await,
    }
}

impl Gateway {
    pub async fn run_button(self: Arc<Self>, path: &Path) {
        enum Outcome {
            Timer,
            Read(std::io::Result<usize>),
        }
        let mut file = match tokio::fs::File::open(path).await {
            Ok(file) => file,
            Err(err) => {
                logf!(self.logger, "button: {err} (button disabled)");
                return;
            }
        };
        logf!(
            self.logger,
            "button monitor: {} (passive, no EVIOCGRAB)",
            path.display()
        );

        let mut tracker = ButtonTracker::new(self.timings.clone());
        let mut buffer = [0u8; 16];
        loop {
            let deadline = tracker.next_deadline();
            let outcome = tokio::select! {
                biased;
                () = sleep_until(deadline) => Outcome::Timer,
                result = file.read(&mut buffer) => Outcome::Read(result),
            };

            match outcome {
                Outcome::Timer => {
                    if let Some(timer_event) = tracker.on_timer(Instant::now()) {
                        self.handle_button_event(timer_event).await;
                    }
                }
                Outcome::Read(Ok(16)) => {
                    if let Some(event) = parse_input_event(&buffer) {
                        match tracker.on_value(event.value, Instant::now()) {
                            Some(ButtonEvent::Down) => {
                                logf!(
                                    self.logger,
                                    "button: KEY_MUTE down (type {} code {})",
                                    event.event_type,
                                    event.code
                                );
                            }
                            Some(ButtonEvent::Up {
                                duration,
                                hold_fired: _,
                            }) => {
                                logf!(self.logger, "button: KEY_MUTE up ({duration:?})");
                            }
                            _ => {}
                        }
                    }
                }
                Outcome::Read(Ok(0)) => {
                    logf!(self.logger, "button read: end of file");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Outcome::Read(Ok(_)) => {}
                Outcome::Read(Err(err)) => {
                    logf!(self.logger, "button read: {err}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    async fn handle_button_event(self: &Arc<Self>, event: ButtonEvent) {
        match event {
            ButtonEvent::Hold => {
                logf!(self.logger, "button: hold 5s -> open Wi-Fi setup window");
                self.open_manual().await;
            }
            ButtonEvent::Decision(count) => match count {
                1 => logf!(
                    self.logger,
                    "button: 1 short press (hardware mute is handled by Pulsar)"
                ),
                3 => {
                    logf!(
                        self.logger,
                        "button: 3 short presses -> toggle Wi-Fi gateway"
                    );
                    self.toggle_manual().await;
                }
                4 => {
                    logf!(self.logger, "button: 4 short presses -> toggle Bluetooth");
                    self.toggle_bluetooth().await;
                }
                other => logf!(self.logger, "button: {other} short presses (ignored)"),
            },
            _ => {}
        }
    }
}

// KEY_MUTE is exported for the tests that synthesise events.
pub const KEY_MUTE_CODE: u16 = KEY_MUTE;
pub const KEY_EVENT: u16 = EV_KEY;

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

    #[test]
    fn parse_input_event_layout() {
        let buffer = event_bytes(123, 456, 1, 113, -1);
        let event = parse_input_event(&buffer).expect("event");
        assert_eq!(
            event,
            InputEvent {
                sec: 123,
                usec: 456,
                event_type: 1,
                code: 113,
                value: -1,
            }
        );
        assert!(parse_input_event(&buffer[..15]).is_none());
    }

    fn tap(tracker: &mut ButtonTracker, at: Instant) {
        assert_eq!(tracker.on_value(1, at), Some(ButtonEvent::Down));
        assert!(matches!(
            tracker.on_value(0, at + Duration::from_millis(50)),
            Some(ButtonEvent::Up { .. })
        ));
    }

    #[test]
    fn one_three_and_four_presses_group() {
        let timings = Timings::default();
        let base = Instant::now();

        let mut one = ButtonTracker::new(timings.clone());
        tap(&mut one, base);
        let deadline = one.next_deadline().expect("decision deadline");
        assert_eq!(one.on_timer(deadline), Some(ButtonEvent::Decision(1)));

        let mut three = ButtonTracker::new(timings.clone());
        tap(&mut three, base);
        tap(&mut three, base + Duration::from_millis(200));
        tap(&mut three, base + Duration::from_millis(400));
        let deadline = three.next_deadline().expect("decision deadline");
        assert_eq!(three.on_timer(deadline), Some(ButtonEvent::Decision(3)));

        // The third press of a four-press gesture must not decide early.
        let mut four = ButtonTracker::new(timings);
        tap(&mut four, base);
        tap(&mut four, base + Duration::from_millis(200));
        tap(&mut four, base + Duration::from_millis(400));
        let early = four.next_deadline().expect("decision deadline");
        tap(&mut four, base + Duration::from_millis(600));
        assert_eq!(four.on_timer(early), None);
        let deadline = four.next_deadline().expect("decision deadline");
        assert_eq!(four.on_timer(deadline), Some(ButtonEvent::Decision(4)));
    }

    #[test]
    fn hold_five_seconds_opens_window_once() {
        let timings = Timings::default();
        let base = Instant::now();
        let mut tracker = ButtonTracker::new(timings.clone());
        assert_eq!(tracker.on_value(1, base), Some(ButtonEvent::Down));
        assert_eq!(tracker.next_deadline(), Some(base + timings.hold_window));
        assert_eq!(
            tracker.on_timer(base + timings.hold_window),
            Some(ButtonEvent::Hold)
        );
        assert_eq!(tracker.on_timer(base + timings.hold_window), None);
        assert!(matches!(
            tracker.on_value(0, base + Duration::from_secs(6)),
            Some(ButtonEvent::Up {
                hold_fired: true,
                ..
            })
        ));
        assert_eq!(tracker.on_timer(base + Duration::from_secs(10)), None);
    }

    #[test]
    fn long_press_clears_the_group() {
        let timings = Timings::default();
        let base = Instant::now();
        let mut tracker = ButtonTracker::new(timings.clone());
        tap(&mut tracker, base);
        tracker.on_value(1, base + Duration::from_millis(200));
        assert!(matches!(
            tracker.on_value(0, base + Duration::from_millis(800)),
            Some(ButtonEvent::Up { duration, hold_fired: false })
                if duration > timings.press_max
        ));
        assert_eq!(tracker.next_deadline(), None);
    }

    #[test]
    fn old_presses_expire() {
        let timings = Timings::default();
        let base = Instant::now();
        let mut tracker = ButtonTracker::new(timings);
        tap(&mut tracker, base);
        // Two seconds later the earlier tap is outside the 1.5 s group.
        tap(&mut tracker, base + Duration::from_secs(2));
        let deadline = tracker.next_deadline().expect("decision deadline");
        assert_eq!(tracker.on_timer(deadline), Some(ButtonEvent::Decision(1)));
    }
}
