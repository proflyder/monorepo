//! The serial link to the Midi's Zigbee co-processor: zbhci framing, stop-and-wait ACKs,
//! retries and the reset/boot lines.
//!
//! The byte stream is any [`AsyncRead`] + [`AsyncWrite`]: the station's tty in production
//! ([`Link::open`]) and an in-memory duplex in tests ([`Link::with_io`]). The reset and boot
//! lines are behind [`RadioLines`]: sysfs in production ([`SysfsLines`]), a recording fake in
//! tests.
//!
//! The link's own timers (reset pulse, boot wait, ACK retries, holds) only advance while
//! [`Link::next`] is polled, so its owner polls it continuously from `open` on, as the C++ loop
//! always ran.

use std::future::poll_fn;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use pulsar_core::Timers;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::bytes::BytesMut;
use tokio_util::codec::Decoder;
use tracing::{error, info, warn};

use crate::frame::{ACK, BOOT_IND, FrameCodec, build_frame};

/// The ACK normally follows within a few ms.
const ACK_TIMEOUT_MS: u64 = 300;
const TRIES: u32 = 3;
const FAILURES_BEFORE_RESET: u32 = 3;
const RESET_PULSE_MS: u64 = 200;
const BOOT_WAIT_MS: u64 = 3000;

/// What the ACK of a sent frame concerns: the sender's continuation (the C++ `AckCb` closures).
/// Frames sent without a tag produce no [`LinkEvent::Ack`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AckTag {
    /// Coordinator 0x0A04 for this endpoint.
    Endpoint(u8),
    /// Coordinator 0x0A01 (form a network).
    Form,
    /// Coordinator 0x0A10 (APS data request) with this radio handle.
    Data(u8),
    /// Coordinator 0x0A06 (energy scan).
    EdScan,
    /// Router 0x0002 (network steering).
    Steer,
    /// Router 0x0005 (leave).
    Leave,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LinkEvent {
    /// Every frame from the chip except ACKs and boot indications.
    Frame { ty: u16, payload: Vec<u8> },
    /// ACK of a frame sent with a tag: 0 = accepted, > 0 firmware status (2 = unknown type),
    /// -1 = no ACK (retries exhausted, or dropped by a hardware reset).
    Ack { tag: AckTag, status: i32 },
    /// The chip (re)started its application: everything it held in RAM is gone.
    Boot,
}

/// The radio's reset and boot lines (the kernel's `zb_control` value files, active low: 1
/// asserts). Writes report whether they reached the line.
pub trait RadioLines {
    fn set_boot(&mut self, asserted: bool) -> bool;
    fn set_reset(&mut self, asserted: bool) -> bool;
    /// The lines as the log names them (the sysfs paths on the station).
    fn describe(&self) -> String;
}

/// The sysfs value files of the station's radio.
#[derive(Clone, Debug)]
pub struct SysfsLines {
    reset: String,
    boot: String,
}

impl SysfsLines {
    pub fn new(reset: impl Into<String>, boot: impl Into<String>) -> Self {
        Self {
            reset: reset.into(),
            boot: boot.into(),
        }
    }
}

impl RadioLines for SysfsLines {
    fn set_boot(&mut self, asserted: bool) -> bool {
        home_linux::write_file(&self.boot, if asserted { "1" } else { "0" })
    }

    fn set_reset(&mut self, asserted: bool) -> bool {
        home_linux::write_file(&self.reset, if asserted { "1" } else { "0" })
    }

    fn describe(&self) -> String {
        format!("{} / {}", self.reset, self.boot)
    }
}

/// Opening the tty and its hardware reset failed before any frame could be exchanged.
#[derive(Debug)]
pub enum RadioError {
    Open { path: String, source: io::Error },
    Configure { path: String, source: io::Error },
}

impl std::fmt::Display for RadioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open { path, source } => {
                write!(f, "open {path}: {}", home_linux::io_error_text(source))
            }
            Self::Configure { path, source } => {
                write!(f, "configure {path}: {}", home_linux::io_error_text(source))
            }
        }
    }
}

impl std::error::Error for RadioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open { source, .. } | Self::Configure { source, .. } => Some(source),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Timer {
    Ack,
    Hold,
    /// The lines could not be driven: carry on as if the chip had booted.
    BootNow,
    ResetPulse,
    BootWait,
}

