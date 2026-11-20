#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

#[allow(unsafe_code)]
mod ffi;

use std::fmt;

use ffi::{HwParams, MixerValue, RawCtl, RawPcm, SwParams};

pub fn init(lib: &'static str) {
    ffi::set_library_path(lib);
    if !lib.is_empty() {
        let _ = ffi::lib();
    }
}

pub fn load() -> Result<(), String> {
    ffi::lib().map(|_| ()).map_err(str::to_owned)
}

pub fn has_modern_aliases() -> bool {
    ffi::lib().is_ok_and(ffi::Lib::modern_aliases)
}

pub fn strerror(err: i32) -> String {
    ffi::lib().map_or_else(
        |_| home_linux::errno_text(err.saturating_neg()),
        |lib| lib.strerror(err),
    )
}

pub fn quiet_error_handler() {
    if let Ok(lib) = ffi::lib() {
        lib.set_quiet_error_handler();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HwStep {
    Load,
    Open,
    Any,
    Access,
    Format,
    Channels,
    Rate,
    Period,
    Buffer,
    Apply,
    GetPeriod,
    GetBuffer,
    SwParams,
    Prepare,
    Drop,
    Resume,
    Read,
    Write,
    Wait,
    AvailUpdate,
    Delay,
    Recover,
    CtlOpen,
    MixerRead,
    MixerWrite,
}

impl HwStep {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Load => "dlopen",
            Self::Open => "snd_pcm_open",
            Self::Any => "hw_params_any",
            Self::Access => "set_access",
            Self::Format => "set_format",
            Self::Channels => "set_channels",
            Self::Rate => "set_rate_near",
            Self::Period => "set_period_size_near",
            Self::Buffer => "set_buffer_size_near",
            Self::Apply => "snd_pcm_hw_params",
            Self::GetPeriod => "get_period_size",
            Self::GetBuffer => "get_buffer_size",
            Self::SwParams => "snd_pcm_sw_params",
            Self::Prepare => "snd_pcm_prepare",
            Self::Drop => "snd_pcm_drop",
            Self::Resume => "snd_pcm_resume",
            Self::Read => "snd_pcm_readi",
            Self::Write => "snd_pcm_writei",
            Self::Wait => "snd_pcm_wait",
            Self::AvailUpdate => "snd_pcm_avail_update",
            Self::Delay => "snd_pcm_delay",
            Self::Recover => "snd_pcm_recover",
            Self::CtlOpen => "snd_ctl_open",
            Self::MixerRead => "snd_ctl_elem_read",
            Self::MixerWrite => "snd_ctl_elem_write",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlsaError {
    pub op: HwStep,
    pub code: i32,
}

impl AlsaError {
    pub fn new(op: HwStep, code: i32) -> Self {
        Self { op, code }
    }

    fn of(op: HwStep, rc: i32) -> Result<(), Self> {
        if rc < 0 {
            Err(Self::new(op, rc))
        } else {
            Ok(())
        }
    }
}

impl fmt::Display for AlsaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.op.as_str(), strerror(self.code))
    }
}

impl std::error::Error for AlsaError {}

#[derive(Clone, Copy)]
pub enum Stream {
    Playback,
    Capture,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Format {
    S16Le,
    S24_3Le,
    S32Le,
}

impl Format {
    fn bytes(self) -> usize {
        match self {
            Self::S16Le => 2,
            Self::S24_3Le => 3,
            Self::S32Le => 4,
        }
    }

