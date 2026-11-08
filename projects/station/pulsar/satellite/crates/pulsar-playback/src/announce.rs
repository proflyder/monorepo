//! The ANNOUNCE output: one writer thread that plays decoded PCM on the model's playback PCM
//! with the priming / padding / drain policy measured on the stations. The policy itself
//! ([`Announce`]) is generic over [`PcmDevice`], so the tests run it on a fake device; the
//! realtime wrapper ([`Output`]) owns the socketpair, the reader thread and the writer thread.

use std::collections::VecDeque;
use std::io::Read as _;
use std::os::fd::{AsRawFd as _, RawFd};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pulsar_alsa::Format;
use pulsar_model::Playback;
use tracing::{debug, error, info, warn};

use crate::device::{AlsaDevice, PcmDevice, PcmInfo, PcmRequest, Recovery, recover};

/// The format playsink converts to (capsfilter before the fdsink) and the PCM is opened with.
pub const CAPS: &str = "audio/x-raw,format=S16LE,layout=interleaved,rate=48000,channels=2";
const RATE: u32 = 48_000;
const CHANNELS: u32 = 2;
const FRAME_BYTES: usize = 4;

const fn ms_bytes(ms: u32) -> usize {
    RATE as usize * FRAME_BYTES * ms as usize / 1000
}

/// Policy of the ANNOUNCE writer. The station numbers (prime 200 ms, silence below two periods,
/// one-second queue, 100 ms pad, 500 ms drain slack) are measurements: see docs/architecture.md,
/// «pulsar-media».
#[derive(Clone, Copy, Debug)]
pub struct OutputTuning {
    /// Output starts once this much is queued, or at the end of the item.
    pub prime_ms: u32,
    /// Device fill below which an empty feed is a stall.
    pub low_periods: usize,
    /// The reader stops taking from the socket above this.
    pub queue_ms: u32,
    /// Silence written after the last sample before the drain.
    pub pad_ms: u32,
    /// Added to the device buffer duration as the longest drain wait.
    pub drain_slack_ms: u64,
    /// Poll period of the writer while an item is active.
    pub slice_ms: u64,
}

impl OutputTuning {
    pub const ANNOUNCE: Self = Self {
        prime_ms: 200,
        low_periods: 2,
        queue_ms: 1000,
        pad_ms: 100,
        drain_slack_ms: 500,
        slice_ms: 10,
    };
}

impl Default for OutputTuning {
    fn default() -> Self {
        Self::ANNOUNCE
    }
}

/// Writer progress of one output item, reported on the GLib thread.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OutEvent {
    /// The first samples went to the device.
    Started(u64),
    /// The last sample has been played.
    PlayedOut(u64),
    /// The PCM could not be opened or failed for good.
    Failed(u64),
}

#[derive(Default)]
struct FeedState {
    data: VecDeque<u8>,
    eof: bool,
    cancelled: bool,
}

/// PCM of one item between its reader and the writer.
#[derive(Default)]
struct Feed {
    state: Mutex<FeedState>,
    space: Condvar,
}

impl Feed {
    fn lock(&self) -> MutexGuard<'_, FeedState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn cancel(&self) {
        let mut st = self.lock();
        st.cancelled = true;
        st.data.clear();
        drop(st);
        self.space.notify_all();
    }

    /// Reader side: appends `chunk`, waiting while `max` bytes are queued. Cancelled items keep
    /// reading and discarding until the fdsink closes its end: closing the socket under it would
    /// give GStreamer an EPIPE / error message on the now unwanted item.
    fn push(&self, chunk: &[u8], max: usize) {
        let mut st = self.lock();
        while st.data.len() >= max && !st.cancelled {
            st = self.space.wait(st).unwrap_or_else(PoisonError::into_inner);
        }
        if !st.cancelled {
            st.data.extend(chunk);
        }
    }

    fn eof(&self) {
        self.lock().eof = true;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    /// Waiting for the prime amount (or the end of the item) before writing.
    Priming,
    Playing,
    /// Everything is written (plus the silence pad); waiting for the device to play it.
    Draining,
}

/// The policy on one item; generic over the device so the tests can drive it in memory.
struct Announce<D: PcmDevice> {
    device: D,
    tuning: OutputTuning,
    info: Option<PcmInfo>,
    cur: Option<(u64, Arc<Feed>)>,
    phase: Phase,
    /// Started was reported for the current item.
    started: bool,
    drain_deadline: Option<Instant>,
    /// Frames of the end pad still to write.
    pad_left: usize,
    /// Gain-scaled PCM on its way to the device.
    scratch: Vec<u8>,
    /// One period of silence.
    silence: Vec<u8>,
    /// f32 bits of the channel gain the writer applies (playsink's volume would act on PCM that
    /// is already queued, up to ~2 s late).
    gain: Arc<AtomicU32>,
}

