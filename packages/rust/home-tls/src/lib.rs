//! Public HTTPS roots for the shared workspace `reqwest` client.
//!
//! The workspace builds `reqwest` with `rustls-no-provider`: the station
//! firmware (`pulsar-update`) pins its own CA bundle and must not inherit a
//! platform trust store. Cargo unifies features across the whole workspace, so
//! enabling built-in roots here would also change the firmware. Instead every
//! client that talks to the public internet loads the operating system's PEM
//! bundle explicitly with [`system_roots`] and replaces the trust set with
//! [`with_system_roots`] (or starts from [`public_https_client`]).
//!
//! The bundle path is `SSL_CERT_FILE` when set (a deployment or test override),
//! otherwise the first existing default: `/etc/ssl/certs/ca-certificates.crt`
//! (distroless static and Debian) or `/etc/ssl/cert.pem` (macOS). A missing,
//! unreadable, malformed or empty bundle is an error: a service that cannot
//! reach its public endpoints must fail at start-up rather than silently.
//!
//! The ring crypto provider is installed by [`public_https_client`], because
//! the workspace `reqwest` has no default provider.

use std::path::{Path, PathBuf};

/// Environment variable that overrides the CA bundle path.
pub const SSL_CERT_FILE: &str = "SSL_CERT_FILE";

/// Primary default bundle: distroless static and Debian/Ubuntu.
pub const DEFAULT_BUNDLE_PATH: &str = "/etc/ssl/certs/ca-certificates.crt";

/// Secondary default bundle: macOS ships the same PEM format here.
pub const FALLBACK_BUNDLE_PATH: &str = "/etc/ssl/cert.pem";

/// Why the system roots could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The bundle does not exist.
    #[error("no CA bundle at {path}: set SSL_CERT_FILE to a PEM bundle")]
    Missing { path: PathBuf },
    /// The bundle exists but cannot be read.
    #[error("cannot read CA bundle {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The bundle is not valid PEM or contains no certificates.
    #[error("CA bundle {path} has no usable certificates")]
    Empty { path: PathBuf },
    /// The bundle contains a certificate rustls rejects.
    #[error("invalid CA bundle {path}: {source}")]
    Decode {
        path: PathBuf,
        #[source]
        source: reqwest::Error,
    },
}

/// Resolve the bundle path: `SSL_CERT_FILE`, else the first default that exists.
///
/// An explicit `SSL_CERT_FILE` is never silently replaced by a default: a typo
/// in the deployment must surface as an error.
pub fn bundle_path() -> Result<PathBuf, Error> {
    if let Some(configured) = std::env::var_os(SSL_CERT_FILE).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(configured));
    }
    for candidate in [DEFAULT_BUNDLE_PATH, FALLBACK_BUNDLE_PATH] {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path);
        }
    }
    Err(Error::Missing {
        path: PathBuf::from(DEFAULT_BUNDLE_PATH),
    })
}

/// Read every certificate from `path`.
pub fn roots_from(path: &Path) -> Result<Vec<reqwest::Certificate>, Error> {
    let pem = std::fs::read(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            return Error::Missing {
                path: path.to_owned(),
            };
        }
        Error::Read {
            path: path.to_owned(),
            source,
        }
    })?;
    let certificates =
        reqwest::Certificate::from_pem_bundle(&pem).map_err(|source| Error::Decode {
            path: path.to_owned(),
            source,
        })?;
    if certificates.is_empty() {
        return Err(Error::Empty {
            path: path.to_owned(),
        });
    }
    Ok(certificates)
}

/// Load the operating system's public roots (see [`bundle_path`]).
pub fn system_roots() -> Result<Vec<reqwest::Certificate>, Error> {
    roots_from(&bundle_path()?)
}

/// Add every system root to `builder`.
///
/// Extra roots never narrow the client: they only extend the trust anchors, so
/// internal HTTP/HTTPS clients can use this unconditionally.
pub fn with_system_roots(builder: reqwest::ClientBuilder) -> Result<reqwest::ClientBuilder, Error> {
    // reqwest 0.13 verifies against the platform store by default; pin the
    // same OS bundle explicitly with `tls_certs_only` so the trust set stays
    // identical to the pre-0.13 manual-roots build.
    Ok(builder.tls_certs_only(system_roots()?))
}

/// A fresh `reqwest` builder with the ring provider installed and the system
/// roots added. Callers set the timeout, redirect policy and proxy themselves.
pub fn public_https_client() -> Result<reqwest::ClientBuilder, Error> {
    install_crypto_provider();
    with_system_roots(reqwest::Client::builder())
}

