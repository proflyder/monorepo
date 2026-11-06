//! Test harness of the link: an in-memory radio on a duplex, recording reset/boot lines and a
//! pseudo-terminal for the real tty path.

use std::cell::Cell;
use std::pin::{Pin, pin};
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use tokio::io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf};

use super::*;
use crate::frame::{CRASH_IND, build_frame};
use home_linux::testing::open_pty;

/// Polls `f` once; None if it is not ready. Runs the link and the radio in lock-step from a
/// single task, so that paused time only moves when the test says so.
pub fn poll_once<F: Future>(f: F) -> Option<F::Output> {
    let mut f = pin!(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => Some(v),
        Poll::Pending => None,
    }
}

fn hex(p: &[u8]) -> String {
    if p.is_empty() {
        return "-".into();
    }
    p.iter().map(|b| format!("{b:02x}")).collect()
}

/// The chip's end of an in-memory link.
pub struct Radio {
    io: DuplexStream,
    rx: Vec<u8>,
}

impl Radio {
    /// The next complete frame the link wrote, if one is there.
    fn read_frame(&mut self) -> Option<(u16, Vec<u8>)> {
        let mut buf = [0u8; 512];
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            let mut rb = ReadBuf::new(&mut buf);
            match Pin::new(&mut self.io).poll_read(&mut cx, &mut rb) {
                Poll::Ready(Ok(())) if !rb.filled().is_empty() => {
                    self.rx.extend_from_slice(rb.filled());
                }
                _ => break,
            }
        }
        if self.rx.len() < 7 {
            return None;
        }
        assert_eq!(self.rx[0], 0x55, "link wrote garbage: {}", hex(&self.rx));
        let ty = u16::from_be_bytes([self.rx[1], self.rx[2]]);
        let len = usize::from(u16::from_be_bytes([self.rx[3], self.rx[4]]));
        if self.rx.len() < len + 7 {
            return None;
        }
        let frame: Vec<u8> = self.rx.drain(..len + 7).collect();
        let payload = frame[6..6 + len].to_vec();
        assert_eq!(build_frame(ty, &payload), frame, "malformed frame");
        Some((ty, payload))
    }

    fn send(&mut self, ty: u16, payload: &[u8]) {
        self.write_raw(&build_frame(ty, payload));
    }

    fn ack(&mut self, ty: u16, status: u8) {
        let [t0, t1] = ty.to_be_bytes();
        self.send(ACK, &[t0, t1, status, 0]);
    }

    fn write_raw(&mut self, mut b: &[u8]) {
        let mut cx = Context::from_waker(Waker::noop());
        while !b.is_empty() {
            match Pin::new(&mut self.io).poll_write(&mut cx, b) {
                Poll::Ready(Ok(n)) if n > 0 => b = &b[n..],
                other => panic!("radio write: {other:?}"),
            }
        }
    }
}

/// Reset and boot value files in memory.
#[derive(Clone)]
pub struct Lines {
    boot: Rc<Cell<bool>>,
    reset: Rc<Cell<bool>>,
    writable: bool,
}

impl Lines {
    fn new() -> Self {
        Self {
            boot: Rc::new(Cell::new(false)),
            reset: Rc::new(Cell::new(false)),
            writable: true,
        }
    }

    fn absent() -> Self {
        Self {
            writable: false,
            ..Self::new()
        }
    }

    fn into_lines(self) -> Box<dyn RadioLines> {
        Box::new(self)
    }

    /// (boot, reset) as last written.
    fn get(&self) -> (String, String) {
        let s = |v: bool| if v { "1" } else { "0" };
        (s(self.boot.get()).into(), s(self.reset.get()).into())
    }
}

impl RadioLines for Lines {
    fn set_boot(&mut self, asserted: bool) -> bool {
        if self.writable {
            self.boot.set(asserted);
        }
        self.writable
    }

    fn set_reset(&mut self, asserted: bool) -> bool {
        if self.writable {
            self.reset.set(asserted);
        }
        self.writable
    }

    fn describe(&self) -> String {
        "reset / boot".into()
    }
}

