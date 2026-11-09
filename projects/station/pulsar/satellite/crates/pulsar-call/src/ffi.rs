//! Единственный модуль `unsafe` крейта: FFI к libopus из сборки прошивки (`@opus`).
//!
//! Кодек используется только для звонка: encoder — моно 48 кГц (вход апсемплирован с 16 кГц
//! VQE), decoder — стерео 48 кГц с даунмиксом в моно (телефон вправе прислать стерео).
//! Символы резолвятся линковкой, dlopen не нужен. Ошибки libopus логируются кодом, без аудио.

use std::ptr::NonNull;

/// 20 мс при 48 кГц.
pub const FRAME: usize = 960;
/// Максимум Opus — 120 мс.
const MAX_FRAME: usize = 5760;

const APPLICATION_VOIP: i32 = 2048;
const SET_BITRATE: i32 = 4002;
const SET_COMPLEXITY: i32 = 4010;
const SET_INBAND_FEC: i32 = 4012;
const SET_PACKET_LOSS: i32 = 4014;
const SET_SIGNAL_VOICE: i32 = 4024;
const SIGNAL_VOICE: i32 = 3001;
/// 24 кбит/с моно — потолок разборчивой речи в узкополосном канале.
const BITRATE: i32 = 24_000;

#[repr(C)]
struct RawEncoder {
    _private: [u8; 0],
}

#[repr(C)]
struct RawDecoder {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn opus_encoder_create(
        fs: i32,
        channels: i32,
        application: i32,
        error: *mut i32,
    ) -> *mut RawEncoder;
    fn opus_encoder_ctl(encoder: *mut RawEncoder, request: i32, ...) -> i32;
    fn opus_encode(
        encoder: *mut RawEncoder,
        pcm: *const i16,
        frame_size: i32,
        data: *mut u8,
        max_data_bytes: i32,
    ) -> i32;
    fn opus_encoder_destroy(encoder: *mut RawEncoder);

    fn opus_decoder_create(fs: i32, channels: i32, error: *mut i32) -> *mut RawDecoder;
    fn opus_decode(
        decoder: *mut RawDecoder,
        data: *const u8,
        len: i32,
        pcm: *mut i16,
        frame_size: i32,
        decode_fec: i32,
    ) -> i32;
    fn opus_decoder_destroy(decoder: *mut RawDecoder);

    fn opus_strerror(error: i32) -> *const std::ffi::c_char;
}

/// Моно-энкодер 48 кГц.
pub struct Encoder(NonNull<RawEncoder>);

// SAFETY: libopus encoder owned exclusively by this wrapper; the structure is not shared.
unsafe impl Send for Encoder {}

impl Encoder {
    /// `None` — libopus не смог создать энкодер (нет памяти / неподдерживаемая частота).
    pub fn new_mono() -> Option<Self> {
        let mut error = 0;
        // SAFETY: valid arguments; on failure the returned pointer is null and is not stored.
        let raw = unsafe { opus_encoder_create(48_000, 1, APPLICATION_VOIP, &mut error) };
        let raw = NonNull::new(raw)?;
        let this = Self(raw);
        if error != 0 {
            return None;
        }
        // SAFETY: `raw` is a live encoder for the lifetime of `this`.
        unsafe {
            opus_encoder_ctl(raw.as_ptr(), SET_BITRATE, BITRATE);
            opus_encoder_ctl(raw.as_ptr(), SET_COMPLEXITY, 3);
            opus_encoder_ctl(raw.as_ptr(), SET_INBAND_FEC, 1);
            opus_encoder_ctl(raw.as_ptr(), SET_PACKET_LOSS, 10);
            opus_encoder_ctl(raw.as_ptr(), SET_SIGNAL_VOICE, SIGNAL_VOICE);
        }
        Some(this)
    }

    /// Один кадр [`FRAME`] сэмплов; пустой `Vec` при ошибке кодека.
    pub fn encode(&mut self, pcm: &[i16]) -> Option<Vec<u8>> {
        if pcm.len() != FRAME {
            return None;
        }
        let mut out = vec![0u8; 400];
        // SAFETY: `pcm` has exactly FRAME samples and `out` is writable for its length.
        let n = unsafe {
            opus_encode(
                self.0.as_ptr(),
                pcm.as_ptr(),
                FRAME as i32,
                out.as_mut_ptr(),
                out.len() as i32,
            )
        };
        if n <= 0 {
            log_error(n);
            return None;
        }
        out.truncate(n as usize);
        Some(out)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: the pointer was created by opus_encoder_create and is destroyed once.
        unsafe { opus_encoder_destroy(self.0.as_ptr()) };
    }
}