    fn raw(self) -> i32 {
        match self {
            Self::S16Le => ffi::FORMAT_S16_LE,
            Self::S24_3Le => ffi::FORMAT_S24_3LE,
            Self::S32Le => ffi::FORMAT_S32_LE,
        }
    }
}

pub struct HwRequest {
    pub format: Format,
    pub channels: u32,
    pub rate: u32,
    pub rate_exact: bool,
    pub period_frames: Option<u64>,
    pub buffer_frames: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HwResult {
    pub rate: u32,
    pub period_frames: u64,
    pub buffer_frames: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct HwTrace {
    pub near: HwResult,
    pub actual: HwResult,
    pub failed: Option<(HwStep, i32)>,
}

pub struct Pcm {
    raw: RawPcm,
    frame_bytes: usize,
}

impl Pcm {
    pub fn open(name: &str, stream: Stream, nonblock: bool) -> Result<Self, AlsaError> {
        let lib = ffi::lib().map_err(|_| AlsaError::new(HwStep::Load, -libc::ENOENT))?;
        let name = ffi::c_name(name).map_err(|e| AlsaError::new(HwStep::Open, e))?;
        let stream = match stream {
            Stream::Playback => ffi::STREAM_PLAYBACK,
            Stream::Capture => ffi::STREAM_CAPTURE,
        };
        let mode = if nonblock { ffi::PCM_NONBLOCK } else { 0 };
        let raw = lib
            .pcm_open(&name, stream, mode)
            .map_err(|e| AlsaError::new(HwStep::Open, e))?;
        Ok(Self {
            raw,
            frame_bytes: 0,
        })
    }

    pub fn set_hw_params(&mut self, req: &HwRequest) -> Result<HwResult, AlsaError> {
        let t = self.set_hw_params_traced(req);
        match t.failed {
            Some((op, err)) => Err(AlsaError::new(op, err)),
            None => Ok(t.actual),
        }
    }

    pub fn set_hw_params_traced(&mut self, req: &HwRequest) -> HwTrace {
        let mut t = HwTrace {
            near: HwResult {
                rate: req.rate,
                period_frames: req.period_frames.unwrap_or(0),
                buffer_frames: req.buffer_frames.unwrap_or(0),
            },
            actual: HwResult::default(),
            failed: None,
        };
        if let Err((op, code)) = self.apply_hw(req, &mut t) {
            t.failed = Some((op, code));
        }
        t
    }

    fn apply_hw(&mut self, req: &HwRequest, t: &mut HwTrace) -> Result<(), (HwStep, i32)> {
        let step = |op: HwStep, rc: i32| AlsaError::of(op, rc).map_err(|e| (e.op, e.code));
        self.frame_bytes = 0;
        let mut hw = HwParams::new(self.raw.lib()).map_err(|e| (HwStep::Any, e))?;
        let pcm = &mut self.raw;
        step(HwStep::Any, pcm.hw_any(&mut hw))?;
        step(
            HwStep::Access,
            pcm.hw_set_access(&mut hw, ffi::ACCESS_RW_INTERLEAVED),
        )?;
        step(HwStep::Format, pcm.hw_set_format(&mut hw, req.format.raw()))?;
        step(HwStep::Channels, pcm.hw_set_channels(&mut hw, req.channels))?;
        if req.rate_exact {
            step(HwStep::Rate, pcm.hw_set_rate(&mut hw, req.rate))?;
        } else {
            step(
                HwStep::Rate,
                pcm.hw_set_rate_near(&mut hw, &mut t.near.rate),
            )?;
        }
        if req.period_frames.is_some() {
            step(
                HwStep::Period,
                pcm.hw_set_period_size_near(&mut hw, &mut t.near.period_frames),
            )?;
        }
        if req.buffer_frames.is_some() {
            step(
                HwStep::Buffer,
                pcm.hw_set_buffer_size_near(&mut hw, &mut t.near.buffer_frames),
            )?;
        }
        step(HwStep::Apply, pcm.hw_apply(&mut hw))?;
        t.actual.rate = t.near.rate;
        step(
            HwStep::GetPeriod,
            hw.get_period_size(&mut t.actual.period_frames),
        )?;
        step(
            HwStep::GetBuffer,
            hw.get_buffer_size(&mut t.actual.buffer_frames),
        )?;
        self.frame_bytes = req.format.bytes() * req.channels as usize;
        Ok(())
    }

    pub fn set_sw_params(&mut self, start_threshold: u64, avail_min: u64) -> Result<(), AlsaError> {
        let mut sw =
            SwParams::new(self.raw.lib()).map_err(|e| AlsaError::new(HwStep::SwParams, e))?;
        AlsaError::of(HwStep::SwParams, self.raw.sw_current(&mut sw))?;
        AlsaError::of(
            HwStep::SwParams,
            self.raw.sw_set_start_threshold(&mut sw, start_threshold),
        )?;
        AlsaError::of(
            HwStep::SwParams,
            self.raw.sw_set_avail_min(&mut sw, avail_min),
        )?;
        AlsaError::of(HwStep::SwParams, self.raw.sw_apply(&mut sw))
    }

    pub fn prepare(&mut self) -> Result<(), AlsaError> {
        AlsaError::of(HwStep::Prepare, self.raw.prepare())
    }

    pub fn drop_frames(&mut self) -> Result<(), AlsaError> {
        AlsaError::of(HwStep::Drop, self.raw.drop_frames())
    }

    pub fn resume(&mut self) -> Result<(), AlsaError> {
        AlsaError::of(HwStep::Resume, self.raw.resume())
    }

    pub fn readi(&mut self, buf: &mut [i16], frames: usize) -> Result<usize, AlsaError> {
        self.raw
            .readi(buf, frames, self.frame_bytes)
            .map_err(|e| AlsaError::new(HwStep::Read, e))
    }

    pub fn readi_s32(&mut self, buf: &mut [i32], frames: usize) -> Result<usize, AlsaError> {
        self.raw
            .readi_s32(buf, frames, self.frame_bytes)
            .map_err(|e| AlsaError::new(HwStep::Read, e))
    }

    pub fn writei(&mut self, buf: &[u8], frames: usize) -> Result<usize, AlsaError> {
        self.raw
            .writei(buf, frames, self.frame_bytes)
            .map_err(|e| AlsaError::new(HwStep::Write, e))
    }

    pub fn wait(&mut self, timeout_ms: i32) -> Result<bool, AlsaError> {
        let rc = self.raw.wait(timeout_ms);
        AlsaError::of(HwStep::Wait, rc).map(|()| rc > 0)
    }

    pub fn avail_update(&mut self) -> Result<usize, AlsaError> {
        self.raw
            .avail_update()
            .map_err(|e| AlsaError::new(HwStep::AvailUpdate, e))
    }

    pub fn delay(&mut self) -> Result<i64, AlsaError> {
        self.raw
            .delay()
            .map_err(|e| AlsaError::new(HwStep::Delay, e))
    }

    pub fn recover(&mut self, err: i32, silent: bool) -> Result<(), AlsaError> {
        AlsaError::of(HwStep::Recover, self.raw.recover(err, silent))
    }
}

pub fn set_mixer_integer(name: &str, values: &[i64]) -> Result<(), AlsaError> {
    let ctl = Ctl::open("hw:0")?;
    ctl.write_integers(name, values)
}

#[derive(Clone, Copy, Debug)]
pub struct OutputGates {
    pub amp_enable_paths: &'static [&'static str],
    pub spk_mute_ctl: Option<&'static str>,
}

#[derive(Clone, Copy, Debug)]
pub struct PlaybackSpec {
    pub pcm: &'static str,
    pub gates: OutputGates,
}

pub fn enable_output(gates: &OutputGates) {
    for path in gates.amp_enable_paths {
        if !home_linux::write_file(path, "1\n") {
            tracing::warn!("amp: cannot write '{path}'");
        }
    }
    if let Some(ctl) = gates.spk_mute_ctl
        && let Err(e) = set_mixer_integer(ctl, &[1])
    {
        tracing::warn!("spk: cannot unmute '{ctl}': {e}");
    }
}

pub fn open_playback(spec: &PlaybackSpec) -> Result<Pcm, AlsaError> {
    enable_output(&spec.gates);
    let pcm = Pcm::open(spec.pcm, Stream::Playback, false)?;
    enable_output(&spec.gates);
    Ok(pcm)
}

pub struct Ctl {
    raw: RawCtl,
}

impl Ctl {
    pub fn open(name: &str) -> Result<Self, AlsaError> {
        let lib = ffi::lib().map_err(|_| AlsaError::new(HwStep::Load, -libc::ENOENT))?;
        let name = ffi::c_name(name).map_err(|e| AlsaError::new(HwStep::CtlOpen, e))?;
        let raw = lib
            .ctl_open(&name)
            .map_err(|e| AlsaError::new(HwStep::CtlOpen, e))?;
        Ok(Self { raw })
    }

