//! WebRTC-медиа на `str0m` (sans-IO) поверх одного UDP-сокета.
//!
//! Роль — `controlled` полноценного ICE-агента (ICE-lite недостаточно: телефон за VPN
//! приходит с другого адреса, и peer-reflexive кандидаты должен принимать именно агент).
//! DTLS-SRTP и ICE полностью внутри str0m; здесь только обвязка: SDP offer/answer, обмен
//! UDP-пакетами, запись Opus-кадров и события для сервиса.
//!
//! SDP и кандидаты не логируются. Ошибки — без содержимого пакетов.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Once};
use std::time::Instant;

use str0m::change::SdpOffer;
use str0m::media::{MediaKind, MediaTime, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc, RtcError};

use crate::ffi;

/// Событие медиа для сервиса.
#[derive(Debug)]
pub enum PeerOutput {
    /// ICE+DTLS установлены: можно писать/читать Opus.
    Connected,
    /// Сессия потеряна (сеть, DTLS, ICE).
    Disconnected,
    /// Пришёл Opus-пакет собеседника и его media time (48 кГц).
    Audio { data: Vec<u8>, time: u64 },
}

/// Ошибка медиа. Не содержит SDP, кандидатов и аудио.
#[derive(Debug, thiserror::Error)]
pub enum PeerError {
    #[error("call: invalid remote offer")]
    Offer,
    #[cfg(test)]
    #[error("call: invalid remote answer")]
    Answer,
    #[error("call: media negotiation failed")]
    Negotiation,
    #[error("call: invalid ICE candidate")]
    Candidate,
    #[error("call: RTC engine error")]
    Engine,
    #[error("call: UDP socket: {0}")]
    Io(#[from] io::Error),
}

impl From<RtcError> for PeerError {
    fn from(_: RtcError) -> Self {
        Self::Engine
    }
}

static CRYPTO: Once = Once::new();

/// UDP-сокет одного локального кандидата.
struct Local {
    socket: Arc<tokio::net::UdpSocket>,
    addr: SocketAddr,
}

/// Приёмник пакетов всех кандидатов. Владеет клонами `Arc`, поэтому не держит заём `Peer`
/// и может жить в ветке `select!` рядом с изменяемым доступом к медиа.
#[derive(Clone)]
pub struct Receiver {
    sockets: Vec<Arc<tokio::net::UdpSocket>>,
}

impl Receiver {
    /// Пакет с любого интерфейса и его источник.
    pub async fn recv(&self) -> io::Result<(Vec<u8>, SocketAddr)> {
        let futures = self.sockets.iter().map(|socket| {
            let socket = Arc::clone(socket);
            Box::pin(async move {
                let mut buf = vec![0u8; 2048];
                let result = socket.recv_from(&mut buf).await;
                (result, buf)
            })
        });
        let ((result, mut buf), _, _) = futures_util::future::select_all(futures).await;
        let (n, source) = result?;
        buf.truncate(n);
        Ok((buf, source))
    }
}

/// Один RTC-сеанс. Сокеты живут здесь же: второй агент на станции не нужен.
pub struct Peer {
    rtc: Rtc,
    locals: Vec<Local>,
    local_addr: SocketAddr,
    mid: Option<Mid>,
    #[cfg(test)]
    pending_offer: Option<str0m::change::SdpPendingOffer>,
    samples: u64,
    connected: bool,
}

impl Peer {
    /// Открыть UDP на всех интерфейсах (`0.0.0.0:0`) и добавить host-кандидаты по каждому
    /// адресу: телефон приходит либо по LAN, либо по VPN, и какой из них доступен, знает
    /// только ICE.
    pub async fn bind() -> Result<Self, PeerError> {
        Self::bind_addrs(ffi::ipv4_addrs()).await
    }

    /// Peer с явным списком адресов: тесты гоняют только loopback.
    async fn bind_addrs(addrs: Vec<Ipv4Addr>) -> Result<Self, PeerError> {
        CRYPTO.call_once(|| str0m::crypto::from_feature_flags().install_process_default());
        let mut rtc = Rtc::builder().build(Instant::now());
        let mut locals = Vec::new();
        for ip in addrs {
            let addr = SocketAddr::from((ip, 0));
            let Ok(socket) = tokio::net::UdpSocket::bind(addr).await else {
                continue;
            };
            let Ok(local) = socket.local_addr() else {
                continue;
            };
            if let Ok(candidate) = Candidate::host(local, "udp") {
                rtc.add_local_candidate(candidate);
                locals.push(Local {
                    socket: Arc::new(socket),
                    addr: local,
                });
            }
        }
        if locals.is_empty() {
            let socket = tokio::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await?;
            let local = socket.local_addr()?;
            let candidate = Candidate::host(local, "udp").map_err(|_| PeerError::Candidate)?;
            rtc.add_local_candidate(candidate);
            locals.push(Local {
                socket: Arc::new(socket),
                addr: local,
            });
        }
        let local_addr = locals[0].addr;
        Ok(Self {
            rtc,
            locals,
            local_addr,
            mid: None,
            #[cfg(test)]
            pending_offer: None,
            samples: 0,
            connected: false,
        })
    }

    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Тестовый peer только на loopback: детерминированный ICE без LAN/VPN.
    #[cfg(test)]
    pub async fn bind_loopback() -> Result<Self, PeerError> {
        Self::bind_addrs(vec![Ipv4Addr::LOCALHOST]).await
    }

