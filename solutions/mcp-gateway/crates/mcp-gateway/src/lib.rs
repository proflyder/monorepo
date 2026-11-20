pub mod app;
pub mod bb;
pub mod chat;
pub mod checks;
pub mod config;
pub mod domain;
pub mod health;
pub mod kubernetes;
pub mod mcp;
pub mod metrics;
pub mod push;
pub mod scheduler;
pub mod server;
pub mod ssh;
pub mod system;
pub mod tools;
pub mod voice;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Transient,
    Permanent,
}
