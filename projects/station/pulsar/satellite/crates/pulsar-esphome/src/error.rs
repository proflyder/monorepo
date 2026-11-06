//! Типизированные ошибки сервера.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Ошибка запуска сервера: порт не открылся.
#[derive(Debug, Error)]
pub enum SpawnError {
    #[error("api: socket: {0}")]
    Socket(#[source] io::Error),
    #[error("api: bind/listen :{port}: {source}")]
    Bind {
        port: u16,
        #[source]
        source: io::Error,
    },
}

/// Ошибка файла ключа шифрования.
#[derive(Debug, Error)]
pub enum KeyError {
    #[error("api: cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("api: cannot remove {path}: {source}")]
    Remove {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("api: {path} is not base64 of 32 non-zero bytes")]
    Invalid { path: PathBuf },
}
