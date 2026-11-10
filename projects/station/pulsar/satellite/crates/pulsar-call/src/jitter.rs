//! Джиттер-буфер downlink: порядок, потери, адаптивная глубина и PLC.
//!
//! Кадры хранятся сырыми (для звонка — Opus-пакеты): декодер stateful, поэтому декодирование,
//! in-band FEC и PLC обязаны идти в порядке воспроизведения. Это делает [`Playout`] в момент
//! [`JitterBuffer::pop`], а не при приходе пакета.
//!
//! Глубина — 40..100 мс (2..5 кадров по 20 мс): меньше — рвётся на Wi-Fi телефона, больше —
//! слышимая задержка разговора. Гейт по цели действует только при (ре)прайминге — на старте и
//! после настоящего голодания. В установившемся режиме отдаётся всё, что уже пришло: иначе
//! каждые 20 мс очередь на выходе пустеет, и ALSA недокармливается.
//!
//! [`JitterBuffer::pop`] возвращает `None`, когда поток иссяк дольше
//! [`JITTER_GIVEUP_MS`](crate::JITTER_GIVEUP_MS): звонок после этого считается потерянным.
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

/// Превращение хранимого кадра в выходной в порядке воспроизведения.
///
/// Декодер Opus хранит состояние между кадрами: обычный кадр, FEC и PLC должны вызываться
/// строго по очереди, поэтому буфер сам ничего не декодирует и не переупорядочивает вызовы.
pub trait Playout<T> {
    /// Что отдаётся наружу: для звонка — PCM.
    type Output;

    /// Обычный кадр из буфера.
    fn frame(&mut self, frame: &T) -> Self::Output;

