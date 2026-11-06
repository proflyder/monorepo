//! Захват ALSA: PCM модели, FIFO и поток `audio-capture`.

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc::Receiver;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use pulsar_alsa::{AlsaError, Format, HwRequest, HwStep, HwTrace, Pcm, Stream};
use pulsar_dsp::Ring;
use pulsar_model::{Capture as CaptureLayout, CaptureFormat};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{error, info, warn};

use super::cmd::CaptureCmd;
use super::ports::CaptureSource;
use super::{FrontendStats, lock};

/// ALSA buffer in VQE chunks: 256 ms with 32 ms chunks, 128 ms on the Mini 3 and Mini (16 ms);
/// the device may grant less (Lite 2: 2048 frames).
const BUFFER_CHUNKS: u64 = 8;
const READ_WAIT_MS: i32 = 200;
pub(super) const REOPEN_DELAY: Duration = Duration::from_secs(2);
// Шаг, с которым спящий поток проверяет остановку: задержка `pause` не должна задерживать
// `Drop` на весь таймаут ALSA.
const STOP_POLL: Duration = Duration::from_millis(50);

pub(super) struct Pipe {
    pub(super) fifo: Mutex<Ring<i16>>,
    pub(super) ready: Condvar,
}

enum CaptureBuf {
    S16(Vec<i16>),
    /// Raw S32 frames and their 16-bit conversion.
    S32 {
        raw: Vec<i32>,
        out: Vec<i16>,
        shift: u32,
    },
}

impl CaptureBuf {
    fn new(format: CaptureFormat, samples: usize) -> Self {
        match format {
            CaptureFormat::S16 => Self::S16(vec![0; samples]),
            CaptureFormat::S32 { shift } => Self::S32 {
                raw: vec![0; samples],
                out: vec![0; samples],
                shift,
            },
        }
    }

    fn read(&mut self, pcm: &mut Pcm, frames: usize) -> Result<usize, AlsaError> {
        match self {
            Self::S16(buf) => pcm.readi(buf, frames),
            Self::S32 { raw, .. } => pcm.readi_s32(raw, frames),
        }
    }

    /// The first `samples` samples read, as the VQE's 16-bit input.
    fn samples(&mut self, samples: usize) -> &[i16] {
        match self {
            Self::S16(buf) => &buf[..samples],
            Self::S32 { raw, out, shift } => {
                s32_to_s16(&raw[..samples], *shift, &mut out[..samples]);
                &out[..samples]
            }
        }
    }
}

/// S32_LE carries a right-justified 24-bit sample on the Mini: shifting out the low `shift` bits
/// keeps the sign extension and gives the VQE's 16-bit input; anything wider saturates.
fn s32_to_s16(src: &[i32], shift: u32, dst: &mut [i16]) {
    for (d, &s) in dst.iter_mut().zip(src) {
        *d = (s >> shift).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    }
}

pub(super) struct AlsaCapture {
    pcm: Pcm,
    buf: CaptureBuf,
    dev: &'static str,
    pub(super) nch: usize,
    /// Frames to read at once, as the device granted (never more than one VQE chunk).
    pub(super) period: usize,
    last_error: i32,
}

impl AlsaCapture {
    /// Opens the model's capture PCM: its sample width (S16 or S32), channels, exactly its rate (a
    /// silent rate mismatch would feed the VQE and spotters wrongly timed audio), period = one VQE
    /// chunk, buffer = 8. The period may differ (the FIFO decouples ALSA periods from VQE chunks).
    pub(super) fn open(capture: &CaptureLayout, period: usize) -> Result<Self, CaptureError> {
        let dev = capture.pcm;
        let format = match capture.format {
            CaptureFormat::S16 => Format::S16Le,
            CaptureFormat::S32 { .. } => Format::S32Le,
        };
        // Non-blocking: reads never wait inside ALSA, so a stop never hangs in snd_pcm_readi.
        let mut pcm = Pcm::open(dev, Stream::Capture, true)
            .map_err(|source| CaptureError::Alsa { pcm: dev, source })?;
        let t = pcm.set_hw_params_traced(&HwRequest {
            format,
            channels: capture.channels,
            rate: capture.rate,
            rate_exact: false,
            period_frames: Some(period as u64),
            buffer_frames: Some(period as u64 * BUFFER_CHUNKS),
        });
        match hw_outcome(&t, capture.rate) {
            Ok((per, buf)) => {
                info!(
                    "capture {dev}: {} Hz, {} ch, {} bits, period {per}, buffer {buf}",
                    t.near.rate,
                    capture.channels,
                    capture.format.bits()
                );
                let per = usize::try_from(per).unwrap_or(usize::MAX);
                let nch = capture.channels as usize;
                let period = per.min(period);
                Ok(Self {
                    pcm,
                    buf: CaptureBuf::new(capture.format, period * nch),
                    dev,
                    nch,
                    period,
                    last_error: 0,
                })
            }
            Err(HwFail::Step(op, code)) => Err(CaptureError::Alsa {
                pcm: dev,
                source: AlsaError::new(op, code),
            }),
            Err(HwFail::Rate(got)) => Err(CaptureError::Rate {
                pcm: dev,
                got,
                want: capture.rate,
            }),
        }
    }
}

