//! Джиттер-буфер downlink: порядок, потери, адаптивная глубина и PLC.
//!
//! Кадры хранятся раскодированными: декодер Opus нужен только на входе и для PLC, поэтому
//! буфер не зависит от кодека и тестируется без него. Глубина — 40..100 мс (2..5 кадров по
//! 20 мс): меньше — рвётся на Wi-Fi телефона, больше — слышимая задержка разговора.
//!
//! Выдача идёт только по часам воспроизведения: пакет кладёт кадр в буфер, а [`pop`]
//! вызывается с периодом кадра. Пока после прайминга в буфере меньше целевой глубины,
//! кадры не отдаются — иначе буфер опустеет и следующий пакет снова станет Late.
//!
//! [`JitterBuffer::pop`] возвращает `None`, когда поток иссяк дольше [`GIVEUP_MS`]: звонок
//! после этого считается потерянным.
//!
//! [`pop`]: JitterBuffer::pop

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

/// Буфер кадров произвольного типа: сеть даёт `T`, воспроизведение забирает его же.
pub struct JitterBuffer<T> {
    frame_ms: u32,
    min_frames: u32,
    max_frames: u32,
    target_frames: u32,
    slots: BTreeMap<u64, T>,
    next: u64,
    started: bool,
    priming: bool,
    prime_t0: i64,
    last_output_ms: i64,
    clean: u32,
}

impl<T> JitterBuffer<T> {
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

    /// Глубина в миллисекундах.
    #[must_use]
    pub fn depth_ms(&self) -> u32 {
        u32::try_from(self.slots.len()).unwrap_or(u32::MAX) * self.frame_ms
    }

