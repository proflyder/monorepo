//! Порты внешнего мира: захват, движок, детектор, запись и время.

use crate::ricardo::DetectorError;
use crate::wake_events::logic;

use super::capture::CaptureError;

/// Источник блоков захвата (interleaved, раскладка модели): ALSA в продакшене, скрипт в тестах.
pub(super) trait CaptureSource: Send {
    /// Читает не больше ёмкости `out` кадров; 0 — данных пока нет (EAGAIN), нужно `wait`.
    fn read(&mut self, out: &mut [i16]) -> Result<usize, CaptureError>;
    /// Ждёт данных не дольше `ms`.
    fn wait(&mut self, ms: i32) -> Result<(), CaptureError>;
    /// После ошибки чтения: сбрасывает очередь устройства; false — устройство нужно открыть заново.
    fn recover(&mut self) -> bool;
}

/// Детектор «Рикардо»: модель microWakeWord поверх очищенного чанка.
pub(super) trait Detector: Send {
    fn feed(&mut self, pcm: &[i16]) -> Result<Detection, DetectorError>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Detection {
    pub(super) hit: bool,
    /// Взвешенное среднее вероятности окна; отрицательное, пока окно детектора заполняется.
    pub(super) score: f32,
}

/// Источник времени потока обработки: системный в продакшене, фиксированный в тестах.
pub(super) trait Clock: Send {
    /// Монотонные миллисекунды (дебаунс и окна записи).
    fn mono_ms(&self) -> i64;
    /// UTC-миллисекунды (имена файлов контракта v1).
    fn utc_ms(&self) -> i64;
}

pub(super) struct SystemClock;

impl Clock for SystemClock {
    fn mono_ms(&self) -> i64 {
        pulsar_core::now_ms()
    }

    fn utc_ms(&self) -> i64 {
        logic::utc_ms()
    }
}

/// Запись wake-событий: готовые окна и счётчики. `NullStore` в тестах — выключенная запись.
pub(super) trait WakeStore: Send {
    fn listening(&mut self, chunk_ms: u64, dry_run: bool);
    fn missed(&mut self, utc_ms: i64);
    /// Один чанк; Some(id) — id записанного hit (сайдкар получит `outcome`).
    fn push(
        &mut self,
        chunk: &[i16],
        mean: Option<f32>,
        hit: Option<f32>,
        dry_run: bool,
        now_ms: i64,
        utc_ms: i64,
    ) -> Option<String>;
    fn reset(&mut self);
}
