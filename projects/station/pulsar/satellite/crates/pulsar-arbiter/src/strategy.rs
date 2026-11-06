//! The decision is pure: ordering of claims, the lookback rule and the solo strategy used when
//! the broadcast socket could not be opened.

use std::rc::Rc;

use tokio::sync::mpsc::UnboundedSender;

use crate::{ArbiterEvent, Claim, ClaimId};

/// A peer claim and when it was received (ms on the actor's monotonic clock).
pub struct Received {
    pub claim: Claim,
    pub at_ms: i64,
}

/// Busy beats idle, then the higher level, then the larger station name.
pub fn beats(a: &Claim, b: &Claim) -> bool {
    if a.busy != b.busy {
        return a.busy;
    }
    if a.level_db != b.level_db {
        return a.level_db > b.level_db;
    }
    a.station > b.station
}

/// Decision over the claims of the same wake word received up to the lookback before ours.
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
    // A peer that claimed more than a window before us decided before our claim existed: it
    // (or whoever beat it) answers already, so we yield whatever our level.
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

/// No socket: the station is alone, so every deciding claim wins without peers.
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