    /// Новый кадр. `now_ms` — CLOCK_MONOTONIC.
    pub fn push(&mut self, seq: u64, frame: T, now_ms: i64) -> Push {
        if !self.started {
            self.started = true;
            self.next = seq;
            self.prime_t0 = now_ms;
            self.last_output_ms = now_ms;
        }
        if seq < self.next {
            // Кадр опоздал: сеть держит больше, чем текущая цель. Увеличиваем цель,
            // чтобы его место не успели сыграть PLC в следующий раз.
            if self.target_frames < self.max_frames {
                self.target_frames += 1;
            }
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
        self.slots.insert(seq, frame);
        Push::Queued
    }

    /// Один кадр на выход. `lost` вызывается для пропущенного номера (PLC декодера).
    ///
    /// В `lost` передаётся следующий имеющийся кадр (если он есть): по нему декодер может
    /// восстановить пропущенный из in-band FEC, не дожидаясь ретрансмиссии.
    ///
    /// `None` — играть нечего: ещё идёт пре-buffer, буфер мельче цели или поток потерян.
    pub fn pop(&mut self, now_ms: i64, lost: &mut dyn FnMut(Option<&T>) -> T) -> Option<T> {
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

        // После прайминга не отдаём кадр, пока не набрана цель: иначе один пакет с
        // задержкой снова опустошит буфер и выдача пойдёт PLC чаще, чем приходит звук.
        if self.depth() < self.target_frames as usize {
            return None;
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
        let replacement = {
            let next = self.slots.get(&self.next.saturating_add(1));
            lost(next)
        };
        self.next += 1;
        self.last_output_ms = now_ms;
        Some(replacement)
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
    /// Кадров на входе в буфер в тестах: цель 2 кадра, поэтому на каждую выдачу
    /// должен быть запас, иначе гейт глубины законно останавливает воспроизведение.
    const PRIME: u64 = 4;

    fn frame(v: i16) -> Vec<i16> {
        vec![v; 3]
    }

    fn plc() -> Vec<i16> {
        vec![-1, -1, -1]
    }

    fn popped(b: &mut JitterBuffer<Vec<i16>>, now: i64) -> Option<Vec<i16>> {
        b.pop(now, &mut |_| plc())
    }

    /// Заполнить буфер выше целевой глубины.
    fn prime(b: &mut JitterBuffer<Vec<i16>>) {
        for seq in 0..PRIME {
            b.push(seq, frame(seq as i16), 0);
        }
    }

    #[test]
    fn frames_play_in_sequence_order() {
        let mut b = JitterBuffer::new(F);
        prime(&mut b);
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        assert_eq!(popped(&mut b, 20), Some(frame(1)));
        assert_eq!(popped(&mut b, 40), Some(frame(2)));
    }

    #[test]
    fn out_of_order_frames_are_reordered() {
        let mut b = JitterBuffer::new(F);
        b.push(0, frame(0), 0);
        b.push(2, frame(2), 0);
        b.push(1, frame(1), 0);
        b.push(3, frame(3), 0);
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        assert_eq!(popped(&mut b, 20), Some(frame(1)));
        assert_eq!(popped(&mut b, 40), Some(frame(2)));
    }

    #[test]
    fn playback_waits_until_the_target_depth_is_refilled() {
        let mut b = JitterBuffer::new(F);
        prime(&mut b);
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        // Глубина 3, цель 2 — можно играть второй кадр.
        assert_eq!(popped(&mut b, 20), Some(frame(1)));
        // Осталось 2 кадра ровно на цель: ещё один тик разрешён.
        assert_eq!(popped(&mut b, 40), Some(frame(2)));
        // Глубина 1 < 2: воспроизведение ждёт следующий пакет, а не гонит PLC.
        assert_eq!(popped(&mut b, 60), None);
        b.push(4, frame(4), 70);
        assert_eq!(popped(&mut b, 80), Some(frame(3)));
    }

    #[test]
    fn a_missing_packet_is_concealed_and_counted_as_a_late_stream() {
        let mut b = JitterBuffer::new(F);
        b.push(0, frame(0), 0);
        b.push(2, frame(2), 0);
        b.push(3, frame(3), 0);
        let before = b.target_ms();
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        assert_eq!(popped(&mut b, 20), Some(plc()));
        assert!(b.target_ms() > before, "loss must deepen the buffer");
        // Цель выросла до трёх кадров: ждём добор, затем играем уже принятый кадр 2.
        b.push(4, frame(4), 30);
        assert_eq!(popped(&mut b, 40), Some(frame(2)));
    }

    #[test]
    fn a_gap_exposes_the_next_frame_for_fec() {
        let mut b = JitterBuffer::new(F);
        b.push(0, frame(0), 0);
        b.push(2, frame(2), 0);
        b.push(3, frame(3), 0);
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        let mut saw_next = None;
        let restored = b.pop(20, &mut |next| {
            saw_next = next.cloned();
            vec![9, 9, 9]
        });
        assert_eq!(restored, Some(vec![9, 9, 9]));
        assert_eq!(saw_next, Some(frame(2)), "FEC needs the next packet");
    }

    #[test]
    fn a_frame_whose_slot_was_played_is_dropped_and_deepens_the_buffer() {
        let mut b = JitterBuffer::new(F);
        prime(&mut b);
        let before = b.target_ms();
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        assert_eq!(b.push(0, frame(0), 40), Push::Late);
        assert!(b.target_ms() > before, "late frame must deepen the buffer");
        assert_eq!(b.push(1, frame(1), 40), Push::Duplicate);
    }

    #[test]
    fn a_starved_stream_stops_after_the_giveup_window() {
        let mut b = JitterBuffer::new(F);
        prime(&mut b);
        for tick in 0..PRIME {
            let _ = popped(&mut b, tick as i64 * 20);
        }
        // Буфер пуст: дальше кадров нет ни в очереди, ни в сети.
        assert!(popped(&mut b, i64::from(JITTER_GIVEUP_MS) + 1000).is_none());
    }

    #[test]
    fn the_target_depth_stays_within_the_bounds() {
        let mut b = JitterBuffer::new(F);
        for i in 0..200 {
            b.push(i, frame(0), i as i64);
            let _ = popped(&mut b, i as i64);
        }
        assert!(b.target_ms() >= JITTER_MIN_MS && b.target_ms() <= JITTER_MAX_MS);
    }

    /// Детерминированный ГПСЧ: тест не должен зависеть от `rand`.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            self.0 >> 33
        }

        fn below(&mut self, bound: u64) -> u64 {
            self.next() % bound
        }
    }

    /// Симуляция: пакеты приходят с джиттером 0..80 мс, выдача идёт по часам 20 мс.
    /// Считаем опоздавшие и PLC; возвращаем (кадров, late, plc).
    fn simulate(loss_percent: u64) -> (u64, u64, u64) {
        const TOTAL: u64 = 1500;
        const JITTER: u64 = 80;
        let mut random = Lcg(0x5eed);
        let mut b = JitterBuffer::new(F);
        let mut arrivals = Vec::new();
        let mut sent = 0u64;
        let mut lost = 0u64;
        while sent < TOTAL {
            let delay = random.below(JITTER + 1);
            if random.below(100) >= loss_percent {
                arrivals.push((sent * 20 + delay, sent));
            } else {
                lost += 1;
            }
            sent += 1;
        }
        arrivals.sort_by_key(|(at, _)| *at);
        let mut late = 0u64;
        let mut plc = 0u64;
        let mut next_arrival = 0;
        for tick in 0..TOTAL * 2 {
            let now = tick as i64 * 20;
            while next_arrival < arrivals.len() && arrivals[next_arrival].0 as i64 <= now {
                let (at, seq) = arrivals[next_arrival];
                if b.push(seq, seq as i16, at as i64) == Push::Late {
                    late += 1;
                }
                next_arrival += 1;
            }
            let _ = b.pop(now, &mut |_| {
                plc += 1;
                -1
            });
        }
        let _ = lost;
        (TOTAL, late, plc)
    }

    #[test]
    fn zero_loss_jitter_sixty_to_eighty_ms_has_almost_no_late_or_plc() {
        let (frames, late, plc) = simulate(0);
        let bad = late + plc;
        assert!(
            bad * 100 < frames * 2,
            "late={late} plc={plc} out of {frames}: buffer must absorb the jitter"
        );
    }

    #[test]
    fn five_percent_loss_is_concealed_by_plc() {
        let (frames, _late, plc) = simulate(5);
        let lost = frames * 5 / 100;
        assert!(plc > 0, "losses must produce PLC");
        assert!(
            plc <= lost * 2 && plc * 100 >= frames,
            "plc={plc} should track the ~{lost} lost frames"
        );
    }
}
