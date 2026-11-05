//! Локальный HTTPS-сервер для тестов клиента и установщика: rustls с тестовым CA из
//! tests/data. Файлы вшиты в тестовый бинарь (`include_str!`), поэтому нет двойственности
//! CARGO_MANIFEST_DIR / runfiles.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use rustls::server::Acceptor;
use rustls::{ProtocolVersion, ServerConfig, SupportedProtocolVersion};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::LazyConfigAcceptor;

const SERVER_PEM: &str = include_str!("../tests/data/server.pem");
const SERVER_KEY: &str = include_str!("../tests/data/server.key");
const CA_PEM: &str = include_str!("../tests/data/ca.pem");

/// Путь к тестовому CA на диске (клиент читает бандл из файла, как на станции).
pub fn ca_file() -> PathBuf {
    static CA: OnceLock<PathBuf> = OnceLock::new();
    CA.get_or_init(|| {
        let path =
            std::env::temp_dir().join(format!("pulsar-update-test-ca-{}.pem", std::process::id()));
        std::fs::write(&path, CA_PEM).expect("write test CA");
        path
    })
    .clone()
}

/// Что соединение делает после ответа.
#[derive(Clone, Copy)]
pub enum End {
    /// TLS close_notify, затем закрыть.
    Close,
    /// Закрыть без close_notify.
    Drop,
    /// Держать соединение открытым, ничего не присылая.
    Hang,
}

#[derive(Clone)]
pub struct Reply {
    pub bytes: Vec<u8>,
    pub end: End,
}

pub fn ok(body: &[u8]) -> Reply {
    let mut bytes =
        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(body);
    Reply {
        bytes,
        end: End::Close,
    }
}

pub fn redirect(location: &str) -> Reply {
    raw(&format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 5\r\n\r\nmoved"
    ))
}

pub fn raw(text: &str) -> Reply {
    Reply {
        bytes: text.as_bytes().to_vec(),
        end: End::Close,
    }
}

pub type Session = (Option<String>, Option<ProtocolVersion>);

pub struct Server {
    pub port: u16,
    /// Заголовки запросов в порядке прихода.
    pub requests: Arc<std::sync::Mutex<Vec<String>>>,
    /// SNI и согласованная версия каждого завершённого handshake.
    pub sessions: Arc<std::sync::Mutex<Vec<Session>>>,
    routes: Arc<std::sync::Mutex<HashMap<String, Reply>>>,
}

impl Server {
    /// TLS 1.2 и 1.3 на 127.0.0.1 из задачи текущего LocalSet; неизвестный путь — 404.
    pub async fn start() -> Self {
        Self::start_with(rustls::DEFAULT_VERSIONS).await
    }

    pub async fn start_with(versions: &[&'static SupportedProtocolVersion]) -> Self {
        let certs: Vec<_> = CertificateDer::pem_slice_iter(SERVER_PEM.as_bytes())
            .collect::<Result<_, _>>()
            .expect("server.pem");
        let key = PrivateKeyDer::from_pem_slice(SERVER_KEY.as_bytes()).expect("server.key");
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(versions)
            .expect("protocol versions")
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .expect("server certificate");
        let config = Arc::new(config);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        let server = Self {
            port,
            requests: Arc::default(),
            sessions: Arc::default(),
            routes: Arc::default(),
        };
        let (requests, sessions, routes) = (
            server.requests.clone(),
            server.sessions.clone(),
            server.routes.clone(),
        );
        tokio::task::spawn_local(async move {
            loop {
                let Ok((tcp, _)) = listener.accept().await else {
                    return;
                };
                let (config, requests, sessions, routes) = (
                    config.clone(),
                    requests.clone(),
                    sessions.clone(),
                    routes.clone(),
                );
                tokio::task::spawn_local(async move {
                    let acceptor = LazyConfigAcceptor::new(Acceptor::default(), tcp);
                    let Ok(start) = acceptor.await else {
                        return;
                    };
                    let sni = start.client_hello().server_name().map(str::to_owned);
                    // Ошибка — клиент не принял сертификат.
                    let Ok(mut tls) = start.into_stream(config).await else {
                        return;
                    };
                    sessions
                        .lock()
                        .expect("sessions")
                        .push((sni, tls.get_ref().1.protocol_version()));
                    let mut head = Vec::new();
                    let mut buf = [0u8; 4096];
                    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                        match tls.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => head.extend_from_slice(&buf[..n]),
                        }
                    }
                    let head = String::from_utf8_lossy(&head).into_owned();
                    let path = head.split(' ').nth(1).unwrap_or_default().to_owned();
                    requests.lock().expect("requests").push(head);
                    let reply = routes.lock().expect("routes").get(&path).cloned();
                    let reply = reply.unwrap_or_else(|| {
                        raw("HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nnot found")
                    });
                    if tls.write_all(&reply.bytes).await.is_err() || tls.flush().await.is_err() {
                        return;
                    }
                    match reply.end {
                        End::Close => {
                            let _ = tls.shutdown().await;
                        }
                        End::Drop => {}
                        End::Hang => std::future::pending::<()>().await,
                    }
                });
            }
        });
        server
    }

    pub fn route(&self, path: &str, reply: Reply) {
        self.routes
            .lock()
            .expect("routes")
            .insert(path.into(), reply);
    }

    pub fn url(&self, path: &str) -> String {
        format!("https://localhost:{}{path}", self.port)
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("requests").clone()
    }
}
