//! Потоковый вывод голоса собеседника: маленький буфер вместо `Announce`-политики.
//!
//! ANNOUNCE — это элемент (TTS/объявление): prime 200 мс, pad, drain. Звонку нужен
//! непрерывный поток с минимальной задержкой, поэтому здесь отдельный writer: 5 мс период,
//! короткий буфер, тишина вместо недоигранного при голоде и без «объявления» конца.
//! Писатель никогда не ждёт полного кадра: что есть в очереди, то и уходит, остаток
//! добивается тишиной, иначе dmix получает xrun и слышимый щелчок.
//!
//! Задержка складывается из буфера dmix (он же `speakers`), периода и очереди канала;
//! период и буфер запрашиваются маленькими, как позволяет профиль модели.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pulsar_alsa::Format;
use pulsar_model::Playback;
use tracing::{debug, error, info, warn};

use crate::device::{AlsaDevice, PcmDevice, PcmInfo, PcmRequest, Recovery, recover};

const CHANNELS: u32 = 2;
const FRAME_BYTES: usize = 4;

/// Настройки канала звонка.
#[derive(Clone, Copy, Debug)]
pub struct CallTuning {
    /// Период ALSA, мс.
    pub period_ms: u32,
    /// Буфер ALSA, мс: главный вклад в задержку downlink.
    pub buffer_ms: u32,
    /// Сколько ждать готовности устройства, мс.
    pub wait_ms: u32,
    /// Сколько PCM держать в очереди, мс (переполнение отбрасывает самое старое).
    pub queue_ms: u32,
    /// Готовность к записи: столько мс в очереди.
    pub prime_ms: u32,
}

impl CallTuning {
    /// Маленький буфер: 5 мс период, 50 мс буфер. Меньше буфер — чаще xruns на dmix,
    /// больше — растёт задержка разговора; 40–60 мс перекрывает джиттер сети и тик 20 мс.
    pub const CALL: Self = Self {
        period_ms: 5,
        buffer_ms: 50,
        wait_ms: 5,
        queue_ms: 120,
        prime_ms: 20,
    };
}

/// Счётчики канала звонка.
#[derive(Clone, Copy, Debug, Default)]
pub struct CallStats {
    /// Сколько мс звука ждёт в очереди.
    pub queued_ms: u32,
    /// Сколько мс реального звука записано в устройство.
    pub written_ms: u64,
    /// Периодов записано меньше `avail` (реальный звук без добивки тишиной).
    pub partial: u64,
    /// Кадров выброшено контролем дрейфа.
    pub overflow: u64,
    /// Ошибок/переполнений ALSA, после которых канал восстанавливался.
    pub xrun: u64,
}

impl Default for CallTuning {
    fn default() -> Self {
        Self::CALL
    }
}

fn frames(rate: u32, ms: u32) -> usize {
    (rate as usize * ms as usize) / 1000
}

/// Состояние живого канала.
struct Shared {
    /// Моно 48 кГц, ещё не записанные в устройство.
    queue: Mutex<VecDeque<i16>>,
    /// Будит писатель, когда появились сэмплы; без данных поток спит, а не опрашивает.
    doorbell: Condvar,
    /// Поколение сброса: писатель выбрасывает всё, что было до него.
    flush_epoch: AtomicU32,
    stop: AtomicBool,
    rate: u32,
    /// Предел очереди в сэмплах (из [`CallTuning::queue_ms`]).
    capacity: usize,
    /// Сколько сэмплов реального звука записано (диагностика).
    written_frames: AtomicU32,
    /// Неполные периоды: реальный звук без добивки тишиной.
    partial: AtomicU64,
    /// Выброшенные кадры контроля дрейфа.
    overflow: AtomicU64,
    /// Восстановления после ошибок ALSA.
    xrun: AtomicU64,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, VecDeque<i16>> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Писатель канала звонка; держит поток и общий буфер.
pub struct CallOut {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl CallOut {
    /// Запустить писатель на PCM модели. `None` — не удалось создать поток.
    pub fn spawn(playback: &'static Playback, tuning: CallTuning) -> Option<Self> {
        let rate = 48_000;
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            doorbell: Condvar::new(),
            flush_epoch: AtomicU32::new(0),
            stop: AtomicBool::new(false),
            rate,
            capacity: frames(rate, tuning.queue_ms),
            written_frames: AtomicU32::new(0),
            partial: AtomicU64::new(0),
            overflow: AtomicU64::new(0),
            xrun: AtomicU64::new(0),
        });
        let worker = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("call-out".into())
            .spawn(move || writer(worker, AlsaDevice::from_playback(playback), tuning))
            .inspect_err(|e| error!("call: output thread: {e}"))
            .ok()?;
        Some(Self {
            shared,
            thread: Some(thread),
        })
    }