impl CaptureSource for AlsaCapture {
    fn read(&mut self, out: &mut [i16]) -> Result<usize, CaptureError> {
        let frames = self.period.min(out.len() / self.nch.max(1));
        let r = self.buf.read(&mut self.pcm, frames);
        match r {
            Ok(n) => {
                let samples = n * self.nch;
                out[..samples].copy_from_slice(self.buf.samples(samples));
                Ok(n)
            }
            // EAGAIN: the caller waits on the device instead of spinning.
            Err(e) if e == AlsaError::new(HwStep::Read, -libc::EAGAIN) => Ok(0),
            Err(source) => {
                self.last_error = source.code;
                Err(CaptureError::Alsa {
                    pcm: self.dev,
                    source,
                })
            }
        }
    }

    fn wait(&mut self, ms: i32) -> Result<(), CaptureError> {
        self.pcm
            .wait(ms)
            .map(|_| ())
            .map_err(|source| CaptureError::Alsa {
                pcm: self.dev,
                source,
            })
    }

    fn recover(&mut self) -> bool {
        self.pcm.recover(self.last_error, true).is_ok()
    }
}

fn pause(running: &AtomicBool, d: Duration) {
    let mut left = d;
    while running.load(Relaxed) && !left.is_zero() {
        let step = left.min(STOP_POLL);
        std::thread::sleep(step);
        left -= step;
    }
}

/// Capture loop with its port opener: production opens ALSA, a test can script frames, errors
/// and the reopen result.
pub(super) struct CaptureLoop<'a> {
    pub(super) open: &'a mut dyn FnMut() -> Result<Box<dyn CaptureSource>, CaptureError>,
    pub(super) nch: usize,
    pub(super) period: usize,
    pub(super) running: &'a AtomicBool,
    pub(super) pipe: &'a Pipe,
    pub(super) stats: &'a Mutex<FrontendStats>,
    pub(super) tap_rx: Receiver<CaptureCmd>,
    pub(super) reopen_delay: Duration,
}

impl CaptureLoop<'_> {
    pub(super) fn run(self) {
        let mut source: Option<Box<dyn CaptureSource>> = None;
        let mut tap: Option<UnboundedSender<Vec<i16>>> = None;
        let mut buf = vec![0i16; self.period * self.nch];
        while self.running.load(Relaxed) {
            while let Ok(CaptureCmd::Tap(t)) = self.tap_rx.try_recv() {
                tap = t;
            }
            let Some(src) = source.as_mut() else {
                match (self.open)() {
                    Ok(s) => source = Some(s),
                    Err(e) => {
                        error!("{e}");
                        pause(self.running, self.reopen_delay);
                    }
                }
                continue;
            };
            let n = match src.read(&mut buf) {
                // EAGAIN: wait on the device, then re-check the stop flag.
                Ok(0) => {
                    if let Err(e) = src.wait(READ_WAIT_MS) {
                        warn!("capture: {e}");
                    }
                    continue;
                }
                Ok(n) => n,
                Err(e) => {
                    warn!("capture: {e}, recovering");
                    lock(&self.pipe.fifo).drop_partial();
                    if !src.recover() {
                        source = None;
                    }
                    continue;
                }
            };
            let block = &buf[..n * self.nch];
            lock(self.stats).captured_frames += n as u64;
            if let Some(t) = &tap {
                let _ = t.send(block.to_vec());
            }
            let dropped = {
                let mut f = lock(&self.pipe.fifo);
                let dropped = f.push(block).map(|items| items / self.nch);
                lock(self.stats).fifo_frames = (f.len() / self.nch) as u64;
                dropped
            };
            self.pipe.ready.notify_one();
            if let Some(d) = dropped {
                lock(self.stats).dropped_frames += d as u64;
                warn!("capture: processing behind, dropping {d} frames");
            }
        }
    }
}

