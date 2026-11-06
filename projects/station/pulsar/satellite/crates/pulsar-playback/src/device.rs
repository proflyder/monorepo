//! The playback PCM device port and its ALSA implementation: opening with the output gates on,
//! the hardware/software parameters every first-party writer uses, xrun recovery and the
//! reopen backoff shared by the writers.

use std::fmt;

use pulsar_alsa::{AlsaError, Format, HwRequest, HwStep, Pcm, PlaybackSpec};

/// Where a device operation failed, for [`PcmError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Open,
    HwParams,
    SwParams,
    Prepare,
    Write,
    Avail,
    Delay,
    Wait,
    Drop,
    Resume,
    Recover,
}

impl Step {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::HwParams => "hw_params",
            Self::SwParams => "sw_params",
            Self::Prepare => "prepare",
            Self::Write => "write",
            Self::Avail => "avail_update",
            Self::Delay => "delay",
            Self::Wait => "wait",
            Self::Drop => "drop",
            Self::Resume => "resume",
            Self::Recover => "recover",
        }
    }
}

/// A failed PCM operation: the step and the negative errno (`-EAGAIN`, `-EPIPE`, …).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcmError {
    /// The caller's step context (`hw_params` for any hw-parameter failure).
    pub step: Step,
    /// The exact ALSA step, when the error came from a device call.
    pub detail: Option<HwStep>,
    pub code: i32,
}

impl PcmError {
    pub fn new(step: Step, code: i32) -> Self {
        Self {
            step,
            detail: None,
            code,
        }
    }

    pub fn from_alsa(step: Step, e: AlsaError) -> Self {
        Self {
            step,
            detail: Some(e.op),
            code: e.code,
        }
    }

    /// The step a writer names before `failed`: the exact hw step for `hw_params`, the outer step
    /// otherwise (open / hw_params / …), matching the previous per-writer messages.
    pub fn context(&self) -> &'static str {
        if self.step == Step::HwParams {
            self.detail.map_or(Step::HwParams.as_str(), HwStep::as_str)
        } else {
            self.step.as_str()
        }
    }

    /// `EINTR` / `EAGAIN`: the same call can be retried as is.
    pub fn is_transient(self) -> bool {
        self.code == -libc::EINTR || self.code == -libc::EAGAIN
    }

    /// `EPIPE` (underrun) or `ESTRPIPE` (suspend): the device needs prepare / resume.
    pub fn is_xrun(self) -> bool {
        self.code == -libc::EPIPE || self.code == -libc::ESTRPIPE
    }
}

impl fmt::Display for PcmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}",
            self.step.as_str(),
            pulsar_alsa::strerror(self.code)
        )
    }
}

impl std::error::Error for PcmError {}

/// Format and geometry of one open playback PCM.
#[derive(Clone, Copy)]
pub struct PcmRequest {
    pub format: Format,
    pub channels: u32,
    pub rate: u32,
    /// true: the rate must be supported as is; false: the nearest rate is accepted.
    pub rate_exact: bool,
}

/// What the device reported after [`PcmDevice::open`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcmInfo {
    pub rate: u32,
    pub period_frames: usize,
    pub buffer_frames: usize,
}

/// A playback PCM the writers drive. Implemented by the firmware's ALSA (behind dlopen) and, for
/// the policy tests, by an in-memory [`FakeDevice`](crate::FakeDevice).
pub trait PcmDevice: Send {
    /// The PCM name of the profile, for log lines.
    fn name(&self) -> &'static str;
    fn is_open(&self) -> bool;
    fn open(&mut self, req: &PcmRequest) -> Result<PcmInfo, PcmError>;
    fn info(&self) -> Option<PcmInfo>;
    /// Writes whole frames; returns the frames taken.
    fn write(&mut self, data: &[u8], frames: usize) -> Result<usize, PcmError>;
    fn avail(&mut self) -> Result<usize, PcmError>;
    /// Frames still queued ahead of the DAC; negative when the device has no such reading.
    fn delay(&mut self) -> Result<i64, PcmError>;
    fn wait(&mut self, timeout_ms: i32) -> Result<bool, PcmError>;
    fn resume(&mut self) -> Result<(), PcmError>;
    fn prepare(&mut self) -> Result<(), PcmError>;
    fn drop_frames(&mut self) -> Result<(), PcmError>;
    fn close(&mut self);
}

