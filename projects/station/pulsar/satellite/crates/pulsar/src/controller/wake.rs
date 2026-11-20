use pulsar_arbiter::{Claim, ClaimId};
use pulsar_audio::endpoint::EndpointPrior;

use super::WakeRecord;
use super::logic::COMMAND_AFTER_WAKE_MS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimToken(pub(super) u64);

impl From<ClaimId> for ClaimToken {
    fn from(id: ClaimId) -> Self {
        Self(id.get())
    }
}

pub struct Claiming {
    claim: ClaimToken,
    phrase: String,
    level_db: f32,
    audio: Vec<i16>,
    boundary: usize,
    endpoint: Option<EndpointPrior>,
    record: Option<WakeRecord>,
}

pub enum Decision {
    Won {
        phrase: String,
        preroll: Vec<i16>,
        live: Vec<i16>,
        endpoint: Option<EndpointPrior>,
        record: Option<WakeRecord>,
        peers: usize,
    },
    Lost {
        best: Claim,
        level_db: f32,
        record: Option<WakeRecord>,
    },
}

#[derive(Default)]
pub enum Arbitration {
    #[default]
    Idle,
    Claiming(Claiming),
    Lost {
        until_ms: i64,
    },
}

impl Arbitration {
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

    pub fn push_audio(&mut self, pcm: &[i16]) {
        if let Self::Claiming(c) = self {
            c.audio.extend_from_slice(pcm);
        }
    }

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

    pub fn lost(&mut self, now: i64) {
        *self = Self::Lost {
            until_ms: now + COMMAND_AFTER_WAKE_MS,
        };
    }

    pub fn blocked(&self, now: i64) -> bool {
        matches!(self, Self::Lost { until_ms } if now < *until_ms)
    }

    pub fn clear(&mut self) {
        *self = Self::Idle;
    }
}
