//! Rust bindings for the canonical `ricardo.v1` Connect contract and a thin
//! scheduler client.
//!
//! The generated code lives in `src/generated` and is committed, mirroring the
//! Go stubs in `packages/proto/ricardo/v1`. It is produced by the official
//! `connectrpc-build` + `buffa` codegen (`generate.sh`); Bazel and Cargo build
//! it as a plain `rust_library` without `build.rs` or `protoc`.
//!
//! The client speaks **Connect JSON over HTTP/1.1**, the same wire shape as
//! `packages/go/scheduler`: protobuf JSON field names, int64 as decimal
//! strings, a bearer `Authorization` header and the
//! `/ricardo.v1.SchedulerService/<Method>` route. The scheduler is a LAN
//! service, so the plaintext transport is intentional; TLS stays a deployment
//! concern in front of the process.
//!
//! `buffa` messages are wire types, not a domain model. Solutions convert them
//! at their `api`/`store` boundary.

pub mod generated;
mod scheduler;
mod transport;

pub use generated::ricardo::v1;
pub use scheduler::{SchedulerClient, SchedulerError};
pub use transport::ReqwestTransport;
