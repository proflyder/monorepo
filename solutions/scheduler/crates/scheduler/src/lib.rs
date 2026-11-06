//! Pure scheduler domain.
//!
//! This crate owns the semantics of the scheduler without I/O: recurrence in
//! IANA time zones, occurrence and ringing-window rules, stable identifiers,
//! canonical idempotency hashes and input validation. It deliberately has no
//! dependency on the protobuf types, SQLite or the network; the later `store`
//! and `api` layers convert to and from their wire representation.
//!
//! The behaviour mirrors the former Go implementation byte for byte: the
//! `testdata/planner/*.json` and `testdata/ids.json` vectors under
//! `solutions/scheduler` are the shared golden tables.

#![deny(missing_debug_implementations)]

pub mod api;
pub mod domain;
pub mod store;