    /// Принять offer и вернуть answer (с локальными кандидатами в SDP).
    pub fn accept_offer(&mut self, sdp: &str) -> Result<String, PeerError> {
        let offer = SdpOffer::from_sdp_string(sdp).map_err(|_| PeerError::Offer)?;
        let answer = self
            .rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|_| PeerError::Offer)?;
        Ok(answer.to_sdp_string())
    }

    /// Создать offer (только для тестов loopback и будущего «колонка звонит»).
    #[cfg(test)]
    pub fn create_offer(&mut self) -> Result<String, PeerError> {
        let mut change = self.rtc.sdp_api();
        let mid = change.add_media(
            MediaKind::Audio,
            str0m::media::Direction::SendRecv,
            None,
            None,
            None,
        );
        let (offer, pending) = change.apply().ok_or(PeerError::Negotiation)?;
        // Offerer не получает `MediaAdded` (событие только у answerer'а): mid известен сразу.
        self.mid = Some(mid);
        self.pending_offer = Some(pending);
        Ok(offer.to_sdp_string())
    }

    /// Применить answer к ранее созданному offer.
    #[cfg(test)]
    pub fn accept_answer(&mut self, sdp: &str) -> Result<(), PeerError> {
        let answer =
            str0m::change::SdpAnswer::from_sdp_string(sdp).map_err(|_| PeerError::Answer)?;
        let pending = self.pending_offer.take().ok_or(PeerError::Negotiation)?;
        self.rtc
            .sdp_api()
            .accept_answer(pending, answer)
            .map_err(|_| PeerError::Answer)?;
        Ok(())
    }

    /// Удалённый trickle-кандидат. Пустая строка и `a=candidate:` допустимы.
    pub fn add_remote_candidate(&mut self, candidate: &str) -> Result<(), PeerError> {
        let candidate = candidate.trim().trim_start_matches("a=");
        if candidate.is_empty() {
            return Ok(());
        }
        let parsed = Candidate::from_sdp_string(candidate).map_err(|_| PeerError::Candidate)?;
        self.rtc.add_remote_candidate(parsed);
        Ok(())
    }

    /// Один UDP-пакет в движок.
    pub fn handle_input(&mut self, packet: &[u8], source: SocketAddr) {
        let now = Instant::now();
        let Ok(contents) = packet.try_into() else {
            return;
        };
        let _ = self.rtc.handle_input(Input::Receive(
            now,
            Receive {
                proto: Protocol::Udp,
                source,
                destination: self.local_addr,
                contents,
            },
        ));
    }

    /// Дренаж `poll_output` до `Timeout`: отправляет пакеты в сокет и возвращает события.
    pub fn drain(&mut self) -> Result<(Vec<PeerOutput>, Instant), PeerError> {
        let mut events = Vec::new();
        let deadline = loop {
            match self.rtc.poll_output()? {
                Output::Timeout(t) => break t,
                Output::Transmit(t) => {
                    // Сокет неблокирующий: потеря одного пакета ICE/RTCP не фатальна.
                    let local = self
                        .locals
                        .iter()
                        .find(|l| l.addr == t.source)
                        .unwrap_or(&self.locals[0]);
                    let _ = local.socket.try_send_to(&t.contents, t.destination);
                }
                Output::Event(event) => match event {
                    Event::Connected => {
                        self.connected = true;
                        events.push(PeerOutput::Connected);
                    }
                    Event::IceConnectionStateChange(state) => {
                        if state == IceConnectionState::Disconnected {
                            self.connected = false;
                            events.push(PeerOutput::Disconnected);
                        }
                    }
                    Event::MediaAdded(added) => {
                        if added.kind == MediaKind::Audio {
                            self.mid = Some(added.mid);
                        }
                    }
                    Event::MediaData(data) => events.push(PeerOutput::Audio {
                        data: data.data.to_vec(),
                        time: data.time.numer(),
                    }),
                    _ => {}
                },
            }
        };
        Ok((events, deadline))
    }