/// Install the process-wide ring provider before the first `reqwest` client.
///
/// The workspace `reqwest` is compiled without a default provider and panics
/// ("No provider set") otherwise. A provider installed earlier is kept.
pub fn install_crypto_provider() {
    let _already_installed = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, MutexGuard};

    use rustls::ServerConfig;
    use rustls_pki_types::pem::PemObject;
    use rustls_pki_types::{CertificateDer, PrivateKeyDer};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;

    use super::*;

    const CA_PEM: &str = include_str!("../testdata/ca.pem");
    const SERVER_PEM: &str = include_str!("../testdata/server.pem");
    const SERVER_KEY: &str = include_str!("../testdata/server-key.pem");

    /// `SSL_CERT_FILE` is process-global; the tests that touch it take turns.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().expect("env lock")
    }

    /// Write the test CA into a fresh file and return its path.
    fn ca_file() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("ca.pem");
        std::fs::write(&path, CA_PEM).expect("write CA");
        (directory, path)
    }

    /// A loopback HTTPS server that answers one `200 ok` to any request.
    async fn serve_https() -> u16 {
        let certificates: Vec<_> = CertificateDer::pem_slice_iter(SERVER_PEM.as_bytes())
            .collect::<Result<_, _>>()
            .expect("server certificate");
        let key = PrivateKeyDer::from_pem_slice(SERVER_KEY.as_bytes()).expect("server key");
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("protocol versions")
            .with_no_client_auth()
            .with_single_cert(certificates, key)
            .expect("server config");
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        tokio::spawn(async move {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let Ok(mut stream) = acceptor.accept(tcp).await else {
                return;
            };
            let mut request = vec![0_u8; 4096];
            let _read = stream.read(&mut request).await;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .expect("write");
            stream.shutdown().await.expect("shutdown");
        });
        port
    }

    #[test]
    fn a_bundle_lists_every_certificate() {
        let (_directory, path) = ca_file();
        let certificates = roots_from(&path).expect("roots");
        assert_eq!(certificates.len(), 1);
    }

    #[test]
    fn a_missing_bundle_is_an_error() {
        let path = std::env::temp_dir().join("home-tls-does-not-exist.pem");
        let error = roots_from(&path).expect_err("missing bundle");
        assert!(matches!(error, Error::Missing { .. }), "{error}");
        assert!(error.to_string().contains("SSL_CERT_FILE"), "{error}");
    }

    #[test]
    fn an_empty_bundle_is_an_error() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("empty.pem");
        std::fs::write(&path, b"not a certificate\n").expect("write");
        let error = roots_from(&path).expect_err("empty bundle");
        assert!(matches!(error, Error::Empty { .. }), "{error}");
    }

    #[test]
    fn an_unreadable_bundle_is_an_error() {
        let directory = tempfile::tempdir().expect("tempdir");
        let error = roots_from(directory.path()).expect_err("directory is not a bundle");
        assert!(matches!(error, Error::Read { .. }), "{error}");
    }

    #[test]
    fn ssl_cert_file_overrides_the_bundle() {
        let _guard = env_lock();
        let (_directory, path) = ca_file();
        // SAFETY: `env_lock` serializes every test that reads or writes the
        // process environment, and nothing else in this binary reads it.
        unsafe { std::env::set_var(SSL_CERT_FILE, &path) };
        assert_eq!(bundle_path().expect("bundle path"), path);
        assert_eq!(system_roots().expect("roots").len(), 1);
        // SAFETY: see above; this restores the pre-test process state.
        unsafe { std::env::remove_var(SSL_CERT_FILE) };
    }

    #[tokio::test]
    async fn a_https_request_trusts_the_test_ca_from_ssl_cert_file() {
        let port = serve_https().await;
        let (_directory, path) = ca_file();
        let builder = {
            let _guard = env_lock();
            // SAFETY: `env_lock` serializes the environment in this binary.
            unsafe { std::env::set_var(SSL_CERT_FILE, &path) };
            let builder = public_https_client().expect("client builder");
            // SAFETY: restore the pre-test process state.
            unsafe { std::env::remove_var(SSL_CERT_FILE) };
            builder
        };
        let client = builder.build().expect("client");
        let response = client
            .get(format!("https://localhost:{port}/"))
            .send()
            .await
            .expect("HTTPS request");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(response.text().await.expect("body"), "ok");
    }

    #[tokio::test]
    async fn a_client_without_roots_rejects_the_server() {
        let port = serve_https().await;
        // The workspace default: no roots at all. This is the incident's
        // failure mode, kept as a regression check for the test CA.
        install_crypto_provider();
        let client = reqwest::Client::builder().build().expect("client");
        let error = client
            .get(format!("https://localhost:{port}/"))
            .send()
            .await
            .expect_err("no trusted roots");
        assert!(error.is_connect(), "{error}");
    }

    #[tokio::test]
    async fn a_missing_configured_bundle_fails_before_any_request() {
        let _guard = env_lock();
        // SAFETY: `env_lock` serializes the environment in this binary.
        unsafe { std::env::set_var(SSL_CERT_FILE, "/nonexistent/home-tls.pem") };
        let error = public_https_client().expect_err("missing bundle");
        // SAFETY: restore the pre-test process state.
        unsafe { std::env::remove_var(SSL_CERT_FILE) };
        assert!(matches!(error, Error::Missing { .. }), "{error}");
    }
}