    pub fn mixer_exists(&self, name: &str) -> bool {
        ffi::c_name(name).is_ok_and(|name| self.raw.mixer_info(&name) == 0)
    }

    pub fn read_integer(&self, name: &str, index: u32) -> Result<i64, AlsaError> {
        let index = index as usize;
        if index >= ffi::CTL_MAX_INTEGERS {
            return Err(AlsaError::new(HwStep::MixerRead, -libc::EINVAL));
        }
        let name = ffi::c_name(name).map_err(|e| AlsaError::new(HwStep::MixerRead, e))?;
        let mut v = MixerValue::new(self.raw.lib(), &name)
            .map_err(|e| AlsaError::new(HwStep::MixerRead, e))?;
        AlsaError::of(HwStep::MixerRead, self.raw.elem_read(&mut v))?;
        Ok(v.integer(index))
    }

    pub fn write_integers(&self, name: &str, values: &[i64]) -> Result<(), AlsaError> {
        if values.len() > ffi::CTL_MAX_INTEGERS {
            return Err(AlsaError::new(HwStep::MixerWrite, -libc::EINVAL));
        }
        let name = ffi::c_name(name).map_err(|e| AlsaError::new(HwStep::MixerWrite, e))?;
        let mut v = MixerValue::new(self.raw.lib(), &name)
            .map_err(|e| AlsaError::new(HwStep::MixerWrite, e))?;
        for (i, &x) in values.iter().enumerate() {
            v.set_integer(i, x);
        }
        AlsaError::of(HwStep::MixerWrite, self.raw.elem_write(&mut v))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn load_from_missing_library_reports_dlerror() {
        let Err(err) = super::ffi::load_from("libasound-does-not-exist.so.2") else {
            panic!("a missing library loaded");
        };
        assert!(err.contains("libasound-does-not-exist.so.2"), "{err}");
        assert!(err.contains("cannot open shared object file"), "{err}");
    }

    #[test]
    fn error_display_names_the_step() {
        let e = super::AlsaError::new(super::HwStep::Channels, -22);
        let text = e.to_string();
        assert!(text.starts_with("set_channels: "), "{text}");
    }
}
