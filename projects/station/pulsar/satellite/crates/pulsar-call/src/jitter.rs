//! Джиттер-буфер downlink: порядок, потери, адаптивная глубина и PLC.
//!
//! Кадры хранятся раскодированными: декодер Opus нужен только на входе и для PLC, поэтому
//! буфер не зависит от кодека и тестируется без него. Глубина — 40..100 мс (2..5 кадров по
//! 20 мс): меньше — рвётся на Wi-Fi телефона, больше — слышимая задержка разговора.
//! [`JitterBuffer::pop`] возвращает `None`, когда поток иссяк дольше [`GIVEUP_MS`]: звонок
//! после этого считается потерянным.

use std::collections::BTreeMap;

use crate::{JITTER_GIVEUP_MS, JITTER_MAX_MS, JITTER_MIN_MS};

/// Кадр слишком опоздал (его место уже отдано дальше) — отбрасывается.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Push {
    /// Кадр в буфере.
    Queued,
    /// Номер уже сыгран или заменён PLC.
    Late,
    /// Такой номер уже есть (дубликат ретрансмиссии).
    Duplicate,
}

/// Сколько тишины до первого кадра после старта считается нормой, мс.
const PRIME_TIMEOUT_MS: i64 = 250;
/// Чистых кадров подряд до уменьшения глубины (5 с при 20 мс).
const SHRINK_EVERY: u32 = 250;

pub struct JitterBuffer {
    frame_ms: u32,
    min_frames: u32,
    max_frames: u32,
    target_frames: u32,
    slots: BTreeMap<u64, Vec<i16>>,
    next: u64,
    started: bool,
    priming: bool,
    prime_t0: i64,
    last_output_ms: i64,
    clean: u32,
}

impl JitterBuffer {
    #[must_use]
    pub fn new(frame_ms: u32) -> Self {
        let min_frames = JITTER_MIN_MS.div_ceil(frame_ms).max(1);
        let max_frames = JITTER_MAX_MS.div_ceil(frame_ms).max(min_frames);
        Self {
            frame_ms,
            min_frames,
            max_frames,
            target_frames: min_frames,
            slots: BTreeMap::new(),
            next: 0,
            started: false,
            priming: true,
            prime_t0: 0,
            last_output_ms: 0,
            clean: 0,
        }
    }

    /// Текущая цель буфера, мс.
    #[must_use]
    pub fn target_ms(&self) -> u32 {
        self.target_frames * self.frame_ms
    }

