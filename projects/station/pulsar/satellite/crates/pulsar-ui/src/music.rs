use pulsar_core::{MUSIC_STALE_MS, MusicFrame, SENDSPIN_FRESH_MS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MusicSource {
    Speaker,
    Sendspin,
}

impl MusicSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Speaker => "speaker",
            Self::Sendspin => "sendspin",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MusicFrames {
    frame: MusicFrame,
    at: Option<i64>,
    sendspin_until: i64,
    beat: bool,
}

impl MusicFrames {
    pub fn push(&mut self, frame: MusicFrame, source: MusicSource, now: i64) -> bool {
        match source {
            MusicSource::Sendspin => self.sendspin_until = now + SENDSPIN_FRESH_MS,
            MusicSource::Speaker if now < self.sendspin_until => return false,
            MusicSource::Speaker => {}
        }
        self.frame = frame;
        self.at = Some(now);
        self.beat |= frame.beat;
        true
    }

    pub fn frame(&self) -> MusicFrame {
        self.frame
    }

    pub fn fresh(&self, now: i64) -> bool {
        self.at.is_some_and(|t| now - t < MUSIC_STALE_MS)
    }

    pub fn take_beat(&mut self) -> bool {
        std::mem::take(&mut self.beat)
    }
}