struct Out {
    ty: u16,
    frame: Vec<u8>,
    tag: Option<AckTag>,
    hold_for: u16,
    hold_ms: u64,
    tries: u32,
}

type Io = Box<dyn AsyncReadWrite>;

trait AsyncReadWrite: AsyncRead + AsyncWrite + Unpin {}
impl<T: AsyncRead + AsyncWrite + Unpin> AsyncReadWrite for T {}

#[cfg(test)]
mod tests;

pub struct Link {
    wire: Io,
    tty: String,
    /// The wire hung up or failed: no more input, timers (ACKs, reset) keep running.
    read_dead: bool,
    lines: Box<dyn RadioLines>,
    timers: Timers<Timer>,
    codec: FrameCodec,
    rx: BytesMut,
    out: VecDeque<Out>,
    events: VecDeque<LinkEvent>,
    in_flight: bool,
    /// Reset pulse or boot wait in progress: nothing is sent.
    resetting: bool,
    /// Reset line asserted: a boot indication now predates the pulse.
    reset_held: bool,
    /// Frame type awaited before anything else is sent (0 = none).
    hold_for: u16,
    /// Consecutive frames without ACK.
    failures: u32,
}

use std::collections::VecDeque;

impl Link {
    /// Opens the tty and hardware-resets the chip; [`LinkEvent::Boot`] follows once its
    /// application runs. Must run inside the tokio runtime (the tty is registered with its
    /// reactor); from then on [`Link::next`] must be polled continuously, or the reset, ACK
    /// retries and holds do not advance.
    pub fn open(tty: &str, lines: Box<dyn RadioLines>) -> Result<Self, RadioError> {
        let port = home_linux::Tty::open(tty, true).map_err(|source| RadioError::Open {
            path: tty.to_owned(),
            source,
        })?;
        port.configure_raw_115200(true)
            .map_err(|source| RadioError::Configure {
                path: tty.to_owned(),
                source,
            })?;
        port.flush_io();
        let wire = port.into_async().map_err(|source| RadioError::Open {
            path: tty.to_owned(),
            source,
        })?;
        Ok(Self::with_io(wire, tty, lines))
    }

    /// A link over `wire` (tests: the radio's end is the other half of a duplex), reset as
    /// [`Link::open`] does.
    pub fn with_io(
        wire: impl AsyncRead + AsyncWrite + Unpin + 'static,
        tty: impl Into<String>,
        lines: Box<dyn RadioLines>,
    ) -> Self {
        let mut link = Self::new(Box::new(wire), tty.into(), lines);
        link.reset();
        link
    }

    fn new(wire: Io, tty: String, lines: Box<dyn RadioLines>) -> Self {
        Self {
            wire,
            tty,
            read_dead: false,
            lines,
            timers: Timers::new(),
            codec: FrameCodec::default(),
            rx: BytesMut::new(),
            out: VecDeque::new(),
            events: VecDeque::new(),
            in_flight: false,
            resetting: false,
            reset_held: false,
            hold_for: 0,
            failures: 0,
        }
    }

    /// Frames queued, the one in flight included.
    pub fn queued(&self) -> usize {
        self.out.len()
    }

    /// Queues a frame. With a tag, its ACK (or its absence) comes back as [`LinkEvent::Ack`].
    pub fn send(&mut self, ty: u16, payload: &[u8], tag: Option<AckTag>) {
        self.send_and_hold(ty, payload, tag, 0, 0);
    }

    /// As [`Link::send`]; after the ACK nothing else is sent until a frame of type `hold_for`
    /// arrives or `hold_ms` pass: for commands during which the firmware busy-waits and would
    /// miss further frames.
    pub fn send_and_hold(
        &mut self,
        ty: u16,
        payload: &[u8],
        tag: Option<AckTag>,
        hold_for: u16,
        hold_ms: u64,
    ) {
        self.out.push_back(Out {
            ty,
            frame: build_frame(ty, payload),
            tag,
            hold_for,
            hold_ms,
            tries: 0,
        });
        self.pump();
    }

