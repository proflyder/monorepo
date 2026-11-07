//! ALSA output of the Sendspin player on the model's playback PCM (dmix, mixed with the GStreamer
//! channels). `configure` / `release` run on the runtime thread (stream start / end), `write` on
//! the library's sync task; the PCM is opened lazily by `write`, so the runtime thread never
//! blocks on it. `io` is held by `write` for its whole duration, including the snd_pcm_wait
//! slices, so the runtime thread never takes it blocking: configure / release only record a
//! request (`req`, held for a few instructions) and apply it right away if the audio thread is
//! idle (try_lock); otherwise the audio thread applies it at its next wait slice and abandons the
//! write in progress.

use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
use std::sync::{Mutex, MutexGuard, PoisonError};

use pulsar_alsa::Format;
use pulsar_model::Playback;
use pulsar_playback::{AlsaDevice, Backoff, PcmDevice, PcmRequest, Recovery, Stats, recover};
use tracing::{debug, error, info};

use crate::ffi::now_us;
use crate::gain;

/// Longest snd_pcm_wait() slice inside one write.
const WAIT_SLICE_MS: i64 = 20;
const REOPEN_RETRY_MS: u32 = 1000;

#[derive(Clone, Copy)]
enum Req {
    None,
    Configure { rate: u32, channels: u8, bits: u8 },
    Release,
}

pub struct Sink {
    io: Mutex<Io>,
    req: Mutex<Req>,
    req_gen: AtomicU32,
    applied_gen: AtomicU32,
    /// f32 bits of the duck gain the output ramps to, 0..1.
    target_gain: AtomicU32,
}