/// The failing capture hw_params step and its error, or the rate the device offered instead of
/// the model's.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum HwFail {
    Step(HwStep, i32),
    Rate(u32),
}

/// Outcome of the capture hw_params in the order the C++ daemon checked them: the rate right
/// after set_rate_near, then period, buffer, apply (the period / buffer read back afterwards were
/// not checked there). Ok = the period and buffer the device chose.
pub(super) fn hw_outcome(t: &HwTrace, rate: u32) -> Result<(u64, u64), HwFail> {
    match t.failed {
        Some((
            op @ (HwStep::Any | HwStep::Access | HwStep::Format | HwStep::Channels | HwStep::Rate),
            e,
        )) => Err(HwFail::Step(op, e)),
        _ if t.near.rate != rate => Err(HwFail::Rate(t.near.rate)),
        Some((op @ (HwStep::Period | HwStep::Buffer | HwStep::Apply), e)) => {
            Err(HwFail::Step(op, e))
        }
        _ => Ok((t.near.period_frames, t.near.buffer_frames)),
    }
}

/// Почему захват не открылся: шаг ALSA или не та частота, на которую согласился драйвер.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("capture {pcm}: {source}")]
    Alsa {
        pcm: &'static str,
        #[source]
        source: AlsaError,
    },
    #[error("capture {pcm}: rate {got} Hz, need {want}")]
    Rate {
        pcm: &'static str,
        got: u32,
        want: u32,
    },
}

