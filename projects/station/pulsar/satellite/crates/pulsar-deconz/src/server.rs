//! The Midi's Zigbee radio offered to Home Assistant (ZHA) and Zigbee2MQTT as a network
//! coordinator speaking the deCONZ serial protocol (ConBee/RaspBee) over TCP: ZHA (zigpy-deconz)
//! connects to `socket://<ip>:<port>`, Zigbee2MQTT to `tcp://<ip>:<port>` with adapter "deconz".
//! The port is advertised over mDNS as `_zigbee-coordinator._tcp` (radio_type=deconz), which ZHA
//! discovers by itself.
//!
//! One client at a time; a new connection replaces the previous one. The transport is any
//! [`AsyncRead`] + [`AsyncWrite`] connection from an [`Acceptor`], so tests drive the real server
//! over `tokio::io::duplex`.
//!
//! The server is driven by its owner (the Zigbee service): `w = server.next().await` followed by
//! `server.handle(w, &mut coord)`, and it is the coordinator's [`Hooks`].

use std::collections::{BTreeMap, VecDeque};
use std::future::poll_fn;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::task::{Context, Poll};

use pulsar_zbhci::bytes::{get_le16, get_le64, put_le16, put_le64};
use socket2::{Domain, SockRef, Socket, Type};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf, ReadHalf, WriteHalf};
use tokio::net::TcpListener;
use tokio_util::bytes::{Buf, BytesMut};
use tokio_util::codec::{Decoder, Encoder};
use tracing::{debug, error, info, warn};

use crate::host::{Change, Formation, HostView};
use crate::params::{self, INVALID_VALUE, SUCCESS};
use crate::slip::{SlipCodec, checksum};
use crate::types::{ApsFrame, Hooks, Ncp};

// Commands.
pub const APS_DATA_CONFIRM: u8 = 0x04;
pub const DEVICE_STATE: u8 = 0x07;
pub const CHANGE_NETWORK_STATE: u8 = 0x08;
pub const READ_PARAMETER: u8 = 0x0A;
pub const WRITE_PARAMETER: u8 = 0x0B;
pub const VERSION: u8 = 0x0D;
pub const DEVICE_STATE_CHANGED: u8 = 0x0E;
pub const APS_DATA_REQUEST: u8 = 0x12;
pub const APS_DATA_INDICATION: u8 = 0x17;

pub const FAILURE: u8 = 1;
const FLAG_CONFIRM: u8 = 0x04;
const FLAG_INDICATION: u8 = 0x08;
const FLAG_FREE_SLOTS: u8 = 0x20;

/// ConBee III (platform 0x09) firmware 0x26780900: zigpy-deconz then scans energy with ZDO
/// Mgmt_NWK_Update_req to the coordinator (answered by the coordinator) and takes the values as
/// they are.
const FIRMWARE_VERSION: u32 = 0x2678_0900;
const MAX_QUEUED: usize = 64;
/// Sends awaiting confirm before "no free slots".
const MAX_IN_FLIGHT: usize = 8;
/// Replies waiting for a client that does not read: beyond this it is dropped (the radio's
/// traffic must not pile up in the daemon's memory).
const OUT_MAX: usize = 64 << 10;
/// Reads per wake-up; more input waits for the next one, after the radio's events.
const READS_PER_WAKE: usize = 4;
/// Frames waiting in the link's queue beyond which APS requests are refused.
const LINK_QUEUE_MAX: usize = 16;
/// Accepting is paused this long after an accept error for want of descriptors or memory: the
/// listener stays ready, so retrying at once would spin.
const ACCEPT_RETRY_MS: u64 = 500;

/// One client connection: the halves let the read and the write branch of [`Server::next`]
/// wait at the same time.
struct Client {
    read: ReadHalf<Conn>,
    write: WriteHalf<Conn>,
}

impl Client {
    fn new(conn: Conn) -> Self {
        let (read, write) = tokio::io::split(conn);
        Self { read, write }
    }
}

/// The deCONZ server's connection type.
pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Unpin {}
impl<T: AsyncRead + AsyncWrite + Unpin> AsyncReadWrite for T {}
pub type Conn = Box<dyn AsyncReadWrite>;