    /// Дописать моно 48 кГц.
    ///
    /// Дрейф выравнивается одним кадром: при переполнении выбрасывается ровно столько же
    /// сэмплов, сколько пришло, а не весь хвост. Иначе разовое опоздание рвёт очередь и
    /// слышимо режет речь.
    pub fn write(&self, pcm: &[i16]) {
        let mut queue = self.shared.lock();
        if queue.len() + pcm.len() > self.shared.capacity {
            let drop = pcm.len().min(queue.len());
            if drop > 0 {
                queue.drain(..drop);
                self.shared.overflow.fetch_add(1, Ordering::Relaxed);
            }
        }
        queue.extend(pcm.iter().copied());
        if queue.len() > self.shared.capacity {
            let excess = queue.len() - self.shared.capacity;
            queue.drain(..excess);
            self.shared.overflow.fetch_add(1, Ordering::Relaxed);
        }
        drop(queue);
        self.shared.doorbell.notify_one();
    }

    /// Сколько мс звука ждёт в очереди.
    #[must_use]
    pub fn queued_ms(&self) -> u32 {
        u32::try_from(self.shared.lock().len() * 1000 / self.shared.rate as usize)
            .unwrap_or(u32::MAX)
    }

    /// Накопленные счётчики канала.
    #[must_use]
    pub fn stats(&self) -> CallStats {
        CallStats {
            queued_ms: self.queued_ms(),
            written_ms: self.played_ms(),
            partial: self.shared.partial.load(Ordering::Relaxed),
            overflow: self.shared.overflow.load(Ordering::Relaxed),
            xrun: self.shared.xrun.load(Ordering::Relaxed),
        }
    }

    /// Сбросить непригранное (звонок завершён или переподключение).
    pub fn flush(&self) {
        self.shared.flush_epoch.fetch_add(1, Ordering::Relaxed);
        self.shared.lock().clear();
        self.shared.doorbell.notify_all();
    }

    /// Сколько мс звука уже ушло в устройство.
    #[must_use]
    pub fn played_ms(&self) -> u64 {
        u64::from(self.shared.written_frames.load(Ordering::Relaxed)) * 1000
            / u64::from(self.shared.rate)
    }
}