impl<D: PcmDevice> Announce<D> {
    pub fn new(device: D, tuning: OutputTuning, gain: Arc<AtomicU32>) -> Self {
        Self {
            device,
            tuning,
            info: None,
            cur: None,
            phase: Phase::Priming,
            started: false,
            drain_deadline: None,
            pad_left: 0,
            scratch: Vec::new(),
            silence: Vec::new(),
            gain,
        }
    }

    /// Item `item` starts (any previous one is dropped).
    pub fn begin(&mut self, item: u64, feed: Arc<Feed>) {
        self.reset();
        self.cur = Some((item, feed));
    }

    /// Drops the current item and stops the device at once.
    pub fn reset(&mut self) {
        if let Some((_, feed)) = self.cur.take() {
            feed.cancel();
        }
        self.phase = Phase::Priming;
        self.started = false;
        self.drain_deadline = None;
        self.pad_left = 0;
        if self.device.is_open() {
            let opened = self.info.is_some();
            let _ = self.device.drop_frames();
            self.device.close();
            self.info = None;
            if opened {
                debug!("player: announce output closed");
            }
        }
    }

    fn finish(&mut self, ev: OutEvent, notify: &mut dyn FnMut(OutEvent)) {
        self.reset();
        notify(ev);
    }

    pub fn service(&mut self, notify: &mut dyn FnMut(OutEvent)) {
        let Some((item, feed)) = self.cur.as_ref().map(|(g, f)| (*g, Arc::clone(f))) else {
            return;
        };
        let mut st = feed.lock();
        if self.phase == Phase::Priming {
            let len = st.data.len();
            if len >= ms_bytes(self.tuning.prime_ms) || (st.eof && len >= FRAME_BYTES) {
                if self.info.is_none() {
                    // Not under the feed lock: the GLib thread takes it to cancel the item.
                    drop(st);
                    if self.open().is_err() {
                        self.finish(OutEvent::Failed(item), notify);
                        return;
                    }
                    st = feed.lock();
                }
                self.phase = Phase::Playing;
                if !self.started {
                    self.started = true;
                    notify(OutEvent::Started(item));
                }
            } else if st.eof {
                // Nothing left to write: play out what the device holds, if it is open.
                drop(st);
                if self.info.is_some() {
                    self.start_drain();
                } else {
                    self.finish(OutEvent::PlayedOut(item), notify);
                }
                return;
            } else {
                drop(st);
                if self.keep_alive().is_err() {
                    self.finish(OutEvent::Failed(item), notify);
                }
                return;
            }
        }
        if self.phase == Phase::Playing {
            let Ok(wrote) = self.write(&mut st) else {
                drop(st);
                self.finish(OutEvent::Failed(item), notify);
                return;
            };
            let (done, empty) = (
                st.eof && st.data.len() < FRAME_BYTES,
                st.data.len() < FRAME_BYTES,
            );
            drop(st);
            if wrote {
                feed.space.notify_all();
            }
            if done {
                self.start_drain();
            } else if empty && self.fill().is_some_and(|f| f < self.low_frames()) {
                info!("player: announce output stalled, priming again");
                self.phase = Phase::Priming;
                if self.keep_alive().is_err() {
                    self.finish(OutEvent::Failed(item), notify);
                }
            }
            return;
        }
        drop(st);
        self.drain(item, notify);
    }

    fn low_frames(&self) -> usize {
        self.info.map_or(0, |i| {
            (self.tuning.low_periods * i.period_frames).min(i.buffer_frames / 2)
        })
    }

    /// Frames queued in the device; None when that cannot be read.
    fn fill(&mut self) -> Option<usize> {
        let info = self.info?;
        let avail = self.device.avail().ok()?;
        Some(info.buffer_frames.saturating_sub(avail))
    }

    /// While stalled: tops the device up with silence so it never runs dry.
    fn keep_alive(&mut self) -> Result<(), ()> {
        let Some(info) = self.info else {
            return Ok(());
        };
        let avail = match self.device.avail() {
            Ok(n) => n,
            Err(e) => {
                let mut xruns = 0;
                return match recover(&mut self.device, e, &mut xruns) {
                    Recovery::Fatal(e) => {
                        error!(
                            "player: announce output error {}",
                            pulsar_alsa::strerror(e.code)
                        );
                        Err(())
                    }
                    _ => Ok(()),
                };
            }
        };
        let fill = info.buffer_frames.saturating_sub(avail);
        if fill >= self.low_frames() || avail < info.period_frames {
            return Ok(());
        }
        match self.device.write(&self.silence, info.period_frames) {
            Ok(_) => Ok(()),
            Err(e) => {
                let mut xruns = 0;
                match recover(&mut self.device, e, &mut xruns) {
                    Recovery::Fatal(e) => {
                        error!(
                            "player: announce output error {}",
                            pulsar_alsa::strerror(e.code)
                        );
                        Err(())
                    }
                    _ => Ok(()),
                }
            }
        }
    }

