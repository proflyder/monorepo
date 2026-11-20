pub const LEVEL_SAMPLES: usize = 512;
pub const LEVEL_CHUNKS: usize = 47;
pub const LEVEL_TOP: usize = 8;
const SPEECH_END_RATIO: f32 = 0.01;
pub const SILENCE_DB: f32 = -100.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelDb(f32);

impl LevelDb {
    pub const SILENCE: Self = Self(SILENCE_DB);

    pub fn from_mean_square(mean_square: f32) -> Self {
        let db = (10.0 * (mean_square.max(0.0) + 1e-10).log10()).clamp(SILENCE_DB, 0.0);
        Self((db * 10.0).round() / 10.0)
    }

    pub fn from_pcm(pcm: &[i16]) -> Self {
        if pcm.is_empty() {
            return Self::SILENCE;
        }
        let mut sq = 0.0f64;
        for &s in pcm {
            let v = f64::from(s) * (1.0 / 32768.0);
            sq = v.mul_add(v, sq);
        }
        Self::from_mean_square((sq / pcm.len() as f64) as f32)
    }

    pub fn as_f32(self) -> f32 {
        self.0
    }
}

impl Default for LevelDb {
    fn default() -> Self {
        Self::SILENCE
    }
}

pub fn mean_square(d: &[f32]) -> f32 {
    let mut sq = 0.0f64;
    for &v in d {
        sq = f64::from(v).mul_add(f64::from(v), sq);
    }
    (sq / d.len() as f64) as f32
}

pub fn level_db(values: &[f32; LEVEL_CHUNKS]) -> LevelDb {
    let mut s = *values;
    s.sort_unstable_by(|a, b| b.total_cmp(a));
    let mut sum = 0.0f32;
    for &v in &s[..LEVEL_TOP] {
        sum += v;
    }
    LevelDb::from_mean_square(sum / LEVEL_TOP as f32)
}

pub fn percentile_db(window: &[f32], q: f32) -> LevelDb {
    if window.is_empty() {
        return LevelDb::SILENCE;
    }
    let mut s = window.to_vec();
    s.sort_unstable_by(f32::total_cmp);
    let i = ((s.len() - 1) as f32 * q.clamp(0.0, 1.0)).round() as usize;
    LevelDb::from_mean_square(s[i])
}

pub struct LevelHistory {
    values: [f32; LEVEL_CHUNKS],
    pos: usize,
    pending: [f32; LEVEL_SAMPLES],
    used: usize,
}

impl LevelHistory {
    pub fn new() -> Self {
        Self {
            values: [0.0; LEVEL_CHUNKS],
            pos: 0,
            pending: [0.0; LEVEL_SAMPLES],
            used: 0,
        }
    }

    pub fn push(&mut self, mut samples: &[f32]) {
        while !samples.is_empty() {
            let n = samples.len().min(LEVEL_SAMPLES - self.used);
            self.pending[self.used..self.used + n].copy_from_slice(&samples[..n]);
            self.used += n;
            samples = &samples[n..];
            if self.used == LEVEL_SAMPLES {
                self.values[self.pos] = mean_square(&self.pending);
                self.pos = (self.pos + 1) % LEVEL_CHUNKS;
                self.used = 0;
            }
        }
    }

    pub fn values(&self) -> &[f32; LEVEL_CHUNKS] {
        &self.values
    }

    pub fn level(&self) -> LevelDb {
        level_db(&self.values)
    }

    pub fn speech_end(&self, frame: usize) -> Option<usize> {
        let newest = (self.pos + LEVEL_CHUNKS - 1) % LEVEL_CHUNKS;
        speech_end_chunks(&self.values, newest)
            .map(|k| (k * LEVEL_SAMPLES + self.used) / frame.max(1))
    }
}

impl Default for LevelHistory {
    fn default() -> Self {
        Self::new()
    }
}

fn speech_end_chunks(h: &[f32; LEVEL_CHUNKS], newest: usize) -> Option<usize> {
    let peak = h.iter().copied().fold(0.0f32, f32::max);
    if peak <= 0.0 {
        return None;
    }
    let floor = SPEECH_END_RATIO * peak;
    (0..LEVEL_CHUNKS).find(|&k| h[(newest + LEVEL_CHUNKS - k) % LEVEL_CHUNKS] >= floor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_is_the_mean_of_the_8_loudest_rounded_to_a_tenth() {
        assert_eq!(level_db(&[0.0; LEVEL_CHUNKS]).as_f32(), -100.0);
        let mut h = [1e-6f32; LEVEL_CHUNKS];
        for i in [0, 5, 13, 20, 30, 40, 45, 46] {
            h[i] = 0.01;
        }
        assert_eq!(level_db(&h).as_f32(), -20.0);
        h[1] = 0.01;
        assert_eq!(level_db(&h).as_f32(), -20.0);
        h[2] = 0.09;
        assert_eq!(level_db(&h).as_f32(), -17.0);
        let at = |db: f64| [10f64.powf(db / 10.0) as f32; LEVEL_CHUNKS];
        assert_eq!(level_db(&at(-32.46)).as_f32(), -32.5);
        assert_eq!(level_db(&at(-32.44)).as_f32(), -32.4);
        assert_eq!(level_db(&at(3.0)).as_f32(), 0.0);
        assert_eq!(level_db(&at(-120.0)).as_f32(), -100.0);
    }

    #[test]
    fn level_window_is_independent_of_vqe_chunk_size() {
        let mut full = LevelHistory::new();
        let mut mini3 = LevelHistory::new();
        let mut old = [0.0; LEVEL_CHUNKS];
        for block in 0..LEVEL_CHUNKS + 5 {
            let samples: Vec<f32> = (0..LEVEL_SAMPLES)
                .map(|i| ((block * 17 + i * 3) % 97) as f32 / 128.0)
                .collect();
            let before = mini3.values().map(f32::to_bits);
            mini3.push(&samples[..256]);
            assert_eq!(mini3.values().map(f32::to_bits), before);
            mini3.push(&samples[256..]);
            full.push(&samples);
            old[block % LEVEL_CHUNKS] = mean_square(&samples);
            assert_eq!(full.values().map(f32::to_bits), old.map(f32::to_bits));
            assert_eq!(mini3.values().map(f32::to_bits), old.map(f32::to_bits));
            assert_eq!(mini3.pos, full.pos);
            assert_eq!(mini3.level(), level_db(&old));
        }
    }

    #[test]
    fn speech_end_is_the_newest_chunk_within_20_db_of_the_peak() {
        let mut h = [0f32; LEVEL_CHUNKS];
        assert_eq!(speech_end_chunks(&h, 10), None);
        for v in &mut h[3..=8] {
            *v = 1.0;
        }
        h[9] = 0.009;
        assert_eq!(speech_end_chunks(&h, 12), Some(4));
        h[9] = 0.01;
        assert_eq!(speech_end_chunks(&h, 12), Some(3));
        assert_eq!(speech_end_chunks(&h, 1), Some(LEVEL_CHUNKS - 9 + 1));
        assert_eq!(speech_end_chunks(&h, 9), Some(0));
    }

    #[test]
    fn pcm_level_matches_the_frontend_scale() {
        assert_eq!(LevelDb::from_pcm(&[]).as_f32(), -100.0);
        assert_eq!(LevelDb::from_pcm(&[3000; 512]).as_f32(), -20.8);
        assert_eq!(LevelDb::from_pcm(&[0; 512]).as_f32(), -100.0);
    }
}