/// A link (reset as at open) and the radio's end of its wire.
fn link_pair(lines: &Lines) -> (Link, Radio) {
    let (a, b) = tokio::io::duplex(1 << 16);
    (
        Link::with_io(a, "", lines.clone().into_lines()),
        Radio {
            io: b,
            rx: Vec::new(),
        },
    )
}

use std::io::{Read as _, Write as _};
use std::time::Duration;

use tokio::time::advance;

fn pattern(n: usize, seed: u8) -> Vec<u8> {
    (0..n).map(|i| seed.wrapping_add((i * 13) as u8)).collect()
}

/// Runs the link until it waits for input or time.
fn settle(link: &mut Link) -> Vec<LinkEvent> {
    let mut evs = Vec::new();
    while let Some(ev) = poll_once(link.next()) {
        evs.push(ev);
    }
    evs
}

async fn ms(link: &mut Link, n: u64) -> Vec<LinkEvent> {
    advance(Duration::from_millis(n)).await;
    settle(link)
}

/// A link whose application has started (reset lines driven, boot indication answered).
async fn booted() -> (Link, Radio, Lines) {
    let lines = Lines::new();
    let (mut link, mut radio) = link_pair(&lines);
    assert_eq!(lines.get(), ("0".into(), "1".into()));
    radio.send(BOOT_IND, &[]); // predates the pulse's end: ignored
    assert!(ms(&mut link, 199).await.is_empty());
    assert!(ms(&mut link, 1).await.is_empty());
    assert_eq!(lines.get(), ("0".into(), "0".into()));
    radio.send(BOOT_IND, &[]);
    assert_eq!(settle(&mut link), [LinkEvent::Boot]);
    (link, radio, lines)
}

fn ack(tag: AckTag, status: i32) -> LinkEvent {
    LinkEvent::Ack { tag, status }
}

#[tokio::test(start_paused = true)]
async fn reset_waits_for_boot_then_sends() {
    let lines = Lines::new();
    let (mut link, mut radio) = link_pair(&lines);
    link.send(0x0900, &[], Some(AckTag::Form));
    assert!(settle(&mut link).is_empty());
    assert_eq!(
        radio.read_frame(),
        None,
        "nothing goes out during the reset"
    );
    assert!(ms(&mut link, 200).await.is_empty());
    // No boot indication: after 3 s the link carries on.
    assert!(ms(&mut link, 2999).await.is_empty());
    assert_eq!(ms(&mut link, 1).await, [LinkEvent::Boot]);
    assert_eq!(radio.read_frame(), Some((0x0900, vec![])));
    radio.ack(0x0900, 0);
    assert_eq!(settle(&mut link), [ack(AckTag::Form, 0)]);
}

#[tokio::test(start_paused = true)]
async fn undrivable_lines_boot_at_once() {
    let absent = Lines::absent();
    let (mut link, mut radio) = link_pair(&absent);
    link.send(0x0045, &[], None);
    assert_eq!(settle(&mut link), [LinkEvent::Boot]);
    assert_eq!(radio.read_frame(), Some((0x0045, vec![])));
}

#[tokio::test(start_paused = true)]
async fn stop_and_wait_with_ack_matching() {
    let (mut link, mut radio, _lines) = booted().await;
    link.send(0x0A00, &[1], Some(AckTag::Form));
    link.send(0x0A03, &[2], Some(AckTag::Steer));
    link.send(0x0A05, &[3], Some(AckTag::Leave));
    assert_eq!(radio.read_frame(), Some((0x0A00, vec![1])));
    assert_eq!(radio.read_frame(), None, "second frame waits for the ACK");
    radio.ack(0x0A99, 0); // another type: not this frame's
    radio.send(ACK, &[0x0A, 0x00]); // too short
    assert!(settle(&mut link).is_empty());
    assert_eq!(radio.read_frame(), None);
    radio.ack(0x0A00, 0);
    assert_eq!(settle(&mut link), [ack(AckTag::Form, 0)]);
    assert_eq!(radio.read_frame(), Some((0x0A03, vec![2])));
    radio.ack(0xFFFF, 2); // concerns the frame in flight
    assert_eq!(settle(&mut link), [ack(AckTag::Steer, 2)]);
    assert_eq!(radio.read_frame(), Some((0x0A05, vec![3])));
    radio.ack(0x0A05, 0);
    radio.ack(0x0A05, 0); // nothing in flight any more
    assert_eq!(settle(&mut link), [ack(AckTag::Leave, 0)]);
}