    /// Продвинуть таймеры до `now`.
    pub fn handle_timeout(&mut self, now: Instant) {
        let _ = self.rtc.handle_input(Input::Timeout(now));
    }

    /// Записать один Opus-кадр (20 мс) в активную аудио-секцию.
    pub fn write_audio(&mut self, packet: &[u8], wallclock: Instant) -> Result<(), PeerError> {
        let Some(mid) = self.mid else {
            return Err(PeerError::Negotiation);
        };
        let Some(writer) = self.rtc.writer(mid) else {
            return Err(PeerError::Negotiation);
        };
        let params = writer.payload_params().next();
        let Some(params) = params else {
            return Err(PeerError::Negotiation);
        };
        let pt = params.pt();
        let media_time = MediaTime::new(self.samples, str0m::media::Frequency::FORTY_EIGHT_KHZ);
        writer
            .write(pt, wallclock, media_time, packet.to_vec())
            .map_err(|_| PeerError::Negotiation)?;
        self.samples += u64::from(crate::FRAME_SAMPLES as u32);
        Ok(())
    }

    /// Закрыть сессию (сервер или телефон положил трубку).
    pub fn close(&mut self) {
        let _ = self.rtc.close();
        let _ = self.drain();
    }

    /// Приёмник для `select!`: клоны `Arc`, без заёма `Peer`.
    #[must_use]
    pub fn receiver(&self) -> Receiver {
        Receiver {
            sockets: self.locals.iter().map(|l| Arc::clone(&l.socket)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::Codec as _;

    /// Два агента в одном процессе: полный ICE + DTLS + Opus-кадр по loopback.
    #[tokio::test]
    async fn two_peers_connect_and_exchange_audio_frames() {
        let mut caller = Peer::bind_loopback().await.unwrap();
        let mut callee = Peer::bind_loopback().await.unwrap();

        let offer = caller.create_offer().unwrap();
        let answer = callee.accept_offer(&offer).unwrap();
        caller.accept_answer(&answer).unwrap();

        // Кандидаты уже в SDP (host), trickle не нужен.
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let caller_rx = caller.receiver();
        let callee_rx = callee.receiver();
        let mut caller_connected = false;
        let mut callee_connected = false;
        let pump = |peer: &mut Peer| -> bool {
            let mut connected = false;
            for _ in 0..3 {
                let Ok((events, _)) = peer.drain() else {
                    return false;
                };
                connected |= events.iter().any(|e| matches!(e, PeerOutput::Connected));
                peer.handle_timeout(Instant::now());
            }
            connected
        };
        while Instant::now() < deadline && !(caller_connected && callee_connected) {
            caller_connected |= pump(&mut caller);
            callee_connected |= pump(&mut callee);
            if let Ok(Ok((packet, src))) =
                tokio::time::timeout(std::time::Duration::from_millis(10), caller_rx.recv()).await
            {
                caller.handle_input(&packet, src);
            }
            if let Ok(Ok((packet, src))) =
                tokio::time::timeout(std::time::Duration::from_millis(10), callee_rx.recv()).await
            {
                eprintln!("callee <- {} bytes", packet.len());
                callee.handle_input(&packet, src);
            } else {
                eprintln!("callee recv timeout");
            }
        }
        assert!(
            caller_connected && callee_connected,
            "ICE/DTLS did not converge in the loopback test (caller={caller_connected} callee={callee_connected})"
        );

        // События, накопившиеся к моменту Connected (MediaAdded), добираем перед записью.
        caller_connected |= pump(&mut caller);
        callee_connected |= pump(&mut callee);
        let _ = (caller_connected, callee_connected);

        // Пишем кадр и убеждаемся, что он доходит до второго агента как MediaData.
        // Настоящий Opus-кадр: str0m разбирает TOC для пакетизации.
        let mut codec = crate::codec::OpusCodec::new().unwrap();
        let frame = codec
            .encode(&vec![0i16; crate::FRAME_SAMPLES])
            .expect("opus encode");
        caller.write_audio(&frame, Instant::now()).unwrap();
        caller.handle_timeout(Instant::now());
        let _ = caller.drain();
        let mut got = None;
        for _ in 0..50 {
            let _ = caller.drain();
            if let Ok(Ok((packet, src))) =
                tokio::time::timeout(std::time::Duration::from_millis(10), callee_rx.recv()).await
            {
                callee.handle_input(&packet, src);
            }
            let Ok((events, _)) = callee.drain() else {
                continue;
            };
            if let Some(PeerOutput::Audio { data, .. }) = events.into_iter().next() {
                got = Some(data);
                break;
            }
        }
        assert_eq!(got.as_deref(), Some(frame.as_slice()));
    }
}