    /// Пропуск на месте ожидаемого номера.
    ///
    /// `next` — следующий уже пришедший кадр, если он есть: по нему декодер может
    /// восстановить пропущенный из in-band FEC, не дожидаясь ретрансмиссии.
    fn lost(&mut self, next: Option<&T>) -> Self::Output;
}

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

    /// Один кадр на выход в порядке воспроизведения.
    ///
    /// `None` — кадров нет: ещё идёт пре-buffer, буфер совсем опустел или поток потерян
    /// (тишина дольше [`JITTER_GIVEUP_MS`]). PLC в пустоту не гонится: после голодания буфер
    /// снова праймится, и гейт по цели ждёт сеть.
    pub fn pop<O>(&mut self, now_ms: i64, playout: &mut dyn Playout<T, Output = O>) -> Option<O> {
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
            return Some(playout.frame(&frame));
        }

        // Совсем пусто: не тянем PLC бесконечно, а ждём сеть и праймимся заново.
        if self.slots.is_empty() {
            if now_ms - self.last_output_ms > i64::from(JITTER_GIVEUP_MS) {
                return None;
            }
            self.priming = true;
            self.prime_t0 = now_ms;
            return None;
        }

        // Пропуск внутри потока: отдаём FEC/PLC и идём к следующему имеющемуся номеру.
        // Глубина растёт: сеть уже показала потерю порядка.
        self.clean = 0;
        if self.target_frames < self.max_frames {
            self.target_frames += 1;
        }
        let replacement = {
            let next = self.slots.get(&self.next.saturating_add(1));
            playout.lost(next)
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

    /// Обычный кадр отдаётся как есть, пропуск считает и заменяет PLC.
    struct Frames<F: FnMut(Option<&Vec<i16>>) -> Vec<i16>>(F);

    impl<F: FnMut(Option<&Vec<i16>>) -> Vec<i16>> Playout<Vec<i16>> for Frames<F> {
        type Output = Vec<i16>;

        fn frame(&mut self, frame: &Vec<i16>) -> Vec<i16> {
            frame.to_vec()
        }

        fn lost(&mut self, next: Option<&Vec<i16>>) -> Vec<i16> {
            (self.0)(next)
        }
    }

    fn popped(b: &mut JitterBuffer<Vec<i16>>, now: i64) -> Option<Vec<i16>> {
        b.pop(now, &mut Frames(|_| plc()))
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
    fn steady_state_plays_out_what_has_arrived() {
        let mut b = JitterBuffer::new(F);
        prime(&mut b);
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        assert_eq!(popped(&mut b, 20), Some(frame(1)));
        assert_eq!(popped(&mut b, 40), Some(frame(2)));
        // Установившийся режим: кадр есть — он играет, даже если глубина ниже цели.
        // Иначе каждые 20 мс возникал бы провал выдачи и недокорм ALSA.
        assert_eq!(popped(&mut b, 60), Some(frame(3)));
        // Совсем пусто: выдавать нечего, но и PLC в пустоту не идёт.
        assert_eq!(popped(&mut b, 80), None);
        b.push(4, frame(4), 90);
        // После голодания — репрайминг: ждём цель, а не играем одиночный кадр.
        assert_eq!(popped(&mut b, 100), None);
        b.push(5, frame(5), 110);
        assert_eq!(popped(&mut b, 120), Some(frame(4)));
    }

    #[test]
    fn an_empty_buffer_does_not_run_plc_into_the_void() {
        let mut b = JitterBuffer::new(F);
        b.push(0, frame(0), 0);
        b.push(1, frame(1), 0);
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        assert_eq!(popped(&mut b, 20), Some(frame(1)));
        let mut losses = 0;
        let mut playout = Frames(|next| {
            losses += 1;
            assert!(next.is_none());
            plc()
        });
        assert_eq!(b.pop(40, &mut playout), None);
        assert_eq!(b.pop(60, &mut playout), None);
        assert_eq!(losses, 0, "empty buffer must not conceal");
    }

    #[test]
    fn a_missing_packet_is_restored_from_the_next_one_and_deepens_the_buffer() {
        let mut b = JitterBuffer::new(F);
        b.push(0, frame(0), 0);
        b.push(2, frame(2), 0);
        b.push(3, frame(3), 0);
        let before = b.target_ms();
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        let mut lost = 0;
        let mut playout = Frames(|next| {
            lost += 1;
            assert_eq!(next, Some(&frame(2)), "FEC needs the next packet");
            vec![9, 9, 9]
        });
        assert_eq!(b.pop(20, &mut playout), Some(vec![9, 9, 9]));
        assert_eq!(lost, 1);
        assert!(b.target_ms() > before, "loss must deepen the buffer");
        // Установившийся режим не ждёт добора глубины: уже принятый кадр играет сразу.
        assert_eq!(popped(&mut b, 40), Some(frame(2)));
        assert_eq!(popped(&mut b, 60), Some(frame(3)));
    }

    #[test]
    fn a_single_loss_is_concealed_exactly_once() {
        let mut b = JitterBuffer::new(F);
        for seq in [0u64, 2, 3, 4] {
            b.push(seq, frame(seq as i16), 0);
        }
        assert_eq!(popped(&mut b, 0), Some(frame(0)));
        let mut losses = 0;
        let mut playout = Frames(|next| {
            losses += 1;
            assert_eq!(next, Some(&frame(2)));
            plc()
        });
        assert_eq!(b.pop(20, &mut playout), Some(plc()));
        assert_eq!(losses, 1, "one loss — one PLC/FEC");
        assert_eq!(popped(&mut b, 40), Some(frame(2)));
        assert_eq!(popped(&mut b, 60), Some(frame(3)));
        assert_eq!(popped(&mut b, 80), Some(frame(4)));
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

    /// Итоги симуляции выдачи.
    struct Sim {
        /// Кадров отдано.
        outputs: u64,
        /// Кадров пришло с опозданием.
        late: u64,
        /// Пропусков, закрытых FEC/PLC.
        concealed: u64,
        /// Тиков после прогрева, на которых играть было нечего.
        gaps: u64,
    }

    /// Симуляция: пакеты приходят с джиттером 0..80 мс, выдача идёт по часам 20 мс.
    fn simulate(loss_percent: u64) -> Sim {
        const TOTAL: u64 = 1500;
        const JITTER: u64 = 80;
        /// Прогрев: прайминг и адаптация цели не считаются голоданием.
        const WARMUP_TICKS: u64 = 100;
        let mut random = Lcg(0x5eed);
        let mut b = JitterBuffer::new(F);
        let mut arrivals = Vec::new();
        let mut sent = 0u64;
        while sent < TOTAL {
            let delay = random.below(JITTER + 1);
            if random.below(100) >= loss_percent {
                arrivals.push((sent * 20 + delay, sent));
            }
            sent += 1;
        }
        arrivals.sort_by_key(|(at, _)| *at);
        let mut sim = Sim {
            outputs: 0,
            late: 0,
            concealed: 0,
            gaps: 0,
        };
        let mut next_arrival = 0;
        // Тики покрывают только время прихода пакетов: хвост после конца потока — это
        // штатное голодание, а не провал выдачи.
        for tick in 0..TOTAL {
            let now = tick as i64 * 20;
            while next_arrival < arrivals.len() && arrivals[next_arrival].0 as i64 <= now {
                let (at, seq) = arrivals[next_arrival];
                if b.push(seq, frame(seq as i16), at as i64) == Push::Late {
                    sim.late += 1;
                }
                next_arrival += 1;
            }
            let mut concealed = 0;
            let played = b.pop(
                now,
                &mut Frames(|_| {
                    concealed += 1;
                    vec![-1]
                }),
            );
            sim.concealed += concealed;
            if played.is_some() {
                sim.outputs += 1;
            } else if tick >= WARMUP_TICKS {
                sim.gaps += 1;
            }
        }
        sim
    }

    #[test]
    fn an_established_stream_never_starves_under_jitter() {
        let sim = simulate(0);
        assert!(sim.outputs > 0);
        assert_eq!(
            sim.gaps, 0,
            "late={} concealed={} outputs={}: steady state must feed the device",
            sim.late, sim.concealed, sim.outputs
        );
    }

    #[test]
    fn zero_loss_jitter_sixty_to_eighty_ms_has_almost_no_late_or_plc() {
        let sim = simulate(0);
        let bad = sim.late + sim.concealed;
        assert!(
            bad * 100 < sim.outputs * 2,
            "late={} concealed={} out of {}: buffer must absorb the jitter",
            sim.late,
            sim.concealed,
            sim.outputs
        );
    }

    #[test]
    fn five_percent_loss_is_concealed_by_plc() {
        let sim = simulate(5);
        let lost = sim.outputs * 5 / 100;
        assert!(sim.concealed > 0, "losses must produce PLC");
        assert!(
            sim.concealed <= lost * 2 && sim.concealed * 100 >= sim.outputs,
            "concealed={} should track the ~{lost} lost frames",
            sim.concealed
        );
    }
}
