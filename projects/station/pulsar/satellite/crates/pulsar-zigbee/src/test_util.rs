//! Test helpers: an in-memory radio for the link, GPIO value files, golden comparison.

use std::path::PathBuf;
use std::pin::{Pin, pin};
use std::sync::atomic::{AtomicU32, Ordering};
use std::task::{Context, Poll, Waker};

use pulsar_zbhci::frame::build_frame;
use pulsar_zbhci::link::{Link, SysfsLines};
use tokio::io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf};

use crate::RadioPaths;

/// Polls `f` once; None if it is not ready. Runs the link and the radio in lock-step from a
/// single task, so that paused time only moves when the test says so.
pub fn poll_once<F: Future>(f: F) -> Option<F::Output> {
    let mut f = pin!(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => Some(v),
        Poll::Pending => None,
    }
}

pub fn hex(p: &[u8]) -> String {
    if p.is_empty() {
        return "-".into();
    }
    p.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compares with a golden file line by line, naming the first difference.
pub fn assert_golden(golden: &str, actual: &str) {
    let want: Vec<&str> = golden.lines().collect();
    let got: Vec<&str> = actual.lines().collect();
    for (i, (w, g)) in want.iter().zip(&got).enumerate() {
        assert_eq!(g, w, "line {} differs from the C++ output", i + 1);
    }
    assert_eq!(
        got.len(),
        want.len(),
        "line count differs from the C++ output"
    );
}

/// The chip's end of an in-memory link.
pub struct Radio {
    io: DuplexStream,
    rx: Vec<u8>,
}

impl Radio {
    /// The next complete frame the link wrote, if one is there.
    pub(crate) fn read_frame(&mut self) -> Option<(u16, Vec<u8>)> {
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

    pub(crate) fn send(&mut self, ty: u16, payload: &[u8]) {
        self.write_raw(&build_frame(ty, payload));
    }

    pub(crate) fn ack(&mut self, ty: u16, status: u8) {
        let [t0, t1] = ty.to_be_bytes();
        self.send(0x8000, &[t0, t1, status, 0]);
    }

    pub(crate) fn write_raw(&mut self, mut b: &[u8]) {
        let mut cx = Context::from_waker(Waker::noop());
        while !b.is_empty() {
            match Pin::new(&mut self.io).poll_write(&mut cx, b) {
                Poll::Ready(Ok(n)) if n > 0 => b = &b[n..],
                other => panic!("radio write: {other:?}"),
            }
        }
    }
}

/// A link (reset as at open) and the radio's end of its wire.
pub fn link_pair(lines: &RadioPaths) -> (Link, Radio) {
    let (a, b) = tokio::io::duplex(1 << 16);
    (
        Link::with_io(
            a,
            lines.tty.clone(),
            Box::new(SysfsLines::new(lines.reset.clone(), lines.boot.clone())),
        ),
        Radio {
            io: b,
            rx: Vec::new(),
        },
    )
}

/// Reset / boot files that do not exist: the link boots at once without driving anything.
pub fn absent_lines() -> RadioPaths {
    RadioPaths {
        tty: String::new(),
        reset: "/nonexistent/zb-reset/value".into(),
        boot: "/nonexistent/zb-boot/value".into(),
    }
}

/// Reset and boot value files in a temporary directory.
pub struct Lines {
    dir: PathBuf,
}

impl Lines {
    pub(crate) fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "pulsar-zigbee-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["reset", "boot"] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        Self { dir }
    }

    pub(crate) fn radio_lines(&self) -> RadioPaths {
        let path = |f: &str| self.dir.join(f).to_string_lossy().into_owned();
        RadioPaths {
            tty: String::new(),
            reset: path("reset"),
            boot: path("boot"),
        }
    }

    pub(crate) fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// (boot, reset) as last written.
    pub(crate) fn get(&self) -> (String, String) {
        let read = |f: &str| std::fs::read_to_string(self.dir.join(f)).unwrap();
        (read("boot"), read("reset"))
    }
}

impl Drop for Lines {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub use home_linux::testing::open_pty;

/// A valid .ybl image of `version` (0x200 bytes + CRC-32 trailer).
pub fn ybl_image(version: u32) -> Vec<u8> {
    let size = 0x200usize;
    let mut v: Vec<u8> = (0..size).map(|i| (i * 7 + 3) as u8).collect();
    v[2..6].copy_from_slice(&version.to_le_bytes());
    v[0x12..0x14].copy_from_slice(&0x140Au16.to_le_bytes());
    v[0x14..0x16].copy_from_slice(&0x0580u16.to_le_bytes());
    v[0x18..0x1C].copy_from_slice(&(size as u32).to_le_bytes());
    v[0x20..0x24].copy_from_slice(b"KNLT");
    let crc = crate::flasher::crc32(&v);
    v.extend_from_slice(&crc.to_le_bytes());
    v
}
