//! Кодек звонка и апсемплинг uplink.
//!
//! [`Codec`] отвязан от libopus, чтобы тесты гоняли сервис без C-линковки. Боевой
//! [`OpusCodec`] — тонкая обёртка [`crate::ffi`]. VQE отдаёт 16 кГц; Opus в WebRTC всегда
//! считает время на 48 кГц, поэтому uplink апсемплируется [`Upsampler`]'ом, а downlink уже
//! 48 кГц и выводится без ресемплинга.

use crate::ffi;
use crate::{FRAME_SAMPLES, UPLINK_RATE};

/// Один Opus-кадр: вход — ровно [`FRAME_SAMPLES`] моно 48 кГц, выход — моно 48 кГц.
pub trait Codec: Send {
    /// Закодировать кадр; `None` — ошибка кодека или неверная длина.
    fn encode(&mut self, pcm: &[i16]) -> Option<Vec<u8>>;
    /// Раскодировать пакет; `None` — битый пакет.
    fn decode(&mut self, packet: &[u8]) -> Option<Vec<i16>>;
    /// Восстановить пропущенный кадр из in-band FEC следующего пакета.
    /// `None` — FEC в пакете нет или декодер его не поддерживает.
    fn decode_fec(&mut self, packet: &[u8]) -> Option<Vec<i16>>;
    /// PLC: кадр на место потерянного.
    fn conceal(&mut self) -> Vec<i16>;
    /// Длительность пакета в сэмплах 48 кГц; `None` — пакет не разбирается.
    ///
    /// Нужна RTP-таймлайну до декодирования: кадр в буфере лежит сырым, а номер
    /// воспроизведения считается по времени пакета.
    fn packet_samples(&self, packet: &[u8]) -> Option<usize>;
}

/// Opus 48 кГц моно/стерео поверх libopus прошивки.
pub struct OpusCodec {
    encoder: ffi::Encoder,
    decoder: ffi::Decoder,
}

impl OpusCodec {
    /// `None` — libopus недоступен; звонок тогда не начинается.
    #[must_use]
    pub fn new() -> Option<Self> {
        Some(Self {
            encoder: ffi::Encoder::new_mono()?,
            decoder: ffi::Decoder::new_stereo()?,
        })
    }
}

impl Codec for OpusCodec {
    fn encode(&mut self, pcm: &[i16]) -> Option<Vec<u8>> {
        self.encoder.encode(pcm)
    }

    fn decode(&mut self, packet: &[u8]) -> Option<Vec<i16>> {
        self.decoder.decode(packet)
    }

    fn decode_fec(&mut self, packet: &[u8]) -> Option<Vec<i16>> {
        self.decoder.decode_fec(packet)
    }

    fn conceal(&mut self) -> Vec<i16> {
        self.decoder.conceal()
    }

    fn packet_samples(&self, packet: &[u8]) -> Option<usize> {
        ffi::packet_samples(packet)
    }
}

/// Один полюс ФНЧ апсемплера: a ≈ 2π·fc/fs при fc ≈ 7 кГц и fs = 48 кГц.
const UPSAMPLE_ALPHA: f32 = 0.62;

/// Линейный апсемплер 16 -> 48 кГц с одним полюсом ФНЧ на ~7 кГц.
///
/// Куб лучше по ВЧ, но для речи 0..4 кГц разница неслышна, а фильтр убирает зеркала
/// интерполяции. Состояние между чанками сохраняется: чанк VQE — 32 мс, кадр Opus — 20 мс,
/// поэтому кадры собираются из остатка предыдущего чанка.
pub struct Upsampler {
    last: f32,
    lp: f32,
    /// Не отданные в кадр сэмплы (скользящее окно 48 кГц).
    pending: Vec<i16>,
}

impl Default for Upsampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Upsampler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            last: 0.0,
            lp: 0.0,
            pending: Vec::with_capacity(FRAME_SAMPLES * 2),
        }
    }

    /// Добавить чанк 16 кГц (обычно [`UPLINK_CHUNK`]).
    pub fn push(&mut self, chunk: &[i16]) {
        let factor = crate::CALL_RATE / UPLINK_RATE;
        for &sample in chunk {
            let from = self.last;
            let to = f32::from(sample);
            for step in 1..=factor {
                let frac = f64::from(step) / f64::from(factor);
                let v = f64::from(from) + (f64::from(to) - f64::from(from)) * frac;
                // Один полюс: y[n] = y[n-1] + a·(x - y[n-1]), a ≈ 2π·fc/fs.
                self.lp += UPSAMPLE_ALPHA * (v as f32 - self.lp);
                let clamped = self.lp.clamp(-32_768.0, 32_767.0);
                self.pending.push(clamped as i16);
            }
            self.last = to;
        }
    }

    /// Забрать готовый кадр [`FRAME_SAMPLES`], если он накопился.
    pub fn take_frame(&mut self) -> Option<Vec<i16>> {
        if self.pending.len() < FRAME_SAMPLES {
            return None;
        }
        let rest = self.pending.split_off(FRAME_SAMPLES);
        Some(std::mem::replace(&mut self.pending, rest))
    }

    /// Сколько сэмплов ждёт кадра (диагностика тестов).
    #[cfg(test)]
    #[must_use]
    pub fn buffered(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UPLINK_CHUNK;

    #[test]
    fn a_constant_signal_stays_constant_through_the_upsampler() {
        let mut up = Upsampler::new();
        // Прогреть фильтр: несколько секунд константы.
        for _ in 0..100 {
            up.push(&vec![1000i16; UPLINK_CHUNK]);
            while up.take_frame().is_some() {}
        }
        assert!(up.buffered() < FRAME_SAMPLES);
    }

    #[test]
    fn frames_come_out_at_a_20ms_cadence() {
        let mut up = Upsampler::new();
        up.push(&vec![0i16; UPLINK_CHUNK]);
        // 512 сэмплов 16 кГц → 1536 сэмплов 48 кГц → один кадр 960, остаток 576.
        assert_eq!(up.take_frame().map(|f| f.len()), Some(FRAME_SAMPLES));
        assert_eq!(up.buffered(), UPLINK_CHUNK * 3 - FRAME_SAMPLES);
        up.push(&vec![0i16; UPLINK_CHUNK]);
        assert!(up.take_frame().is_some());
    }

    #[test]
    fn a_silent_input_never_produces_a_full_scale_frame() {
        let mut up = Upsampler::new();
        up.push(&vec![0i16; UPLINK_CHUNK]);
        let frame = up.take_frame().unwrap();
        assert!(frame.iter().all(|&s| s == 0));
    }
}