/// The model's playback PCM: open_playback (output gates on) and the stock software parameters.
pub struct AlsaDevice {
    spec: PlaybackSpec,
    pcm: Option<Pcm>,
    info: Option<PcmInfo>,
}

impl AlsaDevice {
    pub fn new(spec: PlaybackSpec) -> Self {
        Self {
            spec,
            pcm: None,
            info: None,
        }
    }

    pub fn from_playback(playback: &'static pulsar_model::Playback) -> Self {
        Self::new(PlaybackSpec {
            pcm: playback.pcm,
            gates: pulsar_alsa::OutputGates {
                amp_enable_paths: playback.amp_enable_paths,
                spk_mute_ctl: playback.spk_mute_ctl,
            },
        })
    }

    pub fn is_open(&self) -> bool {
        self.pcm.is_some()
    }

    /// The PCM name of the model profile (for log lines).
    pub fn name(&self) -> &'static str {
        self.spec.pcm
    }
}

impl PcmDevice for AlsaDevice {
    fn name(&self) -> &'static str {
        self.spec.pcm
    }

    fn is_open(&self) -> bool {
        self.pcm.is_some()
    }

    fn open(&mut self, req: &PcmRequest) -> Result<PcmInfo, PcmError> {
        let pcm = pulsar_alsa::open_playback(&self.spec)
            .map_err(|e| PcmError::from_alsa(Step::Open, e))?;
        // Kept even when the parameters below fail: the caller has to close the device it just
        // opened (and the sendspin sink logs that close, as before).
        self.pcm = Some(pcm);
        let pcm = self
            .pcm
            .as_mut()
            .ok_or(PcmError::new(Step::Open, -libc::ENODEV))?;
        let hw = pcm
            .set_hw_params(&HwRequest {
                format: req.format,
                channels: req.channels,
                rate: req.rate,
                rate_exact: req.rate_exact,
                period_frames: None,
                buffer_frames: None,
            })
            .map_err(|e| PcmError::from_alsa(Step::HwParams, e))?;
        let period = hw.period_frames.max(1);
        pcm.set_sw_params(period, period)
            .map_err(|e| PcmError::from_alsa(Step::SwParams, e))?;
        pcm.prepare()
            .map_err(|e| PcmError::from_alsa(Step::Prepare, e))?;
        let info = PcmInfo {
            rate: hw.rate,
            period_frames: usize::try_from(period).unwrap_or(usize::MAX),
            buffer_frames: usize::try_from(hw.buffer_frames).unwrap_or(usize::MAX),
        };
        self.info = Some(info);
        Ok(info)
    }

    fn info(&self) -> Option<PcmInfo> {
        self.info
    }

    fn write(&mut self, data: &[u8], frames: usize) -> Result<usize, PcmError> {
        self.pcm
            .as_mut()
            .ok_or(PcmError::new(Step::Write, -libc::ENODEV))?
            .writei(data, frames)
            .map_err(|e| PcmError::from_alsa(Step::Write, e))
    }

    fn avail(&mut self) -> Result<usize, PcmError> {
        self.pcm
            .as_mut()
            .ok_or(PcmError::new(Step::Avail, -libc::ENODEV))?
            .avail_update()
            .map_err(|e| PcmError::from_alsa(Step::Avail, e))
    }

    fn delay(&mut self) -> Result<i64, PcmError> {
        self.pcm
            .as_mut()
            .ok_or(PcmError::new(Step::Delay, -libc::ENODEV))?
            .delay()
            .map_err(|e| PcmError::from_alsa(Step::Delay, e))
    }

    fn wait(&mut self, timeout_ms: i32) -> Result<bool, PcmError> {
        self.pcm
            .as_mut()
            .ok_or(PcmError::new(Step::Wait, -libc::ENODEV))?
            .wait(timeout_ms)
            .map_err(|e| PcmError::from_alsa(Step::Wait, e))
    }

    fn resume(&mut self) -> Result<(), PcmError> {
        self.pcm
            .as_mut()
            .ok_or(PcmError::new(Step::Resume, -libc::ENODEV))?
            .resume()
            .map_err(|e| PcmError::from_alsa(Step::Resume, e))
    }

    fn prepare(&mut self) -> Result<(), PcmError> {
        self.pcm
            .as_mut()
            .ok_or(PcmError::new(Step::Prepare, -libc::ENODEV))?
            .prepare()
            .map_err(|e| PcmError::from_alsa(Step::Prepare, e))
    }

    fn drop_frames(&mut self) -> Result<(), PcmError> {
        self.pcm
            .as_mut()
            .ok_or(PcmError::new(Step::Drop, -libc::ENODEV))?
            .drop_frames()
            .map_err(|e| PcmError::from_alsa(Step::Drop, e))
    }

    fn close(&mut self) {
        self.pcm = None;
        self.info = None;
    }
}

