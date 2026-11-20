use std::rc::Rc;

use tokio::sync::mpsc::UnboundedSender;

use crate::{ArbiterEvent, Claim, ClaimId};

pub struct Received {
    pub claim: Claim,
    pub at_ms: i64,
}

pub fn beats(a: &Claim, b: &Claim) -> bool {
    if a.busy != b.busy {
        return a.busy;
    }
    if a.level_db != b.level_db {
        return a.level_db > b.level_db;
    }
    a.station > b.station
}

pub fn decision<'a>(
    mine: &'a Claim,
    claim_ms: i64,
    recent: &'a [Received],
    window_ms: i64,
    lookback_ms: i64,
) -> (bool, &'a Claim, usize) {
    let mut best = mine;
    let mut won = true;
    let mut peers = 0;
    let mut early: Option<&Claim> = None;
    for r in recent {
        if r.at_ms < claim_ms - lookback_ms || r.claim.wake != mine.wake {
            continue;
        }
        peers += 1;
        if r.at_ms < claim_ms - window_ms {
            if early.is_none_or(|e| beats(&r.claim, e)) {
                early = Some(&r.claim);
            }
        } else if beats(&r.claim, best) {
            best = &r.claim;
            won = false;
        }
    }
    if let Some(e) = early {
        best = e;
        won = false;
    }
    (won, best, peers)
}

#[derive(Clone)]
pub struct Solo {
    station: Rc<str>,
    events: UnboundedSender<ArbiterEvent>,
}

impl Solo {
    pub fn new(station: String, events: UnboundedSender<ArbiterEvent>) -> Self {
        Self {
            station: station.into(),
            events,
        }
    }

    pub fn claim(&self, id: ClaimId, wake: &str, level_db: f32, busy: bool, decide: bool) {
        if decide {
            let best = Claim {
                station: self.station.to_string(),
                wake: wake.to_owned(),
                busy,
                level_db,
            };
            let _ = self.events.send(ArbiterEvent::Decided {
                claim_id: id,
                won: true,
                best,
                peers: 0,
            });
        }
    }
}