    /// Writes what the device takes. Err when the PCM failed for good.
    fn write(&mut self, st: &mut FeedState) -> Result<bool, ()> {
        let mut wrote = false;
        loop {
            let frames = st.data.len() / FRAME_BYTES;
            if frames == 0 {
                return Ok(wrote);
            }
            if self.info.is_none() {
                return Err(());
            }
            let avail = match self.device.avail() {
                Ok(n) => n,
                Err(e) => {
                    let mut xruns = 0;
                    if let Recovery::Fatal(e) = recover(&mut self.device, e, &mut xruns) {
                        error!(
                            "player: announce output error {}",
                            pulsar_alsa::strerror(e.code)
                        );
                        return Err(());
                    }
                    continue;
                }
            };
            if avail == 0 {
                return Ok(wrote);
            }
            let n = frames.min(avail);
            let gain = f32::from_bits(self.gain.load(Ordering::Relaxed));
            let (head, tail) = st.data.as_slices();
            let head_frames = head.len() / FRAME_BYTES;
            let res = if gain == 1.0 && head_frames > 0 {
                let n = n.min(head_frames);
                self.device.write(&head[..n * FRAME_BYTES], n)
            } else {
                // Scaled (or a frame across the ring's wrap): copied into scratch.
                self.scratch.clear();
                let mut bytes = head.iter().chain(tail).copied().take(n * FRAME_BYTES);
                while let (Some(lo), Some(hi)) = (bytes.next(), bytes.next()) {
                    #[allow(clippy::cast_possible_truncation, reason = "saturating, gain 0..1")]
                    let v = (f32::from(i16::from_le_bytes([lo, hi])) * gain) as i16;
                    self.scratch.extend_from_slice(&v.to_le_bytes());
                }
                self.device.write(&self.scratch, n)
            };
            match res {
                Ok(w) => {
                    st.data.drain(..w * FRAME_BYTES);
                    wrote = true;
                }
                Err(e) => {
                    let mut xruns = 0;
                    if let Recovery::Fatal(e) = recover(&mut self.device, e, &mut xruns) {
                        error!(
                            "player: announce output error {}",
                            pulsar_alsa::strerror(e.code)
                        );
                        return Err(());
                    }
                }
            }
        }
    }

    fn start_drain(&mut self) {
        let Some(info) = self.info else {
            return;
        };
        self.phase = Phase::Draining;
        let buffer = Duration::from_micros(info.buffer_frames as u64 * 1_000_000 / u64::from(RATE));
        self.drain_deadline =
            Some(Instant::now() + buffer + Duration::from_millis(self.tuning.drain_slack_ms));
        self.pad_left = RATE as usize * self.tuning.pad_ms as usize / 1000;
    }

    fn drain(&mut self, item: u64, notify: &mut dyn FnMut(OutEvent)) {
        let Some(info) = self.info else {
            self.finish(OutEvent::PlayedOut(item), notify);
            return;
        };
        while self.pad_left > 0 {
            let n = match self.device.avail() {
                Ok(a) => a.min(self.pad_left).min(info.period_frames),
                Err(_) => break,
            };
            if n == 0 {
                break;
            }
            match self.device.write(&self.silence[..n * FRAME_BYTES], n) {
                Ok(w) => self.pad_left -= w,
                Err(_) => break,
            }
        }
        let played = match self.device.delay() {
            Ok(d) => self.pad_left == 0 && d <= 0,
            // Ran dry (xrun): nothing left to play.
            Err(_) => true,
        };
        if played || self.drain_deadline.is_some_and(|t| Instant::now() >= t) {
            if !played {
                warn!("player: announce output play-out timed out");
            }
            self.finish(OutEvent::PlayedOut(item), notify);
        }
    }

