//! Pure domain logic of the voice proxy: no I/O, time only through [`Clock`].

pub mod announce;
pub mod cancel;
pub mod clock;
pub mod profile;
pub mod queue_payload;
pub mod routing;
pub mod sanitize;

pub use clock::{Clock, SystemClock};
pub use profile::Profile;
