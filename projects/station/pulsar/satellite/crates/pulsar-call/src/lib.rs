//! Звонок на колонку: станция как endpoint STATION в сервисе `home.v1.IntercomService`.
//!
//! Крейт владеет только транспортом звонка и его состоянием. Он не знает про Home Assistant,
//! кольцо, музыку и захват микрофона: контроллер передаёт сюда уже очищенный VQE-поток
//! (16 кГц), получает события звонка и выводит звук собеседника через [`Downlink`].
//!
//! - Сигналинг — исходящее соединение к intercom (Connect JSON, `home-proto`):
//!   [`spawn`] открывает `Watch` (server-streaming) и переподключается с backoff.
//! - Медиа — WebRTC на `str0m` (sans-IO): один UDP-сокет, полноценный ICE-агент в роли
//!   controlled, DTLS-SRTP, Opus 48 кГц. SDP и ICE-кандидаты ходят через сигналинг и нигде
//!   не логируются.
//! - Аудио не пишется и не попадает в логи: в логах только переходы состояний.
//!
//! Без endpoint URL и токена (`/data/pulsar/intercom_token`) модуль выключен: [`spawn`]
//! возвращает handle, который сразу отвечает `Idle`, и ничего не открывает.

#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

mod codec;
mod config;
#[allow(unsafe_code)]
mod ffi;
mod jitter;
mod machine;
mod rtc;
mod service;
mod signaling;

pub use codec::Codec;
pub use config::{CA_FILE, CALL_DIR, CallConfig, ConfigError, TOKEN_FILE, URL_FILE};
pub use machine::{CallPhase, HangupReason};
pub use service::{CallHandle, Downlink, SinkStats, spawn};

/// Событие звонка для контроллера. Аудио через него не идёт.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallEvent {
    /// Входящий звонок от `caller`: контроллер включает сигнал и LED звонка.
    Incoming { caller: String },
    /// Медиа поднялось: спотеры/музыку — на паузу, uplink пошёл.
    Active,
    /// Звонок завершён (любой причиной): контроллер всё восстанавливает.
    Ended,
}

/// Частота очищенного потока фронтенда (VQE), Гц.
pub const UPLINK_RATE: u32 = 16_000;
/// Частота Opus и вывода звонка (WebRTC audio clock), Гц.
pub const CALL_RATE: u32 = 48_000;
/// Длина кадра Opus, мс.
pub const FRAME_MS: u32 = 20;
/// Сэмплов одного кадра на [`CALL_RATE`].
pub const FRAME_SAMPLES: usize = (CALL_RATE as usize * FRAME_MS as usize) / 1000;
/// Сэмплов одного чанка фронтенда на [`UPLINK_RATE`] (32 мс).
pub const UPLINK_CHUNK: usize = 512;

/// Сколько звонит входящий, прежде чем колонка ответит сама, мс.
pub const RING_MS: u64 = 1200;
/// Целевая глубина джиттер-буфера, мс: от [`JITTER_MIN_MS`] до [`JITTER_MAX_MS`].
pub const JITTER_MIN_MS: u32 = 40;
pub const JITTER_MAX_MS: u32 = 100;
/// Сколько тишины (PLC) буфер отдаёт, прежде чем звонок считается потерянным, мс.
pub const JITTER_GIVEUP_MS: u32 = 400;
