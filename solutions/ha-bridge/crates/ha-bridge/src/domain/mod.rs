//! Pure executor decisions: which script to call, in which order, and whether
//! a global `alarm_stop` may silence the currently ringing alarm.
//!
//! Nothing here performs I/O. The rules are the ones the Go bridge encoded in
//! `internal/bridge`, with the same user-visible strings so the golden vectors
//! in `solutions/ha-bridge/testdata/events` stay stable.

mod announce;
mod order;
mod plan;
mod stop;

pub use announce::announce_message;
pub use order::order_events;
pub use plan::{PlanError, PlannedAction, Scripts, action_label, plan, source_kind_label};
pub use stop::stop_matches_active;