#[tokio::test(start_paused = true)]
async fn retries_then_gives_up_then_resets() {
    let (mut link, mut radio, lines) = booted().await;
    // Two frames without ACK: three tries each, -1, failures counted.
    for (i, ty) in [0x0A01u16, 0x0A02].into_iter().enumerate() {
        link.send(ty, &[], Some(AckTag::Data(i as u8)));
        for _ in 0..2 {
            assert_eq!(radio.read_frame(), Some((ty, vec![])));
            assert!(ms(&mut link, 299).await.is_empty());
            assert_eq!(radio.read_frame(), None);
            assert!(ms(&mut link, 1).await.is_empty());
        }
        assert_eq!(radio.read_frame(), Some((ty, vec![])));
        assert_eq!(ms(&mut link, 300).await, [ack(AckTag::Data(i as u8), -1)]);
    }
    // The third in a row resets the radio; what was queued fails with -1.
    link.send(0x0A03, &[], Some(AckTag::Form));
    link.send(0x0A04, &[], Some(AckTag::EdScan));
    link.send(0x0A05, &[], None);
    for _ in 0..2 {
        assert_eq!(radio.read_frame(), Some((0x0A03, vec![])));
        assert!(ms(&mut link, 300).await.is_empty());
    }
    assert_eq!(radio.read_frame(), Some((0x0A03, vec![])));
    assert_eq!(
        ms(&mut link, 300).await,
        [ack(AckTag::Form, -1), ack(AckTag::EdScan, -1)]
    );
    assert_eq!(lines.get(), ("0".into(), "1".into()));
    assert_eq!(radio.read_frame(), None);
    // Frames sent during the reset wait for the boot.
    link.send(0x0A06, &[], Some(AckTag::Steer));
    assert!(ms(&mut link, 200).await.is_empty());
    assert_eq!(lines.get().1, "0");
    radio.send(BOOT_IND, &[]);
    assert_eq!(settle(&mut link), [LinkEvent::Boot]);
    assert_eq!(radio.read_frame(), Some((0x0A06, vec![])));
}

#[tokio::test(start_paused = true)]
async fn a_success_clears_the_failure_count() {
    let (mut link, mut radio, lines) = booted().await;
    for round in 0..3 {
        for ty in [0x0A01u16, 0x0A02] {
            link.send(ty, &[], None);
            for _ in 0..3 {
                assert_eq!(radio.read_frame(), Some((ty, vec![])));
                ms(&mut link, 300).await;
            }
        }
        link.send(0x0A03, &[], None);
        assert_eq!(radio.read_frame(), Some((0x0A03, vec![])), "round {round}");
        radio.ack(0x0A03, 7); // a rejection still is an answer
        settle(&mut link);
    }
    assert_eq!(lines.get().1, "0", "never reset");
}

