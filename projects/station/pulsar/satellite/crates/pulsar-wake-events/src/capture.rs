use pulsar_dsp::Ring;

use super::logic::{self, Kind};
use super::logic::{NEAR_AFTER_HIT_MS, NEAR_GAP_MS};
use super::{HIT_PRE_MS, MISS_MS, POST_MS, RING_MS, Recording};

const MAX_PENDING: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub struct HitInfo {
    pub id: String,
    pub score: f32,
}

#[derive(Debug, Default)]
pub struct Push {
    pub hit: Option<HitInfo>,
    pub near: bool,
    pub finished: Vec<Recording>,
    pub dropped: bool,
}

struct Pending {
    rec: Recording,
    remaining: usize,
}

pub struct Capture {
    frame: usize,
    ring: Ring<i16>,
    seq: u64,
    prev: Option<f32>,
    prev2: Option<f32>,
    last_hit_ms: Option<i64>,
    last_near_ms: Option<i64>,
    pending: Vec<Pending>,
    cutoff: f32,
}

impl Capture {
    pub fn new(frame: usize, cutoff: f32) -> Self {
        let chunks = logic::chunks_for(u64::from(RING_MS), frame);
        Self {
            frame,
            ring: Ring::new(frame, chunks * frame),
            seq: 0,
            prev: None,
            prev2: None,
            last_hit_ms: None,
            last_near_ms: None,
            pending: Vec::new(),
            cutoff,
        }
    }

    pub fn reset(&mut self) {
        self.ring.clear();
        self.prev = None;
        self.prev2 = None;
        self.pending.clear();
    }

    fn push_ring(&mut self, chunk: &[i16]) {
        if !self.ring.push_chunk(chunk) {
            self.reset();
        }
    }

    fn id(&mut self, utc_ms: i64, kind: Kind) -> String {
        self.seq += 1;
        logic::event_id(utc_ms, kind, self.seq)
    }

    pub fn push(
        &mut self,
        chunk: &[i16],
        mean: Option<f32>,
        hit: Option<f32>,
        dry_run: bool,
        now_ms: i64,
        utc_ms: i64,
    ) -> Push {
        self.push_ring(chunk);
        let mut finished = Vec::new();
        for p in &mut self.pending {
            p.rec.samples.extend_from_slice(chunk);
            p.remaining = p.remaining.saturating_sub(1);
        }
        let mut i = 0;
        while i < self.pending.len() {
            if self.pending[i].remaining == 0 {
                finished.push(self.pending.remove(i).rec);
            } else {
                i += 1;
            }
        }

        if let Some(score) = hit {
            self.last_hit_ms = Some(now_ms);
            self.prev = None;
            self.prev2 = None;
            let id = self.id(utc_ms, Kind::Hit);
            let rec = Recording {
                id: id.clone(),
                kind: Kind::Hit,
                utc_ms,
                pre_ms: HIT_PRE_MS,
                post_ms: POST_MS,
                score_peak: score,
                dry_run,
                samples: self
                    .ring
                    .tail(logic::chunks_for(u64::from(HIT_PRE_MS), self.frame)),
                label: None,
                label_source: None,
            };
            let mut dropped = false;
            if self.pending.len() < MAX_PENDING {
                self.pending.push(Pending {
                    rec,
                    remaining: logic::chunks_for(u64::from(POST_MS), self.frame),
                });
            } else {
                dropped = true;
            }
            return Push {
                hit: (!dropped).then_some(HitInfo { id, score }),
                near: false,
                finished,
                dropped,
            };
        }

        let mut near = false;
        if let Some(cur) = mean {
            if let Some(prev) = self.prev
                && logic::near_peak(self.prev2, prev, cur, self.cutoff)
                && self
                    .last_hit_ms
                    .is_none_or(|t| now_ms.saturating_sub(t) >= NEAR_AFTER_HIT_MS)
                && self
                    .last_near_ms
                    .is_none_or(|t| now_ms.saturating_sub(t) >= NEAR_GAP_MS)
            {
                let id = self.id(utc_ms, Kind::Near);
                let mut samples = self
                    .ring
                    .tail(logic::chunks_for(u64::from(HIT_PRE_MS), self.frame) + 1);
                samples.truncate(samples.len().saturating_sub(self.frame));
                samples.extend_from_slice(chunk);
                let post = logic::chunks_for(u64::from(POST_MS), self.frame);
                let rec = Recording {
                    id,
                    kind: Kind::Near,
                    utc_ms,
                    pre_ms: HIT_PRE_MS,
                    post_ms: POST_MS,
                    score_peak: prev,
                    dry_run: false,
                    samples,
                    label: None,
                    label_source: None,
                };
                if self.pending.len() < MAX_PENDING {
                    self.pending.push(Pending {
                        rec,
                        remaining: post.saturating_sub(1),
                    });
                    near = true;
                    self.last_near_ms = Some(now_ms);
                }
            }
            self.prev2 = self.prev;
            self.prev = Some(cur);
        } else {
            self.prev = None;
            self.prev2 = None;
        }
        Push {
            hit: None,
            near,
            finished,
            dropped: false,
        }
    }

