//! Транспорт и слушающий сокет за трейтами: сервер не знает о `TcpStream`, тесты работают на
//! `tokio::io::duplex`.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::task::{Context, Poll};
use std::time::Duration;

use socket2::{Domain, SockRef, Socket, TcpKeepalive, Type};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

use crate::error::SpawnError;

/// Одно соединение. `into_split` отдаёт независимые половины для чтения и записи: клиентская
/// задача мультиплексирует их в одном `select!`.
pub trait Transport: 'static {
    fn into_split(self: Box<Self>) -> (Box<dyn AsyncRead + Unpin>, Box<dyn AsyncWrite + Unpin>);
}

impl<T> Transport for T
where
    T: AsyncRead + AsyncWrite + Unpin + 'static,
{
    fn into_split(self: Box<Self>) -> (Box<dyn AsyncRead + Unpin>, Box<dyn AsyncWrite + Unpin>) {
        let (read, write) = tokio::io::split(*self);
        (Box::new(read), Box::new(write))
    }
}

/// Источник соединений. Актор опрашивает его только когда готов принять клиента.
pub type Incoming = (Box<dyn Transport>, String);

pub trait Listener: 'static {
    fn poll_accept(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Incoming>>;
}

/// TCP-порт API (все IPv4-интерфейсы).
pub struct TcpListener {
    inner: tokio::net::TcpListener,
}

impl TcpListener {
    pub fn bind(port: u16) -> Result<Self, SpawnError> {
        let sock = Socket::new(Domain::IPV4, Type::STREAM, None).map_err(SpawnError::Socket)?;
        sock.set_nonblocking(true).map_err(SpawnError::Socket)?;
        let _ = sock.set_reuse_address(true);
        let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
        sock.bind(&addr.into())
            .and_then(|()| sock.listen(4))
            .map_err(|source| SpawnError::Bind { port, source })?;
        let inner = tokio::net::TcpListener::from_std(sock.into()).map_err(SpawnError::Socket)?;
        Ok(Self { inner })
    }
}

impl Listener for TcpListener {
    fn poll_accept(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Incoming>> {
        match self.inner.poll_accept(cx) {
            Poll::Ready(Ok((stream, addr))) => {
                configure_socket(&stream);
                Poll::Ready(Ok((Box::new(stream), addr.to_string())))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Wi-Fi обрывы оставляют полуоткрытые соединения без FIN. Keepalive находит мёртвого
/// пира примерно за 35 с, `TCP_USER_TIMEOUT` — зависшего (например, во время стрима
/// микрофона) за 30 с, заметно раньше ping-таймаута API.
fn configure_socket(stream: &TcpStream) {
    let s = SockRef::from(stream);
    let _ = s.set_tcp_nodelay(true);
    let keepalive = TcpKeepalive::new()
        .with_time(Duration::from_secs(20))
        .with_interval(Duration::from_secs(5))
        .with_retries(3);
    let _ = s.set_tcp_keepalive(&keepalive);
    #[cfg(target_os = "linux")]
    let _ = s.set_tcp_user_timeout(Some(Duration::from_millis(30_000)));
}

/// Удобный адаптер для тестов: слушатель из канала готовых транспортов.
#[cfg(test)]
pub struct ChannelListener {
    rx: tokio::sync::mpsc::UnboundedReceiver<Incoming>,
}

#[cfg(test)]
impl ChannelListener {
    pub fn new() -> (Self, tokio::sync::mpsc::UnboundedSender<Incoming>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { rx }, tx)
    }
}

#[cfg(test)]
impl Listener for ChannelListener {
    fn poll_accept(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Incoming>> {
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(item)) => Poll::Ready(Ok(item)),
            Poll::Ready(None) => Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe))),
            Poll::Pending => Poll::Pending,
        }
    }
}