    fn open(&mut self) -> Result<(), ()> {
        let req = PcmRequest {
            format: Format::S16Le,
            channels: CHANNELS,
            rate: RATE,
            rate_exact: true,
            period_frames: None,
            buffer_frames: None,
        };
        let info = self.device.open(&req).map_err(|e| {
            error!(
                "player: announce output '{}' {} failed: {e}",
                self.device.name(),
                e.step.as_str()
            );
        })?;
        self.info = Some(info);
        self.silence.resize(info.period_frames * FRAME_BYTES, 0);
        debug!(
            "player: announce output open (period {}, buffer {} frames)",
            info.period_frames, info.buffer_frames
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The realtime wrapper.

enum Cmd {
    Start(u64, Arc<Feed>),
    Flush,
    Quit,
}

struct Item {
    /// The end the fdsink writes; dropping it is the item's end of stream for the reader.
    #[allow(dead_code, reason = "held for its drop only")]
    sink_end: UnixStream,
    feed: Arc<Feed>,
}

/// Owned by the GLib thread's backend.
pub struct Output {
    cmds: Sender<Cmd>,
    gain: Arc<AtomicU32>,
    item: Option<Item>,
    thread: Option<JoinHandle<()>>,
    queue_bytes: usize,
}

impl Output {
    /// Starts the writer thread on the model's playback PCM; `notify` runs on that thread.
    pub fn spawn(
        playback: &'static Playback,
        tuning: OutputTuning,
        notify: Box<dyn Fn(OutEvent) + Send>,
    ) -> Option<Self> {
        let (cmds, rx) = mpsc::channel();
        let gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        let queue = ms_bytes(tuning.queue_ms);
        let writer = Writer {
            core: Announce::new(
                AlsaDevice::from_playback(playback),
                tuning,
                Arc::clone(&gain),
            ),
            notify,
            rx,
        };
        let thread = std::thread::Builder::new()
            .name("announce-out".into())
            .spawn(move || writer.run())
            .inspect_err(|e| error!("player: announce output thread: {e}"))
            .ok()?;
        Some(Self {
            cmds,
            gain,
            item: None,
            thread: Some(thread),
            queue_bytes: queue,
        })
    }

    /// Item `item` starts (any previous one is dropped); returns the fd for the fdsink.
    pub fn begin(&mut self, item: u64) -> Option<RawFd> {
        self.flush();
        let (src, sink_end) = UnixStream::pair()
            .inspect_err(|e| error!("player: announce socketpair: {e}"))
            .ok()?;
        // Non-blocking: a full socket makes the fdsink wait in its GstPoll, which a state change
        // to NULL interrupts, instead of in write().
        sink_end
            .set_nonblocking(true)
            .inspect_err(|e| error!("player: announce socket: {e}"))
            .ok()?;
        let feed = Arc::new(Feed::default());
        let reader_feed = Arc::clone(&feed);
        let queue = self.queue_bytes;
        std::thread::Builder::new()
            .name("announce-rd".into())
            .spawn(move || read_feed(src, &reader_feed, queue))
            .inspect_err(|e| error!("player: announce reader thread: {e}"))
            .ok()?;
        let _ = self.cmds.send(Cmd::Start(item, Arc::clone(&feed)));
        let fd = sink_end.as_raw_fd();
        self.item = Some(Item { sink_end, feed });
        Some(fd)
    }

    /// The pipeline reached EOS, everything decoded is in the socket: the reader sees its end
    /// once it has read the rest, and the writer reports PlayedOut after playing it.
    pub fn end(&mut self) {
        self.item = None;
    }

    /// Channel gain 0..1, applied to what is written from now on.
    pub fn set_gain(&self, gain: f64) {
        #[allow(clippy::cast_possible_truncation, reason = "a gain 0..1")]
        self.gain.store((gain as f32).to_bits(), Ordering::Relaxed);
    }

    /// Drops the current item: queued PCM is discarded and the device stops at once.
    pub fn flush(&mut self) {
        if let Some(item) = self.item.take() {
            item.feed.cancel();
        }
        let _ = self.cmds.send(Cmd::Flush);
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.flush();
        let _ = self.cmds.send(Cmd::Quit);
        if let Some(t) = self.thread.take()
            && t.join().is_err()
        {
            error!("player: announce output thread panicked");
        }
    }
}

struct Writer {
    core: Announce<AlsaDevice>,
    notify: Box<dyn Fn(OutEvent) + Send>,
    rx: Receiver<Cmd>,
}

impl Writer {
    fn run(mut self) {
        loop {
            let slice = Duration::from_millis(self.core.tuning.slice_ms);
            let cmd = if self.core.cur.is_some() {
                match self.rx.recv_timeout(slice) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            } else {
                match self.rx.recv() {
                    Ok(c) => Some(c),
                    Err(_) => break,
                }
            };
            if let Some(mut c) = cmd {
                loop {
                    match c {
                        Cmd::Start(item, feed) => self.core.begin(item, feed),
                        Cmd::Flush => self.core.reset(),
                        Cmd::Quit => {
                            self.core.reset();
                            return;
                        }
                    }
                    match self.rx.try_recv() {
                        Ok(next) => c = next,
                        Err(_) => break,
                    }
                }
            }
            let notify = &mut self.notify;
            self.core.service(&mut |ev| notify(ev));
        }
        self.core.reset();
    }
}

/// Socket -> feed until the fdsink's end is closed; discards once the item is cancelled.
fn read_feed(mut src: UnixStream, feed: &Feed, max: usize) {
    let mut chunk = [0u8; 4096];
    loop {
        let n = match src.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                warn!("player: announce read: {e}");
                break;
            }
        };
        feed.push(&chunk[..n], max);
    }
    feed.eof();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeDevice;

    struct Rig {
        feed: Arc<Feed>,
        core: Announce<FakeDevice>,
        events: Vec<OutEvent>,
    }

    impl Rig {
        fn new(tuning: OutputTuning) -> Self {
            let dev = FakeDevice::new(4800, 1200);
            Self {
                feed: Arc::new(Feed::default()),
                core: Announce::new(dev, tuning, Arc::new(AtomicU32::new(1.0f32.to_bits()))),
                events: Vec::new(),
            }
        }

        fn push(&self, frames: usize) {
            let bytes: Vec<u8> = std::iter::repeat_n(0x11u8, frames * FRAME_BYTES).collect();
            self.feed.push(&bytes, usize::MAX);
        }

        fn service(&mut self) {
            let ev = &mut self.events;
            self.core.service(&mut |e| ev.push(e));
        }

        fn tuning() -> OutputTuning {
            OutputTuning {
                prime_ms: 200,
                low_periods: 2,
                queue_ms: 1000,
                pad_ms: 100,
                drain_slack_ms: 0,
                slice_ms: 10,
            }
        }
    }

    #[test]
    fn primes_before_writing_and_reports_started() {
        let mut r = Rig::new(Rig::tuning());
        r.core.begin(7, Arc::clone(&r.feed));
        r.push(480); // 10 ms
        r.service();
        assert!(r.events.is_empty());
        assert!(r.core.info.is_none());
        // A full prime opens the device and reports Started; nothing is written while the fake
        // device has room.
        r.push(9600); // 200 ms
        r.service();
        assert_eq!(r.events, [OutEvent::Started(7)]);
        assert!(r.core.info.is_some());
        assert!(r.core.device.written_frames > 0);
    }

    #[test]
    fn end_of_item_pads_then_plays_out() {
        let mut r = Rig::new(Rig::tuning());
        r.core.begin(1, Arc::clone(&r.feed));
        r.push(9600);
        r.service();
        r.events.clear();
        r.feed.eof();
        r.service();
        // The pad is one period per call until done; the device still holds frames.
        for _ in 0..60 {
            r.service();
            if !r.events.is_empty() {
                break;
            }
        }
        assert!(r.events.is_empty(), "played out before the device drained");
        for _ in 0..10 {
            r.core.device.play(usize::MAX);
            r.service();
            if !r.events.is_empty() {
                break;
            }
        }
        assert_eq!(r.events, [OutEvent::PlayedOut(1)]);
    }

    #[test]
    fn stall_keeps_the_device_fed_with_silence() {
        let mut r = Rig::new(Rig::tuning());
        r.core.begin(1, Arc::clone(&r.feed));
        r.push(9600);
        r.service();
        // Write the rest of the prime, then drain the device: an empty feed below two periods is
        // a stall, and the writer keeps the device fed with silence.
        r.core.device.play(usize::MAX);
        r.service();
        r.core.device.play(usize::MAX);
        let before = r.core.device.written_frames;
        r.service();
        assert!(r.core.device.written_frames > before);
        assert_eq!(r.core.phase, Phase::Priming);
    }

    #[test]
    fn underrun_is_recovered_and_the_item_continues() {
        let mut r = Rig::new(Rig::tuning());
        r.core.begin(1, Arc::clone(&r.feed));
        r.push(9600);
        r.core.device.fail_write = Some(-libc::EPIPE);
        r.service();
        assert!(r.core.device.prepared > 0);
        assert_eq!(r.events, [OutEvent::Started(1)]);
    }

    #[test]
    fn a_failed_open_fails_the_item() {
        let mut r = Rig::new(Rig::tuning());
        r.core.device.fail_open = Some(crate::device::Step::Open);
        r.core.begin(3, Arc::clone(&r.feed));
        r.push(9600);
        r.service();
        assert_eq!(r.events, [OutEvent::Failed(3)]);
    }
}