/// Audio thread state (under `Sink::io`).
struct Io {
    device: AlsaDevice,
    active: bool,
    want_rate: u32,
    rate: u32,
    want_channels: u8,
    channels: u8,
    want_bits: u8,
    bits: u8,
    open_failed_logged: bool,
    backoff: Backoff,
    /// Frames consumed without reaching the device, not yet reported.
    discarded: u32,
    gain: f32,
    scratch: Vec<u8>,
    stats: Stats,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Sink {
    pub(crate) fn new(playback: &'static Playback) -> Self {
        Self {
            io: Mutex::new(Io {
                device: AlsaDevice::from_playback(playback),
                active: false,
                want_rate: 0,
                rate: 0,
                want_channels: 0,
                channels: 0,
                want_bits: 0,
                bits: 0,
                open_failed_logged: false,
                backoff: Backoff::new(REOPEN_RETRY_MS),
                discarded: 0,
                gain: 1.0,
                scratch: Vec::new(),
                stats: Stats::new(),
            }),
            req: Mutex::new(Req::None),
            req_gen: AtomicU32::new(0),
            applied_gen: AtomicU32::new(0),
            target_gain: AtomicU32::new(1.0f32.to_bits()),
        }
    }

    pub(crate) fn configure(&self, rate: u32, channels: u8, bits: u8) {
        self.request(Req::Configure {
            rate,
            channels,
            bits,
        });
    }

    /// Stream ended: drop what is queued and release the device so dmix is not held while idle.
    pub(crate) fn release(&self) {
        self.request(Req::Release);
    }

    pub(crate) fn set_gain(&self, g: f32) {
        let g = g.clamp(0.0, 1.0);
        self.target_gain.store(g.to_bits(), SeqCst);
        // No stream yet (or the device is closed): start the first frame at the new gain instead
        // of ramping down from 1.0. A desktop/phone volume change before playback is immediate.
        //
        // Never block on `io`: the sync thread holds it across a whole `write` (including
        // `pcm.wait`), and this runs in the controller's event loop on every volume step; a
        // blocking lock here would stall every channel until the write returns.
        if let Ok(mut io) = self.io.try_lock()
            && (!io.active || !io.device.is_open())
        {
            io.gain = g;
        }
    }

    fn target_gain(&self) -> f32 {
        f32::from_bits(self.target_gain.load(SeqCst))
    }

    /// A write now would take the audio: a stream is configured and no request is pending.
    ///
    /// Readiness is currently observed through `Status`; kept as the sink-side
    /// probe for callers that gate on the device instead.
    #[allow(dead_code)]
    pub fn accepts_audio(&self) -> bool {
        !self.pending() && lock(&self.io).active
    }

    /// Sync task: (bytes consumed, frames played to report, their finish time in µs).
    pub(crate) fn write(&self, data: &[u8], timeout_ms: u32) -> (usize, u32, i64) {
        let r = self.write_once(data, timeout_ms);
        // A request that arrived while `io` was held (request()'s try_lock failed) must not wait
        // for the next write: a release may be the last event, and the device would stay open.
        if self.pending() {
            self.apply_pending(&mut lock(&self.io));
        }
        r
    }

    fn pending(&self) -> bool {
        self.req_gen.load(SeqCst) != self.applied_gen.load(SeqCst)
    }

    fn write_once(&self, data: &[u8], timeout_ms: u32) -> (usize, u32, i64) {
        let length = data.len();
        let deadline = now_us() + i64::from(timeout_ms) * 1000;
        let mut frames_done: u32 = 0;
        let mut finish_us = 0i64;
        let mut report = 0u32;
        let mut io = lock(&self.io);
        self.apply_pending(&mut io);
        let bpf = usize::from(io.want_channels) * usize::from(io.want_bits / 8);
        if !io.active || bpf == 0 {
            return (length, 0, 0); // stale audio after stream end: discard
        }
        if !io.device.is_open() && !io.open() {
            // Device unavailable: discard rather than spin the task; the gap is reported as
            // played with the next timed report so the library's buffered-frames estimate stays
            // right.
            io.discarded = io.discarded.wrapping_add((length / bpf) as u32);
            return (length - length % bpf, 0, 0);
        }
        let frames_total = length / bpf;
        let mut scratch = std::mem::take(&mut io.scratch);
        let src: &[u8] = if io.scale(data, frames_total, self.target_gain(), &mut scratch) {
            &scratch
        } else {
            data
        };

        let mut first = true;
        while (frames_done as usize) < frames_total {
            if !first && now_us() >= deadline {
                break;
            }
            first = false;
            let avail = match io.device.avail() {
                Ok(n) => n,
                Err(e) => {
                    if !io.recover(e) {
                        break;
                    }
                    continue;
                }
            };
            if avail == 0 {
                let left_ms = (deadline - now_us()) / 1000;
                if left_ms <= 0 {
                    break;
                }
                match io.device.wait(left_ms.min(WAIT_SLICE_MS) as i32) {
                    Ok(_) => {}
                    Err(e) => {
                        if !io.recover(e) {
                            break;
                        }
                    }
                }
                if self.pending() {
                    // Stream restarted / ended: this audio is stale.
                    io.scratch = scratch;
                    self.apply_pending(&mut io);
                    return (length, 0, 0);
                }
                continue;
            }
            let done = frames_done as usize;
            let chunk = (frames_total - done).min(avail);
            match io.device.write(&src[done * bpf..], chunk) {
                Ok(w) => frames_done += w as u32,
                Err(e) => {
                    if !io.recover(e) {
                        break;
                    }
                }
            }
        }
        io.scratch = scratch;
        // Commit the ramp by the frames actually written; an unwritten tail is presented again.
        io.gain = gain::ramp_end_gain(io.gain, self.target_gain(), io.rate, frames_done);

        if frames_done > 0 {
            // The frames just written finish playing once everything queued ahead of the DAC
            // (our ring + plug/rate converter + dmix ring) has played: now + snd_pcm_delay().
            if let Ok(delay) = io.device.delay()
                && delay >= 0
            {
                let now = now_us();
                finish_us = now + delay * 1_000_000 / i64::from(io.rate);
                io.stats.record_delay(delay, now);
            }
        }
        io.stats.add_frames(frames_done as usize);
        let (channels, gain) = (io.channels, io.gain);
        if let Some(text) = io.stats.maybe_report(now_us(), channels, gain) {
            info!("sendspin: sink {text}");
        }
        let lost = !io.device.is_open();
        if lost && frames_done == 0 {
            io.discarded = io.discarded.wrapping_add((length / bpf) as u32);
        }
        if finish_us != 0 {
            report = frames_done.wrapping_add(io.discarded);
            io.discarded = 0;
        }
        drop(io);
        if lost && frames_done == 0 {
            return (length - length % bpf, report, finish_us); // device lost mid-write: discard
        }
        (frames_done as usize * bpf, report, finish_us)
    }

    fn request(&self, r: Req) {
        {
            let mut req = lock(&self.req);
            *req = r;
            self.req_gen.fetch_add(1, SeqCst);
            drop(req);
        }
        let io = match self.io.try_lock() {
            Ok(io) => Some(io),
            Err(std::sync::TryLockError::Poisoned(p)) => Some(p.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        };
        if let Some(mut io) = io {
            self.apply_pending(&mut io);
        }
    }

    fn apply_pending(&self, io: &mut Io) {
        let r = {
            let mut req = lock(&self.req);
            self.applied_gen.store(self.req_gen.load(SeqCst), SeqCst);
            std::mem::replace(&mut *req, Req::None)
        };
        match r {
            Req::None => {}
            Req::Configure {
                rate,
                channels,
                bits,
            } => io.configure(rate, channels, bits, self.target_gain()),
            Req::Release => io.release(),
        }
    }
}

impl Drop for Sink {
    fn drop(&mut self) {
        self.io
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .close();
    }
}

impl Io {
    fn configure(&mut self, rate: u32, channels: u8, bits: u8, target_gain: f32) {
        if self.device.is_open()
            && (rate != self.rate || channels != self.channels || bits != self.bits)
        {
            self.close();
        }
        if self.device.is_open() {
            // Same format: restart from an empty ring.
            let _ = self.device.drop_frames();
            if self.device.prepare().is_err() {
                self.close();
            }
        }
        self.want_rate = rate;
        self.want_channels = channels;
        self.want_bits = bits;
        self.active = true;
        self.open_failed_logged = false;
        self.backoff.reset();
        self.discarded = 0;
        self.gain = target_gain; // a new stream starts at the current duck level, no ramp
    }

    fn release(&mut self) {
        self.active = false;
        if self.device.is_open() {
            let secs = if self.rate != 0 {
                self.stats.frames_total() as f64 / f64::from(self.rate)
            } else {
                0.0
            };
            info!(
                "sendspin: stream summary: {secs:.1} s written to the device, xruns {}",
                self.stats.xruns
            );
            let _ = self.device.drop_frames();
        }
        self.close();
    }

    fn open(&mut self) -> bool {
        let now = now_us();
        if !self.backoff.ready(now) {
            return false;
        }
        let format = match self.want_bits {
            16 => Format::S16Le,
            24 => Format::S24_3Le,
            32 => Format::S32Le,
            bits => {
                error!("sendspin: unsupported bit depth {bits}");
                self.backoff.defer(now);
                return false;
            }
        };
        let req = PcmRequest {
            format,
            channels: u32::from(self.want_channels),
            rate: self.want_rate,
            rate_exact: true,
        };
        match self.device.open(&req) {
            Ok(info) => {
                self.rate = info.rate;
                self.channels = self.want_channels;
                self.bits = self.want_bits;
                self.stats.reset(info.rate, now);
                info!(
                    "sendspin: ALSA '{}' open {} Hz/{} ch/{} bit (period {}, buffer {} frames = {:.0} ms)",
                    self.playback_name(),
                    self.rate,
                    self.channels,
                    self.bits,
                    info.period_frames,
                    info.buffer_frames,
                    info.buffer_frames as f64 * 1000.0 / f64::from(self.rate)
                );
                true
            }
            Err(e) => {
                if !self.open_failed_logged {
                    error!(
                        "sendspin: ALSA '{}' {} failed for {} Hz/{} ch/{} bit: {e}",
                        self.playback_name(),
                        e.context(),
                        self.want_rate,
                        self.want_channels,
                        self.want_bits
                    );
                }
                self.open_failed_logged = true;
                if self.device.is_open() {
                    // open_playback succeeded, a parameter step failed: the stream was opened and
                    // is closed here, as the previous sink reported.
                    info!("sendspin: ALSA '{}' closed", self.playback_name());
                    self.device.close();
                }
                self.backoff.defer(now);
                false
            }
        }
    }

    fn playback_name(&self) -> &'static str {
        self.device.name()
    }

    fn close(&mut self) {
        if self.device.is_open() {
            info!("sendspin: ALSA '{}' closed", self.playback_name());
            self.device.close();
        }
    }

    fn recover(&mut self, err: pulsar_playback::PcmError) -> bool {
        match recover(&mut self.device, err, &mut self.stats.xruns) {
            Recovery::Retry => true,
            Recovery::Underrun => {
                debug!("sendspin: ALSA underrun recovered");
                true
            }
            Recovery::Suspend => {
                debug!("sendspin: ALSA suspend recovered");
                true
            }
            Recovery::Fatal(e) => {
                error!(
                    "sendspin: ALSA error {}, closing device",
                    pulsar_alsa::strerror(e.code)
                );
                info!("sendspin: ALSA '{}' closed", self.playback_name());
                self.backoff.defer(now_us());
                false
            }
        }
    }

    /// Applies the (ramping) duck gain into `out`; false: write `data` as is.
    fn scale(&self, data: &[u8], frames: usize, target: f32, out: &mut Vec<u8>) -> bool {
        if self.gain == target && target >= 1.0 {
            return false;
        }
        if self.bits != 16 {
            return false; // only 16-bit streams are advertised
        }
        gain::scale_s16(
            data,
            frames,
            usize::from(self.channels),
            self.gain,
            target,
            self.rate,
            out,
        );
        true
    }
}