    /// Hardware reset (reset line pulse with the boot line low). Queued frames fail with -1.
    pub fn reset(&mut self) {
        for t in [
            Timer::Ack,
            Timer::Hold,
            Timer::BootNow,
            Timer::ResetPulse,
            Timer::BootWait,
        ] {
            self.timers.cancel(t);
        }
        self.in_flight = false;
        self.hold_for = 0;
        self.resetting = true;
        for o in std::mem::take(&mut self.out) {
            if let Some(tag) = o.tag {
                self.events.push_back(LinkEvent::Ack { tag, status: -1 });
            }
        }
        // Boot line released: run the application, not the resident bootloader. Then a reset
        // pulse.
        if !self.lines.set_boot(false) || !self.lines.set_reset(true) {
            warn!(
                "zigbee: cannot drive {}, radio not reset",
                self.lines.describe()
            );
            self.timers.set(Timer::BootNow, 0);
            return;
        }
        self.reset_held = true;
        self.timers.set(Timer::ResetPulse, RESET_PULSE_MS);
    }

    /// Next event. Cancel-safe: input and timers are handled between awaits, and events wait in
    /// a queue until returned. The link's timers only run inside this call.
    pub async fn next(&mut self) -> LinkEvent {
        loop {
            if let Some(ev) = self.events.pop_front() {
                return ev;
            }
            let mut buf = [0u8; 512];
            // Input first, as the C++ loop ran fd callbacks before timers: an ACK that is
            // already there wins over its timeout.
            tokio::select! {
                biased;
                r = poll_fn(|cx| poll_read(&mut self.wire, cx, &mut buf)), if !self.read_dead => {
                    match r {
                        Ok(n @ 1..) => {
                            self.rx.extend_from_slice(&buf[..n]);
                            self.read_available();
                            self.parse();
                        }
                        Ok(_) => self.stop_reading("hung up"),
                        Err(e) => self.stop_reading(&home_linux::io_error_text(&e)),
                    }
                }
                t = self.timers.next() => self.on_timer(t),
            }
        }
    }

