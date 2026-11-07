//! HA-таймеры, показ на часах и звонок (controller.cpp "timers", ring).

use std::collections::BTreeMap;

use pulsar_core::now_ms;
use pulsar_esphome::{TimerEvent, TimerInfo};
use tracing::info;

use super::earcons;
use super::logic::{self, HaTimer, Phase};
use super::ports::Display;
use super::{Controller, Pending, Tk};

/// A ringing timer stops by itself after this long (C++ TIMER_RING_MAX_MS).
pub const TIMER_RING_MAX_MS: i64 = 15 * 60 * 1000;

/// Что делать на очередном тике звонка.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingStep {
    /// Потолок 15 мин: звонок заканчивается.
    Stop,
    /// Играет ответ/объявление: звонок уступает канал и продолжится после.
    Yield,
    /// Играть очередной круг.
    Play,
}

/// The ring: one alarm sound every 300 ms, giving the ANNOUNCE channel to speech and resuming
/// after it.
#[derive(Debug, Default)]
pub struct Ring {
    ringing: bool,
    yielded: bool,
    started_ms: i64,
}

impl Ring {
    pub fn ringing(&self) -> bool {
        self.ringing
    }

    pub fn start(&mut self, now: i64) {
        self.ringing = true;
        self.yielded = false;
        self.started_ms = now;
    }

    pub fn stop(&mut self) {
        self.ringing = false;
        self.yielded = false;
    }

    /// The C++ `Controller::ring_timer` decision. None: not ringing.
    pub fn next(&mut self, now: i64, busy: bool) -> Option<RingStep> {
        if !self.ringing {
            return None;
        }
        if now - self.started_ms > TIMER_RING_MAX_MS {
            return Some(RingStep::Stop);
        }
        if busy {
            self.yielded = true;
            return Some(RingStep::Yield);
        }
        Some(RingStep::Play)
    }

    /// The ring yielded to speech and has to continue now; false when it did not yield.
    pub fn take_yield(&mut self) -> bool {
        std::mem::take(&mut self.yielded)
    }
}

/// Что случилось при разборе события HA-таймера.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimerEffects {
    /// A new timer appeared (the C++ hook showed a ring animation).
    pub started: bool,
    /// A timer finished: start ringing unless one already rings.
    pub finished: bool,
}

/// HA timers by id, the one shown on the clock, and the ring.
#[derive(Debug, Default)]
pub struct HaTimers {
    timers: BTreeMap<String, HaTimer>,
    /// Id on the clock, "" = none.
    shown_timer: String,
    /// Its data when it was put there.
    shown: HaTimer,
    ring: Ring,
}

impl HaTimers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ringing(&self) -> bool {
        self.ring.ringing()
    }

    /// Applies a HA timer event; the caller reports `effects` to the UI.
    pub fn on_event(&mut self, t: TimerInfo, now: i64) -> TimerEffects {
        let mut effects = TimerEffects::default();
        let data = HaTimer {
            active: t.is_active,
            end_ms: now + i64::from(t.seconds_left) * 1000,
            seconds_left: t.seconds_left,
        };
        match t.event {
            TimerEvent::Started => {
                effects.started = !self.timers.contains_key(&t.id);
                self.timers.insert(t.id, data);
            }
            TimerEvent::Updated => {
                self.timers.insert(t.id, data);
            }
            TimerEvent::Cancelled => {
                self.timers.remove(&t.id);
            }
            TimerEvent::Finished => {
                self.timers.remove(&t.id);
                effects.finished = true;
            }
        }
        effects
    }

    /// The clock counts a running timer down by itself, so it is set only when the choice or the
    /// timer's data changes, and looked at again when the shown one runs out. Returns the tick
    /// to re-arm for a running timer.
    pub fn show(&mut self, display: &Display, now: i64) -> Option<u64> {
        let Some((id, best, left)) = logic::pick_timer(&self.timers, now) else {
            if !self.shown_timer.is_empty() {
                display.clock.set_timer(0, true);
            }
            self.shown_timer.clear();
            return None;
        };
        let same = id == self.shown_timer
            && best.active == self.shown.active
            && if best.active {
                best.end_ms == self.shown.end_ms
            } else {
                best.seconds_left == self.shown.seconds_left
            };
        if !same {
            display.clock.set_timer(left, best.active);
            self.shown_timer = id.to_owned();
            self.shown = best;
        }
        best.active.then_some(left as u64)
    }

    pub fn ring(&mut self, now: i64, busy: bool) -> Option<RingStep> {
        self.ring.next(now, busy)
    }

    pub fn start_ring(&mut self, now: i64) {
        self.ring.start(now);
    }

    pub fn stop_ring(&mut self) {
        self.ring.stop();
    }

    pub fn take_yield(&mut self) -> bool {
        self.ring.take_yield()
    }
}