/// Стерео-декодер 48 кГц; выход всегда моно (среднее каналов).
pub struct Decoder(NonNull<RawDecoder>);

// SAFETY: libopus decoder owned exclusively by this wrapper; not shared.
unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new_stereo() -> Option<Self> {
        let mut error = 0;
        // SAFETY: same contract as the encoder constructor.
        let raw = unsafe { opus_decoder_create(48_000, 2, &mut error) };
        let raw = NonNull::new(raw)?;
        if error != 0 {
            return None;
        }
        Some(Self(raw))
    }

    /// Раскодировать пакет в моно 48 кГц; `None` — битый пакет или ошибка кодека.
    pub fn decode(&mut self, packet: &[u8]) -> Option<Vec<i16>> {
        self.decode_inner(packet, false)
    }

    /// Восстановить пропущенный кадр из in-band FEC следующего пакета.
    pub fn decode_fec(&mut self, packet: &[u8]) -> Option<Vec<i16>> {
        self.decode_inner(packet, true)
    }

    fn decode_inner(&mut self, packet: &[u8], fec: bool) -> Option<Vec<i16>> {
        let mut stereo = vec![0i16; MAX_FRAME * 2];
        // SAFETY: the packet pointer/length pair is valid and the output buffer holds
        // MAX_FRAME stereo samples, which is the largest Opus frame.
        let n = unsafe {
            opus_decode(
                self.0.as_ptr(),
                packet.as_ptr(),
                packet.len() as i32,
                stereo.as_mut_ptr(),
                MAX_FRAME as i32,
                i32::from(fec),
            )
        };
        if n <= 0 {
            log_error(n);
            return None;
        }
        Some(mono(&stereo, n as usize))
    }

    /// PLC: продолжить последний кадр на [`FRAME`] сэмплов.
    pub fn conceal(&mut self) -> Vec<i16> {
        let mut stereo = vec![0i16; FRAME * 2];
        // SAFETY: null packet with length 0 is libopus' PLC entry point; the output buffer
        // holds FRAME stereo samples.
        let n = unsafe {
            opus_decode(
                self.0.as_ptr(),
                std::ptr::null(),
                0,
                stereo.as_mut_ptr(),
                FRAME as i32,
                0,
            )
        };
        if n <= 0 {
            return vec![0i16; FRAME];
        }
        mono(&stereo, n as usize)
    }
}

/// Стерео-выборка libopus → моно (среднее каналов).
fn mono(stereo: &[i16], frames: usize) -> Vec<i16> {
    stereo[..frames * 2]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|frame| ((i32::from(frame[0]) + i32::from(frame[1])) / 2) as i16)
        .collect()
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: created by opus_decoder_create and destroyed once.
        unsafe { opus_decoder_destroy(self.0.as_ptr()) };
    }
}

fn log_error(code: i32) {
    // SAFETY: opus_strerror returns a static NUL-terminated string for any code.
    let text = unsafe {
        let p = opus_strerror(code);
        if p.is_null() {
            "unknown".to_owned()
        } else {
            std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    };
    tracing::warn!("call: opus error {code}: {text}");
}

/// IPv4-адреса поднятых интерфейсов: по одному host-кандидату ICE на интерфейс.
///
/// Телефон приходит либо по LAN, либо по VPN, и какой адрес ему доступен, решает ICE. При
/// отсутствии адресов (нестандартное окружение) вызывающий добавляет loopback.
pub fn ipv4_addrs() -> Vec<std::net::Ipv4Addr> {
    let mut result = Vec::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills a linked list owned by libc; every non-null node is valid
    // until freeifaddrs, and `ifa_addr` is checked before the cast.
    unsafe {
        if libc::getifaddrs(&mut list) != 0 {
            return result;
        }
        let mut cursor = list;
        while !cursor.is_null() {
            let entry = &*cursor;
            if !entry.ifa_addr.is_null()
                && (*entry.ifa_addr).sa_family == libc::AF_INET as libc::sa_family_t
            {
                let sin = &*(entry.ifa_addr.cast::<libc::sockaddr_in>());
                let ip = std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                let up = entry.ifa_flags & (libc::IFF_UP as u32) != 0;
                if up && !ip.is_unspecified() && !result.contains(&ip) {
                    result.push(ip);
                }
            }
            cursor = entry.ifa_next;
        }
        libc::freeifaddrs(list);
    }
    result
}