impl Drop for CallOut {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "поток владеет Arc; ссылку в 'static поток не передать"
)]
fn writer(shared: Arc<Shared>, mut device: AlsaDevice, tuning: CallTuning) {
    let req = PcmRequest {
        format: Format::S16Le,
        channels: CHANNELS,
        rate: shared.rate,
        rate_exact: true,
        period_frames: Some(frames(shared.rate, tuning.period_ms) as u64),
        buffer_frames: Some(frames(shared.rate, tuning.buffer_ms) as u64),
    };
    let mut info: Option<PcmInfo> = None;
    let mut silence = Vec::new();
    let mut primed = false;
    let mut epoch = shared.flush_epoch.load(Ordering::Relaxed);
    let prime_frames = frames(shared.rate, tuning.prime_ms);
    let mut last_report = Instant::now();

    while !shared.stop.load(Ordering::Relaxed) {
        let current = shared.flush_epoch.load(Ordering::Relaxed);
        if current != epoch {
            epoch = current;
            primed = false;
            if info.is_some() {
                let _ = device.drop_frames();
            }
        }
        // Устройство открывается только на время звонка: вне звонка dmix-клиент не нужен.
        if !primed {
            let queue = shared.lock();
            if queue.len() < prime_frames {
                // Вне звонка устройство закрыто, поток спит до первых сэмплов.
                let (queue, _) = shared
                    .doorbell
                    .wait_timeout(queue, Duration::from_millis(250))
                    .unwrap_or_else(PoisonError::into_inner);
                drop(queue);
                if info.is_some() {
                    device.close();
                    info = None;
                    debug!("call: output closed (idle)");
                }
                continue;
            }
            drop(queue);
            primed = true;
        }
        if info.is_none() {
            match device.open(&req) {
                Ok(open) => {
                    info!(
                        "call: output open (period {} buffer {} frames)",
                        open.period_frames, open.buffer_frames
                    );
                    silence.resize(open.period_frames * FRAME_BYTES, 0);
                    info = Some(open);
                }
                Err(e) => {
                    warn!("call: output open failed: {e}");
                    std::thread::sleep(Duration::from_millis(500));
                    primed = false;
                    continue;
                }
            }
        }
        let Some(open) = info else { continue };

        let avail = match device.avail() {
            Ok(a) => a.min(open.period_frames),
            Err(error) => {
                if !handle(&mut device, error) {
                    info = None;
                }
                continue;
            }
        };
        if avail == 0 {
            let _ = device.wait(i32::try_from(tuning.wait_ms).unwrap_or(5));
            continue;
        }

        // Собираем реальный звук. Неполный период не добиваем тишиной: иначе посреди
        // речи появляется провал, а следующий кадр начинается с середины периода.
        let mut buffer = vec![0u8; avail * FRAME_BYTES];
        let taken = {
            let mut queue = shared.lock();
            let n = avail.min(queue.len());
            for i in 0..n {
                if let Some(sample) = queue.pop_front() {
                    let bytes = sample.to_le_bytes();
                    buffer[i * FRAME_BYTES] = bytes[0];
                    buffer[i * FRAME_BYTES + 1] = bytes[1];
                    buffer[i * FRAME_BYTES + 2] = bytes[0];
                    buffer[i * FRAME_BYTES + 3] = bytes[1];
                }
            }
            n
        };
        let to_write = if taken == 0 {
            // Совсем пусто: тишина держит устройство открытым, задержка не растёт.
            buffer.copy_from_slice(&silence[..avail * FRAME_BYTES]);
            avail
        } else {
            if taken < avail {
                shared.partial.fetch_add(1, Ordering::Relaxed);
            }
            taken
        };
        match device.write(&buffer, to_write) {
            Ok(written) => {
                if taken > 0 {
                    let _ = shared.written_frames.fetch_add(
                        u32::try_from(written).unwrap_or(u32::MAX),
                        Ordering::Relaxed,
                    );
                }
            }
            Err(error) => {
                shared.xrun.fetch_add(1, Ordering::Relaxed);
                if !handle(&mut device, error) {
                    info = None;
                }
            }
        }
        if last_report.elapsed() >= Duration::from_secs(10) {
            last_report = Instant::now();
            debug!(
                "call: output {} ms played, queue {} samples",
                shared.written_frames.load(Ordering::Relaxed) * 1000 / shared.rate,
                shared.lock().len()
            );
        }
    }
    device.close();
}