pub(super) fn deinterleave(
    input: &[i16],
    nch: usize,
    mic_ch: &[u32],
    spk_ch: &[u32],
    mics: &mut [f32],
    spk: &mut [f32],
) {
    let (nmic, nspk) = (mic_ch.len(), spk_ch.len());
    for (i, f) in input.chunks_exact(nch).enumerate() {
        for (c, &ch) in mic_ch.iter().enumerate() {
            mics[i * nmic + c] = f32::from(f[ch as usize]) * (1.0 / 32768.0);
        }
        for (c, &ch) in spk_ch.iter().enumerate() {
            spk[i * nspk + c] = f32::from(f[ch as usize]) * (1.0 / 32768.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::test_support::{ScriptRead, ScriptedCapture};
    use pulsar_alsa::HwResult;
    use pulsar_model::profile_from_compatible;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;
    use std::time::Instant;

    #[test]
    fn capture_retries_the_open_and_recovers_a_read_error() {
        let opened = Arc::new(AtomicUsize::new(0));
        let opened2 = opened.clone();
        let mut open = move || -> Result<Box<dyn CaptureSource>, CaptureError> {
            let n = opened2.fetch_add(1, Relaxed);
            if n < 2 {
                return Err(CaptureError::Alsa {
                    pcm: "script",
                    source: AlsaError::new(HwStep::Open, -16),
                });
            }
            Ok(Box::new(ScriptedCapture {
                // Первое чтение — ошибка с восстановлением, второе — чанк.
                reads: [ScriptRead::Error, ScriptRead::Block(vec![7, 8, 9, 10])].into(),
                recovered: true,
                recoveries: 0,
            }))
        };
        let running = Arc::new(AtomicBool::new(true));
        let pipe = Arc::new(Pipe {
            fifo: Mutex::new(Ring::new(4, 16)),
            ready: Condvar::new(),
        });
        let stats = Arc::new(Mutex::new(FrontendStats::default()));
        let (_tap_tx, tap_rx) = mpsc::channel();
        let handle = {
            let running = running.clone();
            let pipe = pipe.clone();
            let stats = stats.clone();
            std::thread::spawn(move || {
                CaptureLoop {
                    open: &mut open,
                    nch: 1,
                    period: 4,
                    running: &running,
                    pipe: &pipe,
                    stats: &stats,
                    tap_rx,
                    reopen_delay: Duration::from_millis(1),
                }
                .run();
            })
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while !lock(&pipe.fifo).has_chunk() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        running.store(false, Relaxed);
        handle.join().unwrap();
        assert!(
            opened.load(Relaxed) >= 3,
            "the failed opens must be retried"
        );
        assert_eq!(stats.lock().unwrap().captured_frames, 4);
        let mut out = [0i16; 4];
        assert!(lock(&pipe.fifo).take_chunk(&mut out));
        assert_eq!(out, [7, 8, 9, 10]);
    }

    #[test]
    fn s32_capture_keeps_the_top_16_of_24_bits() {
        // Right-justified 24-bit samples (the Mini, shift 8): the sign survives the shift, and
        // a value outside 24 bits saturates instead of wrapping.
        let src = [
            -1 << 8,
            0x007F_FF00,
            -0x0080_0000,
            0x0000_00FF,
            -1,
            0x0100_0000,
            i32::MIN,
        ];
        let mut dst = [0i16; 7];
        s32_to_s16(&src, 8, &mut dst);
        assert_eq!(dst, [-1, 32767, -32768, 0, -1, 32767, -32768]);
    }

    #[test]
    fn deinterleave_supported_layouts() {
        let midi = profile_from_compatible(b"yandex,cucumber\0").unwrap();
        let lite2 = profile_from_compatible(b"yandex,plum\0").unwrap();
        let mini3 = profile_from_compatible(b"yandex,bergamot\0").unwrap();
        for m in [midi, lite2, mini3] {
            let cap = &m.audio.capture;
            let nch = cap.channels as usize;
            let frames = 4;
            // Sample = frame * 100 + channel.
            let input: Vec<i16> = (0..frames * nch)
                .map(|s| ((s / nch) * 100 + s % nch) as i16)
                .collect();
            let mut mics = vec![0.0; frames * cap.mic_channels.len()];
            let mut spk = vec![0.0; frames * cap.spk_channels.len()];
            deinterleave(
                &input,
                nch,
                cap.mic_channels,
                cap.spk_channels,
                &mut mics,
                &mut spk,
            );
            let v = |frame: usize, ch: u32| (frame * 100 + ch as usize) as f32 / 32768.0;
            for i in 0..frames {
                for (c, &ch) in cap.mic_channels.iter().enumerate() {
                    assert_eq!(mics[i * cap.mic_channels.len() + c], v(i, ch));
                }
                for (c, &ch) in cap.spk_channels.iter().enumerate() {
                    assert_eq!(spk[i * cap.spk_channels.len() + c], v(i, ch));
                }
            }
        }
        // Lite 2: mic 0 on ch 0, mic 1 on ch 2, speaker feedback on ch 6.
        let input: Vec<i16> = vec![
            -32768, 1, 16384, 3, 4, 5, 32767, 7, //
            8, 9, 10, 11, 12, 13, 14, 15,
        ];
        let (mut mics, mut spk) = ([0.0; 4], [0.0; 2]);
        deinterleave(
            &input,
            8,
            lite2.audio.capture.mic_channels,
            lite2.audio.capture.spk_channels,
            &mut mics,
            &mut spk,
        );
        assert_eq!(mics, [-1.0, 0.5, 8.0 / 32768.0, 10.0 / 32768.0]);
        assert_eq!(spk, [32767.0 / 32768.0, 14.0 / 32768.0]);
    }

    fn trace(rate: u32, failed: Option<(HwStep, i32)>) -> HwTrace {
        let r = HwResult {
            rate,
            period_frames: 1536,
            buffer_frames: 12288,
        };
        HwTrace {
            near: r,
            actual: r,
            failed,
        }
    }

    #[test]
    fn hw_params_checked_in_cxx_order() {
        assert_eq!(hw_outcome(&trace(48000, None), 48000), Ok((1536, 12288)));
        // The rate is checked right after set_rate_near, before period / buffer / apply.
        assert_eq!(
            hw_outcome(&trace(44100, None), 48000),
            Err(HwFail::Rate(44100))
        );
        assert_eq!(
            hw_outcome(&trace(44100, Some((HwStep::Apply, -5))), 48000),
            Err(HwFail::Rate(44100))
        );
        assert_eq!(
            hw_outcome(&trace(44100, Some((HwStep::Channels, -22))), 48000),
            Err(HwFail::Step(HwStep::Channels, -22))
        );
        assert_eq!(
            hw_outcome(&trace(48000, Some((HwStep::Rate, -22))), 48000),
            Err(HwFail::Step(HwStep::Rate, -22))
        );
        assert_eq!(
            hw_outcome(&trace(48000, Some((HwStep::Buffer, -22))), 48000),
            Err(HwFail::Step(HwStep::Buffer, -22))
        );
        assert_eq!(
            hw_outcome(&trace(48000, Some((HwStep::Apply, -16))), 48000),
            Err(HwFail::Step(HwStep::Apply, -16))
        );
        // Reading the sizes back was not a C++ step: the near values count.
        assert_eq!(
            hw_outcome(&trace(48000, Some((HwStep::GetPeriod, -5))), 48000),
            Ok((1536, 12288))
        );
    }
}
