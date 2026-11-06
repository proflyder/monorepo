//! In-memory [`PcmDevice`] for policy tests: it plays what the writer queues and exposes the
//! operations the tests need to drive (play out, fail the next open / write).

use crate::device::{PcmDevice, PcmError, PcmInfo, PcmRequest, Step};

pub struct FakeDevice {
    pub buffer: usize,
    pub period: usize,
    /// Frames queued in the "device".
    pub queued: usize,
    pub open: bool,
    pub info: Option<PcmInfo>,
    pub written_frames: usize,
    pub prepared: u32,
    pub resumed: u32,
    pub closed: u32,
    /// One-shot failure of the next open / write (errno).
    pub fail_open: Option<Step>,
    pub fail_write: Option<i32>,
}

impl FakeDevice {
    pub fn new(buffer: usize, period: usize) -> Self {
        Self {
            buffer,
            period,
            queued: 0,
            open: false,
            info: None,
            written_frames: 0,
            prepared: 0,
            resumed: 0,
            closed: 0,
            fail_open: None,
            fail_write: None,
        }
    }

    /// Plays out up to `frames` queued frames (normally `usize::MAX`).
    pub fn play(&mut self, frames: usize) {
        self.queued = self.queued.saturating_sub(frames);
    }
}

impl PcmDevice for FakeDevice {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn is_open(&self) -> bool {
        self.open
    }

    fn open(&mut self, req: &PcmRequest) -> Result<PcmInfo, PcmError> {
        if let Some(step) = self.fail_open.take() {
            return Err(PcmError::new(step, -libc::EIO));
        }
        let info = PcmInfo {
            rate: req.rate,
            period_frames: self.period,
            buffer_frames: self.buffer,
        };
        self.open = true;
        self.info = Some(info);
        Ok(info)
    }

    fn info(&self) -> Option<PcmInfo> {
        self.info
    }

    fn write(&mut self, _data: &[u8], frames: usize) -> Result<usize, PcmError> {
        if let Some(code) = self.fail_write.take() {
            return Err(PcmError::new(Step::Write, code));
        }
        let n = frames.min(self.buffer - self.queued);
        self.queued += n;
        self.written_frames += n;
        Ok(n)
    }

    fn avail(&mut self) -> Result<usize, PcmError> {
        Ok(self.buffer - self.queued)
    }

    fn delay(&mut self) -> Result<i64, PcmError> {
        Ok(self.queued as i64)
    }

    fn wait(&mut self, _timeout_ms: i32) -> Result<bool, PcmError> {
        Ok(true)
    }

    fn resume(&mut self) -> Result<(), PcmError> {
        self.resumed += 1;
        Ok(())
    }

    fn prepare(&mut self) -> Result<(), PcmError> {
        self.prepared += 1;
        Ok(())
    }

    fn drop_frames(&mut self) -> Result<(), PcmError> {
        self.queued = 0;
        Ok(())
    }

    fn close(&mut self) {
        self.open = false;
        self.info = None;
        self.closed += 1;
    }
}
