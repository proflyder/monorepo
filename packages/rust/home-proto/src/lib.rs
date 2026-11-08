//! Rust bindings for the canonical `home.v1` Connect contract and a thin
//! scheduler client.
//!
//! The generated code lives in `src/generated` and is committed, mirroring the
//! proto schema in `packages/proto/home/v1`. It is produced by the official
//! `connectrpc-build` + `buffa` codegen (`generate.sh`); Bazel and Cargo build
//! it as a plain `rust_library` without `build.rs` or `protoc`.
//!
//! The client speaks **Connect JSON over HTTP/1.1**, the same wire shape as
//! Android and HA clients: protobuf JSON field names, int64 as decimal
//! strings, a bearer `Authorization` header and the
//! `/home.v1.SchedulerService/<Method>` route. The scheduler is a LAN
//! service, so the plaintext transport is intentional; TLS stays a deployment
//! concern in front of the process.
//!
//! `buffa` messages are wire types, not a domain model. Solutions convert them
//! at their `api`/`store` boundary.

pub mod generated;
mod scenario;
mod scheduler;
mod transport;

pub use generated::home::v1;
pub use scenario::{ScenarioClient, ScenarioError};
pub use scheduler::{SchedulerClient, SchedulerError};
pub use transport::{CaError, ReqwestTransport};