    /// Кадров готово к выдаче.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.slots.len()
    }

    /// Новый кадр. `now_ms` — CLOCK_MONOTONIC.
    pub fn push(&mut self, seq: u64, pcm: Vec<i16>, now_ms: i64) -> Push {
        if !self.started {
            self.started = true;
            self.next = seq;
            self.prime_t0 = now_ms;
            self.last_output_ms = now_ms;
        }
        if seq < self.next {
            return Push::Late;
        }
        if self.slots.contains_key(&seq) {
            return Push::Duplicate;
        }
        // Прыжок больше чем на буфер: поток перезапустился, старые номера уже не сыграть.
        let ahead = seq.saturating_sub(self.next);
        if ahead > u64::from(self.max_frames) * 4 {
            self.reset(seq);
        }
        self.slots.insert(seq, pcm);
        Push::Queued
    }

    /// Один кадр на выход. `lost` вызывается для пропущенного номера (PLC декодера).
    ///
    /// `None` — играть нечего: ещё идёт пре-buffer, буфер пуст или поток потерян.
    pub fn pop(&mut self, now_ms: i64, lost: &mut dyn FnMut() -> Vec<i16>) -> Option<Vec<i16>> {
        if !self.started {
            return None;
        }
        if self.priming {
            let queued = self.depth() as u32;
            if queued >= self.target_frames || now_ms - self.prime_t0 >= PRIME_TIMEOUT_MS {
                self.priming = false;
                self.last_output_ms = now_ms;
            } else {
                return None;
            }
        }

        if let Some(frame) = self.slots.remove(&self.next) {
            self.next += 1;
            self.last_output_ms = now_ms;
            self.clean = self.clean.saturating_add(1);
            if self.clean >= SHRINK_EVERY && self.target_frames > self.min_frames {
                self.target_frames -= 1;
                self.clean = 0;
            }
            return Some(frame);
        }

        let starved = now_ms - self.last_output_ms > i64::from(JITTER_GIVEUP_MS);
        if starved {
            return None;
        }
        // Пропуск внутри потока: отдаём PLC и идём к следующему имеющемуся номеру.
        // Глубина растёт: сеть уже показала потерю порядка.
        self.clean = 0;
        if self.target_frames < self.max_frames {
            self.target_frames += 1;
        }
        if self.slots.is_empty() {
            // Полная тишина в сети: PLC вместо тишины, пока не истечёт GIVEUP.
            self.next += 1;
            self.last_output_ms = now_ms;
            return Some(lost());
        }
        // Пропущен именно `next`, а дальше кадры есть: один PLC и сдвиг на кадр.
        self.next += 1;
        self.last_output_ms = now_ms;
        Some(lost())
    }

    fn reset(&mut self, seq: u64) {
        self.slots.clear();
        self.next = seq;
        self.priming = true;
        self.prime_t0 = self.last_output_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const F: u32 = 20;

    fn frame(v: i16) -> Vec<i16> {
        vec![v; 3]
    }

    fn plc() -> Vec<i16> {
        vec![-1, -1, -1]
    }

    fn popped(b: &mut JitterBuffer, now: i64) -> Option<Vec<i16>> {
        b.pop(now, &mut plc)
    }

    /// Заполнить пре-buffer до готовности.
    fn prime(b: &mut JitterBuffer) {
        for seq in 0..2 {
            b.push(seq, frame(seq as i16), 0);
        }
    }

    #[test]
    fn frames_play_in_sequence_order() {
        let mut b = JitterBuffer::new(F);
        prime(&mut b);
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        assert_eq!(popped(&mut b, 20), Some(frame(1)));
    }

    #[test]
    fn out_of_order_frames_are_reordered() {
        let mut b = JitterBuffer::new(F);
        b.push(0, frame(0), 0);
        b.push(2, frame(2), 0);
        b.push(1, frame(1), 0);
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        assert_eq!(popped(&mut b, 20), Some(frame(1)));
        assert_eq!(popped(&mut b, 40), Some(frame(2)));
    }

    #[test]
    fn a_missing_packet_is_concealed_and_counted_as_a_late_stream() {
        let mut b = JitterBuffer::new(F);
        b.push(0, frame(0), 0);
        b.push(2, frame(2), 0);
        let before = b.target_ms();
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        assert_eq!(popped(&mut b, 20), Some(plc()));
        assert_eq!(popped(&mut b, 40), Some(frame(2)));
        assert!(b.target_ms() > before, "loss must deepen the buffer");
    }

    #[test]
    fn a_frame_whose_slot_was_played_is_dropped() {
        let mut b = JitterBuffer::new(F);
        prime(&mut b);
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        assert_eq!(b.push(0, frame(0), 40), Push::Late);
        assert_eq!(b.push(1, frame(1), 40), Push::Duplicate);
    }

    #[test]
    fn a_starved_stream_stops_after_the_giveup_window() {
        let mut b = JitterBuffer::new(F);
        prime(&mut b);
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        assert_eq!(popped(&mut b, 20), Some(frame(1)));
        // Дальше только PLC, пока не истечёт окно.
        assert_eq!(popped(&mut b, 40), Some(plc()));
        assert!(popped(&mut b, i64::from(JITTER_GIVEUP_MS) + 41).is_none());
    }

    #[test]
    fn the_target_depth_stays_within_the_bounds() {
        let mut b = JitterBuffer::new(F);
        b.push(0, frame(0), 0);
        for i in 0..200 {
            b.push(i, frame(0), i as i64);
            let _ = popped(&mut b, i as i64);
        }
        assert!(b.target_ms() >= JITTER_MIN_MS && b.target_ms() <= JITTER_MAX_MS);
    }
}