#[tokio::test(start_paused = true)]
async fn hold_until_awaited_frame_or_timeout() {
    let (mut link, mut radio, _lines) = booted().await;
    link.send_and_hold(0x0A06, &[9], Some(AckTag::EdScan), 0x8A06, 1000);
    link.send(0x0A10, &[1], None);
    assert_eq!(radio.read_frame(), Some((0x0A06, vec![9])));
    radio.ack(0x0A06, 0);
    assert_eq!(settle(&mut link), [ack(AckTag::EdScan, 0)]);
    radio.send(0x8A11, &[5]); // other frames pass, the hold stays
    assert_eq!(
        settle(&mut link),
        [LinkEvent::Frame {
            ty: 0x8A11,
            payload: vec![5]
        }]
    );
    assert_eq!(radio.read_frame(), None);
    radio.send(0x8A06, &[0]);
    assert_eq!(
        settle(&mut link),
        [LinkEvent::Frame {
            ty: 0x8A06,
            payload: vec![0]
        }]
    );
    assert_eq!(radio.read_frame(), Some((0x0A10, vec![1])));
    radio.ack(0x0A10, 0);
    settle(&mut link);

    // Timed out: released after hold_ms.
    link.send_and_hold(0x0A06, &[], None, 0x8A06, 1000);
    link.send(0x0A10, &[2], None);
    assert_eq!(radio.read_frame(), Some((0x0A06, vec![])));
    radio.ack(0x0A06, 0);
    settle(&mut link);
    assert!(ms(&mut link, 999).await.is_empty());
    assert_eq!(radio.read_frame(), None);
    ms(&mut link, 1).await;
    assert_eq!(radio.read_frame(), Some((0x0A10, vec![2])));
    radio.ack(0x0A10, 0);
    settle(&mut link);

    // A refused command does not hold.
    link.send_and_hold(0x0A06, &[], None, 0x8A06, 1000);
    link.send(0x0A10, &[3], None);
    assert_eq!(radio.read_frame(), Some((0x0A06, vec![])));
    radio.ack(0x0A06, 2);
    settle(&mut link);
    assert_eq!(radio.read_frame(), Some((0x0A10, vec![3])));
}

#[tokio::test(start_paused = true)]
async fn spontaneous_boot_resends_what_was_in_flight() {
    let (mut link, mut radio, lines) = booted().await;
    link.send_and_hold(0x0A06, &[], None, 0x8A06, 10_000);
    link.send(0x0A10, &[1], Some(AckTag::Data(1)));
    assert_eq!(radio.read_frame(), Some((0x0A06, vec![])));
    radio.ack(0x0A06, 0);
    settle(&mut link);
    // Held; the radio restarts: the hold is gone, the queue goes out again.
    radio.send(BOOT_IND, &[]);
    assert_eq!(settle(&mut link), [LinkEvent::Boot]);
    assert_eq!(radio.read_frame(), Some((0x0A10, vec![1])));
    // In flight when the radio restarts: sent again with a fresh set of tries.
    ms(&mut link, 299).await;
    radio.send(BOOT_IND, &[]);
    assert_eq!(settle(&mut link), [LinkEvent::Boot]);
    assert_eq!(radio.read_frame(), Some((0x0A10, vec![1])));
    for _ in 0..2 {
        assert!(ms(&mut link, 300).await.is_empty());
        assert_eq!(radio.read_frame(), Some((0x0A10, vec![1])));
    }
    assert_eq!(ms(&mut link, 300).await, [ack(AckTag::Data(1), -1)]);
    assert_eq!(lines.get().1, "0");
}

#[tokio::test(start_paused = true)]
async fn crash_report_and_frames_pass_through() {
    let (mut link, mut radio, _lines) = booted().await;
    let report = pattern(0x1000, 3);
    radio.write_raw(&[0x00, 0x55]);
    radio.send(CRASH_IND, &report);
    radio.send(0x8A11, &[]);
    assert_eq!(
        settle(&mut link),
        [
            LinkEvent::Frame {
                ty: CRASH_IND,
                payload: report
            },
            LinkEvent::Frame {
                ty: 0x8A11,
                payload: vec![]
            }
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn drop_releases_a_held_reset() {
    let lines = Lines::new();
    let (link, _radio) = link_pair(&lines);
    assert_eq!(lines.get().1, "1");
    drop(link);
    assert_eq!(lines.get().1, "0");
}

#[test]
fn open_reports_a_missing_tty() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        assert!(Link::open("/nonexistent/ttyS2", Lines::absent().into_lines()).is_err());
    });
}

