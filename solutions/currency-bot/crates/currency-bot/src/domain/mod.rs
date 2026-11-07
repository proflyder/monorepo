//! Domain layer: pure model, parser, alert planner, message and schedules.

pub mod alerts;
pub mod clock;
pub mod message;
pub mod model;
pub mod rates;
pub mod schedule;
pub mod texts;
pub mod time;

#[cfg(test)]
mod vectors;
