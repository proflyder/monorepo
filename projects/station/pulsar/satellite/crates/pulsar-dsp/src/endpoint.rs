use crate::level::LevelDb;

pub const CHUNK_MS: u32 = 32;
pub const ONSET_CHUNKS: u32 = 5;
pub const TRAILING_SILENCE_CHUNKS: u32 = 31;
pub const MIN_PRIOR_RANGE_DB: f32 = 3.0;
pub const ONSET_FRACTION: f32 = 0.5;
pub const RELEASE_FRACTION: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EndpointPrior {
    pub noise_db: f32,
    pub speech_db: f32,
}

impl EndpointPrior {
    pub fn thresholds(&self) -> Option<(f32, f32)> {
        if !self.noise_db.is_finite() || !self.speech_db.is_finite() {
            return None;
        }
        let range = self.speech_db - self.noise_db;
        if range < MIN_PRIOR_RANGE_DB {
            return None;
        }
        Some((
            self.noise_db + ONSET_FRACTION * range,
            self.noise_db + RELEASE_FRACTION * range,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Continue,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Waiting,
    Speech,
    Ended,
}

pub struct EndpointDetector {
    prior: EndpointPrior,
    on_db: f32,
    off_db: f32,
    state: State,
    above: u32,
    live_run: u32,
    trailing: u32,
    saw_post_wake_speech: bool,
}

impl EndpointDetector {
    pub fn new(prior: EndpointPrior) -> Option<Self> {
        let (on_db, off_db) = prior.thresholds()?;
        Some(Self {
            prior,
            on_db,
            off_db,
            state: State::Waiting,
            above: 0,
            live_run: 0,
            trailing: 0,
            saw_post_wake_speech: false,
        })
    }

    pub fn prior(&self) -> EndpointPrior {
        self.prior
    }

    pub fn thresholds(&self) -> (f32, f32) {
        (self.on_db, self.off_db)
    }

    pub fn reopen(&mut self) {
        if self.state == State::Ended {
            self.state = State::Speech;
            self.above = 0;
            self.trailing = 0;
            self.live_run = ONSET_CHUNKS;
        }
    }

    fn live_evidence(&mut self, level_db: f32, post_wake: bool) {
        if !post_wake {
            return;
        }
        if level_db >= self.on_db {
            self.live_run += 1;
            if self.live_run >= ONSET_CHUNKS {
                self.saw_post_wake_speech = true;
            }
        } else {
            self.live_run = 0;
        }
    }

    pub fn push(&mut self, level_db: f32, post_wake: bool) -> Step {
        if self.state != State::Ended {
            self.live_evidence(level_db, post_wake);
        }
        match self.state {
            State::Waiting => {
                if level_db >= self.on_db {
                    self.above += 1;
                } else {
                    self.above = 0;
                }
                if self.above >= ONSET_CHUNKS {
                    self.state = State::Speech;
                    self.above = 0;
                    self.trailing = 0;
                }
                Step::Continue
            }
            State::Speech => {
                if level_db <= self.off_db {
                    self.trailing += 1;
                } else {
                    self.trailing = 0;
                }
                if self.trailing < TRAILING_SILENCE_CHUNKS {
                    return Step::Continue;
                }
                self.trailing = 0;
                if self.saw_post_wake_speech {
                    self.state = State::Ended;
                    Step::End
                } else {
                    self.state = State::Waiting;
                    self.above = 0;
                    self.live_run = 0;
                    Step::Continue
                }
            }
            State::Ended => Step::Continue,
        }
    }
}

pub struct EndpointRun {
    detector: EndpointDetector,
    pending: bool,
    sent: bool,
}

impl EndpointRun {
    pub fn new(prior: EndpointPrior, preroll: &[i16], live: &[i16]) -> Option<Self> {
        let detector = EndpointDetector::new(prior)?;
        let mut run = Self {
            detector,
            pending: false,
            sent: false,
        };
        for chunk in preroll.chunks(512) {
            run.push(pcm_level_db(chunk), false);
        }
        for chunk in live.chunks(512) {
            run.push(pcm_level_db(chunk), true);
        }
        Some(run)
    }

    pub fn push(&mut self, level_db: f32, post_wake: bool) {
        match self.detector.push(level_db, post_wake) {
            Step::End => {
                if !self.sent {
                    self.pending = true;
                }
            }
            Step::Continue => {
                if self.pending && level_db >= self.detector.thresholds().1 {
                    self.pending = false;
                    self.detector.reopen();
                }
            }
        }
    }

    pub fn has_pending(&self) -> bool {
        self.pending
    }

    pub fn take_end(&mut self) -> bool {
        if !std::mem::take(&mut self.pending) || self.sent {
            return false;
        }
        self.sent = true;
        true
    }

    pub fn cancel(&mut self) {
        self.pending = false;
        self.sent = true;
    }

    pub fn prior(&self) -> EndpointPrior {
        self.detector.prior()
    }

    pub fn thresholds(&self) -> (f32, f32) {
        self.detector.thresholds()
    }
}

pub fn pcm_level_db(pcm: &[i16]) -> f32 {
    LevelDb::from_pcm(pcm).as_f32()
}

pub fn mean_square_db(ms: f32) -> f32 {
    LevelDb::from_mean_square(ms).as_f32()
}

pub fn percentile_db(window: &[f32], q: f32) -> f32 {
    crate::level::percentile_db(window, q).as_f32()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prior() -> EndpointPrior {
        EndpointPrior {
            noise_db: -55.0,
            speech_db: -25.0,
        }
    }

    fn push(d: &mut EndpointDetector, level: f32, n: usize, post_wake: bool) -> Vec<Step> {
        (0..n).map(|_| d.push(level, post_wake)).collect()
    }

    #[test]
    fn thresholds_are_relative_to_the_wake_word() {
        let (on, off) = prior().thresholds().unwrap();
        assert_eq!(on, -40.0);
        assert_eq!(off, -47.5);
        assert!(
            EndpointDetector::new(EndpointPrior {
                noise_db: -50.0,
                speech_db: -48.0,
            })
            .is_none()
        );
    }

    #[test]
    fn waiting_silence_never_ends() {
        let mut d = EndpointDetector::new(prior()).unwrap();
        assert!(
            push(&mut d, -60.0, 1000, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
    }

    #[test]
    fn speech_then_trailing_silence_ends_once() {
        let mut d = EndpointDetector::new(prior()).unwrap();
        assert!(
            push(&mut d, -30.0, 13, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
        assert!(
            push(&mut d, -60.0, 30, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
        assert_eq!(d.push(-60.0, true), Step::End);
        assert!(
            push(&mut d, -60.0, 100, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
        assert!(
            push(&mut d, -30.0, 100, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
    }

    #[test]
    fn ordinary_pauses_do_not_end() {
        let mut d = EndpointDetector::new(prior()).unwrap();
        push(&mut d, -30.0, 8, true);
        assert!(
            push(&mut d, -60.0, 20, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
        push(&mut d, -30.0, 5, true);
        assert!(
            push(&mut d, -60.0, 30, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
        assert_eq!(d.push(-60.0, true), Step::End);
    }

    #[test]
    fn wake_tail_in_preroll_does_not_end_the_wait() {
        let mut d = EndpointDetector::new(prior()).unwrap();
        push(&mut d, -30.0, 12, false);
        assert!(
            push(&mut d, -60.0, 500, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
    }

    #[test]
    fn command_continuous_from_preroll_ends() {
        let mut d = EndpointDetector::new(prior()).unwrap();
        push(&mut d, -30.0, 6, false);
        push(&mut d, -30.0, 5, true);
        assert!(
            push(&mut d, -60.0, 30, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
        assert_eq!(d.push(-60.0, true), Step::End);
    }

    #[test]
    fn exactly_five_live_chunks_qualify_when_speech_starts_in_live_audio() {
        let mut d = EndpointDetector::new(prior()).unwrap();
        assert!(
            push(&mut d, -30.0, 5, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
        assert!(
            push(&mut d, -60.0, 30, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
        assert_eq!(d.push(-60.0, true), Step::End);
    }

    #[test]
    fn four_live_chunks_do_not_qualify() {
        let mut d = EndpointDetector::new(prior()).unwrap();
        push(&mut d, -30.0, 4, true);
        assert!(
            push(&mut d, -60.0, 100, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
    }

    #[test]
    fn preroll_arms_speech_but_does_not_count_as_live_evidence() {
        let mut d = EndpointDetector::new(prior()).unwrap();
        push(&mut d, -30.0, 5, false);
        push(&mut d, -30.0, 2, true);
        push(&mut d, -30.0, 3, true);
        assert!(
            push(&mut d, -60.0, 30, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
        assert_eq!(d.push(-60.0, true), Step::End);

        let mut d = EndpointDetector::new(prior()).unwrap();
        push(&mut d, -30.0, 5, false);
        push(&mut d, -30.0, 4, true);
        assert!(
            push(&mut d, -60.0, 100, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
    }

    #[test]
    fn short_live_wake_tail_does_not_count_as_a_command() {
        let mut d = EndpointDetector::new(prior()).unwrap();
        push(&mut d, -30.0, 12, false);
        push(&mut d, -30.0, 4, true);
        assert!(
            push(&mut d, -60.0, 500, true)
                .iter()
                .all(|s| *s == Step::Continue)
        );
    }

    #[test]
    fn pcm_speech_and_silence_end_the_phrase() {
        let speech: Vec<i16> = vec![3000; 512];
        let silence: Vec<i16> = vec![20; 512];
        let n = pcm_level_db(&speech);
        let f = pcm_level_db(&silence);
        assert!(n > f + MIN_PRIOR_RANGE_DB, "{n} {f}");
        let mut d = EndpointDetector::new(EndpointPrior {
            noise_db: f,
            speech_db: n,
        })
        .unwrap();
        for _ in 0..13 {
            assert_eq!(d.push(pcm_level_db(&speech), true), Step::Continue);
        }
        let mut end = 0;
        for _ in 0..40 {
            if d.push(pcm_level_db(&silence), true) == Step::End {
                end += 1;
            }
        }
        assert_eq!(end, 1);
    }

    fn run_with_phrase() -> EndpointRun {
        let mut r = EndpointRun::new(prior(), &[], &[]).unwrap();
        for _ in 0..13 {
            r.push(-30.0, true);
        }
        for _ in 0..31 {
            r.push(-60.0, true);
        }
        r
    }

    #[test]
    fn one_graceful_end_per_dialog() {
        let mut r = run_with_phrase();
        assert!(r.take_end());
        assert!(!r.take_end());
        for _ in 0..50 {
            r.push(-60.0, true);
            assert!(!r.take_end());
        }
    }

    #[test]
    fn ha_finishing_first_cancels_the_detector() {
        let mut r = EndpointRun::new(prior(), &[], &[]).unwrap();
        for _ in 0..13 {
            r.push(-30.0, true);
        }
        for _ in 0..31 {
            r.push(-60.0, true);
        }
        r.cancel();
        assert!(!r.take_end());
    }

    #[test]
    fn cancel_and_repeated_wake_cannot_send_the_old_end() {
        let mut r = EndpointRun::new(prior(), &[], &[]).unwrap();
        for _ in 0..13 {
            r.push(-30.0, true);
        }
        for _ in 0..31 {
            r.push(-60.0, true);
        }
        assert!(r.has_pending());
        r.cancel();
        assert!(!r.take_end());
        let mut next = EndpointRun::new(prior(), &[], &[]).unwrap();
        assert!(!next.take_end());
    }

    #[test]
    fn speech_resuming_during_the_earcon_drops_the_deferred_end_but_can_end_later() {
        let mut r = EndpointRun::new(prior(), &[], &[]).unwrap();
        for _ in 0..13 {
            r.push(-30.0, true);
        }
        for _ in 0..31 {
            r.push(-60.0, true);
        }
        assert!(r.has_pending());
        r.push(-30.0, true);
        assert!(!r.has_pending());
        assert!(!r.take_end());
        for _ in 0..31 {
            r.push(-60.0, true);
        }
        assert!(r.take_end());
    }

    #[test]
    fn end_found_in_the_arbitration_backlog_is_not_lost() {
        let mut live = vec![3000i16; 13 * 512];
        live.extend(std::iter::repeat_n(20i16, 31 * 512));
        let speech = pcm_level_db(&live[..512]);
        let floor = pcm_level_db(&live[13 * 512..14 * 512]);
        let prior = EndpointPrior {
            noise_db: floor,
            speech_db: speech,
        };
        let mut r = EndpointRun::new(prior, &[], &live).unwrap();
        assert!(r.has_pending());
        assert!(r.take_end());
    }

    #[test]
    fn quiet_part_of_the_window_is_the_percentile() {
        let mut w = vec![1e-6; 40];
        w.extend_from_slice(&[1e-2; 7]);
        assert!((percentile_db(&w, 0.10) - (-60.0)).abs() < 0.1);
        assert!((percentile_db(&w, 0.90) - (-20.0)).abs() < 0.1);
    }
}