/// A source of client connections (TCP in production, a duplex queue in tests).
pub trait Acceptor {
    fn poll_accept(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<(Conn, SocketAddr)>>;
    fn local_addr(&self) -> Option<SocketAddr>;
}

/// Listens on the port (any address, backlog 2).
pub struct TcpAcceptor(TcpListener);

impl TcpAcceptor {
    pub fn bind(port: u16) -> io::Result<Self> {
        let sock = Socket::new(Domain::IPV4, Type::STREAM.nonblocking(), None)?;
        let _ = sock.set_reuse_address(true);
        let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
        sock.bind(&addr.into())?;
        sock.listen(2)?;
        Ok(Self(TcpListener::from_std(sock.into())?))
    }
}

impl Acceptor for TcpAcceptor {
    fn poll_accept(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<(Conn, SocketAddr)>> {
        match self.0.poll_accept(cx) {
            Poll::Ready(Ok((s, addr))) => {
                let _ = s.set_nodelay(true);
                let _ = SockRef::from(&s).set_keepalive(true);
                Poll::Ready(Ok((Box::new(s) as Conn, addr)))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        self.0.local_addr().ok()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Request {
    req_id: u8,
    /// deCONZ address mode: 1 group, 2 NWK, 3 IEEE.
    dst_mode: u8,
    /// NWK, group or IEEE.
    dst_addr: u64,
    dst_ep: u8,
    src_ep: u8,
}

#[derive(Clone, Copy, Debug)]
struct Confirm {
    req: Request,
    status: u8,
}

/// What woke the server; hand it back to [`Server::handle`].
pub struct Wake(WakeKind);

enum WakeKind {
    Accept(io::Result<(Conn, SocketAddr)>),
    AcceptResume,
    /// `n` bytes of the queue went out; `handle` drains them and flushes the rest.
    Wrote(usize),
    Read(Vec<u8>),
    ReadClosed,
    WriteFailed,
}

/// The outcome of an opportunistic read after the readable wake-up.
enum TryRead {
    Data(Vec<u8>),
    WouldBlock,
    Closed,
}

pub struct Server {
    port: u16,
    acceptor: Option<Box<dyn Acceptor>>,
    /// Accepting stopped until then after EMFILE & co.
    accept_paused_until: Option<tokio::time::Instant>,
    /// An accept error was logged; cleared by the next accepted connection.
    accept_failing: bool,
    client: Option<Client>,
    connected: bool,
    codec: SlipCodec,
    inbuf: BytesMut,
    out: BytesMut,
    /// Per client: confirms of a previous client are dropped.
    generation: u32,
    /// Device state last sent to the client.
    last_state: u8,
    /// The host's network view: written parameters, slots, formation.
    host: HostView,
    indications: VecDeque<ApsFrame>,
    confirms: VecDeque<Confirm>,
    /// Coordinator handle -> request.
    requests: BTreeMap<u32, Request>,
}

impl Server {
    /// A server with a ConBee's default endpoint slots; [`Server::attach`] gives them to the
    /// coordinator. It listens once [`Server::start`] is called.
    pub fn new(port: u16) -> Self {
        Self {
            port,
            acceptor: None,
            accept_paused_until: None,
            accept_failing: false,
            client: None,
            connected: false,
            codec: SlipCodec::default(),
            inbuf: BytesMut::new(),
            out: BytesMut::new(),
            generation: 0,
            last_state: 0xFF,
            host: HostView::new(),
            indications: VecDeque::new(),
            confirms: VecDeque::new(),
            requests: BTreeMap::new(),
        }
    }

    /// A server over `acceptor` instead of a TCP listener (tests).
    pub fn with_acceptor(port: u16, acceptor: impl Acceptor + 'static) -> Self {
        let mut server = Self::new(port);
        server.acceptor = Some(Box::new(acceptor));
        server
    }

    /// Sets the coordinator's endpoints to the slots' (before [`Ncp`] start).
    pub fn attach(&self, coord: &mut dyn Ncp) {
        self.host.attach(coord);
    }

    /// Listens on the port. Errors are logged.
    pub fn start(&mut self) -> bool {
        // An acceptor provided by the embedder is the listener: do not replace it.
        if self.acceptor.is_some() {
            return true;
        }
        match TcpAcceptor::bind(self.port) {
            Ok(acceptor) => {
                self.acceptor = Some(Box::new(acceptor));
                info!("zigbee: deCONZ coordinator on :{}", self.port);
                true
            }
            Err(e) => {
                error!(
                    "zigbee: bind/listen :{}: {}",
                    self.port,
                    home_linux::io_error_text(&e)
                );
                false
            }
        }
    }

    /// The port actually listened on (tests bind port 0).
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.acceptor.as_ref().and_then(|a| a.local_addr())
    }

    /// A client is connected.
    pub fn has_client(&self) -> bool {
        self.connected
    }

    /// A connection, client input, or room to write. Cancel-safe: the write branch reports
    /// exactly the bytes it took as `Wrote(n)`, and a cancelled future is never the one that
    /// wrote them (the `select!` returns that branch). [`Server::handle`] drains the count.
    pub async fn next(&mut self) -> Wake {
        let paused = self.accept_paused_until;
        let mut read_buf = [0u8; 1024];
        let Self {
            acceptor,
            client,
            out,
            ..
        } = self;
        let want_write = !out.is_empty();
        let (mut read, mut write) = match client.as_mut() {
            Some(c) => (Some(&mut c.read), Some(&mut c.write)),
            None => (None, None),
        };
        tokio::select! {
            biased;
            () = async { tokio::time::sleep_until(paused.unwrap()).await }, if paused.is_some() => {
                Wake(WakeKind::AcceptResume)
            }
            r = poll_fn(|cx| poll_write_some(write.as_mut().unwrap(), out, cx)),
                if write.is_some() && want_write => Wake(r),
            r = async {
                poll_fn(|cx| acceptor.as_mut().unwrap().poll_accept(cx)).await
            }, if acceptor.is_some() && paused.is_none() => Wake(WakeKind::Accept(r)),
            r = poll_fn(|cx| poll_readable(read.as_mut().unwrap(), &mut read_buf, cx)),
                if read.is_some() => Wake(r),
        }
    }

    pub fn handle(&mut self, w: Wake, coord: &mut dyn Ncp) {
        match w.0 {
            WakeKind::Accept(Ok((conn, addr))) => {
                self.accept_failing = false;
                self.on_accept(coord, conn, addr);
            }
            WakeKind::Accept(Err(e)) => self.on_accept_error(&e),
            WakeKind::AcceptResume => self.accept_paused_until = None,
            WakeKind::Wrote(n) => {
                self.out.advance(n);
                self.flush();
            }
            WakeKind::Read(bytes) => self.on_readable(coord, &bytes),
            WakeKind::ReadClosed => {
                info!("zigbee: client disconnected");
                self.close_client();
            }
            WakeKind::WriteFailed => self.close_client(),
        }
        // Replies of the whole batch go out together (and hooks' notifications with the next
        // wake-up, whose write branch is on while output waits).
        self.flush();
    }

    fn on_accept_error(&mut self, e: &io::Error) {
        let exhausted = matches!(
            e.raw_os_error(),
            Some(libc::EMFILE | libc::ENFILE | libc::ENOBUFS | libc::ENOMEM)
        );
        if !exhausted {
            return; // that connection only
        }
        if !self.accept_failing {
            warn!("zigbee: accept: {}", home_linux::io_error_text(e));
        }
        self.accept_failing = true;
        self.accept_paused_until =
            Some(tokio::time::Instant::now() + std::time::Duration::from_millis(ACCEPT_RETRY_MS));
    }

    fn on_accept(&mut self, coord: &mut dyn Ncp, conn: Conn, addr: SocketAddr) {
        // Until the radio has reported its network (start-up, or after it restarted), no host is
        // served: one told "no network" would form one, which erases the radio. zigpy and
        // herdsman retry a refused connection; error statuses would break zigpy-deconz (it
        // asserts UNSUPPORTED).
        if !coord.ready() {
            return;
        }
        if self.connected {
            info!("zigbee: new client replaces the previous one");
            self.close_client();
        }
        info!("zigbee: client {} connected", addr.ip());
        self.begin_client(Some(Client::new(conn)));
    }

    fn begin_client(&mut self, client: Option<Client>) {
        self.client = client;
        self.connected = true;
        self.generation = self.generation.wrapping_add(1);
        self.host.forget_client();
        self.last_state = 0xFF;
        self.codec = SlipCodec::default();
        self.inbuf.clear();
        self.out.clear();
        self.indications.clear();
        self.confirms.clear();
    }

    fn close_client(&mut self) {
        if !self.connected {
            return;
        }
        self.connected = false; // a write error inside flush() must not come back here
        // What was already answered goes out first (C++ wrote every reply at once).
        self.flush();
        self.client = None;
        self.out.clear();
        self.inbuf.clear();
        self.indications.clear();
        self.confirms.clear();
    }

    fn flush(&mut self) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let mut failed = false;
        while !self.out.is_empty() {
            let mut cx = Context::from_waker(std::task::Waker::noop());
            match Pin::new(&mut client.write).poll_write(&mut cx, &self.out) {
                Poll::Ready(Ok(n @ 1..)) => {
                    self.out.advance(n);
                }
                Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {}
                // WouldBlock: the next wake-up writes the rest.
                Poll::Pending => break,
                Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::WouldBlock => break,
                Poll::Ready(Ok(_)) | Poll::Ready(Err(_)) => {
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            self.close_client();
        }
    }

    /// Client bytes after the readable wake-up: up to [`READS_PER_WAKE`] KiB, so the radio's
    /// events wait for at most one batch.
    fn on_readable(&mut self, coord: &mut dyn Ncp, first: &[u8]) {
        if !first.is_empty() {
            self.on_bytes(coord, first);
        }
        for _ in 1..READS_PER_WAKE {
            if !self.connected {
                return;
            }
            match self.try_read() {
                TryRead::Data(bytes) => self.on_bytes(coord, &bytes),
                TryRead::WouldBlock => return,
                TryRead::Closed => {
                    info!("zigbee: client disconnected");
                    self.close_client();
                    return;
                }
            }
        }
    }

    /// One non-blocking read (the C++ loop after the readiness wake-up).
    fn try_read(&mut self) -> TryRead {
        let Some(client) = self.client.as_mut() else {
            return TryRead::Closed;
        };
        let mut buf = [0u8; 1024];
        let mut cx = Context::from_waker(std::task::Waker::noop());
        let mut rb = ReadBuf::new(&mut buf);
        match Pin::new(&mut client.read).poll_read(&mut cx, &mut rb) {
            Poll::Ready(Ok(())) if rb.filled().is_empty() => TryRead::Closed,
            Poll::Ready(Ok(())) => TryRead::Data(rb.filled().to_vec()),
            Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {
                TryRead::Data(Vec::new())
            }
            Poll::Ready(Err(_)) => TryRead::Closed,
            Poll::Pending => TryRead::WouldBlock,
        }
    }

    /// Client bytes: SLIP frames end at 0xC0.
    pub(crate) fn on_bytes(&mut self, coord: &mut dyn Ncp, bytes: &[u8]) {
        self.inbuf.extend_from_slice(bytes);
        loop {
            let Ok(Some(frame)) = self.codec.decode(&mut self.inbuf) else {
                return;
            };
            if !self.connected {
                return;
            }
            self.on_slip(coord, &frame);
            if !self.connected {
                return;
            }
        }
    }

    fn on_slip(&mut self, coord: &mut dyn Ncp, f: &[u8]) {
        // Too short or a bad checksum: noise, or the EZSP/ZNP probes ZHA sends to unknown radios.
        if f.len() < 7 {
            return;
        }
        let body = f.len() - 2;
        if checksum(&f[..body]) != get_le16(&f[body..]) {
            return;
        }
        self.command(coord, f[0], f[1], &f[5..body]);
    }

    fn reply(&mut self, cmd: u8, seq: u8, status: u8, body: &[u8], payload_length: bool) {
        let extra = if payload_length { 2 } else { 0 };
        let mut f = vec![cmd, seq, status];
        // deCONZ frames are short (a 200-byte ASDU at most).
        put_le16(&mut f, (5 + extra + body.len()) as u16);
        if payload_length {
            put_le16(&mut f, body.len() as u16);
        }
        f.extend_from_slice(body);
        let sum = checksum(&f);
        put_le16(&mut f, sum);
        let mut bytes = BytesMut::new();
        if self.codec.encode(&f[..], &mut bytes).is_err() {
            return;
        }
        self.out.extend_from_slice(&bytes);
        if self.out.len() > OUT_MAX {
            self.flush();
        }
        if self.out.len() > OUT_MAX {
            self.close_client();
        }
    }

    fn device_state(&self, coord: &dyn Ncp) -> u8 {
        let mut s = self.host.network_state(coord);
        if !self.confirms.is_empty() {
            s |= FLAG_CONFIRM;
        }
        if !self.indications.is_empty() {
            s |= FLAG_INDICATION;
        }
        if coord.pending() < MAX_IN_FLIGHT && coord.link_queued() <= LINK_QUEUE_MAX {
            s |= FLAG_FREE_SLOTS;
        }
        s
    }

    fn notify_state(&mut self, coord: &dyn Ncp) {
        if !self.connected {
            return;
        }
        let s = self.device_state(coord);
        if s == self.last_state {
            return;
        }
        self.last_state = s;
        // seq 0: zigpy numbers its requests 1..255, so this never matches one of them.
        self.reply(DEVICE_STATE_CHANGED, 0, SUCCESS, &[s, 0], false);
    }

    /// `p`: after [command][seq][status][frame length]; commands with a payload length start
    /// with it.
    fn command(&mut self, coord: &mut dyn Ncp, cmd: u8, seq: u8, p: &[u8]) {
        match cmd {
            VERSION => self.reply(cmd, seq, SUCCESS, &FIRMWARE_VERSION.to_le_bytes(), false),
            DEVICE_STATE => {
                self.last_state = self.device_state(coord);
                let s = self.last_state;
                self.reply(cmd, seq, SUCCESS, &[s, 0, 0], false);
            }
            CHANGE_NETWORK_STATE => {
                if let Some(&target) = p.first() {
                    self.change_network_state(coord, seq, target);
                }
            }
            READ_PARAMETER if p.len() >= 3 => self.read_parameter(coord, seq, &p[2..]),
            WRITE_PARAMETER if p.len() >= 3 => self.write_parameter(coord, seq, &p[2..]),
            APS_DATA_REQUEST if p.len() >= 2 => {
                self.aps_data_request(coord, seq, &p[2..], usize::from(get_le16(p)));
            }
            APS_DATA_INDICATION => self.reply_indication(coord, seq),
            APS_DATA_CONFIRM => self.reply_confirm(coord, seq),
            READ_PARAMETER | WRITE_PARAMETER | APS_DATA_REQUEST => {}
            _ => debug!("zigbee: deCONZ command 0x{cmd:02x} not supported"),
        }
    }

    fn read_parameter(&mut self, coord: &mut dyn Ncp, seq: u8, p: &[u8]) {
        let id = p[0];
        let (status, value) = params::read(id, p, coord, &self.host);
        let mut body = vec![id];
        body.extend_from_slice(&value);
        self.reply(READ_PARAMETER, seq, status, &body, true);
    }

    fn write_parameter(&mut self, coord: &mut dyn Ncp, seq: u8, p: &[u8]) {
        let id = p[0];
        let status = params::write(id, &p[1..], coord, &mut self.host);
        self.reply(WRITE_PARAMETER, seq, status, &[id], true);
    }

    fn change_network_state(&mut self, coord: &mut dyn Ncp, seq: u8, target: u8) {
        self.host.change_network_state(coord, target);
        self.reply(CHANGE_NETWORK_STATE, seq, SUCCESS, &[target], false);
        self.on_changed(coord);
    }

    fn reply_indication(&mut self, coord: &mut dyn Ncp, seq: u8) {
        let Some(f) = self.indications.pop_front() else {
            return self.reply(APS_DATA_INDICATION, seq, FAILURE, &[], true);
        };
        self.last_state = self.device_state(coord);
        let mut b = vec![self.last_state];
        b.push(if f.group { 0x01 } else { 0x02 }); // group / NWK
        put_le16(&mut b, f.dst);
        b.push(f.dst_ep);
        b.push(0x02);
        put_le16(&mut b, f.src);
        b.push(f.src_ep);
        put_le16(&mut b, f.profile);
        put_le16(&mut b, f.cluster);
        // At most the radio's 0xF9-byte frame.
        put_le16(&mut b, f.data.len() as u16);
        b.extend_from_slice(&f.data);
        b.extend_from_slice(&[0, 0, f.lqi, 0, 0, 0, 0, f.rssi as u8]);
        self.reply(APS_DATA_INDICATION, seq, SUCCESS, &b, true);
    }

    fn reply_confirm(&mut self, coord: &mut dyn Ncp, seq: u8) {
        let Some(c) = self.confirms.pop_front() else {
            return self.reply(APS_DATA_CONFIRM, seq, FAILURE, &[], true);
        };
        self.last_state = self.device_state(coord);
        let mut b = vec![self.last_state, c.req.req_id, c.req.dst_mode];
        if c.req.dst_mode == 0x03 {
            put_le64(&mut b, c.req.dst_addr);
        } else {
            put_le16(&mut b, c.req.dst_addr as u16);
        }
        if c.req.dst_mode != 0x01 {
            b.push(c.req.dst_ep);
        }
        b.extend_from_slice(&[c.req.src_ep, c.status, 0, 0, 0, 0]);
        self.reply(APS_DATA_CONFIRM, seq, SUCCESS, &b, true);
    }

    /// `p`: [req id][flags][dst mode][dst][dst ep unless group][profile][cluster][src ep]
    /// [asdu len][asdu][tx options][radius][relays if flags & 2]; `declared`: the payload length
    /// the frame states (may differ from `p.len()`).
    fn aps_data_request(&mut self, coord: &mut dyn Ncp, seq: u8, p: &[u8], declared: usize) {
        match parse_aps_request(p, declared) {
            Ok((request, frame)) => self.send_aps_request(coord, seq, request, frame),
            Err((status, req_id)) => self.reply_aps_refused(coord, seq, status, req_id),
        }
    }

    fn reply_aps_refused(&mut self, coord: &mut dyn Ncp, seq: u8, status: u8, req_id: u8) {
        let st = self.device_state(coord);
        self.reply(APS_DATA_REQUEST, seq, status, &[st, req_id], true);
    }

    fn send_aps_request(
        &mut self,
        coord: &mut dyn Ncp,
        seq: u8,
        request: Request,
        frame: ApsFrame,
    ) {
        // A host ignoring the "free slots" flag must not fill the radio's queue (or the daemon's
        // memory) with requests.
        if coord.pending() >= MAX_IN_FLIGHT || coord.link_queued() > LINK_QUEUE_MAX {
            return self.reply_aps_refused(coord, seq, FAILURE, request.req_id);
        }
        let handle = self.generation << 8 | u32::from(request.req_id);
        self.requests.insert(handle, request);
        let ieee = if request.dst_mode == 0x03 {
            request.dst_addr
        } else {
            0
        };
        if !coord.send(handle, frame, ieee) {
            self.requests.remove(&handle);
            return self.reply_aps_refused(coord, seq, FAILURE, request.req_id);
        }
        let st = self.device_state(coord);
        self.reply(APS_DATA_REQUEST, seq, SUCCESS, &[st, request.req_id], true);
    }

    fn on_changed(&mut self, coord: &mut dyn Ncp) {
        match self.host.on_changed(coord) {
            Change::None => {}
            Change::Form(cfg) => {
                coord.form(&cfg, self);
                self.host.formation = Formation::Forming;
            }
            Change::DropClient => {
                if self.connected {
                    info!("zigbee: radio restarting, dropping the client");
                    self.close_client();
                }
            }
        }
        self.notify_state(coord);
    }
}

fn parse_aps_request(p: &[u8], declared: usize) -> Result<(Request, ApsFrame), (u8, u8)> {
    let n = p.len();
    if n < 3 {
        return Err((INVALID_VALUE, 0));
    }
    let mut r = Request {
        req_id: p[0],
        dst_mode: p[2],
        ..Request::default()
    };
    let mut i = 3;
    // zigbee-herdsman sends NWK_addr_req with NWK mode but the IEEE address in place of the NWK
    // one (the payload is 6 bytes longer than it declares): sent as the broadcast it is meant to
    // be.
    let herdsman_nwk_addr = r.dst_mode == 0x02 && n == declared + 6;
    if herdsman_nwk_addr {
        if n < i + 8 {
            return Err((INVALID_VALUE, r.req_id));
        }
        r.dst_addr = 0xFFFD;
        i += 8;
    } else if r.dst_mode == 0x01 || r.dst_mode == 0x02 {
        if n < i + 2 {
            return Err((INVALID_VALUE, r.req_id));
        }
        r.dst_addr = u64::from(get_le16(&p[i..]));
        i += 2;
    } else if r.dst_mode == 0x03 {
        if n < i + 8 {
            return Err((INVALID_VALUE, r.req_id));
        }
        r.dst_addr = get_le64(&p[i..]);
        i += 8;
    } else {
        return Err((INVALID_VALUE, r.req_id));
    }
    if r.dst_mode != 0x01 {
        if n < i + 1 {
            return Err((INVALID_VALUE, r.req_id));
        }
        r.dst_ep = p[i];
        i += 1;
    }
    if n < i + 7 {
        return Err((INVALID_VALUE, r.req_id));
    }
    let mut f = ApsFrame {
        profile: get_le16(&p[i..]),
        cluster: get_le16(&p[i + 2..]),
        src_ep: p[i + 4],
        ..ApsFrame::default()
    };
    r.src_ep = f.src_ep;
    let len = usize::from(get_le16(&p[i + 5..]));
    i += 7;
    if n < i + len {
        return Err((INVALID_VALUE, r.req_id));
    }
    f.data = p[i..i + len].to_vec();
    i += len;
    if n >= i + 2 {
        f.tx_options = p[i];
        f.radius = p[i + 1];
    }
    f.dst_ep = r.dst_ep;
    f.group = r.dst_mode == 0x01;
    f.dst = if r.dst_mode == 0x03 {
        0
    } else {
        r.dst_addr as u16
    };
    Ok((r, f))
}

impl Hooks for Server {
    fn received(&mut self, coord: &mut dyn Ncp, f: &ApsFrame) {
        if !self.connected || !self.host.online {
            return;
        }
        if self.indications.len() >= MAX_QUEUED {
            self.indications.pop_front();
        }
        self.indications.push_back(f.clone());
        self.notify_state(coord);
    }

    fn sent(&mut self, coord: &mut dyn Ncp, handle: u32, status: u8) {
        let Some(r) = self.requests.remove(&handle) else {
            return;
        };
        if handle >> 8 != self.generation || !self.connected {
            return; // a previous client's request
        }
        if self.confirms.len() >= MAX_QUEUED {
            self.confirms.pop_front();
        }
        self.confirms.push_back(Confirm { req: r, status });
        self.notify_state(coord);
    }

    fn changed(&mut self, coord: &mut dyn Ncp) {
        self.on_changed(coord);
    }

    fn formed(&mut self, coord: &mut dyn Ncp, ok: bool) {
        self.host.formed(ok);
        self.notify_state(coord);
    }
}

fn poll_write_some(
    write: &mut WriteHalf<Conn>,
    out: &[u8],
    cx: &mut Context<'_>,
) -> Poll<WakeKind> {
    match Pin::new(write).poll_write(cx, out) {
        Poll::Ready(Ok(n)) => Poll::Ready(WakeKind::Wrote(n)),
        Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {
            Poll::Ready(WakeKind::Wrote(0))
        }
        Poll::Ready(Err(_)) => Poll::Ready(WakeKind::WriteFailed),
        Poll::Pending => Poll::Pending,
    }
}

fn poll_readable(
    read: &mut ReadHalf<Conn>,
    read_buf: &mut [u8],
    cx: &mut Context<'_>,
) -> Poll<WakeKind> {
    let mut rb = ReadBuf::new(read_buf);
    match Pin::new(read).poll_read(cx, &mut rb) {
        Poll::Ready(Ok(())) if rb.filled().is_empty() => Poll::Ready(WakeKind::ReadClosed),
        Poll::Ready(Ok(())) => Poll::Ready(WakeKind::Read(rb.filled().to_vec())),
        Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {
            Poll::Ready(WakeKind::Read(Vec::new()))
        }
        Poll::Ready(Err(_)) => Poll::Ready(WakeKind::ReadClosed),
        Poll::Pending => Poll::Pending,
    }
}
