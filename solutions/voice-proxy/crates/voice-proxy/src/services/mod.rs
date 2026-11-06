//! Service layer: every dependency that performs I/O behind a small type.

mod gateway;
mod ha;
mod queue;
mod results;
mod router;
mod updates;

use std::sync::Arc;

pub use gateway::{Gateway, GatewayError, RESULT_PATH, UPDATES_PATH, UTTERANCE_PATH};
pub use ha::{HaAccess, HaAnnouncer, ProfileReader};
pub use queue::{
    AnnouncementHandler, BUSY_GRACE, DETAIL_IDLE, IDLE_TIMEOUT, LEASE_SECONDS, Producer,
    STALE_AFTER, SchedulerAccess, SchedulerQueue, StationGate, lease_request, station_idle,
};
pub use results::{Results, ResultsOptions};
pub use router::{VoiceRouter, profile_or_unknown};
pub use updates::{Updates, UpdatesOptions, completed_unix};

/// A token resolved on every use, so a rotated mounted secret is picked up
/// without a restart.
pub type TokenProvider = Arc<dyn Fn() -> String + Send + Sync>;
