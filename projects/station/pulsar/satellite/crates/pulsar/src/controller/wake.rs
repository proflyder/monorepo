//! Арбитраж wake word между станциями: claim, окно решения и потерянный turn
//! (controller.cpp "wake arbitration", `docs/voice.md`, «Арбитраж между станциями»).

use pulsar_arbiter::{Claim, ClaimId};
use pulsar_audio::endpoint::EndpointPrior;

use super::WakeRecord;
use super::logic::COMMAND_AFTER_WAKE_MS;

/// Id своего claim в терминах контроллера: порт арбитра отдаёт тот же id, что потом придёт в
/// `Decided`, но не публикует конструктор `ClaimId` ради тестов.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimToken(pub(super) u64);

impl From<ClaimId> for ClaimToken {
    fn from(id: ClaimId) -> Self {
        Self(id.get())
    }
}

/// Wake word, который станция услышала и на который отправила claim: пока станции решают,
/// пре-ролл и речь после слова удерживаются, а командный спотер молчит.
pub struct Claiming {
    claim: ClaimToken,
    phrase: String,
    level_db: f32,
    /// Пре-ролл из события, затем живая речь после него.
    audio: Vec<i16>,
    /// Сколько сэмплов `audio` пришло с событием; остальное — речь, удержанная до решения.
    boundary: usize,
    endpoint: Option<EndpointPrior>,
    record: Option<WakeRecord>,
}

/// Что решили соседи по одному claim.
pub enum Decision {
    /// Станция отвечает; всё удержанное уходит в новый диалог.
    Won {
        phrase: String,
        /// Пре-ролл из события.
        preroll: Vec<i16>,
        /// Речь, пришедшая после события, пока шло решение.
        live: Vec<i16>,
        endpoint: Option<EndpointPrior>,
        record: Option<WakeRecord>,
        peers: usize,
    },
    /// Ответила другая станция: её claim и наш уровень (для лога).
    Lost {
        best: Claim,
        level_db: f32,
        record: Option<WakeRecord>,
    },
}

/// Состояние арбитража: покой, своё окно решения или проигранный turn, в котором команды
/// этой станции молчат, пока отвечает сосед.
#[derive(Default)]
pub enum Arbitration {
    #[default]
    Idle,
    Claiming(Claiming),
    /// Чужой ответ: команды не действуют до `until_ms`.
    Lost {
        until_ms: i64,
    },
}

impl Arbitration {
    /// Новый claim заменяет проигранный turn: своя станция снова участвует.
    pub fn start(
        &mut self,
        claim: ClaimToken,
        phrase: String,
        level_db: f32,
        preroll: Vec<i16>,
        endpoint: Option<EndpointPrior>,
        record: Option<WakeRecord>,
    ) {
        let boundary = preroll.len();
        *self = Self::Claiming(Claiming {
            claim,
            phrase,
            level_db,
            audio: preroll,
            boundary,
            endpoint,
            record,
        });
    }

    pub fn is_claiming(&self) -> bool {
        matches!(self, Self::Claiming(_))
    }

    #[cfg(test)]
    pub fn claim_id(&self) -> Option<ClaimToken> {
        match self {
            Self::Claiming(c) => Some(c.claim),
            _ => None,
        }
    }

    /// Живой звук во время решения продолжает пре-ролл. Вне окна ничего не удерживается.
    pub fn push_audio(&mut self, pcm: &[i16]) {
        if let Self::Claiming(c) = self {
            c.audio.extend_from_slice(pcm);
        }
    }

    /// Соседи решили; None — это чужое или уже отменённое решение.
    pub fn decided(
        &mut self,
        claim_id: ClaimToken,
        won: bool,
        best: &Claim,
        peers: usize,
    ) -> Option<Decision> {
        if !self.is_claiming() {
            return None;
        }
        let Self::Claiming(c) = std::mem::take(self) else {
            unreachable!("checked above");
        };
        if c.claim != claim_id {
            *self = Self::Claiming(c);
            return None;
        }
        if !won {
            let mut record = c.record;
            if let Some(r) = &mut record {
                r.arbiter_lost = true;
            }
            return Some(Decision::Lost {
                best: best.clone(),
                level_db: c.level_db,
                record,
            });
        }
        let mut audio = c.audio;
        let split = c.boundary.min(audio.len());
        let live = audio.split_off(split);
        Some(Decision::Won {
            phrase: c.phrase,
            preroll: audio,
            live,
            endpoint: c.endpoint,
            record: c.record,
            peers,
        })
    }

    /// Отмена claim до решения; вызывать только в `Claiming` (иначе ничего не делает).
    /// Возвращает записанное срабатывание, если оно было.
    pub fn cancel(&mut self) -> Option<WakeRecord> {
        if !self.is_claiming() {
            return None;
        }
        let Self::Claiming(c) = std::mem::take(self) else {
            unreachable!("checked above");
        };
        let mut record = c.record;
        if let Some(r) = &mut record {
            r.cancelled = true;
        }
        record
    }

    /// Ответила другая станция: команды этой станции молчат до `until_ms`.
    pub fn lost(&mut self, now: i64) {
        *self = Self::Lost {
            until_ms: now + COMMAND_AFTER_WAKE_MS,
        };
    }

    pub fn blocked(&self, now: i64) -> bool {
        matches!(self, Self::Lost { until_ms } if now < *until_ms)
    }

    /// Новый диалог или своё слово снимают проигранный turn.
    pub fn clear(&mut self) {
        *self = Self::Idle;
    }
}