/// A tty whose other end is gone (pty master closed: hangup, EIO) must not spin: reading
/// stops, timers go on (the ACK times out), then next() just waits. Run on its own thread
/// so a regression fails on the deadline instead of hanging the test run.
#[test]
fn hangup_stops_reading_without_spinning() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap();
        rt.block_on(async {
            let (master, slave) = open_pty().unwrap();
            let mut link = Link::open(&slave, Lines::absent().into_lines()).unwrap();
            assert_eq!(link.next().await, LinkEvent::Boot);
            drop(master);
            link.send(0x0900, &[], Some(AckTag::Form));
            let ev = tokio::time::timeout(Duration::from_secs(5), link.next()).await;
            assert_eq!(ev.ok(), Some(ack(AckTag::Form, -1)));
            assert!(link.read_dead);
            let idle = tokio::time::timeout(Duration::from_secs(3600), link.next()).await;
            assert!(idle.is_err(), "nothing left to report");
        });
        tx.send(()).unwrap();
    });
    rx.recv_timeout(Duration::from_secs(20))
        .expect("link spun on a hung-up tty");
}

async fn next_within_2s(link: &mut Link) -> LinkEvent {
    tokio::time::timeout(Duration::from_secs(2), link.next())
        .await
        .expect("no event from the link")
}

/// Reads a frame the link wrote to the pty, waiting up to 2 s.
fn read_pty_frame(master: &mut home_linux::Tty) -> (u16, Vec<u8>) {
    let mut buf = Vec::new();
    loop {
        // Poll the master through a plain blocking read after a readiness wait.
        assert!(master.wait_readable(2000), "no frame from the link");
        let mut b = [0u8; 64];
        let n = master.read(&mut b).unwrap();
        buf.extend_from_slice(&b[..n]);
        if buf.len() >= 7 && buf.len() >= 7 + usize::from(u16::from_be_bytes([buf[3], buf[4]])) {
            let ty = u16::from_be_bytes([buf[1], buf[2]]);
            let p = buf[6..buf.len() - 1].to_vec();
            assert_eq!(build_frame(ty, &p), buf);
            return (ty, p);
        }
    }
}

/// A real tty through Link::open: an empty non-blocking read with VMIN = VTIME = 0 returns
/// 0 bytes, which must not count as a hangup. Input arriving in separate chunks keeps being
/// read; only closing the pty's master ends reading, once, without spinning.
#[test]
fn tty_keeps_reading_across_chunks_then_detects_hangup() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let (mut master, slave) = open_pty().unwrap();
            let mut link = Link::open(&slave, Lines::absent().into_lines()).unwrap();
            let pause = || tokio::time::sleep(Duration::from_millis(50));
            assert_eq!(next_within_2s(&mut link).await, LinkEvent::Boot);
            link.send(0x0900, &[], Some(AckTag::Form));
            assert_eq!(read_pty_frame(&mut master), (0x0900, vec![]));
            pause().await;
            master.write_all(&build_frame(BOOT_IND, &[])).unwrap();
            assert_eq!(next_within_2s(&mut link).await, LinkEvent::Boot);
            // Restarted: what was in flight goes out again.
            assert_eq!(read_pty_frame(&mut master), (0x0900, vec![]));
            pause().await;
            let mut chunk = build_frame(ACK, &[0x09, 0x00, 0x00, 0x00]);
            chunk.extend(build_frame(0x8900, &[0x50, 0, 0, 1]));
            master.write_all(&chunk).unwrap();
            assert_eq!(next_within_2s(&mut link).await, ack(AckTag::Form, 0));
            assert_eq!(
                next_within_2s(&mut link).await,
                LinkEvent::Frame {
                    ty: 0x8900,
                    payload: vec![0x50, 0, 0, 1]
                }
            );
            pause().await;
            master.write_all(&build_frame(0x8A00, &[7])).unwrap();
            assert_eq!(
                next_within_2s(&mut link).await,
                LinkEvent::Frame {
                    ty: 0x8A00,
                    payload: vec![7]
                }
            );
            assert!(!link.read_dead);

            drop(master);
            link.send(0x0005, &[], Some(AckTag::Leave));
            assert_eq!(next_within_2s(&mut link).await, ack(AckTag::Leave, -1));
            assert!(link.read_dead);
            let idle = tokio::time::timeout(Duration::from_millis(500), link.next()).await;
            assert!(idle.is_err(), "nothing left to report");
        });
        tx.send(()).unwrap();
    });
    rx.recv_timeout(Duration::from_secs(20))
        .expect("link test failed or spun");
}