impl Controller {
    pub(super) fn on_timer_event(&mut self, t: TimerInfo) {
        info!(
            "timer {} '{}' ({}) left={} active={}",
            t.event as u32,
            t.name,
            t.id,
            t.seconds_left,
            u8::from(t.is_active)
        );
        let now = now_ms();
        let effects = self.timers_ha.on_event(t, now);
        if effects.started {
            self.deps.display.leds.show_timer_started();
        }
        if effects.finished && !self.timers_ha.ringing() {
            self.timers_ha.start_ring(now);
            self.update_duck();
            self.update_ui();
            self.ring_timer();
        }
        self.show_timers();
    }

    pub(super) fn show_timers(&mut self) {
        self.timers.cancel(Tk::TimerTick);
        if let Some(ms) = self.timers_ha.show(&self.deps.display, now_ms()) {
            self.timers.set(Tk::TimerTick, ms);
        }
    }

    /// The ring gives the ANNOUNCE channel to speech (TTS answer, HA announcement) and resumes
    /// after it.
    pub(super) fn ring_timer(&mut self) {
        self.timers.cancel(Tk::RingTick);
        let busy = self.announcing || self.phase == Phase::Speaking;
        match self.timers_ha.ring(now_ms(), busy) {
            None | Some(RingStep::Yield) => {}
            Some(RingStep::Stop) => self.stop_timer_ring(),
            Some(RingStep::Play) => {
                let sound = earcons::path(self.model.earcons.timer);
                self.play_announce(vec![sound], Pending::Ring);
            }
        }
    }

    pub(super) fn resume_ring(&mut self) {
        if self.timers_ha.take_yield() {
            self.ring_timer();
        }
    }

    pub(super) fn stop_timer_ring(&mut self) {
        self.timers_ha.stop_ring();
        self.timers.cancel(Tk::RingTick);
        self.interrupt_speech(); // the ring, or the speech it yielded to
        self.update_duck();
        self.update_ui();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::logic::pick_timer;

    fn timer(event: TimerEvent, id: &str, seconds_left: u32, active: bool) -> TimerInfo {
        TimerInfo {
            event,
            id: id.into(),
            name: "tea".into(),
            total_seconds: 60,
            seconds_left,
            is_active: active,
        }
    }

    #[test]
    fn started_updated_cancelled_and_finished() {
        let mut t = HaTimers::new();
        let now = 1_000_000;
        let e = t.on_event(timer(TimerEvent::Started, "a", 10, true), now);
        assert_eq!(
            e,
            TimerEffects {
                started: true,
                finished: false
            }
        );
        assert_eq!(t.timers.get("a").unwrap().end_ms, now + 10_000);
        // A repeated Started for a known timer is not a new one.
        let e = t.on_event(timer(TimerEvent::Started, "a", 9, true), now);
        assert!(!e.started);
        let e = t.on_event(timer(TimerEvent::Finished, "a", 0, false), now);
        assert!(e.finished);
        assert!(t.timers.is_empty());
    }

    #[test]
    fn paused_timer_keeps_its_seconds() {
        let mut t = HaTimers::new();
        t.on_event(timer(TimerEvent::Started, "a", 5, false), 0);
        assert_eq!(pick_timer(&t.timers, 0), Some(("a", t.timers["a"], 5_000)));
    }

    #[test]
    fn ring_plays_yields_and_stops_at_the_ceiling() {
        let mut t = HaTimers::new();
        let t0 = 5_000;
        assert_eq!(t.ring(t0, false), None);
        t.start_ring(t0);
        assert!(t.ringing());
        assert_eq!(t.ring(t0, false), Some(RingStep::Play));
        // An answer or announcement: the ring yields, then continues.
        assert_eq!(t.ring(t0 + 100, true), Some(RingStep::Yield));
        assert!(t.take_yield());
        assert!(!t.take_yield());
        assert_eq!(t.ring(t0 + 200, false), Some(RingStep::Play));
        // The ceiling, strictly after 15 min as the C++ comparison.
        assert_eq!(t.ring(t0 + TIMER_RING_MAX_MS, false), Some(RingStep::Play));
        assert_eq!(
            t.ring(t0 + TIMER_RING_MAX_MS + 1, false),
            Some(RingStep::Stop)
        );
        t.stop_ring();
        assert!(!t.ringing());
        assert_eq!(t.ring(t0, false), None);
    }
}
