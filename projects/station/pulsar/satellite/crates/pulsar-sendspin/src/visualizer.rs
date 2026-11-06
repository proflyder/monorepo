//! Accumulates the server's visualizer stream (loudness and beat on the drain thread, spectrum
//! frames too) into the `MusicFrame` the LEDs and the clock consume.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use pulsar_core::MusicFrame;

use crate::music;

#[derive(Default)]
pub struct Visualizer {
    loudness: AtomicU32,
    beat: AtomicBool,
}

impl Visualizer {
    pub fn loudness(&self, v: u16) {
        self.loudness
            .store(music::loudness(v).to_bits(), Ordering::Relaxed);
    }

    pub fn beat(&self) {
        self.beat.store(true, Ordering::Relaxed);
    }

    /// One frame from the server's spectrum bins; `None` without bins.
    pub fn frame(&self, bins: &[u16]) -> Option<MusicFrame> {
        let bands = music::bands(bins)?;
        Some(MusicFrame {
            bands,
            loudness: f32::from_bits(self.loudness.load(Ordering::Relaxed)),
            beat: self.beat.swap(false, Ordering::Relaxed),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_carries_loudness_and_consumes_the_beat() {
        let v = Visualizer::default();
        v.loudness(65535);
        v.beat();
        let f = v.frame(&[0, 65535]).unwrap();
        assert_eq!(f.loudness, 1.0);
        assert!(f.beat);
        // The beat is one-shot; the loudness stays.
        let f = v.frame(&[0, 65535]).unwrap();
        assert!(!f.beat);
        assert_eq!(f.loudness, 1.0);
        assert!(v.frame(&[]).is_none());
    }
}
