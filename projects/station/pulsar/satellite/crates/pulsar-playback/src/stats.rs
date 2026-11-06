//! Output statistics of a Sendspin-style writer: frames written, the delay the device reported,
//! xruns and the net rate against wall-clock time.

use std::fmt::Write as _;

const REPORT_US: i64 = 10_000_000;

/// Counters of one open device; reset when the device is opened.
#[derive(Clone, Copy, Debug)]
pub struct Stats {
    rate: u32,
    frames_total: u64,
    pub xruns: u32,
    last_delay_frames: i64,
    delay_min: i64,
    delay_max: i64,
    first_write_us: i64,
    prev_report_us: i64,
    prev_played: i64,
    report_us: i64,
}

impl Stats {
    pub fn new() -> Self {
        Self {
            rate: 0,
            frames_total: 0,
            xruns: 0,
            last_delay_frames: 0,
            delay_min: i64::from(i32::MAX),
            delay_max: 0,
            first_write_us: 0,
            prev_report_us: 0,
            prev_played: 0,
            report_us: 0,
        }
    }

    /// A new device: the counters describe the current open only.
    pub fn reset(&mut self, rate: u32, now_us: i64) {
        // The delay of the device just closed is still the best estimate of what it holds; a
        // reopen keeps it, as before.
        let last_delay = self.last_delay_frames;
        *self = Self::new();
        self.last_delay_frames = last_delay;
        self.rate = rate;
        self.report_us = now_us;
    }

    pub fn frames_total(&self) -> u64 {
        self.frames_total
    }

    pub fn add_frames(&mut self, frames: usize) {
        self.frames_total = self.frames_total.saturating_add(frames as u64);
    }

    /// Records the device's delay reading and the first write time.
    pub fn record_delay(&mut self, delay: i64, now_us: i64) {
        if delay < 0 {
            return;
        }
        self.last_delay_frames = delay;
        self.delay_min = self.delay_min.min(delay);
        self.delay_max = self.delay_max.max(delay);
        if self.first_write_us == 0 {
            self.first_write_us = now_us;
        }
    }

    /// The text of a periodic report, or `None` before the first write (or without a rate).
    pub fn maybe_report(&mut self, now_us: i64, channels: u8, gain: f32) -> Option<String> {
        if now_us - self.report_us < REPORT_US {
            return None;
        }
        self.report_us = now_us;
        if self.rate == 0 || self.first_write_us == 0 {
            return None;
        }
        let rate = f64::from(self.rate);
        let played = self.frames_total as i64 - self.last_delay_frames;
        let interval = self.prev_report_us != 0;
        let wall = (now_us
            - if interval {
                self.prev_report_us
            } else {
                self.first_write_us
            }) as f64
            / 1e6;
        let dplayed = (played - if interval { self.prev_played } else { 0 }) as f64 / rate;
        let ppm = if wall > 0.5 {
            (dplayed / wall - 1.0) * 1e6
        } else {
            0.0
        };
        let mut s = String::new();
        let _ = write!(
            s,
            "{} Hz/{} ch: {:.1} s total, last {wall:.1} s: {dplayed:.3} s played ({ppm:+.0} ppm), output delay {:.1} ms (min {:.1}, max {:.1}), xruns {}, duck gain {:.2}",
            self.rate,
            channels,
            played as f64 / rate,
            self.last_delay_frames as f64 * 1000.0 / rate,
            self.delay_min as f64 * 1000.0 / rate,
            self.delay_max as f64 * 1000.0 / rate,
            self.xruns,
            gain
        );
        self.prev_report_us = now_us;
        self.prev_played = played;
        self.delay_min = i64::from(i32::MAX);
        self.delay_max = 0;
        Some(s)
    }
}

impl Default for Stats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_waits_for_a_write_and_the_period() {
        let mut s = Stats::new();
        assert!(s.maybe_report(1_000_000, 2, 1.0).is_none());
        s.reset(48_000, 0);
        s.add_frames(480);
        s.record_delay(480, 1);
        let text = s.maybe_report(10_000_000, 2, 1.0).unwrap();
        assert_eq!(
            text,
            "48000 Hz/2 ch: 0.0 s total, last 10.0 s: 0.000 s played (-1000000 ppm), output delay 10.0 ms (min 10.0, max 10.0), xruns 0, duck gain 1.00"
        );
        // The min/max window restarts after a report and prints i32::MAX as before.
        let text = s.maybe_report(20_000_000, 2, 1.0).unwrap();
        assert!(text.contains("min 44739242.6"), "{text}");
    }

    #[test]
    fn maybe_report_respects_the_period() {
        let mut s = Stats::new();
        s.reset(48_000, 0);
        s.add_frames(48_000);
        s.record_delay(0, 1);
        assert!(s.maybe_report(5_000_000, 2, 1.0).is_none());
        assert_eq!(s.report_us, 0);
        assert!(s.maybe_report(10_000_000, 2, 1.0).is_some());
        assert_eq!(s.report_us, 10_000_000);
    }
}
