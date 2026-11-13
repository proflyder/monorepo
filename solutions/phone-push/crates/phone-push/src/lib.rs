//! FCM HTTP v1 delivery for the owner's phone.
//!
//! The service is stateless: device tokens live in the scheduler (the phone
//! registers them on the endpoint and bearer it already has) and this process
//! only turns one request into one FCM send. The FCM service account is the
//! single secret it owns, so the MCP gateway and Hermes never see a Google
//! credential.
//!
//! Two message shapes exist on purpose:
//!
//! * **notification** — a normal notification the phone renders from resources
//!   (title/body/deep link), delivered on the visible channel;
//! * **find** — a data-only, high-priority message with no `notification`
//!   block, so the app starts its find-phone foreground service and rings even
//!   when the screen is off, Do Not Disturb is on, or the app was killed.

pub mod api;
pub mod app;
pub mod config;
pub mod devices;
pub mod fcm;
pub mod health;

pub use app::VERSION;
