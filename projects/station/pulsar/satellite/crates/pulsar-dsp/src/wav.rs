//! Канонический WAV с PCM16: заголовок для дозаписи в поток и готовый файл одним куском.

/// Рифф-заголовок 44 байта для PCM16 с `channels` каналами и `rate` Гц; `data_bytes` — размер
/// уже записанных/планируемых данных.
pub fn header(channels: u16, rate: u32, data_bytes: u32) -> [u8; 44] {
    let mut h = [0u8; 44];
    let block = channels * 2;
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&data_bytes.saturating_add(36).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * u32::from(block)).to_le_bytes());
    h[32..34].copy_from_slice(&block.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

/// Файл PCM16 целиком: заголовок и выборки. Выборки чередуются по `channels` каналам.
pub fn pcm16(samples: &[i16], channels: u16, rate: u32) -> Vec<u8> {
    let data = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data);
    out.extend_from_slice(&header(channels, rate, data as u32));
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_canonical_pcm16_mono_16k() {
        let w = pcm16(&[0, 1, -1, i16::MAX], 1, 16000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes(w[20..22].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(w[22..24].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 16000);
        assert_eq!(u32::from_le_bytes(w[28..32].try_into().unwrap()), 32000);
        assert_eq!(u16::from_le_bytes(w[32..34].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(w[34..36].try_into().unwrap()), 16);
        assert_eq!(&w[36..40], b"data");
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 8);
        assert_eq!(&w[44..], &[0, 0, 1, 0, 0xff, 0xff, 0xff, 0x7f]);
    }

    #[test]
    fn multichannel_header() {
        let h = header(8, 48000, 4);
        assert_eq!(u32::from_le_bytes(h[4..8].try_into().unwrap()), 40);
        assert_eq!(u16::from_le_bytes(h[22..24].try_into().unwrap()), 8);
        assert_eq!(u32::from_le_bytes(h[28..32].try_into().unwrap()), 768_000);
        assert_eq!(u16::from_le_bytes(h[32..34].try_into().unwrap()), 16);
    }
}