    /// Takes the rest of what the wire holds without waiting (the C++ read loop until EAGAIN).
    fn read_available(&mut self) {
        let mut buf = [0u8; 512];
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            match poll_read(&mut self.wire, &mut cx, &mut buf) {
                Poll::Ready(Ok(n @ 1..)) => self.rx.extend_from_slice(&buf[..n]),
                Poll::Ready(Ok(_)) => return self.stop_reading("hung up"),
                Poll::Ready(Err(e)) => return self.stop_reading(&home_linux::io_error_text(&e)),
                Poll::Pending => return,
            }
        }
    }

    /// A hung-up or failing tty would report input forever (the C++ loop spun on it): logged
    /// once, then only timers run, so ACKs time out and the reset logic carries on.
    fn stop_reading(&mut self, why: &str) {
        error!(
            "zigbee: read {}: {why}; no more input from the radio",
            self.tty
        );
        self.read_dead = true;
    }

    fn on_timer(&mut self, t: Timer) {
        match t {
            Timer::Ack => self.ack_timeout(),
            Timer::Hold => {
                warn!("zigbee: no 0x{:04x} from the radio", self.hold_for);
                self.release_hold();
            }
            Timer::BootNow => self.booted(),
            Timer::ResetPulse => {
                self.lines.set_reset(false);
                self.reset_held = false;
                self.rx.clear();
                self.codec = FrameCodec::default();
                self.timers.set(Timer::BootWait, BOOT_WAIT_MS);
            }
            Timer::BootWait => {
                warn!("zigbee: no boot indication from the radio, continuing");
                self.booted();
            }
        }
    }

    fn booted(&mut self) {
        for t in [Timer::BootNow, Timer::ResetPulse, Timer::BootWait] {
            self.timers.cancel(t);
        }
        self.resetting = false;
        self.failures = 0;
        self.events.push_back(LinkEvent::Boot);
        self.pump();
    }

    fn pump(&mut self) {
        if self.in_flight || self.resetting || self.hold_for != 0 {
            return;
        }
        let Some(o) = self.out.front_mut() else {
            return;
        };
        o.tries += 1;
        let mut left = &o.frame[..];
        while !left.is_empty() {
            let mut cx = Context::from_waker(Waker::noop());
            match Pin::new(&mut *self.wire).poll_write(&mut cx, left) {
                Poll::Ready(Ok(n @ 1..)) => left = &left[n..],
                Poll::Ready(Err(e)) if e.kind() == io::ErrorKind::Interrupted => {}
                // Kernel tty buffer full: the ACK timeout retries the frame.
                _ => break,
            }
        }
        self.in_flight = true;
        self.timers.set(Timer::Ack, ACK_TIMEOUT_MS);
    }

    fn ack_timeout(&mut self) {
        self.in_flight = false;
        let Some(head) = self.out.front() else {
            return;
        };
        if head.tries < TRIES {
            return self.pump();
        }
        warn!("zigbee: no ACK for frame 0x{:04x}", head.ty);
        self.failures += 1;
        if self.failures >= FAILURES_BEFORE_RESET {
            error!("zigbee: radio not answering, resetting it");
            return self.reset();
        }
        self.finish_head(-1);
    }

    fn finish_head(&mut self, status: i32) {
        self.timers.cancel(Timer::Ack);
        self.in_flight = false;
        let Some(o) = self.out.pop_front() else {
            return;
        };
        if status >= 0 {
            self.failures = 0;
        }
        if status == 0 && o.hold_for != 0 {
            self.hold_for = o.hold_for;
            self.timers.set(Timer::Hold, o.hold_ms);
        }
        if let Some(tag) = o.tag {
            self.events.push_back(LinkEvent::Ack { tag, status });
        }
        self.pump();
    }

    fn release_hold(&mut self) {
        self.timers.cancel(Timer::Hold);
        self.hold_for = 0;
        self.pump();
    }

    fn parse(&mut self) {
        // Handlers never touch the codec or rx (only the reset pulse's timer clears them).
        loop {
            let before = self.rx.len();
            let frame = match self.codec.decode(&mut self.rx) {
                Ok(frame) => frame,
                Err(e) => {
                    self.stop_reading(&home_linux::io_error_text(&e));
                    return;
                }
            };
            match frame {
                Some(frame) => self.handle(frame.ty, &frame.payload),
                // Nothing complete: the scanner may still have skipped garbage.
                None if self.rx.len() == before => return,
                None => {}
            }
        }
    }

    fn handle(&mut self, ty: u16, p: &[u8]) {
        if ty == ACK {
            if p.len() < 3 || !self.in_flight {
                return;
            }
            let Some(head) = self.out.front() else {
                return;
            };
            let acked = u16::from_be_bytes([p[0], p[1]]);
            // 0xFFFF: the firmware could not even find the start byte; it concerns the frame in
            // flight.
            if acked != head.ty && acked != 0xFFFF {
                return;
            }
            if p[2] != 0 {
                warn!(
                    "zigbee: radio rejected frame 0x{:04x}: status 0x{:02x}",
                    head.ty, p[2]
                );
            }
            return self.finish_head(i32::from(p[2]));
        }
        if ty == BOOT_IND {
            if self.reset_held {
                return; // queued before the pulse; the real one follows its release
            }
            info!("zigbee: radio application started");
            if !self.resetting {
                // Spontaneous restart (watchdog or crash): what is in flight never gets its
                // ACK.
                self.timers.cancel(Timer::Ack);
                self.timers.cancel(Timer::Hold);
                self.in_flight = false;
                self.hold_for = 0;
                for o in &mut self.out {
                    o.tries = 0;
                }
            }
            return self.booted();
        }
        let releases = self.hold_for != 0 && ty == self.hold_for;
        self.events.push_back(LinkEvent::Frame {
            ty,
            payload: p.to_vec(),
        });
        if releases {
            self.release_hold();
        }
    }
}

fn poll_read(wire: &mut Io, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
    let mut rb = ReadBuf::new(buf);
    match Pin::new(&mut **wire).poll_read(cx, &mut rb) {
        Poll::Ready(Ok(())) => Poll::Ready(Ok(rb.filled().len())),
        Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
        Poll::Pending => Poll::Pending,
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        if self.reset_held {
            self.lines.set_reset(false); // never leave the radio held in reset
        }
    }
}