    pub fn miss(&mut self, utc_ms: i64) -> Option<Recording> {
        if self.ring.filled_chunks() == 0 {
            return None;
        }
        Some(Recording {
            id: self.id(utc_ms, Kind::Miss),
            kind: Kind::Miss,
            utc_ms,
            pre_ms: MISS_MS,
            post_ms: 0,
            score_peak: 0.0,
            dry_run: false,
            samples: self
                .ring
                .tail(logic::chunks_for(u64::from(MISS_MS), self.frame)),
            label: Some("missed"),
            label_source: Some("user"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: usize = 512;

    fn chunk(k: usize) -> Vec<i16> {
        vec![k as i16; FRAME]
    }

    fn chunks(ms: u64) -> usize {
        logic::chunks_for(ms, FRAME)
    }

    #[test]
    fn hit_carries_three_seconds_before_and_one_after() {
        let mut c = Capture::new(FRAME, 0.9);
        for k in 0..chunks(u64::from(RING_MS)) {
            c.push(&chunk(k), Some(0.1), None, false, 0, 0);
        }
        let p = c.push(&chunk(11), Some(0.95), Some(0.95), false, 10, 0);
        let hit = p.hit.clone().unwrap();
        assert_eq!(hit.id, "19700101T000000Z-hit-1");
        assert!(p.finished.is_empty());
        let mut done = Vec::new();
        for _ in 0..chunks(u64::from(POST_MS)) {
            let p = c.push(&chunk(12), None, None, false, 20, 0);
            done.extend(p.finished);
        }
        assert_eq!(done.len(), 1);
        let rec = &done[0];
        assert_eq!(rec.kind, Kind::Hit);
        assert_eq!(
            rec.samples.len(),
            (chunks(u64::from(HIT_PRE_MS)) + chunks(u64::from(POST_MS))) * FRAME
        );
        assert_eq!(rec.samples.len() / 16, 4032);
        assert_eq!(rec.samples[chunks(u64::from(HIT_PRE_MS)) * FRAME - 1], 11);
        assert_eq!(rec.score_peak, 0.95);
    }

    #[test]
    fn near_miss_ends_at_the_peak_chunk() {
        let mut c = Capture::new(FRAME, 0.9);
        for k in 0..chunks(u64::from(RING_MS)) {
            c.push(&chunk(k), Some(0.1), None, false, 0, 0);
        }
        assert!(!c.push(&chunk(300), Some(0.7), None, false, 1000, 0).near);
        let p = c.push(&chunk(301), Some(0.3), None, false, 1000 + 32, 0);
        assert!(p.near);
        assert!(p.hit.is_none());
        let mut done = Vec::new();
        for _ in 0..chunks(u64::from(POST_MS)) {
            done.extend(
                c.push(&chunk(302), Some(0.0), None, false, 2000, 0)
                    .finished,
            );
        }
        assert_eq!(done.len(), 1);
        let rec = &done[0];
        assert_eq!(rec.kind, Kind::Near);
        assert_eq!(rec.id, "19700101T000000Z-near-1");
        assert_eq!(
            rec.samples.len(),
            (chunks(u64::from(HIT_PRE_MS)) + chunks(u64::from(POST_MS))) * FRAME
        );
        assert_eq!(rec.samples[chunks(u64::from(HIT_PRE_MS)) * FRAME - 1], 300);
        assert_eq!(rec.samples[chunks(u64::from(HIT_PRE_MS)) * FRAME], 301);
    }

    #[test]
    fn near_misses_are_spaced_and_suppressed_around_a_hit() {
        let mut c = Capture::new(FRAME, 0.9);
        for k in 0..5 {
            c.push(&chunk(k), Some(0.1), None, false, 0, 0);
        }
        c.push(&chunk(6), Some(0.7), None, false, 10_000, 0);
        assert!(c.push(&chunk(7), Some(0.1), None, false, 10_032, 0).near);
        c.push(&chunk(8), Some(0.7), None, false, 12_000, 0);
        assert!(!c.push(&chunk(9), Some(0.1), None, false, 12_032, 0).near);
        c.push(&chunk(10), Some(0.1), None, false, 20_000, 0);
        let p = c.push(&chunk(11), Some(0.97), Some(0.97), false, 20_032, 0);
        assert!(p.hit.is_some());
        c.push(&chunk(12), Some(0.6), None, false, 20_500, 0);
        c.push(&chunk(13), Some(0.8), None, false, 20_532, 0);
        assert!(!c.push(&chunk(14), Some(0.2), None, false, 20_564, 0).near);
    }

    #[test]
    fn miss_is_the_last_thirty_seconds_and_labeled() {
        let mut c = Capture::new(FRAME, 0.9);
        assert!(c.miss(0).is_none());
        for k in 0..chunks(u64::from(RING_MS)) {
            c.push(&chunk(k % 3000), Some(0.1), None, false, 0, 0);
        }
        let rec = c.miss(1_000_000).unwrap();
        assert_eq!(rec.kind, Kind::Miss);
        assert_eq!(rec.label, Some("missed"));
        assert_eq!(rec.label_source, Some("user"));
        assert_eq!(rec.samples.len(), chunks(u64::from(MISS_MS)) * FRAME);
        assert_eq!(
            rec.samples[rec.samples.len() - 1],
            (chunks(u64::from(RING_MS)) - 1) as i16
        );
        assert_eq!(rec.id, "19700101T001640Z-miss-1");
    }

    #[test]
    fn reset_drops_the_ring_and_pending_windows() {
        let mut c = Capture::new(FRAME, 0.9);
        for k in 0..10 {
            c.push(&chunk(k), Some(0.1), None, false, 0, 0);
        }
        c.push(&chunk(10), Some(0.95), Some(0.95), false, 0, 0);
        c.reset();
        assert!(c.miss(0).is_none());
    }
}