/// What [`recover`] did with a failed operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// `EINTR` / `EAGAIN`: repeat the call.
    Retry,
    /// The underrun was prepared away.
    Underrun,
    /// The suspend was resumed.
    Suspend,
    /// Fatal: the device is closed and must be reopened.
    Fatal(PcmError),
}

/// Retries a failed operation after an underrun (prepare) or a suspend (resume). The caller logs
/// the outcome with its own prefix, so every writer keeps the text it had before.
pub fn recover(dev: &mut dyn PcmDevice, err: PcmError, xruns: &mut u32) -> Recovery {
    if err.is_transient() {
        return Recovery::Retry;
    }
    if err.is_xrun() {
        *xruns = xruns.saturating_add(1);
        let mut r = if err.code == -libc::ESTRPIPE {
            dev.resume()
        } else {
            Err(PcmError::new(Step::Recover, -1))
        };
        if r.is_err() {
            r = dev.prepare();
        }
        return match r {
            Ok(()) if err.code == -libc::EPIPE => Recovery::Underrun,
            Ok(()) => Recovery::Suspend,
            Err(e) => {
                dev.close();
                Recovery::Fatal(e)
            }
        };
    }
    dev.close();
    Recovery::Fatal(err)
}

/// A fixed-delay reopen gate: after a failed open / lost device the next attempt waits `retry_ms`.
#[derive(Clone, Copy, Debug)]
pub struct Backoff {
    retry_us: i64,
    next_us: i64,
}

impl Backoff {
    pub fn new(retry_ms: u32) -> Self {
        Self {
            retry_us: i64::from(retry_ms) * 1000,
            next_us: 0,
        }
    }

    pub fn ready(&self, now_us: i64) -> bool {
        now_us >= self.next_us
    }

    /// The next attempt waits a full period.
    pub fn defer(&mut self, now_us: i64) {
        self.next_us = now_us + self.retry_us;
    }

    pub fn reset(&mut self) {
        self.next_us = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FakeDevice;

    fn dev() -> FakeDevice {
        let mut d = FakeDevice::new(4800, 1200);
        let _ = d.open(&PcmRequest {
            format: Format::S16Le,
            channels: 2,
            rate: 48000,
            rate_exact: true,
        });
        d
    }

    #[test]
    fn underrun_is_recovered_by_prepare() {
        let mut d = dev();
        let mut xruns = 0;
        let e = PcmError::new(Step::Write, -libc::EPIPE);
        assert_eq!(recover(&mut d, e, &mut xruns), Recovery::Underrun);
        assert_eq!(d.prepared, 1);
        assert_eq!(xruns, 1);
        assert!(d.open);
    }

    #[test]
    fn suspend_is_recovered_by_resume() {
        let mut d = dev();
        let mut xruns = 0;
        let e = PcmError::new(Step::Write, -libc::ESTRPIPE);
        assert_eq!(recover(&mut d, e, &mut xruns), Recovery::Suspend);
        assert_eq!(d.resumed, 1);
        assert_eq!(d.prepared, 0);
    }

    #[test]
    fn a_transient_error_is_retried_without_counting() {
        let mut d = dev();
        let mut xruns = 0;
        let e = PcmError::new(Step::Write, -libc::EAGAIN);
        assert_eq!(recover(&mut d, e, &mut xruns), Recovery::Retry);
        assert_eq!(xruns, 0);
        assert_eq!(d.prepared, 0);
    }

    #[test]
    fn a_fatal_error_closes_the_device() {
        let mut d = dev();
        let mut xruns = 0;
        let e = PcmError::new(Step::Write, -libc::EIO);
        assert_eq!(recover(&mut d, e, &mut xruns), Recovery::Fatal(e));
        assert!(!d.open);
    }

    #[test]
    fn backoff_defers_the_reopen() {
        let mut b = Backoff::new(1000);
        assert!(b.ready(0));
        b.defer(0);
        assert!(!b.ready(999_999));
        assert!(b.ready(1_000_000));
        b.reset();
        assert!(b.ready(1_000_000));
    }
}