fn handle(device: &mut AlsaDevice, error: crate::PcmError) -> bool {
    let mut xruns = 0;
    match recover(device, error, &mut xruns) {
        Recovery::Retry => true,
        Recovery::Underrun | Recovery::Suspend => {
            warn!("call: output recovered ({xruns} xrun)");
            true
        }
        Recovery::Fatal(e) => {
            warn!("call: output failed: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PcmDevice, PcmError, PcmInfo};

    /// Устройство с ограниченным буфером, как dmix: пишет, пока есть место.
    struct TestDevice {
        buffer: usize,
        queued: usize,
        received: Vec<u8>,
        period: usize,
    }

    impl PcmDevice for TestDevice {
        fn name(&self) -> &'static str {
            "test"
        }
        fn is_open(&self) -> bool {
            true
        }
        fn open(&mut self, _req: &PcmRequest) -> Result<PcmInfo, PcmError> {
            Ok(PcmInfo {
                rate: 48_000,
                period_frames: self.period,
                buffer_frames: self.buffer,
            })
        }
        fn info(&self) -> Option<PcmInfo> {
            None
        }
        fn write(&mut self, data: &[u8], frames: usize) -> Result<usize, PcmError> {
            let n = frames.min(self.buffer - self.queued);
            self.received.extend_from_slice(&data[..n * FRAME_BYTES]);
            self.queued += n;
            Ok(n)
        }
        fn avail(&mut self) -> Result<usize, PcmError> {
            Ok(self.buffer - self.queued)
        }
        fn delay(&mut self) -> Result<i64, PcmError> {
            Ok(self.queued as i64)
        }
        fn wait(&mut self, _ms: i32) -> Result<bool, PcmError> {
            Ok(true)
        }
        fn resume(&mut self) -> Result<(), PcmError> {
            Ok(())
        }
        fn prepare(&mut self) -> Result<(), PcmError> {
            Ok(())
        }
        fn drop_frames(&mut self) -> Result<(), PcmError> {
            self.queued = 0;
            Ok(())
        }
        fn close(&mut self) {}
    }

    #[test]
    fn a_mono_sample_is_written_to_both_channels() {
        let mut dev = TestDevice {
            buffer: 480,
            queued: 0,
            received: Vec::new(),
            period: 240,
        };
        let avail = dev.avail().unwrap();
        let mut buffer = vec![0u8; avail * FRAME_BYTES];
        let sample: i16 = 0x1234;
        let bytes = sample.to_le_bytes();
        buffer[0] = bytes[0];
        buffer[1] = bytes[1];
        buffer[2] = bytes[0];
        buffer[3] = bytes[1];
        dev.write(&buffer, avail).unwrap();
        assert_eq!(&dev.received[..4], &buffer[..4]);
    }

    #[test]
    fn the_queue_is_bounded_and_drops_the_oldest() {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            doorbell: Condvar::new(),
            flush_epoch: AtomicU32::new(0),
            stop: AtomicBool::new(false),
            rate: 48_000,
            capacity: frames(48_000, CallTuning::CALL.queue_ms),
            written_frames: AtomicU32::new(0),
            partial: AtomicU64::new(0),
            overflow: AtomicU64::new(0),
            xrun: AtomicU64::new(0),
        });
        let out = CallOut {
            shared: Arc::clone(&shared),
            thread: None,
        };
        let capacity = frames(48_000, CallTuning::CALL.queue_ms);
        out.write(&vec![7i16; capacity + 100]);
        assert_eq!(out.shared.lock().len(), capacity);
        assert_eq!(out.shared.lock().front().copied(), Some(7));
    }

    #[test]
    fn drift_drops_one_frame_instead_of_the_whole_queue() {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            doorbell: Condvar::new(),
            flush_epoch: AtomicU32::new(0),
            stop: AtomicBool::new(false),
            rate: 48_000,
            capacity: 3 * 960,
            written_frames: AtomicU32::new(0),
            partial: AtomicU64::new(0),
            overflow: AtomicU64::new(0),
            xrun: AtomicU64::new(0),
        });
        let out = CallOut {
            shared: Arc::clone(&shared),
            thread: None,
        };
        for value in [1i16, 2, 3] {
            out.write(&vec![value; 960]);
        }
        out.write(&vec![4i16; 960]);
        let queue = out.shared.lock();
        assert_eq!(queue.len(), 3 * 960);
        // Ушёл ровно один старый кадр, остальные два сохранили порядок.
        assert_eq!(
            queue.iter().copied().take(960).collect::<Vec<_>>(),
            vec![2; 960]
        );
        assert_eq!(out.shared.overflow.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn flush_clears_the_queue_and_bumps_the_epoch() {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            doorbell: Condvar::new(),
            flush_epoch: AtomicU32::new(0),
            stop: AtomicBool::new(false),
            rate: 48_000,
            capacity: frames(48_000, CallTuning::CALL.queue_ms),
            written_frames: AtomicU32::new(0),
            partial: AtomicU64::new(0),
            overflow: AtomicU64::new(0),
            xrun: AtomicU64::new(0),
        });
        let out = CallOut {
            shared: Arc::clone(&shared),
            thread: None,
        };
        out.write(&[1, 2, 3]);
        out.flush();
        assert!(out.shared.lock().is_empty());
        assert_eq!(out.shared.flush_epoch.load(Ordering::Relaxed), 1);
    }
}
