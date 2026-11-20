use crate::ricardo::DetectorError;
use crate::wake_events::logic;

use super::capture::CaptureError;

pub(super) trait CaptureSource: Send {
    fn read(&mut self, out: &mut [i16]) -> Result<usize, CaptureError>;
    fn wait(&mut self, ms: i32) -> Result<(), CaptureError>;
    fn recover(&mut self) -> bool;
}

pub(super) trait Detector: Send {
    fn feed(&mut self, pcm: &[i16]) -> Result<Detection, DetectorError>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Detection {
    pub(super) hit: bool,
    pub(super) score: f32,
}

pub(super) trait Clock: Send {
    fn mono_ms(&self) -> i64;
    fn utc_ms(&self) -> i64;
}

pub(super) struct SystemClock;

impl Clock for SystemClock {
    fn mono_ms(&self) -> i64 {
        pulsar_core::now_ms()
    }

    fn utc_ms(&self) -> i64 {
        logic::utc_ms()
    }
}

pub(super) trait WakeStore: Send {
    fn listening(&mut self, chunk_ms: u64, dry_run: bool);
    fn missed(&mut self, utc_ms: i64);
    fn push(
        &mut self,
        chunk: &[i16],
        mean: Option<f32>,
        hit: Option<f32>,
        dry_run: bool,
        now_ms: i64,
        utc_ms: i64,
    ) -> Option<String>;
    fn reset(&mut self);
}
