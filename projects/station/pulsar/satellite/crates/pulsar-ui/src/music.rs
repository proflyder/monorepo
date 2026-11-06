//! Общий источник музыки для кольца и часов: приоритет Sendspin над динамиком, «свежесть»
//! кадров. Обе реализации раньше дублировали это в `pulsar-hw/src/leds.rs` и
//! `pulsar-clock/src/clock.rs`.

use pulsar_core::{MUSIC_STALE_MS, MusicFrame, SENDSPIN_FRESH_MS};

/// Откуда пришёл кадр музыки.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MusicSource {
    /// Референс динамика из захвата.
    Speaker,
    /// Роль визуализатора Sendspin (Music Assistant).
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

/// Последний принятый кадр и правило приоритета: кадры Sendspin выигрывают, пока идут, затем
/// кадры динамика; старше `MUSIC_STALE_MS` кадров нет.
#[derive(Clone, Debug, Default)]
pub struct MusicFrames {
    frame: MusicFrame,
    at: Option<i64>,
    /// Кадры динамика игнорируются до этого момента.
    sendspin_until: i64,
    /// Бит, ещё не нарисованный.
    beat: bool,
}

impl MusicFrames {
    /// Принимает кадр; false — кадр отброшен, потому что Sendspin ещё свежий.
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

    /// Был ли бит с прошлого вызова.
    pub fn take_beat(&mut self) -> bool {
        std::mem::take(&mut self.beat)
    }
}
